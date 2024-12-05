use std::env;
use std::error::Error;
use std::io::Write;
use async_process::Command;
use chrono::Utc;
use serde_json::json;
use measurements::db::{connect_to_db, drop_table, PostGISConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <table> <input_file>", args[0]);
        std::process::exit(1);
    }

    let table = &args[1];
    let input_file = &args[2];
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
    let filename = format!("data/pipeline_{}.json", table);
    let output_file = std::fs::File::create(filename)?;
    let mut output_writer = std::io::BufWriter::new(output_file);
    output_writer.write_all(pipeline_json.as_bytes())?;
    output_writer.flush()?;

    let mut timestamps = Vec::new();
    for i in 0..iterations {
        print!("Running iteration {} of {}... ", i+1, iterations);
        drop_table(&client, table).await?;
        let t_start = std::time::Instant::now();
        let output = Command::new("pdal")
            // .arg("-v 4")
            .arg("pipeline")
            .arg("--input")
            .arg(format!("data/pipeline_{}.json", table))
            .output()
            .await?;

        // let output = Command::new("pwd").output().await?;

        let duration = t_start.elapsed().as_secs_f64();
        // convert output.stderr to string
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
    let filename = format!("results/insertion_results_{}_{}.json", table, start_date.to_rfc3339());
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
