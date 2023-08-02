use std::fs::File;
use std::io::BufReader;
use crate::point::{Point, PointIdAttribute};
use lidarserv_common::geometry::points::PointType;
use lidarserv_common::geometry::position::I32CoordinateSystem;
use log::{info};
use std::path::PathBuf;
use rayon::prelude::ParallelSliceMut;
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
) -> Vec<Point> {
    info!("Reading points...");

    let mut points: Vec<Point> = Vec::new();

    // check file format
    if settings.points_file.extension().unwrap() == "txt" {
        // CSV / TXT Format
        points = iter_points(
            &settings.trajectory_file,
            &settings.points_file,
            settings.offset,
        )
        .unwrap()
        .enumerate()
        .map(|(id, (_, p))| {
            let las_point = p.into_las_point(coordinate_system).unwrap();
            Point {
                position: las_point.position().clone(),
                point_id: PointIdAttribute(id),
                las_attributes: las_point.attribute().clone(),
            }
        })
        .collect();
    } else if settings.points_file.extension().unwrap() == "las" || settings.points_file.extension().unwrap() == "laz" {
        // LAS / LAZ Format
        let f = File::open(&settings.points_file).unwrap();
        let mut reader = BufReader::new(f);
        let compression = settings.points_file.extension().unwrap() == "laz";
        let las_reader : I32LasReadWrite = I32LasReadWrite::new(compression, settings.las_point_record_format);
        let mut result : Las<Vec<LasPoint>> = las_reader.read_las(&mut reader).unwrap();

        //convert Vec<LasPoint> to Vec<Point>
        points = result.points.drain(..).map(|las_point| {
            Point {
                position: las_point.position().clone(),
                point_id: PointIdAttribute::default(),
                las_attributes: las_point.attribute().clone(),
            }
        }).collect();

        //sort points by gps_time
        info!("Sorting LAS points by time");
        result.points.par_sort_unstable_by(|a, b| a.attribute().gps_time.partial_cmp(&b.attribute().gps_time).unwrap());
    } else {
        panic!("Unknown file format");
    }
    info!("Read a total of {} points.", points.len());
    points
}

pub fn reset_data_folder(settings: &settings::Base) {
    info!("Resetting data folder...");
    let data_folder: PathBuf = settings.data_folder.clone();
    std::fs::remove_dir_all(&data_folder).unwrap();
    std::fs::create_dir(&data_folder).unwrap();
    let mut octree = data_folder.clone();
    octree.push("octree");
    std::fs::create_dir(&octree).unwrap();
}
