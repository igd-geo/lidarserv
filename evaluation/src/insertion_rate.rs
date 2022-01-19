use crate::{Config, Point};
use lidarserv_common::index::{Index, Writer};
use nalgebra::min;
use serde_json::json;
use std::thread;
use std::time::{Duration, Instant};

pub fn measure_insertion_rate<I>(
    index: &mut I,
    points: &[Point],
    config: &Config,
) -> (serde_json::value::Value, f64)
where
    I: Index<Point>,
{
    let target_point_pressure: usize = config.target_point_pressure;
    let mut writer = index.writer();
    let mut read_pos = 0;
    let time_start = Instant::now();
    let mut nr_times_to_slow = 0;
    let mut i = 0;
    while read_pos < points.len() {
        let backlog = writer.backlog_size();
        if backlog < target_point_pressure {
            if backlog == 0 {
                nr_times_to_slow += 1;
            }
            let nr_points_left = points.len() - read_pos;
            let nr_points_insert = min(target_point_pressure - backlog, nr_points_left);
            let read_to = read_pos + nr_points_insert;
            let insert_points = points[read_pos..read_to].to_vec();
            writer.insert(insert_points);
            read_pos = read_to;
        }
        thread::sleep(Duration::from_secs_f64(0.005));
        i += 1;
        if i % 1000 == 0 && Instant::now().duration_since(time_start) > Duration::from_secs(60 * 5)
        {
            // has been running since more than 5 minutes
            break;
        }
    }
    let finished_at = Instant::now();
    drop(writer);
    let finalize_duration = Instant::now().duration_since(finished_at);
    let nr_points = read_pos;
    let duration = finished_at.duration_since(time_start);
    let pps = nr_points as f64 / (duration + finalize_duration).as_secs_f64();
    (
        json!({
            "duration_seconds": duration.as_secs_f64(),
            "duration_cleanup_seconds": finalize_duration.as_secs_f64(),
            "nr_points": nr_points,
            "insertion_rate_points_per_sec": pps,
            "nr_times_to_slow": nr_times_to_slow - 1,       // minus one, because before the first insert call, the writer will always be empty.
        }),
        pps,
    )
}
