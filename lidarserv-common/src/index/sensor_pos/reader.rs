use crate::geometry::points::PointType;
use crate::index::sensor_pos::Inner;
use crate::index::Reader;
use crate::nalgebra::Scalar;
use crate::query::Query;
use std::sync::Arc;

pub struct SensorPosReader<GridH, SamplF, Comp: Scalar, LasL, CSys, Point: PointType> {
    query: Box<dyn Query<Point::Position>>,
    inner: Arc<Inner<GridH, SamplF, Comp, LasL, CSys, Point>>,
}

impl<GridH, SamplF, Comp, LasL, CSys, Point> Reader<Point>
    for SensorPosReader<GridH, SamplF, Comp, LasL, CSys, Point>
where
    Point: PointType,
    Comp: Scalar,
{
}

impl<GridH, SamplF, Comp, LasL, CSys, Point> SensorPosReader<GridH, SamplF, Comp, LasL, CSys, Point>
where
    Point: PointType,
    Comp: Scalar,
{
    pub(super) fn new<Q>(
        query: Q,
        inner: Arc<Inner<GridH, SamplF, Comp, LasL, CSys, Point>>,
    ) -> Self
    where
        Q: Query<Point::Position> + 'static,
    {
        SensorPosReader {
            query: Box::new(query),
            inner,
        }
    }
}
