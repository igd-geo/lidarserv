use crate::geometry::bounding_box::{BaseAABB, OptionAABB};
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{CoordinateSystem, Position};
use crate::geometry::position::{I32CoordinateSystem, I32Position};
use crate::las::{LasPointAttributes, ReadLasError};
use las::point::Format;
use las::raw::point::{Flags, ScanAngle};
use las::raw::Header;
use las::Version;
use nalgebra::{Point3, Vector3};
use std::borrow::Borrow;
use std::io;
use std::io::{Read, Write};
use std::string::FromUtf8Error;
use log::debug;

pub fn init_las_header(
    is_compressed: bool,
    bounds_min: Point3<f64>,
    bounds_max: Point3<f64>,
    coordinate_system: &I32CoordinateSystem,
    point_record_format: u8,
) -> (las::raw::Header, Format) {
    // las 1.2, Point format 0-3
    let version = Version::new(1, 2);
    let mut format = Format::new(point_record_format).unwrap();
    format.is_compressed = is_compressed;

    // string "LIDARSERV" for system identifier and generating software
    let mut lidarserv = [0; 32];
    let lidarserv_data = "LIDARSERV".bytes().collect::<Vec<_>>();
    lidarserv[..lidarserv_data.len()].copy_from_slice(lidarserv_data.as_slice());

    let mut point_data_record_format = format.to_u8().unwrap();
    if is_compressed {
        point_data_record_format |= 0x80;
    }

    let header = las::raw::Header {
        version,
        system_identifier: lidarserv,
        generating_software: lidarserv,
        header_size: version.header_size(),
        offset_to_point_data: version.header_size() as u32,
        number_of_variable_length_records: 0,
        point_data_record_format,
        point_data_record_length: format.len(),
        x_scale_factor: coordinate_system.scale().x,
        y_scale_factor: coordinate_system.scale().y,
        z_scale_factor: coordinate_system.scale().z,
        x_offset: coordinate_system.offset().x,
        y_offset: coordinate_system.offset().y,
        z_offset: coordinate_system.offset().z,
        min_x: bounds_min.x,
        min_y: bounds_min.y,
        min_z: bounds_min.z,
        max_x: bounds_max.x,
        max_y: bounds_max.y,
        max_z: bounds_max.z,
        ..Default::default()
    };

    (header, format)
}

pub fn write_point_data_i32<W: Write, P, It>(
    mut writer: W,
    points: It,
    format: &Format,
) -> Result<[u32; 5], io::Error>
where
    P: PointType<Position = I32Position> + WithAttr<LasPointAttributes>,
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
            color: if format.has_color {
                Some(las::Color::new(
                    attributes.color.0,
                    attributes.color.1,
                    attributes.color.2,
                ))
            } else {
                None
            },
            gps_time: if format.has_gps_time { Some(attributes.gps_time) } else { None },
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

pub fn read_las_string(las_str: &[u8]) -> Result<String, FromUtf8Error> {
    let bytes = las_str
        .iter()
        .take_while(|byte| **byte != 0)
        .cloned()
        .collect();
    String::from_utf8(bytes)
}

pub fn read_point_data_i32<R: Read, P>(
    mut read: R,
    format: &Format,
    number_of_point_records: usize,
) -> Result<Vec<P>, ReadLasError>
where
    P: PointType<Position = I32Position> + WithAttr<LasPointAttributes>,
{
    let mut points = Vec::with_capacity(number_of_point_records);
    for _ in 0..number_of_point_records {
        let raw = las::raw::Point::read_from(&mut read, format)?;

        // point with that position
        let position = P::Position::from_components(raw.x, raw.y, raw.z);
        let mut point = P::new(position);

        // set las point attributes
        let mut las_attr = LasPointAttributes {
            intensity: raw.intensity,
            ..Default::default()
        };
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
        las_attr.color = raw
            .color
            .map(|c| (c.red, c.green, c.blue))
            .unwrap_or((0, 0, 0));
        las_attr.gps_time = raw.gps_time.unwrap_or(0.0);
        point.set_attribute(las_attr);

        points.push(point)
    }
    Ok(points)
}

pub fn get_header_info_i32(header: &Header) -> (I32CoordinateSystem, OptionAABB<i32>) {
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

    (coordinate_system, bounds)
}
