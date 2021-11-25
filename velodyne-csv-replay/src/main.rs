mod cli;
mod velodyne_csv_reader;

use crate::cli::Args;
use crate::velodyne_csv_reader::{PointReader, TrajectoryCsvRecord, TrajectoryReader};
use anyhow::{anyhow, Result};
use itertools::Itertools;
use lidarserv_server::common::geometry::points::PointType;
use lidarserv_server::common::geometry::position::F64Position;
use lidarserv_server::common::index::sensor_pos::point::SensorPositionAttribute;
use lidarserv_server::common::las::LasPointAttributes;
use lidarserv_server::common::nalgebra::{Matrix4, Vector3, Vector4};
use lidarserv_server::index::point::GlobalPoint;
use lidarserv_server::net::client::capture_device::CaptureDeviceClient;
use log::{error, info, warn};
use std::f64::consts::PI;
use std::fs::File;
use std::io::BufReader;
use std::mem::take;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use tokio::time::Instant;

#[paw::main]
#[tokio::main]
async fn main(args: Args) {
    simple_logger::init_with_level(args.log_level).unwrap();
    match main_log_errors(args).await {
        Ok(()) => (),
        Err(e) => {
            error!("{}", e);
        }
    }
}

async fn main_log_errors(args: Args) -> Result<()> {
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
fn read_points(args: &Args, sender: tokio::sync::mpsc::Sender<Vec<GlobalPoint>>) -> Result<()> {
    let trajectory_file = BufReader::new(File::open(&args.trajectory_file)?);
    let trajectory_groups = TrajectoryReader::new(trajectory_file)?
        .flat_map(|r| {
            if let Err(e) = &r {
                error!("Skip trajectory record: {}", e);
            }
            r
        })
        .group_by(|it| it.time_stamp);
    let mut trajectory_iter = trajectory_groups
        .into_iter()
        .flat_map(|(time_stamp, group)| {
            let items: Vec<_> = group.collect_vec();
            let time_base = time_stamp as f64;
            let time_step = 1.0 / items.len() as f64;
            items
                .into_iter()
                .enumerate()
                .map(move |(index, item)| (time_base + time_step * index as f64, item))
        })
        .tuple_windows();

    let mut cur_trajectory_segment: Option<(
        (f64, TrajectoryCsvRecord),
        (f64, TrajectoryCsvRecord),
    )> = trajectory_iter.next();

    let points_file = BufReader::new(File::open(&args.points_file)?);
    let points = PointReader::new(points_file)?
        .flat_map(|it| {
            if let Err(e) = &it {
                error!("Skip point record: {}", e);
            }
            it
        })
        .flat_map(|point_record| {
            let t = point_record.time_stamp;
            let ((t1, traj1), (t2, traj2)) = loop {
                let v = match &cur_trajectory_segment {
                    None => {
                        return None;
                    }
                    Some(v) => v,
                };
                if t < v.0 .0 {
                    return None;
                }
                if t <= v.1 .0 {
                    break v;
                }
                cur_trajectory_segment = trajectory_iter.next();
            };
            let (t1, t2) = (*t1, *t2);
            let weight_1 = (t2 - t) / (t2 - t1);
            let weight_2 = (t - t1) / (t2 - t1);
            let interpolated = TrajectoryCsvRecord {
                time_stamp: traj1.time_stamp,
                distance: traj1.distance * weight_1 + traj2.distance * weight_2,
                easting: traj1.easting * weight_1 + traj2.easting * weight_2,
                northing: traj1.northing * weight_1 + traj2.northing * weight_2,
                altitude1: traj1.altitude1 * weight_1 + traj2.altitude1 * weight_2,
                latitude: traj1.latitude * weight_1 + traj2.latitude * weight_2,
                longitude: traj1.longitude * weight_1 + traj2.longitude * weight_2,
                altitude2: traj1.altitude2 * weight_1 + traj2.altitude2 * weight_2,
                roll: traj1.roll * weight_1 + traj2.roll * weight_2,
                pitch: traj1.pitch * weight_1 + traj2.pitch * weight_2,
                heading: traj1.heading * weight_1 + traj2.heading * weight_2,
                velocity_easting: traj1.velocity_easting * weight_1
                    + traj2.velocity_easting * weight_2,
                velocity_northing: traj1.velocity_northing * weight_1
                    + traj2.velocity_northing * weight_2,
                velocity_down: traj1.velocity_down * weight_1 + traj2.velocity_down * weight_2,
            };
            Some((interpolated, point_record))
        });

    let frame_time = 1.0 / args.fps as f64;
    let mut current_frame: Option<(f64, Instant, Vec<_>)> = None;
    for (traj, point) in points {
        let point_scaled_time_stamp = point.time_stamp / args.speed_factor;
        loop {
            let (start_ts, start_time, points) = current_frame
                .get_or_insert_with(|| (point_scaled_time_stamp, Instant::now(), Vec::new()));
            let end_ts = *start_ts + frame_time;
            if point_scaled_time_stamp <= end_ts {
                let trajectory_position = Vector3::new(traj.easting, traj.northing, traj.altitude1);
                let point_hom =
                    Vector4::new(point.point_3d_x, point.point_3d_z, -point.point_3d_y, 1.0);
                let point_hom =
                    Matrix4::new_rotation(Vector3::new(0.0, 0.0, -traj.heading / 360.0 * 2.0 * PI))
                        * point_hom;
                let point_position = point_hom.xyz() / point_hom.w;
                let position = trajectory_position + point_position;
                let mut global_point = GlobalPoint::new(F64Position::new(
                    position.x - args.offset_x,
                    position.y - args.offset_y,
                    position.z - args.offset_z,
                ));
                global_point
                    .attribute_mut::<SensorPositionAttribute<F64Position>>()
                    .0 = F64Position::new(
                    trajectory_position.x - args.offset_x,
                    trajectory_position.y - args.offset_y,
                    trajectory_position.z - args.offset_z,
                );
                global_point.attribute_mut::<LasPointAttributes>().intensity =
                    (point.intensity * u16::MAX as f64) as u16;
                points.push(global_point);
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
