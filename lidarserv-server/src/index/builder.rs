use crate::common::geometry::grid::LodLevel;
use crate::common::index::octree::grid_cell_directory::{GridCellDirectory, GridCellIoError};
use crate::common::index::octree::page_manager::OctreePageLoader;
use crate::common::index::octree::Octree;
use crate::index::settings::{
    GeneralSettings, IndexSettings, IndexType, OctreeSettings, SensorPositionSettings,
};
use crate::index::DynIndex;
use lidarserv_common::geometry::grid::I32GridHierarchy;
use lidarserv_common::geometry::position::I32CoordinateSystem;
use lidarserv_common::geometry::sampling::GridCenterSamplingFactory;
use lidarserv_common::index::octree::OctreeParams;
use lidarserv_common::index::sensor_pos::meta_tree::{MetaTree, MetaTreeIoError};
use lidarserv_common::index::sensor_pos::page_manager::{FileIdDirectory, Loader};
use lidarserv_common::index::sensor_pos::{SensorPosIndex, SensorPosIndexParams};
use lidarserv_common::las::I32LasReadWrite;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BuilderError {
    #[error("Could not load meta tree: {0}")]
    MetaTreeIo(#[from] MetaTreeIoError),

    #[error("Could not load directory: {0}")]
    GridCellIo(#[from] GridCellIoError),
}

pub fn build(settings: IndexSettings, data_path: &Path) -> Result<Box<dyn DynIndex>, BuilderError> {
    let IndexSettings {
        general,
        index_type,
    } = settings;
    match index_type {
        IndexType::SensorPositionIndex(s) => build_sensor_position_index(general, s, data_path),
        IndexType::Octree(o) => build_octree_index(general, o, data_path),
    }
}

fn build_octree_index(
    genaral_settings: GeneralSettings,
    settings: OctreeSettings,
    data_path: &Path,
) -> Result<Box<dyn DynIndex>, BuilderError> {
    // tree stuff
    let node_hierarchy = I32GridHierarchy::new(14); // todo config for that
    let point_hierarchy = I32GridHierarchy::new(21); // todo config for that
    let sample_factory = GridCenterSamplingFactory::new(point_hierarchy);

    // page loading stuff
    let las_loader = I32LasReadWrite::new(genaral_settings.use_compression);
    let page_loader = OctreePageLoader::new(las_loader.clone(), data_path.to_owned());
    let mut directory_file_name = data_path.to_owned();
    directory_file_name.push("directory.bin");
    let page_directory = GridCellDirectory::new(&settings.max_lod, directory_file_name)?;
    let coordinate_system = I32CoordinateSystem::from_las_transform(
        genaral_settings.las_scale,
        genaral_settings.las_offset,
    );

    let octree = Octree::new(OctreeParams {
        num_threads: genaral_settings.nr_threads as u16,
        priority_function: settings.priority_function,
        max_lod: settings.max_lod,
        max_bogus_inner: settings.max_bogus_inner,
        max_bogus_leaf: settings.max_bogus_leaf,
        node_hierarchy,
        page_loader,
        page_directory,
        max_cache_size: genaral_settings.max_cache_size,
        sample_factory,
        loader: las_loader,
        coordinate_system,
    });
    Ok(Box::new(octree))
}

fn build_sensor_position_index(
    general_settings: GeneralSettings,
    settings: SensorPositionSettings,
    data_path: &Path,
) -> Result<Box<dyn DynIndex>, BuilderError> {
    // las loader
    let las_loader = I32LasReadWrite::new(general_settings.use_compression);

    // coordinate system
    let coordinate_system = I32CoordinateSystem::from_las_transform(
        general_settings.las_scale,
        general_settings.las_offset,
    );

    // sampling
    let point_grid_hierarchy = I32GridHierarchy::new(17);
    let sampling_factory = GridCenterSamplingFactory::new(point_grid_hierarchy);

    // sensor grid
    let sensor_grid_hierarchy = I32GridHierarchy::new(14);

    // meta tree
    let mut meta_tree_file_name = data_path.to_owned();
    meta_tree_file_name.push("meta.bin");
    let meta_tree = MetaTree::load_from_file(&meta_tree_file_name, sensor_grid_hierarchy)?;

    // page manager
    let page_loader = Loader::new(
        data_path.to_owned(),
        general_settings.use_compression,
        coordinate_system.clone(),
        las_loader.clone(),
    );
    let directory = FileIdDirectory::from_meta_tree(&meta_tree);
    let page_manager = lidarserv_common::index::sensor_pos::page_manager::PageManager::new(
        page_loader,
        directory,
        general_settings.max_cache_size,
    );

    let params = SensorPosIndexParams {
        nr_threads: general_settings.nr_threads,
        max_node_size: settings.max_nr_points_per_node,
        meta_tree_file: meta_tree_file_name,
        sampling_factory,
        page_manager,
        meta_tree,
        las_loader,
        coordinate_system,
        max_lod: LodLevel::from_level(10), // todo config for this
        max_delay: Duration::from_secs(1), // todo config for this
        coarse_lod_steps: 1,               // todo config for this
    };
    let spi = SensorPosIndex::new(params);

    Ok(Box::new(spi))
}
