use crate::geometry::grid::LodLevel;
use std::fmt::Debug;
use tracy_client::span;

pub mod bounding_box;
pub mod empty;
pub mod plane;
pub mod view_frustum;

/// Implemented for EmptyQuery, BoundingBoxQuery, and ViewFrustumQuery.
pub trait SpatialQuery: Debug {
    /// Returns either the maximum LOD level at the given position
    /// or None if the query does not match the position.
    fn max_lod_position(
        &self,
        position: &I32Position,
        coordinate_system: &I32CoordinateSystem,
    ) -> Option<LodLevel>;

    /// Returns either the maximum LOD level at the given area
    /// or None if the query does not match the area.
    fn max_lod_area(
        &self,
        bounds: &AABB<i32>,
        coordinate_system: &I32CoordinateSystem,
    ) -> Option<LodLevel>;
}

/// Extension trait for Query trait objects for some convenience methods.
pub trait SpatialQueryExt {
    /// Returns true if the query matches the given area
    fn matches_node(
        &self,
        bounds: &AABB<i32>,
        coordinate_system: &I32CoordinateSystem,
        lod: &LodLevel,
    ) -> bool;

    /// Returns true if the query matches the given position
    fn matches_point<Point>(&self, point: &Point, coordinate_system: &I32CoordinateSystem) -> bool
    where
        Point: PointType<Position = I32Position>;
}

/// Implementation for all types that implement Query.
impl<Q> SpatialQueryExt for Q
where
    Q: SpatialQuery + ?Sized,
{
    fn matches_node(
        &self,
        bounds: &AABB<i32>,
        coordinate_system: &I32CoordinateSystem,
        lod: &LodLevel,
    ) -> bool {
        span!("QueryExt::matches_node");
        match self.max_lod_area(bounds, coordinate_system) {
            None => false,
            Some(max_lod) => max_lod >= *lod,
        }
    }

    fn matches_point<Point>(&self, point: &Point, coordinate_system: &I32CoordinateSystem) -> bool
    where
        Point: PointType<Position = I32Position>,
    {
        self.max_lod_position(point.position(), coordinate_system)
            .is_some()
    }
}
