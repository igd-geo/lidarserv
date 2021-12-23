use crate::geometry::bounding_box::{BaseAABB, OptionAABB};
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{CoordinateSystem, I32CoordinateSystem, I32Position, Position};
use crate::nalgebra::Scalar;
use crossbeam_deque::Steal;
use las::point::Format;
use las::raw::point::{Flags, ScanAngle};
use las::{Version, Vlr};
use laz::{
    LasZipCompressor, LasZipDecompressor, LasZipError, LazItemRecordBuilder, LazItemType, LazVlr,
};
use nalgebra::{Point3, Vector3};
use std::borrow::Borrow;
use std::fmt::Debug;
use std::io;
use std::io::{Cursor, Error, Read, Seek, SeekFrom, Write};
use std::string::FromUtf8Error;
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

pub struct WorkStealingLas<Point, Component: Scalar, CSys> {
    pub points_queue: crossbeam_deque::Worker<Vec<Point>>,
    pub points_stealers: Vec<crossbeam_deque::Stealer<Vec<Point>>>,
    pub bogus_queue: crossbeam_deque::Worker<Vec<Point>>,
    pub bogus_stealers: Vec<crossbeam_deque::Stealer<Vec<Point>>>,
    pub bounds: OptionAABB<Component>,
    pub coordinate_system: CSys,
    pub bogus_points_vlr: bool,
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

    fn write_las_work_stealing<W>(
        &self,
        las: WorkStealingLas<Point, <Point::Position as Position>::Component, CSys>,
        wr: W,
    ) -> Result<(), WriteLasError>
    where
        W: Write + Seek + Send;

    #[allow(clippy::type_complexity)]
    fn read_las<R: Read + Seek + Send>(
        &self,
        rd: R,
    ) -> Result<Las<Vec<Point>, <Point::Position as Position>::Component, CSys>, ReadLasError>;
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

    fn write_las_work_stealing_compressed<W, Point>(
        &self,
        las: WorkStealingLas<Point, <Point::Position as Position>::Component, I32CoordinateSystem>,
        mut wr: W,
    ) -> Result<(), WriteLasError>
    where
        W: Write + Seek + Send,
        Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes,
    {
        let WorkStealingLas {
            points_queue,
            points_stealers,
            bogus_queue,
            bogus_stealers,
            bounds,
            coordinate_system,
            bogus_points_vlr: use_bogus_points_vlr,
        } = las;

        // las 1.2, Point format 0
        let version = Version::new(1, 2);
        let mut format = Format::new(0).unwrap();
        format.extra_bytes = Point::NR_EXTRA_BYTES as u16;
        format.is_compressed = self.compression;

        // string "LIDARSERV" for system identifier and generating software
        let mut lidarserv = [0; 32];
        let lidarserv_data = "LIDARSERV".bytes().collect::<Vec<_>>();
        lidarserv[..lidarserv_data.len()].copy_from_slice(lidarserv_data.as_slice());

        // bounds
        let (min, max) = match bounds.into_aabb() {
            Some(aabb) => (
                aabb.min::<I32Position>().decode(&coordinate_system),
                aabb.max::<I32Position>().decode(&coordinate_system),
            ),
            None => (Point3::new(-1.0, -1.0, -1.0), Point3::new(1.0, 1.0, 1.0)),
        };

        let mut header = las::raw::Header {
            version,
            system_identifier: lidarserv,
            generating_software: lidarserv,
            header_size: version.header_size(),
            offset_to_point_data: version.header_size() as u32,
            number_of_variable_length_records: 0,
            point_data_record_format: format.to_u8().unwrap(),
            point_data_record_length: format.len(),
            number_of_point_records: 0,
            number_of_points_by_return: [0; 5],
            x_scale_factor: coordinate_system.scale().x,
            y_scale_factor: coordinate_system.scale().y,
            z_scale_factor: coordinate_system.scale().z,
            x_offset: coordinate_system.offset().x,
            y_offset: coordinate_system.offset().y,
            z_offset: coordinate_system.offset().z,
            min_x: min.x,
            min_y: min.y,
            min_z: min.z,
            max_x: max.x,
            max_y: max.y,
            max_z: max.z,
            ..Default::default()
        };

        // write header
        let header_position = wr.seek(SeekFrom::Current(0))?;
        header.write_to(&mut wr).map_err(|e| match e {
            las::Error::Io(io_e) => io_e,
            _ => panic!("Unexpected error"),
        })?;

        // write laz vlr
        let laz_vlr = {
            let laz_vlr = {
                let mut laz_items = LazItemRecordBuilder::new();
                laz_items.add_item(LazItemType::Point10);
                if format.extra_bytes > 0 {
                    laz_items.add_item(laz::LazItemType::Byte(format.extra_bytes));
                }
                LazVlr::from_laz_items(laz_items.build())
            };
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
            header.point_data_record_format |= 0x80;
            vlr.into_raw(false)
                .unwrap()
                .write_to(&mut wr)
                .map_err(|e| match e {
                    las::Error::Io(io_e) => io_e,
                    _ => panic!("Unexpected error"),
                })?;
            laz_vlr
        };

        // write bogus points vlr
        let bogus_points_vlr_pos = wr.seek(SeekFrom::Current(0))?;
        if use_bogus_points_vlr {
            let vlr = Vlr {
                user_id: BOGUS_POINTS_VLR_USER_ID.to_string(),
                record_id: BOGUS_POINTS_VLR_RECORD_ID,
                description: "Number of non bogus points.".to_string(),
                data: Vec::from(0_u32.to_le_bytes()),
            };
            header.number_of_variable_length_records += 1;
            header.offset_to_point_data += vlr.len(false) as u32;
            vlr.clone()
                .into_raw(false)
                .unwrap()
                .write_to(&mut wr)
                .map_err(|e| match e {
                    las::Error::Io(io_e) => io_e,
                    _ => panic!("Unexpected error"),
                })?;
        };

        let mut compressor = LasZipCompressor::new(wr, laz_vlr).map_err(|e| match e {
            laz::LasZipError::IoError(io_e) => io_e,
            _ => panic!("Unexpected error"),
        })?;

        // write "normal" points
        let mut nr_non_bogus = 0;
        'tasks_loop: loop {
            // get batch of points to write
            let points = match points_queue.pop() {
                Some(v) => v,
                None => 'steal_loop: loop {
                    let mut retry = false;
                    for stealer in &points_stealers {
                        match stealer.steal_batch_and_pop(&points_queue) {
                            Steal::Success(s) => break 'steal_loop s,
                            Steal::Retry => retry = true,
                            Steal::Empty => (),
                        }
                    }
                    if !retry {
                        break 'tasks_loop;
                    }
                },
            };

