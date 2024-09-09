//! A map-like navigation.

use std::f64::consts::{FRAC_PI_2, PI};

use crate::navigation::event::{MouseButton, MouseDragSettings};
use crate::navigation::{Matrices, Navigation, ViewDirection};
use pasture_core::math::AABB;
use pasture_core::nalgebra::{Matrix4, Point, Rotation3, Unit, Vector2, Vector3, Vector4};

/// A navigation, that behaves similar to services like e.g. Google Maps.
/// The user can drag the earths surface with the left mouse button and
/// adjust the viewing angle with the right mouse button.
pub struct MapNavigation {
    /// size of the window in (scaled) pixels
    window_size: Vector2<f64>,

    /// position on the xy plane in world space, that the camera is looking at.
    focus: Vector2<f64>,

    /// How close the camera is to the [Self::focus] point.
    log_camera_distance: f64,

    /// Rotation of the camera.
    ///  First component: how much the camera "looks up" or "looks down".
    ///  Second component: how much the camera "looks left" or "looks right"
    camera_rotation: Vector2<f64>,

    view_matrix: Matrix4<f64>,
    projection_matrix: Matrix4<f64>,
    view_matrix_inv: Matrix4<f64>,
    projection_matrix_inv: Matrix4<f64>,
}

impl MapNavigation {
    fn min_render_distance(&self) -> f64 {
        2.0_f64.powf(self.log_camera_distance) * 0.01
    }

    fn max_render_distance(&self) -> f64 {
        2.0_f64.powf(self.log_camera_distance) * 10000.0
    }

    pub fn new() -> Self {
        let mut nav = MapNavigation {
            window_size: Vector2::new(1.0, 1.0),
            focus: Vector2::new(0.0, 0.0),
            log_camera_distance: 1.0,
            camera_rotation: Vector2::new(std::f64::consts::PI / 4.0, std::f64::consts::PI / 2.0),
            view_matrix: Matrix4::identity(),
            projection_matrix: Matrix4::identity(),
            view_matrix_inv: Matrix4::identity(),
            projection_matrix_inv: Matrix4::identity(),
        };
        nav.update();
        nav
    }

    fn project_window_coord_to_xy_plane(&self, point_window: Vector2<f64>) -> Vector2<f64> {
        // window coordinates to clip coordinates
        // (window has origin at top left)
        let clip_x = point_window.x / self.window_size.x * 2.0 - 1.0;
        let clip_y = -point_window.y / self.window_size.y * 2.0 + 1.0;
        let point_clip = Vector4::new(clip_x, clip_y, 0.0, 1.0);

        // undo projection, to get point in view space
        let point_view = self.projection_matrix_inv * point_clip;

        // convert to a direction
        // (setting w to 0 means, that it will not be influenced by the translation component
        // of any transformation, which is exactly what we would expect from a directional vector
        // that is not rooted anywhere in space.)
        let dir_view = Vector4::new(point_view.x, point_view.y, point_view.z, 0.0);

        // transform back to world space
        let dir = self.view_matrix_inv * dir_view;
        let dir = Vector3::new(dir.x, dir.y, dir.z);

        // camera position in view space
        let cam_view = Vector4::new(0.0, 0.0, 0.0, 1.0);

        // transform back to world space
        let cam = self.view_matrix_inv * cam_view;
        let cam = Vector3::new(cam.x / cam.w, cam.y / cam.w, cam.z / cam.w);

        // The camera position, together with the direction that we calculated
        // from the mouse position, defines a line:
        //     cam + n * dir       for any n in R
        //
        // Find the intersection between the xy-plane and this line.
        let n = cam.z / dir.z;
        let x = cam.x - dir.x * n;
        let y = cam.y - dir.y * n;

        Vector2::new(x, y)
    }
}

impl Navigation for MapNavigation {
    fn on_window_resized(&mut self, w: f64, h: f64) {
        self.window_size.x = w;
        self.window_size.y = h;
    }

