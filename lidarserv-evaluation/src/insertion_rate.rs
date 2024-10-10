use crate::settings::SingleInsertionRateMeasurement;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use lidarserv_common::index::Octree;
use log::info;
use nalgebra::min;
use pasture_core::containers::{
    BorrowedBuffer, InterleavedBuffer, MakeBufferFromLayout, OwningBuffer, VectorBuffer,
};
use pasture_core::layout::PointLayout;
use pasture_io::base::PointReader;
use serde_json::json;
use std::fmt::Write;
use std::thread;
use std::time::{Duration, Instant};

pub fn measure_insertion_rate(
    index: &mut Octree,
    points: &mut impl PointReader,
    settings: &SingleInsertionRateMeasurement,
    timeout_seconds: u64,
) -> anyhow::Result<serde_json::value::Value> {
    // Init
    let nr_points = points
        .get_metadata()
        .number_of_points()
        .expect("unknown number of points");
    let target_point_pressure = settings.target_point_pressure;
    info!(
        "Inserting {} points into index. Timeout: {} seconds",
        nr_points, timeout_seconds
    );

    // Progress bar
    let pb = ProgressBar::new(nr_points as u64);
    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>7}/{len:7} [{msg}] ({eta})")
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
    while read_pos < nr_points {
        let backlog = writer.nr_points_waiting();
        if backlog < target_point_pressure {
            if backlog == 0 {
                nr_times_to_slow += 1;
            }
            let nr_points_left = nr_points - read_pos;
            let nr_points_insert = min(
                min(target_point_pressure - backlog, nr_points_left),
                500_000,
            );
            let points_buffer = points.read::<VectorBuffer>(nr_points_insert)?;
            let mut other_points_buffer =
                VectorBuffer::with_capacity(points_buffer.len(), index.point_layout().clone());
            assert_eq!(
                points_buffer.point_layout().size_of_point_entry(),
                other_points_buffer.point_layout().size_of_point_entry()
            );
            unsafe {
                // safety: index was crated so that its layout matches the file layout.
                other_points_buffer
                    .push_points(points_buffer.get_point_range_ref(0..nr_points_insert))
            };
            writer.insert(&other_points_buffer);
            read_pos += nr_points_insert;
        }
        thread::sleep(Duration::from_secs_f64(0.005));
        i += 1;

        // Update progress bar
        if i % 100 == 0 {
            pb.set_position(read_pos as u64);
            let current_pps = read_pos as f64 / time_start.elapsed().as_secs_f64();
            pb.set_message(format!(
                "{} pps, backlog: {}",
                current_pps as u64,
                writer.nr_points_waiting()
            ));
        }

        // Handle timeout
        if i % 1000 == 0
            && Instant::now().duration_since(time_start) > Duration::from_secs(timeout_seconds)
        {
            info!(
                "Insertion rate measurement timed out after {} seconds",
                timeout_seconds
            );
            break;
        }
    }
    if read_pos == nr_points {
        pb.finish_with_message("Finished");
    } else {
        pb.finish_with_message("Timed out");
    }

    // Finalize
    let finished_at = Instant::now();
    drop(writer);
    let finalize_duration = Instant::now().duration_since(finished_at);
    let nr_points = read_pos;
    let duration = finished_at.duration_since(time_start);
    let pps = nr_points as f64 / (duration + finalize_duration).as_secs_f64();

    Ok(json!({
        "settings": settings,
        "duration_seconds": duration.as_secs_f64(),
        "duration_cleanup_seconds": finalize_duration.as_secs_f64(),
        "nr_points": nr_points,
        "insertion_rate_points_per_sec": pps,
        "nr_times_to_slow": nr_times_to_slow - 1,       // minus one, because before the first insert call, the writer will always be empty.
    }))
}
