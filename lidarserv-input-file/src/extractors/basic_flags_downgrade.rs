use pasture_core::layout::{PointAttributeDataType, PointAttributeMember, PointLayout};
use pasture_io::las::{ATTRIBUTE_BASIC_FLAGS, ATTRIBUTE_EXTENDED_FLAGS};

use super::AttributeExtractor;

/// Extracts the 'ATTRIBUTE_BASIC_FLAGS' attribute by
/// "downgrading" the 'ATTRIBUTE_EXTENDED_FLAGS' attribute.
pub struct LasBasicFlagsDowngradeExtractor {
    src_offset: usize,
    src_stride: usize,
    dst_offset: usize,
    dst_stride: usize,
}

impl LasBasicFlagsDowngradeExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self> {
        if *dst_attribute.attribute_definition() != ATTRIBUTE_BASIC_FLAGS {
            return None;
        }
        let src_attr = src_layout.get_attribute(&ATTRIBUTE_EXTENDED_FLAGS)?.clone();

        assert_eq!(src_attr.datatype(), PointAttributeDataType::U16);
        assert_eq!(dst_attribute.datatype(), PointAttributeDataType::U8);

        Some(LasBasicFlagsDowngradeExtractor {
            src_offset: src_attr.byte_range_within_point().start,
            src_stride: src_layout.size_of_point_entry() as usize,
            dst_offset: dst_attribute.byte_range_within_point().start,
            dst_stride: dst_point_size,
        })
    }
}

impl AttributeExtractor for LasBasicFlagsDowngradeExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);

        for i in 0..nr_points {
            let src_pos = i * self.src_stride + self.src_offset;
            let byte1 = src[src_pos];
            let byte2 = src[src_pos + 1];

            // return number
            let mut return_number: u8 = byte1 & 0x0F;
            if return_number >= 8 {
                return_number = 7;
            }
            let mut flags: u8 = return_number;

            // number of returns
            let mut nr_of_returns: u8 = (byte1 & 0xF0) >> 4;
            if nr_of_returns >= 8 {
                nr_of_returns = 7;
            }
            flags |= nr_of_returns << 3;

            // scan direction & edge of flightline
            let scanner_flags = byte2 & 0xC0;
            flags |= scanner_flags;

            // write
            let dst_pos = i * self.dst_stride + self.dst_offset;
            dst[dst_pos] = flags;
        }
    }
}

#[cfg(test)]
mod test {
    use super::LasBasicFlagsDowngradeExtractor;
    use crate::extractors::AttributeExtractor;

    #[test]
    fn test_basic_flags_downgrade() {
        fn test_case(basic_flags: u8, extended_flags: [u8; 2]) {
            let extractor = LasBasicFlagsDowngradeExtractor {
                src_offset: 0,
                src_stride: 2,
                dst_offset: 0,
                dst_stride: 1,
            };

            let mut result = [0];
            extractor.extract(&extended_flags, &mut result);
            assert_eq!(result, [basic_flags])
        }
        test_case(0x00, [0x00, 0x01]);
        test_case(0x01, [0x01, 0x02]);
        test_case(0x02, [0x02, 0x03]);
        test_case(0x03, [0x03, 0x04]);
        test_case(0x07, [0x0F, 0x05]);
        test_case(0x0F, [0x1F, 0x06]);
        test_case(0x17, [0x2F, 0x07]);
        test_case(0x1F, [0x3F, 0x08]);
        test_case(0x3F, [0xFF, 0x09]);
        test_case(0x7F, [0xFF, 0x4A]);
        test_case(0xBF, [0xFF, 0x8B]);
        test_case(0xFF, [0xFF, 0xCC]);
    }
}
