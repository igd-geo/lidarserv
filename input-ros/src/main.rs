//! Connector that subscribes to a ROS PointCloud2 topic and forwards all incoming points to lidarserv.

use std::{process::exit, thread};

use crate::{cli::Cli, ros::ros_read_thread};
use anyhow::Result;
use clap::Parser;
use lidarserv_server::net::client::capture_device::CaptureDeviceClient;
use log::{error, info};
use tokio::{signal::ctrl_c, sync::mpsc};

mod cli;
mod ros;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    simple_logger::init().unwrap();

    match main_prettyprint_err(cli).await {
        Ok(_) => (),
        Err(e) => {
            error!("{e}");
            exit(1);
        }
    }
}

async fn main_prettyprint_err(cli: Cli) -> Result<()> {
    let (shitdown_sender, mut shutdown) = tokio::sync::broadcast::channel(1);
    let (points_tx, mut points_rx) = mpsc::channel(5);

    tokio::spawn(async move {
        ctrl_c().await.expect("Failed to install ctrl_c listener.");
        info!("Good Bye.");
        shitdown_sender.send(()).ok();
    });

    let mut lidarserv = CaptureDeviceClient::connect(
        (cli.host.clone(), cli.port),
        &mut shutdown,
        cli.enable_compression,
    )
    .await?;

    thread::spawn(move || {
        ros_read_thread(&cli, points_tx);
    });

    while let Some(points) = points_rx.recv().await {
        lidarserv.insert_points(points).await?;
    }

    Ok(())
}
