use crate::geometry::bounding_box::AABB;
use crate::geometry::grid::LodLevel;
use crate::geometry::position::{Component, Position};
use crate::query::Query;

/// A trivial query type, that will always have an empty query result.
#[derive(Debug, Clone, Default)]
pub struct EmptyQuery;

impl EmptyQuery {
    pub fn new() -> Self {
        EmptyQuery
    }
}

impl<Pos, Comp> Query<Pos> for EmptyQuery
where
    Pos: Position<Component = Comp>,
    Comp: Component,
{
    fn max_lod_position(&self, position: &Pos) -> Option<LodLevel> {
        None
    }

    fn max_lod_area(&self, bounds: &AABB<Comp>) -> Option<LodLevel> {
        None
    }
}
