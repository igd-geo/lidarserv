use std::fmt::Write;
use std::fs::create_dir;
use std::time::Instant;
use anyhow::Result;
use clap::{Parser};
use statrs::statistics::{Data, Median, Statistics};
use tokio_postgres::{Client};
use log::{debug, info};
use serde_json::json;
use chrono::Utc;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use log::Level::Debug;
use measurements::db::*;

#[tokio::main]
async fn main() -> Result<()> {
    simple_logger::init_with_level(Debug)?;
    let start_date = Utc::now();

    let args = Args::parse();
    let input_file = args.input_file;
    let input_file_abs = std::fs::canonicalize(&input_file)?;
    let base_folder = input_file_abs.parent().unwrap();
    let iterations = args.iterations as usize;

    // check, if input file exists
    if !std::path::Path::new(&input_file).exists() {
        panic!("Input file {} does not exist", input_file);
    }

    let table = std::path::Path::new(&input_file).file_stem().unwrap().to_str().unwrap().to_lowercase();

    // connect to db
    let postgis_config = PostGISConfig {
        database: "pointclouds".parse()?,
        host: "localhost".parse()?,
        password: "password".parse()?,
        username: "postgres".parse()?
    };
    let (client, _join_handle) = connect_to_db(&postgis_config).await?;
    info!("Existing tables: {:?}", list_tables(&client).await?);

    // open json output file
    let filename = base_folder.join(format!("pg_query_results_{}_{}.json", &table, start_date.to_rfc3339()));
    if !filename.parent().unwrap().exists() {
        debug!("Creating output file folder");
        create_dir(filename.parent().unwrap())?;
    }
    let output_file = std::fs::File::create(filename)?;
    let mut output_writer = std::io::BufWriter::new(output_file);
    let mut json_query_result = json!({});

    // Progress bar
    let pb = ProgressBar::new(9*4*(iterations as u64));
    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>7}/{len:7} [{msg}] ({eta})")
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
        .progress_chars("#>-"));
    // run queries

    info!("Running queries on dataset {}", &table);
    let queries_ahn4 = vec![
        ("intensity_high","pc_patchmin(pa, 'Intensity') >= 1400","PC_FilterBetween(pa, 'Intensity', 1400, 1000000)"),
        ("intensity_low","pc_patchmax(pa, 'Intensity') <= 20","PC_FilterBetween(pa, 'Intensity', 0, 20)"),
        ("return_simple","pc_patchmin(pa, 'NumberOfReturns') >= 1 AND pc_patchmax(pa, 'NumberOfReturns') <= 1","PC_FilterEquals(pa, 'NumberOfReturns', 1)"),
        ("return_multiple","pc_patchmin(pa, 'NumberOfReturns') >= 2","PC_FilterBetween(pa, 'NumberOfReturns', 2, 100)"),
        ("classification_ground","pc_patchmin(pa, 'Classification') >= 2 AND pc_patchmax(pa, 'Classification') <= 2","PC_FilterEquals(pa, 'Classification', 2)"),
        ("classification_building","pc_patchmin(pa, 'Classification') >= 6 AND pc_patchmax(pa, 'Classification') <= 6","PC_FilterEquals(pa, 'Classification', 6)"),
        ("classification_vegetation","pc_patchmin(pa, 'Classification') >= 1 AND pc_patchmax(pa, 'Classification') <= 1","PC_FilterEquals(pa, 'Classification', 1)"),
        ("classification_bridges","pc_patchmin(pa, 'Classification') >= 26 AND pc_patchmax(pa, 'Classification') <= 26","PC_FilterEquals(pa, 'Classification', 26)"),
        ("time1","pc_patchmax(pa, 'GpsTime') < 270521185","PC_FilterBetween(pa, 'GpsTime', 0, 270521185)"),
        ("time2","pc_patchmin(pa, 'GpsTime') >= 270204590 AND pc_patchmax(pa, 'GpsTime') <= 270204900","PC_FilterBetween(pa, 'GpsTime', 270204590, 270204900)"),
        ("time3","pc_patchmin(pa, 'GpsTime') >= 269521185 AND pc_patchmax(pa, 'GpsTime') <= 269522000","PC_FilterBetween(pa, 'GpsTime', 269521185, 269522000)"),
    ];
    let queries_lille = vec![
        ("intensity_high", "pc_patchmin(pa, 'Intensity') > 128","PC_FilterBetween(pa, 'Intensity', 129, 1000000)"),
        ("intensity_low", "pc_patchmax(pa, 'Intensity') <= 2","PC_FilterBetween(pa, 'Intensity', 0, 2)"),
        ("time1", "pc_patchmin(pa, 'GpsTime') >= 4983","PC_FilterBetween(pa, 'GpsTime', 4983, 10000000)"),
        ("time2", "pc_patchmin(pa, 'GpsTime') >= 9120 AND pc_patchmax(pa, 'GpsTime') <= 9158","PC_FilterBetween(pa, 'GpsTime', 9120, 9158)"),
    ];
    let queries_kitti = vec![
        ("classification_ground", "pc_patchmax(pa, 'semantic') <= 12","PC_FilterBetween(pa, 'semantic', 0, 12)"),
        ("classification_building", "pc_patchmin(pa, 'semantic') >= 11 AND pc_patchmax(pa, 'semantic') <= 11","PC_FilterEquals(pa, 'semantic', 11)"),
        ("pointsource1", "pc_patchmin(pa, 'PointSourceID') >= 35 AND pc_patchmax(pa, 'PointSourceID') <= 64","PC_FilterBetween(pa, 'PointSourceID', 35, 64)"),
        ("pointsource2", "pc_patchmin(pa, 'PointSourceID') >= 208 AND pc_patchmax(pa, 'PointSourceID') <= 248","PC_FilterBetween(pa, 'PointSourceID', 208, 248)"),
        ("time1", "pc_patchmin(pa, 'GpsTime') >= 199083995.09382153 AND pc_patchmax(pa, 'GpsTime') <= 466372692.21052635","PC_FilterBetween(pa, 'GpsTime', 199083995.09382153, 466372692.21052635)"),
        ("time2", "pc_patchmin(pa, 'GpsTime') >= 687577131.20366132 AND pc_patchmax(pa, 'GpsTime') <= 805552832.00000000","PC_FilterBetween(pa, 'GpsTime', 687577131.20366132, 805552832.00000000)"),
        ("visible", "pc_patchmax(pa, 'visible') <= 1","PC_FilterBetween(pa, 'visible', 0, 1)"),
        ("rgb", "pc_patchmax(pa, 'red') <= 10","PC_FilterBetween(pa, 'red', 0, 10)"), // todo change to rgb
    ];
    let queries_hamburg = vec![
        // todo
    ];

    let filename = input_file.split("/").last().unwrap();
    let selected_query_set = match filename {
        "AHN4.las" => queries_ahn4,
        "Lille_sorted.las" => queries_lille,
        "kitti_sorted.las" => queries_kitti,
        "Hamburg.las" => queries_hamburg,
        _ => panic!("No queries defined for dataset {}", input_file),
    };
    info!("Running queries on dataset {}", input_file);

    for (name, query_patch, query_point) in selected_query_set {
        json_query_result[name] = run_query(&pb, query_patch, query_point, &table, &client, iterations).await;
    }

    pb.finish();

    // write json to output file
    // write results to file
    info!("Writing results to file");
    let num_points_query_str = format!("SELECT Sum(PC_NumPoints(pa)) FROM {};", &table);
    let num_points_query_result = client.query(num_points_query_str.as_str(), &[]).await?;
    let num_points : i64 = num_points_query_result[0].get(0);
    let num_bytes = std::fs::metadata(input_file.clone())?.len();
    let num_patches_query_str = format!("SELECT Count(*) FROM {};", &table);
    let num_patches_query_result = client.query(num_patches_query_str.as_str(), &[]).await?;
    let num_patches : i64 = num_patches_query_result[0].get(0);
    let hostname = gethostname::gethostname().to_string_lossy().into_owned();
    let end_date = Utc::now();
    let output = json!({
        "env": {
            "hostname": hostname,
            "table": &table,
            "num_patches": num_patches,
            "num_points": num_points,
            "num_bytes": num_bytes,
            "iterations": iterations,
            "started_at": start_date.to_rfc3339(),
            "finished_at": end_date.to_rfc3339(),
            "duration:": (end_date - start_date).num_seconds(),
        },
        "results": json_query_result,
    });
    serde_json::to_writer_pretty(&mut output_writer, &output)?;

    if args.drop_table {
        drop_table(&client, &table).await?;
    }

    Ok(())
}

