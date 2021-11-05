use crate::geometry::points::PointType;
use crate::query::Query;

pub mod octree;
pub mod sensor_pos;

pub trait Index<Point>
where
    Point: PointType,
{
    type Writer: Writer<Point>;
    type Reader: Reader<Point>;

    /// Return a point writer, that can insert points into this index.
    fn writer(&self) -> Self::Writer;
    fn reader<Q>(&self, query: Q) -> Self::Reader
    where
        Q: Query<Point::Position> + 'static;
}

pub trait Writer<Point>
where
    Point: PointType,
{
    /// Insert new points into the index.
    fn insert(&mut self, points: Vec<Point>);
}

pub trait Reader<Point>
where
    Point: PointType,
{
}
