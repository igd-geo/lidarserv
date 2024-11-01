use super::boolvec::BoolVec;
use nalgebra::{SVector, Scalar};

pub trait ComponentwiseCmp {
    /// Result type for the (component-wise) comparison operations below.
    /// For scalar buckets, this will always be bool.
    /// For vector buckets, this will be some VectorN<bool>.
    type Bool: BoolVec;

    /// Component-wise equality check.
    fn is_eq(&self, other: &Self) -> Self::Bool;

    /// Component-wise test if self is smaller then other.
    fn is_less(&self, other: &Self) -> Self::Bool;

    /// Component-wise test if self is smaller or equal then other.
    fn is_less_eq(&self, other: &Self) -> Self::Bool;

    /// Component-wise test if self is larger then other.
    fn is_greater(&self, other: &Self) -> Self::Bool;

    /// Component-wise test if self is larger or equal then other.
    fn is_greater_eq(&self, other: &Self) -> Self::Bool;
}

macro_rules! impl_componentwise_cmp_scalar {
    ($t:ty) => {
        impl ComponentwiseCmp for $t {
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

impl_componentwise_cmp_scalar!(u8);
impl_componentwise_cmp_scalar!(u16);
impl_componentwise_cmp_scalar!(u32);
impl_componentwise_cmp_scalar!(u64);
impl_componentwise_cmp_scalar!(i8);
impl_componentwise_cmp_scalar!(i16);
impl_componentwise_cmp_scalar!(i32);
impl_componentwise_cmp_scalar!(i64);
impl_componentwise_cmp_scalar!(f32);
impl_componentwise_cmp_scalar!(f64);

impl<T, const D: usize> ComponentwiseCmp for SVector<T, D>
where
    T: Scalar + ComponentwiseCmp<Bool = bool>,
{
    type Bool = SVector<bool, D>;

    #[inline]
    fn is_eq(&self, other: &Self) -> Self::Bool {
        self.zip_map(
            other,
            #[inline]
            |l, r| l.is_eq(&r),
        )
    }

    #[inline]
    fn is_less(&self, other: &Self) -> Self::Bool {
        self.zip_map(
            other,
            #[inline]
            |l, r| l.is_less(&r),
        )
    }

    #[inline]
    fn is_less_eq(&self, other: &Self) -> Self::Bool {
        self.zip_map(
            other,
            #[inline]
            |l, r| l.is_less_eq(&r),
        )
    }

    #[inline]
    fn is_greater(&self, other: &Self) -> Self::Bool {
        self.zip_map(
            other,
            #[inline]
            |l, r| l.is_greater(&r),
        )
    }

    #[inline]
    fn is_greater_eq(&self, other: &Self) -> Self::Bool {
        self.zip_map(
            other,
            #[inline]
            |l, r| l.is_greater_eq(&r),
        )
    }
}
