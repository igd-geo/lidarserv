use default_functions_derive::DefaultFunctions;
use lidarserv_common::index::octree::writer::TaskPriorityFunction;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::vec::IntoIter;

#[derive(Serialize, Deserialize, Debug)]
pub struct EvaluationScript {
    #[serde(flatten)]
    pub base: Base,

    #[serde(default)]
    pub defaults: Defaults,

    #[serde(default)]
    pub runs: HashMap<String, MultiRun>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Base {
    pub data_folder: PathBuf,
    pub points_file: PathBuf,
    pub trajectory_file: PathBuf,
    pub las_point_record_format: u8,
    pub enable_cooldown: bool,
    pub use_existing_index: bool,

    #[serde(rename = "output_file")]
    pub output_file_pattern: String,

    #[serde(default)]
    pub offset: Vector3<f64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Defaults {
    #[serde(flatten)]
    pub index: SingleIndex,

    #[serde(default)]
    pub insertion_rate: SingleInsertionRateMeasurement,

    #[serde(default)]
    pub query_perf: SingleQueryPerfMeasurement,

    #[serde(default)]
    pub latency: SingleLatencyMeasurement,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MultiRun {
    #[serde(flatten)]
    pub index: MultiIndex,

    #[serde(default)]
    pub insertion_rate: MultiInsertionRateMeasurement,

    #[serde(default)]
    pub query_perf: MultiQueryPerfMeasurement,

    #[serde(default)]
    pub latency: MultiLatencyMeasurement,
}

impl MultiRun {
    pub fn apply_defaults(&mut self, defaults: &Defaults) {
        self.index.apply_defaults(&defaults.index);
        self.insertion_rate.apply_defaults(&defaults.insertion_rate);
        self.query_perf.apply_defaults(&defaults.query_perf);
        self.latency.apply_defaults(&defaults.latency);
    }
}

#[derive(Debug, Clone, DefaultFunctions, Serialize, Deserialize)]
pub struct SingleIndex {
    #[serde(default = "SingleIndex::default_typ", rename = "type")]
    pub typ: SystemUnderTest,

    #[serde(default = "SingleIndex::default_priority_function")]
    pub priority_function: TaskPriorityFunction,

    #[serde(default = "SingleIndex::default_num_threads")]
    pub num_threads: u16,

    #[serde(default = "SingleIndex::default_cache_size")]
    pub cache_size: usize,

    #[serde(default = "SingleIndex::default_node_size")]
    pub node_size: usize,

    #[serde(default = "SingleIndex::default_compression")]
    pub compression: bool,

    #[serde(default = "SingleIndex::default_nr_bogus_points")]
    pub nr_bogus_points: (usize, usize),

    #[serde(default = "SingleIndex::default_enable_attribute_index")]
    pub enable_attribute_index: bool,

    #[serde(default = "SingleIndex::default_enable_histogram_acceleration")]
    pub enable_histogram_acceleration: bool,

    #[serde(default = "SingleIndex::default_bin_count_intensity")]
    pub bin_count_intensity: usize,

    #[serde(default = "SingleIndex::default_bin_count_return_number")]
    pub bin_count_return_number: usize,

    #[serde(default = "SingleIndex::default_bin_count_classification")]
    pub bin_count_classification: usize,

    #[serde(default = "SingleIndex::default_bin_count_scan_angle_rank")]
    pub bin_count_scan_angle_rank: usize,

    #[serde(default = "SingleIndex::default_bin_count_user_data")]
    pub bin_count_user_data: usize,

    #[serde(default = "SingleIndex::default_bin_count_point_source_id")]
    pub bin_count_point_source_id: usize,

    #[serde(default = "SingleIndex::default_bin_count_color")]
    pub bin_count_color: usize,
}

impl Default for SingleIndex {
    fn default() -> Self {
        SingleIndex {
            typ: SystemUnderTest::Octree,
            priority_function: TaskPriorityFunction::NrPointsWeightedByTaskAge,
            num_threads: 4,
            cache_size: 500,
            node_size: 10000,
            compression: true,
            nr_bogus_points: (0, 0),
            enable_attribute_index: false,
            enable_histogram_acceleration: false,
            bin_count_intensity: 10,
            bin_count_return_number: 8,
            bin_count_classification: 255,
            bin_count_scan_angle_rank: 10,
            bin_count_user_data: 10,
            bin_count_point_source_id: 10,
            bin_count_color: 10,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MultiIndex {
    #[serde(default, rename = "type")]
    pub typ: Option<Vec<SystemUnderTest>>,

    #[serde(default)]
    pub priority_function: Option<Vec<TaskPriorityFunction>>,

    #[serde(default)]
    pub num_threads: Option<Vec<u16>>,

    #[serde(default)]
    pub cache_size: Option<Vec<usize>>,

    #[serde(default)]
    pub node_size: Option<Vec<usize>>,

    #[serde(default)]
    pub compression: Option<Vec<bool>>,

    #[serde(default)]
    pub nr_bogus_points: Option<Vec<(usize, usize)>>,

    #[serde(default)]
    pub enable_attribute_index: Option<Vec<bool>>,

    #[serde(default)]
    pub enable_histogram_acceleration: Option<Vec<bool>>,

    #[serde(default)]
    pub bin_count_intensity: Option<Vec<usize>>,

    #[serde(default)]
    pub bin_count_return_number: Option<Vec<usize>>,

    #[serde(default)]
    pub bin_count_classification: Option<Vec<usize>>,

    #[serde(default)]
    pub bin_count_scan_angle_rank: Option<Vec<usize>>,

    #[serde(default)]
    pub bin_count_user_data: Option<Vec<usize>>,

    #[serde(default)]
    pub bin_count_point_source_id: Option<Vec<usize>>,

    #[serde(default)]
    pub bin_count_color: Option<Vec<usize>>,

}

macro_rules! apply_default {
    ($self:ident.$i:ident <- $def:expr) => {
        if $self.$i.is_none() {
            $self.$i = Some($def.$i)
        }
    };
}

macro_rules! apply_default_vec {
    ($self:ident.$i:ident <- $def:expr) => {
        if $self.$i.is_none() {
            $self.$i = Some(vec![$def.$i])
        }
    };
}

fn expect<T>(t: &Option<T>) -> &T {
    t.as_ref().expect("Not all properties are set - Make sure to call `apply_defaults` before starting to iterate.")
}

impl MultiIndex {
    pub fn apply_defaults(&mut self, defaults: &SingleIndex) {
        apply_default_vec!(self.cache_size <- defaults);
        apply_default_vec!(self.priority_function <- defaults);
        apply_default_vec!(self.typ <- defaults);
        apply_default_vec!(self.compression <- defaults);
        apply_default_vec!(self.num_threads <- defaults);
        apply_default_vec!(self.node_size <- defaults);
        apply_default_vec!(self.nr_bogus_points <- defaults);
        apply_default_vec!(self.enable_attribute_index <- defaults);
        apply_default_vec!(self.enable_histogram_acceleration <- defaults);
        apply_default_vec!(self.bin_count_intensity <- defaults);
        apply_default_vec!(self.bin_count_return_number <- defaults);
        apply_default_vec!(self.bin_count_classification <- defaults);
        apply_default_vec!(self.bin_count_scan_angle_rank <- defaults);
        apply_default_vec!(self.bin_count_user_data <- defaults);
        apply_default_vec!(self.bin_count_point_source_id <- defaults);
        apply_default_vec!(self.bin_count_color <- defaults);
    }

    pub fn individual_runs(&self) -> Vec<SingleIndex> {
        let mut results = Vec::new();
        for &cache_size in expect(&self.cache_size) {
            for &priority_function in expect(&self.priority_function) {
                for &typ in expect(&self.typ) {
                    for &compression in expect(&self.compression) {
                        for &num_threads in expect(&self.num_threads) {
                            for &node_size in expect(&self.node_size) {
                                for &nr_bogus_points in expect(&self.nr_bogus_points) {
                                    for &enable_attribute_index in expect(&self.enable_attribute_index) {
                                        for &enable_histogram_acceleration in expect(&self.enable_histogram_acceleration) {
                                            for &bin_count_intensity in expect(&self.bin_count_intensity) {
                                                for &bin_count_return_number in expect(&self.bin_count_return_number) {
                                                    for &bin_count_classification in expect(&self.bin_count_classification) {
                                                        for &bin_count_scan_angle_rank in expect(&self.bin_count_scan_angle_rank) {
                                                            for &bin_count_user_data in expect(&self.bin_count_user_data) {
                                                                for &bin_count_point_source_id in expect(&self.bin_count_point_source_id) {
                                                                    for &bin_count_color in expect(&self.bin_count_color) {
                                                                        results.push(SingleIndex {
                                                                            typ,
                                                                            priority_function,
                                                                            num_threads,
                                                                            cache_size,
                                                                            node_size,
                                                                            compression,
                                                                            nr_bogus_points,
                                                                            enable_attribute_index,
                                                                            enable_histogram_acceleration,
                                                                            bin_count_intensity,
                                                                            bin_count_return_number,
                                                                            bin_count_classification,
                                                                            bin_count_scan_angle_rank,
                                                                            bin_count_user_data,
                                                                            bin_count_point_source_id,
                                                                            bin_count_color,
                                                                        })
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        results
    }
}

impl IntoIterator for &MultiIndex {
    type Item = SingleIndex;
    type IntoIter = IntoIter<SingleIndex>;

    fn into_iter(self) -> Self::IntoIter {
        self.individual_runs().into_iter()
    }
}

#[derive(Debug, Clone, DefaultFunctions, Serialize, Deserialize)]
pub struct SingleInsertionRateMeasurement {
    #[serde(default = "SingleLatencyMeasurement::default_points_per_sec")]
    pub target_point_pressure: usize,
}

impl Default for SingleInsertionRateMeasurement {
    fn default() -> Self {
        SingleInsertionRateMeasurement {
            target_point_pressure: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MultiInsertionRateMeasurement {
    #[serde(default)]
    pub target_point_pressure: Option<usize>,
}

impl MultiInsertionRateMeasurement {
    pub fn apply_defaults(&mut self, defaults: &SingleInsertionRateMeasurement) {
        apply_default!(self.target_point_pressure <- defaults);
    }

    pub fn single(&self) -> SingleInsertionRateMeasurement {
        let target_point_pressure = *expect(&self.target_point_pressure);
        SingleInsertionRateMeasurement {
            target_point_pressure,
        }
    }
}

#[derive(Debug, Clone, DefaultFunctions, Serialize, Deserialize)]
pub struct SingleLatencyMeasurement {
    #[serde(default = "SingleLatencyMeasurement::default_enable")]
    pub enable: bool,

    #[serde(default = "SingleLatencyMeasurement::default_points_per_sec")]
    pub points_per_sec: usize,

    #[serde(default = "SingleLatencyMeasurement::default_frames_per_sec")]
    pub frames_per_sec: usize,
}

impl Default for SingleLatencyMeasurement {
    fn default() -> Self {
        SingleLatencyMeasurement {
            enable: true,
            points_per_sec: 300000,
            frames_per_sec: 50,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MultiLatencyMeasurement {
    #[serde(default)]
    pub enable: Option<bool>,

    #[serde(default)]
    pub points_per_sec: Option<Vec<usize>>,

    #[serde(default)]
    pub frames_per_sec: Option<Vec<usize>>,
}

impl MultiLatencyMeasurement {
    pub fn apply_defaults(&mut self, defaults: &SingleLatencyMeasurement) {
        apply_default!(self.enable <- defaults);
        apply_default_vec!(self.points_per_sec <- defaults);
        apply_default_vec!(self.frames_per_sec <- defaults);
    }

    pub fn individual_runs(&self) -> Vec<SingleLatencyMeasurement> {
        let mut result = Vec::new();
        if *expect(&self.enable) {
            for &points_per_sec in expect(&self.points_per_sec) {
                for &frames_per_sec in expect(&self.frames_per_sec) {
                    result.push(SingleLatencyMeasurement {
                        enable: true,
                        points_per_sec,
                        frames_per_sec,
                    })
                }
            }
        }
        result
    }
}

impl IntoIterator for &MultiLatencyMeasurement {
    type Item = SingleLatencyMeasurement;
    type IntoIter = IntoIter<SingleLatencyMeasurement>;

    fn into_iter(self) -> Self::IntoIter {
        self.individual_runs().into_iter()
    }
}

#[derive(Debug, Clone, DefaultFunctions, Serialize, Deserialize)]
pub struct SingleQueryPerfMeasurement {
    #[serde(default = "SingleQueryPerfMeasurement::default_enable")]
    enable: bool,
}

impl Default for SingleQueryPerfMeasurement {
    fn default() -> Self {
        SingleQueryPerfMeasurement { enable: true }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MultiQueryPerfMeasurement {
    #[serde(default)]
    enable: Option<bool>,
}

impl MultiQueryPerfMeasurement {
    pub fn apply_defaults(&mut self, defaults: &SingleQueryPerfMeasurement) {
        apply_default!(self.enable <- defaults);
    }

    pub fn single(&self) -> Option<SingleQueryPerfMeasurement> {
        if *expect(&self.enable) {
            Some(SingleQueryPerfMeasurement { enable: true })
        } else {
            None
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, Eq, PartialEq)]
pub enum SystemUnderTest {
    Octree,
    SensorPosTree,
}
