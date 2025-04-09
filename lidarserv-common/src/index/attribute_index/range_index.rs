use super::{IndexFunction, boolvec::BoolVec, cmp::ComponentwiseCmp};
use crate::query::NodeQueryResult;
use nalgebra::{ArrayStorage, SVector, Scalar};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

/// Contains the operations on a point attribute type needed
/// by the range index.
trait MinMax {
    /// Smallest representable value of the type.
    const MIN: Self;

    /// Largest representable value of the type.
    const MAX: Self;

    /// Calculates the component-wise minimum between self and other,
    /// and updates self accordingly.
    fn min_mut(&mut self, other: &Self);

    /// Calculates the component-wise maximum between self and other,
    /// and updates self accordingly.
    fn max_mut(&mut self, other: &Self);
}

macro_rules! impl_minmax_float {
    ($t:ty) => {
        impl MinMax for $t {
            const MIN: Self = <$t>::NEG_INFINITY;
            const MAX: Self = <$t>::INFINITY;

            #[inline]
            fn min_mut(&mut self, other: &Self) {
                *self = self.min(*other);
            }

            #[inline]
            fn max_mut(&mut self, other: &Self) {
                *self = self.max(*other);
            }
        }
    };
}

impl_minmax_float!(f32);
impl_minmax_float!(f64);

macro_rules! impl_minmax_int {
    ($t:ty) => {
        impl MinMax for $t {
            const MIN: Self = <$t>::MIN;
            const MAX: Self = <$t>::MAX;

            #[inline]
            fn min_mut(&mut self, other: &Self) {
                *self = (*self).min(*other);
            }

            #[inline]
            fn max_mut(&mut self, other: &Self) {
                *self = (*self).max(*other);
            }
        }
    };
}

impl_minmax_int!(u8);
impl_minmax_int!(i8);
impl_minmax_int!(u16);
impl_minmax_int!(i16);
impl_minmax_int!(u32);
impl_minmax_int!(i32);
impl_minmax_int!(u64);
impl_minmax_int!(i64);

impl<T, const D: usize> MinMax for SVector<T, D>
where
    T: MinMax + Scalar,
{
    const MIN: Self = SVector::from_array_storage(ArrayStorage([[T::MIN; D]; 1]));
    const MAX: Self = SVector::from_array_storage(ArrayStorage([[T::MAX; D]; 1]));

    #[inline]
    fn min_mut(&mut self, other: &Self) {
        self.zip_apply(
            other,
            #[inline]
            |l, r| l.min_mut(&r),
        );
    }

    #[inline]
    fn max_mut(&mut self, other: &Self) {
        self.zip_apply(
            other,
            #[inline]
            |l, r| l.max_mut(&r),
        );
    }
}

/// Inclusive range between min and max.
/// Describes the range of values found in some octree node and its subtree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueRange<T> {
    min: T,
    max: T,
}

impl<T> ValueRange<T> {
    pub fn new(min: T, max: T) -> Self {
        ValueRange { min, max }
    }
}

/// Attribute indexer that calculates the range of attribute values in each subtree.
pub struct RangeIndex<AttributeValue>(PhantomData<AttributeValue>);

