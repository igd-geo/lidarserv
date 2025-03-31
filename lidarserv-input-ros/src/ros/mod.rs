use anyhow::Result;
use nalgebra::{matrix, Matrix4, UnitQuaternion, Vector3};
use std::{
    sync::{mpsc, Arc},
    time::Duration,
};

use crate::{cli::AppOptions, status::Status};

mod ros1;

pub fn ros_thread(
    args: AppOptions,
    commands_rx: mpsc::Receiver<Command>,
    transforms_tx: mpsc::Sender<Transform>,
    points_tx: mpsc::Sender<PointCloudMessage>,
    status: Arc<Status>,
) -> Result<()> {
    // in the future this could call either ros1 or ros2
    // (once we add support for ros2)
    ros1::ros_thread(args, commands_rx, transforms_tx, points_tx, status)
}

pub enum Command {
    Exit,
}

/// Describes the transformation between some frame in the ROS transform tree and its parent frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    /// Name of the frame, that is described by this transform.
    pub frame: String,

    /// Parent frame. The transform describes the relative location of the frame
    /// with respect to its parent frame.
    pub parent_frame: String,

    /// Static frames are valid for all time (they generally don't "move", are fixed in space.)
    /// Dynamic frames do change over time (e.g. frames attached to a moveable joint. Or the position of the GPS sensor in world space.) and are expected to be updated in regular intervals.
    pub is_static: bool,

    /// Time stamp at which this transform was valid.
    pub time_stamp: Duration,

    /// Translation in 3d space
    pub translation: Vector3<f64>,

    /// Rotation in 3d space
    pub rotation: UnitQuaternion<f64>,
}

/// Contents of a ros PointCloud2 message. Describes a packet of points captured by the lidar sensor.
pub struct PointCloudMessage {
    /// Name of the frame, that the coordinates are in
    pub frame: String,

    /// Time stamp at which these points were captured.
    pub time_stamp: Duration,

    /// Endianess of the point data.
    /// Values might need conversion if this does not match the native endianess.
    pub endianess: Endianess,

    /// Number of points per row
    pub height: usize,

    /// Number of rows
    pub width: usize,

    /// Number of bytes in a point.
    pub point_step: usize,

    /// Number of bytes in a row.
    /// (might be more than `point_step * width`,
    /// because there might be extra padding at the end of each row.)
    pub row_step: usize,

    /// Point layout.
    pub fields: Vec<Field>,

    /// The actual point data
    pub data: Vec<u8>,
}

/// Data types of fields supported by ROS.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Type {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

/// Point attribute as defined by ROS
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Field {
    /// Field name. Common names are "x", "y", "z", "intensity".
    pub name: String,

    /// Byte offset within the point
    pub offset: usize,

    /// Datatype
    pub typ: Type,

    /// Number of elements (usually 1)
    pub count: usize,
}

/// Byte order: Little or big endian.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Endianess {
    BigEndian,
    LittleEndian,
}

impl Transform {
    /// Interpolates between this and some other transform.
    ///
    /// The parameter `w` is the interpolation weight
    /// that controls which value between `self` and `other` to
    /// return. The value `w == 0.0` corresponds to `self` and `w == 1.0`
    /// corresponds to `other`.
    ///
    /// Interpolation might not be possible, if:
    ///  - The angle of the rotations is very close to 180°
    ///  - Both transformations are with respect to different parent frames.
    ///
    /// In this case, a step function is used that returns
    /// `self` for `w < 1.0` and `other` for `w >= 1.0`.
    pub fn interpolate(&self, w: f64, other: &Transform) -> Transform {
        // Interpolate the frame using a step function.
        // (Frames should be identical for 99% of all use cases of calling this function anyway.
        // maybe we should consider logging a warning or even panicking, if the frames are different.)
        let frame = if w < 1.0 {
            self.frame.clone()
        } else {
            other.frame.clone()
        };

        // Linearily interpolate the time stamps.
        let time_stamp = Duration::from_secs_f64(
            self.time_stamp.as_secs_f64() * (1.0 - w) + other.time_stamp.as_secs_f64() * w,
        );

        // Only attempt "normal" interpolation, if both transforms are with respect to the same parent frame.
        if self.parent_frame == other.parent_frame {
            // Interpolate the rotation (slerp). Only continue, if the interpolated rotation is well-defined.
            let maybe_rotation = self.rotation.try_slerp(&other.rotation, w, f64::EPSILON);
            if let Some(rotation) = maybe_rotation {
                // Linearily interpolate the translation.
                let translation = self.translation.lerp(&other.translation, w);

                // Build resulting transform
                return Transform {
                    frame,
                    parent_frame: self.parent_frame.clone(),
                    is_static: false,
                    time_stamp,
                    translation,
                    rotation,
                };
            }
        }

        // use step function if interpolation was not possible
        if w < 1.0 {
            Transform {
                frame,
                parent_frame: self.parent_frame.clone(),
                is_static: false,
                time_stamp,
                translation: self.translation,
                rotation: self.rotation,
            }
        } else {
            Transform {
                frame,
                parent_frame: other.parent_frame.clone(),
                is_static: false,
                time_stamp,
                translation: other.translation,
                rotation: other.rotation,
            }
        }
    }

