use std::error::Error;
use std::io::Write;
use async_process::Command;
use chrono::Utc;
use clap::Parser;
use serde_json::json;
use measurements::db::{connect_to_db, drop_table, PostGISConfig};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    input_file: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let input_file = args.input_file;
    if !std::path::Path::new(&input_file).exists() {
        panic!("Input file {} does not exist", input_file);
    }
    let base_dir = std::path::Path::new(&input_file).parent().unwrap();
    let table = std::path::Path::new(&input_file).file_stem().unwrap().to_str().unwrap();
    let iterations: usize = 1;

    // connect to db
    let postgis_config = PostGISConfig {
        database: "pointclouds".parse()?,
        host: "localhost".parse()?,
        password: "password".parse()?,
        username: "postgres".parse()?
    };
    let (client, _join_handle) = connect_to_db(&postgis_config).await?;

    let pipeline_json = format!(
        r#"
{{
    "pipeline": [
        {{
            "type": "readers.las",
            "filename": "data/{input_file}"
        }},
        {{
            "type": "filters.chipper",
            "capacity": "400"
        }},
        {{
            "type": "writers.pgpointcloud",
            "connection": "host='localhost' dbname='pointclouds' user='postgres' password='password' port='5432'",
            "table": "{table}",
            "compression": "dimensional",
            "pcid": "1"
        }}
    ]
}}
        "#
    );
    // write pipeline to json file
    std::fs::create_dir_all(base_dir.join("data"))?;
    let pipeline_filename = base_dir.join("data").join(format!("pipeline_{}.json", table));
    let pipeline_file = std::fs::File::create(&pipeline_filename)?;
    let mut pipeline_writer = std::io::BufWriter::new(pipeline_file);
    pipeline_writer.write_all(pipeline_json.as_bytes())?;
    pipeline_writer.flush()?;

    let mut timestamps = Vec::new();
    for i in 0..iterations {
        print!("Running iteration {} of {}... ", i+1, iterations);
        drop_table(&client, table).await?;
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
        println!("done in {} seconds with output {:?}, {:?}", duration, stderr, stdout);

        timestamps.push(duration);

        println!("done in {} seconds", duration);
    }

    // calculate average duration
    let duration = timestamps.iter().sum::<f64>() / iterations as f64;

    // write result to json file
    let start_date = Utc::now();
    std::fs::create_dir_all(base_dir.join("results"))?;
    let filename = base_dir.join("results").join(format!("insertion_results_{}_{}.json", table, start_date.to_rfc3339()));
    let output_file = std::fs::File::create(filename)?;
    let mut output_writer = std::io::BufWriter::new(output_file);
    let mut json_output : serde_json::Value = json!({});
    json_output[table] = json!({
        "duration": duration,
    });
    serde_json::to_writer_pretty(&mut output_writer, &json_output)?;

    println!("Pipeline executed in average {} seconds", duration);

    Ok(())
}
