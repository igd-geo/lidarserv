use log::error;
use serde::{Deserialize, Serialize};
use std::env;
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;

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
    pub compression: bool,
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
            compression: get_env("LIDARSERV_COMPRESSION"),
        }
    }
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