    fn on_drag(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, drag: MouseDragSettings) {
        match &drag.button {
            MouseButton::Left => {
                let point1_window = Vector2::new(x1, y1);
                let point2_window = Vector2::new(x2, y2);
                let point1_xy = self.project_window_coord_to_xy_plane(point1_window);
                let point2_xy = self.project_window_coord_to_xy_plane(point2_window);
                self.focus = self.focus + point1_xy - point2_xy;
            }
            MouseButton::Middle | MouseButton::Right => {
                if !drag.alt_pressed {
                    let new_rot_x = self.camera_rotation.x + (y2 - y1) * 0.01;
                    const ONE_DEGREE: f64 = 1.0 / 180.0 * PI;
                    self.camera_rotation.x = new_rot_x.clamp(0.0, FRAC_PI_2 - ONE_DEGREE);
                }
                if !drag.shift_pressed {
                    self.camera_rotation.y += (x1 - x2) * 0.01;
                }
            }
            MouseButton::Other => {}
        }
    }

    fn on_scroll(&mut self, d: f64) {
        self.log_camera_distance -= d * 0.005;
    }

    fn update(&mut self) -> Matrices {
        let look_at = Vector3::new(self.focus.x, self.focus.y, 0.0);

        let rotation = Rotation3::from_axis_angle(
            &Unit::new_unchecked(Vector3::new(0.0, 0.0, 1.0)),
            self.camera_rotation[1],
        ) * Rotation3::from_axis_angle(
            &Unit::new_unchecked(Vector3::new(0.0, 1.0, 0.0)),
            self.camera_rotation[0],
        );
        let camera_direction = rotation * Vector3::new(1.0, 0.0, 0.0);
        let up_direction = rotation * Vector3::new(0.0, 0.0, 1.0);
        let camera_distance = 2.0_f64.powf(self.log_camera_distance);
        let camera_position = look_at - camera_direction * camera_distance;

        self.view_matrix = Matrix4::look_at_rh(
            &Point::from(camera_position),
            &Point::from(look_at),
            &up_direction,
        );
        self.view_matrix_inv = self.view_matrix.try_inverse().unwrap(); // view matrix is always invertible, so safe to unwrap.

        self.projection_matrix = Matrix4::new_perspective(
            self.window_size.x / self.window_size.y,
            std::f64::consts::PI / 4.0,
            self.min_render_distance(),
            self.max_render_distance(),
        );
        self.projection_matrix_inv = self.projection_matrix.try_inverse().unwrap(); // same as for the view matrix

        Matrices {
            view_matrix: self.view_matrix,
            projection_matrix: self.projection_matrix,
            view_matrix_inv: self.view_matrix_inv,
            projection_matrix_inv: self.projection_matrix_inv,
            window_size: self.window_size,
        }
    }

