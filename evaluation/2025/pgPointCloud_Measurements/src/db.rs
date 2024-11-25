use std::fs::File;
use std::io::Write;
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
use crate::attribute_bounds::LasPointAttributeBounds;
use crate::queries::*;

pub struct PostGISConfig {
    pub host: String,
    pub username: String,
    pub database: String,
    pub password: String,
}

pub async fn connect_to_db(config: &PostGISConfig) -> Result<(Client, JoinHandle<()>)> {
    info!("Connecting to database");
    let connect_string = format!(
        "host={} dbname={} user={} password={}",
        config.host, config.database, config.username, config.password
    );

    let (client, connection) = tokio_postgres::connect(&connect_string, NoTls).await?;
    info!("Connection Successfull");

    // The connection object performs the actual communication with the database,
    // so spawn it off to run on its own.
    let join_handle = tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    Ok((client, join_handle))
}

pub async fn drop_table(client: &Client, table: &str) -> Result<()> {
    info!("Dropping table {}", table);
    let drop_query = format!("DROP TABLE IF EXISTS {};", table);
    client.execute(drop_query.as_str(), &[]).await?;
    Ok(())
}