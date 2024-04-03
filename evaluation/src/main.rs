use evaluation::indexes::create_octree_index;
use evaluation::insertion_rate::measure_insertion_rate;
use evaluation::latency::measure_latency;
use evaluation::point::Point;
use evaluation::queries::aabb_full;
use evaluation::query_performance::measure_query_performance;
use evaluation::settings::{Base, EvaluationScript, MultiRun};
use evaluation::thermal_throttle::processor_cooldown;
use evaluation::{read_points, reset_data_folder};
use git_version::git_version;
use lidarserv_common::geometry::position::I32CoordinateSystem;
use lidarserv_common::index::octree::attribute_bounds::LasPointAttributeBounds;
use lidarserv_common::index::{Index, Query};
use log::{error, info, warn};
use nalgebra::Vector3;
use serde_json::json;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::OffsetDateTime;

const VERSION: &str = git_version!(
    prefix = "git:",
    cargo_prefix = "cargo:",
    fallback = "unknown"
);

fn main() {
    // init
    info!("LidarServ Evaluation Tool {}", VERSION);
    dotenv::dotenv().ok();
    pretty_env_logger::init();
    let started_at = OffsetDateTime::now_utc();

    // parse cli
    info!("Parsing CLI arguments");
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
    info!("Reading input file {}", input_file.to_string_lossy());
    let mut f = std::fs::File::open(&input_file).unwrap();
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

    // create output file
    let out_file_name = get_output_filename(&config.base.output_file_pattern);
    let out_file = match std::fs::File::create(out_file_name) {
        Ok(f) => f,
        Err(e) => {
            error!("Could not open output file for writing: {}", e);
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
    info!("Running tests");
    let mut all_results = HashMap::new();
    for (name, mut run) in config.runs.clone() {
        info!("=== {} ===", name);
        run.apply_defaults(&config.defaults);
        info!("Applied defaults: {:?}", run);
        let mut run_results = Vec::new();
        let mut current_run = 1;
        for index in &run.index {
            info!("Running index {}", current_run);
            let result = evaluate(
                &points,
                &run,
                &config.base,
                || create_octree_index(coordinate_system, &config.base, &index),
                config.base.enable_cooldown,
            );
            run_results.push(json!({
                "index": index,
                "results": result,
            }));
            current_run += 1;
        }
        all_results.insert(name, run_results);
    }

    // write results to file
    info!("Writing results to file");
    let hostname = gethostname::gethostname().to_string_lossy().into_owned();
    let start_date = started_at
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());
    let end_date = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());
    let input_file_str = input_file.to_string_lossy().into_owned();
    let output = json!({
        "env": {
            "version": VERSION,
            "hostname": hostname,
            "input_file": input_file_str,
            "input_file_nr_points": points.len(),
            "started_at": start_date,
            "finished_at": end_date,
            "duration:": (OffsetDateTime::now_utc() - started_at).whole_seconds(),

        },
        "runs": all_results
    });
    println!("{}", &output);
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
    enable_cooldown: bool,
) -> serde_json::Value
where
    I: Index<Point>,
    I::Reader: Send + 'static,
    F: Fn() -> I,
{
    // reset data folder if necessary
    if !base_config.use_existing_index {
        reset_data_folder(base_config);
    }

    // Create index
    let mut index = make_index();

    // measure insertion rate
    let mut max_pps = 0.0;
    let mut result_insertion_rate = serde_json::Value::Null;
    if !base_config.use_existing_index {
        if enable_cooldown {
            processor_cooldown()
        };
        info!("Measuring insertion rate...");
        let (inner_result_insertion_rate, inner_max_pps) = measure_insertion_rate(
            &mut index,
            points,
            &run.insertion_rate.single(),
            base_config.indexing_timeout_seconds,
        );
        info!("Results: {}", &inner_result_insertion_rate);
        result_insertion_rate = inner_result_insertion_rate;
        max_pps = inner_max_pps;
    }

    // store index info (e.g. number of nodes)
    let index_info = index.index_info();
    index.flush().unwrap();

    // measure query performance
    let result_query_perf = if run.query_perf.single().is_some() {
        if enable_cooldown {
            processor_cooldown()
        };
        info!("Measuring query perf...");
        let sensorpos_query_perf = measure_query_performance(index);
        info!("Results: {}", &sensorpos_query_perf);
        sensorpos_query_perf
    } else {
        drop(index);
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
        if enable_cooldown {
            processor_cooldown()
        };
        info!(
            "Measuring latency at {} points/sec...",
            measurement_settings.points_per_sec
        );
        let result_latency = {
            let index = make_index();
            let spatial_query = aabb_full();
            let query = Query {
                spatial: Box::new(spatial_query),
                attributes: LasPointAttributeBounds::new(),
                enable_attribute_acceleration: false,
                enable_histogram_acceleration: false,
                enable_point_filtering: false,
            };
            measure_latency(index, points, query, &measurement_settings)
        };
        results_latency.push(result_latency);
    }

    json!({
        "index_info": index_info,
        "latency": results_latency,
        "insertion_rate": result_insertion_rate,
        "query_performance": result_query_perf
    })
}
