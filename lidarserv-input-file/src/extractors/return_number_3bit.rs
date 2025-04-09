use pasture_core::layout::{
    PointAttributeDataType, PointAttributeMember, PointLayout, attributes::RETURN_NUMBER,
};
use pasture_io::las::ATTRIBUTE_BASIC_FLAGS;

use super::AttributeExtractor;

pub struct ReturnNumber3BitExtractor {
    src_stride: usize,
    src_offset: usize,
    dst_stride: usize,
    dst_offset: usize,
}

impl ReturnNumber3BitExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self> {
        if *dst_attribute.attribute_definition() != RETURN_NUMBER {
            return None;
        }
        let src_attr_basic_flags = src_layout.get_attribute(&ATTRIBUTE_BASIC_FLAGS)?.clone();

        assert_eq!(src_attr_basic_flags.datatype(), PointAttributeDataType::U8);
        assert_eq!(dst_attribute.datatype(), PointAttributeDataType::U8);

        Some(ReturnNumber3BitExtractor {
            src_stride: src_layout.size_of_point_entry() as usize,
            src_offset: src_attr_basic_flags.byte_range_within_point().start,
            dst_stride: dst_point_size,
            dst_offset: dst_attribute.byte_range_within_point().start,
        })
    }
}

impl AttributeExtractor for ReturnNumber3BitExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);

        for i in 0..nr_points {
            let src_pos = i * self.src_stride + self.src_offset;
            let dst_pos = i * self.dst_stride + self.dst_offset;

            let basic_flags = src[src_pos];
            let return_number = basic_flags & 0x07;
            dst[dst_pos] = return_number;
        }
    }
}

#[cfg(test)]
mod test {
    use crate::extractors::{AttributeExtractor, return_number_3bit::ReturnNumber3BitExtractor};

    #[test]
    pub fn test_return_number_extractor() {
        let extractor = ReturnNumber3BitExtractor {
            src_stride: 1,
            src_offset: 0,
            dst_stride: 1,
            dst_offset: 0,
        };

        let src = [0x08, 0x11, 0x22, 0x43, 0x84, 0x0D, 0x16, 0x1F];
        let mut dst = [0; 8];
        extractor.extract(&src, &mut dst);
        assert_eq!(dst, [0, 1, 2, 3, 4, 5, 6, 7]);
    }
}
