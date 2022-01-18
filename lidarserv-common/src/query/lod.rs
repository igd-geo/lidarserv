use crate::geometry::bounding_box::AABB;
use crate::geometry::grid::LodLevel;
use crate::geometry::position::{I32CoordinateSystem, I32Position};
use crate::query::Query;

/// A trivial query type, that will return all points up to a certain LOD.
#[derive(Debug, Clone)]
pub struct LodQuery {
    max_lod: LodLevel,
}

impl LodQuery {
    pub fn new(max_lod: LodLevel) -> Self {
        LodQuery { max_lod }
    }
}

impl Query for LodQuery {
    fn max_lod_position(
        &self,
        _position: &I32Position,
        _coordinate_system: &I32CoordinateSystem,
    ) -> Option<LodLevel> {
        Some(self.max_lod)
    }

    fn max_lod_area(
        &self,
        _bounds: &AABB<i32>,
        _coordinate_system: &I32CoordinateSystem,
    ) -> Option<LodLevel> {
        Some(self.max_lod)
    }
}
