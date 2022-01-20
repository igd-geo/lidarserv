use crate::{Config, Point, PointIdAttribute};
use lidarserv_common::geometry::points::PointType;
use lidarserv_common::index::{Index, Node, NodeId, Reader, Writer};
use lidarserv_common::las::I32LasReadWrite;
use lidarserv_common::query::Query;
use serde_json::json;
use std::cmp::{min, Ordering};
use std::collections::HashMap;
use std::io::Cursor;
use std::thread;
use std::thread::sleep;
use std::time::{Duration, Instant};

pub fn measure_latency<I, Q>(
    index: I,
    points: &[Point],
    query: Q,
    config: &Config,
) -> serde_json::value::Value
where
    I: Index<Point>,
    I::Reader: Send + 'static,
    Q: Query + Send + Sync + 'static,
{
    // query thread
    let reader = index.reader(query);
    let (queries_sender, queries_receiver) = crossbeam_channel::unbounded(); // we will never actually push a new query. but the channel is still used to tell the queryer when to stop.
    let rt = thread::spawn(move || read_thread::<I>(reader, queries_receiver));

    // do the insertions in the current thread
    let insertion_times = insertion_thread::<I>(index.writer(), points, config);

    // end query thread
    drop(queries_sender);
    let receive_times = rt.join().unwrap();

    // calculate per-lod mean and median
    let mut delays = Vec::new();
    for rt in &receive_times {
        let mut lod_delays = Vec::new();
        for (point_index, received_at) in rt {
            let delay = *received_at - insertion_times[*point_index];
            lod_delays.push(delay.as_secs_f64())
        }
        delays.push(lod_delays);
    }
    let per_lod_stats = delays
        .into_iter()
        .map(|mut point_latencies| {
            if !point_latencies.is_empty() {
                point_latencies.sort_by(|a, b| {
                    if a < b {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    }
                });
                let median = point_latencies[point_latencies.len() / 2];
                let mean =
                    point_latencies.iter().cloned().sum::<f64>() / point_latencies.len() as f64;
                let quantiles = quantiles(&point_latencies);
                json!({
                    "mean_latency_seconds": mean,
                    "median_latency_seconds": median,
                    "nr_points": point_latencies.len(),
                    "quantiles": quantiles
                })
            } else {
                json!({
                    "nr_points": 0
                })
            }
        })
        .collect::<Vec<_>>();

    // calculate overall latency stats
    let mut delays = Vec::new();
    'point_loop: for (point_index, insertion_time) in insertion_times.into_iter().enumerate() {
        for t in &receive_times {
            if let Some(received_at) = t.get(&point_index) {
                let delay = *received_at - insertion_time;
                delays.push(delay.as_secs_f64());
                continue 'point_loop;
            }
        }
    }
    delays.sort_by(|a, b| {
        if a < b {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    });
    let overall_stats = if !delays.is_empty() {
        let median = delays[delays.len() / 2];
        let mean = delays.iter().cloned().sum::<f64>() / delays.len() as f64;
        let quantiles = quantiles(&delays);
        json!({
            "mean_latency_seconds": mean,
            "median_latency_seconds": median,
            "quantiles": quantiles,
            "nr_points": delays.len()
        })
    } else {
        json!({
            "nr_points": 0
        })
    };

    json!({
        "all_lods": overall_stats,
        "per_lod_level": per_lod_stats,
    })
}

pub fn insertion_thread<I>(mut writer: I::Writer, points: &[Point], config: &Config) -> Vec<Instant>
where
    I: Index<Point>,
{
    let points_per_second: usize = config.pps;
    let frames_per_second: usize = config.fps;
    let mut read_pos = 0;
    let started_at = Instant::now();
    let mut last_insert = started_at;
    let mut insertion_times = vec![started_at; points.len()];

    while read_pos < points.len() {
        // wait for next insert
        let next_insert = last_insert + Duration::from_secs_f64(1.0 / frames_per_second as f64);
        let mut now = Instant::now();
        if next_insert > now {
            let sleep_time = next_insert - now;
            sleep(sleep_time);
            now = next_insert;
        }
        last_insert = now;

        // get points to insert
        let read_to = min(
            ((now - started_at).as_secs_f64() * points_per_second as f64) as usize,
            points.len(),
        );
        let points = points[read_pos..read_to].to_vec();
        read_pos = read_to;

        // remember insertion time
        for point in &points {
            insertion_times[point.attribute::<PointIdAttribute>().0] = now;
        }

        // insert
        writer.insert(points);
    }

    insertion_times
}

pub fn read_thread<I>(
    mut reader: I::Reader,
    mut queries: crossbeam_channel::Receiver<Box<dyn Query + Send + Sync>>,
) -> Vec<HashMap<usize, Instant>>
where
    I: Index<Point>,
{
    let max_lod_level = 10;
    let mut receive_times = Vec::new();
    for _ in 0..=max_lod_level {
        let points = HashMap::new();
        receive_times.push(points);
    }
    let las_loader = I32LasReadWrite::new(true);

    while reader.blocking_update(&mut queries) {
        reader.remove_one();
        if let Some((node_id, node)) = reader.load_one() {
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

            let now = Instant::now();
            let lod_index = node_id.lod().level() as usize;
            for points in point_chunks {
                for point in points {
                    let index: usize = point.attribute::<PointIdAttribute>().0;
                    receive_times[lod_index].entry(index).or_insert(now);
                }
            }
        }
        if let Some((_, repl)) = reader.update_one() {
            let now = Instant::now();
            for (node_id, node) in repl {
                let points = node.las_files().into_iter().map(|data| {
                    las_loader
                        .read_las(Cursor::new(data.as_ref()))
                        .map(|las| las.points as Vec<Point>)
                        .unwrap_or_else(|_| Vec::new())
                });
                let lod_index = node_id.lod().level() as usize;
                for points in points {
                    for point in points {
                        let index: usize = point.attribute::<PointIdAttribute>().0;
                        receive_times[lod_index].entry(index).or_insert(now);
                    }
                }
            }
        }
    }

    receive_times
}

fn quantiles(data: &[f64]) -> serde_json::Value {
    let items = [0, 10, 20, 25, 30, 40, 50, 60, 70, 75, 80, 90, 100]
        .into_iter()
        .map(|percent| {
            json!({
                "percent": percent,
                "value": quantile(data, percent as f64 / 100.0)})
        })
        .collect::<Vec<_>>();
    json!(items)
}

fn quantile(data: &[f64], frac: f64) -> f64 {
    let index_float = frac * (data.len() - 1) as f64;
    let index_l = (index_float as usize).clamp(0, data.len() - 1);
    let index_r = (index_l + 1).clamp(0, data.len() - 1);
    let val_l = data[index_l];
    let val_r = data[index_r];
    let weight = index_float - index_l as f64;
    val_r * weight + val_l * (1.0 - weight)
}
