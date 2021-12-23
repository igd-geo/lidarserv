use crate::config::Config;
use crate::point::{Point, PointIdAttribute};
use lidarserv_common::geometry::points::PointType;
use lidarserv_common::geometry::position::{I32CoordinateSystem, I32Position};
use lidarserv_common::index::sensor_pos::point::SensorPositionAttribute;
use log::info;
use nalgebra::Vector3;
use std::path::PathBuf;
use velodyne_csv_replay::iter_points::iter_points;

pub mod config;
pub mod indexes;
pub mod insertion_rate;
pub mod latency;
pub mod point;
pub mod queries;
pub mod query_performance;
pub mod thermal_throttle;

pub fn read_points(coordinate_system: &I32CoordinateSystem, config: &Config) -> Vec<Point> {
    let point_file: PathBuf = config.points_file.clone();
    let trajectory_file: PathBuf = config.trajectory_file.clone();
    let offset = Vector3::new(config.offset_x, config.offset_y, config.offset_z);

    info!("Reading points...");
    let points: Vec<_> = iter_points(&trajectory_file, &point_file, offset)
        .unwrap()
        .enumerate()
        .map(|(id, (_, p))| {
            let las_point = p.into_las_point(coordinate_system).unwrap();
            Point {
                position: las_point.position().clone(),
                sensor_position: las_point
                    .attribute::<SensorPositionAttribute<I32Position>>()
                    .clone(),
                point_id: PointIdAttribute(id),
            }
        })
        .collect();
    info!("Read a total of {} points.", points.len());
    points
}

pub fn reset_data_folder(config: &Config) {
    let data_folder: PathBuf = config.data_folder.clone();
    std::fs::remove_dir_all(&data_folder).unwrap();
    std::fs::create_dir(&data_folder).unwrap();
    let mut octree = data_folder.clone();
    octree.push("octree");
    let mut sensorpos = data_folder.clone();
    sensorpos.push("sensorpos");
    std::fs::create_dir(&octree).unwrap();
    std::fs::create_dir(&sensorpos).unwrap();
}
