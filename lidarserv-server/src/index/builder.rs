use crate::common::index::octree::grid_cell_directory::{GridCellDirectory, GridCellIoError};
use crate::common::index::octree::page_manager::OctreePageLoader;
use crate::common::index::octree::Octree;
use crate::index::settings::IndexSettings;
use crate::index::DynIndex;
use lidarserv_common::geometry::grid::I32GridHierarchy;
use lidarserv_common::geometry::position::I32CoordinateSystem;
use lidarserv_common::geometry::sampling::GridCenterSamplingFactory;
use lidarserv_common::index::octree::live_metrics_collector::{LiveMetricsCollector, MetricsError};
use lidarserv_common::index::octree::OctreeParams;
use lidarserv_common::las::I32LasReadWrite;
use std::path::{Path, PathBuf};
use thiserror::Error;
use lidarserv_common::index::octree::attribute_index::AttributeIndex;
use crate::index::point::LasPoint;

#[derive(Debug, Error)]
pub enum BuilderError {
    #[error("Could not load directory: {0}")]
    GridCellIo(#[from] GridCellIoError),

    #[error("Could not open metric file: {0}")]
    MetricsIo(#[from] MetricsError),
}

pub fn build(settings: IndexSettings, data_path: &Path) -> Result<Box<dyn DynIndex>, BuilderError> {
    let IndexSettings {
        general_settings,
        octree_settings,
    } = settings;

    // tree stuff
    let node_hierarchy = I32GridHierarchy::new(octree_settings.node_grid_shift);
    let point_hierarchy = I32GridHierarchy::new(octree_settings.point_grid_shift);
    let sample_factory = GridCenterSamplingFactory::new(point_hierarchy);

    // page loading stuff
    let las_loader =
        I32LasReadWrite::new(general_settings.use_compression, general_settings.use_color, general_settings.use_time);
    let page_loader = OctreePageLoader::new(las_loader.clone(), data_path.to_owned());
    let mut directory_file_name = data_path.to_owned();
    directory_file_name.push("directory.bin");
    let page_directory = GridCellDirectory::new(&octree_settings.max_lod, directory_file_name)?;
    let coordinate_system = I32CoordinateSystem::from_las_transform(
        general_settings.las_scale,
        general_settings.las_offset,
    );

    // attribute indexing stuff
    let mut attribute_index = None;
    if octree_settings.enable_attribute_indexing {
        let mut attribute_index_file_name = data_path.to_owned();
        attribute_index_file_name.push("attribute_index.bin");
        attribute_index = Option::from(AttributeIndex::new(octree_settings.max_lod.level() as usize, attribute_index_file_name));
    }

    // metrics
    let metrics = if octree_settings.use_metrics {
        let mut metrics_file_name = PathBuf::new();
        for i in 0.. {
            metrics_file_name = data_path.to_owned();
            metrics_file_name.push(format!("metrics_{}.cbor", i));
            if !metrics_file_name.exists() {
                break;
            }
        }
        let m = LiveMetricsCollector::new_file_backed_collector(&metrics_file_name)?;
        Some(m)
    } else {
        None
    };

    // build octree
    let octree = Octree::new(OctreeParams {
        num_threads: general_settings.nr_threads as u16,
        priority_function: octree_settings.priority_function,
        max_lod: octree_settings.max_lod,
        max_bogus_inner: octree_settings.max_bogus_inner,
        max_bogus_leaf: octree_settings.max_bogus_leaf,
        attribute_index,
        node_hierarchy,
        page_loader,
        page_directory,
        max_cache_size: general_settings.max_cache_size,
        sample_factory,
        loader: las_loader,
        coordinate_system,
        metrics,
        use_point_colors: general_settings.use_color,
        use_point_times: general_settings.use_time,
    });
    Ok(Box::new(octree))
}