mod insertion_rate;
mod latency;
mod point;
mod queries;
mod query_performance;

use crate::insertion_rate::measure_insertion_rate;
use crate::latency::measure_latency;
use crate::point::{Point, PointIdAttribute};
use crate::queries::{preset_query_1, preset_query_2};
use crate::query_performance::measure_query_performance;
use lidarserv_common::geometry::grid::{I32Grid, I32GridHierarchy, LodLevel};
use lidarserv_common::geometry::points::PointType;
use lidarserv_common::geometry::position::{I32CoordinateSystem, I32Position};
use lidarserv_common::geometry::sampling::{GridCenterSampling, GridCenterSamplingFactory};
use lidarserv_common::index::octree::grid_cell_directory::GridCellDirectory;
use lidarserv_common::index::octree::page_manager::OctreePageLoader;
use lidarserv_common::index::octree::writer::TaskPriorityFunction;
use lidarserv_common::index::octree::Octree;
use lidarserv_common::index::sensor_pos::meta_tree::MetaTree;
use lidarserv_common::index::sensor_pos::page_manager::{BinDataLoader, FileIdDirectory};
use lidarserv_common::index::sensor_pos::partitioned_node::RustCellHasher;
use lidarserv_common::index::sensor_pos::point::SensorPositionAttribute;
use lidarserv_common::index::sensor_pos::{SensorPosIndex, SensorPosIndexParams};
use lidarserv_common::las::{I32LasReadWrite, LasPointAttributes};
use lidarserv_server::net::protocol::connection::Connection;
use log::{error, info};
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::env;
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use velodyne_csv_replay::iter_points::iter_points;

type I32SensorPosIndex = SensorPosIndex<
    I32GridHierarchy,
    GridCenterSamplingFactory<I32GridHierarchy, Point, I32Position, i32>,
    i32,
    I32LasReadWrite,
    I32CoordinateSystem,
>;
type I32Octree = Octree<
    Point,
    I32GridHierarchy,
    I32LasReadWrite,
    GridCenterSampling<I32Grid, Point, I32Position, i32>,
    i32,
    I32CoordinateSystem,
    GridCenterSamplingFactory<I32GridHierarchy, Point, I32Position, i32>,
>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub data_folder: PathBuf,
    pub points_file: PathBuf,
    pub trajectory_file: PathBuf,
    pub offset_x: f64,
    pub offset_y: f64,
    pub offset_z: f64,
    pub target_point_pressure: usize,
    pub pps: usize,
    pub fps: usize,
    pub num_threads: u16,
    pub task_priority_function: String,
    pub max_bogus_inner: usize,
    pub max_bogus_leaf: usize,
    pub max_cache_size: usize,
    pub max_node_size: usize,
}

impl Config {
    pub fn from_env() -> Self {
        Config {
            data_folder: get_env("LIDARSERV_DATA_FOLDER"),
            points_file: get_env("LIDARSERV_POINTS_FILE"),
            trajectory_file: get_env("LIDARSERV_TRAJECTORY_FILE"),
            offset_x: get_env("LIDARSERV_OFFSET_X"),
            offset_y: get_env("LIDARSERV_OFFSET_Y"),
            offset_z: get_env("LIDARSERV_OFFSET_Z"),
            target_point_pressure: get_env("LIDARSERV_TARGET_POINT_PRESSURE"),
            pps: get_env("LIDARSERV_PPS"),
            fps: get_env("LIDARSERV_FPS"),
            num_threads: get_env("LIDARSERV_NUM_THREADS"),
            task_priority_function: get_env("LIDARSERV_TASK_PRIORITY_FUNCTION"),
            max_bogus_inner: get_env("LIDARSERV_MAX_BOGUS_INNER"),
            max_bogus_leaf: get_env("LIDARSERV_MAX_BOGUS_LEAF"),
            max_cache_size: get_env("LIDARSERV_MAX_CACHE_SIZE"),
            max_node_size: get_env("LIDARSERV_MAX_NODE_SIZE"),
        }
    }
}

