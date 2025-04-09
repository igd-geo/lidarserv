use crate::cli::InitOptions;
use anyhow::Result;
use dialoguer::{
    Confirm, Input, MultiSelect, Select,
    theme::{ColorfulTheme, Theme},
};
use lidarserv_common::{
    geometry::{
        coordinate_system::CoordinateSystem,
        grid::{GridHierarchy, LodLevel},
        position::{POSITION_ATTRIBUTE_NAME, PositionComponentType, WithComponentTypeOnce},
    },
    index::{
        attribute_index::config::{AttributeIndexConfig, IndexKind, SfcIndexOptions},
        priority_function::TaskPriorityFunction,
    },
};
use lidarserv_server::index::settings::IndexSettings;
use nalgebra::vector;
use pasture_core::layout::{
    PointAttributeDataType, PointAttributeDefinition, PointLayout,
    attributes::{
        CLASSIFICATION, CLASSIFICATION_FLAGS, COLOR_RGB, EDGE_OF_FLIGHT_LINE, GPS_TIME, INTENSITY,
        NIR, NORMAL, NUMBER_OF_RETURNS, POINT_ID, POINT_SOURCE_ID, RETURN_NUMBER,
        RETURN_POINT_WAVEFORM_LOCATION, SCAN_ANGLE, SCAN_ANGLE_RANK, SCAN_DIRECTION_FLAG,
        SCANNER_CHANNEL, WAVE_PACKET_DESCRIPTOR_INDEX, WAVEFORM_DATA_OFFSET, WAVEFORM_PACKET_SIZE,
        WAVEFORM_PARAMETERS,
    },
};
use pasture_io::{
    las::{
        ATTRIBUTE_BASIC_FLAGS, ATTRIBUTE_EXTENDED_FLAGS, ATTRIBUTE_LOCAL_LAS_POSITION,
        point_layout_from_las_point_format,
    },
    las_rs::point::Format,
};
use std::{borrow::Cow, collections::HashSet, num::NonZero, ops::RangeInclusive, path::PathBuf};

fn las_attributes(point_format: u8, exact_binary_repr: bool) -> Vec<PointAttributeDefinition> {
    let format = Format::new(point_format).unwrap();
    let layout = point_layout_from_las_point_format(&format, exact_binary_repr).unwrap();
    let mut attrs: Vec<_> = layout
        .attributes()
        .map(|a| a.attribute_definition().clone())
        .collect();

    // rename the position attribute to "our" position attribute.
    for attr in &mut attrs {
        if attr.name() == ATTRIBUTE_LOCAL_LAS_POSITION.name() {
            *attr = PointAttributeDefinition::custom(
                Cow::Borrowed(POSITION_ATTRIBUTE_NAME),
                attr.datatype(),
            );
        }
    }
    attrs
}

