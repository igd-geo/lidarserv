use pasture_core::layout::{PointAttributeMember, PointLayout};

use super::AttributeExtractor;

/// Extracts any attribute by bitwise copying
/// it from src to dst.
pub struct CopyExtractor {
    src_offset: usize,
    src_stride: usize,
    dst_offset: usize,
    dst_stride: usize,
    len: usize,
}

impl CopyExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        extract_from: &PointLayout,
    ) -> Option<Self> {
        let attribute = dst_attribute.attribute_definition();
        let src_attribute = extract_from.get_attribute(attribute)?;
        Some(CopyExtractor {
            src_offset: src_attribute.byte_range_within_point().start,
            src_stride: extract_from.size_of_point_entry() as usize,
            dst_offset: dst_attribute.byte_range_within_point().start,
            dst_stride: dst_point_size,
            len: dst_attribute.size() as usize,
        })
    }
}

impl AttributeExtractor for CopyExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);
        for i in 0..nr_points {
            let src_start = i * self.src_stride + self.src_offset;
            let src_end = src_start + self.len;
            let dst_start = i * self.dst_stride + self.dst_offset;
            let dst_end = dst_start + self.len;
            dst[dst_start..dst_end].copy_from_slice(&src[src_start..src_end]);
        }
    }
}
