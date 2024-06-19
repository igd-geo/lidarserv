use crate::geometry::position::Position;
/// Defines generic types for lidar points.
/// Points consist at least out of a position, but can also contain any additional attributes.

pub trait PointType {
    type Position: Position;

    /// Create new point at the specified position with all the point attributes initialized
    /// to their default values.
    fn new(position: Self::Position) -> Self;

    /// Position of the point.
    fn position(&self) -> &Self::Position;

    /// Position of the point - mutable.
    fn position_mut(&mut self) -> &mut Self::Position;

    /// Access the point attribute specified by the type parameter.
    #[inline]
    fn attribute<T>(&self) -> &T
    where
        Self: WithAttr<T>,
    {
        self.value()
    }

    /// Access the point attribute specified by the type parameter.
    #[inline]
    fn attribute_mut<T>(&mut self) -> &mut T
    where
        Self: WithAttr<T>,
    {
        self.value_mut()
    }

    /// Access the point attribute specified by the type parameter.
    #[inline]
    fn set_attribute<T>(&mut self, new_value: T)
    where
        Self: WithAttr<T>,
    {
        self.set_value(new_value)
    }
}

pub trait WithAttr<T> {
    fn value(&self) -> &T;

    fn value_mut(&mut self) -> &mut T;

    fn set_value(&mut self, new_value: T) {
        *self.value_mut() = new_value;
    }
}

#[cfg(test)]
mod tests {
    use crate::geometry::points::{PointType, WithAttr};
    use crate::geometry::position::{F64Position, Position};

    struct ExampleAttribute(pub i32);

    struct FooBarAttribute(pub u64);

    struct Point {
        position: F64Position,
        example: ExampleAttribute,
        foo_bar: FooBarAttribute,
    }

    impl PointType for Point {
        type Position = F64Position;

        fn new(position: Self::Position) -> Self {
            Point {
                position,
                example: ExampleAttribute(0),
                foo_bar: FooBarAttribute(0),
            }
        }

        fn position(&self) -> &F64Position {
            &self.position
        }

        fn position_mut(&mut self) -> &mut Self::Position {
            &mut self.position
        }
    }

    impl WithAttr<ExampleAttribute> for Point {
        fn value(&self) -> &ExampleAttribute {
            &self.example
        }

        fn value_mut(&mut self) -> &mut ExampleAttribute {
            &mut self.example
        }
    }

    impl WithAttr<FooBarAttribute> for Point {
        fn value(&self) -> &FooBarAttribute {
            &self.foo_bar
        }

        fn value_mut(&mut self) -> &mut FooBarAttribute {
            &mut self.foo_bar
        }
    }

    #[test]
    fn test_point_attributes() {
        // create test point
        let mut point = Point {
            position: F64Position::from_components(0.0, 1.0, 2.0),
            example: ExampleAttribute(1),
            foo_bar: FooBarAttribute(2),
        };

        // set some attribute values
        point.set_attribute(FooBarAttribute(123));
        point.set_attribute(ExampleAttribute(42));

        // get and check attribute values
        let foo_bar = point.attribute::<FooBarAttribute>();
        let example: &ExampleAttribute = point.attribute();
        assert_eq!(foo_bar.0, 123);
        assert_eq!(example.0, 42);
    }
}
