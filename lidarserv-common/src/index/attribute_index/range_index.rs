use std::marker::PhantomData;

use super::IndexFunction;
use crate::query::{attribute::TestFunction, NodeQueryResult};
use nalgebra::{vector, Scalar, Vector3, Vector4};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueRange<T> {
    min: T,
    max: T,
}

trait BoolVec {
    fn any(&self) -> bool;
    fn all(&self) -> bool;
    fn and(&self, other: &Self) -> Self;
    fn or(&self, other: &Self) -> Self;
    fn not(&self) -> Self;
}

impl BoolVec for bool {
    fn any(&self) -> bool {
        *self
    }

    fn all(&self) -> bool {
        *self
    }

    fn and(&self, other: &Self) -> Self {
        *self && *other
    }

    fn or(&self, other: &Self) -> Self {
        *self || *other
    }

    fn not(&self) -> Self {
        !*self
    }
}

impl BoolVec for Vector3<bool> {
    fn any(&self) -> bool {
        self.x || self.y || self.z
    }

    fn all(&self) -> bool {
        self.x && self.y && self.z
    }

    fn and(&self, other: &Self) -> Self {
        vector![self.x && other.x, self.y && other.y, self.z && other.z,]
    }

    fn or(&self, other: &Self) -> Self {
        vector![self.x || other.x, self.y || other.y, self.z || other.z,]
    }

    fn not(&self) -> Self {
        vector![!self.x, !self.y, !self.z,]
    }
}

impl BoolVec for Vector4<bool> {
    fn any(&self) -> bool {
        self.x || self.y || self.z || self.w
    }

    fn all(&self) -> bool {
        self.x && self.y && self.z && self.w
    }

    fn and(&self, other: &Self) -> Self {
        vector![
            self.x && other.x,
            self.y && other.y,
            self.z && other.z,
            self.w && other.w
        ]
    }

    fn or(&self, other: &Self) -> Self {
        vector![
            self.x || other.x,
            self.y || other.y,
            self.z || other.z,
            self.w || other.w
        ]
    }

    fn not(&self) -> Self {
        vector![!self.x, !self.y, !self.z, !self.w]
    }
}

trait MinMax {
    const MIN: Self;
    const MAX: Self;

    fn min_mut(&mut self, other: &Self);
    fn max_mut(&mut self, other: &Self);

    type Bool: BoolVec;
    fn is_eq(&self, other: &Self) -> Self::Bool;
    fn is_less(&self, other: &Self) -> Self::Bool;
    fn is_less_eq(&self, other: &Self) -> Self::Bool;
    fn is_greater(&self, other: &Self) -> Self::Bool;
    fn is_greater_eq(&self, other: &Self) -> Self::Bool;
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

            type Bool = bool;

            #[inline]
            fn is_eq(&self, other: &Self) -> Self::Bool {
                *self == *other
            }

            #[inline]
            fn is_less(&self, other: &Self) -> Self::Bool {
                *self < *other
            }

            #[inline]
            fn is_less_eq(&self, other: &Self) -> Self::Bool {
                *self <= *other
            }

            #[inline]
            fn is_greater(&self, other: &Self) -> Self::Bool {
                *self > *other
            }

