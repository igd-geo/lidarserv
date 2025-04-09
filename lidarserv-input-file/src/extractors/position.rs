use std::slice;

use lidarserv_common::geometry::{
    coordinate_system::CoordinateSystem,
    position::{Component, POSITION_ATTRIBUTE_NAME, PositionComponentType},
};
use log::warn;
use pasture_core::{
    layout::{PointAttributeDataType, PointAttributeMember, PointLayout},
    nalgebra::{Vector3, vector},
};
use pasture_io::las::ATTRIBUTE_LOCAL_LAS_POSITION;

use super::AttributeExtractor;

/// Reads points from the input, then applies the coordinate system and converts
/// it to the correct component type.
/// Writes the converted position into the target buffer.
pub struct PositionExtractor {
    src_offset: usize,
    src_stride: usize,
    src_component_type: PositionComponentType,
    dst_offset: usize,
    dst_stride: usize,
    dst_component_type: PositionComponentType,
    transform_scale: Vector3<f64>,
    transform_offset: Vector3<f64>,
}

impl PositionExtractor {
    pub fn check(
        dst_coordinate_system: CoordinateSystem,
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
        src_coordinate_system: Option<CoordinateSystem>,
    ) -> Option<Self>
    where
        Self: Sized,
    {
        // check that dst_attribute is a position attribute
        let dst_component_type =
            if *dst_attribute.attribute_definition() == ATTRIBUTE_LOCAL_LAS_POSITION {
                assert_eq!(dst_attribute.datatype(), PointAttributeDataType::Vec3i32);
                PositionComponentType::I32
            } else if dst_attribute.name() == POSITION_ATTRIBUTE_NAME {
                PositionComponentType::from_point_attribute_data_type(dst_attribute.datatype())?
            } else {
                return None;
            };

        // get the position attribute from the source
        let (src_attribute, src_component_type) = if let Some(position_attr) =
            src_layout.get_attribute_by_name(POSITION_ATTRIBUTE_NAME)
        {
            let component_type =
                PositionComponentType::from_point_attribute_data_type(position_attr.datatype())?;
            (position_attr, component_type)
        } else if let Some(position_attr) = src_layout.get_attribute(&ATTRIBUTE_LOCAL_LAS_POSITION)
        {
            assert_eq!(position_attr.datatype(), PointAttributeDataType::Vec3i32);
            (position_attr, PositionComponentType::I32)
        } else {
            return None;
        };

        let (transform_scale, transform_offset) =
            if let Some(src_coordinate_system) = src_coordinate_system {
                let src_scale = *src_coordinate_system.scale();
                let src_offset = *src_coordinate_system.offset();
                let dst_scale = *dst_coordinate_system.scale();
                let dst_offset = *dst_coordinate_system.offset();

                (
                    src_scale.component_div(&dst_scale),
                    (src_offset - dst_offset).component_div(&dst_scale),
                )
            } else {
                let dst_scale = *dst_coordinate_system.scale();
                let dst_offset = *dst_coordinate_system.offset();

                (
                    vector![1.0, 1.0, 1.0].component_div(&dst_scale),
                    -dst_offset.component_div(&dst_scale),
                )
            };
        Some(PositionExtractor {
            src_offset: src_attribute.byte_range_within_point().start,
            src_stride: src_layout.size_of_point_entry() as usize,
            dst_offset: dst_attribute.byte_range_within_point().start,
            dst_stride: dst_point_size,
            src_component_type,
            dst_component_type,
            transform_scale,
            transform_offset,
        })
    }

    fn extract_component_type<CSrc: Component, CDst: Component>(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);
        let mut nr_points_out_of_bounds = 0;
        for i in 0..nr_points {
            let src_start = i * self.src_stride + self.src_offset;
            let src_end = src_start + CSrc::position_attribute().size() as usize;
            let dst_start = i * self.dst_stride + self.dst_offset;
            let dst_end = dst_start + CDst::position_attribute().size() as usize;
            let src_slice = &src[src_start..src_end];
            let dst_slice = &mut dst[dst_start..dst_end];

            // load from src
            let mut src_position = Vector3::<CSrc>::zeros();
            bytemuck::cast_slice_mut::<Vector3<CSrc>, u8>(slice::from_mut(&mut src_position))
                .copy_from_slice(src_slice);

            // coordinate system transformation
            let src_position_f64 = src_position.map(|c| c.to_f64());
            let dst_position_f64 =
                src_position_f64.component_mul(&self.transform_scale) + self.transform_offset;

            let cmin = CDst::MIN.to_f64();
            let cmax = CDst::MAX.to_f64();
            let dst_position = if dst_position_f64.x >= cmin
                && dst_position_f64.y >= cmin
                && dst_position_f64.z >= cmin
                && dst_position_f64.x <= cmax
                && dst_position_f64.y <= cmax
                && dst_position_f64.z <= cmax
            {
                dst_position_f64.map(CDst::from_f64)
            } else {
                nr_points_out_of_bounds += 1;
                Vector3::zeros()
            };

            // store at dst
            dst_slice.copy_from_slice(bytemuck::cast_slice::<Vector3<CDst>, u8>(slice::from_ref(
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
        match (self.src_component_type, self.dst_component_type) {
            (PositionComponentType::F64, PositionComponentType::F64) => {
                self.extract_component_type::<f64, f64>(src, dst)
            }
            (PositionComponentType::F64, PositionComponentType::I32) => {
                self.extract_component_type::<f64, i32>(src, dst)
            }
            (PositionComponentType::I32, PositionComponentType::F64) => {
                self.extract_component_type::<i32, f64>(src, dst)
            }
            (PositionComponentType::I32, PositionComponentType::I32) => {
                self.extract_component_type::<i32, i32>(src, dst)
            }
        }
    }
}
