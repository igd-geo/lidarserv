use crate::geometry::bounding_box::{BaseAABB, OptionAABB};
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{CoordinateSystem, I32CoordinateSystem, I32Position, Position};
use crate::nalgebra::Scalar;
use las::point::Format;
use las::raw::point::{Flags, ScanAngle};
use las::{Version, Vlr};
use laz::{
    LasZipCompressor, LasZipDecompressor, LasZipError, LazItemRecordBuilder, LazItemType, LazVlr,
};
use nalgebra::{Point3, Vector3};
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
    pub bogus_points: Option<u32>,
    pub coordinate_system: CSys,
}

pub trait LasReadWrite<Point, CSys>
where
    Point: PointType,
{
    fn write_las<W: Write + Seek + Send>(
        &self,
        las: Las<&[Point], <Point::Position as Position>::Component, CSys>,
        wr: W,
    ) -> Result<(), WriteLasError>;

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
}

impl<Point> LasReadWrite<Point, I32CoordinateSystem> for I32LasReadWrite
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes,
{
    fn write_las<W: Write + Seek + Send>(
        &self,
        las: Las<&[Point], i32, I32CoordinateSystem>,
        mut wr: W,
    ) -> Result<(), WriteLasError> {
        let Las {
            points,
            bounds,
            bogus_points,
            coordinate_system,
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
            number_of_point_records: points.len() as u32,
            number_of_points_by_return: {
                let mut number_of_points_by_return = [0; 5];
                for point in points {
                    let return_number =
                        point.attribute::<LasPointAttributes>().return_number & 0x07;
                    if return_number != 0 && return_number < 6 {
                        number_of_points_by_return[return_number as usize - 1] += 1;
                    }
                }
                number_of_points_by_return
            },
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
        let bogus_points_vlr = if let Some(bogus_points) = bogus_points {
            let vlr = Vlr {
                user_id: BOGUS_POINTS_VLR_USER_ID.to_string(),
                record_id: BOGUS_POINTS_VLR_RECORD_ID,
                description: "Number of bogus points.".to_string(),
                data: Vec::from(bogus_points.to_le_bytes()),
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

            // encode uncompressed point data into extra buffer
            let mut point_data =
                Cursor::new(Vec::with_capacity(points.len() * format.len() as usize));
            write_point_data_i32(&mut point_data, points, &format)?;
            let point_data = point_data.into_inner();

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
            write_point_data_i32(wr, points, &format)?;
        }
        Ok(())
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
        let bogus_points = vlrs
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
            bogus_points,
            coordinate_system,
        })
    }
}

fn write_point_data_i32<W: Write, P>(
    mut writer: W,
    points: &[P],
    format: &Format,
) -> Result<(), io::Error>
where
    P: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes,
{
    for point in points {
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
        raw_point
            .write_to(&mut writer, format)
            .map_err(|e| match e {
                las::Error::Io(io_e) => io_e,
                _ => panic!("Unexpected error"),
            })?;
    }

    Ok(())
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
        {
            let las_attr = point.attribute_mut::<LasPointAttributes>();
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
        }

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