impl<AttributeValue> Default for RangeIndex<AttributeValue> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<AttributeValue> IndexFunction for RangeIndex<AttributeValue>
where
    AttributeValue: MinMax + ComponentwiseCmp,
{
    type NodeType = ValueRange<AttributeValue>;
    type AttributeValue = AttributeValue;

    fn index(&self, attribute_values: impl Iterator<Item = AttributeValue>) -> Self::NodeType {
        let mut node = ValueRange {
            min: AttributeValue::MAX,
            max: AttributeValue::MIN,
        };
        for value in attribute_values {
            node.min.min_mut(&value);
            node.max.max_mut(&value);
        }
        node
    }

    fn merge(&self, node1: &mut Self::NodeType, node2: Self::NodeType) {
        node1.min.min_mut(&node2.min);
        node1.max.max_mut(&node2.max);
    }

    fn test_eq(&self, node: &Self::NodeType, op: &Self::AttributeValue) -> NodeQueryResult {
        if node.min.is_greater(op).or(&node.max.is_less(op)).any() {
            return NodeQueryResult::Negative;
        }
        if node.min.is_eq(op).all() && node.max.is_eq(op).all() {
            return NodeQueryResult::Positive;
        }
        NodeQueryResult::Partial
    }

    fn test_neq(&self, node: &Self::NodeType, op: &Self::AttributeValue) -> NodeQueryResult {
        if node.min.is_eq(op).and(&node.max.is_eq(op)).any() {
            return NodeQueryResult::Negative;
        }
        if node.min.is_greater(op).or(&node.max.is_less(op)).all() {
            return NodeQueryResult::Positive;
        }
        NodeQueryResult::Partial
    }

    fn test_less(&self, node: &Self::NodeType, op: &Self::AttributeValue) -> NodeQueryResult {
        if node.min.is_greater_eq(op).any() {
            NodeQueryResult::Negative
        } else if node.max.is_less(op).all() {
            NodeQueryResult::Positive
        } else {
            NodeQueryResult::Partial
        }
    }

    fn test_less_eq(&self, node: &Self::NodeType, op: &Self::AttributeValue) -> NodeQueryResult {
        if node.min.is_greater(op).any() {
            NodeQueryResult::Negative
        } else if node.max.is_less_eq(op).all() {
            NodeQueryResult::Positive
        } else {
            NodeQueryResult::Partial
        }
    }

    fn test_greater(&self, node: &Self::NodeType, op: &Self::AttributeValue) -> NodeQueryResult {
        if node.max.is_less_eq(op).any() {
            NodeQueryResult::Negative
        } else if node.min.is_greater(op).all() {
            NodeQueryResult::Positive
        } else {
            NodeQueryResult::Partial
        }
    }

    fn test_greater_eq(&self, node: &Self::NodeType, op: &Self::AttributeValue) -> NodeQueryResult {
        if node.max.is_less(op).any() {
            NodeQueryResult::Negative
        } else if node.min.is_greater_eq(op).all() {
            NodeQueryResult::Positive
        } else {
            NodeQueryResult::Partial
        }
    }
}

#[cfg(test)]
mod tests {
    use nalgebra::{Vector3, vector};

    use crate::{
        index::attribute_index::{IndexFunction, range_index::ValueRange},
        query::NodeQueryResult,
    };

    use super::RangeIndex;

    #[test]
    fn test_range_queries_scalar_eq() {
        let idx = RangeIndex::<i32>::default();
        assert_eq!(
            idx.test_eq(&ValueRange::<i32>::new(1, 3), &5),
            NodeQueryResult::Negative
        );
        assert_eq!(
            idx.test_eq(&ValueRange::<i32>::new(1, 5), &5),
            NodeQueryResult::Partial
        );
        assert_eq!(
            idx.test_eq(&ValueRange::<i32>::new(1, 8), &5),
            NodeQueryResult::Partial
        );
        assert_eq!(
            idx.test_eq(&ValueRange::<i32>::new(5, 8), &5),
            NodeQueryResult::Partial
        );
        assert_eq!(
            idx.test_eq(&ValueRange::<i32>::new(6, 8), &5),
            NodeQueryResult::Negative
        );
        assert_eq!(
            idx.test_eq(&ValueRange::<i32>::new(5, 5), &5),
            NodeQueryResult::Positive
        );
    }

    #[test]
    fn test_range_queries_scalar_neq() {
        let idx = RangeIndex::<i32>::default();
        assert_eq!(
            idx.test_neq(&ValueRange::<i32>::new(1, 3), &5),
            NodeQueryResult::Positive
        );
        assert_eq!(
            idx.test_neq(&ValueRange::<i32>::new(1, 5), &5),
            NodeQueryResult::Partial
        );
        assert_eq!(
            idx.test_neq(&ValueRange::<i32>::new(1, 8), &5),
            NodeQueryResult::Partial
        );
        assert_eq!(
            idx.test_neq(&ValueRange::<i32>::new(5, 8), &5),
            NodeQueryResult::Partial
        );
        assert_eq!(
            idx.test_neq(&ValueRange::<i32>::new(6, 8), &5),
            NodeQueryResult::Positive
        );
        assert_eq!(
            idx.test_neq(&ValueRange::<i32>::new(5, 5), &5),
            NodeQueryResult::Negative
        );
    }

    #[test]
    fn test_range_queries_scalar_less() {
        let idx = RangeIndex::<i32>::default();
        assert_eq!(
            idx.test_less(&ValueRange::<i32>::new(1, 3), &5),
            NodeQueryResult::Positive
        );
        assert_eq!(
            idx.test_less(&ValueRange::<i32>::new(1, 5), &5),
            NodeQueryResult::Partial
        );
        assert_eq!(
            idx.test_less(&ValueRange::<i32>::new(1, 8), &5),
            NodeQueryResult::Partial
        );
        assert_eq!(
            idx.test_less(&ValueRange::<i32>::new(5, 8), &5),
            NodeQueryResult::Negative
        );
        assert_eq!(
            idx.test_less(&ValueRange::<i32>::new(6, 8), &5),
            NodeQueryResult::Negative
        );
        assert_eq!(
            idx.test_less(&ValueRange::<i32>::new(5, 5), &5),
            NodeQueryResult::Negative
        );
    }

