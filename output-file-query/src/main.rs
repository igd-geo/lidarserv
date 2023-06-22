use std::option::Option;
use std::path::PathBuf;
use log::{debug, info, warn};
use las::{Builder, Color, Point, Write, Writer};
use las::point::{Classification, Format, ScanDirection};
use lidarserv_server::common::geometry::bounding_box::{AABB, BaseAABB};
use lidarserv_server::common::geometry::grid::LodLevel;
use lidarserv_server::common::nalgebra::Point3;
use lidarserv_server::net::client::viewer::ViewerClient;
use lidarserv_server::net::LidarServerError;
use lidarserv_common::geometry::points::PointType;
use lidarserv_common::geometry::position::{Position};
use lidarserv_common::index::octree::attribute_bounds::LasPointAttributeBounds;
use lidarserv_server::index::point::{GlobalPoint};
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
    // parse cli arguments
    let bounds = parse_attribute_bounds(&args);
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
    client_write.query_aabb(&aabb, &lod, Some(bounds), args.enable_attribute_acceleration, args.enable_point_filtering).await.unwrap();

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
    for point in points.iter() {
        let direction : ScanDirection = match point.attribute().scan_direction {
            true => ScanDirection::LeftToRight,
            false => ScanDirection::RightToLeft,
        };
        // needed for this point format
        let mut classification = point.attribute().classification as u8;
        if classification > 31 {
            classification = 31;
        }
        let point = Point {
            x: point.position().x() as f64,
            y: point.position().y() as f64,
            z: point.position().z() as f64,
            intensity: point.attribute().intensity as u16,
            return_number: point.attribute().return_number as u8,
            number_of_returns: point.attribute().number_of_returns as u8,
            scan_direction: direction,
            is_edge_of_flight_line: point.attribute().edge_of_flight_line,
            classification: Classification::new(classification).unwrap_or(Classification::new(0).unwrap()),
            scan_angle: point.attribute().scan_angle_rank as f32,
            user_data: point.attribute().user_data as u8,
            point_source_id: point.attribute().point_source_id as u16,
            gps_time: Option::from(point.attribute().gps_time as f64),
            color: Option::from(Color::new(point.attribute().color.0, point.attribute().color.1, point.attribute().color.2)),
            .. Default::default()
        };
        let result = writer.write(point);
        if result.is_err() {
            info!("Error writing point: {:?}", result);
            errors += 1;
        }
    }
    info!("Number of errors: {}", errors);
    debug!("Header: {:?}", writer.header());
    writer.close().unwrap();
    ()
}

fn parse_attribute_bounds(args: &Args) -> LasPointAttributeBounds {
    let mut attribute_bounds = LasPointAttributeBounds::new();
    attribute_bounds.intensity = Some((args.min_intensity.unwrap_or(0), args.max_intensity.unwrap_or(u16::MAX)));
    attribute_bounds.return_number = Some((args.min_return_number.unwrap_or(0), args.max_return_number.unwrap_or(u8::MAX)));
    attribute_bounds.number_of_returns = Some((args.min_number_of_returns.unwrap_or(0), args.max_number_of_returns.unwrap_or(u8::MAX)));
    attribute_bounds.scan_direction = Some((args.min_scan_direction.unwrap_or(0) != 0, args.max_scan_direction.unwrap_or(1) != 0));
    attribute_bounds.edge_of_flight_line = Some((args.min_edge_of_flight_line.unwrap_or(0) != 0, args.max_edge_of_flight_line.unwrap_or(1) != 0));
    attribute_bounds.classification = Some((args.min_classification.unwrap_or(0), args.max_classification.unwrap_or(u8::MAX)));
    attribute_bounds.scan_angle_rank = Some((args.min_scan_angle.unwrap_or(i8::MIN), args.max_scan_angle.unwrap_or(i8::MAX)));
    attribute_bounds.user_data = Some((args.min_user_data.unwrap_or(0), args.max_user_data.unwrap_or(u8::MAX)));
    attribute_bounds.point_source_id = Some((args.min_point_source_id.unwrap_or(0), args.max_point_source_id.unwrap_or(u16::MAX)));
    attribute_bounds.gps_time = Some((args.min_gps_time.unwrap_or(f64::MIN), args.max_gps_time.unwrap_or(f64::MAX)));
    attribute_bounds.color_r = Some((args.min_color_r.unwrap_or(0), args.max_color_r.unwrap_or(u16::MAX)));
    attribute_bounds.color_g = Some((args.min_color_g.unwrap_or(0), args.max_color_g.unwrap_or(u16::MAX)));
    attribute_bounds.color_b = Some((args.min_color_b.unwrap_or(0), args.max_color_b.unwrap_or(u16::MAX)));
    debug!("Parsed Attribute bounds: {:?}", attribute_bounds);
    attribute_bounds
}