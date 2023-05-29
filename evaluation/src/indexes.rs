use crate::settings::{Base, SingleIndex};
use crate::Point;
use lidarserv_common::geometry::grid::{I32GridHierarchy, LodLevel};
use lidarserv_common::geometry::position::I32CoordinateSystem;
use lidarserv_common::geometry::sampling::{GridCenterSampling, GridCenterSamplingFactory};
use lidarserv_common::index::octree::grid_cell_directory::GridCellDirectory;
use lidarserv_common::index::octree::page_manager::OctreePageLoader;
use lidarserv_common::index::octree::{Octree, OctreeParams};
use lidarserv_common::las::I32LasReadWrite;
use std::path::PathBuf;

pub type I32Octree = Octree<Point, GridCenterSampling<Point>, GridCenterSamplingFactory<Point>>;

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
    let las_loader = I32LasReadWrite::new(settings.compression, false, true);
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
        attribute_index: None,
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