fn attributes_interactive(theme: &dyn Theme) -> Vec<PointAttributeDefinition> {
    let s = Select::with_theme(theme)
        .with_prompt("Select a point format preset:")
        .item("Position only - 32 bit integer")
        .item("Position only - 64 bit floating point")
        .item("LAS point format 0")
        .item("LAS point format 1")
        .item("LAS point format 2")
        .item("LAS point format 3")
        .item("LAS point format 4")
        .item("LAS point format 5")
        .item("LAS point format 6")
        .item("LAS point format 7")
        .item("LAS point format 8")
        .item("LAS point format 9")
        .item("LAS point format 10")
        .item("LAS point format 0 - RAW")
        .item("LAS point format 1 - RAW")
        .item("LAS point format 2 - RAW")
        .item("LAS point format 3 - RAW")
        .item("LAS point format 4 - RAW")
        .item("LAS point format 5 - RAW")
        .item("LAS point format 6 - RAW")
        .item("LAS point format 7 - RAW")
        .item("LAS point format 8 - RAW")
        .item("LAS point format 9 - RAW")
        .item("LAS point format 10 - RAW")
        .default(2)
        .interact()
        .unwrap();

    let mut attributes = match s {
        0 => vec![PointAttributeDefinition::custom(
            Cow::Borrowed(POSITION_ATTRIBUTE_NAME),
            PointAttributeDataType::Vec3i32,
        )],
        1 => vec![PointAttributeDefinition::custom(
            Cow::Borrowed(POSITION_ATTRIBUTE_NAME),
            PointAttributeDataType::Vec3f64,
        )],
        2 => las_attributes(0, false),
        3 => las_attributes(1, false),
        4 => las_attributes(2, false),
        5 => las_attributes(3, false),
        6 => las_attributes(4, false),
        7 => las_attributes(5, false),
        8 => las_attributes(6, false),
        9 => las_attributes(7, false),
        10 => las_attributes(8, false),
        11 => las_attributes(9, false),
        12 => las_attributes(10, false),
        13 => las_attributes(0, true),
        14 => las_attributes(1, true),
        15 => las_attributes(2, true),
        16 => las_attributes(3, true),
        17 => las_attributes(4, true),
        18 => las_attributes(5, true),
        19 => las_attributes(6, true),
        20 => las_attributes(7, true),
        21 => las_attributes(8, true),
        22 => las_attributes(9, true),
        23 => las_attributes(10, true),
        _ => unreachable!(),
    };

    loop {
        println!("You have added the following point attributes so far:");
        for attr in &attributes {
            println!(" - {} ({})", attr.name(), attr.datatype());
        }

        let s = Select::with_theme(theme)
            .with_prompt("Edit attributes:")
            .item("Add predefined attribute(s).")
            .item("Add custom attribute.")
            .item("Remove attribute(s).")
            .item("Delete all attributes and start over.")
            .item("Done.")
            .default(4)
            .interact()
            .unwrap();
        match s {
            0 => add_predefined_attributes(theme, &mut attributes),
            1 => add_custom_attribute(theme, &mut attributes),
            2 => remove_attribute(theme, &mut attributes),
            3 => return attributes_interactive(theme),
            4 => break,
            _ => unreachable!(),
        }
    }

    attributes
}

fn add_predefined_attributes(theme: &dyn Theme, attributes: &mut Vec<PointAttributeDefinition>) {
    let mut predefined_attributes = vec![
        INTENSITY,
        COLOR_RGB,
        CLASSIFICATION,
        CLASSIFICATION_FLAGS,
        RETURN_NUMBER,
        NUMBER_OF_RETURNS,
        SCANNER_CHANNEL,
        SCAN_DIRECTION_FLAG,
        EDGE_OF_FLIGHT_LINE,
        SCAN_ANGLE_RANK,
        SCAN_ANGLE,
        POINT_SOURCE_ID,
        GPS_TIME,
        NIR,
        POINT_ID,
        NORMAL,
        WAVE_PACKET_DESCRIPTOR_INDEX,
        WAVEFORM_DATA_OFFSET,
        WAVEFORM_PACKET_SIZE,
        RETURN_POINT_WAVEFORM_LOCATION,
        WAVEFORM_PARAMETERS,
        ATTRIBUTE_BASIC_FLAGS,
        ATTRIBUTE_EXTENDED_FLAGS,
    ];

    let attribute_names: HashSet<_> = attributes.iter().map(|a| a.name().to_string()).collect();
    predefined_attributes.retain(|attr| !attribute_names.contains(attr.name()));

    if predefined_attributes.is_empty() {
        println!("All predefined attributes have already been added.");
        return;
    }

    let items: Vec<String> = predefined_attributes
        .iter()
        .map(|a| format!("{} ({})", a.name(), a.datatype()))
        .collect();
    let selected = MultiSelect::with_theme(theme)
        .with_prompt("Select attribute(s): ")
        .items(&items)
        .interact()
        .unwrap();
    for index in selected {
        attributes.push(predefined_attributes[index].clone());
    }
}

