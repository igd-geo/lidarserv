use crate::geometry::points::PointType;
use crate::index::octree::Inner;
use crate::index::{Node, Reader, Update};
use crate::nalgebra::Scalar;
use crate::query::Query;
use crossbeam_channel::Receiver;
use std::sync::Arc;

pub struct OctreeReader<Point, GridH, LasL, Sampl, Comp: Scalar, CSys, SamplF> {
    #[allow(dead_code)] // todo: will not be dead code any more, once OctreeReader is implemented
    pub(super) inner: Arc<Inner<Point, GridH, LasL, Sampl, Comp, CSys, SamplF>>,
}

impl<Point: PointType, GridH, LasL, Sampl, Comp: Scalar, CSys, SamplF> Reader<Point, CSys>
    for OctreeReader<Point, GridH, LasL, Sampl, Comp, CSys, SamplF>
{
    type NodeId = ();
    type Node = ();

    fn set_query<Q: Query<Point::Position, CSys> + 'static + Send + Sync>(&mut self, _query: Q) {
        todo!()
    }

    fn update(&mut self) {
        todo!()
    }

    fn blocking_update(
        &mut self,
        _queries: &mut Receiver<Box<dyn Query<Point::Position, CSys> + Send + Sync>>,
    ) -> bool {
        todo!()
    }

    fn load_one(&mut self) -> Option<(Self::NodeId, Self::Node)> {
        todo!()
    }

    fn remove_one(&mut self) -> Option<Self::NodeId> {
        todo!()
    }

    fn update_one(&mut self) -> Option<Update<Self::NodeId, Self::Node>> {
        todo!()
    }
}

// todo delete
impl Node for () {
    fn las_files(&self) -> Vec<&[u8]> {
        todo!()
    }
}
