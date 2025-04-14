use super::{IndexFunction, boolvec::BoolVec, cmp::ComponentwiseCmp};
use crate::query::NodeQueryResult;
use nalgebra::{SVector, Scalar};
use num_traits::{CheckedShr, PrimInt};
use serde::{Deserialize, Serialize};
use std::{
    cmp::{Ordering, max},
    marker::PhantomData,
};

/// Attribute type, that can be located on some space filling curve.
pub trait SfcAttribute {
    /// Type for representing the position of this
    /// attribute on a space filling curve.
    type Sfc: Sfc;

    /// Calculates the position of this attribute value on a space filling curve.
    fn sfc(&self) -> Self::Sfc;
}

macro_rules! impl_sfc_int_attribute {
    ($t:ty) => {
        impl SfcAttribute for $t {
            type Sfc = $t;

            #[inline]
            fn sfc(&self) -> Self::Sfc {
                *self
            }
        }
    };
}

impl_sfc_int_attribute!(u8);
impl_sfc_int_attribute!(i8);
impl_sfc_int_attribute!(u16);
impl_sfc_int_attribute!(i16);
impl_sfc_int_attribute!(u32);
impl_sfc_int_attribute!(i32);
impl_sfc_int_attribute!(u64);
impl_sfc_int_attribute!(i64);

impl SfcAttribute for f64 {
    type Sfc = u64;

    fn sfc(&self) -> Self::Sfc {
        const MASK_EXPONENT: u64 = 0x7ff0_0000_0000_0000;
        const MASK_FRACTION: u64 = 0x000f_ffff_ffff_ffff;
        const MASK_SIGN_BIT: u64 = 0x8000_0000_0000_0000;
        let mut bits = self.to_bits();

        // signed zero: treat positive and negative the same
        if (bits & (MASK_EXPONENT | MASK_FRACTION)) == 0 {
            bits |= MASK_SIGN_BIT;
        }

        // NaN: treat all nan values the same - (signalling or not)
        if (bits & MASK_EXPONENT) == MASK_EXPONENT && (bits & MASK_FRACTION) != 0 {
            bits |= MASK_SIGN_BIT | MASK_FRACTION;
        }

        // inverse bits based on the sign, so that we have a total order in u64
        // (i.e.  bits1 < bits2 iff float1 < float2)
        if (bits & MASK_SIGN_BIT) == 0 {
            bits |= MASK_SIGN_BIT;
        } else {
            bits = !bits;
        }

        bits
    }
}

impl SfcAttribute for f32 {
    type Sfc = u32;

    fn sfc(&self) -> Self::Sfc {
        const MASK_SIGN_BIT: u32 = 0x8000_0000;
        const MASK_EXPONENT: u32 = 0x7f80_0000;
        const MASK_FRACTION: u32 = 0x007f_ffff;
        let mut bits = self.to_bits();

        // signed zero: treat positive and negative the same
        if (bits & (MASK_EXPONENT | MASK_FRACTION)) == 0 {
            bits |= MASK_SIGN_BIT;
        }

        // NaN: treat all nan values the same - (signalling or not)
        if (bits & MASK_EXPONENT) == MASK_EXPONENT && (bits & MASK_FRACTION) != 0 {
            bits |= MASK_SIGN_BIT | MASK_FRACTION;
        }

        // inverse bits based on the sign, so that we have a total order in u32
        // (i.e.  bits1 < bits2 iff float1 < float2)
        if (bits & MASK_SIGN_BIT) == 0 {
            bits |= MASK_SIGN_BIT;
        } else {
            bits = !bits;
        }

        bits
    }
}

impl<T, const D: usize> SfcAttribute for SVector<T, D>
where
    T: SfcAttribute + Scalar,
    T::Sfc: Scalar + PrimInt + CheckedShr,
{
    type Sfc = SVector<T::Sfc, D>;

    fn sfc(&self) -> Self::Sfc {
        self.map(|d| d.sfc())
    }
}

