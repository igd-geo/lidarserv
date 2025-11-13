use pasture_core::layout::attributes::COLOR_RGB;
use pasture_core::layout::{PointAttributeDataType, PointAttributeMember, PointLayout};

use super::AttributeExtractor;

pub struct ColorExtractor {
    src_offset: usize,
    src_stride: usize,
    dst_offset: usize,
    dst_stride: usize,
    conversion: Conversion,
}

enum Conversion {
    Copy { len: usize },
    Vec3u16ToVec3u8,
    U32ToVec3u8,
    U64ToVec3u16,
}

impl ColorExtractor {
    pub fn check(
        dst_attribute: &PointAttributeMember,
        dst_point_size: usize,
        src_layout: &PointLayout,
    ) -> Option<Self> {
        if !is_color_attribute(dst_attribute) {
            return None;
        }

        let dst_datatype = dst_attribute.datatype();
        if !matches!(
            dst_datatype,
            PointAttributeDataType::Vec3u8 | PointAttributeDataType::Vec3u16
        ) {
            return None;
        }

        let src_attribute = src_layout
            .get_attribute(dst_attribute.attribute_definition())
            .or_else(|| src_layout.get_attribute_by_name(dst_attribute.name()))
            .or_else(|| {
                let dst_name = dst_attribute.name().to_ascii_lowercase();
                src_layout
                    .attributes()
                    .find(|attr| attr.name().to_ascii_lowercase() == dst_name)
            })?;

        let conversion = match (src_attribute.datatype(), dst_datatype) {
            (PointAttributeDataType::Vec3u8, PointAttributeDataType::Vec3u8) => {
                Conversion::Copy {
                    len: dst_attribute.size() as usize,
                }
            }
            (PointAttributeDataType::Vec3u16, PointAttributeDataType::Vec3u16) => {
                Conversion::Copy {
                    len: dst_attribute.size() as usize,
                }
            }
            (PointAttributeDataType::Vec3u16, PointAttributeDataType::Vec3u8) => {
                Conversion::Vec3u16ToVec3u8
            }
            (PointAttributeDataType::U32, PointAttributeDataType::Vec3u8) => {
                Conversion::U32ToVec3u8
            }
            (PointAttributeDataType::U64, PointAttributeDataType::Vec3u16) => {
                Conversion::U64ToVec3u16
            }
            _ => return None,
        };

        Some(ColorExtractor {
            src_offset: src_attribute.byte_range_within_point().start,
            src_stride: src_layout.size_of_point_entry() as usize,
            dst_offset: dst_attribute.byte_range_within_point().start,
            dst_stride: dst_point_size,
            conversion,
        })
    }
}

impl AttributeExtractor for ColorExtractor {
    fn extract(&self, src: &[u8], dst: &mut [u8]) {
        let nr_points = src.len() / self.src_stride;
        assert_eq!(src.len(), nr_points * self.src_stride);
        assert_eq!(dst.len(), nr_points * self.dst_stride);

        match self.conversion {
            Conversion::Copy { len } => {
                for i in 0..nr_points {
                    let src_start = i * self.src_stride + self.src_offset;
                    let dst_start = i * self.dst_stride + self.dst_offset;
                    dst[dst_start..dst_start + len]
                        .copy_from_slice(&src[src_start..src_start + len]);
                }
            }
            Conversion::Vec3u16ToVec3u8 => {
                for i in 0..nr_points {
                    let src_start = i * self.src_stride + self.src_offset;
                    let dst_start = i * self.dst_stride + self.dst_offset;
                    for c in 0..3 {
                        let s = src_start + c * 2;
                        let value = u16::from_le_bytes([src[s], src[s + 1]]);
                        dst[dst_start + c] = value as u8;
                    }
                }
            }
            Conversion::U32ToVec3u8 => {
                for i in 0..nr_points {
                    let src_start = i * self.src_stride + self.src_offset;
                    let dst_start = i * self.dst_stride + self.dst_offset;
                    let bytes = [
                        src[src_start],
                        src[src_start + 1],
                        src[src_start + 2],
                        src[src_start + 3],
                    ];
                    let packed = u32::from_le_bytes(bytes);
                    dst[dst_start] = (packed & 0xFF) as u8;
                    dst[dst_start + 1] = ((packed >> 8) & 0xFF) as u8;
                    dst[dst_start + 2] = ((packed >> 16) & 0xFF) as u8;
                }
            }
            Conversion::U64ToVec3u16 => {
                for i in 0..nr_points {
                    let src_start = i * self.src_stride + self.src_offset;
                    let dst_start = i * self.dst_stride + self.dst_offset;
                    let bytes = [
                        src[src_start],
                        src[src_start + 1],
                        src[src_start + 2],
                        src[src_start + 3],
                        src[src_start + 4],
                        src[src_start + 5],
                        src[src_start + 6],
                        src[src_start + 7],
                    ];
                    let packed = u64::from_le_bytes(bytes);
                    for c in 0..3 {
                        let value = ((packed >> (c * 16)) & 0xFFFF) as u16;
                        let le = value.to_le_bytes();
                        let dst_idx = dst_start + c * 2;
                        dst[dst_idx] = le[0];
                        dst[dst_idx + 1] = le[1];
                    }
                }
            }
        }
    }
}

fn is_color_attribute(attribute: &PointAttributeMember) -> bool {
    if *attribute.attribute_definition() == COLOR_RGB {
        return true;
    }

    let name = attribute.name().to_ascii_lowercase();
    name.contains("color") || name.ends_with("rgb") || name == "rgb"
}
