use std::{fmt::Debug, sync::Arc};

use nalgebra::{Vector3, Vector4};
use pasture_core::{
    containers::{BorrowedBuffer, BorrowedBufferExt},
    layout::{PointAttributeDataType, PointAttributeDefinition, PrimitiveType},
};
use serde::{Deserialize, Serialize};

use crate::index::attribute_index::{AttributeIndex, boolvec::BoolVec, cmp::ComponentwiseCmp};

use super::{ExecutableQuery, Query};

#[derive(Debug, Clone)]
pub struct AttributeQuery<T> {
    pub attribute: PointAttributeDefinition,
    pub test: TestFunction<T>,
}

pub struct AttributeQueryExecutable<T> {
    attribute_index: Arc<AttributeIndex>,
    query: AttributeQuery<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TestFunction<T> {
    Eq(T),
    Neq(T),
    Less(T),
    LessEq(T),
    Greater(T),
    GreaterEq(T),
    RangeExclusive(T, T),
    RangeLeftInclusive(T, T),
    RangeRightInclusive(T, T),
    RangeAllInclusive(T, T),
}

impl<T> TestFunction<T> {
    // utility to apply a mapping function to each contained attribute value.
    pub fn map<'a, F: FnMut(&'a T) -> G, G>(&'a self, mut fun: F) -> TestFunction<G> {
        match self {
            TestFunction::Eq(t) => TestFunction::Eq(fun(t)),
            TestFunction::Neq(t) => TestFunction::Neq(fun(t)),
            TestFunction::Less(t) => TestFunction::Less(fun(t)),
            TestFunction::LessEq(t) => TestFunction::LessEq(fun(t)),
            TestFunction::Greater(t) => TestFunction::Greater(fun(t)),
            TestFunction::GreaterEq(t) => TestFunction::GreaterEq(fun(t)),
            TestFunction::RangeExclusive(t, u) => TestFunction::RangeExclusive(fun(t), fun(u)),
            TestFunction::RangeLeftInclusive(t, u) => {
                TestFunction::RangeLeftInclusive(fun(t), fun(u))
            }
            TestFunction::RangeRightInclusive(t, u) => {
                TestFunction::RangeRightInclusive(fun(t), fun(u))
            }
            TestFunction::RangeAllInclusive(t, u) => {
                TestFunction::RangeAllInclusive(fun(t), fun(u))
            }
        }
    }
}

impl<T, E> TestFunction<Result<T, E>> {
    pub fn result(self) -> Result<TestFunction<T>, E> {
        Ok(match self {
            TestFunction::Eq(o) => TestFunction::Eq(o?),
            TestFunction::Neq(o) => TestFunction::Neq(o?),
            TestFunction::Less(o) => TestFunction::Less(o?),
            TestFunction::LessEq(o) => TestFunction::LessEq(o?),
            TestFunction::Greater(o) => TestFunction::Greater(o?),
            TestFunction::GreaterEq(o) => TestFunction::GreaterEq(o?),
            TestFunction::RangeExclusive(o, p) => TestFunction::RangeExclusive(o?, p?),
            TestFunction::RangeLeftInclusive(o, p) => TestFunction::RangeLeftInclusive(o?, p?),
            TestFunction::RangeRightInclusive(o, p) => TestFunction::RangeRightInclusive(o?, p?),
            TestFunction::RangeAllInclusive(o, p) => TestFunction::RangeAllInclusive(o?, p?),
        })
    }
}

/// A reference to a test function with the concrete attribute type erased.
///
/// The implementation uses the fact, that a pasture::PrimitiveType is safe to convert to a byte slice and
/// back.
pub struct TestFunctionDyn<'a> {
    datatype: PointAttributeDataType,
    test_function: TestFunction<&'a [u8]>,
}

impl<'a> TestFunctionDyn<'a> {
    /// Datatype of the attribute tested
    pub fn datatype(&self) -> &PointAttributeDataType {
        &self.datatype
    }

    /// Create from a TestFunction<T>.
    pub fn new<T: PrimitiveType>(typed: &'a TestFunction<T>) -> Self {
        let datatype = T::data_type();
        assert_eq!(datatype.size(), size_of::<T>() as u64); // every correct implementation of PrimitiveType should pass these assertions. However, if not, this would be VERY undefined behaviour. So we still assert it just to be sure.
        assert_eq!(datatype.min_alignment(), align_of::<T>() as u64);
        let test_function = typed.map(|a| bytemuck::cast_slice::<T, u8>(std::slice::from_ref(a)));
        TestFunctionDyn {
            datatype,
            test_function,
        }
    }