            #[inline]
            fn is_greater_eq(&self, other: &Self) -> Self::Bool {
                *self >= *other
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

            type Bool = bool;

            #[inline]
            fn is_eq(&self, other: &Self) -> Self::Bool {
                *self == *other
            }

            #[inline]
            fn is_less(&self, other: &Self) -> Self::Bool {
                *self < *other
            }

            #[inline]
            fn is_less_eq(&self, other: &Self) -> Self::Bool {
                *self <= *other
            }

            #[inline]
            fn is_greater(&self, other: &Self) -> Self::Bool {
                *self > *other
            }

            #[inline]
            fn is_greater_eq(&self, other: &Self) -> Self::Bool {
                *self >= *other
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

impl<T> MinMax for Vector3<T>
where
    T: MinMax<Bool = bool> + Scalar,
{
    const MIN: Self = vector![T::MIN, T::MIN, T::MIN];
    const MAX: Self = vector![T::MAX, T::MAX, T::MAX];

    #[inline]
    fn min_mut(&mut self, other: &Self) {
        self.x.min_mut(&other.x);
        self.y.min_mut(&other.y);
        self.z.min_mut(&other.z);
    }

    #[inline]
    fn max_mut(&mut self, other: &Self) {
        self.x.max_mut(&other.x);
        self.y.max_mut(&other.y);
        self.z.max_mut(&other.z);
    }

    type Bool = Vector3<bool>;

    #[inline]
    fn is_eq(&self, other: &Self) -> Self::Bool {
        vector![
            self.x.is_eq(&other.x),
            self.y.is_eq(&other.y),
            self.z.is_eq(&other.z),
        ]
    }

    #[inline]
    fn is_less(&self, other: &Self) -> Self::Bool {
        vector![
            self.x.is_less(&other.x),
            self.y.is_less(&other.y),
            self.z.is_less(&other.z),
        ]
    }

    #[inline]
    fn is_less_eq(&self, other: &Self) -> Self::Bool {
        vector![
            self.x.is_less_eq(&other.x),
            self.y.is_less_eq(&other.y),
            self.z.is_less_eq(&other.z),
        ]
    }

    #[inline]
    fn is_greater(&self, other: &Self) -> Self::Bool {
        vector![
            self.x.is_greater(&other.x),
            self.y.is_greater(&other.y),
            self.z.is_greater(&other.z),
        ]
    }

    #[inline]
    fn is_greater_eq(&self, other: &Self) -> Self::Bool {
        vector![
            self.x.is_greater_eq(&other.x),
            self.y.is_greater_eq(&other.y),
            self.z.is_greater_eq(&other.z),
        ]
    }
}

impl<T> MinMax for Vector4<T>
where
    T: MinMax<Bool = bool> + Scalar,
{
    const MIN: Self = vector![T::MIN, T::MIN, T::MIN, T::MIN];
    const MAX: Self = vector![T::MAX, T::MAX, T::MAX, T::MAX];

    #[inline]
    fn min_mut(&mut self, other: &Self) {
        self.x.min_mut(&other.x);
        self.y.min_mut(&other.y);
        self.z.min_mut(&other.z);
        self.w.min_mut(&other.w);
    }

    #[inline]
    fn max_mut(&mut self, other: &Self) {
        self.x.max_mut(&other.x);
        self.y.max_mut(&other.y);
        self.z.max_mut(&other.z);
        self.w.max_mut(&other.w);
    }

    type Bool = Vector4<bool>;

    #[inline]
    fn is_eq(&self, other: &Self) -> Self::Bool {
        vector![
            self.x.is_eq(&other.x),
            self.y.is_eq(&other.y),
            self.z.is_eq(&other.z),
            self.w.is_eq(&other.w),
        ]
    }

    #[inline]
    fn is_less(&self, other: &Self) -> Self::Bool {
        vector![
            self.x.is_less(&other.x),
            self.y.is_less(&other.y),
            self.z.is_less(&other.z),
            self.w.is_less(&other.w),
        ]
    }

    #[inline]
    fn is_less_eq(&self, other: &Self) -> Self::Bool {
        vector![
            self.x.is_less_eq(&other.x),
            self.y.is_less_eq(&other.y),
            self.z.is_less_eq(&other.z),
            self.w.is_less_eq(&other.w),
        ]
    }

    #[inline]
    fn is_greater(&self, other: &Self) -> Self::Bool {
        vector![
            self.x.is_greater(&other.x),
            self.y.is_greater(&other.y),
            self.z.is_greater(&other.z),
            self.w.is_greater(&other.w),
        ]
    }

    #[inline]
    fn is_greater_eq(&self, other: &Self) -> Self::Bool {
        vector![
            self.x.is_greater_eq(&other.x),
            self.y.is_greater_eq(&other.y),
            self.z.is_greater_eq(&other.z),
            self.w.is_greater_eq(&other.w),
        ]
    }
}

pub struct RangeIndex<AttributeValue>(PhantomData<AttributeValue>);

impl<AttributeValue> Default for RangeIndex<AttributeValue> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<AttributeValue> IndexFunction for RangeIndex<AttributeValue>
where
    AttributeValue: MinMax,
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

    fn test(&self, node: &Self::NodeType, test: &TestFunction<AttributeValue>) -> NodeQueryResult {
        // todo unit tests!!!!!!
        match test {
            TestFunction::Eq(op) => {
                if node.min.is_less_eq(op).all() && node.max.is_greater_eq(op).all() {
                    if node.min.is_eq(op).all() && node.max.is_eq(op).all() {
                        NodeQueryResult::Positive
                    } else {
                        NodeQueryResult::Partial
                    }
                } else {
                    NodeQueryResult::Negative
                }
            }
            TestFunction::Neq(op) => {
                // todo i dont think this is correct...
                if node.min.is_less_eq(op).all() && node.max.is_greater_eq(op).all() {
                    if node.min.is_eq(op).all() && node.max.is_eq(op).all() {
                        NodeQueryResult::Negative
                    } else {
                        NodeQueryResult::Partial
                    }
                } else {
                    NodeQueryResult::Positive
                }
            }

            TestFunction::Less(op) => {
                if node.min.is_greater_eq(op).any() {
                    NodeQueryResult::Negative
                } else if node.max.is_less(op).all() {
                    NodeQueryResult::Positive
                } else {
                    NodeQueryResult::Partial
                }
            }
            TestFunction::LessEq(op) => {
                if node.min.is_greater(op).any() {
                    NodeQueryResult::Negative
                } else if node.max.is_less_eq(op).all() {
                    NodeQueryResult::Positive
                } else {
                    NodeQueryResult::Partial
                }
            }
            TestFunction::Greater(op) => {
                if node.max.is_less_eq(op).any() {
                    NodeQueryResult::Negative
                } else if node.min.is_greater(op).all() {
                    NodeQueryResult::Positive
                } else {
                    NodeQueryResult::Partial
                }
            }
            TestFunction::GreaterEq(op) => {
                if node.max.is_less(op).any() {
                    NodeQueryResult::Negative
                } else if node.min.is_greater_eq(op).all() {
                    NodeQueryResult::Positive
                } else {
                    NodeQueryResult::Partial
                }
            }
            TestFunction::RangeExclusive(_, _) => todo!(),
            TestFunction::RangeLeftInclusive(_, _) => todo!(),
            TestFunction::RangeRightInclusive(_, _) => todo!(),
            TestFunction::RangeInclusive(_, _) => todo!(),
        }
    }
}
