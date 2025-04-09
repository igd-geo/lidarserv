use pasture_core::layout::{
    PointAttributeDataType, PointAttributeMember, PointLayout, attributes::NUMBER_OF_RETURNS,
};
use pasture_io::las::ATTRIBUTE_BASIC_FLAGS;

use super::AttributeExtractor;

pub struct NumberOfReturns3BitExtractor {
    src_stride: usize,
    src_offset: usize,
    dst_stride: usize,
    dst_offset: usize,
}

impl NumberOfReturns3BitExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self> {
        if *dst_attribute.attribute_definition() != NUMBER_OF_RETURNS {
            return None;
        }
        let src_attr_basic_flags = src_layout.get_attribute(&ATTRIBUTE_BASIC_FLAGS)?.clone();

        assert_eq!(src_attr_basic_flags.datatype(), PointAttributeDataType::U8);
        assert_eq!(dst_attribute.datatype(), PointAttributeDataType::U8);

        Some(NumberOfReturns3BitExtractor {
            src_stride: src_layout.size_of_point_entry() as usize,
            src_offset: src_attr_basic_flags.byte_range_within_point().start,
            dst_stride: dst_point_size,
            dst_offset: dst_attribute.byte_range_within_point().start,
        })
    }
}

impl AttributeExtractor for NumberOfReturns3BitExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);

        for i in 0..nr_points {
            let src_pos = i * self.src_stride + self.src_offset;
            let dst_pos = i * self.dst_stride + self.dst_offset;

            let basic_flags = src[src_pos];
            let number_of_returns = (basic_flags & 0x38) >> 3;
            dst[dst_pos] = number_of_returns;
        }
    }
}

#[cfg(test)]
mod test {
    use crate::extractors::{
        AttributeExtractor, number_of_returns_3bit::NumberOfReturns3BitExtractor,
    };

    #[test]
    pub fn test_number_of_returns_extractor() {
        let extractor = NumberOfReturns3BitExtractor {
            src_stride: 1,
            src_offset: 0,
            dst_stride: 1,
            dst_offset: 0,
        };

        let src = [0x00, 0x09, 0x12, 0x1C, 0x60, 0xA8, 0x31, 0x3B];
        let mut dst = [0; 8];
        extractor.extract(&src, &mut dst);
        assert_eq!(dst, [0, 1, 2, 3, 4, 5, 6, 7]);
    }
}
