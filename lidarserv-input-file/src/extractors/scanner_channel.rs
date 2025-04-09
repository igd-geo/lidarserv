use pasture_core::layout::{
    PointAttributeDataType, PointAttributeMember, PointLayout, attributes::SCANNER_CHANNEL,
};
use pasture_io::las::ATTRIBUTE_EXTENDED_FLAGS;

use super::AttributeExtractor;

pub struct ScannerChannelExtractor {
    src_stride: usize,
    src_offset: usize,
    dst_stride: usize,
    dst_offset: usize,
}

impl ScannerChannelExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self> {
        if *dst_attribute.attribute_definition() != SCANNER_CHANNEL {
            return None;
        }
        let src_attr_extended_flags = src_layout.get_attribute(&ATTRIBUTE_EXTENDED_FLAGS)?.clone();

        assert_eq!(
            src_attr_extended_flags.datatype(),
            PointAttributeDataType::U16
        );
        assert_eq!(dst_attribute.datatype(), PointAttributeDataType::U8);

        Some(ScannerChannelExtractor {
            src_stride: src_layout.size_of_point_entry() as usize,
            src_offset: src_attr_extended_flags.byte_range_within_point().start + 1,
            dst_stride: dst_point_size,
            dst_offset: dst_attribute.byte_range_within_point().start,
        })
    }
}

impl AttributeExtractor for ScannerChannelExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);

        for i in 0..nr_points {
            let src_pos = i * self.src_stride + self.src_offset;
            let dst_pos = i * self.dst_stride + self.dst_offset;

            let extended_flags_byte2 = src[src_pos];
            let scanner_channel = (extended_flags_byte2 & 0x30) >> 4;
            dst[dst_pos] = scanner_channel;
        }
    }
}
