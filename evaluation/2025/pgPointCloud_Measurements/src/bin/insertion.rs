use std::error::Error;
use std::io::Write;
use async_process::Command;
use chrono::Utc;
use clap::Parser;
use log::{debug, info};
use log::Level::Debug;
use serde_json::json;
use measurements::db::{connect_to_db, drop_table, list_tables, PostGISConfig};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    input_file: String,

    // dimensional, none, lazperf
    #[arg(short, long)]
    compression: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    simple_logger::init_with_level(Debug)?;
    let args = Args::parse();

    let input_file = &args.input_file;
    if !std::path::Path::new(&input_file).exists() {
        panic!("Input file {} does not exist", input_file);
    }
    let abs_input_file = std::fs::canonicalize(&input_file)?;
    debug!("Absolute input file path is {:?}", abs_input_file);
    let base_dir = abs_input_file.parent().unwrap();
    debug!("Base directory is {:?}", base_dir);
    let table = std::path::Path::new(&input_file).file_stem().unwrap().to_str().unwrap().to_lowercase();
    let iterations: usize = 1;
    info!("Running insertion with input file {} and table {}", input_file, table);

    // connect to db
    let postgis_config = PostGISConfig {
        database: "pointclouds".parse()?,
        host: "localhost".parse()?,
        password: "password".parse()?,
        username: "postgres".parse()?
    };
    let (client, _join_handle) = connect_to_db(&postgis_config).await?;
    let abs_input_file_string = abs_input_file.to_str().unwrap();
    let compression = &args.compression;
    let pipeline_json = format!(
        r#"
{{
    "pipeline": [
        {{
            "type": "readers.las",
            "filename": "{abs_input_file_string}"
        }},
        {{
            "type": "filters.chipper",
            "capacity": "400"
        }},
        {{
            "type": "writers.pgpointcloud",
            "connection": "host='localhost' dbname='pointclouds' user='postgres' password='password' port='5432'",
            "table": "{table}",
            "compression": "{compression}"
        }}
    ]
}}
        "#
    );
    // write pipeline to json file
    std::fs::create_dir_all(base_dir.join("pipelines"))?;
    let pipeline_filename = base_dir.join("pipelines").join(format!("pipeline_{}.json", table));
    debug!("Writing pipeline to file {}", pipeline_filename.to_str().unwrap());
    let pipeline_file = std::fs::File::create(&pipeline_filename)?;
    let mut pipeline_writer = std::io::BufWriter::new(pipeline_file);
    pipeline_writer.write_all(pipeline_json.as_bytes())?;
    pipeline_writer.flush()?;

    let mut timestamps = Vec::new();
    let mut sizes = Vec::new();
    for i in 0..iterations {
        info!("Running iteration {} of {}... ", i+1, iterations);
        drop_table(&client, &table).await?;
        let t_start = std::time::Instant::now();
        let output = Command::new("pdal")
            // .arg("-v 4")
            .arg("pipeline")
            .arg("--input")
            .arg(&pipeline_filename.to_str().unwrap())
            .output()
            .await?;

        let duration = t_start.elapsed().as_secs_f64();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        timestamps.push(duration);
        info!("done in {} seconds with output {:?}, {:?}", duration, stderr, stdout);

        // check if table exists
        let table_exists = list_tables(&client).await?.contains(&table);
        if !table_exists {
            debug!("Existing tables: {:?}", list_tables(&client).await?);
            panic!("Table {} does not exist", table);
        }

        debug!("Querying database size...");
        let size = client.query(format!("SELECT pg_total_relation_size('{}')", table).as_str(), &[]).await?;
        let size: i64 = size[0].get(0);
        sizes.push(size);

        info!("done in {} seconds", duration);
    }
    debug!("Existing tables: {:?}", list_tables(&client).await?);

    // calculate average duration
    let duration = timestamps.iter().sum::<f64>() / iterations as f64;
    let size = sizes.iter().sum::<i64>() / iterations as i64;

    // query number of inserted points
    debug!("Querying number of points in table {}", table);
    let num_points_query = client.query(format!("SELECT SUM(PC_NumPoints(pa)) FROM {} LIMIT 1;", table).as_str(), &[]).await?;
    let num_points: i64 = num_points_query[0].get(0);

    // write result to json file
    let start_date = Utc::now();
    std::fs::create_dir_all(base_dir.join("results"))?;
    let filename = base_dir.join("results").join(format!("pg_insertion_results_{}_{}_{}.json", table, compression, start_date.to_rfc3339()));
    debug!("Writing results to file {}", &filename.to_str().unwrap());
    let output_file = std::fs::File::create(&filename)?;
    let mut output_writer = std::io::BufWriter::new(output_file);
    let mut json_output : serde_json::Value = json!({});
    json_output[table] = json!({
        "table": table,
        "compression": compression,
        "iterations": iterations,
        "start_date": start_date.to_rfc3339(),
        "end_date": Utc::now().to_rfc3339(),
        "size": size,
        "num_points": num_points,
        "duration": duration,
    });
    serde_json::to_writer_pretty(&mut output_writer, &json_output)?;

    info!("Wrote results to file {}", &filename.to_str().unwrap());
    info!("Pipeline executed in average {} seconds", duration);

    Ok(())
}
