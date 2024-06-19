pub mod bounding_box;
pub mod grid;
pub mod points;
pub mod position;
pub mod sampling;

#[cfg(test)]
pub mod test {
    use crate::geometry::points::PointType;
    use crate::geometry::position::{F64Position, Position};

    /// Trivial point type that can be used for unit tests
    #[derive(Debug)]
    pub struct Point {
        position: F64Position,
    }

    impl Point {
        pub fn new(x: f64, y: f64, z: f64) -> Self {
            Point {
                position: F64Position::from_components(x, y, z),
            }
        }
    }

    impl PointType for Point {
        type Position = F64Position;

        fn new(position: Self::Position) -> Self {
            Point { position }
        }

        fn position(&self) -> &Self::Position {
            &self.position
        }

        fn position_mut(&mut self) -> &mut Self::Position {
            &mut self.position
        }
    }
}
