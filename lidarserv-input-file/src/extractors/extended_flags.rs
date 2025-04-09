use pasture_core::layout::{
    PointAttributeDataType, PointAttributeMember, PointLayout,
    attributes::{
        CLASSIFICATION_FLAGS, EDGE_OF_FLIGHT_LINE, NUMBER_OF_RETURNS, RETURN_NUMBER,
        SCAN_DIRECTION_FLAG, SCANNER_CHANNEL,
    },
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
    src_offset_classification_flags: Option<usize>,
    src_offset_scanner_channel: Option<usize>,
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
            src_layout.get_attribute(&CLASSIFICATION_FLAGS).cloned();
        let src_attr_scanner_channel = src_layout.get_attribute(&SCANNER_CHANNEL).cloned();
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
        if let Some(member) = &src_attr_classification_flags {
            assert_eq!(member.datatype(), PointAttributeDataType::U8);
        }
        if let Some(member) = &src_attr_scanner_channel {
            assert_eq!(member.datatype(), PointAttributeDataType::U8);
        }
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
                .map(|member| member.byte_range_within_point().start),
            src_offset_scanner_channel: src_attr_scanner_channel
                .map(|member| member.byte_range_within_point().start),
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
            let mut flags_byte2 = 0;
            if let Some(offset) = self.src_offset_classification_flags {
                let classification_flags: u8 = src_point[offset];
                flags_byte2 |= classification_flags & 0x0F;
            }

            // scanner channel
            if let Some(offset) = self.src_offset_scanner_channel {
                let scanner_channel: u8 = src_point[offset];
                flags_byte2 |= (scanner_channel & 0x03) << 4;
            }

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
    use pasture_core::layout::{
        PointLayout,
        attributes::{
            CLASSIFICATION_FLAGS, EDGE_OF_FLIGHT_LINE, NUMBER_OF_RETURNS, RETURN_NUMBER,
            SCAN_DIRECTION_FLAG, SCANNER_CHANNEL,
        },
    };
    use pasture_io::las::ATTRIBUTE_EXTENDED_FLAGS;

    use crate::extractors::{
        AttributeExtractor, classification_flags::ClassificationFlagsExtractor,
        edge_of_flight_line::EdgeOfFlightLineExtractor, extended_flags::LasExtendedFlagsExtractor,
        number_of_returns_4bit::NumberOfReturns4BitExtractor,
        return_number_4bit::ReturnNumber4BitExtractor,
        scan_direction_flag::ScanDirectionFlagExtractor, scanner_channel::ScannerChannelExtractor,
    };

    #[derive(Debug)]
    struct ExtendedFlagsValues {
        return_number: u8,
        nr_of_returns: u8,
        classification_flags: u8,
        scanner_channel: u8,
        scan_direction_flag: u8,
        edge_of_flightline: u8,
    }

    #[test]
    fn test_extract_extended_flags() {
        fn test_case(input: ExtendedFlagsValues, expected_result: [u8; 2]) {
            let extractor = LasExtendedFlagsExtractor {
                src_offset_return_number: 0,
                src_offset_nr_of_returns: 1,
                src_offset_scan_direction_flag: 2,
                src_offset_edge_of_flightline: 3,
                src_offset_classification_flags: Some(4),
                src_offset_scanner_channel: Some(5),
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
            ExtendedFlagsValues {
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
            ExtendedFlagsValues {
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
            ExtendedFlagsValues {
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
            ExtendedFlagsValues {
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
            ExtendedFlagsValues {
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
            ExtendedFlagsValues {
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
            ExtendedFlagsValues {
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
            ExtendedFlagsValues {
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
            ExtendedFlagsValues {
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
            ExtendedFlagsValues {
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
            ExtendedFlagsValues {
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
            ExtendedFlagsValues {
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

    #[test]
    fn test_extended_flags_roundtrip() {
        fn test_case(values: ExtendedFlagsValues, extended_flags: [u8; 2]) {
            let layout_src = PointLayout::from_attributes(&[ATTRIBUTE_EXTENDED_FLAGS]);
            let layout_dst = PointLayout::from_attributes(&[
                RETURN_NUMBER,
                NUMBER_OF_RETURNS,
                CLASSIFICATION_FLAGS,
                SCANNER_CHANNEL,
                SCAN_DIRECTION_FLAG,
                EDGE_OF_FLIGHT_LINE,
            ]);

            let extr1 = ReturnNumber4BitExtractor::check(
                layout_dst.get_attribute(&RETURN_NUMBER).unwrap(),
                layout_dst.size_of_point_entry() as usize,
                &layout_src,
            )
            .unwrap();
            let extr2 = NumberOfReturns4BitExtractor::check(
                layout_dst.get_attribute(&NUMBER_OF_RETURNS).unwrap(),
                layout_dst.size_of_point_entry() as usize,
                &layout_src,
            )
            .unwrap();
            let extr3 = ScanDirectionFlagExtractor::check(
                layout_dst.get_attribute(&SCAN_DIRECTION_FLAG).unwrap(),
                layout_dst.size_of_point_entry() as usize,
                &layout_src,
            )
            .unwrap();
            let extr4 = EdgeOfFlightLineExtractor::check(
                layout_dst.get_attribute(&EDGE_OF_FLIGHT_LINE).unwrap(),
                layout_dst.size_of_point_entry() as usize,
                &layout_src,
            )
            .unwrap();
            let extr5 = ClassificationFlagsExtractor::check(
                layout_dst.get_attribute(&CLASSIFICATION_FLAGS).unwrap(),
                layout_dst.size_of_point_entry() as usize,
                &layout_src,
            )
            .unwrap();
            let extr6 = ScannerChannelExtractor::check(
                layout_dst.get_attribute(&SCANNER_CHANNEL).unwrap(),
                layout_dst.size_of_point_entry() as usize,
                &layout_src,
            )
            .unwrap();

            let src = extended_flags;
            let mut dst = [0; 6];
            extr1.extract(&src, &mut dst);
            extr2.extract(&src, &mut dst);
            extr3.extract(&src, &mut dst);
            extr4.extract(&src, &mut dst);
            extr5.extract(&src, &mut dst);
            extr6.extract(&src, &mut dst);

            let expected = [
                values.return_number,
                values.nr_of_returns,
                values.classification_flags,
                values.scanner_channel,
                values.scan_direction_flag,
                values.edge_of_flightline,
            ];
            assert_eq!(dst, expected);

            let reverse = LasExtendedFlagsExtractor::check(
                layout_src.get_attribute(&ATTRIBUTE_EXTENDED_FLAGS).unwrap(),
                layout_src.size_of_point_entry() as usize,
                &layout_dst,
            )
            .unwrap();
            let mut orig = [0, 0];
            reverse.extract(&dst, &mut orig);
            assert_eq!(orig, extended_flags);
        }

        test_case(
            ExtendedFlagsValues {
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
            ExtendedFlagsValues {
                return_number: 1,
                nr_of_returns: 15,
                classification_flags: 0x0,
                scanner_channel: 1,
                scan_direction_flag: 0,
                edge_of_flightline: 1,
            },
            [0xF1, 0x90],
        );
        test_case(
            ExtendedFlagsValues {
                return_number: 15,
                nr_of_returns: 15,
                classification_flags: 0x1,
                scanner_channel: 2,
                scan_direction_flag: 1,
                edge_of_flightline: 0,
            },
            [0xFF, 0x61],
        );
        test_case(
            ExtendedFlagsValues {
                return_number: 1,
                nr_of_returns: 1,
                classification_flags: 0xF,
                scanner_channel: 3,
                scan_direction_flag: 0,
                edge_of_flightline: 1,
            },
            [0x11, 0xBF],
        );
    }
}
