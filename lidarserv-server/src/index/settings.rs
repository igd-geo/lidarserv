use crate::common::geometry::grid::LodLevel;
use lidarserv_common::geometry::coordinate_system::CoordinateSystem;
use lidarserv_common::geometry::grid::GridHierarchy;
use lidarserv_common::index::attribute_index::config::AttributeIndexConfig;
use lidarserv_common::index::priority_function::TaskPriorityFunction;
use pasture_core::layout::PointLayout;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSettings {
    pub use_metrics: bool,
    pub node_hierarchy: GridHierarchy,
    pub point_hierarchy: GridHierarchy,
    pub coordinate_system: CoordinateSystem,
    pub max_lod: LodLevel,
    pub max_bogus_inner: usize,
    pub max_bogus_leaf: usize,
    pub enable_compression: bool,
    pub max_cache_size: usize,
    pub priority_function: TaskPriorityFunction,
    pub num_threads: u16,
    pub point_layout: PointLayout,
    pub attribute_indexes: Vec<AttributeIndexConfig>,
}

#[derive(Error, Debug)]
pub enum IndexSettingIoError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    SerDe(#[from] serde_json::Error),
}

fn get_data_folder_settings_file(path: &Path) -> PathBuf {
    let mut file_name = path.to_owned();
    file_name.push("settings.json");
    file_name
}

impl IndexSettings {
    pub fn load_from_file(file_name: &Path) -> Result<Self, IndexSettingIoError> {
        let file = File::open(file_name)?;
        let settings = serde_json::from_reader(file)?;
        Ok(settings)
    }

    pub fn load_from_data_folder(path: &Path) -> Result<Self, IndexSettingIoError> {
        let file_name = get_data_folder_settings_file(path);
        Self::load_from_file(&file_name)
    }

    pub fn save_to_file(&self, file_name: &Path) -> Result<(), IndexSettingIoError> {
        let file = File::create(file_name)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    pub fn save_to_data_folder(&self, path: &Path) -> Result<(), IndexSettingIoError> {
        let file_name = get_data_folder_settings_file(path);
        self.save_to_file(&file_name)
    }
}