            // write
            let mut buf_uncompressed = Vec::new();
            nr_non_bogus += points.len();
            header.number_of_point_records += points.len() as u32;
            let points_by_return = write_point_data_i32(
                Cursor::new(&mut buf_uncompressed),
                points.into_iter(),
                &format,
            )?;
            compressor.compress_many(&buf_uncompressed)?;
            for i in 0..5 {
                header.number_of_points_by_return[i] += points_by_return[i];
            }
        }

        // write bogus points
        'bogus_tasks_loop: loop {
            // get batch of points to write
            let points = match bogus_queue.pop() {
                Some(v) => v,
                None => 'bogus_steal_loop: loop {
                    let mut retry = false;
                    for stealer in &bogus_stealers {
                        match stealer.steal_batch_and_pop(&bogus_queue) {
                            Steal::Success(s) => break 'bogus_steal_loop s,
                            Steal::Retry => retry = true,
                            Steal::Empty => (),
                        }
                    }
                    if !retry {
                        break 'bogus_tasks_loop;
                    }
                },
            };

            // write
            header.number_of_point_records += points.len() as u32;
            let mut buf_uncompressed = Vec::new();
            let points_by_return = write_point_data_i32(
                Cursor::new(&mut buf_uncompressed),
                points.into_iter(),
                &format,
            )?;
            compressor.compress_many(&buf_uncompressed)?;
            for i in 0..5 {
                header.number_of_points_by_return[i] += points_by_return[i];
            }
        }

        // finalize
        compressor.done()?;
        let mut wr = compressor.into_inner();

        // write updated header
        wr.seek(SeekFrom::Start(header_position))?;
        header.write_to(&mut wr).map_err(|e| match e {
            las::Error::Io(io_e) => io_e,
            _ => panic!("Unexpected error"),
        })?;

        // write updated bogus points vlr
        if use_bogus_points_vlr {
            wr.seek(SeekFrom::Start(bogus_points_vlr_pos))?;
            let vlr = Vlr {
                user_id: BOGUS_POINTS_VLR_USER_ID.to_string(),
                record_id: BOGUS_POINTS_VLR_RECORD_ID,
                description: "Number of non bogus points.".to_string(),
                data: Vec::from((nr_non_bogus as u32).to_le_bytes()),
            };
            vlr.into_raw(false)
                .unwrap()
                .write_to(&mut wr)
                .map_err(|e| match e {
                    las::Error::Io(io_e) => io_e,
                    _ => panic!("Unexpected error"),
                })?;
        }
        Ok(())
    }

    fn write_las_work_stealing_uncompressed<W, Point>(
        &self,
        las: WorkStealingLas<Point, <Point::Position as Position>::Component, I32CoordinateSystem>,
        mut wr: W,
    ) -> Result<(), WriteLasError>
    where
        W: Write + Seek + Send,
        Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes,
    {
        let WorkStealingLas {
            points_queue,
            points_stealers,
            bogus_queue,
            bogus_stealers,
            bounds,
            coordinate_system,
            bogus_points_vlr: use_bogus_points_vlr,
        } = las;

        // las 1.2, Point format 0
        let version = Version::new(1, 2);
        let mut format = Format::new(0).unwrap();
        format.extra_bytes = Point::NR_EXTRA_BYTES as u16;
        format.is_compressed = self.compression;

        // string "LIDARSERV" for system identifier and generating software
        let mut lidarserv = [0; 32];
        let lidarserv_data = "LIDARSERV".bytes().collect::<Vec<_>>();
        lidarserv[..lidarserv_data.len()].copy_from_slice(lidarserv_data.as_slice());

        // bounds
        let (min, max) = match bounds.into_aabb() {
            Some(aabb) => (
                aabb.min::<I32Position>().decode(&coordinate_system),
                aabb.max::<I32Position>().decode(&coordinate_system),
            ),
            None => (Point3::new(-1.0, -1.0, -1.0), Point3::new(1.0, 1.0, 1.0)),
        };

        let mut header = las::raw::Header {
            version,
            system_identifier: lidarserv,
            generating_software: lidarserv,
            header_size: version.header_size(),
            offset_to_point_data: version.header_size() as u32,
            number_of_variable_length_records: 0,
            point_data_record_format: format.to_u8().unwrap(),
            point_data_record_length: format.len(),
            number_of_point_records: 0,
            number_of_points_by_return: [0; 5],
            x_scale_factor: coordinate_system.scale().x,
            y_scale_factor: coordinate_system.scale().y,
            z_scale_factor: coordinate_system.scale().z,
            x_offset: coordinate_system.offset().x,
            y_offset: coordinate_system.offset().y,
            z_offset: coordinate_system.offset().z,
            min_x: min.x,
            min_y: min.y,
            min_z: min.z,
            max_x: max.x,
            max_y: max.y,
            max_z: max.z,
            ..Default::default()
        };

        // write header
        let header_position = wr.seek(SeekFrom::Current(0))?;
        header.write_to(&mut wr).map_err(|e| match e {
            las::Error::Io(io_e) => io_e,
            _ => panic!("Unexpected error"),
        })?;

        // write bogus points vlr
        let bogus_points_vlr_pos = wr.seek(SeekFrom::Current(0))?;
        if use_bogus_points_vlr {
            let vlr = Vlr {
                user_id: BOGUS_POINTS_VLR_USER_ID.to_string(),
                record_id: BOGUS_POINTS_VLR_RECORD_ID,
                description: "Number of non bogus points.".to_string(),
                data: Vec::from(0_u32.to_le_bytes()),
            };
            header.number_of_variable_length_records += 1;
            header.offset_to_point_data += vlr.len(false) as u32;
            vlr.into_raw(false)
                .unwrap()
                .write_to(&mut wr)
                .map_err(|e| match e {
                    las::Error::Io(io_e) => io_e,
                    _ => panic!("Unexpected error"),
                })?;
        };

        // write "normal" points
        let mut nr_non_bogus = 0;
        'tasks_loop: loop {
            // get batch of points to write
            let points = match points_queue.pop() {
                Some(v) => v,
                None => 'steal_loop: loop {
                    let mut retry = false;
                    for stealer in &points_stealers {
                        match stealer.steal_batch_and_pop(&points_queue) {
                            Steal::Success(s) => break 'steal_loop s,
                            Steal::Retry => retry = true,
                            Steal::Empty => (),
                        }
                    }
                    if !retry {
                        break 'tasks_loop;
                    }
                },
            };

            // write
            nr_non_bogus += points.len();
            header.number_of_point_records += points.len() as u32;
            let points_by_return = write_point_data_i32(&mut wr, points.into_iter(), &format)?;
            for i in 0..5 {
                header.number_of_points_by_return[i] += points_by_return[i];
            }
        }

        // write bogus points
        'bogus_tasks_loop: loop {
            // get batch of points to write
            let points = match bogus_queue.pop() {
                Some(v) => v,
                None => 'bogus_steal_loop: loop {
                    let mut retry = false;
                    for stealer in &bogus_stealers {
                        match stealer.steal_batch_and_pop(&bogus_queue) {
                            Steal::Success(s) => break 'bogus_steal_loop s,
                            Steal::Retry => retry = true,
                            Steal::Empty => (),
                        }
                    }
                    if !retry {
                        break 'bogus_tasks_loop;
                    }
                },
            };

            // write
            header.number_of_point_records += points.len() as u32;
            let points_by_return = write_point_data_i32(&mut wr, points.into_iter(), &format)?;
            for i in 0..5 {
                header.number_of_points_by_return[i] += points_by_return[i];
            }
        }

        // write updated header
        wr.seek(SeekFrom::Start(header_position))?;
        header.write_to(&mut wr).map_err(|e| match e {
            las::Error::Io(io_e) => io_e,
            _ => panic!("Unexpected error"),
        })?;

        // write updated bogus points vlr
        if use_bogus_points_vlr {
            wr.seek(SeekFrom::Start(bogus_points_vlr_pos))?;
            let vlr = Vlr {
                user_id: BOGUS_POINTS_VLR_USER_ID.to_string(),
                record_id: BOGUS_POINTS_VLR_RECORD_ID,
                description: "Number of non bogus points.".to_string(),
                data: Vec::from((nr_non_bogus as u32).to_le_bytes()),
            };
            vlr.into_raw(false)
                .unwrap()
                .write_to(&mut wr)
                .map_err(|e| match e {
                    las::Error::Io(io_e) => io_e,
                    _ => panic!("Unexpected error"),
                })?;
        }
        Ok(())
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

        // las 1.2, Point format 0
        let version = Version::new(1, 2);
        let mut format = Format::new(0).unwrap();
        format.extra_bytes = Point::NR_EXTRA_BYTES as u16;
        format.is_compressed = self.compression;

        // encode (uncompressed) point data into buffer
        let number_of_point_records = points.len() as u32;
        let mut point_data = Vec::with_capacity(points.len() * format.len() as usize);
        let number_of_points_by_return =
            write_point_data_i32(Cursor::new(&mut point_data), points, &format)?;

        // string "LIDARSERV" for system identifier and generating software
        let mut lidarserv = [0; 32];
        let lidarserv_data = "LIDARSERV".bytes().collect::<Vec<_>>();
        lidarserv[..lidarserv_data.len()].copy_from_slice(lidarserv_data.as_slice());

        // bounds
        let (min, max) = match bounds.into_aabb() {
            Some(aabb) => (
                aabb.min::<I32Position>().decode(&coordinate_system),
                aabb.max::<I32Position>().decode(&coordinate_system),
            ),
            None => (Point3::new(-1.0, -1.0, -1.0), Point3::new(1.0, 1.0, 1.0)),
        };

        // header
        let mut header = las::raw::Header {
            version,
            system_identifier: lidarserv,
            generating_software: lidarserv,
            header_size: version.header_size(),
            offset_to_point_data: version.header_size() as u32,
            number_of_variable_length_records: 0,
            point_data_record_format: format.to_u8().unwrap(),
            point_data_record_length: format.len(),
            number_of_point_records,
            number_of_points_by_return,
            x_scale_factor: coordinate_system.scale().x,
            y_scale_factor: coordinate_system.scale().y,
            z_scale_factor: coordinate_system.scale().z,
            x_offset: coordinate_system.offset().x,
            y_offset: coordinate_system.offset().y,
            z_offset: coordinate_system.offset().z,
            min_x: min.x,
            min_y: min.y,
            min_z: min.z,
            max_x: max.x,
            max_y: max.y,
            max_z: max.z,
            ..Default::default()
        };

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
            let laz_vlr = {
                let mut laz_items = LazItemRecordBuilder::new();
                laz_items.add_item(LazItemType::Point10);
                if format.extra_bytes > 0 {
                    laz_items.add_item(laz::LazItemType::Byte(format.extra_bytes));
                }
                LazVlr::from_laz_items(laz_items.build())
            };
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
            header.point_data_record_format |= 0x80;

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

    fn write_las_work_stealing<W>(
        &self,
        las: WorkStealingLas<Point, <Point::Position as Position>::Component, I32CoordinateSystem>,
        wr: W,
    ) -> Result<(), WriteLasError>
    where
        W: Write + Seek + Send,
    {
        if self.compression {
            self.write_las_work_stealing_compressed(las, wr)
        } else {
            self.write_las_work_stealing_uncompressed(las, wr)
        }
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

        // make coordinate system according to las header transform
        let coordinate_system = I32CoordinateSystem::from_las_transform(
            Vector3::new(
                header.x_scale_factor,
                header.y_scale_factor,
                header.z_scale_factor,
            ),
            Vector3::new(header.x_offset, header.y_offset, header.z_offset),
        );

        // get bounds according to header
        let min_global = Point3::new(header.min_x, header.min_y, header.min_z);
        let max_global = Point3::new(header.max_x, header.max_y, header.max_z);
        let min = coordinate_system
            .encode_position(&min_global)
            .unwrap_or_else(|_| I32Position::from_components(i32::MIN, i32::MIN, i32::MIN));
        let max = coordinate_system
            .encode_position(&max_global)
            .unwrap_or_else(|_| I32Position::from_components(i32::MAX, i32::MAX, i32::MAX));
        let bounds = OptionAABB::new(
            Point3::new(min.x(), min.y(), min.z()),
            Point3::new(max.x(), max.y(), max.z()),
        );

        Ok(Las {
            points,
            bounds,
            non_bogus_points,
            coordinate_system,
        })
    }
}

