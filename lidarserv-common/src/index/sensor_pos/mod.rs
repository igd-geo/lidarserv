pub mod meta_tree;
pub mod page_manager;
pub mod partitioned_node;
pub mod point;
pub mod reader;
pub mod writer;

use crate::geometry::bounding_box::AABB;
use crate::geometry::grid::{GridHierarchy, LodLevel};
use crate::geometry::points::{PointType, WithAttr};
use crate::geometry::position::{Component, Position};
use crate::geometry::sampling::{RawSamplingEntry, Sampling, SamplingFactory};
use crate::index::sensor_pos::meta_tree::{MetaTree, MetaTreeNodeId};
use crate::index::sensor_pos::page_manager::PageManager;
use crate::index::sensor_pos::partitioned_node::RustCellHasher;
use crate::index::sensor_pos::point::SensorPositionAttribute;
use crate::index::sensor_pos::reader::SensorPosReader;
use crate::index::sensor_pos::writer::SensorPosWriter;
use crate::index::Index;
use crate::las::LasReadWrite;
use crate::lru_cache::pager::IoError;
use crate::nalgebra::Scalar;
use crate::query::Query;
pub use las::Point as LasPoint;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::error::Error;
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub struct SensorPosIndex<GridH, SamplF, Comp: Scalar, LasL, CSys, Point, Sampl> {
    inner: Arc<Inner<GridH, SamplF, Comp, LasL, CSys, Point, Sampl>>,
}

pub struct SensorPosIndexParams<SamplF, GridH, Comp: Scalar, LasL, CSys, Point, Sampl> {
    pub nr_threads: usize,
    pub max_node_size: usize,
    pub sampling_factory: SamplF,
    pub page_manager: PageManager<Point, Comp, Sampl, LasL, CSys>,
    pub meta_tree_file: PathBuf,
    pub meta_tree: MetaTree<GridH, Comp>,
    pub las_loader: LasL,
    pub coordinate_system: CSys,
    pub max_lod: LodLevel,
    pub max_delay: Duration,
    pub coarse_lod_steps: usize,
    pub hasher: RustCellHasher,
}

struct Inner<GridH, SamplF, Comp: Scalar, LasL, CSys, Point, Sampl> {
    pub nr_threads: usize,
    pub max_node_size: usize,
    pub page_manager: PageManager<Point, Comp, Sampl, LasL, CSys>,
    pub sampling_factory: SamplF,
    pub meta_tree_file: PathBuf,
    pub las_loader: LasL,
    pub coordinate_system: CSys,
    pub shared: RwLock<Shared<GridH, Comp>>,
    pub max_lod: LodLevel,
    pub max_node_split_level: LodLevel,
    pub max_delay: Duration,
    pub coarse_lod_steps: usize,
    pub hasher: RustCellHasher,
}

struct Shared<GridH, Comp: Scalar> {
    meta_tree: MetaTree<GridH, Comp>,
    readers: Vec<crossbeam_channel::Sender<Update<Comp>>>,
}

#[derive(Clone, Debug)]
struct Update<Comp>
where
    Comp: Scalar,
{
    node: MetaTreeNodeId,
    replaced_by: Vec<Replacement<Comp>>,
}

#[derive(Clone, Debug)]
struct Replacement<Comp: Scalar> {
    replace_with: MetaTreeNodeId,
    bounds: AABB<Comp>,
}

impl<GridH, SamplF, Comp, LasL, CSys, Point, Sampl>
    SensorPosIndex<GridH, SamplF, Comp, LasL, CSys, Point, Sampl>
where
    GridH: GridHierarchy<Component = Comp>,
    Comp: Component,
    CSys: Clone,
    Point: PointType + Clone,
    Point::Position: Position<Component = Comp>,
    LasL: LasReadWrite<Point, CSys> + Clone,
    Sampl: Sampling<Point = Point>,
{
    pub fn new(
        params: SensorPosIndexParams<SamplF, GridH, Comp, LasL, CSys, Point, Sampl>,
    ) -> Self {
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
            hasher,
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
                hasher,
            }),
        }
    }

    pub fn coordinate_system(&self) -> &CSys {
        &self.inner.coordinate_system
    }

    pub fn sampling_factory(&self) -> &SamplF {
        &self.inner.sampling_factory
    }

    pub fn flush(&self) -> Result<(), IoError> {
        self.inner.page_manager.flush()
    }
}

impl<GridH, SamplF, Point, Pos, Comp, LasL, CSys, Sampl, Raw> Index<Point, CSys>
    for SensorPosIndex<GridH, SamplF, Comp, LasL, CSys, Point, Sampl>
where
    GridH: GridHierarchy<Position = Pos, Component = Comp> + Clone + Send + Sync + 'static,
    SamplF: SamplingFactory<Point = Point, Sampling = Sampl> + Send + Sync + 'static,
    Point: PointType<Position = Pos>
        + WithAttr<SensorPositionAttribute<Pos>>
        + Clone
        + Send
        + Sync
        + 'static,
    Pos: Position<Component = Comp> + Clone + Sync,
    Comp: Component + Serialize + DeserializeOwned + Send + Sync,
    LasL: LasReadWrite<Point, CSys> + Clone + Send + Sync + 'static,
    CSys: Clone + PartialEq + Send + Sync + 'static,
    Sampl: Sampling<Point = Point, Raw = Raw> + Send + Clone + 'static,
    Raw: RawSamplingEntry<Point = Point> + Send + 'static,
{
    type Writer = SensorPosWriter<Point, CSys>;
    type Reader = SensorPosReader<GridH, SamplF, Comp, LasL, CSys, Pos, Point, Sampl>;

    fn writer(&self) -> Self::Writer {
        let index_inner = Arc::clone(&self.inner);
        SensorPosWriter::new(index_inner)
    }

    fn reader<Q>(&self, query: Q) -> Self::Reader
    where
        Q: Query<Pos, CSys> + 'static + Send + Sync,
    {
        SensorPosReader::new(query, Arc::clone(&self.inner))
    }

    fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        SensorPosIndex::flush(self).map_err(|e| Box::new(e) as _)
    }
}
