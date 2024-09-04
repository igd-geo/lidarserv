use pasture_core::layout::{
    attributes::{
        CLASSIFICATION_FLAGS, EDGE_OF_FLIGHT_LINE, NUMBER_OF_RETURNS, RETURN_NUMBER,
        SCANNER_CHANNEL, SCAN_DIRECTION_FLAG,
    },
    PointAttributeDataType, PointAttributeMember, PointLayout,
};
use pasture_io::las::ATTRIBUTE_EXTENDED_FLAGS;

use super::AttributeExtractor;

/// Extracts the 'ATTRIBUTE_EXTENDED_FLAGS' attribute from
/// points that have seperate attributes for each flag
/// (return number, number of returns, classification flags,
/// scanner channel, scan direction flag, edge of flightline).
pub struct LasExtendedFlagsExtractor {
    src_stride: usize,
    src_offset_return_number: usize,
    src_offset_nr_of_returns: usize,
    src_offset_classification_flags: usize,
    src_offset_scanner_channel: usize,
    src_offset_scan_direction_flag: usize,
    src_offset_edge_of_flightline: usize,
    dst_stride: usize,
    dst_offset: usize,
}

impl LasExtendedFlagsExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self> {
        if *dst_attribute.attribute_definition() != ATTRIBUTE_EXTENDED_FLAGS {
            return None;
        }
        let src_attr_return_number = src_layout.get_attribute(&RETURN_NUMBER)?.clone();
        let src_attr_nr_of_returns = src_layout.get_attribute(&NUMBER_OF_RETURNS)?.clone();
        let src_attr_classification_flags =
            src_layout.get_attribute(&CLASSIFICATION_FLAGS)?.clone();
        let src_attr_scanner_channel = src_layout.get_attribute(&SCANNER_CHANNEL)?.clone();
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
            src_attr_classification_flags.datatype(),
            PointAttributeDataType::U8
        );
        assert_eq!(
            src_attr_scanner_channel.datatype(),
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
        assert_eq!(dst_attribute.datatype(), PointAttributeDataType::U16);

        Some(LasExtendedFlagsExtractor {
            src_offset_return_number: src_attr_return_number.byte_range_within_point().start,
            src_offset_nr_of_returns: src_attr_nr_of_returns.byte_range_within_point().start,
            src_offset_classification_flags: src_attr_classification_flags
                .byte_range_within_point()
                .start,
            src_offset_scanner_channel: src_attr_scanner_channel.byte_range_within_point().start,
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

impl AttributeExtractor for LasExtendedFlagsExtractor {
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
            if return_number >= 16 {
                return_number = 15;
            }
            let mut flags_byte1: u8 = return_number;

            // number of returns
            let mut nr_of_returns: u8 = src_point[self.src_offset_nr_of_returns];
            if nr_of_returns >= 16 {
                nr_of_returns = 15;
            }
            flags_byte1 |= nr_of_returns << 4;

            // classification flags
            let classification_flags: u8 = src_point[self.src_offset_classification_flags];
            let mut flags_byte2 = classification_flags & 0x0F;

            // scanner channel
            let scanner_channel: u8 = src_point[self.src_offset_scanner_channel];
            flags_byte2 |= (scanner_channel & 0x03) << 4;

            // scan direction
            let scan_direction_flag: u8 = src_point[self.src_offset_scan_direction_flag];
            if scan_direction_flag > 0 {
                flags_byte2 |= 0x40;
            }

            // edge of flightline
            let edge_of_flightline: u8 = src_point[self.src_offset_edge_of_flightline];
            if edge_of_flightline > 0 {
                flags_byte2 |= 0x80;
            }

            // write
            let dst_start = i * self.dst_stride + self.dst_offset;
            dst[dst_start] = flags_byte1;
            dst[dst_start + 1] = flags_byte2;
        }
    }
}

#[cfg(test)]
mod test {
    use crate::extractors::{extended_flags::LasExtendedFlagsExtractor, AttributeExtractor};

    #[test]
    fn test_extract_extended_flags() {
        #[derive(Debug)]
        struct TestInput {
            return_number: u8,
            nr_of_returns: u8,
            classification_flags: u8,
            scanner_channel: u8,
            scan_direction_flag: u8,
            edge_of_flightline: u8,
        }

        fn test_case(input: TestInput, expected_result: [u8; 2]) {
            let extractor = LasExtendedFlagsExtractor {
                src_offset_return_number: 0,
                src_offset_nr_of_returns: 1,
                src_offset_scan_direction_flag: 2,
                src_offset_edge_of_flightline: 3,
                src_offset_classification_flags: 4,
                src_offset_scanner_channel: 5,
                src_stride: 6,
                dst_offset: 0,
                dst_stride: 2,
            };
            let mut result = [0, 0];
            extractor.extract(
                &[
                    input.return_number,
                    input.nr_of_returns,
                    input.scan_direction_flag,
                    input.edge_of_flightline,
                    input.classification_flags,
                    input.scanner_channel,
                ],
                &mut result,
            );
            println!();
            println!("Input: {input:?}");
            println!(
                "Expected extended_flags: 0x{:02x} 0x{:02x}",
                expected_result[0], expected_result[1]
            );
            println!(
                "Actual extended_flags:   0x{:02x} 0x{:02x}",
                result[0], result[1]
            );
            assert_eq!(expected_result, result);
        }

        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 1,
                classification_flags: 0x0,
                scanner_channel: 0,
                scan_direction_flag: 0,
                edge_of_flightline: 0,
            },
            [0x11, 0x00],
        );
        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 2,
                classification_flags: 0x0,
                scanner_channel: 0,
                scan_direction_flag: 0,
                edge_of_flightline: 0,
            },
            [0x21, 0x00],
        );
        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 15,
                classification_flags: 0x0,
                scanner_channel: 0,
                scan_direction_flag: 0,
                edge_of_flightline: 0,
            },
            [0xF1, 0x00],
        );
        test_case(
            TestInput {
                return_number: 2,
                nr_of_returns: 15,
                classification_flags: 0x0,
                scanner_channel: 0,
                scan_direction_flag: 0,
                edge_of_flightline: 0,
            },
            [0xF2, 0x00],
        );
        test_case(
            TestInput {
                return_number: 15,
                nr_of_returns: 15,
                classification_flags: 0x0,
                scanner_channel: 0,
                scan_direction_flag: 0,
                edge_of_flightline: 0,
            },
            [0xFF, 0x00],
        );
        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 1,
                classification_flags: 0x1,
                scanner_channel: 0,
                scan_direction_flag: 0,
                edge_of_flightline: 0,
            },
            [0x11, 0x01],
        );
        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 1,
                classification_flags: 0xF,
                scanner_channel: 0,
                scan_direction_flag: 0,
                edge_of_flightline: 0,
            },
            [0x11, 0x0F],
        );
        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 1,
                classification_flags: 0x0,
                scanner_channel: 1,
                scan_direction_flag: 0,
                edge_of_flightline: 0,
            },
            [0x11, 0x10],
        );
        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 1,
                classification_flags: 0x0,
                scanner_channel: 3,
                scan_direction_flag: 0,
                edge_of_flightline: 0,
            },
            [0x11, 0x30],
        );
        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 1,
                classification_flags: 0x0,
                scanner_channel: 0,
                scan_direction_flag: 1,
                edge_of_flightline: 0,
            },
            [0x11, 0x40],
        );
        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 1,
                classification_flags: 0x0,
                scanner_channel: 0,
                scan_direction_flag: 0,
                edge_of_flightline: 1,
            },
            [0x11, 0x80],
        );
        test_case(
            TestInput {
                return_number: 1,
                nr_of_returns: 1,
                classification_flags: 0x0,
                scanner_channel: 0,
                scan_direction_flag: 1,
                edge_of_flightline: 1,
            },
            [0x11, 0xC0],
        );
    }
}
