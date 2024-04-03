use crate::geometry::grid::LodLevel;
use crate::geometry::points::PointType;
use crate::geometry::position::{I32CoordinateSystem, I32Position};
use crate::index::octree::attribute_bounds::LasPointAttributeBounds;
use crate::query::empty::EmptyQuery;
use crate::query::SpatialQuery;
use std::error::Error;

pub mod octree;

/// Abstracts over a point cloud index.
/// Only used for [Octree] at the moment.
pub trait Index<Point>
where
    Point: PointType<Position = I32Position>,
{
    type Writer: Writer<Point>;
    type Reader: Reader<Point>;

    /// Return a point writer, that can insert points into this index.
    fn writer(&self) -> Self::Writer;
    fn reader(&self, query: Query) -> Self::Reader;
    fn flush(&mut self) -> Result<(), Box<dyn Error>>;
    fn index_info(&self) -> serde_json::Value;
}

/// Abstracts over a point cloud index, that can be written to.
/// Only used for [OctreeWriter] at the moment.
pub trait Writer<Point>
where
    Point: PointType,
{
    // returns the number of points, that are currently being inserted, or are waiting
    // in a buffer/queue for being inserted.
    // i.e. all points, that have been submitted via [Self::insert], but are not visible to queries, yet.
    fn backlog_size(&self) -> usize;

    /// Insert new points into the index.
    fn insert(&mut self, points: Vec<Point>);
}

/// Abstracts over a point cloud index, that can be read from.
/// Only used for [OctreeReader] at the moment.
pub trait Reader<Point>
where
    Point: PointType,
{
    type NodeId: NodeId;

    fn try_update(&mut self, queries: &mut crossbeam_channel::Receiver<Query>) -> bool;

    fn blocking_update(&mut self, queries: &mut crossbeam_channel::Receiver<Query>) -> bool;

    fn load_one(&mut self) -> Option<(Self::NodeId, Vec<Point>, I32CoordinateSystem)>;

    fn remove_one(&mut self) -> Option<Self::NodeId>;

    fn update_one(&mut self) -> Option<Update<Self::NodeId, I32CoordinateSystem, Vec<Point>>>;
}

pub type Update<NodeId, CoordinateSystem, NodeData> =
    (NodeId, CoordinateSystem, Vec<(NodeId, NodeData)>);

/// Currently only implemented for [LeveledGridCell].
pub trait NodeId {
    fn lod(&self) -> LodLevel;
}

#[derive(Debug)]
pub struct Query {
    pub spatial: Box<dyn SpatialQuery + Send + Sync>,
    pub attributes: LasPointAttributeBounds,
    pub enable_attribute_acceleration: bool,
    pub enable_histogram_acceleration: bool,
    pub enable_point_filtering: bool,
}

impl Default for Query {
    fn default() -> Self {
        Self {
            spatial: Box::new(EmptyQuery),
            attributes: LasPointAttributeBounds::default(),
            enable_point_filtering: false,
            enable_attribute_acceleration: false,
            enable_histogram_acceleration: false,
        }
    }
}