fn main() {
    // init
    dotenv::dotenv().unwrap();
    pretty_env_logger::init();
    let base_config = Config::from_env();

    // read point data
    let coordinate_system = I32CoordinateSystem::from_las_transform(
        Vector3::new(0.001, 0.001, 0.001),
        Vector3::new(0.0, 0.0, 0.0),
    );
    let points = read_points(&coordinate_system, &base_config);

    let mut runs = HashMap::new();

    // "default run" with base settings
    {
        let result = evaluate(&base_config, &points, &coordinate_system);
        runs.entry("default".to_string())
            .or_insert_with(Vec::new)
            .push(result);
    }

    // modify task priority function
    let prio_function_names = [
        "Lod",
        "TaskAge",
        "NewestPoint",
        "NrPoints",
        "NrPointsWeighted",
        "OldestPoint",
    ];
    for tpf in prio_function_names {
        let config = Config {
            task_priority_function: tpf.to_string(),
            ..base_config.clone()
        };
        let results = evaluate(&config, &points, &coordinate_system);
        runs.entry("task_priority_function".to_string())
            .or_insert_with(Vec::new)
            .push(results);
    }
    for tpf in prio_function_names {
        let config = Config {
            task_priority_function: tpf.to_string(),
            max_cache_size: 50,
            ..base_config.clone()
        };
        let results = evaluate(&config, &points, &coordinate_system);
        runs.entry("task_priority_function_low_cache".to_string())
            .or_insert_with(Vec::new)
            .push(results);
    }

    // modify threads
    for num_threads in [1, 2, 4, 8] {
        let config = Config {
            num_threads,
            ..base_config.clone()
        };
        let results = evaluate(&config, &points, &coordinate_system);
        runs.entry("num_threads".to_string())
            .or_insert_with(Vec::new)
            .push(results);
    }

    // modify cache size
    for max_cache_size in [8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192] {
        let config = Config {
            max_cache_size,
            ..base_config.clone()
        };
        let results = evaluate(&config, &points, &coordinate_system);
        runs.entry("max_cache_size".to_string())
            .or_insert_with(Vec::new)
            .push(results);
    }

    // modify node size
    for max_node_size in [6_250, 12_500, 25_000, 50_000, 100_000, 200_000] {
        let config = Config {
            max_node_size,
            ..base_config.clone()
        };
        let results = evaluate(&config, &points, &coordinate_system);
        runs.entry("max_node_size".to_string())
            .or_insert_with(Vec::new)
            .push(results);
    }

    println!("{}", json!(runs))
}

fn evaluate(
    config: &Config,
    points: &Vec<Point>,
    coordinate_system: &I32CoordinateSystem,
) -> serde_json::Value {
    info!("Configuration: {:#?}", config);

    // measure latency
    reset_data_folder(&config);
    info!("Measuring octree latency...");
    let octree_latency = {
        let index = create_octree_index(coordinate_system.clone(), &config);
        let query = preset_query_2();
        measure_latency(index, &points, query, &config)
    };
    info!("Results: {}", &octree_latency);
    info!("Measuring sensorpos latency...");
    let sensorpos_latency = {
        let index = create_sensor_pos_index(coordinate_system.clone(), &config);
        let query = preset_query_2();
        measure_latency(index, &points, query, &config)
    };
    info!("Results: {}", &sensorpos_latency);

    // measure insertion rate
    reset_data_folder(&config);
    info!("Measuring octree insertion rate...");
    let mut octree_index = create_octree_index(coordinate_system.clone(), &config);
    let octree_insertion_rate = measure_insertion_rate(&mut octree_index, &points, &config);
    info!("Results: {}", &octree_insertion_rate);
    info!("Measuring sensorpos insertion rate...");
    let mut sensor_pos_index = create_sensor_pos_index(coordinate_system.clone(), &config);
    let sensorpos_insertion_rate = measure_insertion_rate(&mut sensor_pos_index, &points, &config);
    info!("Results: {}", &sensorpos_insertion_rate);

    // measure query performance
    info!("Measuring octree query perf...");
    let octree_query_perf = measure_query_performance(octree_index);
    info!("Results: {}", &octree_query_perf);
    info!("Measuring sensorpos query perf...");
    let sensorpos_query_perf = measure_query_performance(sensor_pos_index);
    info!("Results: {}", &sensorpos_query_perf);
    json!({
        "config": config,
        "sensor_pos_index": {
            "latency": sensorpos_latency,
            "insertion_rate": sensorpos_insertion_rate,
            "query_performance": sensorpos_query_perf
        },
        "octree_index": {
            "latency": octree_latency,
            "insertion_rate": octree_insertion_rate,
            "query_performance": octree_query_perf
        }
    })
}

