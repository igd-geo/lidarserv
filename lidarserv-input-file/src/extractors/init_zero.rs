use pasture_core::layout::PointAttributeMember;

use super::AttributeExtractor;

/// Extractor that zero-initializes an an attribute.
pub struct InitZeroExtractor {
    dst_offset: usize,
    dst_len: usize,
    dst_stride: usize,
}

impl InitZeroExtractor {
    pub fn create(dst_attribute: &PointAttributeMember, dst_point_size: usize) -> Self {
        Self {
            dst_offset: dst_attribute.byte_range_within_point().start,
            dst_len: dst_attribute.size() as usize,
            dst_stride: dst_point_size,
        }
    }
}

impl AttributeExtractor for InitZeroExtractor {
    fn extract(&self, _src: &[u8], dst: &mut [u8]) {
        let nr_points = dst.len() / self.dst_stride;
        assert!(dst.len() == nr_points * self.dst_stride);

        for i in 0..nr_points {
            let dst_pos1 = i * self.dst_stride + self.dst_offset;
            let dst_pos2 = dst_pos1 + self.dst_len;
            let dst_slice = &mut dst[dst_pos1..dst_pos2];
            dst_slice.fill(0);
        }
    }
}
