use std::slice;

use lidarserv_common::geometry::{
    coordinate_system::CoordinateSystem,
    position::{Component, PositionComponentType, WithComponentTypeOnce, POSITION_ATTRIBUTE_NAME},
};
use log::warn;
use pasture_core::{
    layout::{PointAttributeMember, PointLayout},
    nalgebra::Point3,
};

use super::AttributeExtractor;

/// Reads points from the input, then applies the coordinate system and converts
/// it to the correct component type.
/// Writes the converted position into the target buffer.
pub struct PositionExtractor {
    src_offset: usize,
    src_stride: usize,
    dst_offset: usize,
    dst_stride: usize,
    coordinate_system: CoordinateSystem,
    component_type: PositionComponentType,
}

impl PositionExtractor {
    pub fn check(
        dst_coordinate_system: CoordinateSystem,
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self>
    where
        Self: Sized,
    {
        // check that dst_attribute is a position attribute
        let component_type =
            PositionComponentType::from_point_attribute_data_type(dst_attribute.datatype())?;
        if dst_attribute.name() != POSITION_ATTRIBUTE_NAME {
            return None;
        }

        // get the position attribute from the source
        let position_f64 = f64::position_attribute();
        let src_attribute = src_layout.get_attribute(&position_f64)?;

        Some(PositionExtractor {
            src_offset: src_attribute.byte_range_within_point().start,
            src_stride: src_layout.size_of_point_entry() as usize,
            dst_offset: dst_attribute.byte_range_within_point().start,
            dst_stride: dst_point_size,
            coordinate_system: dst_coordinate_system,
            component_type,
        })
    }

    fn extract_component_type<C: Component>(&self, src: &[u8], dst: &mut [u8]) {
        //let position_f64 = f64::position_attribute();
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);
        let mut nr_points_out_of_bounds = 0;
        for i in 0..nr_points {
            let src_start = i * self.src_stride + self.src_offset;
            let src_end = src_start + f64::position_attribute().size() as usize;
            let dst_start = i * self.dst_stride + self.dst_offset;
            let dst_end = dst_start + C::position_attribute().size() as usize;
            let src_slice = &src[src_start..src_end];
            let dst_slice = &mut dst[dst_start..dst_end];

            // load from src
            let mut src_position = Point3::<f64>::origin();
            bytemuck::cast_slice_mut::<Point3<f64>, u8>(slice::from_mut(&mut src_position))
                .copy_from_slice(src_slice);

            // coordinate system transformation
            let dst_position = match self.coordinate_system.encode_position::<C>(src_position) {
                Ok(o) => o,
                Err(_) => {
                    nr_points_out_of_bounds += 1;
                    Default::default()
                }
            };

            // store at dst
            dst_slice.copy_from_slice(bytemuck::cast_slice::<Point3<C>, u8>(slice::from_ref(
                &dst_position,
            )))
        }

        if nr_points_out_of_bounds > 0 {
            warn!(
                "Found {} point(s), that were out of the bounds of the coordinate system.",
                nr_points_out_of_bounds
            )
        }
    }
}

impl AttributeExtractor for PositionExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        struct Wct<'a, 'b, 'c> {
            me: &'a PositionExtractor,
            src: &'b [u8],
            dst: &'c mut [u8],
        }
        impl<'a, 'b, 'c> WithComponentTypeOnce for Wct<'a, 'b, 'c> {
            type Output = ();

            fn run_once<C: Component>(self) -> Self::Output {
                let Wct { me, src, dst } = self;
                me.extract_component_type::<C>(src, dst)
            }
        }
        Wct { me: self, src, dst }.for_component_type_once(self.component_type)
    }
}
