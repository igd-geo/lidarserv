use nalgebra::{Point3, Scalar, Vector3};
use pasture_core::layout::{
    PointAttributeDataType, PointAttributeDefinition, PointLayout, PrimitiveType,
};
use std::{borrow::Cow, fmt::Debug};

use super::grid::GridComponent;

pub const POSITION_ATTRIBUTE_NAME: &str = "Position3D";

/// A position in the global coordinate system
pub type PositionGlobal = Point3<f64>;

/// Positions in the local coordinate system with some scale and offset.
pub type Position<C> = Point3<C>;

pub trait Component:
    nalgebra::Scalar
    + nalgebra::ClosedAdd
    + nalgebra::ClosedSub
    + nalgebra::ClosedMul
    + nalgebra::ClosedDiv
    + num_traits::One
    + num_traits::Zero
    + bytemuck::Pod
    + bytemuck::Zeroable
    + serde::ser::Serialize
    + serde::de::DeserializeOwned
    + Copy
    + Debug
    + Send
    + Sync
    + Sized
    + PartialOrd
    + PartialEq
    + GridComponent
    + PasturePrimitiveHelper
    + 'static
{
    fn to_f64(self) -> f64;
    fn from_f64(value: f64) -> Self;
    fn centre(self, other: Self) -> Self;
    fn position_attribute() -> PointAttributeDefinition;
    const MIN: Self;
    const MAX: Self;
}

impl Component for i32 {
    fn to_f64(self) -> f64 {
        self as f64
    }

    fn from_f64(value: f64) -> Self {
        value.round() as i32
    }

    fn centre(self, other: Self) -> Self {
        (self + other) / 2
    }

    fn position_attribute() -> PointAttributeDefinition {
        PointAttributeDefinition::custom(
            Cow::Borrowed(POSITION_ATTRIBUTE_NAME),
            PointAttributeDataType::Vec3i32,
        )
    }

    const MIN: Self = i32::MIN;
    const MAX: Self = i32::MAX;
}

impl Component for f64 {
    fn to_f64(self) -> f64 {
        self
    }

    fn from_f64(value: f64) -> Self {
        value
    }

    fn centre(self, other: Self) -> Self {
        (self + other) * 0.5
    }

    fn position_attribute() -> PointAttributeDefinition {
        PointAttributeDefinition::custom(
            Cow::Borrowed(POSITION_ATTRIBUTE_NAME),
            PointAttributeDataType::Vec3f64,
        )
    }

    const MIN: Self = f64::MIN;
    const MAX: Self = f64::MAX;
}

/// The type that is used for the component in the position attribute.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PositionComponentType {
    /// Position attribute is Vector3<F64>
    F64,

    /// Position attribute is Vector3<I32>
    I32,
}

impl PositionComponentType {
    pub fn from_layout(layout: &PointLayout) -> Self {
        match layout
            .get_attribute_by_name(POSITION_ATTRIBUTE_NAME)
            .expect("missing position attribute")
            .datatype()
        {
            PointAttributeDataType::Vec3f64 => PositionComponentType::F64,
            PointAttributeDataType::Vec3i32 => PositionComponentType::I32,
            _ => panic!("Unsupported position attribute type"),
        }
    }
}

pub trait WithComponentType {
    type Output;

    fn run<C: Component>(&self) -> Self::Output;

    fn for_component_type(&self, typ: PositionComponentType) -> Self::Output {
        match typ {
            PositionComponentType::F64 => self.run::<f64>(),
            PositionComponentType::I32 => self.run::<i32>(),
        }
    }

    fn for_layout(&self, layout: &PointLayout) -> Self::Output {
        self.for_component_type(PositionComponentType::from_layout(layout))
    }
}

pub trait WithComponentTypeMut {
    type Output;

    fn run_mut<C: Component>(&mut self) -> Self::Output;