    /// Returns the transformation matrix for the transformation from
    /// the frame to the parent frame.
    pub fn matrix(&self) -> Matrix4<f64> {
        let translation = matrix![
            1.0, 0.0, 0.0, self.translation.x;
            0.0, 1.0, 0.0, self.translation.y;
            0.0, 0.0, 1.0, self.translation.z;
            0.0, 0.0, 0.0, 1.0;
        ];
        let rotation = self.rotation.to_rotation_matrix().matrix().to_homogeneous();
        translation * rotation
    }

    /// Returns the transformation matrix for transforming points
    /// from the parent frame into the frame.
    pub fn inverse_matrix(&self) -> Matrix4<f64> {
        let translation = matrix![
            1.0, 0.0, 0.0, -self.translation.x;
            0.0, 1.0, 0.0, -self.translation.y;
            0.0, 0.0, 1.0, -self.translation.z;
            0.0, 0.0, 0.0, 1.0;
        ];
        let rotation = self
            .rotation
            .inverse()
            .to_rotation_matrix()
            .matrix()
            .to_homogeneous();
        rotation * translation
    }
}

impl Type {
    pub fn len(&self) -> usize {
        match self {
            Type::I8 => 1,
            Type::U8 => 1,
            Type::I16 => 2,
            Type::U16 => 2,
            Type::I32 => 4,
            Type::U32 => 4,
            Type::F32 => 4,
            Type::F64 => 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        f64::{self, consts::PI},
        time::Duration,
    };

    use nalgebra::{point, vector, UnitQuaternion, UnitVector3, Vector3};

    use super::Transform;

    /// Tests that [Transform::inverse_matrix] always returns the inverse of [Transform::matrix].
    #[test]
    fn test_transform_inverse() {
        let mut test_data = [
            -3.2, 4.5, -7.8, 9.1, -2.3, 6.7, -8.9, 1.2, -4.6, 7.3, -9.0, 2.4, -5.7, 8.1, -1.3, 3.6,
            -6.8, 9.4, -2.1, 5.9, -7.4, 1.5, -3.9, 6.2, -8.7, 2.8, -4.1, 7.6, -9.3, 3.1, -5.4, 8.9,
            -1.7, 4.2, -6.5, 9.0, -2.9, 5.3, -7.6, 1.8, -4.3, 6.9, -8.2, 2.1, -5.8, 7.4, -1.9, 3.7,
            -6.1, 9.2, -2.4, 5.6, -7.9, 1.3, -4.7, 8.0, -9.1, 2.5, -5.2, 7.1, -3.4, 6.8, -8.5, 1.9,
            -4.0, 7.5, -9.2, 2.7, -5.1, 8.3,
        ]
        .as_slice();
        let mut semi_random = || -> f64 {
            let (number, rest) = test_data
                .split_first()
                .expect("random_numbers array is too short.");
            test_data = rest;
            *number
        };

        for i in 0..10 {
            // create a semi-random transformation object
            let translation = vector![semi_random(), semi_random(), semi_random()];
            let rotation = UnitQuaternion::from_axis_angle(
                &UnitVector3::new_normalize(vector![semi_random(), semi_random(), semi_random()]),
                semi_random(),
            );
            let transform = Transform {
                frame: "frame".to_string(),
                parent_frame: "parent_frame".to_string(),
                is_static: false,
                time_stamp: Duration::ZERO,
                translation,
                rotation,
            };

            // ensure that the transform.inverse_matrix() is indeed the inverse of transform.matrix().
            let matrix = transform.matrix();
            let inverse = transform.inverse_matrix();
            let product = inverse * matrix;
            dbg!(i, translation, rotation, product);
            assert!(product.is_identity(1e-10));
        }
    }

    /// Tests [Transform::matrix] and [Transform::inverse_matrix] using one example transform and point.
    #[test]
    fn test_transform_matrix() {
        // example transform
        // with a 90° rotation in the xy-plane (anti-clockwise)
        // and a translation along the z axis
        let transform = Transform {
            frame: "frame".to_string(),
            parent_frame: "parent_frame".to_string(),
            is_static: false,
            time_stamp: Duration::ZERO,
            translation: vector![2.0, 0.0, 10.0],
            rotation: UnitQuaternion::from_axis_angle(&Vector3::z_axis(), PI * 0.5),
        };

        // transform point
        let point = point![1.0, 0.0, 0.0];
        let transformed = transform.matrix().transform_point(&point);
        let transformed_expected = point![2.0, 1.0, 10.0];
        dbg!(transformed, transformed_expected);
        assert!((transformed - transformed_expected)
            .iter()
            .all(|c| c.abs() < 1e-10));

        // transform back
        let original = transform.inverse_matrix().transform_point(&transformed);
        let original_expected = point;
        dbg!(original, original_expected);
        assert!((original - original_expected)
            .iter()
            .all(|c| c.abs() < 1e-10));
    }

    /// Test for [Transform::interpolate]
    #[test]
    fn test_transform_interpolate() {
        let transform1 = Transform {
            frame: "frame".to_string(),
            parent_frame: "parent_frame".to_string(),
            is_static: false,
            time_stamp: Duration::from_secs(50),
            translation: vector![0.0, 0.0, 10.0],
            rotation: UnitQuaternion::from_axis_angle(
                &UnitVector3::new_normalize(vector![0.0, 0.0, 1.0]),
                PI * 0.5,
            ),
        };
        let transform2 = Transform {
            frame: "frame".to_string(),
            parent_frame: "parent_frame".to_string(),
            is_static: false,
            time_stamp: Duration::from_secs(60),
            translation: vector![0.0, 0.0, 20.0],
            rotation: UnitQuaternion::from_axis_angle(
                &UnitVector3::new_normalize(vector![0.0, 0.0, 1.0]),
                PI,
            ),
        };

        let interpol_1 = transform1.interpolate(0.0, &transform2);
        let interpol_2 = transform1.interpolate(0.5, &transform2);
        let interpol_3 = transform1.interpolate(1.0, &transform2);

        /// Returns true, if both transformations can be considered equal (within some epsilon environment of each other).
        fn transform_eq(t1: &Transform, t2: &Transform) -> bool {
            t1.frame == t2.frame
                && t1.parent_frame == t2.parent_frame
                && t1.is_static == t2.is_static
                && (t1.time_stamp.as_secs_f64() - t2.time_stamp.as_secs_f64()).abs() < f64::EPSILON
                && (t1.translation - t2.translation).norm() < f64::EPSILON
                && t1.rotation.angle_to(&t2.rotation).abs() < f64::EPSILON
        }

        assert!(transform_eq(&interpol_1, &transform1));
        assert!(transform_eq(
            &interpol_2,
            &Transform {
                frame: "frame".to_string(),
                parent_frame: "parent_frame".to_string(),
                is_static: false,
                time_stamp: Duration::from_secs(55),
                translation: vector![0.0, 0.0, 15.0],
                rotation: UnitQuaternion::from_axis_angle(
                    &UnitVector3::new_normalize(vector![0.0, 0.0, 1.0]),
                    PI * 0.75,
                ),
            }
        ));
        assert!(transform_eq(&interpol_3, &transform2));
    }
}
