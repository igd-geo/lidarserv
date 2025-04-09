use super::position::{Component, Position};
use nalgebra::{Vector3, point};
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};

/// An axis aligned bounding box.
/// An Aabb can also be empty. An empty aabb is commonly represented by
/// setting the minimum zo C::MAX and the maximum to C::MIN.
#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(bound = "")] // Serialize and Deserialize traits are already implied by Component. So no additional bound is required.
pub struct Aabb<C: Component> {
    pub min: Position<C>,
    pub max: Position<C>,
}

impl<C: Component> Aabb<C> {
    /// Constructs an empty bounding box.
    pub fn empty() -> Self {
        Aabb {
            min: point![C::MAX, C::MAX, C::MAX],
            max: point![C::MIN, C::MIN, C::MIN],
        }
    }

    /// Construct a new AABB with the given bounds.
    pub fn new(min: Position<C>, max: Position<C>) -> Self {
        Aabb { min, max }
    }

    /// Checks, if the bounding box is empty.
    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x || self.min.y > self.max.y || self.min.z > self.max.z
    }

    /// Check, if the given point is within the bounds.
    pub fn contains(&self, point: Position<C>) -> bool {
        self.min.x <= point.x
            && self.min.y <= point.y
            && self.min.z <= point.z
            && self.max.x >= point.x
            && self.max.y >= point.y
            && self.max.z >= point.z
    }

    /// Grow the bounding box, so that it contains the given position.
    pub fn extend(&mut self, position: Position<C>) {
        if self.min.x > position.x {
            self.min.x = position.x;
        }
        if self.min.y > position.y {
            self.min.y = position.y;
        }
        if self.min.z > position.z {
            self.min.z = position.z;
        }
        if self.max.x < position.x {
            self.max.x = position.x;
        }
        if self.max.y < position.y {
            self.max.y = position.y;
        }
        if self.max.z < position.z {
            self.max.z = position.z;
        }
    }

    /// Grow the bounding box, so that it contains the other aabb.
    pub fn extend_aabb(&mut self, other: &Self) {
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
            self.max.x = other.max.x;
        }
        if other.max.y > self.max.y {
            self.max.y = other.max.y;
        }
        if other.max.z > self.max.z {
            self.max.z = other.max.z;
        }
    }

    /// returns the center of the bounding box, or None if the bounding box is empty.
    pub fn center(&self) -> Option<Position<C>> {
        if self.is_empty() {
            None
        } else {
            Some(Vector3::from_fn(|i, _| self.min[i].centre(self.max[i])).into())
        }
    }

    /// Checks if the two bounding boxes overlap.
    pub fn intersects_aabb(&self, other: Self) -> bool {
        if self.max.x < other.min.x {
            return false;
        }
        if self.max.y < other.min.y {
            return false;
        }
        if self.max.z < other.min.z {
            return false;
        }
        if self.min.x > other.max.x {
            return false;
        }
        if self.min.y > other.max.y {
            return false;
        }
        if self.min.z > other.max.z {
            return false;
        }
        if self.is_empty() {
            return false;
        }
        if other.is_empty() {
            return false;
        }
        true
    }

    /// Checks if the `other` bounding box is fully inside this bounding box.
    pub fn contains_aabb(&self, other: Self) -> bool {
        if other.min.x < self.min.x {
            return false;
        }
        if other.min.y < self.min.y {
            return false;
        }
        if other.min.z < self.min.z {
            return false;
        }
        if other.max.x > self.max.x {
            return false;
        }
        if other.max.y > self.max.y {
            return false;
        }
        if other.max.z > self.max.z {
            return false;
        }
        true
    }
}

