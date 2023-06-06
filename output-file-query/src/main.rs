use std::fmt::Error;
use std::fs::File;
use std::io::BufWriter;
use std::thread;
use std::io::Cursor;
use log::{debug, info, trace};
use serde::de::Unexpected::Option;
use las::{Builder, Point, raw, Write, Writer};
use lidarserv_server::common::geometry::bounding_box::{AABB, BaseAABB};
use lidarserv_server::common::geometry::grid::LodLevel;
use lidarserv_server::common::nalgebra::Point3;
use lidarserv_server::net::client::viewer::ViewerClient;
use lidarserv_server::net::LidarServerError;
use lidarserv_common::geometry::bounding_box::OptionAABB;
use lidarserv_common::geometry::points::PointType;
use lidarserv_common::geometry::position::{I32CoordinateSystem, I32Position, Position};
use lidarserv_server::index::point::{GenericPoint, GlobalPoint, LasPoint};
use crate::cli::Args;

mod cli;

#[paw::main]
fn main(args: Args) {
    simple_logger::init_with_level(args.log_level).unwrap();
    info!("Starting client.");
    network_thread(args);
}

#[tokio::main]
async fn network_thread(args: Args) -> Result<(), LidarServerError> {
    // connect
    info!("Connecting to server at {}:{}", args.host, args.port);
    let (exit_sender, mut exit_receiver) = tokio::sync::broadcast::channel(1);
    let client = ViewerClient::connect((args.host, args.port), &mut exit_receiver).await?;

    // create channel
    info!("Creating channel.");
    let (mut client_read, mut client_write) = client.into_split();

    // create query
    info!("Creating query.");
    let aabb = AABB::new(
        Point3::new(args.min_x, args.min_y, args.min_z),
        Point3::new(args.max_x, args.max_y, args.max_z),
    );
    let lod = LodLevel::from_level(args.lod);
    info!("Query: {:?} {:?}", aabb, lod);

    // send query
    info!("Sending query.");
    client_write.query_aabb(&aabb, &lod).await.unwrap();

    // receive result
    let mut points : Vec<GlobalPoint> = Vec::new();
    info!("Receiving result.");
    loop {
        let result = client_read.receive_update(&mut exit_receiver).await.unwrap();
        client_write.ack().await.unwrap();
        info!("Result: {:?}", result);

        if result.result_complete {
            info!("Result complete.");
            write_points_to_las_file(&points);
            // break;
        }

        for node in result.insert.iter() {
            info!("Node: {:?}", node);
            for point in node.points.iter() {
                points.push(point.clone());
            }
        }
        info!("Number of points: {}", points.len());
    }

    // save as las
    info!("Saving result as las.");
    // write_points_to_las_file(&points);
    Ok(())
}

fn write_points_to_las_file(points: &Vec<GlobalPoint>) {
    // create Output file
    let mut path = std::env::current_dir().unwrap();
    path.push("output.las");
    let mut writer = Writer::from_path(path, Default::default()).unwrap();
    let point = Point { x: 1., y: 2., z: 3., ..Default::default() };
    for point in points.iter() {
        let point = Point {
            x: point.position().x() as f64,
            y: point.position().y() as f64,
            z: point.position().z() as f64,
            ..Default::default()
        };
        let _ = writer.write(point);
    }
    writer.close().unwrap();
    ()
}