    /// Convert it back to a TestFunction<&T>.
    /// (Where &T is a reference into the original TestFunction)
    /// The T::data_type() must match self.datatype() - otherwise: panick!
    pub fn convert_to<T: PrimitiveType>(&self) -> TestFunction<&'a T> {
        let datatype = T::data_type();
        assert_eq!(datatype.size(), size_of::<T>() as u64);
        assert_eq!(datatype.min_alignment(), align_of::<T>() as u64);
        assert_eq!(datatype, self.datatype);
        self.test_function
            .map(|data| &bytemuck::cast_slice::<u8, T>(data)[0])
    }
}

pub trait FilterableAttributeType:
    PrimitiveType + ComponentwiseCmp + Debug + Send + Sync + 'static
{
}

impl FilterableAttributeType for u8 {}
impl FilterableAttributeType for u16 {}
impl FilterableAttributeType for u32 {}
impl FilterableAttributeType for u64 {}
impl FilterableAttributeType for i8 {}
impl FilterableAttributeType for i16 {}
impl FilterableAttributeType for i32 {}
impl FilterableAttributeType for i64 {}
impl FilterableAttributeType for f32 {}
impl FilterableAttributeType for f64 {}
impl FilterableAttributeType for Vector3<u8> {}
impl FilterableAttributeType for Vector3<u16> {}
impl FilterableAttributeType for Vector3<f32> {}
impl FilterableAttributeType for Vector3<i32> {}
impl FilterableAttributeType for Vector3<f64> {}
impl FilterableAttributeType for Vector4<u8> {}

#[derive(Debug, Copy, Clone, Eq, PartialEq, thiserror::Error)]
pub enum AttriuteQueryError {
    #[error("Missmatch between attribute type and query type.")]
    AttributeType,
}

impl<T> Query for AttributeQuery<T>
where
    T: FilterableAttributeType,
{
    type Executable = AttributeQueryExecutable<T>;
    type Error = AttriuteQueryError;

    fn prepare(self, ctx: &super::QueryContext) -> Result<Self::Executable, Self::Error> {
        // todo maybe allow prepare to return a result?
        if self.attribute.datatype() != T::data_type() {
            return Err(AttriuteQueryError::AttributeType);
        }
        Ok(AttributeQueryExecutable {
            attribute_index: Arc::clone(&ctx.attribute_index),
            query: self,
        })
    }
}

impl<T> ExecutableQuery for AttributeQueryExecutable<T>
where
    T: FilterableAttributeType,
{
    fn matches_node(&self, node: crate::geometry::grid::LeveledGridCell) -> super::NodeQueryResult {
        self.attribute_index.test(&node, &self.query)
    }

    fn matches_points(
        &self,
        _lod: crate::geometry::grid::LodLevel,
        points: &pasture_core::containers::VectorBuffer,
    ) -> Vec<bool> {
        let mut result_vec = vec![false; points.len()];
        let result = &mut result_vec;
        let values = points
            .view_attribute::<T>(&self.query.attribute)
            .into_iter();

        match &self.query.test {
            TestFunction::Eq(o) => filter_points(
                result,
                values,
                #[inline]
                |t| t.is_eq(o).all(),
            ),
            TestFunction::Neq(o) => filter_points(
                result,
                values,
                #[inline]
                |t| t.is_eq(o).not().all(),
            ),
            TestFunction::Less(o) => filter_points(
                result,
                values,
                #[inline]
                |t| t.is_less(o).all(),
            ),
            TestFunction::LessEq(o) => filter_points(
                result,
                values,
                #[inline]
                |t| t.is_less_eq(o).all(),
            ),
            TestFunction::Greater(o) => filter_points(
                result,
                values,
                #[inline]
                |t| t.is_greater(o).all(),
            ),
            TestFunction::GreaterEq(o) => filter_points(
                result,
                values,
                #[inline]
                |t| t.is_greater_eq(o).all(),
            ),
            TestFunction::RangeExclusive(l, r) => filter_points(
                result,
                values,
                #[inline]
                |t| t.is_greater(l).and(&t.is_less(r)).all(),
            ),
            TestFunction::RangeLeftInclusive(l, r) => filter_points(
                result,
                values,
                #[inline]
                |t| t.is_greater_eq(l).and(&t.is_less(r)).all(),
            ),
            TestFunction::RangeRightInclusive(l, r) => filter_points(
                result,
                values,
                #[inline]
                |t| t.is_greater(l).and(&t.is_less_eq(r)).all(),
            ),
            TestFunction::RangeAllInclusive(l, r) => filter_points(
                result,
                values,
                #[inline]
                |t| t.is_greater_eq(l).and(&t.is_less_eq(r)).all(),
            ),
        }

        result_vec
    }
}

fn filter_points<T: PrimitiveType>(
    result: &mut [bool],
    values: impl Iterator<Item = T>,
    test_function: impl Fn(T) -> bool,
) {
    for (i, value) in values.enumerate() {
        result[i] = test_function(value)
    }
}
