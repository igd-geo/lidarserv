//! # ToDo
//!
//! This needs quite some work. It was simply copied over from the old version.
//!
//!  - Don't recalculate the view frustum every time. Do it once when building the query.
//!  - Look through what else can be put into the builder instead of redoing it every time.
//!  - In matches_node, distinguish between NodeQueryResult::Partial and NodeQueryResult::Positive. (Currently always returns Partial.)
//!  - Unit tests!
use nalgebra::{Matrix4, Point3, Vector4};
use pasture_core::containers::{BorrowedBuffer, VectorBuffer};
use serde::{Deserialize, Serialize};

use crate::{
    geometry::{
        bounding_box::Aabb,
        coordinate_system::CoordinateSystem,
        grid::{GridHierarchy, LeveledGridCell, LodLevel},
        plane::Plane,
        position::{Component, PositionComponentType, WithComponentTypeOnce},
    },
    query::QueryContext,
};

use super::{NodeQueryResult, Query, QueryBuilder};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ViewFrustumQuery {
    pub view_projection_matrix: Matrix4<f64>,
    pub view_projection_matrix_inv: Matrix4<f64>,
    pub window_width_pixels: f64,
    pub min_distance_pixels: f64,
}

struct ViewFrustomQueryPrepared {
    component_type: PositionComponentType,
    coordinate_system: CoordinateSystem,
    node_hierarchy: GridHierarchy,
    view_projection_matrix: Matrix4<f64>,
    view_projection_matrix_inv: Matrix4<f64>,
    clip_min_point_dist: f64,
    lod0_point_distance: f64,
}

impl QueryBuilder for ViewFrustumQuery {
    fn build(self, ctx: &super::QueryContext) -> impl Query {
        let clip_min_point_dist = self.min_distance_pixels / self.window_width_pixels * 2.0;

        struct Wct<'a> {
            ctx: &'a QueryContext,
        }
        impl<'a> WithComponentTypeOnce for Wct<'a> {
            type Output = f64;

            fn run_once<C: Component>(self) -> Self::Output {
                let local_point_distance = self
                    .ctx
                    .point_hierarchy
                    .level::<C>(LodLevel::base())
                    .cell_size();
                self.ctx
                    .coordinate_system
                    .decode_distance(local_point_distance)
            }
        }
        let lod0_point_distance = Wct { ctx }.for_component_type_once(ctx.component_type);

        ViewFrustomQueryPrepared {
            component_type: ctx.component_type,
            coordinate_system: ctx.coordinate_system,
            node_hierarchy: ctx.node_hierarchy,
            view_projection_matrix: self.view_projection_matrix,
            view_projection_matrix_inv: self.view_projection_matrix_inv,
            clip_min_point_dist,
            lod0_point_distance,
        }
    }
}

impl ViewFrustomQueryPrepared {
    fn max_lod_position<C: Component>(&self, position: Point3<C>) -> Option<LodLevel> {
        // intersection test with view frustum
        let position = self.coordinate_system.decode_position(position);
        let position_hom = Vector4::new(position.x, position.y, position.z, 1.0);
        let clip_position_hom = self.view_projection_matrix * position_hom;
        let clip_position = clip_position_hom.xyz() / clip_position_hom.w;
        if clip_position.x < -1.0
            || clip_position.x > 1.0
            || clip_position.y < -1.0
            || clip_position.y > 1.0
            || clip_position.z < -1.0
            || clip_position.z > 1.0
        {
            return None;
        }

        // calculate the lod for that point
        let clip_min_point_dist_hom = Vector4::new(
            self.clip_min_point_dist * clip_position_hom.w,
            0.0,
            0.0,
            0.0,
        );
        let min_point_dist: f64 = (self.view_projection_matrix_inv * clip_min_point_dist_hom)
            .xyz()
            .norm();
        let lod_level = (self.lod0_point_distance / min_point_dist).log2().ceil() as u8;
        Some(LodLevel::from_level(lod_level))
    }

