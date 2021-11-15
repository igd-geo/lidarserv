//! Implementation of the camera controls.
use crate::navigation::event::MouseDragSettings;
use pasture_core::math::AABB;
use pasture_core::nalgebra::{Matrix4, Vector2};

pub mod event;
pub mod map_navigation;

/// The Navigation is responsible for implementing the camera controls in the viewer.
///
/// It gets passed all input events, and controls the camera (view matrix and projection matrix)
/// based on the user input.
pub trait Navigation {
    /// Gets called, when the window size changes.
    fn on_window_resized(&mut self, w: f64, h: f64);

    /// Gets called, when the user "drags" the point cloud,
    /// by moving the mouse with a button held down by the specified amount of pixels.
    fn on_drag(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, drag: MouseDragSettings);

    /// Gets called, when the user scrolls with the mouse wheel.
    fn on_scroll(&mut self, d: f64);

    /// Gets called each frame before the rendering starts.
    /// The matrices, that are returned by [Self::update] are used during rendering.
    fn update(&mut self) -> Matrices;

    /// Moves the camera, such that the given aabb is in view.
    fn focus_on(&mut self, aabb: AABB<f64>);

    /// Changes the direction from which the camera looks at the point cloud.
    fn view_direction(&mut self, view: ViewDirection);

    // todo keyboard input
}

/// Matrices, that define the camera behavior.
///
/// The **view matrix** transforms (homogeneous) coordinates from world space into camera space.
///
/// World space is the "original" coordinate system of the point clouds. It is a right-handed
/// coordinate system. For most point clouds, it is oriented, such that the xy-plane is the "floor"
/// and the z axis points into the sky.
///
/// The **projection matrix** then transforms the coordinates in camera space to clip space.
/// For the clip space we are using clip coordinates, as defined by OpenGL. Renderers
/// that use other graphics APIs might therefore need to adapt the projection matrix to the
/// specific coordinate system used by their graphics API.
/// In particular, clip space ranges from -1.0 to 1.0 for any of the three axes. Anything
/// outside that range will be clipped away. Also, it is a left handed coordinate system, with
/// the X axis pointing to the right, Y pointing up, and Z pointing "into" the screen.
///
/// It is noteworthy, that world space is right-handed,
/// so the projection matrix needs to flip the Z axis.
///
/// Finally, the **window size** is the last bit of information that is needed, to map from
/// clip space to the actual on-screen pixel coordinates:
///  `x = (clip_x + 1.0) / 2.0 * window_size.x` and
///  `y = (clip_y + 1.0) / 2.0 * window_size.y`
#[derive(Clone, PartialEq)]
pub struct Matrices {
    pub view_matrix: Matrix4<f64>,
    pub projection_matrix: Matrix4<f64>,
    pub view_matrix_inv: Matrix4<f64>,
    pub projection_matrix_inv: Matrix4<f64>,
    pub window_size: Vector2<f64>,
}

/// The direction from which the camera looks at the scene
#[derive(Copy, Clone, Debug)]
pub enum ViewDirection {
    /// Viewed from above, camera looks straight down.
    Top,

    /// Viewed from the "left side". The camera looks into the direction of positive x.
    Left,

    /// Viewed from the "right side". The camera looks into the direction of negative x.
    Right,

    /// Viewed from "the front". The camera looks into the direction of positive y.
    Front,

    /// Viewed from "the back side". The camera looks into the direction of negative y.
    Back,

    TopLeft,
    TopRight,
    TopFront,
    TopBack,
}
