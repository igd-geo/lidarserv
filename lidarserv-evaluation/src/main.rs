use std::{
    collections::HashMap,
    fs::File,
    io::{Read, SeekFrom, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    thread::sleep,
    time::Duration,
};

use anyhow::anyhow;
use chrono::{Local, Utc};
use clap::Parser;
use cli::EvaluationOptions;
use git_version::git_version;
use insertion_rate::measure_insertion_rate;
use lidarserv_common::{
    geometry::{
        coordinate_system::CoordinateSystem,
        grid::{GridHierarchy, LodLevel},
    },
    index::{Octree, OctreeParams},
};
use log::{debug, error, info};
use nalgebra::vector;
use pasture_core::layout::{attributes::POSITION_3D, PointAttributeMember, PointLayout};
use pasture_io::{
    base::{PointReader, SeekToPoint},
    las::{LASReader, ATTRIBUTE_LOCAL_LAS_POSITION},
};
use query_performance::measure_one_query;
use serde_json::{json, Value};
use settings::{Base, EvaluationScript, MultiRun, SingleIndex};

mod cli;
mod insertion_rate;
mod query_performance;
mod settings;

const VERSION: &str = git_version!(
    prefix = "git:",
    cargo_prefix = "cargo:",
    fallback = "unknown"
);

fn main() -> ExitCode {
    human_panic::setup_panic!();
    let args = EvaluationOptions::parse();
    simple_logger::init_with_level(args.log_level).expect("Failed to initialize logger.");
    match main_result(args) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            debug!("{e:?}");
            ExitCode::FAILURE
        }
    }
}

