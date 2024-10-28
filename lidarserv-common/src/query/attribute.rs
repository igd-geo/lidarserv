use pasture_core::layout::{PointAttributeDataType, PointAttributeDefinition, PrimitiveType};

pub struct AttributeQuery<T> {
    pub attribute: PointAttributeDefinition,
    pub test: TestFunction<T>,
}

pub enum TestFunction<T> {
    // todo model this nicer
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
    fn map<'a, F: FnMut(&'a T) -> G, G>(&'a self, mut fun: F) -> TestFunction<G> {
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
