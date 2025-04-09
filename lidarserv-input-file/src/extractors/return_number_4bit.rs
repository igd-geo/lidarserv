use pasture_core::layout::{
    PointAttributeDataType, PointAttributeMember, PointLayout, attributes::RETURN_NUMBER,
};
use pasture_io::las::ATTRIBUTE_EXTENDED_FLAGS;

use super::AttributeExtractor;

pub struct ReturnNumber4BitExtractor {
    src_stride: usize,
    src_offset: usize,
    dst_stride: usize,
    dst_offset: usize,
}

impl ReturnNumber4BitExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self> {
        if *dst_attribute.attribute_definition() != RETURN_NUMBER {
            return None;
        }
        let src_attr_extended_flags = src_layout.get_attribute(&ATTRIBUTE_EXTENDED_FLAGS)?.clone();

        assert_eq!(
            src_attr_extended_flags.datatype(),
            PointAttributeDataType::U16
        );
        assert_eq!(dst_attribute.datatype(), PointAttributeDataType::U8);

        Some(ReturnNumber4BitExtractor {
            src_stride: src_layout.size_of_point_entry() as usize,
            src_offset: src_attr_extended_flags.byte_range_within_point().start,
            dst_stride: dst_point_size,
            dst_offset: dst_attribute.byte_range_within_point().start,
        })
    }
}

impl AttributeExtractor for ReturnNumber4BitExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);

        for i in 0..nr_points {
            let src_pos = i * self.src_stride + self.src_offset;
            let dst_pos = i * self.dst_stride + self.dst_offset;

            let extended_flags_byte1 = src[src_pos];
            let return_number = extended_flags_byte1 & 0x0F;
            dst[dst_pos] = return_number;
        }
    }
}

#[cfg(test)]
mod test {
    use crate::extractors::{AttributeExtractor, return_number_4bit::ReturnNumber4BitExtractor};

    #[test]
    pub fn test_return_number_extractor() {
        let extractor = ReturnNumber4BitExtractor {
            src_stride: 2,
            src_offset: 0,
            dst_stride: 1,
            dst_offset: 0,
        };

        let src = [
            0x10, 0x00, 0x21, 0x00, 0x42, 0x00, 0x83, 0x01, 0x04, 0x02, 0x05, 0x04, 0x06, 0x08,
            0x07, 0x10, 0x08, 0x20, 0x09, 0x40, 0x0A, 0x80, 0x1B, 0x10, 0x2C, 0x20, 0x3D, 0x30,
            0x4E, 0x50, 0x6F, 0x70,
        ];
        let mut dst = [0; 16];
        extractor.extract(&src, &mut dst);
        assert_eq!(dst, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
    }
}
