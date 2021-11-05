use crate::geometry::points::PointType;
use crate::index::Reader;
use crate::query::Query;

pub struct SensorPosReader<Point: PointType> {
    query: Box<dyn Query<Point::Position>>,
}

impl<Point> Reader<Point> for SensorPosReader<Point> where Point: PointType {}

impl<Point> SensorPosReader<Point>
where
    Point: PointType,
{
    pub fn new<Q>(query: Q) -> Self
    where
        Q: Query<Point::Position> + 'static,
    {
        SensorPosReader {
            query: Box::new(query),
        }
    }
}