fn add_custom_attribute(theme: &dyn Theme, attributes: &mut Vec<PointAttributeDefinition>) {
    let name: String = Input::with_theme(theme)
        .with_prompt("Enter attribute name: ")
        .interact()
        .unwrap();
    let name = name.trim();
    if name.is_empty() {
        return;
    }

    let types = vec![
        PointAttributeDataType::U8,
        PointAttributeDataType::I8,
        PointAttributeDataType::U16,
        PointAttributeDataType::I16,
        PointAttributeDataType::U32,
        PointAttributeDataType::I32,
        PointAttributeDataType::U64,
        PointAttributeDataType::I64,
        PointAttributeDataType::F32,
        PointAttributeDataType::F64,
        PointAttributeDataType::Vec3u8,
        PointAttributeDataType::Vec3u16,
        PointAttributeDataType::Vec3f32,
        PointAttributeDataType::Vec3i32,
        PointAttributeDataType::Vec3f64,
        PointAttributeDataType::Vec4u8,
    ];
    let mut type_names: Vec<_> = types.iter().map(|t| t.to_string()).collect();
    type_names.push("ByteArray".to_string());

    let s = Select::with_theme(theme)
        .with_prompt("Select data type:")
        .items(&type_names)
        .interact()
        .unwrap();
    let typ = if s < types.len() {
        types[s]
    } else {
        let len: u64 = Input::with_theme(theme)
            .with_prompt("Len: ")
            .interact()
            .unwrap();
        PointAttributeDataType::ByteArray(len)
    };

    attributes.push(PointAttributeDefinition::custom(
        Cow::Owned(name.to_string()),
        typ,
    ));
}

fn remove_attribute(theme: &dyn Theme, attributes: &mut Vec<PointAttributeDefinition>) {
    if attributes.is_empty() {
        println!("No attributes to delete.");
        return;
    }
    let mut names: Vec<_> = attributes
        .iter()
        .map(|attr| (format!("{} ({})", attr.name(), attr.datatype()), true))
        .collect();
    loop {
        let s = MultiSelect::with_theme(theme)
            .with_prompt("Un-check the attributes to delete:")
            .items_checked(&names)
            .report(false)
            .interact()
            .unwrap();
        let keep: HashSet<_> = s.into_iter().collect();
        let keep_position = keep
            .iter()
            .cloned()
            .any(|index| attributes[index].name() == POSITION_ATTRIBUTE_NAME);
        if !keep_position {
            println!("Cannot remove the position attribute.");
            names.iter_mut().for_each(|(_, select)| *select = false);
            for index in keep {
                names[index].1 = true;
            }
            continue;
        }
        *attributes = attributes
            .iter()
            .enumerate()
            .filter(|(i, _)| keep.contains(i))
            .map(|(_, attr)| attr.clone())
            .collect();
        break;
    }
}

fn attribute_indexes_interactive(
    theme: &dyn Theme,
    attrs: &[PointAttributeDefinition],
) -> Vec<AttributeIndexConfig> {
    let mut result = Vec::new();
    loop {
        let add_attribute_indexes_prompt = if result.is_empty() {
            "Would you like to add any attribute indexes?"
        } else {
            "Would you like to add more attribute indexes?"
        };
        let add_attribute_indexes = Confirm::with_theme(theme)
            .with_prompt(add_attribute_indexes_prompt)
            .default(false)
            .interact()
            .unwrap();
        if !add_attribute_indexes {
            break;
        }

        let selected_attrs = MultiSelect::with_theme(theme)
            .with_prompt("Which attributes would you like to index?")
            .items(attrs)
            .interact()
            .unwrap();

        let s = Select::with_theme(theme)
            .with_prompt("Index type:")
            .item("Range Index")
            .item("Space Filling Curve index")
            .item("(Cancel)")
            .default(0)
            .interact()
            .unwrap();
        let idx = match s {
            0 => IndexKind::RangeIndex,
            1 => {
                let nr_bins = Input::with_theme(theme)
                    .with_prompt("Number of bins:")
                    .default(10)
                    .validate_with(|inp: &usize| {
                        if *inp < 1 {
                            Err("Must be at least 1")
                        } else if *inp > 32 {
                            Err("More than 32 bins are not supported.")
                        } else {
                            Ok(())
                        }
                    })
                    .interact()
                    .unwrap();
                IndexKind::SfcIndex(SfcIndexOptions { nr_bins })
            }
            2 => continue,
            _ => unreachable!(),
        };
        for attr_idx in selected_attrs {
            result.push(AttributeIndexConfig {
                attribute: attrs[attr_idx].clone(),
                path: PathBuf::new(),
                index: idx,
            });
        }
    }

    // set unique filenames
    for (i, index) in result.iter_mut().enumerate() {
        let name_base = index
            .attribute
            .name()
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>();
        index.path = PathBuf::from(format!("attribute_index_{}_{}.bin", i, name_base));
    }
    result
}

