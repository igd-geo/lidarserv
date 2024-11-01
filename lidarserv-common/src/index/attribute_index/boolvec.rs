use nalgebra::SVector;

/// A fixed-size (usually: 1, 3 or 4) vector of boolean values.
/// Used to assert a component-wise condition on an point attribute value
/// (in case of non-scalar attributes like color, or normals)
pub trait BoolVec {
    /// Component-wise logical and
    fn and(&self, other: &Self) -> Self;

    /// Component-wise logical or
    fn or(&self, other: &Self) -> Self;

    /// Component-wise not
    fn not(&self) -> Self;

    /// Returns true, if at least one component is true.
    fn any(&self) -> bool;

    /// Returns true, if all components are true.
    fn all(&self) -> bool;
}

impl BoolVec for bool {
    #[inline]
    fn any(&self) -> bool {
        *self
    }

    #[inline]
    fn all(&self) -> bool {
        *self
    }

    #[inline]
    fn not(&self) -> Self {
        !self
    }

    #[inline]
    fn and(&self, other: &Self) -> Self {
        *self && *other
    }

    #[inline]
    fn or(&self, other: &Self) -> Self {
        *self || *other
    }
}

impl<const D: usize> BoolVec for SVector<bool, D> {
    #[inline]
    fn any(&self) -> bool {
        self.fold(
            false,
            #[inline]
            |a, b| a || b,
        )
    }

    #[inline]
    fn all(&self) -> bool {
        self.fold(
            true,
            #[inline]
            |a, b| a && b,
        )
    }

    #[inline]
    fn and(&self, other: &Self) -> Self {
        self.zip_map(
            other,
            #[inline]
            |l, r| l && r,
        )
    }

    #[inline]
    fn or(&self, other: &Self) -> Self {
        self.zip_map(
            other,
            #[inline]
            |l, r| l || r,
        )
    }

    #[inline]
    fn not(&self) -> Self {
        self.map(
            #[inline]
            |c| !c,
        )
    }
}
