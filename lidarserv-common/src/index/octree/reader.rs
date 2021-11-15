use crate::geometry::points::PointType;
use crate::index::octree::Inner;
use crate::index::sensor_pos::writer::IndexError;
use crate::index::{Node, Reader};
use crate::nalgebra::Scalar;
use crate::query::Query;
use crossbeam_channel::Receiver;
use std::sync::Arc;

pub struct OctreeReader<Point, GridH, LasL, Sampl, Comp: Scalar, CSys, SamplF> {
    pub(super) inner: Arc<Inner<Point, GridH, LasL, Sampl, Comp, CSys, SamplF>>,
}

impl<Point: PointType, GridH, LasL, Sampl, Comp: Scalar, CSys, SamplF> Reader<Point>
    for OctreeReader<Point, GridH, LasL, Sampl, Comp, CSys, SamplF>
{
    type NodeId = ();
    type Node = ();

    fn set_query<Q: Query<Point::Position> + 'static + Send + Sync>(&mut self, query: Q) {
        todo!()
    }

    fn update(&mut self) {
        todo!()
    }

    fn blocking_update(
        &mut self,
        queries: &mut Receiver<Box<dyn Query<Point::Position> + Send + Sync>>,
    ) -> bool {
        todo!()
    }

    fn load_one(&mut self) -> Option<(Self::NodeId, Self::Node)> {
        todo!()
    }

    fn remove_one(&mut self) -> Option<Self::NodeId> {
        todo!()
    }

    fn update_one(&mut self) -> Option<(Self::NodeId, Vec<(Self::NodeId, Self::Node)>)> {
        todo!()
    }
}

impl Node for () {
    fn las_files(&self) -> Vec<&[u8]> {
        todo!()
    }
}
