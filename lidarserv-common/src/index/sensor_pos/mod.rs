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
use crate::las::{I32LasReadWrite, LasExtraBytes, LasPointAttributes};
use crate::lru_cache::pager::IoError;
use crate::query::Query;
pub use las::Point as LasPoint;
use std::error::Error;
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub struct SensorPosIndex<SamplF, Point, Sampl> {
    inner: Arc<Inner<SamplF, Point, Sampl>>,
}

pub struct SensorPosIndexParams<SamplF, Point, Sampl> {
    pub nr_threads: usize,
    pub max_node_size: usize,
    pub sampling_factory: SamplF,
    pub page_manager: PageManager<Point, Sampl>,
    pub meta_tree_file: PathBuf,
    pub meta_tree: MetaTree,
    pub las_loader: I32LasReadWrite,
    pub coordinate_system: I32CoordinateSystem,
    pub max_lod: LodLevel,
    pub max_delay: Duration,
    pub coarse_lod_steps: usize,
}

struct Inner<SamplF, Point, Sampl> {
    pub nr_threads: usize,
    pub max_node_size: usize,
    pub page_manager: PageManager<Point, Sampl>,
    pub sampling_factory: SamplF,
    pub meta_tree_file: PathBuf,
    pub las_loader: I32LasReadWrite,
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

impl<SamplF, Point, Sampl> SensorPosIndex<SamplF, Point, Sampl>
where
    Point: PointType<Position = I32Position> + WithAttr<LasPointAttributes> + LasExtraBytes + Clone,
    Sampl: Sampling<Point = Point>,
{
    pub fn new(params: SensorPosIndexParams<SamplF, Point, Sampl>) -> Self {
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

impl<SamplF, Point, Sampl> Index<Point> for SensorPosIndex<SamplF, Point, Sampl>
where
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl> + Send + Sync + 'static,
    Point: PointType<Position = I32Position>
        + WithAttr<SensorPositionAttribute>
        + WithAttr<LasPointAttributes>
        + LasExtraBytes
        + Clone
        + Send
        + Sync
        + 'static,
    Sampl: Sampling<Point = Point> + Send + Sync + Clone + 'static,
{
    type Writer = SensorPosWriter<Sampl, Point>;
    type Reader = SensorPosReader<SamplF, Point, Sampl>;

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
