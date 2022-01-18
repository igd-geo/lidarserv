pub mod bounding_box;
pub mod empty;
pub mod lod;
pub mod view_frustum;

use crate::geometry::bounding_box::AABB;
use crate::geometry::grid::LodLevel;
use crate::geometry::points::PointType;
use crate::geometry::position::{I32CoordinateSystem, I32Position};

pub trait Query {
    fn max_lod_position(
        &self,
        position: &I32Position,
        coordinate_system: &I32CoordinateSystem,
    ) -> Option<LodLevel>;

    fn max_lod_area(
        &self,
        bounds: &AABB<i32>,
        coordinate_system: &I32CoordinateSystem,
    ) -> Option<LodLevel>;
}

pub trait QueryExt {
    fn matches_node(
        &self,
        bounds: &AABB<i32>,
        coordinate_system: &I32CoordinateSystem,
        lod: &LodLevel,
    ) -> bool;

    fn matches_point<Point>(
        &self,
        point: &Point,
        coordinate_system: &I32CoordinateSystem,
        lod: &LodLevel,
    ) -> bool
    where
        Point: PointType<Position = I32Position>;
}

impl<Q> QueryExt for Q
where
    Q: Query + ?Sized,
{
    fn matches_node(
        &self,
        bounds: &AABB<i32>,
        coordinate_system: &I32CoordinateSystem,
        lod: &LodLevel,
    ) -> bool {
        match self.max_lod_area(bounds, coordinate_system) {
            None => false,
            Some(max_lod) => max_lod >= *lod,
        }
    }

    fn matches_point<Point>(
        &self,
        point: &Point,
        coordinate_system: &I32CoordinateSystem,
        lod: &LodLevel,
    ) -> bool
    where
        Point: PointType<Position = I32Position>,
    {
        match self.max_lod_position(point.position(), coordinate_system) {
            None => false,
            Some(max_lod) => max_lod >= *lod,
        }
    }
}