fn coordinate_system_interactive(
    theme: &dyn Theme,
    point_layout: &PointLayout,
) -> CoordinateSystem {
    let position = PositionComponentType::from_layout(point_layout);
    let default_scale = match position {
        PositionComponentType::F64 => 1.0,
        PositionComponentType::I32 => 0.001,
    };
    let scale_x: f64 = Input::with_theme(theme)
        .with_prompt("Scale X:")
        .default(default_scale)
        .interact()
        .unwrap();
    let scale_y: f64 = Input::with_theme(theme)
        .with_prompt("Scale Y:")
        .default(scale_x)
        .interact()
        .unwrap();
    let scale_z: f64 = Input::with_theme(theme)
        .with_prompt("Scale Z:")
        .default(scale_x)
        .interact()
        .unwrap();
    let offset_x: f64 = Input::with_theme(theme)
        .with_prompt("Offset X:")
        .default(0.0)
        .interact()
        .unwrap();
    let offset_y: f64 = Input::with_theme(theme)
        .with_prompt("Offset Y:")
        .default(0.0)
        .interact()
        .unwrap();
    let offset_z: f64 = Input::with_theme(theme)
        .with_prompt("Offset Z:")
        .default(0.0)
        .interact()
        .unwrap();
    CoordinateSystem::from_las_transform(
        vector![scale_x, scale_y, scale_z],
        vector![offset_x, offset_y, offset_z],
    )
}

struct OctreeParams {
    node_hierarchy: GridHierarchy,
    point_hierarchy: GridHierarchy,
    max_lod: LodLevel,
}

