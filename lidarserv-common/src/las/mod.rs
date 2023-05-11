mod helpers;

use crate::geometry::bounding_box::{BaseAABB, OptionAABB};
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{I32CoordinateSystem, I32Position, Position};
use crate::las::helpers::{
    get_header_info_i32, init_las_header, read_las_string, read_point_data_i32,
    write_point_data_i32,
};
use las::point::Format;
use las::Vlr;
use laz::laszip::ChunkTable;
use laz::{LasZipCompressor, LasZipDecompressor, LasZipError, LazVlr, LazVlrBuilder};
use nalgebra::Point3;
use std::borrow::Borrow;
use std::fmt::Debug;
use std::io::SeekFrom::Start;
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

pub struct Las<Points> {
    pub points: Points,
    pub bounds: OptionAABB<i32>,
    pub non_bogus_points: Option<u32>,
    pub coordinate_system: I32CoordinateSystem,
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
    pub gps_time: f64,
    pub color: (u16, u16, u16),
}

#[derive(Debug, Clone)]
pub struct I32LasReadWrite {
    compression: bool,
    color: bool,
    gps_time: bool,
    las_extra_bytes: bool,
}

impl I32LasReadWrite {
    pub fn new(use_compression: bool, use_color: bool, use_time: bool, use_extra_bytes: bool) -> Self {
        I32LasReadWrite {
            compression: use_compression,
            color: use_color,

            gps_time: use_time,
            las_extra_bytes: use_extra_bytes,
        }
    }

    pub fn write_las<Point, It>(&self, las: Las<It>) -> Vec<u8>
    where
        It: Iterator + ExactSizeIterator,
        It::Item: Borrow<Point>,
        Point: PointType<Position = I32Position> + LasExtraBytes + WithAttr<LasPointAttributes>,
    {
        let mut wr = Cursor::new(Vec::new());
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
            self.color,
            self.gps_time,
        );

        // encode (uncompressed) point data into buffer
        let number_of_point_records = points.len() as u32;
        let mut point_data = Vec::with_capacity(points.len() * format.len() as usize);
        let number_of_points_by_return =
            write_point_data_i32(Cursor::new(&mut point_data), points, &format, self.color, self.gps_time)
                .unwrap(); // unwrap: cursor will not throw i/o errors
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
            let chunk_size = 50_000;
            let laz_vlr = LazVlrBuilder::default()
                .with_point_format(format.to_u8().unwrap(), format.extra_bytes)
                .unwrap()
                .with_fixed_chunk_size(chunk_size)
                .build();
            let vlr = {
                let mut laz_vlr_data = Cursor::new(Vec::new());
                laz_vlr.write_to(&mut laz_vlr_data).unwrap(); // unwrap: cursor will not throw i/o errors
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
            header.write_to(&mut wr).unwrap(); // unwrap: cursor does not produce i/o errors

            // write bogus points vlr
            if let Some(vlr) = bogus_points_vlr {
                vlr.into_raw(false).unwrap().write_to(&mut wr).unwrap(); // unwrap: Cursor does not produce i/o errors
            }

            // write laz vlr
            vlr.into_raw(false).unwrap().write_to(&mut wr).unwrap(); // unwrap: Cursor does not produce i/o errors

            // compress and write point data
            let mut compressor = LasZipCompressor::new(&mut wr, laz_vlr).unwrap(); // unwrap: this only returns an error, if the LazVlr is not valid. Since we created it with the LazVlrBuilder, it should be all right.
            compressor.compress_many(point_data.as_slice()).unwrap(); // unwrap: Cursor does not produce i/o errors
            compressor.done().unwrap(); // unwrap: Cursor does not produce i/o errors
        } else {
            // write header
            header.write_to(&mut wr).unwrap(); // unwrap: Cursor does not produce i/o errors

            // write bogus points vlr
            if let Some(vlr) = bogus_points_vlr {
                vlr.into_raw(false).unwrap().write_to(&mut wr).unwrap(); // unwrap: Cursor does not produce i/o errors
            }

            // write point data
            wr.write_all(point_data.as_slice()).unwrap(); // unwrap: Cursor does not produce i/o errors
        }
        wr.into_inner()
    }

    pub fn read_las<R, Point>(&self, mut read: R) -> Result<Las<Vec<Point>>, ReadLasError>
    where
        R: Read + Seek + Send,
        Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes,
    {
        // read header
        let header = las::raw::Header::read_from(&mut read)?;

        // check format
        let mut format = Format::new(header.point_data_record_format)?;
        let point_data_record_format = format.to_u8()?;
        if point_data_record_format > 3 {
            return Err(ReadLasError::FileFormat {
                desc: "Only point formats 0-3 are supported.".to_string(),
            });
        }
        format.extra_bytes = header.point_data_record_length - format.len();
        if (format.extra_bytes as usize != Point::NR_EXTRA_BYTES) && self.las_extra_bytes {
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
                true,
            )?
        } else {
            read.seek(SeekFrom::Start(header.offset_to_point_data as u64))?;
            read_point_data_i32(read, &format, header.number_of_point_records as usize, self.las_extra_bytes)?
        };

        let (coordinate_system, bounds) = get_header_info_i32(&header);

        Ok(Las {
            points,
            bounds,
            non_bogus_points,
            coordinate_system,
        })
    }
}

