use chrono::NaiveDate;
use lidarserv_common::index::priority_function::TaskPriorityFunction;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
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

impl Default for EvaluationScript {
    fn default() -> Self {
        Self {
            base: Default::default(),
            defaults: Default::default(),
            runs: HashMap::from_iter([(
                "example".to_string(),
                MultiRun {
                    index: MultiIndex {
                        cache_size: Some(vec![500, 1_000, 5_000, 10_000, 50_000]),
                        compression: Some(vec![false, true]),
                        ..Default::default()
                    },
                    insertion_rate: MultiInsertionRateMeasurement::default(),
                    query_perf: MultiQueryPerfMeasurement::default(),
                },
            )]),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Base {
    /// Base folder that all filenames are relative to.
    /// This is the folder that the settings file is located in.
    /// Therefore, it is not serialized/deserialized.
    #[serde(skip)]
    pub base_folder: PathBuf,

    /// Las file containing the test data
    pub points_file: PathBuf,

    /// folder where the index will be created
    pub index_folder: PathBuf,

    #[serde(rename = "output_file", default = "output_file_pattern_default")]
    pub output_file_pattern: String,

    #[serde(default)]
    pub use_existing_index: bool,

    #[serde(default)]
    pub cooldown_seconds: u64,
    //pub las_point_record_format: u8,
    pub indexing_timeout_seconds: u64,
    //#[serde(default)]
    //pub offset: Vector3<f64>,
}

impl Base {
    pub fn points_file_absolute(&self) -> PathBuf {
        self.base_folder.join(&self.points_file)
    }

    pub fn index_folder_absolute(&self) -> PathBuf {
        self.base_folder.join(&self.index_folder)
    }

    pub fn output_file_absolute(&self, date: NaiveDate, index: u32) -> PathBuf {
        let date_str = date.format("%Y-%m-%d").to_string();
        let with_date = self.output_file_pattern.replace("%d", &date_str);
        let with_index = with_date.replace("%i", &index.to_string());
        self.base_folder.join(with_index)
    }

    pub fn is_output_filename_indexed(&self) -> bool {
        self.output_file_pattern.contains("%i")
    }
}

impl Default for Base {
    fn default() -> Self {
        Self {
            base_folder: PathBuf::new(),
            points_file: PathBuf::from_str("./points.las").unwrap(),
            index_folder: PathBuf::from_str("./index").unwrap(),
            output_file_pattern: output_file_pattern_default(),
            use_existing_index: false,
            cooldown_seconds: 0,
            indexing_timeout_seconds: 60 * 15,
        }
    }
}

fn output_file_pattern_default() -> String {
    "evaluation_%d_%i.json".to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Defaults {
    #[serde(flatten)]
    pub index: SingleIndex,

    #[serde(default)]
    pub insertion_rate: SingleInsertionRateMeasurement,

    #[serde(default)]
    pub query_perf: SingleQueryPerfMeasurement,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MultiRun {
    #[serde(flatten)]
    pub index: MultiIndex,

    #[serde(default)]
    pub insertion_rate: MultiInsertionRateMeasurement,

    #[serde(default)]
    pub query_perf: MultiQueryPerfMeasurement,
}

impl MultiRun {
    pub fn apply_defaults(&mut self, defaults: &Defaults) {
        self.index.apply_defaults(&defaults.index);
        self.insertion_rate.apply_defaults(&defaults.insertion_rate);
        self.query_perf.apply_defaults(&defaults.query_perf);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SingleIndex {
    pub node_hierarchy: i16,
    pub point_hierarchy: i16,
    pub priority_function: TaskPriorityFunction,
    pub num_threads: u16,
    pub cache_size: usize,
    pub compression: bool,
    pub nr_bogus_points: (usize, usize),
    pub max_lod: u8,
    // pub enable_attribute_index: bool,
    // pub enable_histogram_acceleration: bool,
    // pub bin_count_intensity: usize,
    // pub bin_count_return_number: usize,
    // pub bin_count_classification: usize,
    // pub bin_count_scan_angle_rank: usize,
    // pub bin_count_user_data: usize,
    // pub bin_count_point_source_id: usize,
    // pub bin_count_color: usize,
}

impl Default for SingleIndex {
    fn default() -> Self {
        SingleIndex {
            node_hierarchy: 13,
            point_hierarchy: 5,
            priority_function: TaskPriorityFunction::NrPointsWeightedByTaskAge,
            num_threads: 4,
            cache_size: 5000,
            compression: true,
            nr_bogus_points: (0, 0),
            max_lod: 10,
            // enable_attribute_index: true,
            // enable_histogram_acceleration: true,
            // bin_count_intensity: 10,
            // bin_count_return_number: 8,
            // bin_count_classification: 255,
            // bin_count_scan_angle_rank: 10,
            // bin_count_user_data: 10,
            // bin_count_point_source_id: 10,
            // bin_count_color: 10,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MultiIndex {
    #[serde(default)]
    pub node_hierarchy: Option<Vec<i16>>,

    #[serde(default)]
    pub point_hierarchy: Option<Vec<i16>>,

    #[serde(default)]
    pub priority_function: Option<Vec<TaskPriorityFunction>>,

    #[serde(default)]
    pub num_threads: Option<Vec<u16>>,

    #[serde(default)]
    pub cache_size: Option<Vec<usize>>,

    #[serde(default)]
    pub compression: Option<Vec<bool>>,

    #[serde(default)]
    pub nr_bogus_points: Option<Vec<(usize, usize)>>,

    #[serde(default)]
    pub max_lod: Option<Vec<u8>>,
    //    #[serde(default)]
    //    pub enable_attribute_index: Option<Vec<bool>>,
    //
    //    #[serde(default)]
    //    pub enable_histogram_acceleration: Option<Vec<bool>>,
    //
    //    #[serde(default)]
    //    pub bin_count_intensity: Option<Vec<usize>>,
    //
    //    #[serde(default)]
    //    pub bin_count_return_number: Option<Vec<usize>>,
    //
    //    #[serde(default)]
    //    pub bin_count_classification: Option<Vec<usize>>,
    //
    //    #[serde(default)]
    //    pub bin_count_scan_angle_rank: Option<Vec<usize>>,
    //
    //    #[serde(default)]
    //    pub bin_count_user_data: Option<Vec<usize>>,
    //
    //    #[serde(default)]
    //    pub bin_count_point_source_id: Option<Vec<usize>>,
    //
    //    #[serde(default)]
    //    pub bin_count_color: Option<Vec<usize>>,
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
        apply_default_vec!(self.node_hierarchy <- defaults);
        apply_default_vec!(self.point_hierarchy <- defaults);
        apply_default_vec!(self.compression <- defaults);
        apply_default_vec!(self.num_threads <- defaults);
        apply_default_vec!(self.nr_bogus_points <- defaults);
        apply_default_vec!(self.max_lod <- defaults);
        //apply_default_vec!(self.enable_attribute_index <- defaults);
        //apply_default_vec!(self.enable_histogram_acceleration <- defaults);
        //apply_default_vec!(self.bin_count_intensity <- defaults);
        //apply_default_vec!(self.bin_count_return_number <- defaults);
        //apply_default_vec!(self.bin_count_classification <- defaults);
        //apply_default_vec!(self.bin_count_scan_angle_rank <- defaults);
        //apply_default_vec!(self.bin_count_user_data <- defaults);
        //apply_default_vec!(self.bin_count_point_source_id <- defaults);
        //apply_default_vec!(self.bin_count_color <- defaults);
    }

    pub fn individual_runs(&self) -> Vec<SingleIndex> {
        let mut results = Vec::new();
        for &cache_size in expect(&self.cache_size) {
            for &priority_function in expect(&self.priority_function) {
                for &node_hierarchy in expect(&self.node_hierarchy) {
                    for &point_hierarchy in expect(&self.point_hierarchy) {
                        for &compression in expect(&self.compression) {
                            for &num_threads in expect(&self.num_threads) {
                                for &nr_bogus_points in expect(&self.nr_bogus_points) {
                                    for &max_lod in expect(&self.max_lod) {
                                        //for &enable_attribute_index in
                                        //    expect(&self.enable_attribute_index)
                                        //{
                                        //    for &enable_histogram_acceleration in
                                        //        expect(&self.enable_histogram_acceleration)
                                        //    {
                                        //        for &bin_count_intensity in
                                        //            expect(&self.bin_count_intensity)
                                        //        {
                                        //            for &bin_count_return_number in
                                        //                expect(&self.bin_count_return_number)
                                        //            {
                                        //                for &bin_count_classification in
                                        //                    expect(&self.bin_count_classification)
                                        //                {
                                        //                    for &bin_count_scan_angle_rank in
                                        //                        expect(&self.bin_count_scan_angle_rank)
                                        //                    {
                                        //                        for &bin_count_user_data in
                                        //                            expect(&self.bin_count_user_data)
                                        //                        {
                                        //
                                        //
                                        //
                                        //
                                        //for &bin_count_point_source_id in
                                        //    expect(&self.bin_count_point_source_id)
                                        //{
                                        //for &bin_count_color in expect(&self.bin_count_color) {
                                        results.push(SingleIndex {
                                            node_hierarchy,
                                            point_hierarchy,
                                            priority_function,
                                            num_threads,
                                            cache_size,
                                            compression,
                                            nr_bogus_points,
                                            max_lod,
                                            //enable_attribute_index,
                                            //enable_histogram_acceleration,
                                            //bin_count_intensity,
                                            //bin_count_return_number,
                                            //bin_count_classification,
                                            //bin_count_scan_angle_rank,
                                            //bin_count_user_data,
                                            //bin_count_point_source_id,
                                            //bin_count_color,
                                        })
                                        //}
                                        //}
                                        //                        }
                                        //                    }
                                        //                }
                                        //            }
                                        //        }
                                        //    }
                                        //}
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SingleInsertionRateMeasurement {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SingleLatencyMeasurement {
    pub enable: bool,
    pub points_per_sec: usize,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleQueryPerfMeasurement {
    queries: HashMap<String, String>,
}

impl Default for SingleQueryPerfMeasurement {
    fn default() -> Self {
        SingleQueryPerfMeasurement {
            queries: HashMap::from_iter([
                ("full-point-cloud".to_string(), "full".to_string()),
                ("root-nodes".to_string(), "lod(0)".to_string()),
                ("lod1".to_string(), "lod(1)".to_string()),
                ("lod2".to_string(), "lod(2)".to_string()),
                ("lod3".to_string(), "lod(3)".to_string()),
            ]),
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MultiQueryPerfMeasurement {
    #[serde(default)]
    queries: Option<HashMap<String, String>>,
}

impl MultiQueryPerfMeasurement {
    pub fn apply_defaults(&mut self, defaults: &SingleQueryPerfMeasurement) {
        apply_default!(self.queries <- defaults.clone());
    }

    pub fn queries(&self) -> &HashMap<String, String> {
        expect(&self.queries)
    }
}
