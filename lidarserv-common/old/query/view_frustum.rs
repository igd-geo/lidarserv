use crate::geometry::grid::LodLevel;
use crate::geometry::position::Position;
use crate::geometry::sampling::SamplingFactory;
use crate::query::SpatialQuery;
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

impl SpatialQuery for ViewFrustumQuery {
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