impl<C: Component> Default for Aabb<C> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<C: Component> Debug for Aabb<C> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            write!(f, "Aabb (empty)")
        } else {
            write!(
                f,
                "Aabb ({:?},{:?},{:?} - {:?},{:?},{:?})",
                self.min.x, self.min.y, self.min.z, self.max.x, self.max.y, self.max.z
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use nalgebra::point;

    use crate::geometry::bounding_box::Aabb;

    #[test]
    fn is_empty() {
        let aabb = Aabb::<f64>::empty();
        assert!(aabb.is_empty());

        let aabb = Aabb::new(point![2.0, 4.0, 1.0], point![4.0, 5.0, 2.0]);
        assert!(!aabb.is_empty());
    }

    #[test]
    fn contains() {
        let aabb = Aabb::new(point![2.0, 4.0, 1.0], point![4.0, 5.0, 2.0]);
        assert!(aabb.contains(point![2.0, 4.0, 1.0]));
        assert!(aabb.contains(point![3.0, 4.5, 1.5]));
        assert!(aabb.contains(point![4.0, 5.0, 2.0]));
        assert!(!aabb.contains(point![1.5, 4.5, 1.5]));
        assert!(!aabb.contains(point![4.5, 4.5, 1.5]));
        assert!(!aabb.contains(point![3.0, 3.5, 1.5]));
        assert!(!aabb.contains(point![3.0, 5.5, 1.5]));
        assert!(!aabb.contains(point![3.0, 4.5, 0.5]));
        assert!(!aabb.contains(point![3.0, 4.5, 2.5]));
    }

    #[test]
    fn extend() {
        let mut aabb = Aabb::empty();
        aabb.extend(point![1.0, 2.0, 3.0]);
        assert_eq!(aabb.min, point![1.0, 2.0, 3.0]);
        assert_eq!(aabb.max, point![1.0, 2.0, 3.0]);
        aabb.extend(point![3.0, 2.0, 1.0]);
        assert_eq!(aabb.min, point![1.0, 2.0, 1.0]);
        assert_eq!(aabb.max, point![3.0, 2.0, 3.0]);
    }

    #[test]
    fn extend_union() {
        let mut aabb = Aabb::new(point![2.0, 4.0, 1.0], point![4.0, 5.0, 2.0]);
        let other = Aabb::new(point![2.0, 2.0, 2.0], point![3.0, 5.0, 3.0]);
        aabb.extend_aabb(&other);
        assert_eq!(aabb.min, point![2.0, 2.0, 1.0]);
        assert_eq!(aabb.max, point![4.0, 5.0, 3.0]);
    }

    #[test]
    fn intersects() {
        let aabb = Aabb::new(point![2.0, 4.0, 1.0], point![4.0, 5.0, 2.0]);
        let other1 = Aabb::new(point![4.5, 5.5, 2.5], point![5.0, 6.0, 6.0]);
        let other2 = Aabb::new(point![4.0, 5.0, 2.0], point![5.0, 6.0, 6.0]);
        let other3 = Aabb::new(point![2.5, 4.5, 0.0], point![3.5, 6.0, 3.0]);
        assert!(!aabb.intersects_aabb(other1));
        assert!(!other1.intersects_aabb(aabb));
        assert!(aabb.intersects_aabb(other2));
        assert!(other2.intersects_aabb(aabb));
        assert!(aabb.intersects_aabb(other3));
        assert!(other3.intersects_aabb(aabb));
    }

    #[test]
    fn contains_other() {
        let aabb = Aabb::new(point![2.0, 4.0, 1.0], point![4.0, 5.0, 2.0]);
        let other1 = Aabb::new(point![2.5, 4.5, 1.5], point![2.9, 4.9, 1.9]);
        let other2 = Aabb::new(point![1.5, 4.5, 1.5], point![2.9, 4.9, 1.9]);
        let other3 = Aabb::new(point![5.0, 1.0, 3.0], point![6.0, 2.0, 4.0]);
        assert!(aabb.contains_aabb(other1));
        assert!(!aabb.contains_aabb(other2));
        assert!(!aabb.contains_aabb(other3));
    }

    #[test]
    fn intersects_special_cases() {
        let empty = Aabb::empty();
        let full = Aabb::new(
            point![i32::MIN, i32::MIN, i32::MIN],
            point![i32::MAX, i32::MAX, i32::MAX],
        );
        let normal = Aabb::new(point![100, 200, 300], point![700, 800, 900]);

        assert!(!empty.intersects_aabb(empty));
        assert!(!empty.intersects_aabb(full));
        assert!(!empty.intersects_aabb(normal));
        assert!(!full.intersects_aabb(empty));
        assert!(full.intersects_aabb(full));
        assert!(full.intersects_aabb(normal));
        assert!(!normal.intersects_aabb(empty));
        assert!(normal.intersects_aabb(full));
        assert!(normal.intersects_aabb(normal));
    }

    #[test]
    fn contains_special_cases() {
        let empty = Aabb::empty();
        let full = Aabb::new(
            point![i32::MIN, i32::MIN, i32::MIN],
            point![i32::MAX, i32::MAX, i32::MAX],
        );
        let normal = Aabb::new(point![100, 200, 300], point![700, 800, 900]);

        assert!(empty.contains_aabb(empty));
        assert!(!empty.contains_aabb(full));
        assert!(!empty.contains_aabb(normal));
        assert!(full.contains_aabb(empty));
        assert!(full.contains_aabb(full));
        assert!(full.contains_aabb(normal));
        assert!(normal.contains_aabb(empty));
        assert!(!normal.contains_aabb(full));
        assert!(normal.contains_aabb(normal));
    }

    #[test]
    fn centre() {
        let aabb = Aabb::new(point![2.0, 4.0, 1.0], point![4.0, 5.0, 2.0]);
        assert_eq!(aabb.center(), Some(point![3.0, 4.5, 1.5]));
        assert_eq!(Aabb::<f64>::empty().center(), None);
    }
}
