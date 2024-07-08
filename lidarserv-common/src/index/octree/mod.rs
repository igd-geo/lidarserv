pub mod attribute_bounds;
pub mod attribute_histograms;
pub mod attribute_index;
pub mod grid_cell_directory;
pub mod histogram;
pub mod live_metrics_collector;
pub mod page_manager;
pub mod reader;
pub mod writer;

use crate::geometry::grid::{GridCell, I32GridHierarchy, LeveledGridCell, LodLevel};
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{CoordinateSystem, I32CoordinateSystem, I32Position};
use crate::geometry::sampling::{Sampling, SamplingFactory};
use crate::index::octree::attribute_histograms::HistogramSettings;
use crate::index::octree::attribute_index::AttributeIndex;
use crate::index::octree::grid_cell_directory::GridCellDirectory;
use crate::index::octree::live_metrics_collector::LiveMetricsCollector;
use crate::index::octree::page_manager::{LasPageManager, OctreePageLoader, Page};
use crate::index::octree::reader::OctreeReader;
use crate::index::octree::writer::{OctreeWriter, TaskPriorityFunction};
use crate::index::Index;
use crate::las::{I32LasReadWrite, LasPointAttributes};
use crate::query::Query;
use log::{debug, info, warn};
use serde::Serialize;
use serde_json::{json, Value};
use std::error::Error;
use std::fmt::Formatter;
use std::option::Option;
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

impl<Point, Sampl, SamplF> std::fmt::Debug for Octree<Point, Sampl, SamplF>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + Clone,
    Sampl: Sampling<Point = Point>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Octree")
            .field("num_threads", &self.inner.num_threads)
            .field("max_lod", &self.inner.max_lod)
            .field("max_bogus_inner", &self.inner.max_bogus_inner)
            .field("max_bogus_leaf", &self.inner.max_bogus_leaf)
            .field("node_hierarchy", &self.inner.node_hierarchy)
            // .field("page_cache", &self.inner.page_cache)
            // .field("attribute_index", &self.inner.attribute_index)
            .field(
                "enable_histogram_acceleration",
                &self.inner.enable_histogram_acceleration,
            )
            .field("histogram_settings", &self.inner.histogram_settings)
            // .field("sample_factory", &self.inner.sample_factory)
            .field("loader", &self.inner.loader)
            .field("coordinate_system", &self.inner.coordinate_system)
            // .field("metrics", &self.inner.metrics)
            .finish()
    }
}

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
                point_record_format,
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
        let size = self.inner.page_cache.size();
        debug!(
            "Flushing all octree pages: max={:?}, curr={:?}",
            size.0, size.1
        );
        self.inner
            .page_cache
            .flush()
            .map_err(|e| FlushError(format!("{}", e)))?;

        debug!("Flushing directory");
        let mut directory = self.inner.page_cache.directory();
        directory
            .write_to_file()
            .map_err(|e| FlushError(format!("{}", e)))?;

        debug!("Flushing attribute index");
        let attribute_index = &self.inner.attribute_index;
        match attribute_index {
            Some(index) => index
                .write_to_file_if_dirty()
                .map_err(|e| FlushError(format!("{}", e)))?,
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

    fn index_info(&self) -> Value {
        let attribute_index_size = match &self.inner.attribute_index {
            Some(index) => index.size(),
            None => 0,
        };
        let histogram_points = match &self.inner.attribute_index {
            Some(index) => index.histogram_points(),
            None => json!("N/A"),
        };
        // Calculate the size of the root cell
        let root_cell: LeveledGridCell = LeveledGridCell {
            lod: LodLevel::from_level(0),
            pos: GridCell { x: 0, y: 0, z: 0 },
        };
        let root_cell_size: I32Position = self
            .inner
            .node_hierarchy
            .get_leveled_cell_bounds(&root_cell)
            .max();
        let root_cell_size_decoded = self.coordinate_system().decode_position(&root_cell_size);

        // calculate the spacing in lod0
        let root_point_spacing = self
            .inner
            .sample_factory
            .build(&LodLevel::base())
            .point_distance();
        let root_point_spacing_decoded =
            self.coordinate_system().decode_distance(root_point_spacing);

        // calculate tthe number of points on each lod
        info!("Calculating point statistics...");
        #[derive(Debug, Default, Serialize)]
        struct LodPointCounter {
            nr_points: u64,
            nr_bogus_points: u64,
        }
        let mut nr_counting_errors = 0;
        let mut nr_points_by_lod = vec![];
        for lod_level in 0..=self.inner.max_lod.level() {
            debug!("Calculating point statistics for LOD {lod_level}...");
            while nr_points_by_lod.len() <= lod_level as usize {
                nr_points_by_lod.push(LodPointCounter::default())
            }
            let counter = &mut nr_points_by_lod[lod_level as usize];
            let lod = LodLevel::from_level(lod_level);
            let nodes = self.inner.page_cache.directory().get_cells_for_lod(&lod);
            for node in nodes {
                match self.inner.page_cache.load(&node) {
                    Ok(Some(page)) => {
                        let loaded = page.get_node(
                            &self.inner.loader,
                            || self.inner.sample_factory.build(&lod),
                            &self.inner.coordinate_system,
                        );
                        match loaded {
                            Ok(node) => {
                                counter.nr_points += node.sampling.len() as u64;
                                counter.nr_bogus_points += node.bogus_points.len() as u64;
                            }
                            Err(e) => {
                                warn!("Error loading node {node:?}: {e}");
                                nr_counting_errors += 1
                            }
                        }
                    }
                    Ok(None) => {
                        warn!("Node {node:?} does not exist.");
                        nr_counting_errors += 1
                    }
                    Err(e) => {
                        warn!("Error loading node {node:?}: {e}");
                        nr_counting_errors += 1
                    }
                }
            }
        }
        let lod_point_statistics = if nr_counting_errors == 0 {
            json!(nr_points_by_lod)
        } else {
            json!(null)
        };
        json!(
            {
                "attribute_index": attribute_index_size,
                "histogram_points": histogram_points,
                "root_cell_size": root_cell_size_decoded,
                "root_point_distance": root_point_spacing_decoded,
                "directory_info": self.inner.page_cache.directory().info(),
                "num_points_per_level": lod_point_statistics,
            }
        )
    }
}
