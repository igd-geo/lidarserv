use crate::cli::{InitOptions};
use crate::common::geometry::grid::LodLevel;
use crate::index::settings::{
    GeneralSettings, IndexSettings, OctreeSettings,
};
use anyhow::Result;

pub fn run(init_options: InitOptions) -> Result<()> {
    // create the directory
    std::fs::create_dir_all(&init_options.path)?;

    // write settings
    let settings = IndexSettings {
        general_settings: GeneralSettings {
            nr_threads: init_options.num_threads,
            max_cache_size: init_options.cache_size,
            las_scale: init_options.las_scale.0,
            las_offset: init_options.las_offset.0,
            use_compression: !init_options.las_no_compression,
            use_color: init_options.las_color,
            use_time: init_options.las_time,
        },
        octree_settings: OctreeSettings {
            priority_function: init_options.mno_task_priority,
            max_lod: LodLevel::from_level(init_options.max_lod),
            max_bogus_inner: init_options
                .mno_bogus_inner
                .unwrap_or(init_options.mno_bogus),
            max_bogus_leaf: init_options
                .mno_bogus_leaf
                .unwrap_or(init_options.mno_bogus),
            enable_attribute_indexing: init_options.enable_attribute_indexing,
            use_metrics: init_options.mno_use_metrics,
            point_grid_shift: 31
                - (init_options.point_grid_size / init_options.las_scale.0.x)
                .log2()
                .round() as u16,
            node_grid_shift: 31
                - (init_options.mno_node_grid_size / init_options.las_scale.0.x)
                .log2()
                .round() as u16,
        }
    };
    settings.save_to_data_folder(&init_options.path)?;
    Ok(())
}
