use pasture_core::layout::{
    PointAttributeDataType, PointAttributeMember, PointLayout, attributes::EDGE_OF_FLIGHT_LINE,
};
use pasture_io::las::{ATTRIBUTE_BASIC_FLAGS, ATTRIBUTE_EXTENDED_FLAGS};

use super::AttributeExtractor;

pub struct EdgeOfFlightLineExtractor {
    src_stride: usize,
    src_offset: usize,
    dst_stride: usize,
    dst_offset: usize,
}

impl EdgeOfFlightLineExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self> {
        if *dst_attribute.attribute_definition() != EDGE_OF_FLIGHT_LINE {
            return None;
        }
        assert_eq!(dst_attribute.datatype(), PointAttributeDataType::U8);

        let src_offset =
            if let Some(src_attr_basic_flags) = src_layout.get_attribute(&ATTRIBUTE_BASIC_FLAGS) {
                assert_eq!(src_attr_basic_flags.datatype(), PointAttributeDataType::U8);
                src_attr_basic_flags.byte_range_within_point().start
            } else if let Some(src_attr_extended_flags) =
                src_layout.get_attribute(&ATTRIBUTE_EXTENDED_FLAGS)
            {
                assert_eq!(
                    src_attr_extended_flags.datatype(),
                    PointAttributeDataType::U16
                );
                src_attr_extended_flags.byte_range_within_point().start + 1
            } else {
                return None;
            };

        Some(EdgeOfFlightLineExtractor {
            src_stride: src_layout.size_of_point_entry() as usize,
            src_offset,
            dst_stride: dst_point_size,
            dst_offset: dst_attribute.byte_range_within_point().start,
        })
    }
}

impl AttributeExtractor for EdgeOfFlightLineExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert!(src.len() == nr_points * self.src_stride);
        assert!(dst.len() == nr_points * self.dst_stride);

        for i in 0..nr_points {
            let src_pos = i * self.src_stride + self.src_offset;
            let dst_pos = i * self.dst_stride + self.dst_offset;

            let basic_flags = src[src_pos];
            let edge_of_flight_line = (basic_flags & 0x80) >> 7;
            dst[dst_pos] = edge_of_flight_line;
        }
    }
}

#[cfg(test)]
mod test {
    use crate::extractors::{AttributeExtractor, edge_of_flight_line::EdgeOfFlightLineExtractor};

    #[test]
    pub fn test_edge_of_flightline_extractor() {
        let extractor = EdgeOfFlightLineExtractor {
            src_stride: 1,
            src_offset: 0,
            dst_stride: 1,
            dst_offset: 0,
        };
        let src = [0x01, 0x82, 0x04, 0x88, 0x10, 0xA0, 0x40, 0x80];
        let mut dst = [0; 8];
        extractor.extract(&src, &mut dst);
        assert_eq!(dst, [0x0, 0x1, 0x0, 0x1, 0x0, 0x1, 0x0, 0x1]);
    }
}
