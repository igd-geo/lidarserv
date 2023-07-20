pub mod grid_cell_directory;
pub mod live_metrics_collector;
pub mod page_manager;
pub mod reader;
pub mod writer;
pub mod attribute_index;
pub mod attribute_bounds;
pub mod attribute_histograms;
pub mod histogram;

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
use crate::las::{I32LasReadWrite, LasPointAttributes};
use crate::query::Query;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::option::Option;
use thiserror::Error;
use crate::index::octree::attribute_histograms::HistogramSettings;
use crate::index::octree::attribute_index::AttributeIndex;

struct Inner<Point, Sampl, SamplF> {
    num_threads: u16,
    priority_function: TaskPriorityFunction,
    max_lod: LodLevel,
    max_bogus_inner: usize,
    max_bogus_leaf: usize,
    node_hierarchy: I32GridHierarchy,
    subscriptions: Mutex<Vec<crossbeam_channel::Sender<LeveledGridCell>>>,
    page_cache: LasPageManager<Sampl, Point>,
    attribute_index: Option<AttributeIndex>,
    enable_histogram_acceleration: bool,
    histogram_settings: HistogramSettings,
    sample_factory: SamplF,
    loader: I32LasReadWrite,
    coordinate_system: I32CoordinateSystem,
    metrics: Arc<LiveMetricsCollector>,
    point_record_format: u8,
}

pub struct OctreeParams<Point, Sampl, SamplF> {
    pub num_threads: u16,
    pub priority_function: TaskPriorityFunction,
    pub max_lod: LodLevel,
    pub max_bogus_inner: usize,
    pub max_bogus_leaf: usize,
    pub attribute_index: Option<AttributeIndex>,
    pub enable_histogram_acceleration: bool,
    pub histogram_settings: HistogramSettings,
    pub node_hierarchy: I32GridHierarchy,
    pub page_loader: OctreePageLoader<Page<Sampl, Point>>,
    pub page_directory: GridCellDirectory,
    pub max_cache_size: usize,
    pub sample_factory: SamplF,
    pub loader: I32LasReadWrite,
    pub coordinate_system: I32CoordinateSystem,
    pub metrics: Option<LiveMetricsCollector>,
    pub point_record_format: u8,
}

pub struct Octree<Point, Sampl, SamplF> {
    inner: Arc<Inner<Point, Sampl, SamplF>>,
}

#[derive(Error, Debug)]
#[error("Error while flushing to disk: {0}")]
pub struct FlushError(String);

impl<Point, Sampl, SamplF> Octree<Point, Sampl, SamplF>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + Clone,
    Sampl: Sampling<Point = Point>,
{
    pub fn new(params: OctreeParams<Point, Sampl, SamplF>) -> Self {
        let OctreeParams {
            num_threads,
            priority_function,
            max_lod,
            max_bogus_inner,
            max_bogus_leaf,
            attribute_index,
            enable_histogram_acceleration,
            histogram_settings,
            node_hierarchy,
            page_loader,
            page_directory,
            max_cache_size,
            sample_factory,
            loader,
            coordinate_system,
            metrics,
            point_record_format,
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
                attribute_index,
                enable_histogram_acceleration,
                histogram_settings,
                sample_factory,
                loader,
                coordinate_system,
                metrics: Arc::new(
                    metrics.unwrap_or_else(LiveMetricsCollector::new_discarding_collector),
                ),
                point_record_format
            }),
        }
    }

    pub fn coordinate_system(&self) -> &I32CoordinateSystem {
        &self.inner.coordinate_system
    }

    pub fn sampling_factory(&self) -> &SamplF {
        &self.inner.sample_factory
    }

    pub fn point_record_format(&self) -> u8 {
        self.inner.point_record_format
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

        let attribute_index = &self.inner.attribute_index;
        match attribute_index {
            Some(index) => index.write_to_file().map_err(|e| FlushError(format!("{}", e)))?,
            None => {}
        }
        Ok(())
    }
}

impl<Point, Sampl, SamplF> Index<Point> for Octree<Point, Sampl, SamplF>
where
    Point: PointType<Position = I32Position>
        + WithAttr<LasPointAttributes>
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
