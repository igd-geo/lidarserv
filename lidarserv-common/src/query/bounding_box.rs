use crate::geometry::bounding_box::{BaseAABB, AABB};
use crate::geometry::grid::LodLevel;
use crate::geometry::position::{Component, Position};
use crate::query::Query;
use nalgebra::Scalar;

#[derive(Debug, Clone)]
pub struct BoundingBoxQuery<Comp: Scalar> {
    bounds: AABB<Comp>,
    lod: LodLevel,
}

impl<Comp: Component> BoundingBoxQuery<Comp> {
    pub fn new(bounds: AABB<Comp>, lod: LodLevel) -> Self {
        BoundingBoxQuery { bounds, lod }
    }
}

impl<Comp, Pos, CSys> Query<Pos, CSys> for BoundingBoxQuery<Comp>
where
    Comp: Component,
    Pos: Position<Component = Comp>,
{
    fn max_lod_position(&self, position: &Pos, _coordinate_system: &CSys) -> Option<LodLevel> {
        if self.bounds.contains(position) {
            Some(self.lod)
        } else {
            None
        }
    }

    fn max_lod_area(&self, bounds: &AABB<Comp>, _coordinate_system: &CSys) -> Option<LodLevel> {
        if AABB::intersects(bounds, &self.bounds) {
            Some(self.lod)
        } else {
            None
        }
    }
}
