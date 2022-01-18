use crate::geometry::grid::LodLevel;
use crate::geometry::points::PointType;
use crate::geometry::position::I32Position;
use crate::query::Query;
use std::error::Error;
use std::sync::Arc;

pub mod octree;
pub mod sensor_pos;

pub trait Index<Point>
where
    Point: PointType<Position = I32Position>,
{
    type Writer: Writer<Point>;
    type Reader: Reader<Point>;

    /// Return a point writer, that can insert points into this index.
    fn writer(&self) -> Self::Writer;
    fn reader<Q>(&self, query: Q) -> Self::Reader
    where
        Q: Query + 'static + Send + Sync;
    fn flush(&mut self) -> Result<(), Box<dyn Error>>;
}

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

pub trait Reader<Point>
where
    Point: PointType,
{
    type NodeId: NodeId;
    type Node: Node;

    fn set_query<Q: Query + 'static + Send + Sync>(&mut self, query: Q);

    fn update(&mut self);

    fn blocking_update(
        &mut self,
        queries: &mut crossbeam_channel::Receiver<Box<dyn Query + Send + Sync>>,
    ) -> bool;

    fn load_one(&mut self) -> Option<(Self::NodeId, Self::Node)>;

    fn remove_one(&mut self) -> Option<Self::NodeId>;

    fn update_one(&mut self) -> Option<Update<Self::NodeId, Self::Node>>;
}

pub type Update<NodeId, NodeData> = (NodeId, Vec<(NodeId, NodeData)>);

pub trait Node {
    fn las_files(&self) -> Vec<Arc<Vec<u8>>>;
}

pub trait NodeId {
    fn lod(&self) -> LodLevel;
}
