use pasture_core::layout::{
    PointAttributeDataType, PointAttributeMember, PointLayout, attributes::NUMBER_OF_RETURNS,
};
use pasture_io::las::ATTRIBUTE_EXTENDED_FLAGS;

use super::AttributeExtractor;

pub struct NumberOfReturns4BitExtractor {
    src_stride: usize,
    src_offset: usize,
    dst_stride: usize,
    dst_offset: usize,
}

impl NumberOfReturns4BitExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self> {
        if *dst_attribute.attribute_definition() != NUMBER_OF_RETURNS {
            return None;
        }
        let src_attr_extended_flags = src_layout.get_attribute(&ATTRIBUTE_EXTENDED_FLAGS)?.clone();

        assert_eq!(
            src_attr_extended_flags.datatype(),
            PointAttributeDataType::U16
        );
        assert_eq!(dst_attribute.datatype(), PointAttributeDataType::U8);

        Some(NumberOfReturns4BitExtractor {
            src_stride: src_layout.size_of_point_entry() as usize,
            src_offset: src_attr_extended_flags.byte_range_within_point().start,
            dst_stride: dst_point_size,
            dst_offset: dst_attribute.byte_range_within_point().start,
        })
    }
}

impl AttributeExtractor for NumberOfReturns4BitExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);

        for i in 0..nr_points {
            let src_pos = i * self.src_stride + self.src_offset;
            let dst_pos = i * self.dst_stride + self.dst_offset;

            let extended_flags_byte1 = src[src_pos];
            let number_of_returns = (extended_flags_byte1 & 0xF0) >> 4;
            dst[dst_pos] = number_of_returns;
        }
    }
}

#[cfg(test)]
mod test {
    use crate::extractors::{
        AttributeExtractor, number_of_returns_4bit::NumberOfReturns4BitExtractor,
    };

    #[test]
    pub fn test_number_of_returns_extractor() {
        let extractor = NumberOfReturns4BitExtractor {
            src_stride: 2,
            src_offset: 0,
            dst_stride: 1,
            dst_offset: 0,
        };

        let src = [
            0x01, 0x02, 0x13, 0x04, 0x25, 0x06, 0x37, 0x08, 0x49, 0x10, 0x50, 0x20, 0x60, 0x40,
            0x70, 0x80, 0x81, 0x23, 0x94, 0x56, 0xA7, 0x89, 0xBA, 0xBC, 0xCD, 0xEF, 0xD0, 0x00,
            0xE0, 0x00, 0xF0, 0x00,
        ];
        let mut dst = [0; 16];
        extractor.extract(&src, &mut dst);
        assert_eq!(dst, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
    }
}
