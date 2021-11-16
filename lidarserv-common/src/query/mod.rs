pub mod bounding_box;
pub mod empty;
pub mod view_frustum;

use crate::geometry::bounding_box::AABB;
use crate::geometry::grid::LodLevel;
use crate::geometry::points::PointType;
use crate::geometry::position::{Component, CoordinateSystem, Position};

pub trait Query<Pos, CSys>
where
    Pos: Position,
{
    fn max_lod_position(&self, position: &Pos, coordinate_system: &CSys) -> Option<LodLevel>;

    fn max_lod_area(
        &self,
        bounds: &AABB<Pos::Component>,
        coordinate_system: &CSys,
    ) -> Option<LodLevel>;
}

pub trait QueryExt<Pos, CSys>
where
    Pos: Position,
{
    fn matches_node(
        &self,
        bounds: &AABB<Pos::Component>,
        coordinate_system: &CSys,
        lod: &LodLevel,
    ) -> bool;

    fn matches_point<Point>(&self, point: &Point, coordinate_system: &CSys, lod: &LodLevel) -> bool
    where
        Point: PointType<Position = Pos>;
}

impl<Pos, Q, CSys> QueryExt<Pos, CSys> for Q
where
    Pos: Position,
    Q: Query<Pos, CSys> + ?Sized,
{
    fn matches_node(
        &self,
        bounds: &AABB<Pos::Component>,
        coordinate_system: &CSys,
        lod: &LodLevel,
    ) -> bool {
        match self.max_lod_area(bounds, coordinate_system) {
            None => false,
            Some(max_lod) => max_lod >= *lod,
        }
    }

    fn matches_point<Point>(&self, point: &Point, coordinate_system: &CSys, lod: &LodLevel) -> bool
    where
        Point: PointType<Position = Pos>,
    {
        match self.max_lod_position(point.position(), coordinate_system) {
            None => false,
            Some(max_lod) => max_lod >= *lod,
        }
    }
}
