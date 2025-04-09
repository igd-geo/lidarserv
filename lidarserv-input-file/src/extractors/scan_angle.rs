use std::slice;

use pasture_core::layout::{
    PointAttributeDataType, PointAttributeMember, PointLayout,
    attributes::{SCAN_ANGLE, SCAN_ANGLE_RANK},
};

use super::AttributeExtractor;

/// Extracts the 'SCAN_ANGLE' from points by
/// converting the 'SCAN_ANGLE_RANK' attribute.
pub struct ScanAngleExtractor {
    src_offset: usize,
    src_stride: usize,
    dst_offset: usize,
    dst_stride: usize,
}

impl ScanAngleExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self> {
        if *dst_attribute.attribute_definition() != SCAN_ANGLE {
            return None;
        }
        let src_attribute = src_layout.get_attribute(&SCAN_ANGLE_RANK)?.clone();

        assert_eq!(src_attribute.datatype(), PointAttributeDataType::I8);
        assert_eq!(dst_attribute.datatype(), PointAttributeDataType::I16);

        Some(ScanAngleExtractor {
            src_offset: src_attribute.byte_range_within_point().start,
            src_stride: src_layout.size_of_point_entry() as usize,
            dst_offset: dst_attribute.byte_range_within_point().start,
            dst_stride: dst_point_size,
        })
    }
}

impl AttributeExtractor for ScanAngleExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);
        for i in 0..nr_points {
            let src_pos = i * self.src_stride + self.src_offset;
            let dst_start = i * self.dst_stride + self.dst_offset;
            let dst_end = dst_start + 2;
            let dst_slice = &mut dst[dst_start..dst_end];

            // read scan angle
            let scan_angle_rank: i8 = bytemuck::cast(src[src_pos]);

            // convert to scan angle rank
            // scan angle: 1 step = 0.006 degrees
            // scan angle rank: 1 step = 1 degree
            let scan_angle = ((scan_angle_rank as f64) / 0.006) as i16;

            // write
            dst_slice.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&scan_angle)));
        }
    }
}