pub fn async_write_compressed_las_with_variable_chunk_size<Point, W>(
    chunks: crossbeam_channel::Receiver<Vec<Point>>,
    coordinate_system: &I32CoordinateSystem,
    mut write: W,
    use_color: bool,
    use_time: bool,
) -> Result<(), std::io::Error>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes,
    W: Write + Seek + Send,
{
    // header
    let (mut header, format) = init_las_header(
        Point::NR_EXTRA_BYTES as u16,
        true,
        Point3::new(-1.0, -1.0, -1.0),
        Point3::new(1.0, 1.0, 1.0),
        coordinate_system,
        use_color,
        use_time,
    );
    let header_pos = write.seek(SeekFrom::Current(0))?;
    match header.write_to(&mut write) {
        Ok(_) => {}
        Err(las::Error::Io(e)) => return Err(e),
        Err(_) => unreachable!(),
    }

    // las vlr
    let laz_vlr = LazVlrBuilder::default()
        .with_point_format(format.to_u8().unwrap(), format.extra_bytes)
        .unwrap()
        .with_variable_chunk_size()
        .build();
    let vlr = {
        let mut laz_vlr_data = Cursor::new(Vec::new());
        laz_vlr.write_to(&mut laz_vlr_data).unwrap(); // unwrap: cursor will not throw i/o errors
        Vlr {
            user_id: LazVlr::USER_ID.to_string(),
            record_id: LazVlr::RECORD_ID,
            description: LazVlr::DESCRIPTION.to_string(),
            data: laz_vlr_data.into_inner(),
        }
    };
    header.number_of_variable_length_records += 1;
    header.offset_to_point_data += vlr.len(false) as u32;
    vlr.into_raw(false).unwrap().write_to(&mut write).unwrap();

    // points
    let mut aabb = OptionAABB::empty();
    {
        let mut compressor = LasZipCompressor::new(&mut write, laz_vlr).unwrap();
        compressor.compress_chunks(chunks.iter().map(|chunk| {
            // update aabb
            for point in &chunk {
                // println!("Point ({},{},{}), Int: {}, Time: {} ", point.position().x(), point.position().y(), point.position().z(), point.attribute().intensity, point.attribute().gps_time);
                aabb.extend(point.position())
            }

            // update number of points
            header.number_of_point_records += chunk.len() as u32;

            // encode point data
            let mut point_data = Vec::with_capacity(chunk.len() * format.len() as usize);
            let number_of_points_by_return = write_point_data_i32::<_, Point, _>(
                Cursor::new(&mut point_data),
                chunk.iter(),
                &format,
                use_color,
                use_time,
            )
            .unwrap(); // unwrap: cursor will not throw i/o errors

            // update number of points by return
            for (i, nr_points) in number_of_points_by_return.into_iter().enumerate() {
                header.number_of_points_by_return[i] += nr_points;
            }

            point_data
        }))?;
        compressor.done()?;
    }

    // update header
    if let Some(aabb) = aabb.into_aabb() {
        let min = aabb.min::<I32Position>().decode(coordinate_system);
        let max = aabb.max::<I32Position>().decode(coordinate_system);
        header.min_x = min.x;
        header.min_y = min.y;
        header.min_z = min.z;
        header.max_x = max.x;
        header.max_y = max.y;
        header.max_z = max.z;
    }
    write.seek(Start(header_pos))?;
    match header.write_to(&mut write) {
        Ok(_) => {}
        Err(las::Error::Io(e)) => return Err(e),
        Err(_) => unreachable!(),
    }

    Ok(())
}