fn write_point_data_i32<W: Write, P, It>(
    mut writer: W,
    points: It,
    format: &Format,
) -> Result<[u32; 5], io::Error>
where
    P: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes,
    It: Iterator,
    It::Item: Borrow<P>,
{
    let mut number_of_points_by_return = [0; 5];

    for point in points {
        let point = point.borrow();

        // count points by return number
        let return_number = point.attribute::<LasPointAttributes>().return_number & 0x07;
        if return_number != 0 && return_number < 6 {
            number_of_points_by_return[return_number as usize - 1] += 1;
        }

        // create raw point
        let attributes = point.attribute::<LasPointAttributes>();
        let extra_bytes = point.get_extra_bytes();
        assert_eq!(extra_bytes.len(), P::NR_EXTRA_BYTES);
        let raw_point = las::raw::Point {
            x: point.position().x(),
            y: point.position().y(),
            z: point.position().z(),
            intensity: attributes.intensity,
            flags: Flags::TwoByte(
                ((attributes.return_number & 0x07) << 5)
                    | ((attributes.number_of_returns & 0x07) << 2)
                    | if attributes.scan_direction { 2 } else { 0 }
                    | if attributes.edge_of_flight_line { 1 } else { 0 },
                attributes.classification,
            ),
            scan_angle: ScanAngle::Rank(attributes.scan_angle_rank),
            user_data: attributes.user_data,
            point_source_id: attributes.point_source_id,
            extra_bytes,
            ..Default::default()
        };

        // write into given stream
        raw_point
            .write_to(&mut writer, format)
            .map_err(|e| match e {
                las::Error::Io(io_e) => io_e,
                _ => panic!("Unexpected error"),
            })?;
    }

    Ok(number_of_points_by_return)
}

