use std::slice;

use pasture_core::layout::{
    PointAttributeDataType, PointAttributeMember, PointLayout,
    attributes::{SCAN_ANGLE, SCAN_ANGLE_RANK},
};

use super::AttributeExtractor;

/// Extracts the 'SCAN_ANGLE_RANK' from points by
/// converting the 'SCAN_ANGLE' attribute.
pub struct ScanAngleRankExtractor {
    src_offset: usize,
    src_stride: usize,
    dst_offset: usize,
    dst_stride: usize,
}

impl ScanAngleRankExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self> {
        if *dst_attribute.attribute_definition() != SCAN_ANGLE_RANK {
            return None;
        }
        let src_attribute = src_layout.get_attribute(&SCAN_ANGLE)?.clone();

        assert_eq!(src_attribute.datatype(), PointAttributeDataType::I16);
        assert_eq!(dst_attribute.datatype(), PointAttributeDataType::I8);

        Some(ScanAngleRankExtractor {
            src_offset: src_attribute.byte_range_within_point().start,
            src_stride: src_layout.size_of_point_entry() as usize,
            dst_offset: dst_attribute.byte_range_within_point().start,
            dst_stride: dst_point_size,
        })
    }
}

impl AttributeExtractor for ScanAngleRankExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);
        for i in 0..nr_points {
            let src_start = i * self.src_stride + self.src_offset;
            let src_end = src_start + 2;
            let src_slice = &src[src_start..src_end];
            let dst_start = i * self.dst_stride + self.dst_offset;
            let dst_end = dst_start + 1;
            let dst_slice = &mut dst[dst_start..dst_end];

            // read scan angle
            let mut scan_angle: i16 = 0;
            bytemuck::cast_slice_mut(slice::from_mut(&mut scan_angle)).copy_from_slice(src_slice);

            // convert to scan angle rank
            // scan angle: 1 step = 0.006 degrees
            // scan angle rank: 1 step = 1 degree
            let scan_angle_rank = (scan_angle as f64 * 0.006)
                .round()
                .clamp(i8::MIN as f64, i8::MAX as f64) as i8;

            // write
            dst_slice.copy_from_slice(bytemuck::cast_slice(slice::from_ref(&scan_angle_rank)));
        }
    }
}
