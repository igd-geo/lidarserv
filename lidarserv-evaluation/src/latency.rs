use chrono::Utc;
use crossbeam_channel::Receiver;
use itertools::Itertools;
use lidarserv_common::geometry::grid::{LeveledGridCell, LodLevel};
use lidarserv_common::geometry::position::PositionComponentType;
use lidarserv_common::index::Octree;
use lidarserv_common::index::attribute_index::AttributeIndex;
use lidarserv_common::index::reader::{OctreeReader, QueryConfig};
use lidarserv_common::query::{ExecutableQuery, Query as QueryTrait, QueryContext};
use lidarserv_server::index::query::Query;
use log::info;
use pasture_core::containers::{BorrowedBuffer, InterleavedBuffer, OwningBuffer, VectorBuffer};
use pasture_core::layout::PointLayout;
use pasture_io::base::PointReader;
use rand::rng;
use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self};
use std::time::{Duration, Instant};

#[derive(Debug, Serialize, Deserialize)]
struct Stats {
    nr_points: usize,
    mean: f64,
    percentiles: Vec<(i32, f64)>,
}

const NR_WAITING_POINTS_ABORT_THRESHOLD: usize = 191739611;

pub fn measure_latency(
    index: &mut Octree,
    points: &mut (impl PointReader + Send),
    query: Query,
    query_config: QueryConfig,
    pps: usize,
    sample_points_per_second: usize,
) -> anyhow::Result<serde_json::value::Value> {
    thread::scope(|scope| -> Result<serde_json::Value, anyhow::Error> {
        let points_per_frame = pps / 10;
        let samples_per_frame = sample_points_per_second / 10;

        // predict index time
        if let Some(total_nr_points) = points.get_metadata().number_of_points() {
            let index_time_seconds = (total_nr_points as f64 / pps as f64) as u64;

            let hours = index_time_seconds / 3600;
            let minutes = (index_time_seconds % 3600) / 60;
            let seconds = index_time_seconds % 60;

            let time_now = Utc::now();
            let time_done = time_now + Duration::from_secs(index_time_seconds);
            info!("Input file contains {total_nr_points} points.");
            info!("At {pps} points/s, this will take {hours}h {minutes}m {seconds}s to index.");
            info!("The current time is {time_now}. We will be done at {time_done}.");
        }

        // start read thread
        let (points_tx, points_rx) = mpsc::sync_channel(100);
        let read_thread_handle = {
            let point_layout = index.point_layout().clone();
            scope.spawn(move || read_thread(points_tx, points_per_frame, points, point_layout))
        };

        // start sampling thread
        let (frames_tx, frames_rx) = mpsc::sync_channel(100);
        let shared_samples = Arc::new(Mutex::new(Default::default()));
        let sample_thread_handle = {
            let prepared_query = query.clone().prepare(&QueryContext {
                node_hierarchy: index.node_hierarchy(),
                point_hierarchy: index.point_hierarchy(),
                coordinate_system: index.coordinate_system(),
                component_type: PositionComponentType::from_layout(index.point_layout()),
                attribute_index: Arc::new(AttributeIndex::new()),
                point_layout: index.point_layout().clone(),
            })?;
            let shared = Arc::clone(&shared_samples);
            scope.spawn(move || {
                sample_thread(
                    points_rx,
                    frames_tx,
                    prepared_query,
                    samples_per_frame,
                    shared,
                );
            })
        };

        // start query thread
        let (exit_tx, exit_rx) = crossbeam_channel::unbounded();
        let query_thread_handle = {
            let mut query_reader = index.reader(query.clone()).unwrap();
            query_reader.set_query(query.clone(), query_config)?;
            let shared_samples = Arc::clone(&shared_samples);
            scope.spawn(move || query_thread(query_reader, exit_rx, shared_samples))
        };

        // wait for the buffers to fill up
        thread::sleep(Duration::from_secs(10));

        // insert points
        let mut nr_points_inserted = 0;
        let start_time = Instant::now();
        let mut insertion_times = HashMap::new();
        let mut writer = index.writer();
        for (frame_idx, points) in frames_rx {
            // wait for next frame
            {
                let next_frame_time =
                    start_time + Duration::from_secs_f64(nr_points_inserted as f64 / pps as f64);
                let now = Instant::now();
                if next_frame_time > now {
                    let wait_time = next_frame_time - now;
                    thread::sleep(wait_time);
                } else if now - next_frame_time > Duration::from_secs(5) {
                    info!("Too slow to replay points at {pps} points per second. Aborting.");
                    exit_tx.send(()).ok();
                    return Ok(json!({
                        "error": "Too slow to replay points.",
                    }));
                }
                nr_points_inserted += points.len();
            }

            // security abort to avoit running out of memory
            if writer.nr_points_waiting() > NR_WAITING_POINTS_ABORT_THRESHOLD {
                info!("Too slow to index points at {pps} points per second. Aborting.");
                exit_tx.send(()).ok();
                return Ok(json!({
                    "error": "Too slow to index points.",
                }));
            }

            // record insertion time
            let now = Instant::now();
            insertion_times.insert(frame_idx, now);

            // insert points
            writer.insert(&points);
        }

        // stop indexer
        drop(writer);

        // stop threads
        exit_tx.send(()).ok();
        query_thread_handle.join().unwrap();
        sample_thread_handle.join().unwrap();
        read_thread_handle.join().unwrap()?;

        // analyze result
        let mut durations_any_lod = vec![];
        let mut durations_by_lod: HashMap<LodLevel, Vec<Duration>> = HashMap::new();
        let shared = shared_samples.lock().unwrap();
        for sample in shared.values() {
            if let Some((read_time, lod_level)) = sample.query {
                let insertion_time = *insertion_times
                    .get(&sample.frame_idx)
                    .expect("missing insertion time");
                let duration = read_time - insertion_time;
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
    })
}

fn read_thread(
    points_tx: mpsc::SyncSender<VectorBuffer>,
    buf_size: usize,
    points: &mut impl PointReader,
    point_layout: PointLayout,
) -> Result<(), anyhow::Error> {
    loop {
        // read points
        let frame: VectorBuffer = points.read(buf_size)?;
        if frame.is_empty() {
            return Ok(());
        }

        // copy to buffer of correct layout
        // (This is a hack, that is needed because we are always calling
        // the position attribute "Position3d", while pasture calls it differently
        // depending on its point attribute data type.)
        let mut other_frame = VectorBuffer::with_capacity(frame.len(), point_layout.clone());
        assert_eq!(
            frame.point_layout().size_of_point_entry(),
            other_frame.point_layout().size_of_point_entry()
        );
        unsafe {
            // safety: index was crated so that its layout matches the file layout.
            other_frame.push_points(frame.get_point_range_ref(0..frame.len()))
        };

        // send
        match points_tx.send(other_frame) {
            Ok(_) => (),
            Err(_) => return Ok(()),
        }
    }
}

struct PointSample {
    frame_idx: usize,
    query: Option<(Instant, LodLevel)>,
}

fn sample_thread(
    points_rx: mpsc::Receiver<VectorBuffer>,
    points_tx: mpsc::SyncSender<(usize, VectorBuffer)>,
    query: Box<dyn ExecutableQuery>,
    samples_per_frame: usize,
    shared: Arc<Mutex<HashMap<Vec<u8>, PointSample>>>,
) {
    let mut rng = rng();
    for (frame_idx, frame) in points_rx.into_iter().enumerate() {
        // evaluate the query against each point
        let query_matches = query.matches_points(LodLevel::base(), &frame);
        let positive_indices = query_matches
            .into_iter()
            .enumerate()
            .filter_map(|(idx, matches)| if matches { Some(idx) } else { None })
            .collect_vec();

        // select random sample points from positive points
        let entries = positive_indices
            .choose_multiple(&mut rng, samples_per_frame.min(positive_indices.len()))
            .copied()
            .map(|idx| {
                (
                    frame.get_point_ref(idx).to_owned(),
                    PointSample {
                        frame_idx,
                        query: None,
                    },
                )
            })
            .collect_vec();

        // insert samples into shared storage
        {
            let mut shared_lock = shared.lock().unwrap();
            shared_lock.extend(entries);
        }

        // send to indexer
        match points_tx.send((frame_idx, frame)) {
            Ok(_) => (),
            Err(_) => return,
        }
    }
}

fn query_thread(
    mut reader: OctreeReader,
    exit: Receiver<()>,
    shared: Arc<Mutex<HashMap<Vec<u8>, PointSample>>>,
) {
    let mut insert_done = false;
    loop {
        if !insert_done {
            let should_exit = reader.wait_update_or(&exit);
            if should_exit.is_some() {
                insert_done = true;
            }
        }
        let mut should_exit = insert_done;
        if let Some((cell, points)) = reader.load_one() {
            should_exit = false;
            analyze_node(cell, points, &shared);
        }
        if let Some((cell, points)) = reader.reload_one() {
            should_exit = false;
            analyze_node(cell, points, &shared);
        }
        if reader.remove_one().is_some() {
            unreachable!("The query is never changed, so nodes can just be added");
        }
        if should_exit {
            break;
        }
    }
}

fn analyze_node(
    cell: LeveledGridCell,
    points: VectorBuffer,
    shared: &Mutex<HashMap<Vec<u8>, PointSample>>,
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
