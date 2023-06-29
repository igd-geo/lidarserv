use std::fs::File;
use std::io::BufReader;
use crate::point::{Point, PointIdAttribute};
use lidarserv_common::geometry::points::PointType;
use lidarserv_common::geometry::position::I32CoordinateSystem;
use log::info;
use std::path::PathBuf;
use input_file_replay::iter_points::iter_points;
use lidarserv_common::las::{I32LasReadWrite, Las};
use lidarserv_server::index::point::LasPoint;

pub mod indexes;
pub mod insertion_rate;
pub mod latency;
pub mod point;
pub mod queries;
pub mod query_performance;
pub mod settings;
pub mod thermal_throttle;

pub fn read_points(
    coordinate_system: &I32CoordinateSystem,
    settings: &settings::Base,
) -> Vec<LasPoint> {
    info!("Reading points...");

    let mut points: Vec<LasPoint> = Vec::new();

    // check file format
    if settings.trajectory_file.extension().unwrap() != "las" {
        // CSV / TXT Format
        points = iter_points(
            &settings.trajectory_file,
            &settings.points_file,
            settings.offset,
        )
            .unwrap()
            .enumerate()
            .map(|(id, (_, p))| {
                p.into_las_point(coordinate_system).unwrap()
            })
            .collect();
    } else {
        // LAS Format
        let f = File::open(&settings.points_file).unwrap();
        let mut reader = BufReader::new(f);
        let las_reader : I32LasReadWrite = I32LasReadWrite::new(false, settings.las_point_record_format);
        let mut result : Las<Vec<LasPoint>> = las_reader.read_las(&mut reader).unwrap();
    }
    info!("Read a total of {} points.", points.len());
    points
}

pub fn reset_data_folder(settings: &settings::Base) {
    let data_folder: PathBuf = settings.data_folder.clone();
    std::fs::remove_dir_all(&data_folder).unwrap();
    std::fs::create_dir(&data_folder).unwrap();
    let mut octree = data_folder.clone();
    octree.push("octree");
    std::fs::create_dir(&octree).unwrap();
}
