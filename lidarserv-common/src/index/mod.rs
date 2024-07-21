use crate::{
    geometry::{
        coordinate_system::CoordinateSystem,
        grid::{GridHierarchy, LeveledGridCell, LodLevel},
    },
    io::{
        pasture::{Compression, PastureIo},
        InMemoryPointCodec, PointIoError,
    },
    lru_cache::pager::PageManager,
    query::QueryBuilder,
};
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
    //pub page_loader: OctreePageLoader<Page<Sampl, Point>>,
    //pub page_directory: GridCellDirectory,
    //pub loader: I32LasReadWrite,
    //pub coordinate_system: I32CoordinateSystem,
    //pub metrics: Option<LiveMetricsCollector>,
    //pub point_record_format: u8,
    //pub enable_histogram_acceleration: bool,
    //pub attribute_index: Option<AttributeIndex>,
    //pub histogram_settings: HistogramSettings,
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
    //attribute_index: Option<AttributeIndex>,
    //enable_histogram_acceleration: bool,
    //histogram_settings: HistogramSettings,
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

        //debug!("Flushing attribute index");
        //let attribute_index = &self.inner.attribute_index;
        //match attribute_index {
        //    Some(index) => index.write_to_file_if_dirty()?,
        //    None => {}
        //}
        Ok(())
    }

    pub fn writer(&self) -> OctreeWriter {
        OctreeWriter::new(Arc::clone(&self.inner))
    }

    pub fn reader(&self, initial_query: impl QueryBuilder) -> OctreeReader {
        OctreeReader::new(Arc::clone(&self.inner), initial_query)
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
