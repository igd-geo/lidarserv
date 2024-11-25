use std::fs::File;
use std::fmt::Write;
use std::io::Write as IoWrite;
use std::ptr::null;
use std::time::Instant;
use anyhow::Result;
use clap::{App, Arg};
use las::{Read, Reader};
use statrs::statistics::{Data, Median, Statistics};
use tokio::task::JoinHandle;
use tokio_postgres::{Client, NoTls};
use log::{debug, error, info, trace, warn};
use serde_json::json;
use chrono::Utc;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use measurements::attribute_bounds::LasPointAttributeBounds;
use measurements::queries::*;
use measurements::db::*;

async fn run_query(
    pb: &ProgressBar,
    patch_query: &str,
    point_query: &str,
    dataset: &str,
    client: &Client,
    iterations: usize,
) -> serde_json::Value {
    info!("Running query {:?} with {:?} iterations", point_query, iterations);

    // let patch_filter = format!("SELECT pc_astext(PC_Explode(pa)) FROM {} WHERE {};", dataset, patch_query);
    // let patch_filter_patch_count = format!("SELECT Count(*) FROM {} WHERE {};", dataset, patch_query);
    // let total_patch_count = format!("SELECT Count(*) FROM {};", dataset);
    // let point_filter = format!("SELECT pc_astext(PC_Explode({})) FROM {};", point_query, dataset);
    // let patch_and_point_filter = format!("SELECT pc_astext(PC_Explode({})) FROM {} WHERE {}", point_query, dataset, patch_query);

    // pc_uncompress(pa) instead of pc_astext(pc_explode(pa))
    let patch_filter = format!("SELECT PC_Uncompress(pa)) FROM {} WHERE {};", dataset, patch_query);
    let patch_filter_patch_count = format!("SELECT Count(*) FROM {} WHERE {};", dataset, patch_query);
    let total_patch_count = format!("SELECT Count(*) FROM {};", dataset);
    let point_filter = format!("SELECT PC_Uncompress({})) FROM {};", point_query, dataset);
    let patch_and_point_filter = format!("SELECT PC_Uncompress({})) FROM {} WHERE {}", point_query, dataset, patch_query);

    // load all timing
    let mut timings = vec![];
    let mut num_rows = 0;
    for _ in 0..iterations {
        let t_start = Instant::now();
        let load_query = format!("SELECT pc_astext(pc_explode(pa)) FROM {};", dataset);
        let rows = client.query(&load_query, &[]).await.unwrap();
        num_rows = rows.len();
        timings.push(t_start.elapsed().as_secs_f64());
        pb.inc(1);
    }
    let load_median = Data::new(timings.clone()).median();
    let load_stddev = (&timings).std_dev();
    let load_mean = (&timings).mean();
    // get total patch count
    let rows = client.query(total_patch_count.as_str(), &[]).await.unwrap();
    info!("test {:?}", rows);
    let total_patch_count: i64 = rows[0].get::<_, Option<i64>>(0).unwrap_or_else(|| 0);

    // patch filtering measurements
    let mut timings = vec![];
    let mut patch_num_rows = 0;
    for _ in 0..iterations {
        let t_start = Instant::now();
        debug!("Running patch filter: {:?}", patch_filter);
        let rows = client.query(&patch_filter, &[]).await.unwrap();
        patch_num_rows = rows.len();
        timings.push(t_start.elapsed().as_secs_f64());
        pb.inc(1);
    }
    let patch_median = Data::new(timings.clone()).median();
    let patch_stddev = (&timings).std_dev();
    let patch_mean = (&timings).mean();
    // get node count after patch filtering
    let rows = client.query(patch_filter_patch_count.as_str(), &[]).await.unwrap();
    let filtered_patch_count: i64 = rows[0].get::<_, Option<i64>>(0).unwrap_or_else(|| 0);


    // point filtering measurements
    timings.clear();
    let mut point_num_rows = 0;
    for _ in 0..iterations {
        let t_start = Instant::now();
        debug!("Running point filter: {:?}", point_filter);
        let rows = client.query(&point_filter, &[]).await.unwrap();
        point_num_rows = rows.len();
        timings.push(t_start.elapsed().as_secs_f64());
        pb.inc(1);
    }
    let point_median = Data::new(timings.clone()).median();
    let point_stddev = (&timings).std_dev();
    let point_mean = (&timings).mean();

    // patch and point filtering measurements
    timings.clear();
    let mut point_patch_num_rows = 0;
    for _ in 0..iterations {
        let t_start = Instant::now();
        debug!("Running patch and point filter: {:?}", patch_and_point_filter);
        let rows = client.query(&patch_and_point_filter, &[]).await.unwrap();
        point_patch_num_rows = rows.len();
        timings.push(t_start.elapsed().as_secs_f64());
        pb.inc(1);
    }
    let point_patch_median = Data::new(timings.clone()).median();
    let point_patch_stddev = (&timings).std_dev();
    let point_patch_mean = (&timings).mean();

    json!({
        "raw_spatial": {
            "median": load_median,
            "stddev": load_stddev,
            "mean": load_mean,
            "num_points": num_rows,
            "num_patches": total_patch_count,
            "pps": num_rows as f64 / load_mean,
        },
        "only_node_acc": {
            "median": patch_median,
            "stddev": patch_stddev,
            "mean": patch_mean,
            "num_points": patch_num_rows,
            "num_patches": filtered_patch_count,
            "pps": patch_num_rows as f64 / patch_mean,
        },
        "point_filtering_with_node_acc": {
            "median": point_patch_median,
            "stddev": point_patch_stddev,
            "mean": point_patch_mean,
            "num_points": point_patch_num_rows,
            "num_patches": filtered_patch_count,
            "pps": point_patch_num_rows as f64 / point_patch_mean,
        },
        "raw_point_filtering": {
            "median": point_median,
            "stddev": point_stddev,
            "mean": point_mean,
            "num_points": point_num_rows,
            "num_patches": filtered_patch_count,
            "pps": point_num_rows as f64 / point_mean,
        },
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    simple_logger::init_with_level(log::Level::Debug).unwrap();
    let start_date = Utc::now();

    // config
    let table = "ahn4_200t";
    let input_file = "data/ahn4_200t.las";
    let iterations : usize = 1;

    // connect to db
    let postgis_config = PostGISConfig {
        database: "pointclouds".parse()?,
        host: "localhost".parse()?,
        password: "password".parse()?,
        username: "postgres".parse()?
    };
    let (client, _join_handle) = connect_to_db(&postgis_config).await?;

    // open json output file
    let filename = format!("results/results_{}_{}.json", table, start_date.to_rfc3339());
    let output_file = std::fs::File::create(filename)?;
    let mut output_writer = std::io::BufWriter::new(output_file);
    let mut json_output : serde_json::Value = json!({});
    let mut json_query_result = json!({});

    // Progress bar
    let pb = ProgressBar::new(9*4*(iterations as u64));
    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>7}/{len:7} [{msg}] ({eta})")
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
        .progress_chars("#>-"));
    // run queries
    info!("Running queries on dataset {}", table);
    json_query_result["time_range"] = run_query(&pb, time_range_patchwise(), time_range_pointwise(), table, &client, iterations).await;
    json_query_result["ground_classification"] = run_query(&pb, ground_classification_patchwise(), ground_classification_pointwise(), table, &client, iterations).await;
    json_query_result["building_classification"] = run_query(&pb, building_classification_patchwise(), building_classification_pointwise(), table, &client, iterations).await;
    json_query_result["vegetation_classification"] = run_query(&pb, vegetation_classification_patchwise(), vegetation_classification_pointwise(), table, &client, iterations).await;
    json_query_result["high_intensity"] = run_query(&pb, high_intensity_patchwise(), high_intensity_pointwise(), table, &client, iterations).await;
    json_query_result["low_intensity"] = run_query(&pb, low_intensity_patchwise(), low_intensity_pointwise(), table, &client, iterations).await;
    json_query_result["normal_x_vertical"] = run_query(&pb, normal_x_vertical_patchwise(), normal_x_vertical_pointwise(), table, &client, iterations).await;
    json_query_result["one_return"] = run_query(&pb, one_return_patchwise(), one_return_pointwise(), table, &client, iterations).await;
    json_query_result["mixed_ground_and_time"] = run_query(&pb, mixed_ground_and_time_patchwise(), mixed_ground_and_time_pointwise(), table, &client, iterations).await;
    pb.finish();

    // write json to output file
    // write results to file
    info!("Writing results to file");
    let num_points_query_str = format!("SELECT Sum(PC_NumPoints(pa)) FROM {};", table);
    let num_points_query_result = client.query(num_points_query_str.as_str(), &[]).await?;
    let num_points : i64 = num_points_query_result[0].get(0);
    let num_patches_query_str = format!("SELECT Count(*) FROM {};", table);
    let num_patches_query_result = client.query(num_patches_query_str.as_str(), &[]).await?;
    let num_patches : i64 = num_patches_query_result[0].get(0);
    let hostname = gethostname::gethostname().to_string_lossy().into_owned();
    let end_date = Utc::now();
    let output = json!({
        "env": {
            "hostname": hostname,
            "table": table,
            "num_patches": num_patches,
            "num_points": num_points,
            "iterations": iterations,
            "started_at": start_date.to_rfc3339(),
            "finished_at": end_date.to_rfc3339(),
            "duration:": (end_date - start_date).num_seconds(),
        },
        "results": json_query_result,
    });
    serde_json::to_writer_pretty(&mut output_writer, &output)?;

    Ok(())
}