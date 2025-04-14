use anyhow::anyhow;
use chrono::{Local, Utc};
use clap::Parser;
use cli::EvaluationOptions;
use converter::{ConvertingPointReader, MissingAttributesStrategy};
use git_version::git_version;
use indexmap::IndexMap;
use indicatif::MultiProgress;
use indicatif_log_bridge::LogWrapper;
use insertion_rate::measure_insertion_rate;
use itertools::Itertools;
use latency::measure_latency;
use lidarserv_common::{
    geometry::{
        coordinate_system::CoordinateSystem,
        grid::{GridHierarchy, LodLevel},
    },
    index::{Octree, OctreeParams, attribute_index::config::IndexKind, reader::QueryConfig},
};
use lidarserv_server::index::query::Query;
use log::{debug, error, info, warn};
use nalgebra::vector;
use pasture_io::{
    base::{PointReader, SeekToPoint},
    las::LASReader,
};
use query_performance::measure_one_query;
use serde_json::{Value, json};
use settings::{
    Base, ElevationRun, EnabledAttributeIndexes, EvaluationScript, EvaluationSettings, MultiIndex,
    QueryFiltering, SingleIndex,
};
use simple_logger::SimpleLogger;
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, SeekFrom, Write},
    panic::catch_unwind,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::OnceLock,
    thread::sleep,
    time::Duration,
};
extern crate fs_extra;
use fs_extra::dir::get_size;

mod cli;
mod converter;
mod insertion_rate;
mod latency;
mod query_performance;
mod settings;

const VERSION: &str = git_version!(
    prefix = "git:",
    cargo_prefix = "cargo:",
    fallback = "unknown"
);

pub static MULTI_PROGRESS: OnceLock<MultiProgress> = OnceLock::new();

