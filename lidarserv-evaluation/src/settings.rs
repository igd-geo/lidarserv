use chrono::NaiveDate;
use indexmap::IndexMap;
use itertools::iproduct;
use lidarserv_common::geometry::coordinate_system::CoordinateSystem;
use lidarserv_common::geometry::position::POSITION_ATTRIBUTE_NAME;
use lidarserv_common::index::priority_function::TaskPriorityFunction;
use nalgebra::vector;
use pasture_core::layout::{PointAttributeDataType, PointAttributeDefinition, PointLayout};
use pasture_io::las::point_layout_from_las_point_format;
use pasture_io::las_rs::point::Format;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use toml::map::Map;

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct EvaluationScript {
    #[serde(flatten)]
    pub base: Base,

    #[serde(default)]
    pub defaults: SingleIndex,

    #[serde(default)]
    pub runs: HashMap<String, MultiIndex>,
}

impl Default for EvaluationScript {
    fn default() -> Self {
        let default_toml = include_str!("defaults.toml");
        toml::from_str(default_toml).expect("defaults.toml is invalid.")
    }
}

#[test]
#[cfg(test)]
fn test_default_toml_is_valid() {
    let default_script = EvaluationScript::default();
    assert!(default_script.runs.contains_key("example"));

    let default_index = SingleIndex::default();
    assert!(matches!(
        default_index.priority_function,
        TaskPriorityFunction::NrPointsWeightedByTaskAge
    ));
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
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

    #[serde(default)]
    pub indexing_timeout_seconds: Option<u64>,
    //#[serde(default)]
    //pub offset: Vector3<f64>,
    #[serde(default = "target_point_pressure_default")]
    pub target_point_pressure: usize,

    #[serde(default)]
    pub queries: IndexMap<String, String>,

    #[serde(default)]
    pub attributes: Attributes,

    #[serde(default = "coordinate_system_default")]
    pub coordinate_system: CoordinateSystem,
}

