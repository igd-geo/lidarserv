use std::fmt::Write;
use crate::settings::SingleInsertionRateMeasurement;
use crate::Point;
use lidarserv_common::index::{Index, Writer};
use nalgebra::min;
use serde_json::json;
use std::thread;
use std::time::{Duration, Instant};
use log::info;
use indicatif::{HumanDuration, MultiProgress, ProgressBar, ProgressState, ProgressStyle};

pub fn measure_insertion_rate<I>(
    index: &mut I,
    points: &[Point],
    settings: &SingleInsertionRateMeasurement,
) -> (serde_json::value::Value, f64)
where
    I: Index<Point>,
{
    // Init
    let target_point_pressure = settings.target_point_pressure;
    let estimated_duration = points.len() as f64 / target_point_pressure as f64;
    info!("Inserting {} points into index. Minimal duration: {} seconds", points.len(), estimated_duration);

    // Progress bar
    let pb = ProgressBar::new(points.len() as u64);
    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>7}/{len:7} [{msg}PPS] ({eta})")
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
        .progress_chars("#>-"));

    // Prepare insertion
    let mut writer = index.writer();
    let mut read_pos = 0;
    let time_start = Instant::now();
    let mut nr_times_to_slow = 0;
    let mut i = 0;

    // Insertion loop
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

        if i % 1000 == 0 {
            pb.set_position(read_pos as u64);
            let current_pps = read_pos as f64 / time_start.elapsed().as_secs_f64();
            pb.set_message(&format!("{:.0}", current_pps));
        }
        // if i % 1000 == 0 && Instant::now().duration_since(time_start) > Duration::from_secs(estimated_duration as u64)
        // {
        //     // if we are slower than the estimated duration, break
        //     info!("Insertion rate measurement took longer than estimated duration. Breaking.");
        //     break;
        // }
    }

    // Finalize
    let finished_at = Instant::now();
    drop(writer);
    let finalize_duration = Instant::now().duration_since(finished_at);
    let nr_points = read_pos;
    let duration = finished_at.duration_since(time_start);
    let pps = nr_points as f64 / (duration + finalize_duration).as_secs_f64();
    pb.finish_with_message("All points inserted");

    (
        json!({
            "settings": settings,
            "duration_seconds": duration.as_secs_f64(),
            "duration_cleanup_seconds": finalize_duration.as_secs_f64(),
            "nr_points": nr_points,
            "insertion_rate_points_per_sec": pps,
            "nr_times_to_slow": nr_times_to_slow - 1,       // minus one, because before the first insert call, the writer will always be empty.
        }),
        pps,
    )
}