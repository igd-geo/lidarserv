use crate::geometry::bounding_box::{BaseAABB, AABB};
use crate::geometry::grid::LodLevel;
use crate::geometry::position::{I32CoordinateSystem, I32Position};
use crate::query::SpatialQuery;

#[derive(Debug, Clone)]
pub struct BoundingBoxQuery {
    bounds: AABB<i32>,
    lod: LodLevel,
}

impl BoundingBoxQuery {
    pub fn new(bounds: AABB<i32>, lod: LodLevel) -> Self {
        BoundingBoxQuery { bounds, lod }
    }
}

impl SpatialQuery for BoundingBoxQuery {
    fn max_lod_position(
        &self,
        position: &I32Position,
        _coordinate_system: &I32CoordinateSystem,
    ) -> Option<LodLevel> {
        if self.bounds.contains(position) {
            Some(self.lod)
        } else {
            None
        }
    }

    fn max_lod_area(
        &self,
        bounds: &AABB<i32>,
        _coordinate_system: &I32CoordinateSystem,
    ) -> Option<LodLevel> {
        if AABB::intersects(bounds, &self.bounds) {
            Some(self.lod)
        } else {
            None
        }
    }
}
