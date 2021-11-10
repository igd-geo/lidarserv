use crate::geometry::position::{Component, Position};
use nalgebra::{Point3, Scalar};
use num_traits::Bounded;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};

/// An axis aligned bounding box
pub trait BaseAABB<C: Scalar>: Debug + Clone + PartialEq {
    /// Construct a new AABB with the given bounds.
    fn new(min: Point3<C>, max: Point3<C>) -> Self;

    /// Check, if the given point is within the bounds.
    fn contains<P: Position<Component = C>>(&self, point: &P) -> bool;

    /// Grow the bounding box, so that it contains the given position.
    fn extend<P: Position<Component = C>>(&mut self, position: &P);

    fn extend_other(&mut self, other: &OptionAABB<C>);
}

/// An axis aligned bounding box.
///
/// The bounding box is defined via a minimum and a maximum bound. However, no assertion is made if
/// `min <= max` actually holds. If the min bound is larger than the max bound, the bounding box
/// can be thought of as being empty.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct OptionAABB<C: Scalar> {
    min: Point3<C>,
    max: Point3<C>,
}

impl<C: Component> BaseAABB<C> for OptionAABB<C> {
    fn new(min: Point3<C>, max: Point3<C>) -> Self {
        OptionAABB { min, max }
    }

    fn contains<P: Position<Component = C>>(&self, point: &P) -> bool {
        self.min.x <= point.x()
            && self.min.y <= point.y()
            && self.min.z <= point.z()
            && self.max.x >= point.x()
            && self.max.y >= point.y()
            && self.max.z >= point.z()
    }

    fn extend<P: Position<Component = C>>(&mut self, position: &P) {
        if self.min.x > position.x() {
            self.min.x = position.x();
        }
        if self.min.y > position.y() {
            self.min.y = position.y();
        }
        if self.min.z > position.z() {
            self.min.z = position.z();
        }
        if self.max.x < position.x() {
            self.max.x = position.x();
        }
        if self.max.y < position.y() {
            self.max.y = position.y();
        }
        if self.max.z < position.z() {
            self.max.z = position.z();
        }
    }

    fn extend_other(&mut self, other: &OptionAABB<C>) {
        if other.min.x < self.min.x {
            self.min.x = other.min.x;
        }
        if other.min.y < self.min.y {
            self.min.y = other.min.y;
        }
        if other.min.z < self.min.z {
            self.min.z = other.min.z;
        }
        if other.max.x > self.max.x {
            self.max.x = other.max.y;
        }
        if other.max.y > self.max.y {
            self.max.y = other.max.y;
        }
        if other.max.z > self.max.z {
            self.max.z = other.max.z;
        }
    }
}

impl<C: Component> OptionAABB<C> {
    /// Constructs an empty bounding box.
    pub fn empty() -> Self
    where
        C: Bounded,
    {
        let min = C::min_value();
        let max = C::max_value();
        OptionAABB {
            min: Point3::new(max, max, max),
            max: Point3::new(min, min, min),
        }
    }

    /// Checks, if the bounding box is empty.
    pub fn is_empty(&self) -> bool
    where
        C: PartialOrd,
    {
        self.min.x > self.max.x || self.min.y > self.max.y || self.min.z > self.max.z
    }

    /// Tries to convert this [OptionAABB] into an [AABB].
    /// Returns None, if the bounding box is empty, otherwise the
    /// corresponding [AABB] with the same bounds is returned.
    pub fn into_aabb(self) -> Option<AABB<C>>
    where
        C: PartialOrd,
    {
        if self.is_empty() {
            None
        } else {
            Some(AABB { inner: self })
        }
    }
}

impl<C: Component> Default for OptionAABB<C> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<C: Component> Debug for OptionAABB<C> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            write!(f, "OptionAABB (empty)")
        } else {
            write!(
                f,
                "OptionAABB ({:?},{:?},{:?} - {:?},{:?},{:?})",
                self.min.x, self.min.y, self.min.z, self.max.x, self.max.y, self.max.z
            )
        }
    }
}

/// An axis aligned bounding box, that is guaranteed to be non-empty.
///
/// An [AABB] can be obtained from an [OptionAABB], by checking for its emptiness:
/// ```rust
/// use nalgebra::Point3;
/// use lidarserv_common::geometry::bounding_box::{AABB, BaseAABB, OptionAABB};
///
/// let option_aabb = OptionAABB::new(
///     Point3::new(0.0, 0.0, 0.0),
///     Point3::new(2.3, 2.3, 2.3),
/// );
/// let aabb: AABB<f64> = match option_aabb.into_aabb() {
///     Some(a) => a,
///     None => panic!("AABB is empty"),
/// };
/// ```
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct AABB<C: Scalar> {
    inner: OptionAABB<C>,
}

impl<C: Component> BaseAABB<C> for AABB<C> {
    /// Create a new AABB from the specified bounds.
    /// Panics, if for any component the min bound is larger than the max bound.
    fn new(min: Point3<C>, max: Point3<C>) -> Self {
        assert!(min.x <= max.x);
        assert!(min.y <= max.y);
        assert!(min.z <= max.z);
        AABB {
            inner: OptionAABB::new(min, max),
        }
    }

    #[inline]
    fn contains<P: Position<Component = C>>(&self, point: &P) -> bool {
        self.inner.contains(point)
    }

    #[inline]
    fn extend<P: Position<Component = C>>(&mut self, position: &P) {
        self.inner.extend(position)
    }

    #[inline]
    fn extend_other(&mut self, other: &OptionAABB<C>) {
        self.inner.extend_other(other)
    }
}

impl<C: Component> AABB<C> {
    pub fn center<P: Position<Component = C>>(&self) -> P {
        P::from_components(
            C::center(self.inner.min.x, self.inner.max.x),
            C::center(self.inner.min.y, self.inner.max.y),
            C::center(self.inner.min.z, self.inner.max.z),
        )
    }