/// Position on a space filling curve
pub trait Sfc {
    /// Shifts the value to the right by the given number of bits.
    ///
    /// This effectively increases the size of the bucket. For example,
    /// when shifting scalar values by 3 bits, 8 original values
    /// will be mapped to the same shifted value. So, we have a bucket size of 8.
    #[must_use]
    fn shift(&self, bits: u8) -> Self;

    /// Compares self to other.
    ///
    /// The comparison is stable against shifting:
    /// When shifting a sorted list of values,
    /// it will still be sorted afterwards.
    /// Two different values might become equal
    /// after shifting, due to the (intended) loss of accuracy.
    /// But they will never flip from
    /// 'a<b' to 'b<a'.
    ///
    /// For scalars this can be implemented as a
    /// normal integer comparison.
    /// For vectors, the result of the comparison
    /// is equivalent to comparing the z-order interleaved
    /// value of both operands.
    fn interleaved_cmp(&self, other: &Self) -> Ordering;
}

macro_rules! impl_sfc_int {
    ($t:ty) => {
        impl Sfc for $t {
            #[inline]
            fn shift(&self, bits: u8) -> Self {
                // todo use unbounded_shr() once the unbounded_shifts feature is stable.
                // https://github.com/rust-lang/rust/issues/129375
                self.checked_shr(bits as u32).unwrap_or(0)
            }

            #[inline]
            fn interleaved_cmp(&self, other: &Self) -> Ordering {
                self.cmp(other)
            }
        }
    };
}

impl_sfc_int!(u8);
impl_sfc_int!(u16);
impl_sfc_int!(u32);
impl_sfc_int!(u64);
impl_sfc_int!(i8);
impl_sfc_int!(i16);
impl_sfc_int!(i32);
impl_sfc_int!(i64);

impl<T, const D: usize> Sfc for SVector<T, D>
where
    T: Scalar + PrimInt + CheckedShr,
{
    #[inline]
    fn shift(&self, bits: u8) -> Self {
        self.map(
            #[inline]
            |c| c.checked_shr(bits as u32).unwrap_or_else(T::zero),
        )
    }

    fn interleaved_cmp(&self, other: &Self) -> Ordering {
        // instead of explicitely interleaving the components,
        // we use this implicit comparison, which should lead to the same result.
        let diff = self.zip_map(other, |l, r| l ^ r);
        let prefixlen = diff.map(|d| d.leading_zeros());
        let (shortest_prefix, _) = prefixlen.argmin();
        self[shortest_prefix].cmp(&other[shortest_prefix])
    }
}

/// Used as the node type by the histogram attribute index to accelerate queries.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SfcIndexNode<R> {
    /// By how many bits have the values in bins been shifted.
    shift: u8,

    /// List of sfc values that covers all attribute values.
    bins: Vec<R>,
}

/// Implementation of the space filling curve index.
///
/// Approximates the attribute values in each node using
/// a set of ranges on a space filling curve.
pub struct SfcIndex<Attr> {
    bins: usize,
    _phantom: PhantomData<Attr>,
}

fn query_result(pos: bool, neg: bool) -> NodeQueryResult {
    match (pos, neg) {
        (true, true) => NodeQueryResult::Partial,
        (true, false) => NodeQueryResult::Positive,
        (false, true) => NodeQueryResult::Negative,
        (false, false) => NodeQueryResult::Negative,
    }
}

impl<Attr> SfcIndex<Attr> {
    pub fn new(bins: usize) -> Self {
        SfcIndex {
            bins,
            _phantom: PhantomData,
        }
    }
}

