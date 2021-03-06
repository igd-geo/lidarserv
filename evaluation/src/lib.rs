use crate::point::{Point, PointIdAttribute};
use lidarserv_common::geometry::points::PointType;
use lidarserv_common::geometry::position::I32CoordinateSystem;
use lidarserv_common::index::sensor_pos::point::SensorPositionAttribute;
use log::info;
use std::path::PathBuf;
use velodyne_csv_replay::iter_points::iter_points;

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
    let points: Vec<_> = iter_points(
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
            sensor_position: las_point.attribute::<SensorPositionAttribute>().clone(),
            point_id: PointIdAttribute(id),
        }
    })
    .collect();
    info!("Read a total of {} points.", points.len());
    points
}

pub fn reset_data_folder(settings: &settings::Base) {
    let data_folder: PathBuf = settings.data_folder.clone();
    std::fs::remove_dir_all(&data_folder).unwrap();
    std::fs::create_dir(&data_folder).unwrap();
    let mut octree = data_folder.clone();
    octree.push("octree");
    let mut sensorpos = data_folder;
    sensorpos.push("sensorpos");
    std::fs::create_dir(&octree).unwrap();
    std::fs::create_dir(&sensorpos).unwrap();
}
