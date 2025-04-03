use lidarserv_common::index::{Octree, reader::QueryConfig};
use lidarserv_server::index::query::Query;
use log::debug;
use pasture_core::containers::BorrowedBuffer;
use serde_json::json;
use std::time::Instant;

use crate::settings::QueryFiltering;

pub fn measure_one_query(
    index: &mut Octree,
    query_str: &str,
    filtering: QueryFiltering,
) -> serde_json::value::Value {
    debug!("Flushing index");
    index.flush().unwrap();

    let query = match Query::parse(query_str) {
        Ok(q) => q,
        Err(e) => {
            return json!({
                "error": format!("{e}"),
                "detail": format!("{e:#?}")
            });
        }
    };

    debug!("Executing query");
    let time_start = Instant::now();
    let init_query = Query::parse("empty").unwrap();
    let mut r = match index.reader(init_query) {
        Ok(r) => r,
        Err(e) => {
            return json!({
                "error": format!("{e}")
            });
        }
    };
    match r.set_query(
        query,
        QueryConfig {
            enable_attribute_index: filtering != QueryFiltering::NodeFilteringWithoutAttributeIndex,
            enable_point_filtering: filtering == QueryFiltering::PointFiltering,
        },
    ) {
        Ok(_) => {}
        Err(e) => {
            return json!({
                "error": format!("{e}")
            });
        }
    }

    let mut nodes_sizes = Vec::new();
    while let Some((_node_id, node)) = r.load_one() {
        nodes_sizes.push(node.len());
    }
    let time_finish_query = Instant::now();
    let nr_nodes = nodes_sizes.len();
    let mut nr_points = 0;
    let mut nr_non_empty_nodes = 0;
    for nr_points_in_node in nodes_sizes {
        nr_points += nr_points_in_node;
        if nr_points_in_node > 0 {
            nr_non_empty_nodes += 1;
        }
    }
    let time_finish_load = Instant::now();

    json!({
        "nr_nodes": nr_nodes,
        "nr_non_empty_nodes": nr_non_empty_nodes,
        "nr_points": nr_points,
        "query_time_seconds": (time_finish_query - time_start).as_secs_f64(),
        "load_time_seconds": (time_finish_load - time_finish_query).as_secs_f64(),
    })
}
