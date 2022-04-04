use evaluation::indexes::{create_octree_index, create_sensor_pos_index};
use evaluation::insertion_rate::measure_insertion_rate;
use evaluation::latency::measure_latency;
use evaluation::point::Point;
use evaluation::queries::preset_query_2;
use evaluation::query_performance::measure_query_performance;
use evaluation::settings::{Base, EvaluationScript, MultiRun, SystemUnderTest};
use evaluation::thermal_throttle::processor_cooldown;
use evaluation::{read_points, reset_data_folder, settings};
use lidarserv_common::geometry::position::I32CoordinateSystem;
use lidarserv_common::index::Index;
use log::{error, info, warn};
use nalgebra::Vector3;
use serde_json::json;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use time::macros::format_description;
use time::OffsetDateTime;

fn main() {
    // init
    dotenv::dotenv().unwrap();
    pretty_env_logger::init();

    // parse cli
    let args: Vec<_> = std::env::args_os().collect();
    let input_file = match args.len().cmp(&2) {
        Ordering::Less => {
            error!("Missing argument 'input file'");
            return;
        }
        Ordering::Greater => {
            error!("Too many arguments. Expected a path to the input file as the only argument.");
            return;
        }
        Ordering::Equal => {
            let mut args = args;
            args.pop().unwrap()
        }
    };

    // read input file
    let mut f = std::fs::File::open(input_file).unwrap();
    let mut config_toml = String::new();
    f.read_to_string(&mut config_toml).unwrap();
    let config: EvaluationScript = match toml::from_str(&config_toml) {
        Ok(r) => r,
        Err(e) => {
            if let Some((row, col)) = e.line_col() {
                eprintln!("{}:{} - {}", row, col, e);
            } else {
                eprintln!("?:? - {}", e);
            }
            return;
        }
    };

    // read point data
    let coordinate_system = I32CoordinateSystem::from_las_transform(
        Vector3::new(0.001, 0.001, 0.001),
        Vector3::new(0.0, 0.0, 0.0),
    );
    let points = read_points(&coordinate_system, &config.base);

    // run tests
    let mut all_results = HashMap::new();
    for (name, mut run) in config.runs.clone() {
        info!("=== {} ===", name);
        run.apply_defaults(&config.defaults);
        let mut run_results = Vec::new();
        for index in &run.index {
            let result = match index.typ {
                SystemUnderTest::Octree => evaluate(&points, &run, &config.base, || {
                    create_octree_index(coordinate_system.clone(), &config.base, &index)
                }),
                SystemUnderTest::SensorPosTree => evaluate(&points, &run, &config.base, || {
                    create_sensor_pos_index(coordinate_system.clone(), &config.base, &index)
                }),
            };
            run_results.push(json!({
                "index": index,
                "results": result,
            }));
        }
        all_results.insert(name, run_results);
    }

    // write results to file
    let output = json!(all_results);
    println!("{}", &output);
    let out_file_name = get_output_filename(&config.base.output_file_pattern);
    let out_file = match std::fs::File::create(out_file_name) {
        Ok(f) => f,
        Err(e) => {
            error!("Could not open output file for writing: {}", e);
            return;
        }
    };
    match serde_json::to_writer_pretty(out_file, &output) {
        Ok(_) => (),
        Err(e) => error!("Could not write output file: {}", e),
    };
}

fn get_output_filename(pattern: &str) -> PathBuf {
    // replace date
    let now = match OffsetDateTime::now_local() {
        Ok(v) => v,
        Err(_) => OffsetDateTime::now_utc(),
    };
    let date_str = now
        .date()
        .format(&format_description!(
            "[year repr:full]-[month padding:zero repr:numerical]-[day padding:zero]"
        ))
        .unwrap_or_else(|_| "%d".into());
    let with_date = pattern.replace("%d", &date_str);

    // replace file number
    if !with_date.contains("%i") {
        return with_date.into();
    }
    for i in 1.. {
        let with_index = with_date.replace("%i", &i.to_string());
        let path = Path::new(&with_index);
        if !path.exists() {
            return with_index.into();
        }
    }
    panic!("No free file name found")
}

/*
fn run_simple(base_config: Config, points: Vec<Point>, coordinate_system: I32CoordinateSystem) {
    let mut runs = HashMap::new();

    // modify compression
    for compression in [false, true] {
        let config = Config {
            compression,
            ..base_config.clone()
        };
        let result = evaluate(&config, &points, &coordinate_system, true, true, false);
        runs.entry("compression".to_string())
            .or_insert_with(Vec::new)
            .push(result);
    }

    println!("{}", json!(runs));
}

fn run_full(base_config: Config, points: Vec<Point>, coordinate_system: I32CoordinateSystem) {
    let mut runs = HashMap::new();

    // "default run" with base settings
    {
        let result = evaluate(&base_config, &points, &coordinate_system, true, true, false);
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
        let results = evaluate(&config, &points, &coordinate_system, true, false, false);
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
        let results = evaluate(&config, &points, &coordinate_system, true, false, false);
        runs.entry("task_priority_function_low_cache".to_string())
            .or_insert_with(Vec::new)
            .push(results);
    }

    // modify threads
    for num_threads in [1, 2, 3, 4, 5, 6, 7, 8] {
        let config = Config {
            num_threads,
            ..base_config.clone()
        };
        let results = evaluate(&config, &points, &coordinate_system, true, true, false);
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
        let results = evaluate(&config, &points, &coordinate_system, true, true, false);
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
        let results = evaluate(&config, &points, &coordinate_system, false, true, false);
        runs.entry("max_node_size".to_string())
            .or_insert_with(Vec::new)
            .push(results);
    }

    println!("{}", json!(runs))
}*/

fn evaluate<I, F>(
    points: &[Point],
    run: &MultiRun,
    base_config: &Base,
    make_index: F,
) -> serde_json::Value
where
    I: Index<Point>,
    I::Reader: Send + 'static,
    F: Fn() -> I,
{
    // measure insertion rate
    let mut index = make_index();
    reset_data_folder(base_config);
    processor_cooldown();
    info!("Measuring insertion rate...");
    let (result_insertion_rate, max_pps) =
        measure_insertion_rate(&mut index, points, &run.insertion_rate.single());
    info!("Results: {}", &result_insertion_rate);

    // measure query performance
    let result_query_perf = if run.query_perf.single().is_some() {
        processor_cooldown();
        info!("Measuring query perf...");
        let sensorpos_query_perf = measure_query_performance(index);
        info!("Results: {}", &sensorpos_query_perf);
        sensorpos_query_perf
    } else {
        serde_json::Value::Null
    };

    // measure latency
    let mut results_latency = vec![];
    for measurement_settings in &run.latency {
        if measurement_settings.points_per_sec as f64 > max_pps {
            warn!(
                "Skipping latency measurement with {} points/sec, because the indexer is too slow (only reached {} points/sec).",
                measurement_settings.points_per_sec, max_pps
            );
            continue;
        }
        reset_data_folder(base_config);
        processor_cooldown();
        info!(
            "Measuring latency at {} points/sec...",
            measurement_settings.points_per_sec
        );
        let result_latency = {
            let index = make_index();
            let query = preset_query_2();
            measure_latency(index, points, query, &measurement_settings)
        };
        results_latency.push(result_latency);
    }

    json!({
        "latency": results_latency,
        "insertion_rate": result_insertion_rate,
        "query_performance": result_query_perf
    })
}
