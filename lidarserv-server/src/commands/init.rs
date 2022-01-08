use crate::cli::{Index, InitOptions};
use crate::common::geometry::grid::LodLevel;
use crate::common::index::octree::writer::TaskPriorityFunction;
use crate::index::settings::{
    GeneralSettings, IndexSettings, IndexType, OctreeSettings, SensorPositionSettings,
};
use anyhow::Result;
use lidarserv_common::index::sensor_pos::partitioned_node::RustCellHasher;

pub fn run(init_options: InitOptions) -> Result<()> {
    // create the directory
    std::fs::create_dir_all(&init_options.path)?;

    // write settings
    let settings = IndexSettings {
        general: GeneralSettings {
            nr_threads: init_options.num_threads,
            max_cache_size: init_options.cache_size,
            las_scale: init_options.las_scale.0,
            las_offset: init_options.las_offset.0,
            use_compression: !init_options.las_no_compression,
        },
        index_type: match init_options.index {
            Index::Mno => IndexType::Octree(OctreeSettings {
                priority_function: TaskPriorityFunction::NrPoints,
                max_lod: LodLevel::from_level(init_options.max_lod),
                max_bogus_inner: 0,
                max_bogus_leaf: 0,
            }),
            Index::Bvg => IndexType::SensorPositionIndex(SensorPositionSettings {
                max_nr_points_per_node: init_options.bvg_max_points_per_node,
                hash_state: RustCellHasher::new_random().state(),
            }),
        },
    };
    settings.save_to_data_folder(&init_options.path)?;
    Ok(())
}
