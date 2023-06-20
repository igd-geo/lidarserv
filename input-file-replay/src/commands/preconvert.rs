use crate::cli::PreConvertArgs;
use crate::{iter_points, Vector3};
use anyhow::{bail, Result};
use lidarserv_server::common::geometry::position::{I32CoordinateSystem, Position};
use lidarserv_server::common::las::{async_write_compressed_las_with_variable_chunk_size, I32LasReadWrite, Las, LasPointAttributes};
use lidarserv_server::index::point::LasPoint;
use lidarserv_server::net::protocol::connection::Connection;
use lidarserv_server::net::protocol::messages::{CoordinateSystem, Message};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::mem::take;
use std::path::PathBuf;
use std::thread;
use tokio::net::TcpStream;
use log::{info, warn};
use lidarserv_server::common::geometry::points::{PointType, WithAttr};

pub async fn preconvert(args: PreConvertArgs) -> Result<()> {
    // get coordinate system from server
    let (coordinate_system, point_record_format) = get_server_settings(&args).await?;
    let coordinate_system = match coordinate_system {
        CoordinateSystem::I32CoordinateSystem { scale, offset } => {
            let cs = I32CoordinateSystem::from_las_transform(scale, offset);
            info!("Server Coordinate System: {:?}", cs);
            cs
        }
    };

    // check for file extension
    let points_file = PathBuf::from(&args.points_file);
    let extension = points_file.extension();
    if extension.is_none() {
        bail!("Points file has no extension");
    }

    // create channel for point chunks
    let (chunks_sender, chunks_receiver) = crossbeam_channel::bounded(100);

    // choose read thread based on file extension
    let t1 = {
        let args = args.clone();
        let coordinate_system = coordinate_system.clone();
        match extension.unwrap().to_str() {
            Some("txt") => {
                // read points from input files (csv)
                thread::spawn(move || read_points_from_csv(&args, chunks_sender, &coordinate_system))
            }
            Some("csv") => {
                // read points from input files (csv)
                thread::spawn(move || read_points_from_csv(&args, chunks_sender, &coordinate_system))
            }
            Some("las") => {
                // read points from input files (las)
                thread::spawn(move || read_points_from_las(&args, chunks_sender, &coordinate_system, point_record_format))
            }
            _ => bail!("Unknown file extension"),
        }
    };

    // write points to output file (laz)
    let t2 = {
        let args = args.clone();
        thread::spawn(move || write_points(&args, chunks_receiver, &coordinate_system, point_record_format))
    };

    // wait for read / write threads to finish
    t1.join().unwrap()?;
    t2.join().unwrap()?;
    Ok(())
}

const PROTOCOL_VERSION: u32 = 1;

async fn get_server_settings(args: &PreConvertArgs) -> Result<(CoordinateSystem, u8)> {
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
    let (coordinate_system, point_record_format) = match pc_info {
        Message::PointCloudInfo {
            coordinate_system,
            point_record_format,
        } => (coordinate_system, point_record_format),
        _ => bail!("Protocol error"),
    };

    Ok((coordinate_system, point_record_format))
}

fn read_points_from_csv(
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
            info!("Sending frame {} with {} points", current_frame, current_frame_points.len());
            sender.send(take(&mut current_frame_points))?;
            current_frame += 1;
        }
        let mut point = match point.into_las_point(coordinate_system) {
            Ok(p) => p,
            Err(_) => continue, // points that are outside the bounds of the coordinate system are excluded
        };

        // add time attribute to point
        let mut attr = LasPointAttributes::default();
        attr.gps_time = t;
        point.set_value(attr);
        current_frame_points.push(point);
    }
    if !current_frame_points.is_empty() {
        info!("Sending last frame with {} points", current_frame_points.len());
        sender.send(current_frame_points)?;
    }
    Ok(())
}

fn read_points_from_las(
    args: &PreConvertArgs,
    sender: crossbeam_channel::Sender<Vec<LasPoint>>,
    coordinate_system: &I32CoordinateSystem,
    point_record_format: u8,
) -> Result<()> {

    // read args
    let points_file = PathBuf::from(&args.points_file);

    // read points
    let f = File::open(points_file)?;
    let mut reader = BufReader::new(f);
    // TODO choose use_color and use_time dynamically
    let las_reader : I32LasReadWrite = I32LasReadWrite::new(false, point_record_format);
    let mut result : Las<Vec<LasPoint>> = las_reader.read_las(&mut reader)?;
    info!("LAS File Coordinate System: {:?}", result.coordinate_system);

    //sort points by time
    info!("Sorting points by time");
    result.points.sort_by(|a, b| a.attribute().gps_time.partial_cmp(&b.attribute().gps_time).unwrap());

    let t0 = result.points[0].attribute().gps_time;
    let mut current_frame = 0;
    let mut current_frame_points = Vec::new();

    // chunk points into frames and send them
    for point in result.points {
        let t = point.attribute::<LasPointAttributes>().gps_time;
        let frame_number = ((t - t0) / args.speed_factor * args.fps as f64) as i32;
        while current_frame < frame_number {
            if current_frame_points.is_empty() && args.skip_empty_frames {
                // warn!("Skipping empty frame {}", current_frame);
                current_frame += 1;
                continue;
            }
            info!("Sending frame {} with {} points", current_frame, current_frame_points.len());
            sender.send(take(&mut current_frame_points))?;
            current_frame += 1;
        }

        // convert from las file coordinate system to server coordinate system
        let pos = point.position().transcode(&result.coordinate_system, coordinate_system).unwrap();
        //TODO handle better

        // create new point with new position and same attributes
        let attr = point.attribute::<LasPointAttributes>();
        let mut point:LasPoint = LasPoint::new(pos);
        point.set_value(attr.clone());

        current_frame_points.push(point);
    }
    if !current_frame_points.is_empty() {
        info!("Sending last frame with {} points", current_frame_points.len());
        sender.send(current_frame_points)?;
    }

    Ok(())
}

fn write_points(
    args: &PreConvertArgs,
    receiver: crossbeam_channel::Receiver<Vec<LasPoint>>,
    coordinate_system: &I32CoordinateSystem,
    point_record_format: u8,
) -> Result<()> {
    let write = File::create(&args.output_file)?;
    let write = BufWriter::new(write);
    async_write_compressed_las_with_variable_chunk_size(receiver, coordinate_system, write, point_record_format)?;
    Ok(())
}
