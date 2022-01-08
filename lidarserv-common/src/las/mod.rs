mod helpers;

use crate::geometry::bounding_box::OptionAABB;
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{I32CoordinateSystem, I32Position, Position};
use crate::las::helpers::{
    get_header_info_i32, init_las_header, read_las_string, read_point_data_i32,
    write_point_data_i32,
};
use crate::nalgebra::Scalar;
use crate::span;
use crate::utils::thread_pool::Threads;
use crossbeam_deque::{Steal, Worker};
use crossbeam_utils::CachePadded;
use las::point::Format;
use las::Vlr;
use laz::laszip::{ChunkTable, ChunkTableEntry};
use laz::record::{
    RecordCompressor, RecordDecompressor, SequentialPointRecordCompressor,
    SequentialPointRecordDecompressor,
};
use laz::{
    LasZipCompressor, LasZipDecompressor, LasZipError, LazItemRecordBuilder, LazItemType, LazVlr,
    LazVlrBuilder,
};
use nalgebra::Point3;
use std::borrow::Borrow;
use std::cmp;
use std::fmt::Debug;
use std::io::{Cursor, Error, Read, Seek, SeekFrom, Write};
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ReadLasError {
    #[error(transparent)]
    Io(Arc<std::io::Error>), // std::io:::Error is not Clone. Wrapping it in an Arc allows us to make ReadLasError Clone.
    #[error("Bad LAS file: {desc}")]
    FileFormat { desc: String },
}

#[derive(Error, Debug)]
pub enum WriteLasError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<std::io::Error> for ReadLasError {
    fn from(e: Error) -> Self {
        ReadLasError::Io(Arc::new(e))
    }
}

impl From<LasZipError> for ReadLasError {
    fn from(e: LasZipError) -> Self {
        match e {
            LasZipError::IoError(io) => ReadLasError::Io(Arc::new(io)),
            _ => ReadLasError::FileFormat {
                desc: format!("{}", e),
            },
        }
    }
}

impl From<las::Error> for ReadLasError {
    fn from(e: las::Error) -> Self {
        match e {
            las::Error::Io(io) => ReadLasError::Io(Arc::new(io)),
            _ => ReadLasError::FileFormat {
                desc: format!("{}", e),
            },
        }
    }
}

// todo not registered for now. Need to see, if I will keep using it like this.
//  https://www.asprs.org/Misc/LAS-Key-Request-Form.html
const BOGUS_POINTS_VLR_USER_ID: &str = "BOGUS_POINTS";
const BOGUS_POINTS_VLR_RECORD_ID: u16 = 1337;

pub struct Las<Points, Component: Scalar, CSys> {
    pub points: Points,
    pub bounds: OptionAABB<Component>,
    pub non_bogus_points: Option<u32>,
    pub coordinate_system: CSys,
}

pub trait LasReadWrite<Point, CSys>
where
    Point: PointType,
{
    fn write_las<W, It>(
        &self,
        las: Las<It, <Point::Position as Position>::Component, CSys>,
        wr: W,
    ) -> Result<(), WriteLasError>
    where
        W: Write + Seek + Send,
        It: Iterator + ExactSizeIterator,
        It::Item: Borrow<Point>;

    fn write_las_par(
        &self,
        las: Las<&[Point], <Point::Position as Position>::Component, CSys>,
        thread_pool: &mut Threads,
    ) -> Vec<u8>
    where
        Point: Sync;

    #[allow(clippy::type_complexity)]
    fn read_las<R: Read + Seek + Send>(
        &self,
        rd: R,
    ) -> Result<Las<Vec<Point>, <Point::Position as Position>::Component, CSys>, ReadLasError>;

    fn read_las_par(
        &self,
        data: &[u8],
        thread_pool: &mut Threads,
    ) -> Result<Las<Vec<Point>, <Point::Position as Position>::Component, CSys>, ReadLasError>
    where
        Point: Send;
}

pub trait LasExtraBytes {
    const NR_EXTRA_BYTES: usize;

    fn get_extra_bytes(&self) -> Vec<u8>;
    fn set_extra_bytes(&mut self, extra_bytes: &[u8]);
}