    fn max_lod_area<C: Component>(&self, bounds: Aabb<C>) -> Option<LodLevel> {
        // convert aabb to global coordinates
        let min = self.coordinate_system.decode_position(bounds.min);
        let max = self.coordinate_system.decode_position(bounds.max);
        let bounds = Aabb::new(min, max);

        // get vertices and planes, that make up both the view frustum and the aabb
        let frustum_vertices = CubeVertices::from_aabb(Aabb::new(
            Point3::new(-1.0, -1.0, -1.0),
            Point3::new(1.0, 1.0, 1.0),
        ))
        .map(|clip_v| {
            let clip_v_hom = Vector4::new(clip_v.x, clip_v.y, clip_v.z, 1.0);
            let world_v_hom = self.view_projection_matrix_inv * clip_v_hom;
            let world_v = world_v_hom.xyz() / world_v_hom.w;
            world_v.into()
        });
        let frustum_planes = frustum_vertices.planes();
        let aabb_vertices = CubeVertices::from_aabb(bounds);
        let aabb_planes = aabb_vertices.planes();

        // intersection test between aabb and view frustum
        // Both view frustum and aabb are convex, so we can use the separating axis theorem
        if aabb_planes.iter().any(|aabb_plane| {
            frustum_vertices
                .points()
                .iter()
                .all(|frustum_vert| aabb_plane.is_on_negative_side(*frustum_vert))
        }) {
            return None;
        }
        if frustum_planes.iter().any(|frustum_plane| {
            aabb_vertices
                .points()
                .iter()
                .all(|aabb_vert| frustum_plane.is_on_negative_side(*aabb_vert))
        }) {
            return None;
        }

        // for the max lod calculation: Use the point in the aabb, that is the closest to the camera.
        let near_clipping_plane = &frustum_planes[4];
        let (mut min_d_point, min_d) = aabb_vertices
            .points()
            .iter()
            .map(|p| (*p, near_clipping_plane.signed_distance(*p)))
            .min_by(|(_, a), (_, b)| {
                // f64::total_cmp is still unstable... :(
                if a < b {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            })
            .unwrap();
        if min_d < 0.0 {
            min_d_point = near_clipping_plane.project_onto_plane(min_d_point);
        }

        // calculate the lod for that point
        let min_d_point_hom = Vector4::new(min_d_point.x, min_d_point.y, min_d_point.z, 1.0);
        let clip_min_d_point_hom = self.view_projection_matrix * min_d_point_hom;
        let clip_min_point_dist_hom = Vector4::new(
            self.clip_min_point_dist * clip_min_d_point_hom.w,
            0.0,
            0.0,
            0.0,
        );
        let min_point_dist = (self.view_projection_matrix_inv * clip_min_point_dist_hom)
            .xyz()
            .norm();
        let lod_level = (self.lod0_point_distance / min_point_dist).log2().ceil() as u8;
        Some(LodLevel::from_level(lod_level))
    }
}

impl Query for ViewFrustomQueryPrepared {
    fn matches_node(&self, node: LeveledGridCell) -> NodeQueryResult {
        struct Wct<'a> {
            query: &'a ViewFrustomQueryPrepared,
            node: LeveledGridCell,
        }
        impl<'a> WithComponentTypeOnce for Wct<'a> {
            type Output = NodeQueryResult;

            fn run_once<C: Component>(self) -> Self::Output {
                let Self { query, node } = self;
                let node_aabb_local = query.node_hierarchy.get_leveled_cell_bounds::<C>(node);
                let max_lod = query.max_lod_area(node_aabb_local);
                if let Some(max_lod) = max_lod {
                    // todo detect fully positive nodes, too.
                    if node.lod <= max_lod {
                        NodeQueryResult::Partial
                    } else {
                        NodeQueryResult::Negative
                    }
                } else {
                    NodeQueryResult::Negative
                }
            }
        }
        Wct { query: self, node }.for_component_type_once(self.component_type)
    }

    fn matches_points(&self, lod: LodLevel, points: &VectorBuffer) -> Vec<bool> {
        struct Wct<'a> {
            query: &'a ViewFrustomQueryPrepared,
            lod: LodLevel,
            points: &'a VectorBuffer,
        }
        impl<'a> WithComponentTypeOnce for Wct<'a> {
            type Output = Vec<bool>;

            fn run_once<C: Component>(self) -> Self::Output {
                let Self { query, lod, points } = self;

                points
                    .view_attribute::<C::PasturePrimitive>(&C::position_attribute())
                    .into_iter()
                    .map(|p| p.into())
                    .map(|point_local| {
                        let max_lod = query.max_lod_position(point_local);
                        max_lod.is_some_and(|max_lod| lod <= max_lod)
                    })
                    .collect()
            }
        }
        Wct {
            query: self,
            lod,
            points,
        }
        .for_component_type_once(self.component_type)
    }
}

struct CubeVertices([Point3<f64>; 8]);

