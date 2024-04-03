use crate::geometry::bounding_box::AABB;
use crate::geometry::grid::LodLevel;
use crate::geometry::position::{I32CoordinateSystem, I32Position};
use crate::query::SpatialQuery;

/// A trivial query type, that will always have an empty query result.
#[derive(Debug, Clone, Default)]
pub struct EmptyQuery;

impl EmptyQuery {
    pub fn new() -> Self {
        EmptyQuery
    }
}

impl SpatialQuery for EmptyQuery {
    fn max_lod_position(
        &self,
        _position: &I32Position,
        _coordinate_system: &I32CoordinateSystem,
    ) -> Option<LodLevel> {
        None
    }

    fn max_lod_area(
        &self,
        _bounds: &AABB<i32>,
        _coordinate_system: &I32CoordinateSystem,
    ) -> Option<LodLevel> {
        None
    }
}
