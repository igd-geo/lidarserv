use core::f64;
use nalgebra::{Vector3, vector};
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, mem, ops::RangeInclusive};
use thiserror::Error;

use crate::f64_utils::{f64_next_down, f64_next_up};

use super::{
    bounding_box::Aabb,
    position::{Component, Position, PositionGlobal},
};

/// Error type for [CoordinateSystem::encode_position].
#[derive(Debug, Error)]
pub enum CoordinateSystemError {
    #[error(
        "The coordinate cannot be represented in this coordinate system, because it is out of its bounds."
    )]
    OutOfBounds,
}

/// The coordinate system is used to convert between
/// the stored coordinates and actual "world coordinates".
#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize)]
pub struct CoordinateSystem {
    scale: Vector3<f64>,
    offset: Vector3<f64>,
}

impl CoordinateSystem {
    /// Construct a new coordinate system with unit scale and no offset
    pub fn new_identity() -> Self {
        CoordinateSystem {
            scale: vector![1.0, 1.0, 1.0],
            offset: vector![0.0, 0.0, 0.0],
        }
    }

    /// Construct a new coordinate system with the given scale and offset
    pub fn from_las_transform(scale: Vector3<f64>, offset: Vector3<f64>) -> Self {
        CoordinateSystem { scale, offset }
    }

    pub fn scale(&self) -> &Vector3<f64> {
        &self.scale
    }

    pub fn offset(&self) -> &Vector3<f64> {
        &self.offset
    }

    pub fn add_offset(&mut self, offset: Vector3<f64>) {
        self.offset += offset;
    }

    pub fn encode_position<C: Component>(
        &self,
        global: PositionGlobal,
    ) -> Result<Position<C>, CoordinateSystemError> {
        // transformation
        let inner = (global.coords - self.offset).component_div(&self.scale);

        // bounds check
        let int_min = C::MIN.to_f64();
        let int_max = C::MAX.to_f64();
        if inner.x < int_min
            || inner.x > int_max
            || inner.y < int_min
            || inner.y > int_max
            || inner.z < int_min
            || inner.z > int_max
        {
            return Err(CoordinateSystemError::OutOfBounds);
        }

        // convert to int
        Ok(inner.map(|c| C::from_f64(c)).into())
    }

    pub fn encode_distance<C: Component>(&self, global: f64) -> Result<C, CoordinateSystemError> {
        // transformation
        let inner = global / self.scale.x;

        // bounds check
        let int_min = C::MIN.to_f64();
        let int_max = C::MAX.to_f64();
        if inner < int_min || inner > int_max {
            return Err(CoordinateSystemError::OutOfBounds);
        }

        // convert to int
        Ok(C::from_f64(inner))
    }

    pub fn decode_position<C: Component>(&self, pos: Position<C>) -> PositionGlobal {
        let pos_f64 = pos.map(|c| c.to_f64()).coords;
        (self.offset + pos_f64.component_mul(&self.scale)).into()
    }

    /// Returns the bounds in the global coordinate system,
    /// that can be represented using the given component type in this coordinate system.
    ///
    /// This performs conservative rounding.
    /// Float values are always rounded towards the inside of the
    /// bounds (I.e.: The minimum is always rounded up and the maximum is always
    /// rounded down).
    /// With normal rounding, bounds.min and bounds.max could end up slightly outside the
    /// exact counds, causing encode_position to fail.
    /// This guarantees, that bounds.min and bounds.max are always slightly inside the
    /// real bounds and encode_position will never fail.
    pub fn bounds<C: Component>(&self) -> Aabb<f64> {
        let mut result = Aabb::empty();
        for i in 0..3 {
            let scale = self.scale[i];
            let offset = self.offset[i];

            let a1 = C::MIN.to_f64();
            let a2 = C::MAX.to_f64();

            // apply scale
            let mut b1 = a1 * scale;
            let mut b2 = a2 * scale;

            // make sure that the result is rounded into the correct direction
            if b1 / scale < a1 {
                if scale > 0.0 {
                    b1 = f64_next_up(b1);
                } else {
                    b1 = f64_next_down(b1);
                }
            }
            if b2 / scale > a2 {
                if scale > 0.0 {
                    b2 = f64_next_down(b2);
                } else {
                    b2 = f64_next_up(b2);
                }
            }

            // if scale is negativ, the minimum becomes the maximum and vice-versa
            if scale < 0.0 {
                mem::swap(&mut b1, &mut b2)
            }

            // apply offset
            let mut c1 = b1 + offset;
            let mut c2 = b2 + offset;

            // make sure that the result is rounded into the correct direction
            if c1 - offset < b1 {
                c1 = f64_next_up(c1);
            }
            if c2 - offset > b2 {
                c2 = f64_next_down(c2);
            }

            // done
            result.min[i] = c1;
            result.max[i] = c2;
        }

        result
    }

