use crossbeam_channel::Receiver;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use lidarserv_common::geometry::grid::{LeveledGridCell, LodLevel};
use lidarserv_common::index::reader::OctreeReader;
use lidarserv_common::index::Octree;
use lidarserv_server::index::query::Query;
use log::info;
use nalgebra::min;
use pasture_core::containers::{BorrowedBuffer, InterleavedBuffer, OwningBuffer, VectorBuffer};
use pasture_io::base::PointReader;
use rand::seq::index::sample;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::{Arc, Mutex};
use std::thread::{self, spawn};
use std::time::{Duration, Instant};

use crate::MULTI_PROGRESS;

#[derive(Debug)]
struct Sample {
    insert: Instant,
    query: Option<(Instant, LodLevel)>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Stats {
    nr_points: usize,
    mean: f64,
    percentiles: Vec<(i32, f64)>,
}

pub fn measure_latency(
    index: &mut Octree,
    points: &mut impl PointReader,
    query: Query,
    pps: usize,
    sample_points_per_second: usize,
) -> anyhow::Result<serde_json::value::Value> {
    // Init
    let nr_points = points
        .get_metadata()
        .number_of_points()
        .expect("unknown number of points");

    // Progress bar
    let multi = MULTI_PROGRESS
        .get()
        .expect("MULTI_PROGRESS not initialized");
    let pb = multi.add(ProgressBar::new(nr_points as u64));
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>7}/{len:7} ({eta})",
        )
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
            write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
        })
        .progress_chars("#>-"),
    );

    // query thread
    let shared = Arc::new(Mutex::new(HashMap::new()));
    let shared2 = Arc::clone(&shared);
    let (exit_tx, exit_rx) = crossbeam_channel::unbounded();
    let reader = index.reader(query).unwrap();
    let query_thread_handle = spawn(move || query_thread(reader, exit_rx, shared2));

    // Prepare insertion
    let mut writer = index.writer();
    let mut read_pos = 0;
    let start_time = Instant::now();
    let mut last_update = Instant::now();
    let mut total_nr_sample_points = 0;

    // Insertion loop
    while read_pos < nr_points {
        // how many points to read?
        let now = Instant::now();
        let read_until = ((now - start_time).as_secs_f64() * pps as f64) as usize;
        let nr_points_left = nr_points - read_pos;
        let nr_points_insert = min(read_until - read_pos, nr_points_left);
        if nr_points_insert > pps * 5 {
            info!("Too slow to replay points at {pps} points per second. Aborting.");
            break;
        }
        let points_waiting = writer.nr_points_waiting();
        if points_waiting > pps * 5 {
            info!("Too slow to index points at {pps} points per second. Aborting.");
            break;
        }

        // read correct number of points
        let points_buffer = points.read::<VectorBuffer>(nr_points_insert)?;
        let mut other_points_buffer =
            VectorBuffer::with_capacity(points_buffer.len(), index.point_layout().clone());
        assert_eq!(
            points_buffer.point_layout().size_of_point_entry(),
            other_points_buffer.point_layout().size_of_point_entry()
        );
        unsafe {
            // safety: index was crated so that its layout matches the file layout.
            other_points_buffer.push_points(points_buffer.get_point_range_ref(0..nr_points_insert))
        };

        // choose sample points
        let next_total_nr_sample_points =
            ((now - start_time).as_secs_f64() * sample_points_per_second as f64) as usize;
        let nr_sample_points = min(
            next_total_nr_sample_points - total_nr_sample_points,
            other_points_buffer.len(),
        );
        total_nr_sample_points += nr_sample_points;
        let mut rng = thread_rng();
        let indices = sample(&mut rng, other_points_buffer.len(), nr_sample_points);
        {
            let mut chosen = shared.lock().unwrap();
            let insert_time = Instant::now();
            for i in indices {
                let mut data =
                    vec![0; other_points_buffer.point_layout().size_of_point_entry() as usize];
                other_points_buffer.get_point(i, &mut data);
                chosen.insert(
                    data,
                    Sample {
                        insert: insert_time,
                        query: None,
                    },
                );
            }
        }

        // insert
        writer.insert(&other_points_buffer);

        // update state
        read_pos += nr_points_insert;

        thread::sleep(Duration::from_secs_f64(0.01));

        // Update progress bar and check timeout
        if (now - last_update) > Duration::from_secs_f64(0.1) {
            last_update = now;
            pb.set_position(read_pos as u64);
        }
    }
    drop(pb);

    // Finalize
    drop(writer);

    // stop querying
    exit_tx.send(()).ok();
    query_thread_handle.join().unwrap();

    // analyze result
    if read_pos != nr_points {
        return Ok(json!({
            "error": "Too slow.",
        }));
    }
    let mut durations_any_lod = vec![];
    let mut durations_by_lod: HashMap<LodLevel, Vec<Duration>> = HashMap::new();
    let shared = shared.lock().unwrap();
    for sample in shared.values() {
        if let Some((read_time, lod_level)) = sample.query {
            let duration = read_time - sample.insert;
            durations_any_lod.push(duration);
            durations_by_lod
                .entry(lod_level)
                .or_default()
                .push(duration);
        }
    }

    Ok(json!({
        "nr_samples": shared.len(),
        "nr_samples_positive": durations_any_lod.len(),
        "stats": calculate_stats(durations_any_lod),
        "stats_by_lod": durations_by_lod.into_iter().map(|(lod, durations)| (lod.to_string(), calculate_stats(durations))).collect::<HashMap<_, _>>(),
    }))
}

fn query_thread(
    mut reader: OctreeReader,
    exit: Receiver<()>,
    shared: Arc<Mutex<HashMap<Vec<u8>, Sample>>>,
) {
    loop {
        let should_exit = reader.wait_update_or(&exit);
        if should_exit.is_some() {
            break;
        }
        if let Some((cell, points)) = reader.load_one() {
            analyze_node(cell, points, &shared);
        }
        if let Some((cell, points)) = reader.reload_one() {
            analyze_node(cell, points, &shared);
        }
        if reader.remove_one().is_some() {
            unreachable!("The query is never changed, so nodes can just be added");
        }
    }
}

fn analyze_node(
    cell: LeveledGridCell,
    points: VectorBuffer,
    shared: &Mutex<HashMap<Vec<u8>, Sample>>,
) {
    let mut lock = shared.lock().unwrap();
    let now = Instant::now();
    for i in 0..points.len() {
        let point = points.get_point_ref(i);
        if let Some(sample) = lock.get_mut(point) {
            if sample.query.is_none() {
                sample.query = Some((now, cell.lod))
            }
        }
    }
}

fn calculate_stats(mut durations: Vec<Duration>) -> Option<Stats> {
    if durations.is_empty() {
        return None;
    }
    let sum = durations.iter().map(|d| d.as_secs_f64()).sum::<f64>();
    let mean = sum / durations.len() as f64;
    durations.sort();

    let percentiles = (0..=100)
        .step_by(5)
        .map(|percentile| {
            let index = (percentile as f64) / 100.0 * (durations.len() - 1) as f64;
            let i1 = index as usize;
            let i2 = i1 + 1;
            let value = if i2 >= durations.len() {
                durations[i1].as_secs_f64()
            } else {
                let d1 = durations[i1].as_secs_f64();
                let d2 = durations[i2].as_secs_f64();
                let interpol = index - (i1 as f64);
                d2 * interpol + d1 * (1.0 - interpol)
            };
            (percentile, value)
        })
        .collect();

    Some(Stats {
        nr_points: durations.len(),
        mean,
        percentiles,
    })
}