fn get_output_filename(base_config: &Base) -> Result<PathBuf, anyhow::Error> {
    // replace date
    let date = Local::now().date_naive();

    if !base_config.is_output_filename_indexed() {
        let path = base_config.output_file_absolute(date, 0);
        if path.exists() {
            return Err(anyhow!("Output file {} already exists.", path.display()));
        } else {
            return Ok(path);
        }
    }
    for i in 1.. {
        let path = base_config.output_file_absolute(date, i);
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(anyhow!("No free file name found"))
}

fn main_result(args: EvaluationOptions) -> Result<(), anyhow::Error> {
    // create default config file if it does not exist
    if !args.input_file.exists() {
        info!("Config file does not exist.");
        info!(
            "Creating example config file at: {}",
            args.input_file.display()
        );
        let default_config = EvaluationScript::default();
        let mut file = File::create_new(&args.input_file)?;
        file.write_all(toml::to_string_pretty(&default_config)?.as_bytes())?;
        return Ok(());
    }

    // read config file
    let mut f = std::fs::File::open(&args.input_file).unwrap();
    let mut config_toml = String::new();
    f.read_to_string(&mut config_toml).unwrap();
    let mut config: EvaluationScript = toml::from_str(&config_toml)?;
    config.base.base_folder = args.input_file.parent().unwrap_or(Path::new("")).to_owned();

    // check number of points
    let nr_points = {
        let input_file = LASReader::from_path(config.base.points_file_absolute(), true)?;
        input_file.remaining_points()
    };

    // create output file
    let out_file_name = get_output_filename(&config.base)?;
    let out_file = std::fs::File::create(out_file_name)?;

    // run tests
    info!("Running tests");
    let started_at = Utc::now();
    let mut all_results = HashMap::new();
    for (name, run) in &config.runs {
        info!("=== {} ===", name);
        let mut run = run.clone();
        run.apply_defaults(&config.defaults);
        debug!("Applied defaults: {:?}", run);
        let mut run_results = Vec::new();
        let mut current_run = 1;
        for index in &run.index {
            info!("Running index {}", current_run);
            let result = match evaluate(&index, &run, &config.base) {
                Ok(o) => o,
                Err(e) => {
                    error!("Evaluation run finished with an error: {e}");
                    debug!("{e:#?}");
                    json!({
                        "error": format!("{e}"),
                        "details": format!("{e:#?}"),
                    })
                }
            };
            run_results.push(json!({
                "index": index,
                "results": result,
            }));
            current_run += 1;
        }
        all_results.insert(name, run_results);
    }
    let finished_at = Utc::now();

    // write results to file
    info!("Writing results to file");
    let hostname = gethostname::gethostname().to_string_lossy().into_owned();
    let start_date = started_at.to_rfc3339();
    let end_date = finished_at.to_rfc3339();
    let input_file_str = args.input_file.to_string_lossy();
    let duration = (finished_at - started_at).num_seconds();
    let output = json!({
        "env": {
            "version": VERSION,
            "hostname": hostname,
            "config_file": input_file_str,
            "nr_points": nr_points,
            "started_at": start_date,
            "finished_at": end_date,
            "duration:": duration,
        },
        "runs": all_results
    });
    println!("{}", &output);
    match serde_json::to_writer_pretty(out_file, &output) {
        Ok(_) => (),
        Err(e) => error!("Could not write output file: {}", e),
    };

    Ok(())
}

pub fn reset_data_folder(settings: &Base) -> Result<(), anyhow::Error> {
    info!("Resetting data folder...");
    let data_folder = settings.index_folder_absolute();
    if data_folder.exists() {
        std::fs::remove_dir_all(&data_folder)?;
    }
    std::fs::create_dir_all(&data_folder)?;
    Ok(())
}

pub fn processor_cooldown(base_config: &Base) {
    if base_config.cooldown_seconds > 0 {
        info!("Processor cooldown: {}s", base_config.cooldown_seconds);
        sleep(Duration::from_secs(base_config.cooldown_seconds));
    }
}

fn evaluate(
    index_config: &SingleIndex,
    run: &MultiRun,
    base_config: &Base,
) -> Result<Value, anyhow::Error> {
    // reset data folder if necessary
    if !base_config.use_existing_index {
        reset_data_folder(base_config)?;
    }

    // open input file
    let mut input_file = LASReader::from_path(base_config.points_file_absolute(), true)?;

    // point layout
    // freely definable layout and proper layout conversion
    let attributes = input_file
        .get_default_point_layout()
        .attributes()
        .cloned()
        .map(|attr| {
            if attr.name() == ATTRIBUTE_LOCAL_LAS_POSITION.name() {
                PointAttributeMember::custom(POSITION_3D.name(), attr.datatype(), attr.offset())
            } else {
                attr
            }
        })
        .collect::<Vec<_>>();
    let point_layout = PointLayout::from_members_and_alignment(&attributes, 1);

    // Create index
    let mut index = Octree::new(OctreeParams {
        directory_file: base_config.index_folder_absolute().join("directory.bin"),
        point_data_folder: base_config.index_folder_absolute(),
        metrics_file: None,
        point_layout,
        node_hierarchy: GridHierarchy::new(index_config.node_hierarchy),
        point_hierarchy: GridHierarchy::new(index_config.point_hierarchy),
        coordinate_system: {
            let t = input_file.header().transforms();
            CoordinateSystem::from_las_transform(
                vector![t.x.scale, t.y.scale, t.z.scale],
                vector![t.x.offset, t.y.offset, t.z.offset],
            )
        },
        max_lod: LodLevel::from_level(index_config.max_lod),
        max_bogus_inner: index_config.nr_bogus_points.0,
        max_bogus_leaf: index_config.nr_bogus_points.1,
        enable_compression: index_config.compression,
        max_cache_size: index_config.cache_size,
        priority_function: index_config.priority_function,
        num_threads: index_config.num_threads,
    })?;

    // measure insertion rate
    let mut result_insertion_rate = serde_json::Value::Null;
    if !base_config.use_existing_index {
        processor_cooldown(base_config);
        info!("Measuring insertion rate...");
        input_file.seek_point(SeekFrom::Start(0))?;
        let inner_result_insertion_rate = measure_insertion_rate(
            &mut index,
            &mut input_file,
            &run.insertion_rate.single(),
            base_config.indexing_timeout_seconds,
        )?;
        info!("Results: {}", &inner_result_insertion_rate);
        result_insertion_rate = inner_result_insertion_rate;
    }

    // measure query performance
    let mut query_perf_results = HashMap::new();
    for (query_name, query) in run.query_perf.queries() {
        processor_cooldown(base_config);
        info!("Measuring query perf: {query_name}: {query}");
        let sensorpos_query_perf = measure_one_query(&mut index, query);
        query_perf_results.insert(query_name.clone(), sensorpos_query_perf);
    }
    let result_query_perf = if !query_perf_results.is_empty() {
        let result = json!(query_perf_results);
        info!("Results: {}", &result);
        result
    } else {
        drop(index);
        serde_json::Value::Null
    };

    Ok(json!({
        //"index_info": index_info, // TODO
        //"latency": results_latency,   // TODO
        "insertion_rate": result_insertion_rate,
        "query_performance": result_query_perf
    }))
}
