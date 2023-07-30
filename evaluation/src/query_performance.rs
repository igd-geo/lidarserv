use std::fmt::Debug;
use crate::queries::{aabb_full, ground_classification};
use crate::Point;
use lidarserv_common::index::{Index, Reader};
use lidarserv_common::las::I32LasReadWrite;
use lidarserv_common::query::Query;
use serde_json::json;
use std::time::Instant;
use log::{debug, info};
use lidarserv_common::index::octree::attribute_bounds::LasPointAttributeBounds;

pub fn measure_query_performance<I>(
    mut index: I,
) -> serde_json::value::Value
where
    I: Index<Point>,
{
    json!({
        "ground_classification": measure_one_query(&mut index, aabb_full(), ground_classification()),
    })
}

fn measure_one_query<I, Q>(
    index: &mut I,
    query: Q,
    filter: LasPointAttributeBounds,
) -> serde_json::value::Value
    where
        I: Index<Point>,
        Q: Query + Send + Sync + 'static + Clone + Debug,
{
    info!("Measuring query performance for query: {:?} and filter {:?}", query, filter);

    // measure point filtering without acceleration
    debug!("measure point filtering without acceleration");
    let raw_point_filtering = measure_one_query_part(index, query.clone(), filter, false, false, true);

    // measure point filtering with node acceleration
    debug!("measure point filtering with node acceleration");
    let point_filtering_with_node_acc = measure_one_query_part(index, query.clone(), filter, true, false, true);

    // measure point filtering with node acceleration and histogram acceleration
    debug!("measure point filtering with node acceleration and histogram acceleration");
    let point_filtering_with_full_acc = measure_one_query_part(index, query.clone(), filter, true, true, true);

    // measure only node filtering
    debug!("measure only node filtering");
    let only_node_acc = measure_one_query_part(index, query.clone(), filter, true, false, false);

    // measure only node filtering with histogram acceleration
    debug!("measure only node filtering with histogram acceleration");
    let only_full_acc = measure_one_query_part(index, query.clone(), filter, true, true, false);

    json!({
        "raw_point_filtering": raw_point_filtering,
        "point_filtering_with_node_acc": point_filtering_with_node_acc,
        "point_filtering_with_full_acc": point_filtering_with_full_acc,
        "only_node_acc": only_node_acc,
        "only_full_acc": only_full_acc,
    })
}


fn measure_one_query_part<I, Q>(
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

    let las_loader = I32LasReadWrite::new(true, 3);

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
    for points in nodes {
        nr_points += points.len();
    }
    let time_finish_load = Instant::now();

    json!({
        "nr_nodes": nr_nodes,
        "nr_points": nr_points,
        "query_time_seconds": (time_finish_query - time_start).as_secs_f64(),
        "load_time_seconds": (time_finish_load - time_finish_query).as_secs_f64(),
    })
}
