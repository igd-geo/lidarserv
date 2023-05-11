use crate::settings::{Base, SingleIndex};
use crate::Point;
use lidarserv_common::geometry::grid::{I32GridHierarchy, LodLevel};
use lidarserv_common::geometry::position::I32CoordinateSystem;
use lidarserv_common::geometry::sampling::{GridCenterSampling, GridCenterSamplingFactory};
use lidarserv_common::index::octree::grid_cell_directory::GridCellDirectory;
use lidarserv_common::index::octree::page_manager::OctreePageLoader;
use lidarserv_common::index::octree::{Octree, OctreeParams};
use lidarserv_common::index::sensor_pos::meta_tree::MetaTree;
use lidarserv_common::index::sensor_pos::page_manager::{FileIdDirectory, Loader};
use lidarserv_common::index::sensor_pos::{SensorPosIndex, SensorPosIndexParams};
use lidarserv_common::las::I32LasReadWrite;
use std::path::PathBuf;
use std::time::Duration;

pub type I32SensorPosIndex =
    SensorPosIndex<GridCenterSamplingFactory<Point>, Point, GridCenterSampling<Point>>;
pub type I32Octree = Octree<Point, GridCenterSampling<Point>, GridCenterSamplingFactory<Point>>;

pub fn create_sensor_pos_index(
    coordinate_system: I32CoordinateSystem,
    base_settings: &Base,
    settings: &SingleIndex,
) -> I32SensorPosIndex {
    let mut data_folder: PathBuf = base_settings.data_folder.clone();
    data_folder.push("sensorpos");
    let mut meta_tree_file = data_folder.clone();
    meta_tree_file.push("meta.bin");

    let max_cache_size = settings.cache_size;

    let point_grid_hierarchy = I32GridHierarchy::new(17);
    let sampling_factory = GridCenterSamplingFactory::new(point_grid_hierarchy);
    let sensor_grid_hierarchy = I32GridHierarchy::new(14);
    let meta_tree = MetaTree::new(sensor_grid_hierarchy);
    let las_loader = I32LasReadWrite::new(settings.compression, false, true , true);
    let page_loader = Loader::new(
        data_folder.clone(),
        settings.compression,
        coordinate_system.clone(),
        las_loader.clone(),
    );
    let directory = FileIdDirectory::from_meta_tree(&meta_tree);
    let page_manager = lidarserv_common::index::sensor_pos::page_manager::PageManager::new(
        page_loader,
        directory,
        max_cache_size,
    );

    let params = SensorPosIndexParams {
        nr_threads: settings.num_threads as usize,
        max_node_size: settings.node_size,
        meta_tree_file,
        sampling_factory,
        page_manager,
        meta_tree,
        las_loader,
        coordinate_system,
        max_lod: LodLevel::from_level(10),
        max_delay: Duration::from_secs(1),
        coarse_lod_steps: 5,
        use_point_colors: false,
        use_point_times: false,
    };
    SensorPosIndex::new(params)
}

pub fn create_octree_index(
    coordinate_system: I32CoordinateSystem,
    base_settings: &Base,
    settings: &SingleIndex,
) -> I32Octree {
    let mut data_folder: PathBuf = base_settings.data_folder.clone();
    data_folder.push("octree");
    let node_hierarchy = I32GridHierarchy::new(11);
    let point_hierarchy = I32GridHierarchy::new(17);
    let max_lod = LodLevel::from_level(10);
    let sample_factory = GridCenterSamplingFactory::new(point_hierarchy);
    let las_loader = I32LasReadWrite::new(settings.compression, false, true, true);
    let page_loader = OctreePageLoader::new(las_loader.clone(), data_folder.clone());
    let mut directory_file_name = data_folder.clone();
    directory_file_name.push("directory.bin");
    let page_directory = GridCellDirectory::new(&max_lod, directory_file_name).unwrap();
    let (max_bogus_inner, max_bogus_leaf) = settings.nr_bogus_points;

    Octree::new(OctreeParams {
        num_threads: settings.num_threads,
        priority_function: settings.priority_function,
        max_lod,
        max_bogus_inner,
        max_bogus_leaf,
        node_hierarchy,
        page_loader,
        page_directory,
        max_cache_size: settings.cache_size,
        sample_factory,
        loader: las_loader,
        coordinate_system,
        metrics: None,
        use_point_colors: false,
        use_point_times: false,
    })
}