    fn focus_on(&mut self, aabb: AABB<f64>) {
        // focus at the point under the center
        let center = aabb.center();
        self.focus = center.xy().coords;

        // position the camera close to the center
        let initial_distance = 0.05_f64; // 5cm
        self.log_camera_distance = initial_distance.log2();
        let matrices = self.update();

        // project the vertices of the bounding box into camera space
        let points = [
            Vector3::new(aabb.min().x, aabb.min().y, aabb.min().z),
            Vector3::new(aabb.min().x, aabb.min().y, aabb.max().z),
            Vector3::new(aabb.min().x, aabb.max().y, aabb.min().z),
            Vector3::new(aabb.min().x, aabb.max().y, aabb.max().z),
            Vector3::new(aabb.max().x, aabb.min().y, aabb.min().z),
            Vector3::new(aabb.max().x, aabb.min().y, aabb.max().z),
            Vector3::new(aabb.max().x, aabb.max().y, aabb.min().z),
            Vector3::new(aabb.max().x, aabb.max().y, aabb.max().z),
        ]
        .iter()
        .map(|p| Vector4::new(p.x, p.y, p.z, 1.0))
        .map(|p| matrices.view_matrix * p)
        .collect::<Vec<_>>();

        // move the camera back, until all vertices of the aabb are inside the view frustum
        let mut move_back_by = 0.0;
        let direction_move_back = matrices.projection_matrix * Vector4::new(0.0, 0.0, -1.0, 0.0); // direction, that points in clip space will move in, when the camera is moved back (by one unit).
        for point in points {
            // transform point from camera- to clip space
            let point_clip = matrices.projection_matrix * point;

            // calculate, how far we need to move the camera back,
            // so that point_clip is within clip space.
            // the position in clip space, after moving the camera back by n units will be
            // point_clip + n * direction_move_back

            // near plane
            // solve: perspective_division(point_clip + a * direction_move_back).z == -1
            //  =>  (point_clip   + a * direction_move_back  ).z / (point_clip   + a * direction_move_back  ).w == -1
            //  =>  (point_clip.z + a * direction_move_back.z)   / (point_clip.w + a * direction_move_back.w)   == -1
            //  =>  (point_clip.z + a * direction_move_back.z) == -1 * (point_clip.w + a * direction_move_back.w)
            //  =>  point_clip.z + a * direction_move_back.z == -point_clip.w - a * direction_move_back.w
            //  =>  a * direction_move_back.z + a * direction_move_back.w == - point_clip.z - point_clip.w
            //  =>  a * (direction_move_back.z + direction_move_back.w) == - point_clip.z - point_clip.w
            //  =>  a == (- point_clip.z - point_clip.w) / (direction_move_back.z + direction_move_back.w)
            //  =>  a == -(point_clip.z + point_clip.w) / (direction_move_back.z + direction_move_back.w)
            {
                let a = -(point_clip.z + point_clip.w)
                    / (direction_move_back.z + direction_move_back.w);
                if a > move_back_by {
                    move_back_by = a
                }
            }

            // right plane
            // equivalent to near plane
            {
                let a = -(point_clip.x + point_clip.w)
                    / (direction_move_back.x + direction_move_back.w);
                if a > move_back_by {
                    move_back_by = a
                }
            }

            // bottom plane
            // equivalent to near plane
            {
                let a = -(point_clip.y + point_clip.w)
                    / (direction_move_back.y + direction_move_back.w);
                if a > move_back_by {
                    move_back_by = a
                }
            }

            // left plane
            // solve: perspective_division(point_clip + a * direction_move_back).x == 1
            //  =>  (point_clip   + a * direction_move_back  ).x / (point_clip   + a * direction_move_back  ).w == 1
            //  =>  (point_clip.x + a * direction_move_back.x)   / (point_clip.w + a * direction_move_back.w)   == 1
            //  =>  point_clip.x + a * direction_move_back.x == point_clip.w + a * direction_move_back.w
            //  =>  a * direction_move_back.x - a * direction_move_back.w == point_clip.w - point_clip.x
            //  =>  a * (direction_move_back.x - direction_move_back.w) == point_clip.w - point_clip.x
            //  =>  a == (point_clip.w - point_clip.x) / (direction_move_back.x - direction_move_back.w)
            {
                let a =
                    (point_clip.w - point_clip.x) / (direction_move_back.x - direction_move_back.w);
                if a > move_back_by {
                    move_back_by = a
                }
            }

            // top plane
            // equivalent to left plane
            {
                let a =
                    (point_clip.w - point_clip.y) / (direction_move_back.y - direction_move_back.w);
                if a > move_back_by {
                    move_back_by = a
                }
            }
        }
        self.log_camera_distance = (initial_distance + move_back_by).log2();
    }

    fn view_direction(&mut self, view: ViewDirection) {
        self.camera_rotation = match view {
            ViewDirection::Top => {
                Vector2::new(std::f64::consts::PI / 2.0, std::f64::consts::PI / 2.0)
            }
            ViewDirection::Left => Vector2::new(0.0, 0.0),
            ViewDirection::Front => Vector2::new(0.0, std::f64::consts::PI * 0.5),
            ViewDirection::Right => Vector2::new(0.0, std::f64::consts::PI),
            ViewDirection::Back => Vector2::new(0.0, std::f64::consts::PI * 1.5),
            ViewDirection::TopLeft => Vector2::new(std::f64::consts::PI / 4.0, 0.0),
            ViewDirection::TopFront => {
                Vector2::new(std::f64::consts::PI / 4.0, std::f64::consts::PI * 0.5)
            }
            ViewDirection::TopRight => {
                Vector2::new(std::f64::consts::PI / 4.0, std::f64::consts::PI)
            }
            ViewDirection::TopBack => {
                Vector2::new(std::f64::consts::PI / 4.0, std::f64::consts::PI * 1.5)
            }
        }
    }
}

impl Default for MapNavigation {
    fn default() -> Self {
        Self::new()
    }
}