/// Splits a laz file into multiple ones, one file per chunk. This is done efficiently without
/// de-compressing and re-compressing the point data.
pub fn async_split_compressed_las<R>(
    sender: crossbeam_channel::Sender<Vec<u8>>,
    mut read: R,
) -> Result<(), ReadLasError>
where
    R: Read + Seek,
{
    // read header
    let mut header = las::raw::Header::read_from(&mut read)?;
    let format = Format::new(header.point_data_record_format)?;
    if !format.is_compressed {
        return Err(ReadLasError::FileFormat {
            desc: "Expected compressed las".to_string(),
        });
    }

    // read vlrs
    let mut vlrs = Vec::new();
    for _ in 0..header.number_of_variable_length_records {
        let vlr = las::raw::Vlr::read_from(&mut read, false)?;
        vlrs.push(vlr);
    }

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
            desc: "Missing LasZip VLR.".to_string(),
        });
    };
    let laszip_vlr = LazVlr::read_from(vlr.data.as_slice())?;

    // read chunk table
    read.seek(Start(header.offset_to_point_data as u64))?;
    let chunk_table = ChunkTable::read_from(&mut read, &laszip_vlr)?;

    // split chunks into individual laz files
    let mut laz_vlr_data = Vec::new();
    vlr.write_to(Cursor::new(&mut laz_vlr_data))?;
    header.number_of_variable_length_records = 1;
    header.offset_to_point_data = header.header_size as u32 + laz_vlr_data.len() as u32;
    for chunk_table_entry in &chunk_table {
        // read chunk
        let mut chunk_data = vec![0_u8; chunk_table_entry.byte_count as usize];
        read.read_exact(&mut chunk_data)?;

        // chunk table for the single-chunk split file
        let mut new_chunk_table = ChunkTable::with_capacity(1);
        new_chunk_table.push(*chunk_table_entry);

        // construct laz file with only this chunk
        let nr_points = chunk_table_entry.point_count as u32;
        header.number_of_points_by_return = [nr_points, 0, 0, 0, 0];
        header.number_of_point_records = nr_points;
        let mut write = Cursor::new(Vec::new());
        header.write_to(&mut write)?; // write header
        write.write_all(&laz_vlr_data)?; // write laz vlr
        write.write_all(
            &(header.offset_to_point_data as u64 + 8 + chunk_table_entry.byte_count).to_le_bytes(),
        )?; // write chunk table offset
        write.write_all(&chunk_data)?; // write point data
        new_chunk_table.write_to(&mut write, &laszip_vlr)?; // write chunk table

        // send
        match sender.send(write.into_inner()) {
            Ok(_) => (),
            Err(_) => return Ok(()), // if the corresponding receiver is closed, we can just stop.
        }
    }

    Ok(())
}