    pub fn min<P: Position<Component = C>>(&self) -> P {
        P::from_components(self.inner.min.x, self.inner.min.y, self.inner.min.z)
    }

    pub fn max<P: Position<Component = C>>(&self) -> P {
        P::from_components(self.inner.max.x, self.inner.max.y, self.inner.max.z)
    }

    pub fn intersects(&self, other: &Self) -> bool {
        if self.inner.max.x < other.inner.min.x {
            return false;
        }
        if self.inner.max.y < other.inner.min.y {
            return false;
        }
        if self.inner.max.z < other.inner.min.z {
            return false;
        }
        if self.inner.min.x > other.inner.max.x {
            return false;
        }
        if self.inner.min.y > other.inner.max.y {
            return false;
        }
        if self.inner.min.z > other.inner.max.z {
            return false;
        }
        true
    }

    pub fn extend_union(&mut self, other: &Self) {
        if other.inner.min.x < self.inner.min.x {
            self.inner.min.x = other.inner.min.x;
        }
        if other.inner.min.y < self.inner.min.y {
            self.inner.min.y = other.inner.min.y;
        }
        if other.inner.min.z < self.inner.min.z {
            self.inner.min.z = other.inner.min.z;
        }

        if other.inner.max.x > self.inner.max.x {
            self.inner.max.x = other.inner.max.x;
        }
        if other.inner.max.y > self.inner.max.y {
            self.inner.max.y = other.inner.max.y;
        }
        if other.inner.max.z > self.inner.max.z {
            self.inner.max.z = other.inner.max.z;
        }
    }
}

impl<C: Scalar> Debug for AABB<C> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AABB ({:?},{:?},{:?} - {:?},{:?},{:?})",
            self.inner.min.x,
            self.inner.min.y,
            self.inner.min.z,
            self.inner.max.x,
            self.inner.max.y,
            self.inner.max.z
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::geometry::bounding_box::{BaseAABB, OptionAABB};
    use crate::geometry::position::{F64Position, Position};
    use crate::geometry::test::Point;
    use crate::nalgebra::Point3;

    #[test]
    fn is_empty() {
        let aabb = OptionAABB::<f64>::empty();
        assert!(aabb.is_empty());
        assert!(aabb.into_aabb().is_none());
    }

    #[test]
    fn contains() {
        let aabb = OptionAABB::new(Point3::new(2.0, 4.0, 1.0), Point3::new(4.0, 5.0, 2.0));
        assert!(aabb.contains(&F64Position::from_components(2.0, 4.0, 1.0)));
        assert!(aabb.contains(&F64Position::from_components(3.0, 4.5, 1.5)));
        assert!(aabb.contains(&F64Position::from_components(4.0, 5.0, 2.0)));
        assert!(!aabb.contains(&F64Position::from_components(1.5, 4.5, 1.5)));
        assert!(!aabb.contains(&F64Position::from_components(4.5, 4.5, 1.5)));
        assert!(!aabb.contains(&F64Position::from_components(3.0, 3.5, 1.5)));
        assert!(!aabb.contains(&F64Position::from_components(3.0, 5.5, 1.5)));
        assert!(!aabb.contains(&F64Position::from_components(3.0, 4.5, 0.5)));
        assert!(!aabb.contains(&F64Position::from_components(3.0, 4.5, 2.5)));
    }

    #[test]
    fn extend() {
        let mut aabb = OptionAABB::empty();
        aabb.extend(&F64Position::from_components(1.0, 2.0, 3.0));
        assert_eq!(aabb.min, Point3::new(1.0, 2.0, 3.0));
        assert_eq!(aabb.max, Point3::new(1.0, 2.0, 3.0));
        aabb.extend(&F64Position::from_components(3.0, 2.0, 1.0));
        assert_eq!(aabb.min, Point3::new(1.0, 2.0, 1.0));
        assert_eq!(aabb.max, Point3::new(3.0, 2.0, 3.0));
    }

    #[test]
    fn extend_union() {
        let mut aabb = OptionAABB::new(Point3::new(2.0, 4.0, 1.0), Point3::new(4.0, 5.0, 2.0))
            .into_aabb()
            .unwrap();
        let other = OptionAABB::new(Point3::new(2.0, 2.0, 2.0), Point3::new(3.0, 5.0, 3.0))
            .into_aabb()
            .unwrap();
        aabb.extend_union(&other);
        assert_eq!(aabb.inner.min, Point3::new(2.0, 2.0, 1.0));
        assert_eq!(aabb.inner.max, Point3::new(4.0, 5.0, 3.0));
    }

    #[test]
    fn intersects() {
        let mut aabb = OptionAABB::new(Point3::new(2.0, 4.0, 1.0), Point3::new(4.0, 5.0, 2.0))
            .into_aabb()
            .unwrap();
        let mut other1 = OptionAABB::new(Point3::new(4.5, 5.5, 2.5), Point3::new(5.0, 6.0, 6.0))
            .into_aabb()
            .unwrap();
        let mut other2 = OptionAABB::new(Point3::new(4.0, 5.0, 2.0), Point3::new(5.0, 6.0, 6.0))
            .into_aabb()
            .unwrap();
        let mut other3 = OptionAABB::new(Point3::new(2.5, 4.5, 0.0), Point3::new(3.5, 6.0, 3.0))
            .into_aabb()
            .unwrap();
        assert!(!aabb.intersects(&other1));
        assert!(!other1.intersects(&aabb));
        assert!(aabb.intersects(&other2));
        assert!(other2.intersects(&aabb));
        assert!(aabb.intersects(&other3));
        assert!(other3.intersects(&aabb));
    }
}
