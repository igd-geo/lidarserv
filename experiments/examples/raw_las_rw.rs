use las::point::Format;
use las::{Version, Vlr, Write};
use laz::{LasZipCompressor, LasZipDecompressor, LazItemRecordBuilder, LazItemType, LazVlr};
use lidarserv_common::nalgebra::Point3;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::{Cursor, Seek, SeekFrom};
use std::string::FromUtf8Error;

#[derive(Debug, Clone)]
struct LasReadError;

impl Display for LasReadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Las read error")
    }
}

impl Error for LasReadError {}

fn main() {
    let points = vec![
        Point3::new(0, 1, 2),
        Point3::new(3, 4, 5),
        Point3::new(6, 7, 8),
    ];

    let data = write(points.clone()).unwrap();
    std::fs::write("example_write.las", &data).unwrap();

    let points_back = read(data).unwrap();
    println!("{:?}", points_back);

    let data = write_compressed(points.clone()).unwrap();
    std::fs::write("example_write.laz", &data).unwrap();

    let points_back = read(data).unwrap();
    println!("{:?}", points_back);

    let data = write_compressed_baseline(points).unwrap();
    std::fs::write("example_write_baseline.laz", &data).unwrap();

    let points_back = read(data).unwrap();
    println!("{:?}", points_back);
}

fn write(points: Vec<Point3<i32>>) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut data = Vec::new();
    let mut write = Cursor::new(&mut data);

    // write header
    let version = Version::new(1, 2);
    let mut format = Format::new(0).unwrap();
    format.extra_bytes = 0; // nr bytes here
    let mut lidarserv = [0; 32];
    let lidarserv_data = "LIDARSERV".bytes().collect::<Vec<_>>();
    lidarserv[..lidarserv_data.len()].copy_from_slice(lidarserv_data.as_slice());
    let header = las::raw::Header {
        version,
        system_identifier: lidarserv,
        generating_software: lidarserv,
        header_size: version.header_size(),
        offset_to_point_data: version.header_size() as u32,
        number_of_variable_length_records: 0,
        point_data_record_format: format.to_u8().unwrap(),
        point_data_record_length: format.len(),
        number_of_point_records: points.len() as u32,
        number_of_points_by_return: [points.len() as u32, 0, 0, 0, 0],
        x_scale_factor: 1., // coordinate system transform here
        y_scale_factor: 1.,
        z_scale_factor: 1.,
        x_offset: 0.,
        y_offset: 0.,
        z_offset: 0.,
        min_x: 0.0, // bounding box here
        min_y: 1.0,
        min_z: 2.0,
        max_x: 6.0,
        max_y: 7.0,
        max_z: 8.0,
        ..Default::default()
    };
    header.write_to(&mut write)?;

    // write points
    for point in points {
        let raw_point = las::raw::Point {
            x: point.x,
            y: point.y,
            z: point.z,
            ..Default::default()
        };
        raw_point.write_to(&mut write, &format)?;
    }

    drop(write);
    Ok(data)
}

