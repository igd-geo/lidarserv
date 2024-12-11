use crate::navigation::Matrices;
use pasture_core::nalgebra::{Matrix4, Rotation3, UnitQuaternion, Vector4};
use std::time::Duration;

/// Helper for creating smooth transitions between two sets of view/projection matrices.
pub struct AnimationHelper {
    current: Option<Animation>,
}

#[derive(Clone)]
struct Animation {
    from: Matrices,
    to: Matrices,
    duration: Duration,
    elapsed: Duration,
    easing: PolynomialEasingFunction,
}

#[derive(Copy, Clone)]
pub struct PolynomialEasingFunction(f64, f64, f64, f64);

impl PolynomialEasingFunction {
    pub fn linear() -> PolynomialEasingFunction {
        PolynomialEasingFunction(0.0, 0.0, 1.0, 0.0)
    }

    pub fn ease_in() -> PolynomialEasingFunction {
        PolynomialEasingFunction(0.0, 1.0, 0.0, 0.0)
    }

    pub fn ease_out() -> PolynomialEasingFunction {
        PolynomialEasingFunction(0.0, -1.0, 2.0, 0.0)
    }

    pub fn ease_in_out() -> PolynomialEasingFunction {
        PolynomialEasingFunction(-2.0, 3.0, 0.0, 0.0)
    }

    fn interpolation_factor(&self, linear: f64) -> f64 {
        let PolynomialEasingFunction(a, b, c, d) = *self;
        a * linear.powi(3) + b * linear.powi(2) + c * linear + d
    }
}

impl AnimationHelper {
    pub fn new() -> Self {
        AnimationHelper { current: None }
    }

    /// Starts an animation.
    pub fn start(
        &mut self,
        from: Matrices,
        to: Matrices,
        duration: Duration,
        easing: PolynomialEasingFunction,
    ) {
        match &mut self.current {
            None => {
                self.current = Some(Animation {
                    from,
                    to,
                    duration,
                    elapsed: Duration::ZERO,
                    easing,
                })
            }
            Some(current) => {
                let progress = current.elapsed.as_secs_f64() / current.duration.as_secs_f64();
                let t = current.easing.interpolation_factor(progress);
                let current_value = Self::interpolate_linear(&current.from, &current.to, t);

                *current = Animation {
                    from: current_value,
                    to,
                    duration,
                    elapsed: Duration::ZERO,
                    easing,
                }
            }
        }
    }

    /// Updates the value to animate to, if the animation is running.
    /// If the animation is inactive, this is a noop.
    pub fn update_animation_target(&mut self, to: Matrices) {
        if let Some(current) = &mut self.current {
            *current = Animation {
                to,
                ..current.clone()
            };
        }
    }

    /// Stops the current animation
    pub fn abort(&mut self) {
        self.current = None;
    }

    /// Advances the time by the given duration.
    pub fn update(&mut self, delta_t: Duration) {
        if let Some(current) = &mut self.current {
            current.elapsed += delta_t;
            if current.elapsed > current.duration {
                self.abort()
            }
        }
    }

    fn interpolate_linear(v1: &Matrices, v2: &Matrices, t: f64) -> Matrices {
        // matrix to convert from the coordinate system of the camera in v2 to the one in v1
        // (defined, such that cam_2_to_1 * v2.view_matrix == v1.view_matrix )
        let cam_2_to_1 = v1.view_matrix * v2.view_matrix_inv;

        // decomposition into translation and rotation
        // (assumes cam_2_to_1 is an isometry - but that should be reasonable)
        let translation_vec_hom = cam_2_to_1 * Vector4::new(0.0, 0.0, 0.0, 1.0);
        let translation_vec = translation_vec_hom.xyz() / translation_vec_hom.w;
        let mut rotation_mat =
            Rotation3::from_matrix_unchecked(cam_2_to_1.fixed_view::<3, 3>(0, 0).into());
        rotation_mat.renormalize();
        let rotation_quaternion: UnitQuaternion<f64> =
            UnitQuaternion::from_rotation_matrix(&rotation_mat);

        // interpolate translation and rotation
        let interpolated_translation_vec = (1.0 - t) * translation_vec;
        let interpolated_rotation_quaternion = rotation_quaternion
            .try_slerp(&UnitQuaternion::identity(), t, 0.00001)
            .unwrap_or_else(|| rotation_quaternion.nlerp(&UnitQuaternion::identity(), t));

        // re-compose the interpolated translation and rotation components
        let rot = interpolated_rotation_quaternion.to_rotation_matrix();
        let mat = Matrix4::new(
            rot[(0, 0)],
            rot[(0, 1)],
            rot[(0, 2)],
            interpolated_translation_vec.x,
            rot[(1, 0)],
            rot[(1, 1)],
            rot[(1, 2)],
            interpolated_translation_vec.y,
            rot[(2, 0)],
            rot[(2, 1)],
            rot[(2, 2)],
            interpolated_translation_vec.z,
            0.0,
            0.0,
            0.0,
            1.0,
        );
        let view_matrix = mat * v2.view_matrix;
        let view_matrix_inv = mat.try_inverse().unwrap() * v2.view_matrix_inv;

        Matrices {
            view_matrix,
            view_matrix_inv,
            projection_matrix: v2.projection_matrix, // do not interpolate the projection matrix - this might mess with e.g. the window size changing
            projection_matrix_inv: v2.projection_matrix_inv,
            window_size: v2.window_size,
        }
    }

    pub fn get_animated_value(&self) -> Option<Matrices> {
        match &self.current {
            None => None,
            Some(current) => {
                let progress = current.elapsed.as_secs_f64() / current.duration.as_secs_f64();
                let t = current.easing.interpolation_factor(progress);
                let current_value = Self::interpolate_linear(&current.from, &current.to, t);
                Some(current_value)
            }
        }
    }
}
