use evaluation::config::Config;
use evaluation::indexes::{create_octree_index, create_sensor_pos_index};
use evaluation::insertion_rate::measure_insertion_rate;
use evaluation::latency::measure_latency;
use evaluation::point::Point;
use evaluation::queries::preset_query_2;
use evaluation::query_performance::measure_query_performance;
use evaluation::thermal_throttle::processor_cooldown;
use evaluation::{read_points, reset_data_folder};
use lidarserv_common::geometry::position::I32CoordinateSystem;
use log::{info, warn};
use nalgebra::Vector3;
use serde_json::json;
use std::collections::HashMap;

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

    let is_simple = std::env::args().any(|arg| arg == "simple");
    if is_simple {
        run_simple(base_config, points, coordinate_system);
    } else {
        run_full(base_config, points, coordinate_system);
    }
}

fn run_simple(base_config: Config, points: Vec<Point>, coordinate_system: I32CoordinateSystem) {
    let mut runs = HashMap::new();

    // modify compression
    for compression in [false, true] {
        let config = Config {
            compression,
            ..base_config.clone()
        };
        let result = evaluate(&config, &points, &coordinate_system, true, true);
        runs.entry("compression".to_string())
            .or_insert_with(Vec::new)
            .push(result);
    }

    println!("{}", json!(runs))
}

fn run_full(base_config: Config, points: Vec<Point>, coordinate_system: I32CoordinateSystem) {
    let mut runs = HashMap::new();

    // "default run" with base settings
    {
        let result = evaluate(&base_config, &points, &coordinate_system, true, true);
        runs.entry("default".to_string())
            .or_insert_with(Vec::new)
            .push(result);
    }

    // modify task priority function
    let task_priority_functions = [
        "NrPoints",
        "TaskAge",
        "Lod",
        "NrPointsWeighted1",
        "NrPointsWeighted2",
        "NrPointsWeighted3",
    ];
    for tpf in task_priority_functions {
        let config = Config {
            task_priority_function: tpf.to_string(),
            ..base_config.clone()
        };
        let results = evaluate(&config, &points, &coordinate_system, true, false);
        runs.entry("task_priority_function".to_string())
            .or_insert_with(Vec::new)
            .push(results);
    }
    for tpf in task_priority_functions {
        let config = Config {
            task_priority_function: tpf.to_string(),
            max_cache_size: 0,
            ..base_config.clone()
        };
        let results = evaluate(&config, &points, &coordinate_system, true, false);
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
        let results = evaluate(&config, &points, &coordinate_system, true, true);
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
        let results = evaluate(&config, &points, &coordinate_system, true, true);
        runs.entry("max_cache_size".to_string())
            .or_insert_with(Vec::new)
            .push(results);
    }

    // modify node size
    for max_node_size in [
        3_125, 6_250, 12_500, 25_000, 50_000, 100_000, 200_000, 400_000,
    ] {
        let config = Config {
            max_node_size,
            ..base_config.clone()
        };
        let results = evaluate(&config, &points, &coordinate_system, false, true);
        runs.entry("max_node_size".to_string())
            .or_insert_with(Vec::new)
            .push(results);
    }

    println!("{}", json!(runs))
}

fn evaluate_octree(
    config: &Config,
    points: &[Point],
    coordinate_system: &I32CoordinateSystem,
) -> serde_json::Value {
    // measure insertion rate
    reset_data_folder(config);
    processor_cooldown();
    info!("Measuring octree insertion rate...");
    let mut octree_index = create_octree_index(coordinate_system.clone(), config);
    let (octree_insertion_rate, max_pps) =
        measure_insertion_rate(&mut octree_index, points, config);
    info!("Results: {}", &octree_insertion_rate);

    // measure query performance
    processor_cooldown();
    info!("Measuring octree query perf...");
    let octree_query_perf = measure_query_performance(octree_index);
    info!("Results: {}", &octree_query_perf);

    // measure latency
    let octree_latency = if config.pps as f64 > max_pps {
        warn!(
            "Skipping latency measurement, because the indexer is too slow for {} points/sec.",
            config.pps
        );
        None as Option<serde_json::Value>
    } else {
        reset_data_folder(config);
        processor_cooldown();
        info!("Measuring octree latency...");
        let octree_latency = {
            let index = create_octree_index(coordinate_system.clone(), config);
            let query = preset_query_2();
            measure_latency(index, points, query, config)
        };
        info!("Results: {}", &octree_latency);
        Some(octree_latency)
    };

    json!({
        "latency": octree_latency,
        "insertion_rate": octree_insertion_rate,
        "query_performance": octree_query_perf
    })
}

fn evaluate_sensor_pos_index(
    config: &Config,
    points: &[Point],
    coordinate_system: &I32CoordinateSystem,
) -> serde_json::Value {
    // measure insertion rate
    reset_data_folder(config);
    processor_cooldown();
    info!("Measuring sensorpos insertion rate...");
    let mut sensor_pos_index = create_sensor_pos_index(coordinate_system.clone(), config);
    let (sensorpos_insertion_rate, max_pps) =
        measure_insertion_rate(&mut sensor_pos_index, points, config);
    info!("Results: {}", &sensorpos_insertion_rate);

    // measure query performance
    processor_cooldown();
    info!("Measuring sensorpos query perf...");
    let sensorpos_query_perf = measure_query_performance(sensor_pos_index);
    info!("Results: {}", &sensorpos_query_perf);

    // measure latency
    let sensorpos_latency = if config.pps as f64 > max_pps {
        warn!(
            "Skipping latency measurement, because the indexer is too slow for {} points/sec.",
            config.pps
        );
        None as Option<serde_json::Value>
    } else {
        reset_data_folder(config);
        processor_cooldown();
        info!("Measuring sensorpos latency...");
        let sensorpos_latency = {
            let index = create_sensor_pos_index(coordinate_system.clone(), config);
            let query = preset_query_2();
            measure_latency(index, points, query, config)
        };
        info!("Results: {}", &sensorpos_latency);
        Some(sensorpos_latency)
    };

    json!({
        "latency": sensorpos_latency,
        "insertion_rate": sensorpos_insertion_rate,
        "query_performance": sensorpos_query_perf
    })
}

fn evaluate(
    config: &Config,
    points: &[Point],
    coordinate_system: &I32CoordinateSystem,
    octree: bool,
    sensorpos: bool,
) -> serde_json::Value {
    info!("Configuration: {:#?}", config);

    let mut result = HashMap::new();
    if octree {
        let octree_results = evaluate_octree(config, points, coordinate_system);
        result.insert("octree_index".to_string(), octree_results);
    }
    if sensorpos {
        let sensorpos_results = evaluate_sensor_pos_index(config, points, coordinate_system);
        result.insert("sensor_pos_index".to_string(), sensorpos_results);
    }
    result.insert("config".to_string(), json!(config));

    json!(result)
}
