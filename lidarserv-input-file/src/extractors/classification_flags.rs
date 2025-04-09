use pasture_core::layout::{
    PointAttributeDataType, PointAttributeMember, PointLayout, attributes::CLASSIFICATION_FLAGS,
};
use pasture_io::las::ATTRIBUTE_EXTENDED_FLAGS;

use super::AttributeExtractor;

pub struct ClassificationFlagsExtractor {
    src_stride: usize,
    src_offset: usize,
    dst_stride: usize,
    dst_offset: usize,
}

impl ClassificationFlagsExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self> {
        if *dst_attribute.attribute_definition() != CLASSIFICATION_FLAGS {
            return None;
        }
        let src_attr_extended_flags = src_layout.get_attribute(&ATTRIBUTE_EXTENDED_FLAGS)?.clone();

        assert_eq!(
            src_attr_extended_flags.datatype(),
            PointAttributeDataType::U16
        );
        assert_eq!(dst_attribute.datatype(), PointAttributeDataType::U8);

        Some(ClassificationFlagsExtractor {
            src_stride: src_layout.size_of_point_entry() as usize,
            src_offset: src_attr_extended_flags.byte_range_within_point().start + 1,
            dst_stride: dst_point_size,
            dst_offset: dst_attribute.byte_range_within_point().start,
        })
    }
}

impl AttributeExtractor for ClassificationFlagsExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);

        for i in 0..nr_points {
            let src_pos = i * self.src_stride + self.src_offset;
            let dst_pos = i * self.dst_stride + self.dst_offset;

            let extended_flags_byte2 = src[src_pos];
            let number_of_returns = extended_flags_byte2 & 0x0F;
            dst[dst_pos] = number_of_returns;
        }
    }
}

#[cfg(test)]
mod test {
    use crate::extractors::AttributeExtractor;

    use super::ClassificationFlagsExtractor;

    #[test]
    pub fn test_classification_flags_extractor() {
        let extractor = ClassificationFlagsExtractor {
            src_stride: 2,
            src_offset: 1,
            dst_stride: 1,
            dst_offset: 0,
        };
        let src = [0x12, 0x30, 0x45, 0x61, 0x78, 0x94, 0xAB, 0xC8, 0xDE, 0xFF];
        let mut dst = [0; 5];
        extractor.extract(&src, &mut dst);
        assert_eq!(dst, [0x00, 0x01, 0x04, 0x08, 0xF]);
    }
}
