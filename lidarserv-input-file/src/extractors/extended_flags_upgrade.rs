use pasture_core::layout::{PointAttributeDataType, PointAttributeMember, PointLayout};
use pasture_io::las::{ATTRIBUTE_BASIC_FLAGS, ATTRIBUTE_EXTENDED_FLAGS};

use super::AttributeExtractor;

/// Extracts the 'ATTRIBUTE_EXTENDED_FLAGS' attribute by
/// "upgrading" the 'ATTRIBUTE_BASIC_FLAGS' attribute.
/// The Classification flags are set to 0.
pub struct LasExtendedFlagsUpgradeExtractor {
    src_offset: usize,
    src_stride: usize,
    dst_offset: usize,
    dst_stride: usize,
}

impl LasExtendedFlagsUpgradeExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self> {
        if *dst_attribute.attribute_definition() != ATTRIBUTE_EXTENDED_FLAGS {
            return None;
        }
        let src_attr = src_layout.get_attribute(&ATTRIBUTE_BASIC_FLAGS)?.clone();

        assert_eq!(src_attr.datatype(), PointAttributeDataType::U8);
        assert_eq!(dst_attribute.datatype(), PointAttributeDataType::U16);

        Some(LasExtendedFlagsUpgradeExtractor {
            src_offset: src_attr.byte_range_within_point().start,
            src_stride: src_layout.size_of_point_entry() as usize,
            dst_offset: dst_attribute.byte_range_within_point().start,
            dst_stride: dst_point_size,
        })
    }
}

impl AttributeExtractor for LasExtendedFlagsUpgradeExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);

        for i in 0..nr_points {
            let src_pos = i * self.src_stride + self.src_offset;
            let basic_flags = src[src_pos];

            // return number
            let mut byte1: u8 = basic_flags & 0x07;

            // number of returns
            byte1 |= (basic_flags & 0x38) << 1;

            // scan direction & edge of flightline
            let byte2 = basic_flags & 0xC0;

            // write
            let dst_pos = i * self.dst_stride + self.dst_offset;
            dst[dst_pos] = byte1;
            dst[dst_pos + 1] = byte2;
        }
    }
}

#[cfg(test)]
mod test {
    use super::LasExtendedFlagsUpgradeExtractor;
    use crate::extractors::AttributeExtractor;

    #[test]
    fn test_extended_flags_upgrade() {
        fn test_case(basic_flags: u8, extended_flags: [u8; 2]) {
            let extractor = LasExtendedFlagsUpgradeExtractor {
                src_offset: 0,
                src_stride: 1,
                dst_offset: 0,
                dst_stride: 2,
            };

            let mut result = [0; 2];
            extractor.extract(&[basic_flags], &mut result);
            assert_eq!(result, extended_flags)
        }
        test_case(0x00, [0x00, 0x00]);
        test_case(0x01, [0x01, 0x00]);
        test_case(0x02, [0x02, 0x00]);
        test_case(0x03, [0x03, 0x00]);
        test_case(0x07, [0x07, 0x00]);
        test_case(0x0F, [0x17, 0x00]);
        test_case(0x17, [0x27, 0x00]);
        test_case(0x1F, [0x37, 0x00]);
        test_case(0x3F, [0x77, 0x00]);
        test_case(0x7F, [0x77, 0x40]);
        test_case(0xBF, [0x77, 0x80]);
        test_case(0xFF, [0x77, 0xC0]);
    }
}
