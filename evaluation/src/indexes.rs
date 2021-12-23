use crate::{Config, Point};
use lidarserv_common::geometry::grid::{I32Grid, I32GridHierarchy, LodLevel};
use lidarserv_common::geometry::position::{I32CoordinateSystem, I32Position};
use lidarserv_common::geometry::sampling::{GridCenterSampling, GridCenterSamplingFactory};
use lidarserv_common::index::octree::grid_cell_directory::GridCellDirectory;
use lidarserv_common::index::octree::page_manager::OctreePageLoader;
use lidarserv_common::index::octree::writer::TaskPriorityFunction;
use lidarserv_common::index::octree::Octree;
use lidarserv_common::index::sensor_pos::meta_tree::MetaTree;
use lidarserv_common::index::sensor_pos::page_manager::{BinDataLoader, FileIdDirectory};
use lidarserv_common::index::sensor_pos::partitioned_node::RustCellHasher;
use lidarserv_common::index::sensor_pos::{SensorPosIndex, SensorPosIndexParams};
use lidarserv_common::las::I32LasReadWrite;
use log::error;
use std::path::PathBuf;
use std::time::Duration;

pub type I32SensorPosIndex = SensorPosIndex<
    I32GridHierarchy,
    GridCenterSamplingFactory<I32GridHierarchy, Point, I32Position, i32>,
    i32,
    I32LasReadWrite,
    I32CoordinateSystem,
>;
pub type I32Octree = Octree<
    Point,
    I32GridHierarchy,
    I32LasReadWrite,
    GridCenterSampling<I32Grid, Point, I32Position, i32>,
    i32,
    I32CoordinateSystem,
    GridCenterSamplingFactory<I32GridHierarchy, Point, I32Position, i32>,
>;

pub fn create_sensor_pos_index(
    coordinate_system: I32CoordinateSystem,
    config: &Config,
) -> I32SensorPosIndex {
    let mut data_folder: PathBuf = config.data_folder.clone();
    data_folder.push("sensorpos");
    let mut meta_tree_file = data_folder.clone();
    meta_tree_file.push("meta.bin");

    let nr_threads = config.num_threads;
    let max_cache_size = config.max_cache_size;

    let point_grid_hierarchy = I32GridHierarchy::new(17);
    let sampling_factory = GridCenterSamplingFactory::new(point_grid_hierarchy);
    let sensor_grid_hierarchy = I32GridHierarchy::new(14);
    let meta_tree = MetaTree::new(sensor_grid_hierarchy);
    let page_loader = BinDataLoader::new(data_folder.clone(), "laz".to_string());
    let directory = FileIdDirectory::from_meta_tree(&meta_tree, nr_threads as usize);
    let page_manager = lidarserv_common::index::sensor_pos::page_manager::PageManager::new(
        page_loader,
        directory,
        max_cache_size,
    );
    let las_loader = I32LasReadWrite::new(config.compression);

    let params = SensorPosIndexParams {
        nr_threads: config.num_threads as usize,
        max_node_size: config.max_node_size,
        meta_tree_file,
        sampling_factory,
        page_manager,
        meta_tree,
        las_loader,
        coordinate_system,
        max_lod: LodLevel::from_level(10),
        max_delay: Duration::from_secs(1),
        coarse_lod_steps: 5,
        hasher: RustCellHasher::from_state((83675784, 435659)),
    };
    SensorPosIndex::new(params)
}

pub fn create_octree_index(coordinate_system: I32CoordinateSystem, config: &Config) -> I32Octree {
    let mut data_folder: PathBuf = config.data_folder.clone();
    data_folder.push("octree");
    let node_hierarchy = I32GridHierarchy::new(11);
    let point_hierarchy = I32GridHierarchy::new(17);
    let max_lod = LodLevel::from_level(10);
    let sample_factory = GridCenterSamplingFactory::new(point_hierarchy);
    let las_loader = I32LasReadWrite::new(config.compression);
    let page_loader = OctreePageLoader::new(las_loader.clone(), data_folder.clone());
    let mut directory_file_name = data_folder.clone();
    directory_file_name.push("directory.bin");
    let page_directory = GridCellDirectory::new(&max_lod, directory_file_name).unwrap();

    Octree::new(
        config.num_threads,
        match config.task_priority_function.as_str() {
            "Lod" => TaskPriorityFunction::Lod,
            "TaskAge" => TaskPriorityFunction::TaskAge,
            "NewestPoint" => TaskPriorityFunction::NewestPoint,
            "NrPoints" => TaskPriorityFunction::NrPoints,
            "NrPointsWeighted1" => TaskPriorityFunction::NrPointsWeightedByTaskAge,
            "NrPointsWeighted2" => TaskPriorityFunction::NrPointsWeightedByOldestPoint,
            "NrPointsWeighted3" => TaskPriorityFunction::NrPointsWeightedByNegNewestPoint,
            "OldestPoint" => TaskPriorityFunction::OldestPoint,
            _ => {
                error!("invalid value for LIDARSERV_TASK_PRIORITY_FUNCTION");
                panic!()
            }
        },
        max_lod,
        config.max_bogus_inner,
        config.max_bogus_leaf,
        node_hierarchy,
        page_loader,
        page_directory,
        config.max_cache_size,
        sample_factory,
        las_loader,
        coordinate_system,
    )
}
