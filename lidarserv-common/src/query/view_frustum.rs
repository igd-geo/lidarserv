use crate::geometry::bounding_box::{BaseAABB, AABB};
use crate::geometry::grid::LodLevel;
use crate::geometry::points::PointType;
use crate::geometry::position::{
    CoordinateSystem, F64Position, I32CoordinateSystem, I32Position, Position,
};
use crate::geometry::sampling::{Sampling, SamplingFactory};
use crate::query::Query;
use nalgebra::{Matrix4, Point3, Vector3, Vector4};

#[derive(Debug)]
pub struct ViewFrustumQuery {
    view_projection_matrix: Matrix4<f64>,
    view_projection_matrix_inv: Matrix4<f64>,
    clip_min_point_dist: f64,
    lod0_point_distance: f64,
}

impl ViewFrustumQuery {
    pub const fn new_raw(
        view_projection_matrix: Matrix4<f64>,
        view_projection_matrix_inv: Matrix4<f64>,
        clip_min_point_dist: f64,
        lod0_point_distance: f64,
    ) -> Self {
        ViewFrustumQuery {
            view_projection_matrix,
            view_projection_matrix_inv,
            clip_min_point_dist,
            lod0_point_distance,
        }
    }

    pub fn new<SamplF, Point, CSys, Pos>(
        view_projection_matrix: Matrix4<f64>,
        view_projection_matrix_inv: Matrix4<f64>,
        window_width_pixels: f64,
        min_distance_pixels: f64,
        sampling_factory: &SamplF,
        coordinate_system: &CSys,
    ) -> Self
    where
        SamplF: SamplingFactory<Point = Point>,
        Point: PointType<Position = Pos>,
        CSys: CoordinateSystem<Position = Pos>,
        Pos: Position,
    {
        let clip_min_point_dist = min_distance_pixels / window_width_pixels * 2.0;

        let lod0_point_distance = sampling_factory.build(&LodLevel::base()).point_distance();
        let lod0_point_distance = coordinate_system.decode_distance(lod0_point_distance);

        ViewFrustumQuery {
            view_projection_matrix,
            view_projection_matrix_inv,
            clip_min_point_dist,
            lod0_point_distance,
        }
    }
}

impl Query for ViewFrustumQuery {
    fn max_lod_position(
        &self,
        position: &I32Position,
        coordinate_system: &I32CoordinateSystem,
    ) -> Option<LodLevel> {
        // intersection test with view frustum
        let position = position.decode(coordinate_system);
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
        let lod_level = (self.lod0_point_distance / min_point_dist).log2().ceil() as u16;
        Some(LodLevel::from_level(lod_level))
    }

