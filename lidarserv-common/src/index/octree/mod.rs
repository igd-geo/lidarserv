pub mod grid_cell_directory;
pub mod live_metrics_collector;
pub mod page_manager;
pub mod reader;
pub mod writer;

use crate::geometry::grid::{I32GridHierarchy, LeveledGridCell, LodLevel};
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{I32CoordinateSystem, I32Position};
use crate::geometry::sampling::{Sampling, SamplingFactory};
use crate::index::octree::grid_cell_directory::GridCellDirectory;
use crate::index::octree::live_metrics_collector::LiveMetricsCollector;
use crate::index::octree::page_manager::{LasPageManager, OctreePageLoader, Page};
use crate::index::octree::reader::OctreeReader;
use crate::index::octree::writer::{OctreeWriter, TaskPriorityFunction};
use crate::index::Index;
use crate::las::{I32LasReadWrite, LasExtraBytes, LasPointAttributes};
use crate::query::Query;
use std::error::Error;
use std::sync::{Arc, Mutex};
use thiserror::Error;

struct Inner<Point, Sampl, SamplF> {
    num_threads: u16,
    priority_function: TaskPriorityFunction,
    max_lod: LodLevel,
    max_bogus_inner: usize,
    max_bogus_leaf: usize,
    node_hierarchy: I32GridHierarchy,
    subscriptions: Mutex<Vec<crossbeam_channel::Sender<LeveledGridCell>>>,
    page_cache: LasPageManager<Sampl, Point>,
    sample_factory: SamplF,
    loader: I32LasReadWrite,
    coordinate_system: I32CoordinateSystem,
    metrics: Arc<LiveMetricsCollector>,
    use_point_colors: bool,
}

pub struct OctreeParams<Point, Sampl, SamplF> {
    pub num_threads: u16,
    pub priority_function: TaskPriorityFunction,
    pub max_lod: LodLevel,
    pub max_bogus_inner: usize,
    pub max_bogus_leaf: usize,
    pub node_hierarchy: I32GridHierarchy,
    pub page_loader: OctreePageLoader<Page<Sampl, Point>>,
    pub page_directory: GridCellDirectory,
    pub max_cache_size: usize,
    pub sample_factory: SamplF,
    pub loader: I32LasReadWrite,
    pub coordinate_system: I32CoordinateSystem,
    pub metrics: Option<LiveMetricsCollector>,
    pub use_point_colors: bool,
}

pub struct Octree<Point, Sampl, SamplF> {
    inner: Arc<Inner<Point, Sampl, SamplF>>,
}

#[derive(Error, Debug)]
#[error("Error while flushing to disk: {0}")]
pub struct FlushError(String);

impl<Point, Sampl, SamplF> Octree<Point, Sampl, SamplF>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes + Clone,
    Sampl: Sampling<Point = Point>,
{
    pub fn new(params: OctreeParams<Point, Sampl, SamplF>) -> Self {
        let OctreeParams {
            num_threads,
            priority_function,
            max_lod,
            max_bogus_inner,
            max_bogus_leaf,
            node_hierarchy,
            page_loader,
            page_directory,
            max_cache_size,
            sample_factory,
            loader,
            coordinate_system,
            metrics,
            use_point_colors,
        } = params;
        Octree {
            inner: Arc::new(Inner {
                num_threads,
                priority_function,
                max_lod,
                max_bogus_inner,
                max_bogus_leaf,
                node_hierarchy,
                subscriptions: Mutex::new(vec![]),
                page_cache: LasPageManager::new(page_loader, page_directory, max_cache_size),
                sample_factory,
                loader,
                coordinate_system,
                metrics: Arc::new(
                    metrics.unwrap_or_else(LiveMetricsCollector::new_discarding_collector),
                ),
                use_point_colors,
            }),
        }
    }

    pub fn coordinate_system(&self) -> &I32CoordinateSystem {
        &self.inner.coordinate_system
    }

    pub fn sampling_factory(&self) -> &SamplF {
        &self.inner.sample_factory
    }

    pub fn use_point_colors(&self) -> bool {
        self.inner.use_point_colors
    }

    pub fn flush(&mut self) -> Result<(), FlushError> {
        self.inner
            .page_cache
            .flush()
            .map_err(|e| FlushError(format!("{}", e)))?;

        let mut directory = self.inner.page_cache.directory();
        directory
            .write_to_file()
            .map_err(|e| FlushError(format!("{}", e)))?;

        Ok(())
    }
}

impl<Point, Sampl, SamplF> Index<Point> for Octree<Point, Sampl, SamplF>
where
    Point: PointType<Position = I32Position>
        + WithAttr<LasPointAttributes>
        + LasExtraBytes
        + Clone
        + Send
        + Sync
        + 'static,
    Sampl: Sampling<Point = Point> + Clone + Send + Sync + 'static,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl> + Send + Sync + 'static,
{
    type Writer = OctreeWriter<Point>;
    type Reader = OctreeReader<Point, Sampl, SamplF>;

    fn writer(&self) -> Self::Writer {
        OctreeWriter::new(Arc::clone(&self.inner))
    }

    fn reader<Q>(&self, query: Q) -> Self::Reader
    where
        Q: Query + 'static + Send + Sync,
    {
        OctreeReader::new(query, Arc::clone(&self.inner))
    }

    fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        Octree::flush(self).map_err(|e| Box::new(e) as Box<dyn Error>)
    }
}
