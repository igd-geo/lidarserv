use crate::geometry::points::PointType;
use crate::index::octree::Inner;
use crate::index::Reader;
use crate::nalgebra::Scalar;
use std::sync::Arc;

pub struct OctreeReader<Point, GridH, LasL, Sampl, Comp: Scalar, CSys, SamplF> {
    pub(super) inner: Arc<Inner<Point, GridH, LasL, Sampl, Comp, CSys, SamplF>>,
}

impl<Point: PointType, GridH, LasL, Sampl, Comp: Scalar, CSys, SamplF> Reader<Point>
    for OctreeReader<Point, GridH, LasL, Sampl, Comp, CSys, SamplF>
{
}
