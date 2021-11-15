pub mod bounding_box;
pub mod empty;

use crate::geometry::bounding_box::AABB;
use crate::geometry::grid::LodLevel;
use crate::geometry::points::PointType;
use crate::geometry::position::Position;

pub trait Query<Pos>
where
    Pos: Position,
{
    fn max_lod_position(&self, position: &Pos) -> Option<LodLevel>;

    fn max_lod_area(&self, bounds: &AABB<Pos::Component>) -> Option<LodLevel>;
}

pub trait QueryExt<Pos>
where
    Pos: Position,
{
    fn matches_node(&self, bounds: &AABB<Pos::Component>, lod: &LodLevel) -> bool;
    fn matches_point<Point>(&self, point: &Point, lod: &LodLevel) -> bool
    where
        Point: PointType<Position = Pos>;
}

impl<Pos: Position, Q: Query<Pos> + ?Sized> QueryExt<Pos> for Q {
    fn matches_node(&self, bounds: &AABB<Pos::Component>, lod: &LodLevel) -> bool {
        match self.max_lod_area(bounds) {
            None => false,
            Some(max_lod) => max_lod >= *lod,
        }
    }
    fn matches_point<Point>(&self, point: &Point, lod: &LodLevel) -> bool
    where
        Point: PointType<Position = Pos>,
    {
        match self.max_lod_position(point.position()) {
            None => false,
            Some(max_lod) => max_lod >= *lod,
        }
    }
}