fn read_point_data_i32<R: Read, P>(
    mut read: R,
    format: &Format,
    number_of_point_records: usize,
) -> Result<Vec<P>, ReadLasError>
where
    P: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes,
{
    let mut points = Vec::with_capacity(number_of_point_records);
    for _ in 0..number_of_point_records {
        let raw = las::raw::Point::read_from(&mut read, format)?;

        // point with that position
        let position = P::Position::from_components(raw.x, raw.y, raw.z);
        let mut point = P::new(position);

        // set las point attributes
        let mut las_attr = LasPointAttributes::default();
        las_attr.intensity = raw.intensity;
        if let Flags::TwoByte(b1, b2) = raw.flags {
            las_attr.return_number = (b1 & 0xE0) >> 5;
            las_attr.number_of_returns = (b1 & 0x1C) >> 2;
            las_attr.scan_direction = (b1 & 0x02) == 0x02;
            las_attr.edge_of_flight_line = (b1 & 0x01) == 0x01;
            las_attr.classification = b2;
        } else {
            unreachable!("Point format 0 will always have Flags::TwoByte.")
        }
        las_attr.scan_angle_rank = if let ScanAngle::Rank(a) = raw.scan_angle {
            a
        } else {
            unreachable!("Point format 0 will always have ScanAngle::Rank.")
        };
        las_attr.user_data = raw.user_data;
        las_attr.point_source_id = raw.point_source_id;
        point.set_attribute(las_attr);

        // set extra bytes
        point.set_extra_bytes(raw.extra_bytes.as_slice());
        points.push(point)
    }
    Ok(points)
}

fn read_las_string(las_str: &[u8]) -> Result<String, FromUtf8Error> {
    let bytes = las_str
        .iter()
        .take_while(|byte| **byte != 0)
        .cloned()
        .collect();
    String::from_utf8(bytes)
}
