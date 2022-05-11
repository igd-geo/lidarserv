use crate::cli::PreConvertArgs;
use crate::{iter_points, Vector3};
use anyhow::{bail, Result};
use lidarserv_server::common::geometry::position::I32CoordinateSystem;
use lidarserv_server::common::las::async_write_compressed_las_with_variable_chunk_size;
use lidarserv_server::index::point::LasPoint;
use lidarserv_server::net::protocol::connection::Connection;
use lidarserv_server::net::protocol::messages::{CoordinateSystem, Message};
use std::fs::File;
use std::io::BufWriter;
use std::mem::take;
use std::path::PathBuf;
use std::thread;
use tokio::net::TcpStream;

pub async fn preconvert(args: PreConvertArgs) -> Result<()> {
    // get coordinate system from server
    let (coordinate_system, _) = get_server_settings(&args).await?;
    let coordinate_system = match coordinate_system {
        CoordinateSystem::I32CoordinateSystem { scale, offset } => {
            I32CoordinateSystem::from_las_transform(scale, offset)
        }
    };

    // read points from input files (csv)
    let (chunks_sender, chunks_receiver) = crossbeam_channel::bounded(100);
    let t1 = {
        let args = args.clone();
        let coordinate_system = coordinate_system.clone();
        thread::spawn(move || read_points(&args, chunks_sender, &coordinate_system))
    };

    // write points to output file (laz)
    let t2 = {
        let args = args.clone();
        thread::spawn(move || write_points(&args, chunks_receiver, &coordinate_system))
    };

    // wait for read / write threads to finish
    t1.join().unwrap()?;
    t2.join().unwrap()?;
    Ok(())
}

const PROTOCOL_VERSION: u32 = 1;

async fn get_server_settings(args: &PreConvertArgs) -> Result<(CoordinateSystem, bool)> {
    // connect
    let (_sender, mut shutdown) = tokio::sync::broadcast::channel(1);
    let tcp_con = TcpStream::connect((args.host.as_str(), args.port)).await?;
    let peer_addr = tcp_con.peer_addr()?;
    let mut connection = Connection::new(tcp_con, peer_addr, &mut shutdown).await?;

    // exchange hello messages and check each others protocol compatibility
    connection
        .write_message(&Message::Hello {
            protocol_version: PROTOCOL_VERSION,
        })
        .await?;
    let hello = connection.read_message(&mut shutdown).await?;
    match hello {
        Message::Hello { protocol_version } => {
            if protocol_version != PROTOCOL_VERSION {
                bail!(
                    "Protocol version mismatch (Server: {}, Client: {}).",
                    protocol_version,
                    PROTOCOL_VERSION
                );
            }
        }
        _ => bail!("Protocol error"),
    };

    // wait for the point cloud info.
    let pc_info = connection.read_message(&mut shutdown).await?;
    let (coordinate_system, use_color) = match pc_info {
        Message::PointCloudInfo {
            coordinate_system,
            color,
        } => (coordinate_system, color),
        _ => bail!("Protocol error"),
    };

    Ok((coordinate_system, use_color))
}

fn read_points(
    args: &PreConvertArgs,
    sender: crossbeam_channel::Sender<Vec<LasPoint>>,
    coordinate_system: &I32CoordinateSystem,
) -> Result<()> {
    let trajectory_file = PathBuf::from(&args.trajectory_file);
    let points_file = PathBuf::from(&args.points_file);
    let offset = Vector3::new(args.offset_x, args.offset_y, args.offset_z);

    let mut t0 = None;
    let mut current_frame = 0;
    let mut current_frame_points = Vec::new();

    for (t, point) in iter_points::iter_points(&trajectory_file, &points_file, offset)? {
        let t0 = *t0.get_or_insert(t);
        let frame_number = ((t - t0) / args.speed_factor * args.fps as f64) as i32;
        while current_frame < frame_number {
            sender.send(take(&mut current_frame_points))?;
            current_frame += 1;
        }
        let point = match point.into_las_point(coordinate_system) {
            Ok(p) => p,
            Err(_) => continue, // points that are outside the bounds of the coordinate system are excluded
        };
        current_frame_points.push(point);
    }
    if !current_frame_points.is_empty() {
        sender.send(current_frame_points)?;
    }
    Ok(())
}

fn write_points(
    args: &PreConvertArgs,
    receiver: crossbeam_channel::Receiver<Vec<LasPoint>>,
    coordinate_system: &I32CoordinateSystem,
) -> Result<()> {
    let write = File::create(&args.output_file)?;
    let write = BufWriter::new(write);
    async_write_compressed_las_with_variable_chunk_size(receiver, coordinate_system, write)?;
    Ok(())
}
