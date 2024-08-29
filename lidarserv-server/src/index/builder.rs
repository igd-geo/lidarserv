use crate::index::settings::IndexSettings;
use anyhow::Result;
use lidarserv_common::index::{Octree, OctreeParams};
use std::path::{Path, PathBuf};

pub fn build(settings: IndexSettings, data_path: &Path) -> Result<Octree> {
    let IndexSettings {
        use_metrics,
        node_hierarchy,
        point_hierarchy,
        coordinate_system,
        max_lod,
        max_bogus_inner,
        max_bogus_leaf,
        enable_compression,
        max_cache_size,
        priority_function,
        num_threads,
        point_layout,
    } = settings;

    // metrics
    let metrics_file = if use_metrics {
        let mut metrics_file_name = PathBuf::new();
        for i in 0.. {
            metrics_file_name = data_path.join(format!("metrics_{}.cbor", i));
            if !metrics_file_name.exists() {
                break;
            }
        }
        Some(metrics_file_name)
    } else {
        None
    };

    // build octree
    Octree::new(OctreeParams {
        directory_file: data_path.join("directory.bin"),
        point_data_folder: data_path.to_path_buf(),
        metrics_file,
        point_layout,
        node_hierarchy,
        point_hierarchy,
        coordinate_system,
        max_lod,
        max_bogus_inner,
        max_bogus_leaf,
        enable_compression,
        max_cache_size,
        priority_function,
        num_threads,
    })
}
