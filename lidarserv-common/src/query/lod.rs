use crate::geometry::bounding_box::AABB;
use crate::geometry::grid::LodLevel;
use crate::geometry::position::{Component, Position};
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

impl<Pos, Comp, CSys> Query<Pos, CSys> for LodQuery
where
    Pos: Position<Component = Comp>,
    Comp: Component,
{
    fn max_lod_position(&self, _position: &Pos, _coordinate_system: &CSys) -> Option<LodLevel> {
        Some(self.max_lod)
    }

    fn max_lod_area(&self, _bounds: &AABB<Comp>, _coordinate_system: &CSys) -> Option<LodLevel> {
        Some(self.max_lod)
    }
}