    fn for_component_type_mut(&mut self, typ: PositionComponentType) -> Self::Output {
        match typ {
            PositionComponentType::F64 => self.run_mut::<f64>(),
            PositionComponentType::I32 => self.run_mut::<i32>(),
        }
    }

    fn for_layout_mut(&mut self, layout: &PointLayout) -> Self::Output {
        self.for_component_type_mut(PositionComponentType::from_layout(layout))
    }
}

pub trait WithComponentTypeOnce: Sized {
    type Output;

    fn run_once<C: Component>(self) -> Self::Output;

    fn for_component_type_once(self, typ: PositionComponentType) -> Self::Output {
        match typ {
            PositionComponentType::F64 => self.run_once::<f64>(),
            PositionComponentType::I32 => self.run_once::<i32>(),
        }
    }

    fn for_layout_once(self, layout: &PointLayout) -> Self::Output {
        self.for_component_type_once(PositionComponentType::from_layout(layout))
    }
}

impl<T> WithComponentTypeMut for T
where
    T: WithComponentType,
{
    type Output = T::Output;

    fn run_mut<C: Component>(&mut self) -> Self::Output {
        self.run::<C>()
    }
}

impl<T> WithComponentTypeOnce for T
where
    T: WithComponentTypeMut,
{
    type Output = T::Output;

    fn run_once<C: Component>(mut self) -> Self::Output {
        self.run_mut::<C>()
    }
}

pub trait PasturePrimitiveHelper: Scalar {
    type PasturePrimitive: PrimitiveType + Default;

    fn pasture_to_position(p: Self::PasturePrimitive) -> Position<Self>;
    fn position_to_pasture(p: Position<Self>) -> Self::PasturePrimitive;
}

impl<C> PasturePrimitiveHelper for C
where
    C: Scalar,
    Vector3<C>: PrimitiveType + Default,
{
    type PasturePrimitive = Vector3<Self>;

    fn pasture_to_position(p: Self::PasturePrimitive) -> Position<Self> {
        Position::from(p)
    }

    fn position_to_pasture(p: Position<Self>) -> Self::PasturePrimitive {
        p.coords
    }
}

#[cfg(test)]
mod tests {
    use pasture_core::layout::attributes::POSITION_3D;

    use crate::geometry::position::POSITION_ATTRIBUTE_NAME;

    #[test]
    fn position_attribute_name() {
        assert_eq!(POSITION_ATTRIBUTE_NAME, POSITION_3D.name())
    }
}