impl CubeVertices {
    pub fn from_aabb(aabb: Aabb<f64>) -> Self {
        let min = aabb.min;
        let max = aabb.max;
        CubeVertices([
            Point3::new(min.x, min.y, min.z),
            Point3::new(min.x, min.y, max.z),
            Point3::new(min.x, max.y, min.z),
            Point3::new(min.x, max.y, max.z),
            Point3::new(max.x, min.y, min.z),
            Point3::new(max.x, min.y, max.z),
            Point3::new(max.x, max.y, min.z),
            Point3::new(max.x, max.y, max.z),
        ])
    }

    pub fn map<F: Fn(Point3<f64>) -> Point3<f64>>(self, f: F) -> Self {
        let mut new_vertices = CubeVertices(self.0.map(f));
        // reorder points, such that the planes are oriented correctly.
        if new_vertices
            .plane_x_min()
            .is_on_negative_side(new_vertices.x2y2z2())
        {
            new_vertices.0.swap(0, 4);
            new_vertices.0.swap(1, 5);
            new_vertices.0.swap(2, 6);
            new_vertices.0.swap(3, 7);
        }
        if new_vertices
            .plane_y_min()
            .is_on_negative_side(new_vertices.x2y2z2())
        {
            new_vertices.0.swap(0, 2);
            new_vertices.0.swap(1, 3);
            new_vertices.0.swap(4, 6);
            new_vertices.0.swap(5, 7);
        }
        if new_vertices
            .plane_z_min()
            .is_on_negative_side(new_vertices.x2y2z2())
        {
            new_vertices.0.swap(0, 1);
            new_vertices.0.swap(2, 3);
            new_vertices.0.swap(4, 5);
            new_vertices.0.swap(6, 7);
        }
        new_vertices
    }

    pub fn points(&self) -> &[Point3<f64>; 8] {
        &self.0
    }

    pub fn planes(&self) -> [Plane; 6] {
        [
            self.plane_x_min(),
            self.plane_x_max(),
            self.plane_y_min(),
            self.plane_y_max(),
            self.plane_z_min(),
            self.plane_z_max(),
        ]
    }

    pub fn plane_x_min(&self) -> Plane {
        Plane::from_triangle(self.x1y1z1(), self.x1y2z1(), self.x1y1z2())
    }

    pub fn plane_x_max(&self) -> Plane {
        Plane::from_triangle(self.x2y1z1(), self.x2y1z2(), self.x2y2z1())
    }

    pub fn plane_y_min(&self) -> Plane {
        Plane::from_triangle(self.x1y1z1(), self.x1y1z2(), self.x2y1z1())
    }

    pub fn plane_y_max(&self) -> Plane {
        Plane::from_triangle(self.x1y2z1(), self.x2y2z1(), self.x1y2z2())
    }

    pub fn plane_z_min(&self) -> Plane {
        Plane::from_triangle(self.x1y1z1(), self.x2y1z1(), self.x1y2z1())
    }

    pub fn plane_z_max(&self) -> Plane {
        Plane::from_triangle(self.x1y1z2(), self.x1y2z2(), self.x2y1z2())
    }

    #[inline]
    pub fn x1y1z1(&self) -> Point3<f64> {
        self.0[0]
    }

    #[inline]
    pub fn x1y1z2(&self) -> Point3<f64> {
        self.0[1]
    }

    #[inline]
    pub fn x1y2z1(&self) -> Point3<f64> {
        self.0[2]
    }

    #[inline]
    pub fn x1y2z2(&self) -> Point3<f64> {
        self.0[3]
    }

    #[inline]
    pub fn x2y1z1(&self) -> Point3<f64> {
        self.0[4]
    }

    #[inline]
    pub fn x2y1z2(&self) -> Point3<f64> {
        self.0[5]
    }

    #[inline]
    pub fn x2y2z1(&self) -> Point3<f64> {
        self.0[6]
    }

    #[inline]
    pub fn x2y2z2(&self) -> Point3<f64> {
        self.0[7]
    }
}

#[cfg(test)]
mod tests {
    use crate::{geometry::bounding_box::Aabb, query::view_frustum::CubeVertices};
    use nalgebra::Point3;

    #[test]
    fn test_cube_inside_out() {
        let c = CubeVertices::from_aabb(Aabb::new(
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 1.0, 1.0),
        ));
        let center = Point3::new(0.5, 0.5, 0.5);
        assert!(c.plane_x_min().is_on_positive_side(center));
        assert!(c.plane_x_max().is_on_positive_side(center));
        assert!(c.plane_y_min().is_on_positive_side(center));
        assert!(c.plane_y_max().is_on_positive_side(center));
        assert!(c.plane_z_min().is_on_positive_side(center));
        assert!(c.plane_z_max().is_on_positive_side(center));
    }
}