#[derive(Clone, Debug, Default)]
pub struct LasPointAttributes {
    pub intensity: u16,
    pub return_number: u8,
    pub number_of_returns: u8,
    pub scan_direction: bool,
    pub edge_of_flight_line: bool,
    pub classification: u8,
    pub scan_angle_rank: i8,
    pub user_data: u8,
    pub point_source_id: u16,
}

#[derive(Debug, Clone)]
pub struct I32LasReadWrite {
    compression: bool,
}

impl I32LasReadWrite {
    pub fn new(use_compression: bool) -> Self {
        I32LasReadWrite {
            compression: use_compression,
        }
    }
}

impl<Point> LasReadWrite<Point, I32CoordinateSystem> for I32LasReadWrite
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes,
{
    fn write_las<W, It>(
        &self,
        las: Las<It, i32, I32CoordinateSystem>,
        mut wr: W,
    ) -> Result<(), WriteLasError>
    where
        W: Write + Seek + Send,
        It: Iterator + ExactSizeIterator,
        It::Item: Borrow<Point>,
    {
        let Las {
            points,
            bounds,
            non_bogus_points,
            coordinate_system,
        } = las;

        // bounds
        let (min, max) = match bounds.into_aabb() {
            Some(aabb) => (
                aabb.min::<I32Position>().decode(&coordinate_system),
                aabb.max::<I32Position>().decode(&coordinate_system),
            ),
            None => (Point3::new(-1.0, -1.0, -1.0), Point3::new(1.0, 1.0, 1.0)),
        };

        // header
        let (mut header, format) = init_las_header(
            Point::NR_EXTRA_BYTES as u16,
            self.compression,
            min,
            max,
            &coordinate_system,
        );

        // encode (uncompressed) point data into buffer
        let number_of_point_records = points.len() as u32;
        let mut point_data = Vec::with_capacity(points.len() * format.len() as usize);
        let number_of_points_by_return =
            write_point_data_i32(Cursor::new(&mut point_data), points, &format)?;
        header.number_of_points_by_return = number_of_points_by_return;
        header.number_of_point_records = number_of_point_records;

        // bogus points vlr
        let bogus_points_vlr = if let Some(non_bogus_points) = non_bogus_points {
            let vlr = Vlr {
                user_id: BOGUS_POINTS_VLR_USER_ID.to_string(),
                record_id: BOGUS_POINTS_VLR_RECORD_ID,
                description: "Number of non bogus points.".to_string(),
                data: Vec::from(non_bogus_points.to_le_bytes()),
            };
            header.number_of_variable_length_records += 1;
            header.offset_to_point_data += vlr.len(false) as u32;
            Some(vlr)
        } else {
            None
        };

        // compression
        if self.compression {
            let chunk_size = (number_of_point_records / 16).clamp(50, 50_000);
            let laz_vlr = LazVlrBuilder::default()
                .with_point_format(format.to_u8().unwrap(), format.extra_bytes)
                .unwrap()
                .with_fixed_chunk_size(chunk_size)
                .build();
            let vlr = {
                let mut laz_vlr_data = Cursor::new(Vec::new());
                laz_vlr.write_to(&mut laz_vlr_data)?;
                Vlr {
                    user_id: LazVlr::USER_ID.to_string(),
                    record_id: LazVlr::RECORD_ID,
                    description: LazVlr::DESCRIPTION.to_string(),
                    data: laz_vlr_data.into_inner(),
                }
            };
            header.number_of_variable_length_records += 1;
            header.offset_to_point_data += vlr.len(false) as u32;

            // write header
            header.write_to(&mut wr).map_err(|e| match e {
                las::Error::Io(io_e) => io_e,
                _ => panic!("Unexpected error"),
            })?;

            // write bogus points vlr
            if let Some(vlr) = bogus_points_vlr {
                vlr.into_raw(false)
                    .unwrap()
                    .write_to(&mut wr)
                    .map_err(|e| match e {
                        las::Error::Io(io_e) => io_e,
                        _ => panic!("Unexpected error"),
                    })?;
            }

            // write laz vlr
            vlr.into_raw(false)
                .unwrap()
                .write_to(&mut wr)
                .map_err(|e| match e {
                    las::Error::Io(io_e) => io_e,
                    _ => panic!("Unexpected error"),
                })?;

            // compress and write point data
            let mut compressor = LasZipCompressor::new(wr, laz_vlr).map_err(|e| match e {
                laz::LasZipError::IoError(io_e) => io_e,
                _ => panic!("Unexpected error"),
            })?;
            compressor.compress_many(point_data.as_slice())?;
            compressor.done()?;
        } else {
            // write header
            header.write_to(&mut wr).map_err(|e| match e {
                las::Error::Io(io_e) => io_e,
                _ => panic!("Unexpected error"),
            })?;

            // write bogus points vlr
            if let Some(vlr) = bogus_points_vlr {
                vlr.into_raw(false)
                    .unwrap()
                    .write_to(&mut wr)
                    .map_err(|e| match e {
                        las::Error::Io(io_e) => io_e,
                        _ => panic!("Unexpected error"),
                    })?;
            }

            // write point data
            wr.write_all(point_data.as_slice())?;
        }
        Ok(())
    }

    fn write_las_par(
        &self,
        las: Las<&[Point], <Point::Position as Position>::Component, I32CoordinateSystem>,
        thread_pool: &mut Threads,
    ) -> Vec<u8>
    where
        Point: Sync,
    {
        let Las {
            points,
            bounds,
            non_bogus_points,
            coordinate_system,
        } = las;

        // bounds
        let (min, max) = match bounds.into_aabb() {
            Some(aabb) => (
                aabb.min::<I32Position>().decode(&coordinate_system),
                aabb.max::<I32Position>().decode(&coordinate_system),
            ),
            None => (Point3::new(-1.0, -1.0, -1.0), Point3::new(1.0, 1.0, 1.0)),
        };

        // header
        let (mut header, format) = init_las_header(
            Point::NR_EXTRA_BYTES as u16,
            self.compression,
            min,
            max,
            &coordinate_system,
        );

        // laz: items and vlr data and chunk table
        let nr_points = points.len();
        let chunk_size = std::cmp::min(nr_points / thread_pool.num_threads() / 8 + 1, 50_000);
        let laz_vlr = LazVlrBuilder::default()
            .with_point_format(format.to_u8().unwrap(), format.extra_bytes)
            .unwrap()
            .with_fixed_chunk_size(chunk_size as u32)
            .build();
        let mut chunks = Vec::new();
        {
            let mut remaining = points;
            while !remaining.is_empty() {
                let this_chunk_size = std::cmp::min(chunk_size, remaining.len());
                let (chunk_points, r) = remaining.split_at(this_chunk_size);
                chunks.push(chunk_points);
                remaining = r;
            }
        }

        // buffer for uncompressed point data, split into slices for the individual chunks
        let point_size = header.point_data_record_length as usize;
        let mut las_buffers = Vec::new();
        let mut las_slices = Vec::new();
        if self.compression {
            let cache_line_size = 128;
            for chunk in &chunks {
                let this_chunk_bytes = point_size * chunk.len();
                las_buffers.push(CachePadded::new(vec![
                    0_u8;
                    this_chunk_bytes + cache_line_size * 2  // we cannot control, that the memory allocated by the Vec is aligned to the cache lines, but we can at least insert some padding at the beginning and end.
                ]));
            }
            for chunk in &mut las_buffers {
                let range = cache_line_size..chunk.len() - cache_line_size;
                las_slices.push(Some(&mut chunk[range]));
            }
        } else {
            let las_size = nr_points * point_size;
            las_buffers.push(CachePadded::new(vec![0_u8; las_size]));
            let mut remaining_buffer = las_buffers[0].as_mut_slice();
            for chunk in &chunks {
                let chunk_bytes = point_size * chunk.len();
                let (slice, r) = remaining_buffer.split_at_mut(chunk_bytes);
                remaining_buffer = r;
                las_slices.push(Some(slice));
            }
        };
        header.number_of_point_records = nr_points as u32;

        // buffer for compressed point data
        let mut compressed_chunks = Vec::new();
        for _ in &las_slices {
            compressed_chunks.push(CachePadded::new(Vec::<u8>::new()));
        }
        let mut remaining = compressed_chunks.as_mut_slice();
        let mut laz_slices = Vec::new();
        while let Some((first, rest)) = remaining.split_first_mut() {
            laz_slices.push(Some(&mut **first));
            remaining = rest;
        }

        // task queues for the worker threads
        let mut worker_queues = Vec::new();
        let mut stealers = Vec::new();
        for _ in 0..thread_pool.num_threads() {
            let worker_queue = Worker::new_fifo();
            let stealer = worker_queue.stealer();
            worker_queues.push(Some(worker_queue));
            stealers.push(stealer);
        }
        for (i, chunk) in chunks.into_iter().enumerate() {
            let thread_id = i % worker_queues.len();
            worker_queues[thread_id].as_mut().unwrap().push((
                chunk,
                las_slices[i].take().unwrap(),
                laz_slices[i].take().unwrap(),
            ));
        }

        // worker thread args
        let mut args = Vec::new();
        for thread_id in 0..thread_pool.num_threads() {
            args.push((worker_queues[thread_id].take().unwrap(), stealers.clone()));
        }
        drop(worker_queues);
        drop(stealers);

        let results = thread_pool
            .execute_with_args(args, |thread_id, (worker_queue, mut stealers)| {
                let mut number_of_points_by_return = [0_u32; 5];

                let mut s = Vec::new();
                for i in 0..stealers.len() {
                    let index = (i + thread_id) % stealers.len();
                    s.push(stealers[index].clone());
                }
                stealers = s;

                loop {
                    // get one chunk to encode
                    let next = if let Some(chunk) = worker_queue.pop() {
                        Some(chunk)
                    } else {
                        'retry_loop: loop {
                            let mut retry = false;
                            for stealer in &stealers {
                                match stealer.steal_batch_and_pop(&worker_queue) {
                                    Steal::Empty => {}
                                    Steal::Retry => {
                                        retry = true; // potentially come back later
                                    }
                                    Steal::Success(chunk) => {
                                        break 'retry_loop Some(chunk);
                                    }
                                }
                            }
                            if !retry {
                                break 'retry_loop None;
                            }
                        }
                    };
                    let (chunk, las_data, laz_data) = match next {
                        None => return number_of_points_by_return,
                        Some(c) => c,
                    };

                    // encode las
                    // unwrap: this can only create I/O errors, when writing past the las_data slice.
                    // however, we initialized the slices to fit the las data size of each chunk.
                    let mut cursor = Cursor::new(las_data);
                    let chunk_number_of_points_by_return =
                        write_point_data_i32::<_, Point, _>(&mut cursor, chunk.iter(), &format)
                            .unwrap();
                    for i in 0..5 {
                        number_of_points_by_return[i] += chunk_number_of_points_by_return[i];
                    }
                    let las_data = cursor.into_inner();

                    // encode laz
                    // unwraps: fields are valid, because we created them with the provided LazItemRecordBuilder
                    // unwraps: cursor will never throw I/O errors, because it is backed by a Vec.
                    if self.compression {
                        let mut cursor = Cursor::new(laz_data);
                        let mut compressor = SequentialPointRecordCompressor::new(&mut cursor);
                        compressor.set_fields_from(laz_vlr.items()).unwrap();
                        compressor.compress_many(las_data).unwrap();
                        compressor.done().unwrap();
                    }
                }
            })
            .join();

        // assemble encoded chunks
        let mut cursor = Cursor::new(Vec::new());

        // reserve space for header
        header.write_to(&mut cursor).unwrap(); // unwrap: cursor of Vec never throws io errors

        // laz vlr
        if self.compression {
            let mut data = Vec::new();
            laz_vlr.write_to(&mut Cursor::new(&mut data)).unwrap();
            let vlr = Vlr {
                user_id: LazVlr::USER_ID.to_string(),
                record_id: LazVlr::RECORD_ID,
                description: LazVlr::DESCRIPTION.to_string(),
                data,
            };
            header.offset_to_point_data += vlr.len(false) as u32;
            header.number_of_variable_length_records += 1;
            vlr.into_raw(false).unwrap().write_to(&mut cursor).unwrap();
        }

        // bogus points vlr
        if let Some(non_bogus_points) = non_bogus_points {
            let vlr = Vlr {
                user_id: BOGUS_POINTS_VLR_USER_ID.to_string(),
                record_id: BOGUS_POINTS_VLR_RECORD_ID,
                description: "Number of non bogus points.".to_string(),
                data: Vec::from(non_bogus_points.to_le_bytes()),
            };
            header.number_of_variable_length_records += 1;
            header.offset_to_point_data += vlr.len(false) as u32;
            vlr.into_raw(false).unwrap().write_to(&mut cursor).unwrap();
        }

        // laz chunk table offset
        if self.compression {
            let chunk_table_offset = header.offset_to_point_data as i64
                + 8
                + compressed_chunks
                    .iter()
                    .map(|c| c.len() as i64)
                    .sum::<i64>();
            cursor.write_all(&chunk_table_offset.to_le_bytes()).unwrap();
        };

        // point data
        if self.compression {
            for chunk in &compressed_chunks {
                cursor.write_all(chunk).unwrap();
            }
        } else {
            for chunk in &las_buffers {
                cursor.write_all(&**chunk).unwrap();
            }
        }

        // chunk table
        if self.compression {
            let nr_chunks = compressed_chunks.len();
            let mut chunk_table = ChunkTable::with_capacity(nr_chunks);
            for i in 0..nr_chunks {
                chunk_table.push(ChunkTableEntry {
                    point_count: chunk_size as u64, // we can ignore the point count, because it is only relevant for variably sized chunks. We are using fixed size chunks, in which case the point count is not written into the chunk table.
                    byte_count: compressed_chunks[i].len() as u64,
                });
            }
            chunk_table.write_to(&mut cursor, &laz_vlr).unwrap();
        }

        // header
        for points_by_return in results {
            for i in 0..5 {
                header.number_of_points_by_return[i] += points_by_return[i];
            }
        }
        cursor.seek(SeekFrom::Start(0)).unwrap(); // unwrap: address 0 is always valid
        header.write_to(&mut cursor).unwrap(); // unwrap: cursor of Vec never throws io errors

        cursor.into_inner()
    }

    fn read_las<R: Read + Seek + Send>(
        &self,
        mut read: R,
    ) -> Result<Las<Vec<Point>, i32, I32CoordinateSystem>, ReadLasError> {
        // read header
        let header = las::raw::Header::read_from(&mut read)?;

        // check format
        let mut format = Format::new(header.point_data_record_format)?;
        if format.to_u8()? != 0 {
            // only point format 0 for now
            return Err(ReadLasError::FileFormat {
                desc: "Only point format 0 is supported.".to_string(),
            });
        }
        format.extra_bytes = header.point_data_record_length - format.len();
        if format.extra_bytes as usize != Point::NR_EXTRA_BYTES {
            // extra bytes need to match
            return Err(ReadLasError::FileFormat {
                desc: format!(
                    "Number of extra bytes does not match. (Expected {}, got {})",
                    Point::NR_EXTRA_BYTES,
                    format.extra_bytes
                ),
            });
        }

        // read vlrs
        let mut vlrs = Vec::new();
        for _ in 0..header.number_of_variable_length_records {
            let vlr = las::raw::Vlr::read_from(&mut read, false)?;
            vlrs.push(vlr);
        }

        // find and parse bogus points vlr
        let non_bogus_points = vlrs
            .iter()
            .find(|it| {
                read_las_string(&it.user_id)
                    .map(|uid| uid == BOGUS_POINTS_VLR_USER_ID)
                    .unwrap_or(false)
                    && it.record_id == BOGUS_POINTS_VLR_RECORD_ID
            })
            .map(|vlr| {
                let mut le_bytes = [0; 4];
                if vlr.data.len() == 4 {
                    le_bytes.copy_from_slice(&vlr.data[..4])
                };
                u32::from_le_bytes(le_bytes)
            });

        // read points - either compressed, or raw
        let points: Vec<Point> = if format.is_compressed {
            // find laszip vlr
            let vlr = if let Some(v) = vlrs.iter().find(|it| {
                read_las_string(&it.user_id)
                    .map(|uid| uid == LazVlr::USER_ID)
                    .unwrap_or(false)
                    && it.record_id == LazVlr::RECORD_ID
            }) {
                v
            } else {
                return Err(ReadLasError::FileFormat {
                    desc: "Missing LasZip VLR in compressed LAS (*.laz) file.".to_string(),
                });
            };

            // parse laszip vlr
            let laszip_vlr = LazVlr::read_from(vlr.data.as_slice())?;

            // decompress
            let mut decompressor = LasZipDecompressor::new(read, laszip_vlr)?;
            let mut data = vec![
                0;
                header.point_data_record_length as usize
                    * header.number_of_point_records as usize
            ];
            decompressor.decompress_many(data.as_mut_slice())?;

            // read decompressed point data
            read_point_data_i32(
                data.as_slice(),
                &format,
                header.number_of_point_records as usize,
            )?
        } else {
            read.seek(SeekFrom::Start(header.offset_to_point_data as u64))?;
            read_point_data_i32(read, &format, header.number_of_point_records as usize)?
        };

        let (coordinate_system, bounds) = get_header_info_i32(&header);

        Ok(Las {
            points,
            bounds,
            non_bogus_points,
            coordinate_system,
        })
    }

    fn read_las_par(
        &self,
        data: &[u8],
        thread_pool: &mut Threads,
    ) -> Result<
        Las<Vec<Point>, <Point::Position as Position>::Component, I32CoordinateSystem>,
        ReadLasError,
    >
    where
        Point: Send,
    {
        let mut read = Cursor::new(data);

        // read header
        let header = las::raw::Header::read_from(&mut read)?;

        // check format
        let mut format = Format::new(header.point_data_record_format)?;
        if format.to_u8()? != 0 {
            // only point format 0 for now
            return Err(ReadLasError::FileFormat {
                desc: "Only point format 0 is supported.".to_string(),
            });
        }
        format.extra_bytes = header.point_data_record_length - format.len();
        if format.extra_bytes as usize != Point::NR_EXTRA_BYTES {
            // extra bytes need to match
            return Err(ReadLasError::FileFormat {
                desc: format!(
                    "Number of extra bytes does not match. (Expected {}, got {})",
                    Point::NR_EXTRA_BYTES,
                    format.extra_bytes
                ),
            });
        }

        // read vlrs
        let mut vlrs = Vec::new();
        for _ in 0..header.number_of_variable_length_records {
            let vlr = las::raw::Vlr::read_from(&mut read, false)?;
            vlrs.push(vlr);
        }

        // find and parse bogus points vlr
        let non_bogus_points = vlrs
            .iter()
            .find(|it| {
                read_las_string(&it.user_id)
                    .map(|uid| uid == BOGUS_POINTS_VLR_USER_ID)
                    .unwrap_or(false)
                    && it.record_id == BOGUS_POINTS_VLR_RECORD_ID
            })
            .map(|vlr| {
                let mut le_bytes = [0; 4];
                if vlr.data.len() == 4 {
                    le_bytes.copy_from_slice(&vlr.data[..4])
                };
                u32::from_le_bytes(le_bytes)
            });

        // find and parse laszip vlr
        let laz_vlr = if format.is_compressed {
            let vlr = if let Some(v) = vlrs.iter().find(|it| {
                read_las_string(&it.user_id)
                    .map(|uid| uid == LazVlr::USER_ID)
                    .unwrap_or(false)
                    && it.record_id == LazVlr::RECORD_ID
            }) {
                v
            } else {
                return Err(ReadLasError::FileFormat {
                    desc: "Missing LasZip VLR in compressed LAS (*.laz) file.".to_string(),
                });
            };
            let parsed = LazVlr::read_from(vlr.data.as_slice())?;
            Some(parsed)
        } else {
            None
        };

        // parse chunk table
        let chunk_table = if format.is_compressed {
            // read chunk table
            read.seek(SeekFrom::Start(header.offset_to_point_data as u64))?;
            ChunkTable::read_from(&mut read, laz_vlr.as_ref().unwrap())?
        } else {
            // generate an artificial chunk table, that determines how to split the data into
            // individual tasks for parallel parsing
            let nr_tasks = thread_pool.num_threads() * 10;
            let points_per_task = header.number_of_point_records / nr_tasks as u32;
            let mut ct = ChunkTable::with_capacity(nr_tasks + 1);
            for _ in 0..nr_tasks {
                ct.push(ChunkTableEntry {
                    point_count: points_per_task as u64,
                    byte_count: points_per_task as u64 * header.point_data_record_length as u64,
                });
            }
            let remaining =
                header.number_of_point_records as u64 - nr_tasks as u64 * points_per_task as u64;
            if remaining > 0 {
                ct.push(ChunkTableEntry {
                    point_count: remaining,
                    byte_count: remaining * header.point_data_record_length as u64,
                });
            }
            ct
        };

        // split data into slices for each chunk
        let first_chunk_pos = if format.is_compressed {
            // account for 64 bit chunk table offset in compressed case
            header.offset_to_point_data as usize + 8
        } else {
            header.offset_to_point_data as usize
        };
        let mut remaining = &data[first_chunk_pos..];
        let mut chunk_slices = Vec::new();
        for chunk in &chunk_table {
            let len = chunk.byte_count as usize;
            if remaining.len() < len {
                return Err(ReadLasError::FileFormat {
                    desc: "Unexpected EOF".to_string(),
                });
            }
            let chunk_data = &remaining[..len];
            chunk_slices.push(chunk_data);
            remaining = &remaining[len..];
        }

        // point buffers for the results
        let mut parsed_chunks = Vec::new();
        for _ in 0..chunk_slices.len() {
            parsed_chunks.push(Vec::<Point>::new());
        }

        // worker threads
        let mut worker_queues = Vec::new();
        let mut stealers = Vec::new();
        for _ in 0..thread_pool.num_threads() {
            let worker = Worker::new_fifo();
            let stealer = worker.stealer();
            worker_queues.push(worker);
            stealers.push(stealer);
        }
        for (chunk_id, parsed) in parsed_chunks.iter_mut().enumerate() {
            let thread_id = chunk_id % thread_pool.num_threads();
            worker_queues[thread_id].push((chunk_table[chunk_id], chunk_slices[chunk_id], parsed));
        }

        // parse each chunk in parallel
        let results = thread_pool
            .execute_with_args(
                worker_queues,
                |thread_id, worker_queue| -> Result<(), ReadLasError> {
                    loop {
                        // get a chunk to process
                        let next = if let Some(chunk) = worker_queue.pop() {
                            Some(chunk)
                        } else {
                            'retry_loop: loop {
                                let mut retry = false;
                                for stealer in &stealers {
                                    match stealer.steal_batch_and_pop(&worker_queue) {
                                        Steal::Empty => {}
                                        Steal::Retry => {
                                            retry = true; // potentially come back later
                                        }
                                        Steal::Success(chunk) => {
                                            break 'retry_loop Some(chunk);
                                        }
                                    }
                                }
                                if !retry {
                                    break 'retry_loop None;
                                }
                            }
                        };
                        let (chunk, chunk_data, chunk_points) = if let Some(n) = next {
                            n
                        } else {
                            return Ok(());
                        };

                        // decompress
                        let mut uncompressed_point_data_buf = Vec::new();
                        let uncompressed_point_data = if format.is_compressed {
                            let read = Cursor::new(chunk_data);
                            let laz_vlr = laz_vlr.as_ref().unwrap();
                            let uncompressed_size = chunk.point_count as usize
                                * header.point_data_record_length as usize;
                            uncompressed_point_data_buf.resize(uncompressed_size, 0);
                            let mut decompressor = SequentialPointRecordDecompressor::new(read);
                            decompressor.set_fields_from(laz_vlr.items())?;
                            let decompressed_bytes = decompressor.decompress_until_end_of_file(
                                uncompressed_point_data_buf.as_mut_slice(),
                            )?;
                            &uncompressed_point_data_buf[..decompressed_bytes]
                        } else {
                            chunk_data
                        };

                        // parse las
                        let cursor = Cursor::new(uncompressed_point_data);
                        let number_of_point_records = uncompressed_point_data.len()
                            / header.point_data_record_length as usize;
                        let mut points =
                            read_point_data_i32(cursor, &format, number_of_point_records)?;
                        chunk_points.append(&mut points);
                    }
                },
            )
            .join();
        drop(stealers);

        // check that all threads terminated without errors
        for result in results {
            result?;
        }

        let (coordinate_system, bounds) = get_header_info_i32(&header);
        let result = Las {
            points: parsed_chunks.into_iter().flatten().collect(), // note instead of doing this flattening (which needs to copy every point), we could prepare the result Vec beforehand and write each point to the correct position directly after parsing it
            bounds,
            non_bogus_points,
            coordinate_system,
        };
        Ok(result)
    }
}
