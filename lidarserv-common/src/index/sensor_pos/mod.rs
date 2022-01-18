pub mod meta_tree;
pub mod page_manager;
pub mod partitioned_node;
pub mod point;
pub mod reader;
pub mod writer;

use crate::geometry::bounding_box::AABB;
use crate::geometry::grid::LodLevel;
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{I32CoordinateSystem, I32Position};
use crate::geometry::sampling::{Sampling, SamplingFactory};
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId};
use crate::index::sensor_pos::page_manager::PageManager;
use crate::index::sensor_pos::point::SensorPositionAttribute;
use crate::index::sensor_pos::reader::SensorPosReader;
use crate::index::sensor_pos::writer::SensorPosWriter;
use crate::index::Index;
use crate::las::LasReadWrite;
use crate::lru_cache::pager::IoError;
use crate::query::Query;
pub use las::Point as LasPoint;
use std::error::Error;
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub struct SensorPosIndex<SamplF, LasL, Point, Sampl> {
    inner: Arc<Inner<SamplF, LasL, Point, Sampl>>,
}

pub struct SensorPosIndexParams<SamplF, LasL, Point, Sampl> {
    pub nr_threads: usize,
    pub max_node_size: usize,
    pub sampling_factory: SamplF,
    pub page_manager: PageManager<Point, Sampl, LasL>,
    pub meta_tree_file: PathBuf,
    pub meta_tree: MetaTree,
    pub las_loader: LasL,
    pub coordinate_system: I32CoordinateSystem,
    pub max_lod: LodLevel,
    pub max_delay: Duration,
    pub coarse_lod_steps: usize,
}

struct Inner<SamplF, LasL, Point, Sampl> {
    pub nr_threads: usize,
    pub max_node_size: usize,
    pub page_manager: PageManager<Point, Sampl, LasL>,
    pub sampling_factory: SamplF,
    pub meta_tree_file: PathBuf,
    pub las_loader: LasL,
    pub coordinate_system: I32CoordinateSystem,
    pub shared: RwLock<Shared>,
    pub max_lod: LodLevel,
    pub max_node_split_level: LodLevel,
    pub max_delay: Duration,
    pub coarse_lod_steps: usize,
}

struct Shared {
    meta_tree: MetaTree,
    readers: Vec<crossbeam_channel::Sender<Update>>,
}

#[derive(Clone, Debug)]
struct Update {
    node: MetaTreeNodeId,
    replaced_by: Vec<Replacement>,
}

#[derive(Clone, Debug)]
struct Replacement {
    replace_with: MetaTreeNodeId,
    bounds: AABB<i32>,
}

impl<SamplF, LasL, Point, Sampl> SensorPosIndex<SamplF, LasL, Point, Sampl>
where
    Point: PointType<Position = I32Position> + Clone,
    LasL: LasReadWrite<Point> + Clone,
    Sampl: Sampling<Point = Point>,
{
    pub fn new(params: SensorPosIndexParams<SamplF, LasL, Point, Sampl>) -> Self {
        let SensorPosIndexParams {
            nr_threads,
            max_node_size,
            sampling_factory,
            page_manager,
            meta_tree_file,
            meta_tree,
            las_loader,
            coordinate_system,
            max_lod,
            max_delay,
            coarse_lod_steps,
        } = params;
        SensorPosIndex {
            inner: Arc::new(Inner {
                nr_threads,
                max_node_size,
                sampling_factory,
                page_manager,
                meta_tree_file,
                las_loader,
                coordinate_system,
                max_lod,
                max_node_split_level: if max_lod <= meta_tree.sensor_grid_hierarchy().max_level() {
                    max_lod
                } else {
                    meta_tree.sensor_grid_hierarchy().max_level()
                },
                max_delay,
                shared: RwLock::new(Shared {
                    meta_tree,
                    readers: vec![],
                }),
                coarse_lod_steps,
            }),
        }
    }

    pub fn coordinate_system(&self) -> &I32CoordinateSystem {
        &self.inner.coordinate_system
    }

    pub fn sampling_factory(&self) -> &SamplF {
        &self.inner.sampling_factory
    }

    pub fn flush(&self) -> Result<(), IoError> {
        self.inner.page_manager.flush()
    }
}

impl<SamplF, Point, LasL, Sampl> Index<Point> for SensorPosIndex<SamplF, LasL, Point, Sampl>
where
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl> + Send + Sync + 'static,
    Point: PointType<Position = I32Position>
        + WithAttr<SensorPositionAttribute>
        + Clone
        + Send
        + Sync
        + 'static,
    LasL: LasReadWrite<Point> + Clone + Send + Sync + 'static,
    Sampl: Sampling<Point = Point> + Send + Sync + Clone + 'static,
{
    type Writer = SensorPosWriter<Point>;
    type Reader = SensorPosReader<SamplF, LasL, Point, Sampl>;

    fn writer(&self) -> Self::Writer {
        let index_inner = Arc::clone(&self.inner);
        SensorPosWriter::new(index_inner)
    }

    fn reader<Q>(&self, query: Q) -> Self::Reader
    where
        Q: Query + 'static + Send + Sync,
    {
        SensorPosReader::new(query, Arc::clone(&self.inner))
    }

    fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        SensorPosIndex::flush(self).map_err(|e| Box::new(e) as _)
    }
}
