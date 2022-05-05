use crossbeam_channel::{Receiver, Sender};
use serde::de::{Error, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::Formatter;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use std::{io, thread};
use thiserror::Error;

pub struct LiveMetricsCollector {
    thread: Option<JoinHandle<io::Result<()>>>,
    started_at: Instant,
    sender: Option<Sender<MetricMessage>>,
}

#[derive(Debug, Error)]
pub enum MetricsError {
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum MetricName {
    #[serde(rename = "t")]
    NrIncomingTasks,
    #[serde(rename = "p")]
    NrIncomingPoints,
    #[serde(rename = "a")]
    NrPointsAdded,
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
    value: f64,
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
    impl<'de> Visitor<'de> for F64DurationVisitor {
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
        let started_at = Instant::now();
        let thread = thread::spawn(move || Self::collector_thread(file, receiver));

        let collector = LiveMetricsCollector {
            thread: Some(thread),
            sender: Some(sender),
            started_at,
        };
        Ok(collector)
    }

    /// Builds a new Metrics collector, that just discards any metric that is passed to its [Self::metric] function.
    pub fn new_discarding_collector() -> LiveMetricsCollector {
        LiveMetricsCollector {
            thread: None,
            started_at: Instant::now(),
            sender: None,
        }
    }

    #[inline]
    pub fn metric(&self, metric: MetricName, value: f64) {
        if let Some(s) = self.sender.as_ref() {
            s.send(MetricMessage {
                time_stamp: Instant::now().duration_since(self.started_at),
                metric,
                value,
            })
            .unwrap(); // unwrap: Channel must be still open - it will only be closed in drop().
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
