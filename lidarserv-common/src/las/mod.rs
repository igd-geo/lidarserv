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
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::fmt::Debug;
use std::io::SeekFrom::Start;
use std::io::{Cursor, Error, Read, Seek, Write};
use std::sync::Arc;
use thiserror::Error;
use tracy_client::span;

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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
    point_record_format: u8,
}

impl I32LasReadWrite {
    pub fn new(use_compression: bool, point_record_format: u8) -> Self {
        I32LasReadWrite {
            compression: use_compression,
            point_record_format,
        }
    }

    pub fn write_las<Point, It>(&self, las: Las<It>) -> Vec<u8>
    where
        It: Iterator + ExactSizeIterator,
        It::Item: Borrow<Point>,
        Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes>,
    {
        let _span = span!("I32LasReadWrite::write_las");
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
            self.compression,
            min,
            max,
            &coordinate_system,
            self.point_record_format,
        );

        // encode (uncompressed) point data into buffer
        let number_of_point_records = points.len() as u32;
        let mut point_data = Vec::with_capacity(points.len() * format.len() as usize);
        let number_of_points_by_return =
            write_point_data_i32(Cursor::new(&mut point_data), points, &format).unwrap(); // unwrap: cursor will not throw i/o errors
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
        Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes>,
    {
        let _span = span!("I32LasReadWrite::read_las");
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

        // header.number_of_point_records is legacy and may be 0
        // in this case, we need to use the large file header
        let mut number_of_point_records: usize = header.number_of_point_records as usize;
        if number_of_point_records == 0 && header.large_file.is_some() {
            number_of_point_records = header.large_file.unwrap().number_of_point_records as usize;
        };

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
            read_point_data_i32(data.as_slice(), &format, number_of_point_records)?
        } else {
            read.seek(Start(header.offset_to_point_data as u64))?;
            read_point_data_i32(read, &format, number_of_point_records)?
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
    point_record_format: u8,
) -> Result<(), std::io::Error>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes>,
    W: Write + Seek + Send,
{
    let _main_span = span!("async_write_compressed_las_with_variable_chunk_size");
    // header
    let _header_span = span!("async_write_compressed_las_with_variable_chunk_size::header");
    let (mut header, format) = init_las_header(
        true,
        Point3::new(-1.0, -1.0, -1.0),
        Point3::new(1.0, 1.0, 1.0),
        coordinate_system,
        point_record_format,
    );
    let header_pos = write.stream_position()?;
    match header.write_to(&mut write) {
        Ok(_) => {}
        Err(las::Error::Io(e)) => return Err(e),
        Err(_) => unreachable!(),
    }
    drop(_header_span);

    // las vlr
    let _vlr_span = span!("async_write_compressed_las_with_variable_chunk_size::vlr");
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
    drop(_vlr_span);

    // points
    let _points_span = span!("async_write_compressed_las_with_variable_chunk_size::points");
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
    drop(_points_span);

    // update header
    let _update_header_span =
        span!("async_write_compressed_las_with_variable_chunk_size::update_header");
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
    drop(_update_header_span);

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

/// Comparison between self-written LAS reader/writer and the las crate reader/writer.
/// Also shows how to use the different writers and readers.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::points::{PointType, WithAttr};
    use crate::geometry::position::{I32CoordinateSystem, I32Position, Position};
    use las::{Builder, Color, Point, Read, Reader, Write as LasWrite, Writer};
    use serde_json::{json, Value};
    use std::fs::File;
    use std::io::{BufReader, Write};
    use std::path::Path;

    /// Some implementations and definitions from the server to be able to use it here
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GenericPoint<Position> {
        position: Position,
        las_attributes: Box<LasPointAttributes>,
    }
    pub type LasPoint = GenericPoint<I32Position>;
    impl<Pos> PointType for GenericPoint<Pos>
    where
        Pos: Position + Default,
    {
        type Position = Pos;

        fn new(position: Self::Position) -> Self {
            GenericPoint {
                position,
                las_attributes: Default::default(),
            }
        }

        fn position(&self) -> &Self::Position {
            &self.position
        }

        fn position_mut(&mut self) -> &mut Self::Position {
            &mut self.position
        }
    }

    impl<Pos> WithAttr<LasPointAttributes> for GenericPoint<Pos> {
        fn value(&self) -> &LasPointAttributes {
            &self.las_attributes
        }

        fn value_mut(&mut self) -> &mut LasPointAttributes {
            &mut self.las_attributes
        }
    }

    #[test]
    fn test_write_read_las() {
        // write
        let pointcloud = custom_pointcloud(1);
        let loader = I32LasReadWrite::new(false, 3);
        let data = loader.write_las::<LasPoint, _>(Las {
            points: pointcloud.iter(),
            bounds: OptionAABB::default(),
            non_bogus_points: Some(pointcloud.len() as u32),
            coordinate_system: I32CoordinateSystem::new(
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(3.0, 3.0, 3.0),
            ),
        });

        // read
        let loader = I32LasReadWrite::new(false, 3);
        let reader = Cursor::new(data);
        println!("Point Record Format: {:?}", loader.point_record_format);
        println!("Compression: {:?}", loader.compression);
        let result: Las<Vec<LasPoint>> = loader.read_las(reader).unwrap();
        println!("Coordinate System: {:?}", result.coordinate_system);
        println!("Bounds: {:?}", result.bounds);
        println!("Non Bogus Points: {:?}", result.non_bogus_points);
        println!("Points: {:?}", result.points.len());
        println!("{:?}", result.points.first().unwrap());
    }

    #[test]
    /// Compare the self-written LAS reader/writer with the las crate reader/writer.
    /// Output a json file with the results.
    fn compare_writer_reader() {
        // tests of las crate
        let mut las_crate = json!({});
        for num_points in [1, 10, 100, 1000, 10000, 100000, 1000000].iter() {
            let result = write_read_las_crate(*num_points);
            las_crate[format!("{:?}", num_points)] = result;
        }

        // tests of self-written las reader/writer
        let mut custom_las = json!({});
        for num_points in [1, 10, 100, 1000, 10000, 100000, 1000000].iter() {
            let result = write_read_custom(*num_points);
            custom_las[format!("{:?}", num_points)] = result;
        }

        // write json
        let path = Path::new("test.json");
        let file = File::create(path).unwrap();
        let json = json!({
            "las_crate": las_crate,
            "custom_las": custom_las,
        });
        serde_json::to_writer_pretty(file, &json).unwrap();
    }

    /// Create a pointcloud with the las crate.
    fn las_crate_pointcloud(num_points: usize) -> Vec<Point> {
        let mut pointcloud = Vec::new();
        for i in 0..num_points {
            let mut point = Point {
                x: 1.,
                y: 2.,
                z: 3.,
                ..Default::default()
            };
            point.gps_time = Some(i as f64);
            point.color = Some(Color::new(0, 0, 0));
            pointcloud.push(point);
        }
        pointcloud
    }

    /// Create a pointcloud with custom point type.
    fn custom_pointcloud(num_points: usize) -> Vec<LasPoint> {
        let mut pointcloud = Vec::new();
        for i in 0..num_points {
            let position = I32Position::from_components(i as i32, i as i32, i as i32);
            let mod_attributes = LasPointAttributes {
                return_number: 3,
                number_of_returns: 6,
                edge_of_flight_line: true,
                scan_direction: true,
                ..Default::default()
            };
            let las_attributes = Box::new(mod_attributes);
            pointcloud.push(LasPoint {
                position,
                las_attributes,
            });
        }
        pointcloud
    }

    /// Write and read a pointcloud with the las crate.
    fn write_read_las_crate(num_points: usize) -> Value {
        // WRITING
        // Init
        println!("TESTING {:?} POINTS", num_points);
        let pointcloud = las_crate_pointcloud(num_points);
        let path = Path::new("test_write_read_las_crate.las");

        // Init Writing
        let init_time = std::time::Instant::now();
        let mut builder = Builder::from((1, 4));
        builder.point_format = Format::new(3).unwrap();
        let header = builder.into_header().unwrap();
        let mut writer = Writer::from_path(path, header).unwrap();
        let mut errors = 0;

        // Writing
        let start_time = std::time::Instant::now();
        for point in pointcloud.iter() {
            let result = writer.write(point.clone());
            if result.is_err() {
                errors += 1;
            }
        }
        let end_time = std::time::Instant::now();
        writer.close().unwrap();
        let close_time = std::time::Instant::now();

        let init_start_write = start_time.duration_since(init_time);
        let start_end_write = end_time.duration_since(start_time);
        let end_close_write = close_time.duration_since(end_time);
        let total_write = close_time.duration_since(init_time);
        let pps_write = pointcloud.len() as f64 / start_end_write.as_secs_f64();

        println!("WRITING");
        println!(
            "Number of points: {:?}, Number of errors {:?}",
            writer.header().number_of_points(),
            errors
        );
        println!(
            "Init time: {:?}, Write Time: {:?}, Close time: {:?}",
            init_start_write, start_end_write, end_close_write
        );
        println!("Total time: {:?}", total_write);
        println!("Points per second: {:?}", pps_write);

        // READING
        // Init
        let init_time = std::time::Instant::now();
        let read = BufReader::new(File::open(path).unwrap());
        let mut reader = Reader::new(read).unwrap();

        // Reading
        let mut pointcloud: Vec<Point> = Vec::new();
        let start_time = std::time::Instant::now();
        for wrapped_point in reader.points() {
            let point = wrapped_point.unwrap();
            pointcloud.push(point);
        }
        let end_time = std::time::Instant::now();

        let init_start_read = start_time.duration_since(init_time);
        let start_end_read = end_time.duration_since(start_time);
        let total_read = end_time.duration_since(init_time);
        let pps_read = pointcloud.len() as f64 / start_end_read.as_secs_f64();

        println!("READING");
        println!("Number of points: {:?}", pointcloud.len());
        println!(
            "Init time: {:?}, Write Time: {:?}",
            init_start_read, start_end_read
        );
        println!("Total time: {:?}", total_read);
        println!("Points per second: {:?}", pps_read);

        // Remove file if it exists
        if path.exists() {
            std::fs::remove_file(path).unwrap();
        }

        // Return results
        json!(
            {
                "total_write": total_write.as_secs_f64(),
                "total_read": total_read.as_secs_f64(),
            }
        )
    }

    fn write_read_custom(num_points: usize) -> Value {
        // Init
        let pointcloud = custom_pointcloud(num_points);
        let path = Path::new("test_write_read_custom.las");

        // Writing
        let init_time = std::time::Instant::now();
        let loader = I32LasReadWrite::new(false, 3);
        let data = loader.write_las::<LasPoint, _>(Las {
            points: pointcloud.iter(),
            bounds: OptionAABB::default(),
            non_bogus_points: Some(pointcloud.len() as u32),
            coordinate_system: I32CoordinateSystem::new(
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(3.0, 3.0, 3.0),
            ),
        });
        let mut file = File::create(path).unwrap();
        file.write_all(data.as_slice()).unwrap();
        file.sync_all().unwrap();
        let end_time = std::time::Instant::now();
        let total_write = end_time.duration_since(init_time);

        // Reading
        let init_time = std::time::Instant::now();
        let loader = I32LasReadWrite::new(false, 3);
        let mut reader = BufReader::new(File::open(path).unwrap());
        let result: Las<Vec<LasPoint>> = loader.read_las(&mut reader).unwrap();
        let end_time = std::time::Instant::now();
        let total_read = end_time.duration_since(init_time);

        // Remove file if it exists
        if path.exists() {
            std::fs::remove_file(path).unwrap();
        }

        // check if result points are the same as the input points
        let mut errors = 0;
        for (point1, point2) in pointcloud.iter().zip(result.points.iter()) {
            if point1.position().distance_to(point2.position()) > 0 {
                errors += 1;
            }
            if point1.attribute().return_number != point2.attribute().return_number {
                errors += 1;
            }
        }
        println!("ERRORS: {:?}", errors);

        // Return results
        json!(
            {
                "total_write": total_write.as_secs_f64(),
                "total_read": total_read.as_secs_f64(),
            }
        )
    }
}
