use crate::settings::{Base, SingleIndex};
use crate::Point;
use lidarserv_common::geometry::grid::{GridHierarchy, LodLevel};
use lidarserv_common::geometry::position::I32CoordinateSystem;
use lidarserv_common::geometry::sampling::{GridCenterSampling, GridCenterSamplingFactory};
use lidarserv_common::index::octree::attribute_index::AttributeIndex;
use lidarserv_common::index::octree::grid_cell_directory::GridCellDirectory;
use lidarserv_common::index::octree::page_manager::OctreePageLoader;
use lidarserv_common::index::octree::{Octree, OctreeParams};
use lidarserv_common::las::I32LasReadWrite;
use log::info;
use std::path::PathBuf;

pub type I32Octree = Octree<Point, GridCenterSampling<Point>, GridCenterSamplingFactory<Point>>;

pub fn create_octree_index(
    coordinate_system: I32CoordinateSystem,
    base_settings: &Base,
    settings: &SingleIndex,
) -> I32Octree {
    let mut data_folder: PathBuf = base_settings.data_folder.clone();
    data_folder.push("octree");
    let node_hierarchy = GridHierarchy::new(settings.node_hierarchy);
    let point_hierarchy = GridHierarchy::new(settings.point_hierarchy);
    let max_lod = LodLevel::from_level(10);
    let sample_factory = GridCenterSamplingFactory::new(point_hierarchy);
    let las_loader = I32LasReadWrite::new(settings.compression, 3);
    let page_loader = OctreePageLoader::new(las_loader.clone(), data_folder.clone());
    let mut directory_file_name = data_folder.clone();
    directory_file_name.push("directory.bin");
    let page_directory = GridCellDirectory::new(&max_lod, directory_file_name).unwrap();
    let (max_bogus_inner, max_bogus_leaf) = settings.nr_bogus_points;
    let mut attribute_index = None;
    if settings.enable_attribute_index {
        let mut attribute_index_file_name = data_folder.clone();
        attribute_index_file_name.push("attribute_index.bin");
        attribute_index =
            Some(AttributeIndex::new(max_lod.level() as usize, attribute_index_file_name).unwrap());
        if settings.enable_histogram_acceleration {
            attribute_index
                .as_mut()
                .unwrap()
                .set_histogram_acceleration(true);
        }
    }
    let mut histogram_settings =
        lidarserv_common::index::octree::attribute_histograms::HistogramSettings::default();
    if settings.enable_histogram_acceleration {
        histogram_settings =
            lidarserv_common::index::octree::attribute_histograms::HistogramSettings {
                bin_count_intensity: settings.bin_count_intensity,
                bin_count_return_number: settings.bin_count_return_number,
                bin_count_classification: settings.bin_count_classification,
                bin_count_scan_angle_rank: settings.bin_count_scan_angle_rank,
                bin_count_user_data: settings.bin_count_user_data,
                bin_count_point_source_id: settings.bin_count_point_source_id,
                bin_count_color: settings.bin_count_color,
            };
    }

    let octree = Octree::new(OctreeParams {
        num_threads: settings.num_threads,
        priority_function: settings.priority_function,
        max_lod,
        max_bogus_inner,
        max_bogus_leaf,
        attribute_index,
        enable_histogram_acceleration: settings.enable_histogram_acceleration,
        histogram_settings,
        node_hierarchy,
        page_loader,
        page_directory,
        max_cache_size: settings.cache_size,
        sample_factory,
        loader: las_loader,
        coordinate_system,
        metrics: None,
        point_record_format: 3,
    });
    info!("{:?}", octree);
    octree
}