    #[test]
    fn test_range_queries_scalar_lesseq() {
        let idx = RangeIndex::<i32>::default();
        assert_eq!(
            idx.test_less_eq(&ValueRange::<i32>::new(1, 3), &5),
            NodeQueryResult::Positive
        );
        assert_eq!(
            idx.test_less_eq(&ValueRange::<i32>::new(1, 5), &5),
            NodeQueryResult::Positive
        );
        assert_eq!(
            idx.test_less_eq(&ValueRange::<i32>::new(1, 8), &5),
            NodeQueryResult::Partial
        );
        assert_eq!(
            idx.test_less_eq(&ValueRange::<i32>::new(5, 8), &5),
            NodeQueryResult::Partial
        );
        assert_eq!(
            idx.test_less_eq(&ValueRange::<i32>::new(6, 8), &5),
            NodeQueryResult::Negative
        );
        assert_eq!(
            idx.test_less_eq(&ValueRange::<i32>::new(5, 5), &5),
            NodeQueryResult::Positive
        );
    }

    #[test]
    fn test_range_queries_scalar_greater() {
        let idx = RangeIndex::<i32>::default();
        assert_eq!(
            idx.test_greater(&ValueRange::<i32>::new(1, 3), &5),
            NodeQueryResult::Negative
        );
        assert_eq!(
            idx.test_greater(&ValueRange::<i32>::new(1, 5), &5),
            NodeQueryResult::Negative
        );
        assert_eq!(
            idx.test_greater(&ValueRange::<i32>::new(1, 8), &5),
            NodeQueryResult::Partial
        );
        assert_eq!(
            idx.test_greater(&ValueRange::<i32>::new(5, 8), &5),
            NodeQueryResult::Partial
        );
        assert_eq!(
            idx.test_greater(&ValueRange::<i32>::new(6, 8), &5),
            NodeQueryResult::Positive
        );
        assert_eq!(
            idx.test_greater(&ValueRange::<i32>::new(5, 5), &5),
            NodeQueryResult::Negative
        );
    }

    #[test]
    fn test_range_queries_scalar_greatereq() {
        let idx = RangeIndex::<i32>::default();
        assert_eq!(
            idx.test_greater_eq(&ValueRange::<i32>::new(1, 3), &5),
            NodeQueryResult::Negative
        );
        assert_eq!(
            idx.test_greater_eq(&ValueRange::<i32>::new(1, 5), &5),
            NodeQueryResult::Partial
        );
        assert_eq!(
            idx.test_greater_eq(&ValueRange::<i32>::new(1, 8), &5),
            NodeQueryResult::Partial
        );
        assert_eq!(
            idx.test_greater_eq(&ValueRange::<i32>::new(5, 8), &5),
            NodeQueryResult::Positive
        );
        assert_eq!(
            idx.test_greater_eq(&ValueRange::<i32>::new(6, 8), &5),
            NodeQueryResult::Positive
        );
        assert_eq!(
            idx.test_greater_eq(&ValueRange::<i32>::new(5, 5), &5),
            NodeQueryResult::Positive
        );
    }

