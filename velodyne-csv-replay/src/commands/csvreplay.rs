use crate::cli::LiveReplayArgs;
use crate::{iter_points, Vector3};
use anyhow::{anyhow, Result};
use lidarserv_server::index::point::GlobalPoint;
use lidarserv_server::net::client::capture_device::CaptureDeviceClient;
use log::{info, warn};
use std::mem::take;
use std::path::PathBuf;
use std::thread;
use std::thread::sleep;
use std::time::{Duration, Instant};

pub async fn replay_csv(args: LiveReplayArgs) -> Result<()> {
    // connect
    let (_sender, mut shutdown) = tokio::sync::broadcast::channel(1);
    let mut client = CaptureDeviceClient::connect(
        (args.host.as_str(), args.port),
        &mut shutdown,
        !args.no_compression,
    )
    .await?;

    // start reading points (in separate IO thread)
    let (points_sender, mut points_receiver) = tokio::sync::mpsc::channel(args.fps as usize);
    let read_points_thread = thread::spawn(move || read_points(&args, points_sender));

    let mut points_sent = 0;
    let mut frames_sent = 0;
    let mut last_points_sent = 0;
    let mut last_frames_sent = 0;
    let mut last_status = Instant::now();

    // send points
    while let Some(points) = points_receiver.recv().await {
        points_sent += points.len();
        frames_sent += 1;
        client.insert_points(points).await?;
        let now = Instant::now();
        let time_since_last_status = now.duration_since(last_status);
        if time_since_last_status > Duration::from_secs(2) {
            last_status = now;
            let pps =
                (points_sent - last_points_sent) as f64 / time_since_last_status.as_secs_f64();
            let fps =
                (frames_sent - last_frames_sent) as f64 / time_since_last_status.as_secs_f64();
            last_points_sent = points_sent;
            last_frames_sent = frames_sent;
            info!(
                "Sent {} points in {} frames; fps = {:.1}, pps = {:.0}",
                points_sent, frames_sent, fps, pps
            );
        }
    }

    // finish threads (forwarding any errors that have occurred)
    read_points_thread
        .join()
        .map_err(|_| anyhow!("Reader thread panicked."))??;
    Ok(())
}

/// Thread for blocking (file) IO
fn read_points(
    args: &LiveReplayArgs,
    sender: tokio::sync::mpsc::Sender<Vec<GlobalPoint>>,
) -> Result<()> {
    let frame_time = 1.0 / args.fps as f64;
    let mut current_frame: Option<(f64, Instant, Vec<_>)> = None;

    let trajectory_file = PathBuf::from(&args.trajectory_file);
    let points_file = PathBuf::from(&args.points_file);
    let offset = Vector3::new(args.offset_x, args.offset_y, args.offset_z);

    for (t, point) in iter_points::iter_points(&trajectory_file, &points_file, offset)? {
        let point_scaled_time_stamp = t / args.speed_factor;
        loop {
            let (start_ts, start_time, points) = current_frame
                .get_or_insert_with(|| (point_scaled_time_stamp, Instant::now(), Vec::new()));
            let end_ts = *start_ts + frame_time;
            if point_scaled_time_stamp <= end_ts {
                points.push(point);
                break;
            }
            let end_time = *start_time + Duration::from_secs_f64(frame_time);
            let now = Instant::now();
            if now < end_time {
                sleep(end_time.duration_since(now));
            } else if now > end_time + Duration::from_secs(2) {
                warn!(
                    "Falling behind. Points will be sent at s slower rate, but as fast as I can."
                );
            }
            *start_ts = end_ts;
            *start_time = end_time;
            let points = take(points);
            sender.blocking_send(points)?;
        }
    }

    if let Some((_, _, remaining_points)) = current_frame {
        sender.blocking_send(remaining_points)?;
    }
    Ok(())
}
