use crate::nalgebra::{distance_squared, Scalar};
use nalgebra::{Point3, Vector3};
use num_traits::Bounded;
use std::fmt::Debug;
use std::ops::{Add, Sub};
use thiserror::Error;

/// Error type for [CoordinateSystem::encode_position].
#[derive(Debug, Error)]
pub enum CoordinateSystemError {
    #[error("The coordinate cannot be represented in this coordinate system, because it is out of its bounds.")]
    OutOfBounds,
}

/// A frame of reference for points.
pub trait CoordinateSystem: Debug {
    type Position: Position;

    /// Constructs a point in this coordinate system, that represents the given global
    /// position coordinates.
    fn encode_position(
        &self,
        global: &Point3<f64>,
    ) -> Result<Self::Position, CoordinateSystemError>;

    /// Returns the global position coordinates, that the point in this coordinate system represents.
    fn decode_position(&self, pos: &Self::Position) -> Point3<f64>;

    fn decode_distance(&self, distance: <Self::Position as Position>::Component) -> f64;
}

pub trait Component:
    Copy + Scalar + PartialOrd + Bounded + Sub<Output = Self> + Add<Output = Self>
{
    /// Center between two numbers ( result = (x1 + x2) / 2 )
    fn center(x1: Self, x2: Self) -> Self;
}

/// A position in 3d space
pub trait Position: Debug {
    type Component: Component;

    /// Returns the X component of the position in the coordinate system that it was created in.
    /// (To get the global point in space that this position encodes, independently from the used
    /// coordinate system, use [Position::decode].)
    fn x(&self) -> Self::Component;

    /// Returns the Y component of the position in the coordinate system that it was created in.
    fn y(&self) -> Self::Component;

    /// Returns the Z component of the position in the coordinate system that it was created in.
    fn z(&self) -> Self::Component;

    /// Constructs a position directly from its x,y,z components.
    /// You most likely want to construct a position via some [CoordinateSystem], instead. This
    /// is mostly useful for quick 'n dirty unit tests.
    fn from_components(x: Self::Component, y: Self::Component, z: Self::Component) -> Self;

    /// Calculate the distance between the two positions, assuming both positions were created in
    /// the same coordinate system.
    /// The distance is not meant to be interpreted as an absolute number, i.e. it cannot be
    /// interpreted as "distance in meters". Rather distances can be compared to each other, to
    /// establish an "is-closer-to-than" relationship.
    fn distance_to(&self, other: &Self) -> Self::Component;

    /// Constructs a point in the given coordinate system, that represents the given global
    /// position coordinates.
    #[inline]
    fn encode<C>(coordinate_system: &C, global: &Point3<f64>) -> Result<Self, CoordinateSystemError>
    where
        C: CoordinateSystem<Position = Self>,
        Self: Sized,
    {
        coordinate_system.encode_position(global)
    }

    /// Returns the global position coordinates, that this position encodes.
    #[inline]
    fn decode<C>(&self, coordinate_system: &C) -> Point3<f64>
    where
        C: CoordinateSystem<Position = Self>,
    {
        coordinate_system.decode_position(self)
    }

    #[inline]
    fn transcode<C1, C2>(
        &self,
        source_coordinate_system: &C1,
        target_coordinate_system: &C2,
    ) -> Result<C2::Position, CoordinateSystemError>
    where
        C1: CoordinateSystem<Position = Self>,
        C2: CoordinateSystem,
    {
        let global = source_coordinate_system.decode_position(self);
        target_coordinate_system.encode_position(&global)
    }
}

impl Component for f32 {
    fn center(x1: Self, x2: Self) -> Self {
        (x1 + x2) / 2.0
    }
}

impl Component for f64 {
    fn center(x1: Self, x2: Self) -> Self {
        (x1 + x2) / 2.0
    }
}

impl Component for i32 {
    fn center(x1: Self, x2: Self) -> Self {
        x1 / 2 + x2 / 2
    }
}

impl Component for i64 {
    fn center(x1: Self, x2: Self) -> Self {
        (x1 + x2) / 2
    }
}

/// A simple coordinate system for f64 positions,
/// that does not apply any transformation to the global point coordinates.
#[derive(Debug, Copy, Clone, Default)]
pub struct F64CoordinateSystem;

/// Position with f64 x, y and z components.
#[derive(Debug, Clone, PartialEq)]
pub struct F64Position(Point3<f64>);