    #[test]
    fn test_range_queries_vec_test_neq() {
        let idx = RangeIndex::<Vector3<i32>>::default();

        assert_eq!(
            idx.test_neq(
                &ValueRange::new(vector![1, 7, 1], vector![2, 8, 2]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Positive
        );

        assert_eq!(
            idx.test_neq(
                &ValueRange::new(vector![5, 5, 5], vector![5, 5, 5]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_neq(
                &ValueRange::new(vector![1, 1, 5], vector![5, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Partial
        );

        assert_eq!(
            idx.test_neq(
                &ValueRange::new(vector![7, 5, 5], vector![8, 5, 5]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_neq(
                &ValueRange::new(vector![5, 1, 1], vector![5, 5, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_neq(
                &ValueRange::new(vector![5, 1, 7], vector![8, 2, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Partial
        );

        assert_eq!(
            idx.test_neq(
                &ValueRange::new(vector![7, 5, 5], vector![8, 5, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );
    }

    #[test]
    fn test_range_queries_vec_test_eq() {
        let idx = RangeIndex::<Vector3<i32>>::default();

        assert_eq!(
            idx.test_eq(
                &ValueRange::new(vector![5, 5, 5], vector![5, 5, 5]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Positive
        );

        assert_eq!(
            idx.test_eq(
                &ValueRange::new(vector![1, 7, 7], vector![2, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_eq(
                &ValueRange::new(vector![1, 1, 5], vector![5, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Partial
        );

        assert_eq!(
            idx.test_eq(
                &ValueRange::new(vector![5, 1, 7], vector![5, 2, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_eq(
                &ValueRange::new(vector![1, 1, 1], vector![2, 5, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_eq(
                &ValueRange::new(vector![5, 5, 5], vector![8, 5, 5]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Partial
        );

        assert_eq!(
            idx.test_eq(
                &ValueRange::new(vector![5, 7, 5], vector![5, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );
    }

    #[test]
    fn test_range_queries_vec_test_greater_eq() {
        let idx = RangeIndex::<Vector3<i32>>::default();

        assert_eq!(
            idx.test_greater_eq(
                &ValueRange::new(vector![5, 7, 5], vector![8, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Positive
        );

        assert_eq!(
            idx.test_greater_eq(
                &ValueRange::new(vector![1, 1, 1], vector![2, 2, 2]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_greater_eq(
                &ValueRange::new(vector![1, 1, 5], vector![5, 8, 5]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Partial
        );

        assert_eq!(
            idx.test_greater_eq(
                &ValueRange::new(vector![7, 1, 1], vector![8, 2, 2]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_greater_eq(
                &ValueRange::new(vector![1, 1, 1], vector![2, 5, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_greater_eq(
                &ValueRange::new(vector![1, 5, 7], vector![5, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Partial
        );

        assert_eq!(
            idx.test_greater_eq(
                &ValueRange::new(vector![7, 1, 5], vector![8, 2, 5]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );
    }

    #[test]
    fn test_range_queries_vec_test_greater() {
        let idx = RangeIndex::<Vector3<i32>>::default();

        assert_eq!(
            idx.test_greater(
                &ValueRange::new(vector![7, 7, 7], vector![8, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Positive
        );

        assert_eq!(
            idx.test_greater(
                &ValueRange::new(vector![1, 1, 5], vector![2, 5, 5]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_greater(
                &ValueRange::new(vector![1, 5, 5], vector![8, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Partial
        );

        assert_eq!(
            idx.test_greater(
                &ValueRange::new(vector![7, 1, 1], vector![8, 2, 5]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_greater(
                &ValueRange::new(vector![5, 1, 1], vector![5, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_greater(
                &ValueRange::new(vector![5, 7, 7], vector![8, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Partial
        );

        assert_eq!(
            idx.test_greater(
                &ValueRange::new(vector![7, 5, 5], vector![8, 5, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );
    }

    #[test]
    fn test_range_queries_vec_test_less() {
        let idx = RangeIndex::<Vector3<i32>>::default();

        assert_eq!(
            idx.test_less(
                &ValueRange::new(vector![1, 1, 1], vector![2, 2, 2]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Positive
        );

        assert_eq!(
            idx.test_less(
                &ValueRange::new(vector![5, 5, 7], vector![5, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_less(
                &ValueRange::new(vector![1, 1, 1], vector![5, 8, 5]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Partial
        );

        assert_eq!(
            idx.test_less(
                &ValueRange::new(vector![1, 5, 5], vector![2, 5, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_less(
                &ValueRange::new(vector![7, 1, 1], vector![8, 5, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_less(
                &ValueRange::new(vector![1, 1, 1], vector![5, 2, 2]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Partial
        );

        assert_eq!(
            idx.test_less(
                &ValueRange::new(vector![1, 5, 1], vector![2, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );
    }

    #[test]
    fn test_range_queries_vec_test_less_eq() {
        let idx = RangeIndex::<Vector3<i32>>::default();

        assert_eq!(
            idx.test_less_eq(
                &ValueRange::new(vector![1, 1, 1], vector![2, 5, 2]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Positive
        );

        assert_eq!(
            idx.test_less_eq(
                &ValueRange::new(vector![7, 7, 7], vector![8, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_less_eq(
                &ValueRange::new(vector![1, 5, 5], vector![8, 5, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Partial
        );

        assert_eq!(
            idx.test_less_eq(
                &ValueRange::new(vector![1, 7, 7], vector![5, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_less_eq(
                &ValueRange::new(vector![7, 1, 5], vector![8, 8, 5]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );

        assert_eq!(
            idx.test_less_eq(
                &ValueRange::new(vector![5, 1, 1], vector![8, 2, 5]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Partial
        );

        assert_eq!(
            idx.test_less_eq(
                &ValueRange::new(vector![1, 7, 5], vector![5, 8, 8]),
                &vector![5, 5, 5]
            ),
            NodeQueryResult::Negative
        );
    }
}