/*
Here are some macros, to implement WithComponentType,
WithComponentTypeMut and WithComponentTypeOnce on the fly.
Basically, the with_component_type macro is to the WithComponentType
trait, what closures are to the Fn trait. See tests below for examples.

HOWEVER, I am not sure, if they should be used in actual code,
because in their current state, rustfmt will not format the
macro contents. In many cases, the code inside the macro body
will be quite long, so this is actually really annoying.

I will keep this commented out for now, until I find a solution for
this, or until rustfmt becomes better.

#[macro_export]
macro_rules! wct_impl {
    (
        $traitname:ident $fnname:ident [$($self:tt)*] $selfvar:ident
        <$ty:ident> $($rest:tt)*
    ) => {
        $crate::wct_impl!(
            $traitname $fnname [$($self)*] $selfvar
            <, $ty> $($rest)*
        )
    };
    (
        $traitname:ident $fnname:ident [$($self:tt)*] $selfvar:ident
        <$($lifetime:lifetime),*, $ty:ident>($($arg:ident: $argty:ty = $argval:expr),*) $block:block
    ) => {
        $crate::wct_impl!(
            $traitname $fnname [$($self)*] $selfvar
            <$($lifetime),*, $ty> ($($arg: $argty = $argval),*) -> () $block
        )
    };
    ($traitname:ident $fnname:ident [$($self:tt)*] $selfvar:ident <$($lifetime:lifetime),*, $ty:ident>($($arg:ident: $argty:ty = $argval:expr),*) -> $ret:ty $block:block) => {{
        struct Wct<$($lifetime),*> {
            $(
                $arg: $argty
            ),*
        }
        impl<$($lifetime),*> $crate::geometry::position::$traitname for Wct<$($lifetime),*> {
            type Output = $ret;

            fn $fnname<$ty: $crate::geometry::position::Component>($($self)*) -> Self::Output {
                let Wct{$($arg),*} = $selfvar;
                $block
            }

        }
        Wct {
            $(
                $arg: $argval
            ),*
        }
    }};
}

#[macro_export]
macro_rules! with_component_type {
    ($($tt:tt)*) => {
        $crate::wct_impl!(WithComponentType run [&self] self $($tt)*)
    };
}

#[macro_export]
macro_rules! with_component_type_mut {
    ($($tt:tt)*) => {
        $crate::wct_impl!(WithComponentTypeMut run_mut [&mut self] self $($tt)*)
    };
}

#[macro_export]
macro_rules! with_component_type_once {
    ($($tt:tt)*) => {
        $crate::wct_impl!(WithComponentTypeOnce run_once [self] self $($tt)*)
    };
}

#[cfg(test)]
mod macro_tests {
    use std::borrow::Cow;

    use pasture_core::layout::{PointAttributeDataType, PointAttributeDefinition, PointLayout};

    use crate::geometry::position::WithComponentTypeMut;

    use super::{WithComponentType, WithComponentTypeOnce};

    fn make_test_layout_f64() -> PointLayout {
        PointLayout::from_attributes(&[PointAttributeDefinition::custom(
            Cow::Borrowed(POSITION_ATTRIBUTE_NAME),
            PointAttributeDataType::Vec3f64,
        )])
    }

    fn make_test_layout_i32() -> PointLayout {
        PointLayout::from_attributes(&[PointAttributeDefinition::custom(
            Cow::Borrowed(POSITION_ATTRIBUTE_NAME),
            PointAttributeDataType::Vec3i32,
        )])
    }

    #[test]
    fn test_macro() {
        let wct = with_component_type!(<A>() -> usize {
            std::mem::size_of::<A>()
        });

        let test_layout_f64 = make_test_layout_f64();
        let test_layout_int = make_test_layout_i32();
        assert_eq!(wct.for_layout(&test_layout_f64), 8);
        assert_eq!(wct.for_layout(&test_layout_int), 4);
    }

    #[test]
    fn test_macro_mut() {
        let mut wct = with_component_type_mut!(<A>(counter: u32 = 0) -> u32 {
            *counter += 1;
            *counter
        });

        let test_layout = make_test_layout_f64();
        assert_eq!(wct.for_layout_mut(&test_layout), 1);
        assert_eq!(wct.for_layout_mut(&test_layout), 2);
        assert_eq!(wct.for_layout_mut(&test_layout), 3);
    }

    #[test]
    fn test_macro_mut_with_borrow() {
        let mut counter = 0;
        let mut wct = with_component_type_mut!(<'a, A>(
            my_reference: &'a mut u32 = &mut counter
        ) {
            **my_reference += 1;
        });

        let test_layout = make_test_layout_f64();
        wct.for_layout_mut(&test_layout);
        wct.for_layout_mut(&test_layout);
        wct.for_layout_mut(&test_layout);

        assert_eq!(counter, 3);
    }

    #[test]
    fn test_macro_once() {
        let input = vec!["Hello", "World"];
        let wct = with_component_type_once!(<A>(
            myvec: Vec<&'static str> = input
        ) -> Vec<String> {
            myvec.into_iter().map(|s| s.to_string()).collect()
        });

        let test_layout = make_test_layout_f64();
        let result = wct.for_layout_once(&test_layout);
        assert_eq!(result, vec!["Hello".to_string(), "World".to_string()]);
    }
}
 */