impl F64Position {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        F64Position(Point3::new(x, y, z))
    }

    pub fn global_position(&self) -> &Point3<f64> {
        &self.0
    }

    pub fn set_x(&mut self, x: f64) {
        self.0.x = x
    }

    pub fn set_y(&mut self, y: f64) {
        self.0.y = y
    }

    pub fn set_z(&mut self, z: f64) {
        self.0.z = z
    }
}

impl Default for F64Position {
    fn default() -> Self {
        F64Position(Point3::new(0.0, 0.0, 0.0))
    }
}

impl F64CoordinateSystem {
    /// Construct new coordinate system.
    pub fn new() -> Self {
        F64CoordinateSystem
    }
}

impl CoordinateSystem for F64CoordinateSystem {
    type Position = F64Position;

    #[inline]
    fn encode_position(
        &self,
        global: &Point3<f64>,
    ) -> Result<Self::Position, CoordinateSystemError> {
        Ok(F64Position(*global))
    }

    #[inline]
    fn decode_position(&self, pos: &Self::Position) -> Point3<f64> {
        pos.0
    }

    #[inline]
    fn decode_distance(&self, distance: f64) -> f64 {
        distance
    }
}

impl Position for F64Position {
    type Component = f64;

    fn x(&self) -> Self::Component {
        self.0.x
    }

    fn y(&self) -> Self::Component {
        self.0.y
    }

    fn z(&self) -> Self::Component {
        self.0.z
    }

    fn from_components(x: Self::Component, y: Self::Component, z: Self::Component) -> Self {
        F64Position(Point3::new(x, y, z))
    }

    /// Squared (euclidean) distance.
    fn distance_to(&self, other: &Self) -> Self::Component {
        distance_squared(&self.0, &other.0)
    }
}

/// Coordinate system for i32 coordinates, that is given explicit bounds for the coordinates,
/// that it can represent. Coordinates within the bounds are mapped to the
/// range \[i32::MIN - i32::MAX\].
#[derive(Debug, Clone, PartialEq)]
pub struct I32CoordinateSystem {
    scale: Vector3<f64>,
    offset: Vector3<f64>,
}

impl I32CoordinateSystem {
    /// Construct a new coordinate system with the given bounds.
    pub fn new(min: Point3<f64>, max: Point3<f64>) -> Self {
        assert!(min.x < max.x);
        assert!(min.y < max.y);
        assert!(min.z < max.z);
        let int_min = i32::MIN as f64;
        let int_max = i32::MAX as f64;
        let int_range = Vector3::repeat(int_max - int_min);
        let range = max.coords - min.coords;
        let scale = range.component_div(&int_range);
        let offset = min.coords - Vector3::repeat(int_min).component_mul(&scale);

        I32CoordinateSystem { scale, offset }
    }

    pub fn from_las_transform(scale: Vector3<f64>, offset: Vector3<f64>) -> Self {
        I32CoordinateSystem { scale, offset }
    }

    pub fn scale(&self) -> &Vector3<f64> {
        &self.scale
    }

    pub fn offset(&self) -> &Vector3<f64> {
        &self.offset
    }
}

impl CoordinateSystem for I32CoordinateSystem {
    type Position = I32Position;

    fn encode_position(
        &self,
        global: &Point3<f64>,
    ) -> Result<Self::Position, CoordinateSystemError> {
        // transformation
        let inner = (global.coords - self.offset).component_div(&self.scale);

        // bounds check
        let int_min = i32::MIN as f64;
        let int_max = i32::MAX as f64;
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
        Ok(I32Position(Point3::new(
            inner.x.round() as i32,
            inner.y.round() as i32,
            inner.z.round() as i32,
        )))
    }

    fn decode_position(&self, pos: &Self::Position) -> Point3<f64> {
        let pos_f64 = Vector3::<f64>::new(pos.0.x as f64, pos.0.y as f64, pos.0.z as f64);
        Point3::from(pos_f64.component_mul(&self.scale) + self.offset)
    }

    fn decode_distance(&self, distance: i32) -> f64 {
        // assume a distance along the x axis
        self.scale.x * distance as f64
    }
}

/// Position with i32 x, y and z components.
#[derive(Debug, Clone)]
pub struct I32Position(Point3<i32>);

impl Default for I32Position {
    fn default() -> Self {
        I32Position(Point3::new(0, 0, 0))
    }
}

