use std::fmt::Debug;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use crate::Point;
use lidarserv_common::index::{Index, Reader};
use lidarserv_common::query::Query;
use serde_json::json;
use std::time::Instant;
use log::{debug, info};
use nalgebra::Point3;
use lidarserv_common::geometry::bounding_box::OptionAABB;
use lidarserv_common::geometry::points::{PointType, WithAttr};
use lidarserv_common::geometry::position::I32CoordinateSystem;
use lidarserv_common::index::octree::attribute_bounds::LasPointAttributeBounds;
use lidarserv_common::las::{I32LasReadWrite, Las};
use lidarserv_server::index::point::{GlobalPoint, LasPoint};
use crate::queries::*;

pub fn measure_query_performance<I>(
    mut index: I,
    output_queries: bool,
) -> serde_json::value::Value
where
    I: Index<Point>,
{
    json!({
        "time_range": measure_one_query("time_range", output_queries, &mut index, aabb_full(), time_range()),
        "ground_classification": measure_one_query("ground_classification", output_queries, &mut index, aabb_full(), ground_classification()),
        "building_classification": measure_one_query("building_classification",output_queries,  &mut index, aabb_full(), building_classification()),
        "vegetation_classification": measure_one_query("vegetation_classification", output_queries, &mut index, aabb_full(), vegetation_classification()),
        "normal_x_vertical": measure_one_query("normal_x_vertical", output_queries, &mut index, aabb_full(), normal_x_vertical()),
        "high_intensity": measure_one_query("high_intensity", output_queries, &mut index, aabb_full(), high_intensity()),
        "low_intensity": measure_one_query("low_intensity", output_queries, &mut index, aabb_full(), low_intensity()),
        "one_return": measure_one_query("one_return", output_queries, &mut index, aabb_full(), one_return()),
        "mixed_ground_and_time": measure_one_query("mixed_ground_and_time", output_queries, &mut index, aabb_full(), mixed_ground_and_time()),
    })
}

fn measure_one_query<I, Q>(
    name: &str,
    output_queries: bool,
    index: &mut I,
    query: Q,
    filter: LasPointAttributeBounds,
) -> serde_json::value::Value
    where
        I: Index<Point>,
        Q: Query + Send + Sync + 'static + Clone + Debug,
{
    info!("Measuring query performance for query: {:?} and filter {:?}", query, filter);

    // measure only spatial query
    info!("measure only spatial query");
    let raw_spatial = measure_one_query_part(name, output_queries, index, query.clone(), filter, false, false, false);

    // measure point filtering without acceleration
    info!("measure point filtering without acceleration");
    let raw_point_filtering = measure_one_query_part(name, output_queries, index, query.clone(), filter, false, false, true);

    // measure point filtering with node acceleration
    info!("measure point filtering with node acceleration");
    let point_filtering_with_node_acc = measure_one_query_part(name, output_queries, index, query.clone(), filter, true, false, true);

    // measure point filtering with node acceleration and histogram acceleration
    info!("measure point filtering with node acceleration and histogram acceleration");
    let point_filtering_with_full_acc = measure_one_query_part(name, output_queries, index, query.clone(), filter, true, true, true);

    // measure only node filtering
    info!("measure only node filtering");
    let only_node_acc = measure_one_query_part(name, output_queries, index, query.clone(), filter, true, false, false);

    // measure only node filtering with histogram acceleration
    info!("measure only node filtering with histogram acceleration");
    let only_full_acc = measure_one_query_part(name, output_queries, index, query.clone(), filter, true, true, false);

    json!({
        "raw_spatial": raw_spatial,
        "raw_point_filtering": raw_point_filtering,
        "point_filtering_with_node_acc": point_filtering_with_node_acc,
        "point_filtering_with_full_acc": point_filtering_with_full_acc,
        "only_node_acc": only_node_acc,
        "only_full_acc": only_full_acc,
    })
}


fn measure_one_query_part<I, Q>(
    name: &str,
    output_queries: bool,
    index: &mut I,
    query: Q,
    filter: LasPointAttributeBounds,
    enable_node_acceleration: bool,
    enable_histogram_acceleration: bool,
    enable_point_filtering: bool,
) -> serde_json::value::Value
where
    I: Index<Point>,
    Q: Query + Send + Sync + 'static,
{
    debug!("Flushing index");
    index.flush().unwrap();

    let time_start = Instant::now();
    let mut r = index.reader(query);
    r.set_filter((Some(filter), enable_node_acceleration, enable_histogram_acceleration, enable_point_filtering));
    let mut nodes = Vec::new();
    while let Some((_node_id, node, _coordinate_system)) = r.load_one() {
        nodes.push(node);
    }
    let time_finish_query = Instant::now();
    let nr_nodes = nodes.len();
    let mut nr_points = 0;
    let mut nr_non_empty_nodes = 0;
    for points in &nodes {
        nr_points += points.len();
        if points.len() > 0 {
            nr_non_empty_nodes += 1;
        }
    }
    let time_finish_load = Instant::now();

    if output_queries {
        info!("Storing query result as LAS file...");
        let mut las_points : Vec<LasPoint> = Vec::new();
        for points in &nodes {
            for point in points {
                let mut las_point = LasPoint::new(point.position().clone());
                las_point.set_value(point.las_attributes.clone());
                las_points.push(las_point);
            }
        }
        let filename = format!("query_result_{}_n{}_h{}_p{}.las", name, enable_node_acceleration, enable_histogram_acceleration, enable_point_filtering);
        let path = Path::new(&filename);
        let loader = I32LasReadWrite::new(true, 3);
        let data = loader.write_las::<LasPoint, _>(Las {
            points: las_points.iter(),
            bounds: OptionAABB::default(),
            non_bogus_points: Some(las_points.len() as u32),
            coordinate_system: I32CoordinateSystem::new(Point3::new(0.0,0.0,0.0), Point3::new(3.0,3.0,3.0)),
        });
        let mut file = File::create(&path).unwrap();
        file.write_all(data.as_slice()).unwrap();
        file.sync_all().unwrap();
        info!("Stored query result as LAS file: {}", filename);
    }

    json!({
        "nr_nodes": nr_nodes,
        "nr_non_empty_nodes": nr_non_empty_nodes,
        "nr_points": nr_points,
        "query_time_seconds": (time_finish_query - time_start).as_secs_f64(),
        "load_time_seconds": (time_finish_load - time_finish_query).as_secs_f64(),
    })
}
