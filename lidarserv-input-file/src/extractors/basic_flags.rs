use pasture_core::layout::{
    attributes::{EDGE_OF_FLIGHT_LINE, NUMBER_OF_RETURNS, RETURN_NUMBER, SCAN_DIRECTION_FLAG},
    PointAttributeDataType, PointAttributeMember, PointLayout,
};
use pasture_io::las::ATTRIBUTE_BASIC_FLAGS;

use super::AttributeExtractor;

/// Extracts the 'ATTRIBUTE_BASIC_FLAGS' attribute from
/// points that have seperate attributes for each flag
/// (return number, number of returns,
/// scan direction flag, edge of flightline).
pub struct LasBasicFlagsExtractor {
    src_offset_return_number: usize,
    src_offset_nr_of_returns: usize,
    src_offset_scan_direction_flag: usize,
    src_offset_edge_of_flightline: usize,
    src_stride: usize,
    dst_offset: usize,
    dst_stride: usize,
}

impl LasBasicFlagsExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self> {
        if *dst_attribute.attribute_definition() != ATTRIBUTE_BASIC_FLAGS {
            return None;
        }
        let src_attr_return_number = src_layout.get_attribute(&RETURN_NUMBER)?.clone();
        let src_attr_nr_of_returns = src_layout.get_attribute(&NUMBER_OF_RETURNS)?.clone();
        let src_attr_scan_direction_flag = src_layout.get_attribute(&SCAN_DIRECTION_FLAG)?.clone();
        let src_attr_edge_of_flightline = src_layout.get_attribute(&EDGE_OF_FLIGHT_LINE)?.clone();

        assert_eq!(
            src_attr_return_number.datatype(),
            PointAttributeDataType::U8
        );
        assert_eq!(
            src_attr_nr_of_returns.datatype(),
            PointAttributeDataType::U8
        );
        assert_eq!(
            src_attr_scan_direction_flag.datatype(),
            PointAttributeDataType::U8
        );
        assert_eq!(
            src_attr_edge_of_flightline.datatype(),
            PointAttributeDataType::U8
        );
        assert_eq!(dst_attribute.datatype(), PointAttributeDataType::U8);

        Some(LasBasicFlagsExtractor {
            src_offset_return_number: src_attr_return_number.byte_range_within_point().start,
            src_offset_nr_of_returns: src_attr_nr_of_returns.byte_range_within_point().start,
            src_offset_scan_direction_flag: src_attr_scan_direction_flag
                .byte_range_within_point()
                .start,
            src_offset_edge_of_flightline: src_attr_edge_of_flightline
                .byte_range_within_point()
                .start,
            src_stride: src_layout.size_of_point_entry() as usize,
            dst_offset: dst_attribute.byte_range_within_point().start,
            dst_stride: dst_point_size,
        })
    }
}

impl AttributeExtractor for LasBasicFlagsExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);

        for i in 0..nr_points {
            let src_start = i * self.src_stride;
            let src_end = src_start + self.src_stride;
            let src_point = &src[src_start..src_end];

            // return number
            let mut return_number: u8 = src_point[self.src_offset_return_number];
            if return_number >= 8 {
                return_number = 7;
            }
            let mut flags: u8 = return_number;

            // number of returns
            let mut nr_of_returns: u8 = src_point[self.src_offset_nr_of_returns];
            if nr_of_returns >= 8 {
                nr_of_returns = 7;
            }
            flags |= nr_of_returns << 3;

            // scan direction
            let scan_direction_flag: u8 = src_point[self.src_offset_scan_direction_flag];
            if scan_direction_flag > 0 {
                flags |= 0x40;
            }

            // edge of flightline
            let edge_of_flightline: u8 = src_point[self.src_offset_edge_of_flightline];
            if edge_of_flightline > 0 {
                flags |= 0x80;
            }

            // write
            let dst_start = i * self.dst_stride + self.dst_offset;
            dst[dst_start] = flags;
        }
    }
}

#[cfg(test)]
mod test {
    use std::slice;

    use crate::extractors::AttributeExtractor;

    use super::LasBasicFlagsExtractor;

    #[test]
    fn test_extract_basic_flags() {
        #[derive(Debug)]
        struct TestInput {
            return_number: u8,
            nr_of_returns: u8,
            scan_direction_flag: u8,
            edge_of_flightline: u8,
        }
        fn test_case(input: TestInput, expected_result: u8) {
            let extractor = LasBasicFlagsExtractor {
                src_offset_return_number: 0,
                src_offset_nr_of_returns: 1,
                src_offset_scan_direction_flag: 2,
                src_offset_edge_of_flightline: 3,
                src_stride: 4,
                dst_offset: 0,
                dst_stride: 1,
            };

            let mut result = 0_u8;
            extractor.extract(
                &[
                    input.return_number,
                    input.nr_of_returns,
                    input.scan_direction_flag,
                    input.edge_of_flightline,
                ],
                slice::from_mut(&mut result),
            );
            println!();
            println!("Input: {input:?}");
            println!("Expected basic_flags: 0x{expected_result:02x}");
            println!("Actual basic_flags:   0x{result:02x}");
            assert_eq!(result, expected_result)
        }

        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 1,
                scan_direction_flag: 0,
                edge_of_flightline: 0,
            },
            0x09,
        );
        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 2,
                scan_direction_flag: 0,
                edge_of_flightline: 0,
            },
            0x11,
        );
        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 7,
                scan_direction_flag: 0,
                edge_of_flightline: 0,
            },
            0x39,
        );
        test_case(
            TestInput {
                return_number: 2,
                nr_of_returns: 7,
                scan_direction_flag: 0,
                edge_of_flightline: 0,
            },
            0x3A,
        );
        test_case(
            TestInput {
                return_number: 7,
                nr_of_returns: 7,
                scan_direction_flag: 0,
                edge_of_flightline: 0,
            },
            0x3F,
        );
        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 1,
                scan_direction_flag: 1,
                edge_of_flightline: 0,
            },
            0x49,
        );
        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 1,
                scan_direction_flag: 0,
                edge_of_flightline: 1,
            },
            0x89,
        );
    }
}