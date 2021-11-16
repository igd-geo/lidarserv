pub mod grid_cell_directory;
pub mod page_manager;
pub mod reader;
pub mod writer;

use crate::geometry::grid::{GridHierarchy, LeveledGridCell, LodLevel};
use crate::geometry::points::PointType;
use crate::geometry::position::{Component, CoordinateSystem, Position};
use crate::geometry::sampling::{Sampling, SamplingFactory};
use crate::index::octree::grid_cell_directory::GridCellDirectory;
use crate::index::octree::page_manager::{LasPageManager, OctreePageLoader, Page};
use crate::index::octree::reader::OctreeReader;
use crate::index::octree::writer::{OctreeWriter, TaskPriorityFunction};
use crate::index::Index;
use crate::las::LasReadWrite;
use crate::nalgebra::Scalar;
use crate::query::Query;
use std::sync::{Arc, Mutex};
use thiserror::Error;

struct Inner<Point, GridH, LasL, Sampl, Comp: Scalar, CSys, SamplF> {
    num_threads: u16,
    priority_function: TaskPriorityFunction,
    max_lod: LodLevel,
    max_bogus_inner: usize,
    max_bogus_leaf: usize,
    node_hierarchy: GridH,
    subscriptions: Mutex<Vec<crossbeam_channel::Sender<LeveledGridCell>>>,
    page_cache: LasPageManager<LasL, Sampl, Point, Comp, CSys>,
    sample_factory: SamplF,
    loader: LasL,
    coordinate_system: CSys,
}

pub struct Octree<Point, GridH, LasL, Sampl, Comp: Scalar, CSys, SamplF> {
    inner: Arc<Inner<Point, GridH, LasL, Sampl, Comp, CSys, SamplF>>,
}

#[derive(Error, Debug)]
#[error("Error while flushing to disk: {0}")]
pub struct FlushError(String);

impl<Point, GridH, LasL, Sampl, Comp: Scalar, CSys, SamplF, Pos>
    Octree<Point, GridH, LasL, Sampl, Comp, CSys, SamplF>
where
    LasL: LasReadWrite<Point, CSys> + Clone,
    CSys: CoordinateSystem<Position = Pos> + Clone + PartialEq,
    Point: PointType<Position = Pos> + Clone,
    Sampl: Sampling<Point = Point>,
    Comp: Component,
    Pos: Position<Component = Comp>,
{
    pub fn new(
        num_threads: u16,
        priority_function: TaskPriorityFunction,
        max_lod: LodLevel,
        max_bogus_inner: usize,
        max_bogus_leaf: usize,
        node_hierarchy: GridH,
        page_loader: OctreePageLoader<LasL, Page<Sampl, Point, Comp, CSys>>,
        page_directory: GridCellDirectory,
        max_cache_size: usize,
        sample_factory: SamplF,
        loader: LasL,
        coordinate_system: CSys,
    ) -> Self {
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
            }),
        }
    }

    pub fn coordinate_system(&self) -> &CSys {
        &self.inner.coordinate_system
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

impl<Point, GridH, LasL, Sampl, Comp, CSys, SamplF, Pos> Index<Point, CSys>
    for Octree<Point, GridH, LasL, Sampl, Comp, CSys, SamplF>
where
    Point: PointType<Position = Pos> + Clone + Send + Sync + 'static,
    Pos: Position<Component = Comp>,
    GridH: GridHierarchy<Component = Comp, Position = Pos> + Clone + Send + Sync + 'static,
    LasL: LasReadWrite<Point, CSys> + Clone + Send + Sync + 'static,
    Sampl: Sampling<Point = Point> + Clone + Send + Sync + 'static,
    Comp: Component + Send + Sync,
    CSys: CoordinateSystem<Position = Pos> + Clone + PartialEq + Send + Sync + 'static,
    SamplF:
        SamplingFactory<Param = LodLevel, Point = Point, Sampling = Sampl> + Send + Sync + 'static,
{
    type Writer = OctreeWriter<Point, GridH>;
    type Reader = OctreeReader<Point, GridH, LasL, Sampl, Comp, CSys, SamplF>;

    fn writer(&self) -> Self::Writer {
        OctreeWriter::new(Arc::clone(&self.inner))
    }

    fn reader<Q>(&self, _query: Q) -> Self::Reader
    where
        Q: Query<Pos, CSys> + 'static,
    {
        OctreeReader {
            inner: Arc::clone(&self.inner),
        }
    }
}