impl Position for I32Position {
    type Component = i32;

    fn x(&self) -> Self::Component {
        self.0.x
    }

    fn y(&self) -> Self::Component {
        self.0.y
    }

    fn z(&self) -> Self::Component {
        self.0.z
    }

    fn from_components(x: Self::Component, y: Self::Component, z: Self::Component) -> Self {
        I32Position(Point3::new(x, y, z))
    }

    /// manhattan distance
    fn distance_to(&self, other: &Self) -> Self::Component {
        let p = self.0 - other.0;
        p.x.abs()
            .saturating_add(p.y.abs())
            .saturating_add(p.z.abs())
    }
}

#[cfg(test)]
mod tests {
    use crate::geometry::position::{
        CoordinateSystem, F64CoordinateSystem, F64Position, I32CoordinateSystem, I32Position,
        Position,
    };
    use crate::nalgebra::{Point3, Vector3};

    #[test]
    fn f64_position_encode_decode() {
        let coordinate_system = F64CoordinateSystem::new();

        // encode
        let p = F64Position::encode(&coordinate_system, &Point3::new(1.0, 2.0, 3.0)).unwrap();
        assert_eq!(p, F64Position(Point3::new(1.0, 2.0, 3.0)));
        assert_eq!(p.x(), 1.0);
        assert_eq!(p.y(), 2.0);
        assert_eq!(p.z(), 3.0);

        // decode
        let global = p.decode(&coordinate_system);
        assert_eq!(global, Point3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn int_position_encode_decode_las() {
        // make sure, that the definition of scale and offset is consistent with the one
        // of the point transformations in LAS.
        let coordinate_system = I32CoordinateSystem::from_las_transform(
            Vector3::new(0.01, 0.01, 0.01),
            Vector3::new(5.0, 5.0, 5.0),
        );

        // encode
        let p = coordinate_system
            .encode_position(&Point3::new(4.0, 5.2, 6.01))
            .unwrap();
        assert_eq!(p.x(), -100);
        assert_eq!(p.y(), 20);
        assert_eq!(p.z(), 101);

        // decode
        let global = I32Position::from_components(-200, 1, 2).decode(&coordinate_system);
        assert_eq!(global, Point3::new(3.0, 5.01, 5.02));
    }

    #[test]
    fn int_position_encode_decode() {
        let coordinate_system = I32CoordinateSystem::new(
            Point3::new(-10.0, -10.0, -10.0),
            Point3::new(10.0, 10.0, 10.0),
        );

        // encode
        let p = coordinate_system
            .encode_position(&Point3::new(-10.0, 0.0, 10.0))
            .unwrap();
        assert_eq!(p.x(), i32::MIN);
        assert_eq!(p.y(), 0);
        assert_eq!(p.z(), i32::MAX);

        // decode
        let global = p.decode(&coordinate_system);
        let diff = (global.coords - Vector3::new(-10.0, 0.0, 10.0)).norm();
        assert!(diff < 0.001); // allow for small rounding errors
    }

    #[test]
    fn int_position_encode_bounds() {
        let coordinate_system = I32CoordinateSystem::new(
            Point3::new(-10.0, -10.0, -10.0),
            Point3::new(10.0, 10.0, 10.0),
        );

        // bounds min is still ok
        assert!(coordinate_system
            .encode_position(&Point3::new(-10.0, -10.0, -10.0))
            .is_ok());

        // bounds max is still ok
        assert!(coordinate_system
            .encode_position(&Point3::new(10.0, 10.0, 10.0))
            .is_ok());

        // everything, that exceeds the bounds leads to an error
        assert!(coordinate_system
            .encode_position(&Point3::new(-10.1, 0.0, 0.0))
            .is_err());
        assert!(coordinate_system
            .encode_position(&Point3::new(0.0, -10.1, 0.0))
            .is_err());
        assert!(coordinate_system
            .encode_position(&Point3::new(0.0, 0.0, -10.1))
            .is_err());
        assert!(coordinate_system
            .encode_position(&Point3::new(10.1, 0.0, 0.0))
            .is_err());
        assert!(coordinate_system
            .encode_position(&Point3::new(0.0, 10.1, 0.0))
            .is_err());
        assert!(coordinate_system
            .encode_position(&Point3::new(0.0, 0.0, 10.1))
            .is_err());
    }
}