impl<Attr> IndexFunction for SfcIndex<Attr>
where
    Attr: SfcAttribute,
    Attr::Sfc: Eq + ComponentwiseCmp,
{
    type AttributeValue = Attr;

    type NodeType = SfcIndexNode<Attr::Sfc>;

    fn index(
        &self,
        attribute_values: impl Iterator<Item = Self::AttributeValue>,
    ) -> Self::NodeType {
        // make sorted buffer of all the (unshifted) bins.
        let mut buf = Vec::<Attr::Sfc>::new();
        for attr in attribute_values {
            buf.push(attr.sfc());
        }
        buf.sort_unstable_by(Attr::Sfc::interleaved_cmp);

        // collect into a node.
        let mut shift = 0;
        let mut node_buf = Vec::<Attr::Sfc>::with_capacity(self.bins);
        'bin_loop: for bin_to_add in buf.into_iter() {
            // apply shift to the new bin
            let mut bin_to_add = bin_to_add.shift(shift);

            // ensure there is space in the node
            'shift_loop: loop {
                // dont add duplicate bins
                if let Some(last) = node_buf.last() {
                    if *last == bin_to_add {
                        continue 'bin_loop;
                    }
                }

                // make space by shifting
                if node_buf.len() < self.bins {
                    break 'shift_loop;
                }
                shift += 1;
                bin_to_add = bin_to_add.shift(1);
                for bin in &mut node_buf {
                    *bin = bin.shift(1);
                }
                node_buf.dedup();
            }

            // add
            node_buf.push(bin_to_add);
        }

        // create node
        SfcIndexNode {
            shift,
            bins: node_buf,
        }
    }

    fn merge(&self, node1: &mut Self::NodeType, node2: Self::NodeType) {
        // make sure nodes have same shift
        let mut shift = max(node1.shift, node2.shift);
        let mut node1_addshift = shift - node1.shift;
        let mut node2_addshift = shift - node2.shift;

        // merge
        let mut buf1 = node1.bins.as_slice();
        let mut buf2 = node2.bins.as_slice();
        let mut merge_buf = Vec::<Attr::Sfc>::with_capacity(self.bins);
        'bin_loop: loop {
            // get a bin value from node1 or node2
            let s1 = buf1.split_first();
            let s2 = buf2.split_first();
            let mut bin = match (s1, s2) {
                (None, None) => break 'bin_loop,
                (Some((head1, tail1)), None) => {
                    buf1 = tail1;
                    head1.shift(node1_addshift)
                }
                (None, Some((head2, tail2))) => {
                    buf2 = tail2;
                    head2.shift(node2_addshift)
                }
                (Some((head1, tail1)), Some((head2, tail2))) => {
                    let head1_shifted = head1.shift(node1_addshift);
                    let head2_shifted = head2.shift(node2_addshift);
                    if head1_shifted.interleaved_cmp(&head2_shifted).is_lt() {
                        buf1 = tail1;
                        head1_shifted
                    } else {
                        buf2 = tail2;
                        head2_shifted
                    }
                }
            };

            // ensure there is space
            'shift_loop: loop {
                // dont add duplicate bins
                if let Some(last) = merge_buf.last() {
                    if bin == *last {
                        continue 'bin_loop;
                    }
                }

                // make space by shifting
                if merge_buf.len() < self.bins {
                    break 'shift_loop;
                }
                shift += 1;
                node1_addshift += 1;
                node2_addshift += 1;
                bin = bin.shift(1);
                for bin in &mut merge_buf {
                    *bin = bin.shift(1);
                }
                merge_buf.dedup();
            }

            // add to merge buffer
            merge_buf.push(bin);
        }

        node1.shift = shift;
        node1.bins = merge_buf;
    }

    fn test_eq(
        &self,
        node: &Self::NodeType,
        op: &Self::AttributeValue,
    ) -> crate::query::NodeQueryResult {
        let op_bin = op.sfc().shift(node.shift);
        let mut pos = false;
        let mut neg = false;
        for bin in &node.bins {
            neg = true;
            if bin.is_eq(&op_bin).all() {
                pos = true;
                break;
            }
        }
        query_result(pos, neg)
    }

    fn test_neq(
        &self,
        node: &Self::NodeType,
        op: &Self::AttributeValue,
    ) -> crate::query::NodeQueryResult {
        let op_bin = op.sfc().shift(node.shift);
        let mut pos = false;
        let mut neg = false;
        for bin in &node.bins {
            pos = true;
            if bin.is_eq(&op_bin).any() {
                neg = true;
                break;
            }
        }
        query_result(pos, neg)
    }

    fn test_less(
        &self,
        node: &Self::NodeType,
        op: &Self::AttributeValue,
    ) -> crate::query::NodeQueryResult {
        let op_bin = op.sfc().shift(node.shift);
        let mut pos = false;
        let mut neg = false;
        for bin in &node.bins {
            if bin.is_less_eq(&op_bin).all() {
                pos = true;
            }
            if bin.is_greater_eq(&op_bin).any() {
                neg = true;
            }
            if pos && neg {
                break;
            }
        }
        query_result(pos, neg)
    }

    fn test_less_eq(
        &self,
        node: &Self::NodeType,
        op: &Self::AttributeValue,
    ) -> crate::query::NodeQueryResult {
        self.test_less(node, op)
    }

    fn test_greater(
        &self,
        node: &Self::NodeType,
        op: &Self::AttributeValue,
    ) -> crate::query::NodeQueryResult {
        let op_bin = op.sfc().shift(node.shift);
        let mut pos = false;
        let mut neg = false;
        for bin in &node.bins {
            if bin.is_greater_eq(&op_bin).all() {
                pos = true;
            }
            if bin.is_less_eq(&op_bin).any() {
                neg = true;
            }
            if pos && neg {
                break;
            }
        }
        query_result(pos, neg)
    }

    fn test_greater_eq(
        &self,
        node: &Self::NodeType,
        op: &Self::AttributeValue,
    ) -> crate::query::NodeQueryResult {
        self.test_greater(node, op)
    }

    fn test_range_inclusive(
        &self,
        node: &Self::NodeType,
        op1: &Self::AttributeValue,
        op2: &Self::AttributeValue,
    ) -> NodeQueryResult {
        let op1_bin = op1.sfc().shift(node.shift);
        let op2_bin = op2.sfc().shift(node.shift);
        let mut pos = false;
        let mut neg = false;
        for bin in &node.bins {
            if bin
                .is_greater_eq(&op1_bin)
                .and(&bin.is_less_eq(&op2_bin))
                .all()
            {
                pos = true;
            }
            if bin
                .is_less_eq(&op1_bin)
                .or(&bin.is_greater_eq(&op2_bin))
                .any()
            {
                neg = true;
            }
            if pos && neg {
                break;
            }
        }
        query_result(pos, neg)
    }

    #[inline]
    fn test_range_left_inclusive(
        &self,
        node: &Self::NodeType,
        op1: &Self::AttributeValue,
        op2: &Self::AttributeValue,
    ) -> NodeQueryResult {
        self.test_range_inclusive(node, op1, op2)
    }

    #[inline]
    fn test_range_right_inclusive(
        &self,
        node: &Self::NodeType,
        op1: &Self::AttributeValue,
        op2: &Self::AttributeValue,
    ) -> NodeQueryResult {
        self.test_range_inclusive(node, op1, op2)
    }

    #[inline]
    fn test_range_exclusive(
        &self,
        node: &Self::NodeType,
        op1: &Self::AttributeValue,
        op2: &Self::AttributeValue,
    ) -> NodeQueryResult {
        self.test_range_inclusive(node, op1, op2)
    }
}

#[cfg(test)]
mod test {
    use super::SfcAttribute;

    #[test]
    fn test_f32_sfc() {
        let test_values: &[f32] = &[
            987654.0,
            32.123,
            f32::INFINITY,
            f32::NEG_INFINITY,
            0.0,
            -10.123,
            -1.23,
            -0.123,
        ];
        for v1 in test_values {
            for v2 in test_values {
                let expected_result = v1.total_cmp(v2);
                let actual_result = v1.sfc().cmp(&v2.sfc());
                assert_eq!(actual_result, expected_result);
            }
        }
    }

    #[test]
    fn test_f64_sfc() {
        let test_values: &[f64] = &[
            987654.0,
            32.123,
            f64::INFINITY,
            f64::NEG_INFINITY,
            0.0,
            -10.123,
            -1.23,
            -0.123,
        ];
        for v1 in test_values {
            for v2 in test_values {
                let expected_result = v1.total_cmp(v2);
                let actual_result = v1.sfc().cmp(&v2.sfc());
                assert_eq!(actual_result, expected_result);
            }
        }
    }
}