    pub fn bounds_distance<C: Component>(&self) -> RangeInclusive<f64> {
        let scale = self.scale.x;

        let a1 = C::MIN.to_f64();
        let a2 = C::MAX.to_f64();

        // apply scale
        let mut b1 = a1 * scale;
        let mut b2 = a2 * scale;

        // make sure that the result is rounded into the correct direction
        if b1 / scale < a1 {
            if scale > 0.0 {
                b1 = f64_next_up(b1);
            } else {
                b1 = f64_next_down(b1);
            }
        }
        if b2 / scale > a2 {
            if scale > 0.0 {
                b2 = f64_next_down(b2);
            } else {
                b2 = f64_next_up(b2);
            }
        }

        // if scale is negativ, the minimum becomes the maximum and vice-versa
        if scale < 0.0 {
            mem::swap(&mut b1, &mut b2)
        }

        b1..=b2
    }

    pub fn decode_distance<C: Component>(&self, distance: C) -> f64 {
        // assume a distance along the x axis
        self.scale.x * distance.to_f64()
    }
}

#[cfg(test)]
mod tests {
    use nalgebra::{point, vector};

    use crate::{
        geometry::{bounding_box::Aabb, coordinate_system::CoordinateSystem},
        nalgebra::{Point3, Vector3},
    };

    #[test]
    fn int_position_encode_decode_las() {
        // make sure, that the definition of scale and offset is consistent with the one
        // of the point transformations in LAS.
        let coordinate_system = CoordinateSystem::from_las_transform(
            Vector3::new(0.01, 0.01, 0.01),
            Vector3::new(5.0, 5.0, 5.0),
        );

        // encode
        let p: Point3<i32> = coordinate_system
            .encode_position(Point3::new(4.0, 5.2, 6.01))
            .unwrap();
        assert_eq!(p.x, -100);
        assert_eq!(p.y, 20);
        assert_eq!(p.z, 101);

        // decode
        let global = coordinate_system.decode_position(Point3::new(-200, 1, 2));
        assert_eq!(global, Point3::new(3.0, 5.01, 5.02));
    }

    #[test]
    fn test_bounds() {
        let cs1 = CoordinateSystem::new_identity();
        let cs2 = CoordinateSystem::from_las_transform(
            vector![0.25, 0.25, 0.25],
            vector![0.0, 2.0, -3.0],
        );
        let cs3 = CoordinateSystem::from_las_transform(
            vector![-0.25, -0.25, -0.25],
            vector![0.0, 0.0, 0.0],
        );

        assert_eq!(
            cs1.bounds::<f64>(),
            Aabb::new(
                point![f64::MIN, f64::MIN, f64::MIN],
                point![f64::MAX, f64::MAX, f64::MAX],
            )
        );
        assert_eq!(
            cs1.bounds::<i32>(),
            Aabb::new(
                point![-2147483648.0, -2147483648.0, -2147483648.0],
                point![2147483647.0, 2147483647.0, 2147483647.0],
            )
        );
        assert_eq!(
            cs2.bounds::<f64>(),
            Aabb::new(
                point![f64::MIN, f64::MIN, f64::MIN] * 0.25 + vector![0.0, 2.0, -3.0],
                point![f64::MAX, f64::MAX, f64::MAX] * 0.25 + vector![0.0, 2.0, -3.0],
            )
        );
        assert_eq!(
            cs2.bounds::<i32>(),
            Aabb::new(
                point![-536870912.0, -536870910.0, -536870915.0],
                point![536870911.75, 536870913.75, 536870908.75],
            )
        );
        assert_eq!(
            cs3.bounds::<f64>(),
            Aabb::new(
                point![f64::MIN, f64::MIN, f64::MIN] * 0.25,
                point![f64::MAX, f64::MAX, f64::MAX] * 0.25,
            )
        );
        assert_eq!(
            cs3.bounds::<i32>(),
            Aabb::new(
                point![-536870911.75, -536870911.75, -536870911.75],
                point![536870912.0, 536870912.0, 536870912.0],
            )
        );
    }
}
