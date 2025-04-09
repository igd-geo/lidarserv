use std::{
    fs::File,
    io::{BufWriter, Read, Seek, SeekFrom, Write, stdout},
    path::PathBuf,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::RecvTimeoutError,
    },
    thread::{self},
    time::Duration,
};

use anyhow::Result;
use clap::Parser;
use cli::AppOptions;
use human_panic::setup_panic;
use lidarserv_server::{
    index::query::Query,
    net::client::viewer::{PartialResult, QueryConfig, ViewerClient},
};
use log::{debug, error, info, warn};
use pasture_core::containers::{BorrowedBuffer, VectorBuffer};
use pasture_io::{
    base::PointWriter,
    las::{LASWriter, path_is_compressed_las_file},
};
use tokio::sync::{
    broadcast,
    mpsc::{self, Receiver, Sender},
};

mod cli;

#[tokio::main]
async fn main() -> ExitCode {
    setup_panic!();

    // arg parsing
    let args = AppOptions::parse();

    // logger
    simple_logger::init_with_level(args.log_level).unwrap();

    // report thread
    let status = Arc::new(Status::default());
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();
    let report_thread_handle = {
        let status = Arc::clone(&status);
        thread::spawn(move || report_thread(status, shutdown_rx))
    };

    // writer thread
    let (points_tx, points_rx) = mpsc::channel(100);
    let writer_thread_handle = {
        let file_name = args.outfile.clone();
        thread::spawn(move || write_thread(points_rx, file_name))
    };

    let exit_code = match query_thread(points_tx, &args, status).await {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            debug!("{e:?}");
            ExitCode::FAILURE
        }
    };
    writer_thread_handle.join().unwrap().unwrap();
    drop(shutdown_tx);
    report_thread_handle.join().unwrap();
    exit_code
}

async fn query_thread(
    points_tx: Sender<VectorBuffer>,
    args: &AppOptions,
    status: Arc<Status>,
) -> Result<()> {
    // parse query
    let query = if args.json {
        serde_json::from_str(&args.query)?
    } else {
        Query::parse(&args.query)?
    };

    // connect
    let (_shutdown_tx, mut shutdown_rx) = broadcast::channel(1);
    let mut client =
        ViewerClient::connect((args.host.as_str(), args.port), &mut shutdown_rx).await?;

    // send query
    client
        .write
        .query_oneshot(
            query,
            &QueryConfig {
                point_filtering: !args.disable_point_filtering,
            },
        )
        .await?;

    loop {
        let update = client
            .read
            .receive_update_global_coordinates(&mut shutdown_rx)
            .await?;

        match update {
            PartialResult::DeleteNode(_) => warn!("Received unexpected DeleteNode message."),
            PartialResult::UpdateNode(update) => {
                status
                    .points_received
                    .fetch_add(update.points.len() as u64, Ordering::Relaxed);
                status.nodes_received.fetch_add(1, Ordering::Relaxed);
                points_tx.send(update.points).await?
            }
            PartialResult::Complete => break,
        }
    }

    Ok(())
}

fn create_unlinked_file() -> Result<File> {
    let folder = std::env::temp_dir();
    let file_name = folder.join(format!("lidarserv-query.{}.las", std::process::id()));
    let file = File::create_new(&file_name)?;
    std::fs::remove_file(file_name)?;
    Ok(file)
}

fn write_thread(mut points_rx: Receiver<VectorBuffer>, file_name: Option<PathBuf>) -> Result<()> {
    // todo the las writer will not always write all attributes.
    //  - LasBasicFlags and LasExtendedFlags attributes are ignored. instead
    //    these flags are always hand-built from its parts.
    //  - No extra bytes are supported
    // However I consider this something to be fixed in pasture_io
    // rather than here.
    //
    // todo the las writer will always use the default coordinate transform.
    // create custom transform based on lidarserv coordinate system.
    let mut writer = None;
    while let Some(chunk) = points_rx.blocking_recv() {
        if writer.is_none() {
            let file = match &file_name {
                Some(s) => File::create_new(s)?,
                None => create_unlinked_file()?,
            };
            let buf_file = BufWriter::new(file);
            let is_compressed = match &file_name {
                Some(s) => path_is_compressed_las_file(s)?,
                None => false,
            };
            let w = LASWriter::from_writer_and_point_layout(
                buf_file,
                chunk.point_layout(),
                is_compressed,
            )?;
            writer = Some(w);
        }
        let writer = writer.as_mut().unwrap();
        writer.write(&chunk)?;
    }
    if let Some(writer) = &mut writer {
        writer.flush()?;
    }

    if file_name.is_none() {
        if let Some(writer) = writer {
            let mut file = writer.into_inner()?.into_inner()?;
            let mut bytes_remaining = file.seek(SeekFrom::End(0))? as usize;
            file.seek(SeekFrom::Start(0))?;
            let stdout = stdout();
            let mut ostream = stdout.lock();
            let mut buffer = vec![0_u8; 1024 * 1024]; // 1MiB buffer

            while bytes_remaining > 0 {
                let read_bytes = bytes_remaining.min(buffer.len());
                let buf_slice = &mut buffer[..read_bytes];
                file.read_exact(buf_slice)?;
                ostream.write_all(buf_slice)?;
                bytes_remaining -= read_bytes;
            }
        }
    }
    Ok(())
}

fn report_thread(status: Arc<Status>, shutdown_rx: std::sync::mpsc::Receiver<()>) {
    let mut last_received_points = 0;
    let mut last_received_nodes = 0;
    loop {
        // sleep
        let exit = match shutdown_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(_) => true,
            Err(RecvTimeoutError::Disconnected) => true,
            Err(RecvTimeoutError::Timeout) => false,
        };

        // print status
        let points_received = status.points_received.load(Ordering::Relaxed);
        let nodes_received = status.nodes_received.load(Ordering::Relaxed);
        let pps = points_received - last_received_points;
        let nps = nodes_received - last_received_nodes;
        last_received_points = points_received;
        last_received_nodes = nodes_received;
        if exit {
            info!("Total number of points: {points_received:10}");
            break;
        } else {
            info!("[pps: {pps:7} nps:{nps:5}] Received points: {points_received:10}");
        }
    }
}

#[derive(Debug, Default)]
struct Status {
    points_received: AtomicU64,
    nodes_received: AtomicU64,
}
