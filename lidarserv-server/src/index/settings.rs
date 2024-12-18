use crate::common::geometry::grid::LodLevel;
use crate::common::index::octree::writer::TaskPriorityFunction;
use lidarserv_common::nalgebra::Vector3;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::{Path, PathBuf};
use thiserror::Error;
use lidarserv_common::index::octree::attribute_histograms::HistogramSettings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSettings {
    pub general_settings: GeneralSettings,
    pub octree_settings: OctreeSettings,
    pub histogram_settings: HistogramSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSettings {
    pub nr_threads: usize,
    pub max_cache_size: usize,
    pub las_scale: Vector3<f64>,
    pub las_offset: Vector3<f64>,
    pub use_compression: bool,
    pub point_record_format: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OctreeSettings {
    pub priority_function: TaskPriorityFunction,
    pub max_lod: LodLevel,
    pub max_bogus_inner: usize,
    pub max_bogus_leaf: usize,
    pub enable_attribute_indexing: bool,
    pub enable_histogram_acceleration: bool,
    pub use_metrics: bool,
    pub point_grid_shift: u16,
    pub node_grid_shift: u16,
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
        serde_json::to_writer(file, self)?;
        Ok(())
    }

    pub fn save_to_data_folder(&self, path: &Path) -> Result<(), IndexSettingIoError> {
        let file_name = get_data_folder_settings_file(path);
        self.save_to_file(&file_name)
    }
}