    fn max_lod_area(
        &self,
        bounds: &AABB<i32>,
        coordinate_system: &I32CoordinateSystem,
    ) -> Option<LodLevel> {
        // convert aabb to global coordinates
        let min = coordinate_system.decode_position(&bounds.min());
        let max = coordinate_system.decode_position(&bounds.max());
        let bounds = AABB::new(min, max);

        // get vertices and planes, that make up both the view frustum and the aabb
        let frustum_vertices = CubeVertices::from_aabb(&AABB::new(
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
        let aabb_vertices = CubeVertices::from_aabb(&bounds);
        let aabb_planes = aabb_vertices.planes();

        // intersection test between aabb and view frustum
        // Both view frustum and aabb are convex, so we can use the separating axis theorem
        if aabb_planes.iter().any(|aabb_plane| {
            frustum_vertices
                .points()
                .iter()
                .all(|frustum_vert| aabb_plane.is_on_negative_side(frustum_vert))
        }) {
            return None;
        }
        if frustum_planes.iter().any(|frustum_plane| {
            aabb_vertices
                .points()
                .iter()
                .all(|aabb_vert| frustum_plane.is_on_negative_side(aabb_vert))
        }) {
            return None;
        }

        // for the max lod calculation: Use the point in the aabb, that is the closest to the camera.
        let near_clipping_plane = &frustum_planes[4];
        let (mut min_d_point, min_d) = aabb_vertices
            .points()
            .iter()
            .map(|p| (*p, near_clipping_plane.signed_distance(p)))
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
            min_d_point = near_clipping_plane.project_onto_plane(&min_d_point);
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
        let lod_level = (self.lod0_point_distance / min_point_dist).log2().ceil() as u16;
        Some(LodLevel::from_level(lod_level))
    }
}

struct CubeVertices([Point3<f64>; 8]);

impl CubeVertices {
    pub fn from_aabb(aabb: &AABB<f64>) -> Self {
        let min = aabb.min::<F64Position>();
        let max = aabb.max::<F64Position>();
        CubeVertices([
            Point3::new(min.x(), min.y(), min.z()),
            Point3::new(min.x(), min.y(), max.z()),
            Point3::new(min.x(), max.y(), min.z()),
            Point3::new(min.x(), max.y(), max.z()),
            Point3::new(max.x(), min.y(), min.z()),
            Point3::new(max.x(), min.y(), max.z()),
            Point3::new(max.x(), max.y(), min.z()),
            Point3::new(max.x(), max.y(), max.z()),
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
    pub fn x1y1z1(&self) -> &Point3<f64> {
        &self.0[0]
    }

    #[inline]
    pub fn x1y1z2(&self) -> &Point3<f64> {
        &self.0[1]
    }

    #[inline]
    pub fn x1y2z1(&self) -> &Point3<f64> {
        &self.0[2]
    }

    #[inline]
    pub fn x1y2z2(&self) -> &Point3<f64> {
        &self.0[3]
    }

    #[inline]
    pub fn x2y1z1(&self) -> &Point3<f64> {
        &self.0[4]
    }

    #[inline]
    pub fn x2y1z2(&self) -> &Point3<f64> {
        &self.0[5]
    }

    #[inline]
    pub fn x2y2z1(&self) -> &Point3<f64> {
        &self.0[6]
    }

    #[inline]
    pub fn x2y2z2(&self) -> &Point3<f64> {
        &self.0[7]
    }
}

struct Plane {
    normal: Vector3<f64>,
    b: f64,
}

impl Plane {
    pub fn from_triangle(p1: &Point3<f64>, p2: &Point3<f64>, p3: &Point3<f64>) -> Self {
        let normal = (p2 - p1).cross(&(p3 - p1)).normalize();
        let b = normal.dot(&p1.coords);
        Plane { normal, b }
    }

    pub fn signed_distance(&self, p: &Point3<f64>) -> f64 {
        self.normal.dot(&p.coords) - self.b
    }

    pub fn is_on_positive_side(&self, p: &Point3<f64>) -> bool {
        self.signed_distance(p) >= 0.0
    }

    pub fn is_on_negative_side(&self, p: &Point3<f64>) -> bool {
        !self.is_on_positive_side(p)
    }

    pub fn project_onto_plane(&self, p: &Point3<f64>) -> Point3<f64> {
        p - self.normal * self.signed_distance(p)
    }
}

#[cfg(test)]
mod tests {
    use crate::geometry::bounding_box::{BaseAABB, AABB};
    use crate::query::view_frustum::{CubeVertices, Plane};
    use nalgebra::{Point3, Vector3};

    #[test]
    fn test_plane_from_triangle() {
        let p = Plane::from_triangle(
            &Point3::new(1.0, 0.0, 0.5),
            &Point3::new(2.0, 0.0, 0.5),
            &Point3::new(1.0, 3.0, 0.5),
        );
        assert_eq!(Vector3::new(0.0, 0.0, 1.0), p.normal);
        assert_eq!(0.5, p.b);
        assert!(p.is_on_positive_side(&Point3::new(0.0, 0.0, 1.0)));
        assert!(p.is_on_negative_side(&Point3::new(0.0, 0.0, 0.0)));
    }

    #[test]
    fn test_cube_inside_out() {
        let c = CubeVertices::from_aabb(&AABB::new(
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 1.0, 1.0),
        ));
        let center = Point3::new(0.5, 0.5, 0.5);
        assert!(c.plane_x_min().is_on_positive_side(&center));
        assert!(c.plane_x_max().is_on_positive_side(&center));
        assert!(c.plane_y_min().is_on_positive_side(&center));
        assert!(c.plane_y_max().is_on_positive_side(&center));
        assert!(c.plane_z_min().is_on_positive_side(&center));
        assert!(c.plane_z_max().is_on_positive_side(&center));
    }

    #[test]
    fn test_project_point_on_plane() {
        let p = Plane::from_triangle(
            &Point3::new(1.0, 0.0, 0.5),
            &Point3::new(2.0, 0.0, 0.5),
            &Point3::new(1.0, 3.0, 0.5),
        );
        let on_plane = p.project_onto_plane(&Point3::new(1.0, 2.0, 3.0));
        assert_eq!(on_plane, Point3::new(1.0, 2.0, 0.5));
    }
}
