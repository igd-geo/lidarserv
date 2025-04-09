//! # ToDo
//!
//!  - Unit tests!
use std::convert::Infallible;

use super::{ExecutableQuery, NodeQueryResult, Query};
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
use nalgebra::{Isometry3, Matrix4, Perspective3, Point3, Vector2, Vector3, Vector4, vector};
use pasture_core::containers::{BorrowedBufferExt, VectorBuffer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ViewFrustumQuery {
    /// position of the viewer
    pub camera_pos: Point3<f64>,

    /// looking direction of the viewer
    pub camera_dir: Vector3<f64>,

    /// up vector
    pub camera_up: Vector3<f64>,

    /// The field of view along the y axis. This is the angle between uppermost and
    /// lowermost faces of the frustum.
    /// see: https://nalgebra.org/docs/user_guide/projections#perspective-projection
    pub fov_y: f64,

    /// The distance between the viewer (the origin) and the closest face of the
    /// frustum parallel to the xy-plane. If used for a 3D rendering application,
    /// this is the closest clipping plane.
    /// see: https://nalgebra.org/docs/user_guide/projections#perspective-projection
    pub z_near: f64,

    /// The distance between the viewer (the origin) and the furthest face of the
    /// frustum parallel to the xy-plane. If used for a 3D rendering application,
    /// this is the furthest clipping plane.
    /// see: https://nalgebra.org/docs/user_guide/projections#perspective-projection
    pub z_far: f64,

    /// The window size (in pixels)
    pub window_size: Vector2<f64>,

    /// The distance between two points on the screen
    pub max_distance: f64,
}

pub struct ViewFrustomQueryExecutable {
    component_type: PositionComponentType,
    coordinate_system: CoordinateSystem,
    node_hierarchy: GridHierarchy,
    view_projection_matrix: Matrix4<f64>,
    view_projection_matrix_inv: Matrix4<f64>,
    clip_max_point_dist: f64,
    lod0_point_distance: f64,

    // ---
    frustum_vertices: CubeVertices,
    frustum_planes: [Plane; 6],
}

impl Query for ViewFrustumQuery {
    type Executable = ViewFrustomQueryExecutable;
    type Error = Infallible;

    fn prepare(self, ctx: &QueryContext) -> Result<Self::Executable, Self::Error> {
        let target = self.camera_pos + self.camera_dir;
        let aspect = self.window_size.x / self.window_size.y;
        let view_transform = Isometry3::look_at_rh(&self.camera_pos, &target, &self.camera_up);
        let proj_transform = Perspective3::new(aspect, self.fov_y, self.z_near, self.z_far);
        let view_projection_matrix = proj_transform.as_matrix() * view_transform.to_matrix();
        let view_projection_matrix_inv =
            view_transform.inverse().to_matrix() * proj_transform.inverse();

        let clip_max_point_dist = self.max_distance / self.window_size.x * 2.0;

        struct Wct<'a> {
            ctx: &'a QueryContext,
        }
        impl WithComponentTypeOnce for Wct<'_> {
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

        let frustum_vertices = CubeVertices::from_aabb(Aabb::new(
            Point3::new(-1.0, -1.0, -1.0),
            Point3::new(1.0, 1.0, 1.0),
        ))
        .map(|clip_v| view_projection_matrix_inv.transform_point(&clip_v));
        let frustum_planes = frustum_vertices.planes();

        Ok(ViewFrustomQueryExecutable {
            component_type: ctx.component_type,
            coordinate_system: ctx.coordinate_system,
            node_hierarchy: ctx.node_hierarchy,
            view_projection_matrix,
            view_projection_matrix_inv,
            clip_max_point_dist,
            lod0_point_distance,
            frustum_vertices,
            frustum_planes,
        })
    }
}

impl ViewFrustomQueryExecutable {
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
            self.clip_max_point_dist * clip_position_hom.w,
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