fn coordinate_system_default() -> CoordinateSystem {
    CoordinateSystem::from_las_transform(vector![0.001, 0.001, 0.001], vector![0.0, 0.0, 0.0])
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum AttributesPreset {
    PositionF64,
    PositionI32,
    LasPointFormat0,
    LasPointFormat1,
    LasPointFormat2,
    LasPointFormat3,
    LasPointFormat4,
    LasPointFormat5,
    LasPointFormat6,
    LasPointFormat7,
    LasPointFormat8,
    LasPointFormat9,
    LasPointFormat10,
}

impl AttributesPreset {
    pub fn attributes(self) -> Vec<PointAttributeDefinition> {
        fn las_attributes(point_format: u8) -> Vec<PointAttributeDefinition> {
            let format = Format::new(point_format).unwrap();
            let layout = point_layout_from_las_point_format(&format, false).unwrap();
            layout
                .attributes()
                .map(|a| a.attribute_definition().clone())
                .collect()
        }

        match self {
            AttributesPreset::PositionF64 => vec![PointAttributeDefinition::custom(
                Cow::Borrowed(POSITION_ATTRIBUTE_NAME),
                PointAttributeDataType::Vec3f64,
            )],
            AttributesPreset::PositionI32 => vec![PointAttributeDefinition::custom(
                Cow::Borrowed(POSITION_ATTRIBUTE_NAME),
                PointAttributeDataType::Vec3i32,
            )],
            AttributesPreset::LasPointFormat0 => las_attributes(0),
            AttributesPreset::LasPointFormat1 => las_attributes(1),
            AttributesPreset::LasPointFormat2 => las_attributes(2),
            AttributesPreset::LasPointFormat3 => las_attributes(3),
            AttributesPreset::LasPointFormat4 => las_attributes(4),
            AttributesPreset::LasPointFormat5 => las_attributes(5),
            AttributesPreset::LasPointFormat6 => las_attributes(6),
            AttributesPreset::LasPointFormat7 => las_attributes(7),
            AttributesPreset::LasPointFormat8 => las_attributes(8),
            AttributesPreset::LasPointFormat9 => las_attributes(9),
            AttributesPreset::LasPointFormat10 => las_attributes(10),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Attributes {
    Preset(AttributesPreset),
    Manual(IndexMap<String, PointAttributeDataType>),
}

impl Attributes {
    pub fn attributes(&self) -> Vec<PointAttributeDefinition> {
        match self {
            Attributes::Preset(attributes_preset) => attributes_preset.attributes(),
            Attributes::Manual(index_map) => index_map
                .iter()
                .map(|(name, data_type)| {
                    PointAttributeDefinition::custom(Cow::Owned(name.to_string()), *data_type)
                })
                .collect(),
        }
    }

    pub fn point_layout(&self) -> PointLayout {
        PointLayout::from_attributes(&self.attributes())
    }
}

impl Default for Attributes {
    fn default() -> Self {
        Attributes::Preset(AttributesPreset::PositionF64)
    }
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

fn output_file_pattern_default() -> String {
    EvaluationScript::default().base.output_file_pattern
}

fn target_point_pressure_default() -> usize {
    EvaluationScript::default().base.target_point_pressure
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
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
        let toml = include_str!("defaults.toml");

        let value: toml::Value = toml::from_str(toml).expect("defaults.toml is invalid.");
        let toml::Value::Table(mut value) = value else {
            panic!("defaults.toml is invalid.")
        };

        let defaults = value.remove("defaults").expect("defaults.toml is invalid.");
        let toml::Value::Table(defaults) = defaults else {
            panic!("defaults.toml is invalid.")
        };

        fn parse_key<T: DeserializeOwned>(map: &Map<String, toml::Value>, key: &str) -> T {
            let value = map.get(key).expect("defaults.toml is invalid.");
            let string = serde_json::to_string(value).expect("defaults.toml is invalid.");
            serde_json::from_str(&string).expect("defaults.toml is invalid.")
        }
        SingleIndex {
            node_hierarchy: parse_key(&defaults, "node_hierarchy"),
            point_hierarchy: parse_key(&defaults, "point_hierarchy"),
            priority_function: parse_key(&defaults, "priority_function"),
            num_threads: parse_key(&defaults, "num_threads"),
            cache_size: parse_key(&defaults, "cache_size"),
            compression: parse_key(&defaults, "compression"),
            nr_bogus_points: parse_key(&defaults, "nr_bogus_points"),
            max_lod: parse_key(&defaults, "max_lod"),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MultiIndex {
    pub node_hierarchy: Option<Vec<i16>>,
    pub point_hierarchy: Option<Vec<i16>>,
    pub priority_function: Option<Vec<TaskPriorityFunction>>,
    pub num_threads: Option<Vec<u16>>,
    pub cache_size: Option<Vec<usize>>,
    pub compression: Option<Vec<bool>>,
    pub nr_bogus_points: Option<Vec<(usize, usize)>>,
    pub max_lod: Option<Vec<u8>>,
    // pub enable_attribute_index: Option<Vec<bool>>,
    // pub enable_histogram_acceleration: Option<Vec<bool>>,
    // pub bin_count_intensity: Option<Vec<usize>>,
    // pub bin_count_return_number: Option<Vec<usize>>,
    // pub bin_count_classification: Option<Vec<usize>>,
    // pub bin_count_scan_angle_rank: Option<Vec<usize>>,
    // pub bin_count_user_data: Option<Vec<usize>>,
    // pub bin_count_point_source_id: Option<Vec<usize>>,
    // pub bin_count_color: Option<Vec<usize>>,
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
}

impl<'a> IntoIterator for &'a MultiIndex {
    type Item = SingleIndex;
    type IntoIter = Box<dyn Iterator<Item = SingleIndex> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        let iter = iproduct!(
            expect(&self.node_hierarchy),
            expect(&self.point_hierarchy),
            expect(&self.priority_function),
            expect(&self.num_threads),
            expect(&self.cache_size),
            expect(&self.compression),
            expect(&self.nr_bogus_points),
            expect(&self.max_lod),
        )
        .map(
            |(
                &node_hierarchy,
                &point_hierarchy,
                &priority_function,
                &num_threads,
                &cache_size,
                &compression,
                &nr_bogus_points,
                &max_lod,
            )| SingleIndex {
                node_hierarchy,
                point_hierarchy,
                priority_function,
                num_threads,
                cache_size,
                compression,
                nr_bogus_points,
                max_lod,
            },
        );

        Box::new(iter)
    }
}