fn read_points(coordinate_system: &I32CoordinateSystem, config: &Config) -> Vec<Point> {
    let point_file: PathBuf = config.points_file.clone();
    let trajectory_file: PathBuf = config.trajectory_file.clone();
    let offset = Vector3::new(config.offset_x, config.offset_y, config.offset_z);

    info!("Reading points...");
    let points: Vec<_> = iter_points(&trajectory_file, &point_file, offset)
        .unwrap()
        .enumerate()
        .map(|(id, (t, p))| {
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

fn reset_data_folder(config: &Config) {
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

fn create_sensor_pos_index(
    coordinate_system: I32CoordinateSystem,
    config: &Config,
) -> I32SensorPosIndex {
    let mut data_folder: PathBuf = config.data_folder.clone();
    data_folder.push("sensorpos");
    let mut meta_tree_file = data_folder.clone();
    meta_tree_file.push("meta.bin");

    let nr_threads = config.num_threads;
    let max_cache_size = config.max_cache_size;

    let point_grid_hierarchy = I32GridHierarchy::new(17);
    let sampling_factory = GridCenterSamplingFactory::new(point_grid_hierarchy);
    let sensor_grid_hierarchy = I32GridHierarchy::new(14);
    let meta_tree = MetaTree::new(sensor_grid_hierarchy);
    let page_loader = BinDataLoader::new(data_folder.clone(), "laz".to_string());
    let directory = FileIdDirectory::from_meta_tree(&meta_tree, nr_threads as usize);
    let page_manager = lidarserv_common::index::sensor_pos::page_manager::PageManager::new(
        page_loader,
        directory,
        max_cache_size,
    );
    let las_loader = I32LasReadWrite::new(true);

    let params = SensorPosIndexParams {
        nr_threads: config.num_threads as usize,
        max_node_size: config.max_node_size,
        meta_tree_file,
        sampling_factory,
        page_manager,
        meta_tree,
        las_loader,
        coordinate_system,
        max_lod: LodLevel::from_level(10),
        max_delay: Duration::from_secs(1),
        coarse_lod_steps: 5,
        hasher: RustCellHasher::from_state((83675784, 435659)),
    };
    SensorPosIndex::new(params)
}

fn create_octree_index(coordinate_system: I32CoordinateSystem, config: &Config) -> I32Octree {
    let mut data_folder: PathBuf = config.data_folder.clone();
    data_folder.push("octree");
    let node_hierarchy = I32GridHierarchy::new(11);
    let point_hierarchy = I32GridHierarchy::new(17);
    let max_lod = LodLevel::from_level(10);
    let sample_factory = GridCenterSamplingFactory::new(point_hierarchy);
    let las_loader = I32LasReadWrite::new(true);
    let page_loader = OctreePageLoader::new(las_loader.clone(), data_folder.clone());
    let mut directory_file_name = data_folder.clone();
    directory_file_name.push("directory.bin");
    let page_directory = GridCellDirectory::new(&max_lod, directory_file_name).unwrap();

    Octree::new(
        config.num_threads,
        match config.task_priority_function.as_str() {
            "Lod" => TaskPriorityFunction::Lod,
            "TaskAge" => TaskPriorityFunction::TaskAge,
            "NewestPoint" => TaskPriorityFunction::NewestPoint,
            "NrPoints" => TaskPriorityFunction::NrPoints,
            "NrPointsWeighted" => TaskPriorityFunction::NrPointsWeightedByTaskAge,
            "OldestPoint" => TaskPriorityFunction::OldestPoint,
            _ => {
                error!("invalid value for LIDARSERV_TASK_PRIORITY_FUNCTION");
                panic!()
            }
        },
        max_lod,
        config.max_bogus_inner,
        config.max_bogus_leaf,
        node_hierarchy,
        page_loader,
        page_directory,
        config.max_cache_size,
        sample_factory,
        las_loader,
        coordinate_system,
    )
}

fn get_env<T: FromStr>(name: &str) -> T
where
    <T as FromStr>::Err: Display,
{
    let str_val = match env::var(name) {
        Ok(v) => v,
        Err(_) => {
            error!("Missing env var: {}", name);
            panic!();
        }
    };
    match T::from_str(&str_val) {
        Ok(v) => v,
        Err(e) => {
            error!("Invalid value {}: {}", name, e);
            panic!();
        }
    }
}