fn write_compressed(points: Vec<Point3<i32>>) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut data = Vec::new();
    let mut write = Cursor::new(&mut data);

    let version = Version::new(1, 2);
    let mut format = Format::new(0).unwrap();
    format.is_compressed = true;
    format.extra_bytes = 0; // nr bytes here

    // laszip vlr
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
        laz_vlr.write_to(&mut laz_vlr_data).unwrap();
        Vlr {
            user_id: LazVlr::USER_ID.to_string(),
            record_id: LazVlr::RECORD_ID,
            description: LazVlr::DESCRIPTION.to_string(),
            data: laz_vlr_data.into_inner(),
        }
    };

    // write header
    let mut lidarserv = [0; 32];
    let lidarserv_data = "LIDARSERV".bytes().collect::<Vec<_>>();
    lidarserv[..lidarserv_data.len()].copy_from_slice(lidarserv_data.as_slice());
    let header = las::raw::Header {
        version,
        system_identifier: lidarserv,
        generating_software: lidarserv,
        header_size: version.header_size(),
        offset_to_point_data: version.header_size() as u32 + vlr.len(false) as u32,
        number_of_variable_length_records: 1,
        point_data_record_format: format.to_u8().unwrap() | 0x80,
        point_data_record_length: format.len(),
        number_of_point_records: points.len() as u32,
        number_of_points_by_return: [points.len() as u32, 0, 0, 0, 0],
        x_scale_factor: 1., // coordinate system transform here
        y_scale_factor: 1.,
        z_scale_factor: 1.,
        x_offset: 0.,
        y_offset: 0.,
        z_offset: 0.,
        min_x: 0.0, // bounding box here
        min_y: 1.0,
        min_z: 2.0,
        max_x: 6.0,
        max_y: 7.0,
        max_z: 8.0,
        ..Default::default()
    };
    header.write_to(&mut write)?;

    // write laszip vlr
    vlr.into_raw(false)?.write_to(&mut write)?;

    // write uncompressed point data
    let mut point_data = Cursor::new(Vec::with_capacity(points.len() * format.len() as usize));
    for point in points {
        let raw_point = las::raw::Point {
            x: point.x,
            y: point.y,
            z: point.z,
            ..Default::default()
        };
        raw_point.write_to(&mut point_data, &format)?;
    }
    let point_data = point_data.into_inner();

    // compress points
    let mut compressor = LasZipCompressor::new(write, laz_vlr)?;
    compressor.compress_many(point_data.as_slice()).unwrap();
    compressor.done().unwrap();
    drop(compressor);

    Ok(data)
}

fn read_las_string(las_str: &[u8]) -> Result<String, FromUtf8Error> {
    let bytes = las_str
        .iter()
        .take_while(|byte| **byte != 0)
        .cloned()
        .collect();
    String::from_utf8(bytes)
}

fn read(data: Vec<u8>) -> Result<Vec<Point3<i32>>, Box<dyn Error>> {
    let mut read = Cursor::new(data);

    // read header
    let header = las::raw::Header::read_from(&mut read)?;

    // read vlrs
    let mut vlrs = Vec::new();
    for _ in 0..header.number_of_variable_length_records {
        let vlr = las::raw::Vlr::read_from(&mut read, false)?;
        vlrs.push(vlr);
    }

    // check, if file is compressed
    let format = Format::new(header.point_data_record_format)?;
    let is_compressed = format.is_compressed;

    // if compressed: decompress point data
    if is_compressed {
        // find laszip vlr
        let vlr = if let Some(v) = vlrs.iter().find(|it| {
            read_las_string(&it.user_id)
                .map(|uid| uid == LazVlr::USER_ID)
                .unwrap_or(false)
                && it.record_id == LazVlr::RECORD_ID
        }) {
            v
        } else {
            return Err(Box::new(LasReadError));
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
        decompressor.decompress_many(data.as_mut_slice()).unwrap();
        read = Cursor::new(data);
    } else {
        read.seek(SeekFrom::Start(header.offset_to_point_data as u64))?;
    }

    // read points
    let mut points = Vec::new();
    for _ in 0..header.number_of_point_records {
        let raw_point = las::raw::Point::read_from(&mut read, &format)?;
        points.push(Point3::new(raw_point.x, raw_point.y, raw_point.z));
    }

    Ok(points)
}

fn write_compressed_baseline(points: Vec<Point3<i32>>) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut header = las::Builder::default();
    header.point_format.is_compressed = true;
    let header = header.into_header()?;
    let data = Cursor::new(Vec::new());
    let mut writer = las::Writer::new(data, header)?;
    for point in points {
        writer.write(las::Point {
            x: point.x as f64,
            y: point.y as f64,
            z: point.z as f64,
            ..Default::default()
        })?;
    }
    Ok(writer.into_inner()?.into_inner())
}
