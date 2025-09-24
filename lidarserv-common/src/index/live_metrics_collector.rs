use crossbeam_channel::{Receiver, Sender};
use serde::de::{Error, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::VecDeque;
use std::fmt::Formatter;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use std::{io, thread};
use sysinfo::{CpuRefreshKind, DiskRefreshKind, Disks, MemoryRefreshKind, RefreshKind, System};
use thiserror::Error;
use tracy_client::plot;

pub struct LiveMetricsCollector {
    thread: Option<JoinHandle<io::Result<()>>>,
    started_at: Instant,
    sender: Option<Sender<MetricMessage>>,
    metric_nr_incoming_tasks: AtomicUsize,
    metric_nr_incoming_points: AtomicUsize,
    metric_nr_cached_nodes: AtomicUsize,
    metric_nr_active_nodes: AtomicUsize,
    metric_nr_points_per_second: Mutex<PointsPerSecondMetric>,
    system_info: Mutex<SysinfoMetrics>,
}

struct PointsPerSecondMetric {
    value: usize,
    add_points_history: VecDeque<(Instant, usize)>,
}

struct SysinfoMetrics {
    /// sysinfo for memory and cpu usage
    system: System,

    /// sysinfo for disk space
    disks: Disks,

    /// Data path where the disk usage will be measured.
    path: Option<PathBuf>,

    /// The last position at which the disk containing the path was found.
    ///
    /// Assuming that the ordering of disks returned by the sysinfo crate is
    /// pretty stable, we can reuse this index and avoid checking all disks
    /// each time the metrics are snapshotted.
    last_disk_index: Option<usize>,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: Duration,
    pub nr_incoming_tasks: u64,
    pub nr_incoming_points: u64,
    pub nr_cached_nodes: u64,
    pub nr_active_nodes: u64,
    pub nr_points_per_second: u64,
    pub memory_usage: u64,
    pub memory_total: u64,
    pub swap_memory_usage: u64,
    pub swap_memory_total: u64,
    pub cpu_usage: f32,
    pub disk_used: u64,
    pub disk_total: u64,
}

#[derive(Debug, Error)]
pub enum MetricsError {
    #[error(transparent)]
    IO(#[from] io::Error),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum MetricName {
    #[serde(rename = "t")]
    NrIncomingTasks,
    #[serde(rename = "p")]
    NrIncomingPoints,
    #[serde(rename = "a")]
    NrPointsAdded,
    #[serde(rename = "n")]
    NrNodesCached,
    #[serde(rename = "m")]
    NrNodesRecentlyAccessed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetricMessage {
    #[serde(
        rename = "t",
        serialize_with = "serialize_duration_as_f64",
        deserialize_with = "deserialize_duration_from_f64"
    )]
    time_stamp: Duration,
    #[serde(rename = "m")]
    metric: MetricName,
    #[serde(rename = "v")]
    value: usize,
}

fn serialize_duration_as_f64<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_f64(duration.as_secs_f64())
}

fn deserialize_duration_from_f64<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    struct F64DurationVisitor;
    impl Visitor<'_> for F64DurationVisitor {
        type Value = Duration;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("a f64, indicating the number of passed seconds")
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            if v >= 0 {
                self.visit_u64(v as u64)
            } else {
                Err(E::invalid_value(
                    Unexpected::Signed(v),
                    &"a positive number",
                ))
            }
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(Duration::from_secs(v))
        }

        fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(Duration::from_secs_f64(v))
        }
    }
    deserializer.deserialize_f64(F64DurationVisitor)
}

impl LiveMetricsCollector {
    /// Builds a new Metrics collector, that writes all metrics passed to [Self::metric] into the given file.
    pub fn new_file_backed_collector(file_name: &Path) -> Result<Self, MetricsError> {
        // open file for writing
        let file = File::create(file_name)?;
        let file = BufWriter::new(file);

        // channel to send metric messages to the writer thread
        let (sender, receiver) = crossbeam_channel::unbounded();

        // start thread for metrics
        let thread = thread::spawn(move || Self::collector_thread(file, receiver));

        let mut collector = LiveMetricsCollector::default();
        collector.thread = Some(thread);
        collector.sender = Some(sender);
        Ok(collector)
    }

    /// Builds a new Metrics collector, that just discards any metric that is passed to its [Self::metric] function.
    pub fn new_discarding_collector() -> LiveMetricsCollector {
        Self::default()
    }

    #[inline]
    pub fn metric(&self, metric: MetricName, value: usize) {
        if let Some(s) = self.sender.as_ref() {
            s.send(MetricMessage {
                time_stamp: Instant::now().duration_since(self.started_at),
                metric,
                value,
            })
            .unwrap(); // unwrap: Channel must be still open - it will only be closed in drop().
        }

        match metric {
            MetricName::NrIncomingTasks => {
                plot!("Task queue length", value as f64);
                self.metric_nr_incoming_tasks
                    .store(value, Ordering::Relaxed);
            }
            MetricName::NrIncomingPoints => {
                plot!("Task queue length in points", value as f64);
                self.metric_nr_incoming_points
                    .store(value, Ordering::Relaxed);
            }
            MetricName::NrNodesCached => {
                plot!("Page LRU Cache size", value as f64);
                self.metric_nr_cached_nodes.store(value, Ordering::Relaxed);
            }
            MetricName::NrNodesRecentlyAccessed => {
                plot!("Page LRU Cache recently used", value as f64);
                self.metric_nr_active_nodes.store(value, Ordering::Relaxed);
            }
            MetricName::NrPointsAdded => {
                let now = Instant::now();
                let pps = self.incoming_points_metric(now, value);
                plot!("Incoming points per second", pps as f64);
            }
        }
    }

