use std::fmt::Error;
use std::fs::File;
use std::option::Option;
use std::io::BufWriter;
use std::thread;
use std::io::Cursor;
use std::path::PathBuf;
use log::{debug, info, trace, warn};
use las::{Builder, Color, Header, Point, raw, Write, Writer};
use las::point::{Classification, Format, ScanDirection};
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
    // create path
    let path = PathBuf::from(args.output_file);

    // connect
    info!("Connecting to server at {}:{}", args.host, args.port);
    let (exit_sender, mut exit_receiver) = tokio::sync::broadcast::channel(1);
    let client = ViewerClient::connect((args.host, args.port), &mut exit_receiver).await?;

    // create channel
    debug!("Creating channel.");
    let (mut client_read, mut client_write) = client.into_split();

    // create query
    debug!("Creating query.");
    let aabb = AABB::new(
        Point3::new(args.min_x, args.min_y, args.min_z),
        Point3::new(args.max_x, args.max_y, args.max_z),
    );
    let lod = LodLevel::from_level(args.lod);
    info!("Query: {:?} {:?}", aabb, lod);

    // send query
    debug!("Sending query.");
    client_write.query_aabb(&aabb, &lod).await.unwrap();

    // receive result
    let mut points : Vec<GlobalPoint> = Vec::new();
    debug!("Receiving result.");
    loop {
        let result = client_read.receive_update(&mut exit_receiver).await.unwrap();
        client_write.ack().await.unwrap();
        debug!("Result: {:?}", result);

        if result.result_complete {
            info!("Result complete.");
            write_points_to_las_file(&path, &points);
            // break;
        }

        for node in result.insert.iter() {
            debug!("Node: {:?}", node);
            for point in node.points.iter() {
                points.push(point.clone());
            }
        }
        debug!("Number of points: {}", points.len());
    }
}

fn write_points_to_las_file(path: &PathBuf, points: &Vec<GlobalPoint>) {
    info!("Writing {:?} points to file: {:?}", points.len(), path);
    if points.len() == 0 {
        warn!("No points to write.");
    }
    let mut builder = Builder::from((1, 4));
    builder.point_format = Format::new(3).unwrap();
    let header = builder.into_header().unwrap();
    let mut writer = Writer::from_path(&path, header).unwrap();
    let mut errors = 0;
    let point = Point { x: 1., y: 2., z: 3., ..Default::default() };
    for point in points.iter() {
        let direction : ScanDirection = match point.attribute().scan_direction {
            true => ScanDirection::LeftToRight,
            false => ScanDirection::RightToLeft,
            _ => ScanDirection::default(),
        };
        let point = Point {
            x: point.position().x() as f64,
            y: point.position().y() as f64,
            z: point.position().z() as f64,
            intensity: point.attribute().intensity as u16,
            return_number: point.attribute().return_number as u8, //todo wrong
            number_of_returns: point.attribute().number_of_returns as u8,
            scan_direction: direction, //todo wrong
            is_edge_of_flight_line: point.attribute().edge_of_flight_line, //todo wrong
            classification: Classification::new(point.attribute().classification as u8).unwrap(),
            scan_angle: point.attribute().scan_angle_rank as f32,
            user_data: point.attribute().user_data as u8,
            point_source_id: point.attribute().point_source_id as u16,
            gps_time: Option::from(point.attribute().gps_time as f64),  //Todo test
            color: Option::from(Color::new(point.attribute().color.0, point.attribute().color.1, point.attribute().color.2)), //Todo test
            .. Default::default()
        };
        let result = writer.write(point);
        if result.is_err() {
            errors += 1;
        }
    }
    info!("Number of errors: {}", errors);
    debug!("Header: {:?}", writer.header());
    writer.close().unwrap();
    ()
}