fn octree_interactive(
    theme: &dyn Theme,
    coordinate_system: &CoordinateSystem,
    layout: &PointLayout,
) -> OctreeParams {
    fn auto_estimate_params(
        theme: &dyn Theme,
        coordinate_system: &CoordinateSystem,
        layout: &PointLayout,
    ) -> Result<OctreeParams, String> {
        let root_size_global: f64 = Input::with_theme(theme)
            .with_prompt("Largest node size in metres:")
            .default(100.0)
            .interact()
            .unwrap();
        let finest_grid_spacing_global: f64 = Input::with_theme(theme)
            .with_prompt("Finest point spacing in metres:")
            .default(0.01)
            .interact()
            .unwrap();
        let sampling_grid_size: u32 = Input::with_theme(theme)
            .with_prompt("Sampling grid size:")
            .default(128)
            .interact()
            .unwrap();

        struct Wct<'a> {
            coordinate_system: &'a CoordinateSystem,
        }
        impl WithComponentTypeOnce for Wct<'_> {
            type Output = (RangeInclusive<i16>, RangeInclusive<f64>);

            fn run_once<C: lidarserv_common::geometry::position::Component>(self) -> Self::Output {
                (
                    C::grid_get_level_minmax(),
                    self.coordinate_system.bounds_distance::<C>(),
                )
            }
        }
        let (allowed_levels, allowed_dist) = Wct { coordinate_system }.for_layout_once(layout);

        // node size
        let shift = sampling_grid_size.ilog2() as i16;
        let allowed_node_levels = (*allowed_levels.start() + shift)..=(*allowed_levels.end());
        if allowed_node_levels.is_empty() {
            let max_shift = allowed_levels.end() - allowed_levels.start();
            let max_sampling_grid_size = if max_shift >= 32 {
                u32::MAX
            } else {
                1_u32 << max_shift
            };
            return Err(format!(
                "The 'Sampling grid size' value is too large. \nReduce 'Sampling grid size' to {max_sampling_grid_size} or less."
            ));
        }

        // node level
        if root_size_global <= 0.0 {
            return Err("The 'Largest node size' value must be larger than 0.".to_string());
        }
        if !allowed_dist.contains(&root_size_global) {
            return Err(format!(
                "The 'Largest node size' value is larger than the size of the coordinate system. Reduce it to {} or less.",
                allowed_dist.end()
            ));
        }
        let root_size: f64 = coordinate_system.encode_distance(root_size_global).unwrap();
        let node_level = root_size.log2().floor() as i16;
        if node_level < *allowed_node_levels.start() {
            let min_node_level = *allowed_node_levels.start();
            let min_root_node_size_local = 2.0_f64.powi(min_node_level as i32);
            let min_root_node_size = coordinate_system.decode_distance(min_root_node_size_local);
            return Err(format!(
                "The 'Largest node size' value is too small.\nYou need to increase the 'Largest node size' and/or decrease the 'Sampling grid size'.\nIf you want to keep the current 'Sampling grid size' of {sampling_grid_size}, then 'Largest node size' must be increased to {min_root_node_size} or above."
            ));
        }
        if node_level > *allowed_node_levels.end() {
            let max_node_level = *allowed_node_levels.end();
            let max_root_node_size_local = 2.0_f64.powi(max_node_level as i32);
            let max_root_node_size = coordinate_system.decode_distance(max_root_node_size_local);
            return Err(format!(
                "The 'Largest node size' value is too large.\nYou need to decrease the 'Largest node size' to {max_root_node_size} or below."
            ));
        }
        assert!(allowed_levels.contains(&node_level));

        // point level
        let point_level = node_level - shift;
        assert!(allowed_levels.contains(&point_level));

        // max lod
        if finest_grid_spacing_global <= 0.0 {
            return Err("The 'Finest point spacing' value must be larger than 0.".to_string());
        }
        if !allowed_dist.contains(&finest_grid_spacing_global) {
            return Err(format!(
                "The 'Finest point spacing' value is larger than the size of the coordinate system. Reduce it to {} or less.",
                allowed_dist.end()
            ));
        }
        let finest_grid_spacing: f64 = coordinate_system
            .encode_distance(finest_grid_spacing_global)
            .unwrap();
        let point_level_finest = finest_grid_spacing.log2().floor() as i16;
        let max_lod = if point_level_finest <= point_level {
            LodLevel::from_level((point_level - point_level_finest) as u8)
        } else {
            let max_point_level_finest = point_level;
            let max_finest_grid_spacing_local = 2.0_f64.powi(max_point_level_finest as i32);
            let max_finest_point_spacing =
                coordinate_system.decode_distance(max_finest_grid_spacing_local);
            return Err(format!(
                "The 'Finest point spacing' value is too large. \nPlease reduce 'Finest point spacing', and/or increase 'Largest node size', and/or reduce 'Sampling grid size'. \nIf you want to keep the current 'Largest node size' of {root_size_global} and 'Sampling grid size' of {sampling_grid_size}, then 'Finest point spacing' must be {max_finest_point_spacing} or smaller."
            ));
        };

        Ok(OctreeParams {
            node_hierarchy: GridHierarchy::new(node_level),
            point_hierarchy: GridHierarchy::new(point_level),
            max_lod,
        })
    }

    fn summary(params: &OctreeParams, coordinate_system: &CoordinateSystem) {
        println!("Based on your input, the following octree parameters have been calculated:");
        println!(" - node hierarchy shift: {}", params.node_hierarchy.shift());
        println!(
            " - point hierarchy shift: {}",
            params.point_hierarchy.shift()
        );
        println!(" - max level of detail: {}", params.max_lod);
        println!();
        println!("With these parameters, the octree will have the following properties:");

        for lod_level in 0..=params.max_lod.level() {
            let lod = LodLevel::from_level(lod_level);
            let node_size = coordinate_system
                .decode_distance(params.node_hierarchy.level::<f64>(lod).cell_size());
            println!(" - In {lod}, the node size is {node_size:0.3} metres.");
        }
        println!(
            " - Each node contains a {0}x{0}x{0} sampling grid.",
            2_i32.pow((params.node_hierarchy.shift() - params.point_hierarchy.shift()) as u32)
        );
        for lod_level in 0..=params.max_lod.level() {
            let lod = LodLevel::from_level(lod_level);
            let point_dist = coordinate_system
                .decode_distance(params.point_hierarchy.level::<f64>(lod).cell_size());
            println!(" - In {lod}, the point distance is {point_dist:0.3} metres.");
        }
    }

    loop {
        match auto_estimate_params(theme, coordinate_system, layout) {
            Ok(params) => {
                summary(&params, coordinate_system);
                let is_ok = Confirm::with_theme(theme)
                    .with_prompt("Does this look acceptable to you?")
                    .default(true)
                    .interact()
                    .unwrap();
                if is_ok {
                    return params;
                }
            }
            Err(e) => println!("{e}"),
        }
    }
}