fn main() -> ExitCode {
    human_panic::setup_panic!();
    let args = EvaluationOptions::parse();
    let level = args.log_level.to_level_filter();
    let logger = SimpleLogger::new().with_level(level);
    let multi = MULTI_PROGRESS.get_or_init(MultiProgress::new).clone();
    LogWrapper::new(multi, logger).try_init().unwrap();

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
        let mut default_config = EvaluationScript::default();
        if !args.run.is_empty() {
            let mut runs_from_preset = default_config.runs.into_values();
            default_config.runs = IndexMap::new();
            for name in &args.run {
                let run = runs_from_preset
                    .next()
                    .unwrap_or_else(ElevationRun::default);
                default_config.runs.insert(name.clone(), run);
            }
        }
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

    // check if input file exists
    if !config.base.points_file_absolute().exists() {
        return Err(anyhow!(
            "Input pointcloud file {} does not exist.",
            config.base.points_file_absolute().display()
        ));
    }

    // check number of points
    let nr_points = {
        let input_file = LASReader::from_path(config.base.points_file_absolute(), true)?;
        input_file.remaining_points()
    };

    // create output file
    let out_file_name = get_output_filename(&config.base)?;
    let out_file = std::fs::File::create(&out_file_name)?;

    // determine which runs to execute
    let enabled_runs = if args.run.is_empty() {
        config.runs.iter().collect_vec()
    } else {
        let mut enabled_runs = Vec::new();
        for name in &args.run {
            if let Some(entry) = config.runs.get_key_value(name) {
                enabled_runs.push(entry)
            } else {
                return Err(anyhow!("The run '{name}' is not defined in the toml file."));
            }
        }
        enabled_runs
    };

    // run tests
    info!("Running tests");
    let started_at = Utc::now();
    let mut last_index = None;
    let mut all_results = HashMap::new();
    for (name, run) in enabled_runs {
        info!("=== {} ===", name);
        let mut run = run.clone();
        run.index.apply_defaults(&config.defaults.index);
        let settings = run.settings.apply_defaults(&config.defaults.settings);
        debug!("Applied defaults: {:?}", run);
        let mut run_results = Vec::new();
        let mut current_run = 1;
        for index in &run.index {
            info!("--- {name} run {current_run} ---");
            prettyprint_index_run(&run.index, &index);
            let result = match evaluate(&index, &config.base, settings.clone(), last_index.as_ref())
            {
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
            last_index = Some(index.clone());
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
    let input_file_str = match args.input_file.canonicalize() {
        Ok(canonical) => canonical.to_string_lossy().into_owned(),
        Err(_) => args.input_file.to_string_lossy().into_owned(),
    };
    let duration = (finished_at - started_at).num_seconds();
    let output = json!({
        "env": {
            "version": VERSION,
            "hostname": hostname,
            "config_file": input_file_str,
            "nr_points": nr_points,
            "nr_bytes": config.base.points_file_absolute().metadata()?.len(),
            "started_at": start_date,
            "finished_at": end_date,
            "duration:": duration,
        },
        "settings": config.base,
        "runs": all_results
    });
    info!("Writing results to: {}", out_file_name.display());
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

fn open_input_file(
    base_config: &Base,
) -> Result<impl PointReader + SeekToPoint + Send + use<>, anyhow::Error> {
    // open input file
    let raw_input_file = LASReader::from_path(base_config.points_file_absolute(), true)?;
    let trans = raw_input_file.header().transforms();
    let src_coordinate_system = CoordinateSystem::from_las_transform(
        vector![trans.x.scale, trans.y.scale, trans.z.scale],
        vector![trans.x.offset, trans.y.offset, trans.z.offset],
    );
    let point_layout = base_config.attributes.point_layout();
    let coordinate_system = base_config.coordinate_system;
    let input_file = ConvertingPointReader::new(
        raw_input_file,
        src_coordinate_system,
        point_layout.clone(),
        coordinate_system,
        MissingAttributesStrategy::ZeroInitializeAndWarn,
    )?;
    Ok(input_file)
}

fn create_index(index_config: &SingleIndex, base_config: &Base) -> Result<Octree, anyhow::Error> {
    let point_layout = base_config.attributes.point_layout();
    let coordinate_system = base_config.coordinate_system;
    let all_attribute_indexes = base_config.attribute_indexes();
    let attribute_indexes = match index_config.enable_attribute_index {
        EnabledAttributeIndexes::All => all_attribute_indexes,
        EnabledAttributeIndexes::RangeIndexOnly => all_attribute_indexes
            .into_iter()
            .filter(|idx| idx.index == IndexKind::RangeIndex)
            .collect(),
        EnabledAttributeIndexes::SfcIndexOnly => all_attribute_indexes
            .into_iter()
            .filter(|idx| matches!(idx.index, IndexKind::SfcIndex(_)))
            .collect(),
        EnabledAttributeIndexes::None => vec![],
    };
    if attribute_indexes.is_empty() {
        match index_config.enable_attribute_index {
            EnabledAttributeIndexes::All => {
                warn!("Attribute indexing is enabled, but no indexed attributes are configured.")
            }
            EnabledAttributeIndexes::RangeIndexOnly | EnabledAttributeIndexes::SfcIndexOnly => {
                warn!(
                    "Attribute indexing is enabled, but no indexed attributes are configured with the selected index type."
                )
            }
            EnabledAttributeIndexes::None => (),
        }
    }

    // Create index
    let index = Octree::new(OctreeParams {
        directory_file: base_config.index_folder_absolute().join("directory.bin"),
        point_data_folder: base_config.index_folder_absolute(),
        metrics_file: None,
        point_layout,
        node_hierarchy: GridHierarchy::new(index_config.node_hierarchy),
        point_hierarchy: GridHierarchy::new(index_config.point_hierarchy),
        coordinate_system,
        max_lod: LodLevel::from_level(index_config.max_lod),
        max_bogus_inner: index_config.nr_bogus_points.0,
        max_bogus_leaf: index_config.nr_bogus_points.1,
        enable_compression: index_config.compression,
        max_cache_size: index_config.cache_size,
        priority_function: index_config.priority_function,
        num_threads: index_config.num_threads,
        attribute_indexes,
    })?;

    Ok(index)
}

fn evaluate(
    index_config: &SingleIndex,
    base_config: &Base,
    settings: EvaluationSettings,
    last_index_config: Option<&SingleIndex>,
) -> Result<Value, anyhow::Error> {
    let unwind_result = catch_unwind(|| {
        // measure insertion rate
        let mut result_insertion_rate = serde_json::Value::Null;
        if settings.measure_index_speed {
            reset_data_folder(base_config)?;
            processor_cooldown(base_config);
            info!("Measuring insertion rate...");
            let mut index = create_index(index_config, base_config)?;
            let mut input_file = open_input_file(base_config)?;
            input_file.seek_point(SeekFrom::Start(0))?;
            let inner_result_insertion_rate = measure_insertion_rate(
                &mut index,
                &mut input_file,
                base_config.target_point_pressure,
                base_config
                    .indexing_timeout_seconds
                    .map(Duration::from_secs),
            )?;
            info!("Flush...");
            index.flush()?;
            info!("Results: {}", &inner_result_insertion_rate);
            result_insertion_rate = inner_result_insertion_rate;
        }

        // measure latency
        let mut result_latency = serde_json::Value::Null;
        if settings.measure_query_latency {
            if base_config.queries.is_empty() {
                warn!("Query latency measurements are enabled, but no queries are defined.");
            }
            let mut result_latency_inner = HashMap::new();
            for (query_name, query_str) in &base_config.queries {
                reset_data_folder(base_config)?;
                processor_cooldown(base_config);
                info!("Measuring query latency... [{query_name}]");
                let query = Query::parse(query_str)?;
                let query_config = QueryConfig {
                    enable_attribute_index: index_config.enable_point_filtering
                        != QueryFiltering::NodeFilteringWithoutAttributeIndex,
                    enable_point_filtering: index_config.enable_point_filtering
                        == QueryFiltering::PointFiltering,
                };
                let mut index = create_index(index_config, base_config)?;
                let mut input_file = open_input_file(base_config)?;
                input_file.seek_point(SeekFrom::Start(0))?;
                let result = measure_latency(
                    &mut index,
                    &mut input_file,
                    query,
                    query_config,
                    base_config.latency_replay_pps,
                    base_config.latency_sample_pps,
                )?;
                info!("Flush...");
                index.flush()?;
                info!("Results {query_name}: {}", &result);
                result_latency_inner.insert(query_name, result);
            }

            result_latency = serde_json::to_value(result_latency_inner)?;
        }

        // measure query performance
        let mut result_query_perf = serde_json::Value::Null;
        if settings.measure_query_speed {
            // (re) create the index if needed
            if !settings.measure_index_speed
                && (!settings.measure_query_latency || base_config.queries.is_empty())
            {
                if last_index_config.is_some_and(|l| !l.needs_reindexing(index_config)) {
                    info!("Keeping last index without reindexing.");
                } else {
                    info!("Recreating index before querying.");
                    reset_data_folder(base_config)?;
                    let mut index = create_index(index_config, base_config)?;
                    let mut input_file = open_input_file(base_config)?;
                    input_file.seek_point(SeekFrom::Start(0))?;
                    measure_insertion_rate(
                        // measure_insertion_rate is the fastest way to create the index,
                        // as it replays the data as fast as possible.
                        &mut index,
                        &mut input_file,
                        1_000_000,
                        None,
                    )?;
                    index.flush()?;
                }
            }

            // Here comes the actual query performance test
            let mut index = create_index(index_config, base_config)?;
            let mut query_perf_results = HashMap::new();
            for (query_name, query) in &base_config.queries {
                processor_cooldown(base_config);
                info!("Measuring query perf: {query_name}: {query}");
                let sensorpos_query_perf =
                    measure_one_query(&mut index, query, index_config.enable_point_filtering);
                query_perf_results.insert(query_name.clone(), sensorpos_query_perf);
            }
            let result = json!(query_perf_results);
            info!("Results: {}", &result);
            result_query_perf = result;
        }

        // measure index folder size
        let index_folder = base_config.base_folder.join(&base_config.index_folder);
        let index_folder_size = get_size(index_folder).unwrap();

        Ok(json!({
            //"index_info": index_info, // TODO
            "index_folder_size": index_folder_size,
            "latency": result_latency,
            "insertion_rate": result_insertion_rate,
            "query_performance": result_query_perf
        }))
    });

    match unwind_result {
        Ok(o) => o,
        Err(e) => match e.downcast_ref::<String>() {
            Some(e) => Err(anyhow!("Panick! ({e})")),
            _ => match e.downcast_ref::<&str>() {
                Some(e) => Err(anyhow!("Panick! ({e})")),
                _ => Err(anyhow!("Panick!")),
            },
        },
    }
}

pub fn prettyprint_index_run(multi: &MultiIndex, index: &SingleIndex) {
    macro_rules! prettyprint_index_run {
        ($($fields:ident),*) => {
            let SingleIndex {$($fields),*} = index;
            $(
                {
                    let cnt = multi.$fields.as_ref().map(|it| it.len()).unwrap_or(0);
                    if cnt > 1 {
                        log::info!("--- {} = {:?}", std::stringify!($fields), $fields);
                    }
                }
            )*
        };
    }
    prettyprint_index_run!(
        cache_size,
        priority_function,
        node_hierarchy,
        point_hierarchy,
        compression,
        num_threads,
        nr_bogus_points,
        max_lod,
        enable_attribute_index,
        enable_point_filtering
    );
}