    fn max_lod_area<C: Component>(&self, bounds: Aabb<C>) -> (Option<LodLevel>, Option<LodLevel>) {
        // convert aabb to global coordinates
        let min = self.coordinate_system.decode_position(bounds.min);
        let max = self.coordinate_system.decode_position(bounds.max);
        let bounds = Aabb::new(min, max);

        // get vertices and planes of the aabb
        let aabb_vertices = CubeVertices::from_aabb(bounds);
        let aabb_planes = aabb_vertices.planes();

        // intersection test between aabb and view frustum
        // Both view frustum and aabb are convex, so we can use the separating axis theorem
        if self.frustum_planes.iter().any(|frustum_plane| {
            aabb_vertices
                .points()
                .iter()
                .all(|aabb_vert| frustum_plane.is_on_negative_side(*aabb_vert))
        }) {
            return (None, None);
        }
        if aabb_planes.iter().any(|aabb_plane| {
            self.frustum_vertices
                .points()
                .iter()
                .all(|frustum_vert| aabb_plane.is_on_negative_side(*frustum_vert))
        }) {
            return (None, None);
        }
        let aabb_fully_inside_frustum: bool = self.frustum_planes.iter().all(|frustum_plane| {
            aabb_vertices
                .points()
                .iter()
                .all(|aabb_vert| frustum_plane.is_on_positive_side(*aabb_vert))
        });

        // calculate the min lod for this node (based on the furthest point from the camera)
        let near_clipping_plane = &self.frustum_planes[4];
        let min_lod = if aabb_fully_inside_frustum {
            let point = aabb_vertices
                .points()
                .iter()
                .copied()
                .max_by(|a, b| {
                    let dist_a = near_clipping_plane.signed_distance(*a);
                    let dist_b = near_clipping_plane.signed_distance(*b);
                    dist_a.total_cmp(&dist_b)
                })
                .unwrap();
            let point_clip = self.view_projection_matrix.transform_point(&point);
            let offset_clip = point_clip + vector![self.clip_max_point_dist, 0.0, 0.0];
            let offset = self
                .view_projection_matrix_inv
                .transform_point(&offset_clip);
            let max_point_dist = (offset - point).norm();
            let lod = (self.lod0_point_distance / max_point_dist).log2().ceil() as u8;
            Some(LodLevel::from_level(lod))
        } else {
            None
        };

        // calculate the max lod for this node (based on the closest point to the camera)
        let max_lod = {
            let (mut point, min_d) = aabb_vertices
                .points()
                .iter()
                .map(|p| (*p, near_clipping_plane.signed_distance(*p)))
                .min_by(|(_, a), (_, b)| a.total_cmp(b))
                .unwrap();
            if min_d < 0.0 {
                point = near_clipping_plane.project_onto_plane(point);
            }
            let point_clip = self.view_projection_matrix.transform_point(&point);
            let offset_clip = point_clip + vector![self.clip_max_point_dist, 0.0, 0.0];
            let offset = self
                .view_projection_matrix_inv
                .transform_point(&offset_clip);
            let max_point_dist = (offset - point).norm();
            let lod = (self.lod0_point_distance / max_point_dist).log2().ceil() as u8;
            Some(LodLevel::from_level(lod))
        };

        (min_lod, max_lod)
    }
}

impl ExecutableQuery for ViewFrustomQueryExecutable {
    fn matches_node(&self, node: LeveledGridCell) -> NodeQueryResult {
        struct Wct<'a> {
            query: &'a ViewFrustomQueryExecutable,
            node: LeveledGridCell,
        }
        impl WithComponentTypeOnce for Wct<'_> {
            type Output = NodeQueryResult;

            fn run_once<C: Component>(self) -> Self::Output {
                let Self { query, node } = self;
                let node_aabb_local = query.node_hierarchy.get_leveled_cell_bounds::<C>(node);
                let (min_lod, max_lod) = query.max_lod_area(node_aabb_local);
                if let Some(min_lod) = min_lod {
                    if node.lod <= min_lod {
                        return NodeQueryResult::Positive;
                    }
                }
                if let Some(max_lod) = max_lod {
                    if node.lod > max_lod {
                        return NodeQueryResult::Negative;
                    }
                } else {
                    return NodeQueryResult::Negative;
                }
                NodeQueryResult::Partial
            }
        }
        Wct { query: self, node }.for_component_type_once(self.component_type)
    }

    fn matches_points(&self, lod: LodLevel, points: &VectorBuffer) -> Vec<bool> {
        struct Wct<'a> {
            query: &'a ViewFrustomQueryExecutable,
            lod: LodLevel,
            points: &'a VectorBuffer,
        }
        impl WithComponentTypeOnce for Wct<'_> {
            type Output = Vec<bool>;

            fn run_once<C: Component>(self) -> Self::Output {
                let Self { query, lod, points } = self;

                points
                    .view_attribute::<C::PasturePrimitive>(&C::position_attribute())
                    .into_iter()
                    .map(|p| C::pasture_to_position(p))
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