async fn run_query(
    pb: &ProgressBar,
    patch_query: &str,
    point_query: &str,
    dataset: &str,
    client: &Client,
    iterations: usize,
) -> serde_json::Value {
    info!("[QUERY] Running query {:?} with {:?} iterations", point_query, iterations);

    // get total patch count
    let total_patch_count_query = format!("SELECT Count(*) FROM {};", dataset);
    let total_patch_count: i64 = match client.query(total_patch_count_query.as_str(), &[]).await {
        Ok(rows) => rows[0].get::<_, Option<i64>>(0).unwrap_or_else(|| 0),
        Err(e) => {
            let error_json = json!({ "error": format!("Failed to get total patch count: {}", e) });
            return error_json;
        }
    };

    // get node count after patch filtering
    let patch_filter_patch_count_query = format!("SELECT Count(*) FROM {} WHERE {};", dataset, patch_query);
    let filtered_patch_count: i64 = match client.query(patch_filter_patch_count_query.as_str(), &[]).await {
        Ok(rows) => rows[0].get::<_, Option<i64>>(0).unwrap_or_else(|| 0),
        Err(e) => {
            let error_json = json!({ "error": format!("Failed to get filtered patch count: {}", e) });
            return error_json;
        }
    };

    let queries = vec![
        ("raw_spatial", format!("SELECT pc_astext(pc_explode(pa)) FROM {};", dataset)),
        ("only_node_acc", format!("SELECT PC_Uncompress(pa) FROM {} WHERE {};", dataset, patch_query)),
        ("raw_point_filtering", format!("SELECT PC_Uncompress({}) FROM {};", point_query, dataset)),
        ("point_filtering_with_node_acc", format!("SELECT PC_Uncompress({}) FROM {} WHERE {}", point_query, dataset, patch_query)),
    ];

    let mut json = json!({});
    for (name, query) in queries {
        let mut timings = vec![];
        let mut num_rows = 0;
        for _ in 0..iterations {
            info!("Sending {}", name);
            let t_start = Instant::now();
            match client.query(&query, &[]).await {
                Ok(rows) => {
                    num_rows = rows.len();
                    timings.push(t_start.elapsed().as_secs_f64());
                    pb.inc(1);
                }
                Err(e) => {
                    let error_json = json!({ "error": format!("Failed to execute query {}: {}", name, e) });
                    json[name] = error_json;
                }
            }
        }
        let median = Data::new(timings.clone()).median();
        let stddev = (&timings).std_dev();
        let mean = (&timings).mean();
        json[name] = json!({
            "median": median,
            "stddev": stddev,
            "mean": mean,
            "num_points": num_rows,
            "num_patches": filtered_patch_count,
            "pps": num_rows as f64 / mean,
            "total_patches": total_patch_count,
        });
    }

    json
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    input_file: String,

    #[arg(long, default_value_t = 1)]
    iterations: u8,

    #[arg(long)]
    drop_table: bool,
}