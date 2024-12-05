use chrono::NaiveDate;
use indexmap::IndexMap;
use itertools::iproduct;
use lidarserv_common::geometry::coordinate_system::CoordinateSystem;
use lidarserv_common::geometry::position::POSITION_ATTRIBUTE_NAME;
use lidarserv_common::index::attribute_index::config::{AttributeIndexConfig, IndexKind};
use lidarserv_common::index::priority_function::TaskPriorityFunction;
use log::warn;
use nalgebra::vector;
use pasture_core::layout::{PointAttributeDataType, PointAttributeDefinition, PointLayout};
use pasture_io::las::{point_layout_from_las_point_format, ATTRIBUTE_LOCAL_LAS_POSITION};
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

    #[serde(default)]
    pub indexed_attributes: HashMap<String, Vec<IndexKind>>,
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
    LasPointFormat0Raw,
    LasPointFormat1Raw,
    LasPointFormat2Raw,
    LasPointFormat3Raw,
    LasPointFormat4Raw,
    LasPointFormat5Raw,
    LasPointFormat6Raw,
    LasPointFormat7Raw,
    LasPointFormat8Raw,
    LasPointFormat9Raw,
    LasPointFormat10Raw,
}

impl AttributesPreset {
    pub fn attributes(self) -> Vec<PointAttributeDefinition> {
        fn las_attributes(
            point_format: u8,
            exact_binary_repr: bool,
        ) -> Vec<PointAttributeDefinition> {
            let format = Format::new(point_format).unwrap();
            let layout = point_layout_from_las_point_format(&format, exact_binary_repr).unwrap();
            let mut attrs = layout
                .attributes()
                .map(|a| a.attribute_definition().clone())
                .collect::<Vec<_>>();

            // rename the position attribute to "our" position attribute.
            for attr in &mut attrs {
                if attr.name() == ATTRIBUTE_LOCAL_LAS_POSITION.name() {
                    *attr = PointAttributeDefinition::custom(
                        Cow::Borrowed(POSITION_ATTRIBUTE_NAME),
                        attr.datatype(),
                    );
                }
            }

            attrs
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
            AttributesPreset::LasPointFormat0 => las_attributes(0, false),
            AttributesPreset::LasPointFormat1 => las_attributes(1, false),
            AttributesPreset::LasPointFormat2 => las_attributes(2, false),
            AttributesPreset::LasPointFormat3 => las_attributes(3, false),
            AttributesPreset::LasPointFormat4 => las_attributes(4, false),
            AttributesPreset::LasPointFormat5 => las_attributes(5, false),
            AttributesPreset::LasPointFormat6 => las_attributes(6, false),
            AttributesPreset::LasPointFormat7 => las_attributes(7, false),
            AttributesPreset::LasPointFormat8 => las_attributes(8, false),
            AttributesPreset::LasPointFormat9 => las_attributes(9, false),
            AttributesPreset::LasPointFormat10 => las_attributes(10, false),
            AttributesPreset::LasPointFormat0Raw => las_attributes(0, true),
            AttributesPreset::LasPointFormat1Raw => las_attributes(1, true),
            AttributesPreset::LasPointFormat2Raw => las_attributes(2, true),
            AttributesPreset::LasPointFormat3Raw => las_attributes(3, true),
            AttributesPreset::LasPointFormat4Raw => las_attributes(4, true),
            AttributesPreset::LasPointFormat5Raw => las_attributes(5, true),
            AttributesPreset::LasPointFormat6Raw => las_attributes(6, true),
            AttributesPreset::LasPointFormat7Raw => las_attributes(7, true),
            AttributesPreset::LasPointFormat8Raw => las_attributes(8, true),
            AttributesPreset::LasPointFormat9Raw => las_attributes(9, true),
            AttributesPreset::LasPointFormat10Raw => las_attributes(10, true),
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

    pub fn attribute_indexes(&self) -> Vec<AttributeIndexConfig> {
        let mut result = Vec::new();
        let attributes = self.attributes.attributes();
        for (attr_name, indexes) in self.indexed_attributes.iter() {
            let Some(attr) = attributes
                .iter()
                .find(|a| a.name().to_lowercase() == attr_name.to_lowercase())
            else {
                warn!("Attribute {} does not exist. (Ignoring)", attr_name);
                warn!("Available attributes: {:?}", attributes.iter().map(|a| a.name()).collect::<Vec<_>>());
                continue;
            };
            for index in indexes {
                let i = result.len();
                let path = self.base_folder.join(format!("attribute-index-{}.bin", i));
                result.push(AttributeIndexConfig {
                    attribute: attr.clone(),
                    path,
                    index: *index,
                });
            }
        }
        result
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
    pub enable_attribute_index: bool,
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
            enable_attribute_index: parse_key(&defaults, "enable_attribute_index"),
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
    pub enable_attribute_index: Option<Vec<bool>>,
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
        apply_default_vec!(self.enable_attribute_index <- defaults);
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
            expect(&self.enable_attribute_index),
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
                &enable_attribute_index,
            )| SingleIndex {
                node_hierarchy,
                point_hierarchy,
                priority_function,
                num_threads,
                cache_size,
                compression,
                nr_bogus_points,
                max_lod,
                enable_attribute_index,
            },
        );

        Box::new(iter)
    }
}
