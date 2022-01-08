use crate::queries::{preset_query_1, preset_query_2, preset_query_3};
use crate::Point;
use lidarserv_common::geometry::position::{I32CoordinateSystem, I32Position};
use lidarserv_common::index::{Index, Node, Reader};
use lidarserv_common::las::{I32LasReadWrite, LasReadWrite};
use lidarserv_common::query::Query;
use serde_json::json;
use std::io::Cursor;
use std::time::Instant;

pub fn measure_query_performance<I>(mut index: I) -> serde_json::value::Value
where
    I: Index<Point, I32CoordinateSystem>,
{
    json!({
        "query_1": measure_one_query(&mut index, preset_query_1()),
        "query_2": measure_one_query(&mut index, preset_query_2()),
        "query_3": measure_one_query(&mut index, preset_query_3()),
    })
}

fn measure_one_query<I, Q>(index: &mut I, query: Q) -> serde_json::value::Value
where
    I: Index<Point, I32CoordinateSystem>,
    Q: Query<I32Position, I32CoordinateSystem> + Send + Sync + 'static,
{
    index.flush().unwrap();
    let las_loader = I32LasReadWrite::new(true);

    let time_start = Instant::now();
    let mut r = index.reader(query);
    let mut nodes = Vec::new();
    while let Some((_node_id, node)) = r.load_one() {
        nodes.push(node);
    }
    let time_finish_query = Instant::now();
    let nr_nodes = nodes.len();
    let mut nr_points = 0;
    for node in nodes {
        let point_chunks: Vec<_> = node
            .las_files()
            .into_iter()
            .map(|data| {
                las_loader
                    .read_las(Cursor::new(data.as_ref()))
                    .map(|las| las.points as Vec<Point>)
                    .unwrap_or_else(|_| Vec::new())
            })
            .collect();
        for points in point_chunks {
            nr_points += points.len();
        }
    }
    let time_finish_load = Instant::now();

    json!({
        "nr_nodes": nr_nodes,
        "nr_points": nr_points,
        "query_time_seconds": (time_finish_query - time_start).as_secs_f64(),
        "load_time_seconds": (time_finish_load - time_finish_query).as_secs_f64(),
    })
}