struct IndexerSettings {
    num_threads: u16,
    cache_size: usize,
    bogus_inner: usize,
    bogus_leaf: usize,
    priority_function: TaskPriorityFunction,
    use_metrics: bool,
}

fn indexer_settings_interactive(theme: &dyn Theme) -> IndexerSettings {
    let nr_threads = Input::with_theme(theme)
        .with_prompt("Number of threads: ")
        .default(
            std::thread::available_parallelism()
                .unwrap_or(NonZero::new(1).unwrap())
                .get() as u16,
        )
        .interact()
        .unwrap();

    let priority_function_choices = [
        TaskPriorityFunction::NrPointsWeightedByTaskAge,
        TaskPriorityFunction::NrPoints,
        TaskPriorityFunction::Lod,
        TaskPriorityFunction::OldestPoint,
        TaskPriorityFunction::TaskAge,
    ];
    let priority_function_names = priority_function_choices.map(|p| p.to_string());
    let s = Select::with_theme(theme)
        .with_prompt("Task priority function: ")
        .items(&priority_function_names)
        .default(0)
        .interact()
        .unwrap();
    let priority_function = priority_function_choices[s];

    let cache_size = Input::with_theme(theme)
        .with_prompt("Cache size (nodes): ")
        .default(5_000)
        .interact()
        .unwrap();

    let bogus_inner = Input::with_theme(theme)
        .with_prompt("Maximum number of bogus points per inner node:")
        .default(500)
        .interact()
        .unwrap();
    let bogus_leaf = Input::with_theme(theme)
        .with_prompt("Maximum number of bogus points per leaf node:")
        .default(5000)
        .interact()
        .unwrap();

    let use_metrics = Confirm::with_theme(theme)
        .with_prompt("Should metrics be recorded during indexing?")
        .default(false)
        .interact()
        .unwrap();

    IndexerSettings {
        num_threads: nr_threads,
        cache_size,
        bogus_inner,
        bogus_leaf,
        priority_function,
        use_metrics,
    }
}

fn print_header(header: &str) {
    println!();
    println!("################################################################################");
    println!("# {header}");
    println!("################################################################################");
}

pub fn run(init_options: InitOptions) -> Result<()> {
    let theme = ColorfulTheme::default();

    // attributes
    print_header("Point Format");
    let attrs: Vec<PointAttributeDefinition> = attributes_interactive(&theme);
    let point_layout = PointLayout::from_attributes(&attrs);
    let s = Select::with_theme(&theme)
        .with_prompt("Point Data compression: ")
        .item("None")
        .item("Lz4")
        .default(0)
        .interact()
        .unwrap();
    let enable_compression = s == 1;

    // coordinate system
    print_header("Coordinate System");
    let coordinate_system = coordinate_system_interactive(&theme, &point_layout);

    // octree setup
    print_header("Octree");
    let octree_params = octree_interactive(&theme, &coordinate_system, &point_layout);

    // indexer settings
    print_header("Indexing");
    let index_params = indexer_settings_interactive(&theme);
    let attribute_indexes = attribute_indexes_interactive(&theme, &attrs);

    // Remaining stuff
    let settings = IndexSettings {
        node_hierarchy: octree_params.node_hierarchy,
        point_hierarchy: octree_params.point_hierarchy,
        coordinate_system,
        point_layout,
        max_lod: octree_params.max_lod,
        priority_function: index_params.priority_function,
        num_threads: index_params.num_threads,
        max_cache_size: index_params.cache_size,
        max_bogus_inner: index_params.bogus_inner,
        max_bogus_leaf: index_params.bogus_leaf,
        use_metrics: index_params.use_metrics,
        enable_compression,
        attribute_indexes,
    };

    // create the directory and write settings json
    std::fs::create_dir_all(&init_options.path)?;
    settings.save_to_data_folder(&init_options.path)?;
    Ok(())
}
