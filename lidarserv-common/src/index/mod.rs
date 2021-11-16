use crate::geometry::points::PointType;
use crate::query::Query;

pub mod octree;
pub mod sensor_pos;

pub trait Index<Point, CSys>
where
    Point: PointType,
{
    type Writer: Writer<Point>;
    type Reader: Reader<Point, CSys>;

    /// Return a point writer, that can insert points into this index.
    fn writer(&self) -> Self::Writer;
    fn reader<Q>(&self, query: Q) -> Self::Reader
    where
        Q: Query<Point::Position, CSys> + 'static + Send + Sync;
}

pub trait Writer<Point>
where
    Point: PointType,
{
    /// Insert new points into the index.
    fn insert(&mut self, points: Vec<Point>);
}

pub trait Reader<Point, CSys>
where
    Point: PointType,
{
    type NodeId;
    type Node: Node;

    fn set_query<Q: Query<Point::Position, CSys> + 'static + Send + Sync>(&mut self, query: Q);

    fn update(&mut self);

    fn blocking_update(
        &mut self,
        queries: &mut crossbeam_channel::Receiver<
            Box<dyn Query<Point::Position, CSys> + Send + Sync>,
        >,
    ) -> bool;

    fn load_one(&mut self) -> Option<(Self::NodeId, Self::Node)>;

    fn remove_one(&mut self) -> Option<Self::NodeId>;

    fn update_one(&mut self) -> Option<Update<Self::NodeId, Self::Node>>;
}

pub type Update<NodeId, NodeData> = (NodeId, Vec<(NodeId, NodeData)>);

pub trait Node {
    fn las_files(&self) -> Vec<&[u8]>;
}