    fn collector_thread(mut file: impl Write, receiver: Receiver<MetricMessage>) -> io::Result<()> {
        for metric in receiver {
            match ciborium::ser::into_writer(&metric, &mut file) {
                Ok(()) => {}
                Err(ciborium::ser::Error::Io(io)) => return Err(io),
                Err(ciborium::ser::Error::Value(_)) => unreachable!(),
            };
        }
        Ok(())
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let now = Instant::now();
        let mut result = MetricsSnapshot {
            timestamp: now.duration_since(self.started_at),
            nr_incoming_tasks: self.metric_nr_incoming_tasks.load(Ordering::Relaxed) as u64,
            nr_incoming_points: self.metric_nr_incoming_points.load(Ordering::Relaxed) as u64,
            nr_cached_nodes: self.metric_nr_cached_nodes.load(Ordering::Relaxed) as u64,
            nr_active_nodes: self.metric_nr_active_nodes.load(Ordering::Relaxed) as u64,
            nr_points_per_second: self.incoming_points_metric(now, 0) as u64,
            memory_usage: 0,
            memory_total: 0,
            swap_memory_usage: 0,
            swap_memory_total: 0,
            cpu_usage: 0.0,
            disk_used: 0,
            disk_total: 0,
        };
        {
            // set memory and cpu metrics
            let mut sysinfo = self.system_info.lock().unwrap();
            sysinfo
                .system
                .refresh_specifics(Self::sysinfo_refresh_kind());
            result.cpu_usage = sysinfo.system.global_cpu_usage();
            result.memory_total = sysinfo.system.total_memory();
            result.memory_usage = sysinfo.system.used_memory();
            result.swap_memory_total = sysinfo.system.total_swap();
            result.swap_memory_usage = sysinfo.system.used_swap();

            // set disk space metrics (if a path is set for this)
            let SysinfoMetrics {
                ref mut disks,
                ref mut path,
                ref mut last_disk_index,
                ..
            } = *sysinfo;
            if let Some(data_path) = path {
                let disks = disks.list_mut();

                // check if the last_disk_index is still correct.
                // (most likely the case - unset it otherwise)
                if let Some(index) = *last_disk_index {
                    if let Some(disk) = disks.get_mut(index) {
                        if !data_path.starts_with(disk.mount_point()) {
                            *last_disk_index = None;
                        }
                    } else {
                        *last_disk_index = None;
                    }
                }

                // find the disk that contains the path
                if last_disk_index.is_none() {
                    for (index, disk) in disks.iter_mut().enumerate() {
                        if data_path.starts_with(disk.mount_point()) {
                            *last_disk_index = Some(index);
                            break;
                        }
                    }
                }

                // refresh that disk and calculate metrics.
                if let Some(index) = *last_disk_index {
                    let disk = &mut disks[index];
                    disk.refresh_specifics(DiskRefreshKind::nothing().with_storage());
                    result.disk_total = disk.total_space();
                    result.disk_used = disk.total_space() - disk.available_space();
                } else {
                    // if finally no matching disk was found, we disable this metric by unsetting the path again.
                    *path = None;
                }
            }
        }
        result
    }

    pub fn set_path(&self, path: PathBuf) {
        let Ok(abspath) = std::path::absolute(path) else {
            return;
        };
        let mut sysinfo = self.system_info.lock().unwrap();
        sysinfo.path = Some(abspath);
        sysinfo.last_disk_index = None;
    }

    fn incoming_points_metric(&self, now: Instant, add_incoming_points: usize) -> usize {
        let mut metric = self.metric_nr_points_per_second.lock().unwrap();
        if add_incoming_points > 0 {
            metric
                .add_points_history
                .push_back((now, add_incoming_points));
            metric.value += add_incoming_points;
        }
        while let Some((time, count)) = metric.add_points_history.front()
            && now.duration_since(*time) >= Duration::from_secs(1)
        {
            metric.value -= *count;
            metric.add_points_history.pop_front();
        }
        metric.value
    }

    #[inline]
    fn sysinfo_refresh_kind() -> RefreshKind {
        RefreshKind::nothing()
            .with_memory(MemoryRefreshKind::nothing().with_ram().with_swap())
            .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
    }
}

impl Drop for LiveMetricsCollector {
    fn drop(&mut self) {
        // close channel to make the thread stop
        if let Some(s) = self.sender.take() {
            drop(s)
        }

        // wait for thread to terminate
        if let Some(t) = self.thread.take() {
            t.join().unwrap().unwrap()
        }
    }
}

impl Default for LiveMetricsCollector {
    fn default() -> Self {
        Self {
            thread: None,
            started_at: Instant::now(),
            sender: None,
            metric_nr_incoming_tasks: AtomicUsize::new(0),
            metric_nr_incoming_points: AtomicUsize::new(0),
            metric_nr_cached_nodes: AtomicUsize::new(0),
            metric_nr_active_nodes: AtomicUsize::new(0),
            metric_nr_points_per_second: Mutex::new(PointsPerSecondMetric {
                value: 0,
                add_points_history: VecDeque::new(),
            }),
            system_info: Mutex::new(SysinfoMetrics {
                system: System::new_with_specifics(Self::sysinfo_refresh_kind()),
                disks: Disks::new_with_refreshed_list_specifics(
                    DiskRefreshKind::nothing().with_storage(),
                ),
                path: None,
                last_disk_index: None,
            }),
        }
    }
}
