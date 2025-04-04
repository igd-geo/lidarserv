use self::attribute_index::AttributeIndex;
use crate::{
    geometry::{
        self,
        coordinate_system::CoordinateSystem,
        grid::{GridHierarchy, LeveledGridCell, LodLevel},
        position::WithComponentTypeOnce,
    },
    io::{
        InMemoryPointCodec, PointIoError,
        pasture::{Compression, PastureIo},
    },
    lru_cache::pager::PageManager,
    query::Query,
};
use attribute_index::config::AttributeIndexConfig;
use geometry::bounding_box::Aabb;
use grid_cell_directory::GridCellDirectory;
use lazy_node::LazyNode;
use live_metrics_collector::LiveMetricsCollector;
use log::debug;
use page_loader::OctreeLoader;
use pasture_core::layout::PointLayout;
use priority_function::TaskPriorityFunction;
use reader::OctreeReader;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use writer::OctreeWriter;

pub mod attribute_index;
pub mod grid_cell_directory;
pub mod lazy_node;
pub mod live_metrics_collector;
pub mod page_loader;
pub mod priority_function;
pub mod reader;
pub mod writer;

#[derive(Debug, Clone)]
pub struct OctreeParams {
    // file paths
    pub directory_file: PathBuf,
    pub point_data_folder: PathBuf,
    pub metrics_file: Option<PathBuf>,

    // tree settings
    pub point_layout: PointLayout,
    pub node_hierarchy: GridHierarchy,
    pub point_hierarchy: GridHierarchy,
    pub coordinate_system: CoordinateSystem,
    pub max_lod: LodLevel,
    pub max_bogus_inner: usize,
    pub max_bogus_leaf: usize,
    pub enable_compression: bool,

    // indexing settings
    pub max_cache_size: usize,
    pub priority_function: TaskPriorityFunction,
    pub num_threads: u16,

    // attribute indexing
    pub attribute_indexes: Vec<AttributeIndexConfig>,
}

pub struct Octree {
    inner: Arc<Inner>,
}

type OctreeNodeCache =
    PageManager<OctreeLoader, LeveledGridCell, LazyNode, PointIoError, GridCellDirectory>;

struct Inner {
    num_threads: u16,
    priority_function: TaskPriorityFunction,
    subscriptions: Mutex<Vec<crossbeam_channel::Sender<LeveledGridCell>>>,
    page_cache: OctreeNodeCache,
    codec: Arc<dyn InMemoryPointCodec + Send + Sync>,
    point_layout: PointLayout,
    max_lod: LodLevel,
    point_hierarchy: GridHierarchy,
    node_hierarchy: GridHierarchy,
    max_bogus_inner: usize,
    max_bogus_leaf: usize,
    metrics: Arc<LiveMetricsCollector>,
    coordinate_system: CoordinateSystem,
    attribute_index: Arc<AttributeIndex>,
}

impl Octree {
    pub fn new(config: OctreeParams) -> anyhow::Result<Self> {
        let page_directory = GridCellDirectory::new(config.max_lod, config.directory_file.clone())?;
        let codec = PastureIo::new(if config.enable_compression {
            Compression::Lz4
        } else {
            Compression::None
        });
        let codec: Arc<dyn InMemoryPointCodec + Send + Sync> = Arc::new(codec);
        let page_loader = OctreeLoader::new(config.point_data_folder.clone(), Arc::clone(&codec));
        let page_cache = OctreeNodeCache::new(page_loader, page_directory, config.max_cache_size);
        let metrics = match config.metrics_file {
            Some(file) => LiveMetricsCollector::new_file_backed_collector(&file)?,
            None => LiveMetricsCollector::new_discarding_collector(),
        };
        let metrics = Arc::new(metrics);

        let mut attribute_index = AttributeIndex::new();
        for attribute_index_cfg in config.attribute_indexes {
            attribute_index.add_index_from_config(
                attribute_index_cfg.attribute,
                &attribute_index_cfg.index,
                attribute_index_cfg.path,
            )?
        }

        Ok(Octree {
            inner: Arc::new(Inner {
                subscriptions: Mutex::new(vec![]),
                page_cache,
                codec,
                metrics,
                point_layout: config.point_layout,
                max_lod: config.max_lod,
                point_hierarchy: config.point_hierarchy,
                node_hierarchy: config.node_hierarchy,
                max_bogus_inner: config.max_bogus_inner,
                max_bogus_leaf: config.max_bogus_leaf,
                priority_function: config.priority_function,
                num_threads: config.num_threads,
                coordinate_system: config.coordinate_system,
                attribute_index: Arc::new(attribute_index),
            }),
        })
    }

    pub fn flush(&mut self) -> Result<(), anyhow::Error> {
        let size = self.inner.page_cache.size();
        debug!(
            "Flushing all octree pages: max={:?}, curr={:?}",
            size.0, size.1
        );
        self.inner.page_cache.flush()?;

        debug!("Flushing directory");
        let mut directory = self.inner.page_cache.directory();
        directory.write_to_file()?;

        debug!("Flushing attribute index");
        self.inner.attribute_index.flush()?;
        //let attribute_index = &self.inner.attribute_index;
        //match attribute_index {
        //    Some(index) => index.write_to_file_if_dirty()?,
        //    None => {}
        //}
        Ok(())
    }

    pub fn coordinate_system(&self) -> CoordinateSystem {
        self.inner.coordinate_system
    }

    pub fn point_layout(&self) -> &PointLayout {
        &self.inner.point_layout
    }

    pub fn node_hierarchy(&self) -> GridHierarchy {
        self.inner.node_hierarchy
    }

    pub fn point_hierarchy(&self) -> GridHierarchy {
        self.inner.point_hierarchy
    }

    pub fn writer(&self) -> OctreeWriter {
        OctreeWriter::new(Arc::clone(&self.inner))
    }

    pub fn current_aabb(&self) -> Aabb<f64> {
        struct Wct<'a> {
            octree: &'a Octree,
        }

        impl WithComponentTypeOnce for Wct<'_> {
            type Output = Aabb<f64>;

            fn run_once<C: geometry::position::Component>(self) -> Self::Output {
                let mut aabb: Aabb<f64> = Aabb::empty();
                let node_hierarchy = self.octree.inner.node_hierarchy;
                let root_nodes = self.octree.inner.page_cache.directory().get_root_cells();
                let coordinate_system = self.octree.inner.coordinate_system;
                for node in root_nodes {
                    let bounds_local = node_hierarchy.get_leveled_cell_bounds::<C>(node);
                    let bounds_global_1 = coordinate_system.decode_position(bounds_local.min);
                    let bounds_global_2 = coordinate_system.decode_position(bounds_local.max);
                    aabb.extend(bounds_global_1);
                    aabb.extend(bounds_global_2);
                }
                aabb
            }
        }

        Wct { octree: self }.for_layout_once(&self.inner.point_layout)
    }

    pub fn reader<Q: Query>(&self, initial_query: Q) -> Result<OctreeReader, Q::Error> {
        OctreeReader::new(Arc::clone(&self.inner), initial_query)
    }

    pub fn cache_size(&self) -> usize {
        self.inner.page_cache.size().1
    }
}

#[cfg(test)]
mod test {
    use std::any::type_name;

    use super::Inner;

    fn ensure_send_sync_sized_static<T: Send + Sync + Sized + 'static>() {
        let name = type_name::<T>();
        println!("Type {name} is Send + Sync + Sized + 'static."); // test would not compile otherwise.
    }

    #[test]
    fn octree_inner_is_send() {
        ensure_send_sync_sized_static::<Inner>()
    }
}
