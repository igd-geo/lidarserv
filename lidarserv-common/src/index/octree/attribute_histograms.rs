use crate::las::LasPointAttributes;
use serde::{Deserialize, Serialize};
use crate::index::octree::histogram::Histogram;
use crate::index::octree::attribute_bounds::LasPointAttributeBounds;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LasPointAttributeHistograms {
    pub intensity: Histogram<u16>,
    pub return_number: Histogram<u8>,
    pub number_of_returns: Histogram<u8>,
    pub classification: Histogram<u8>,
    pub scan_angle_rank: Histogram<i8>,
    pub user_data: Histogram<u8>,
    pub point_source_id: Histogram<u16>,
    pub color_r: Histogram<u16>,
    pub color_g: Histogram<u16>,
    pub color_b: Histogram<u16>,
}

impl LasPointAttributeHistograms {

    pub fn new() -> Self {
        let bin_count_8bit = 25;
        let bin_count_16bit = 25;
        Self {
            intensity: Histogram::<u16>::new(0,65535,bin_count_16bit),
            return_number: Histogram::<u8>::new(0,7, 8),
            number_of_returns: Histogram::<u8>::new(0,7,8),
            classification: Histogram::<u8>::new(0,255,bin_count_8bit),
            scan_angle_rank: Histogram::<i8>::new(-90,90,bin_count_8bit),
            user_data: Histogram::<u8>::new(0,255,bin_count_8bit),
            point_source_id: Histogram::<u16>::new(0,65535,bin_count_16bit),
            color_r: Histogram::<u16>::new(0,65535,bin_count_16bit),
            color_g: Histogram::<u16>::new(0,65535,bin_count_16bit),
            color_b: Histogram::<u16>::new(0,65535,bin_count_16bit),
        }
    }

    pub fn fill_with(&mut self, attributes: &LasPointAttributes) {
        self.intensity.add(attributes.intensity);
        self.return_number.add(attributes.return_number);
        self.number_of_returns.add(attributes.number_of_returns);
        self.classification.add(attributes.classification);
        self.scan_angle_rank.add(attributes.scan_angle_rank);
        self.user_data.add(attributes.user_data);
        self.point_source_id.add(attributes.point_source_id);
        self.color_r.add(attributes.color.0);
        self.color_g.add(attributes.color.1);
        self.color_b.add(attributes.color.2);
    }

    pub fn add_histograms(&mut self, other: &LasPointAttributeHistograms) {
        self.intensity.add_histogram(&other.intensity);
        self.return_number.add_histogram(&other.return_number);
        self.number_of_returns.add_histogram(&other.number_of_returns);
        self.classification.add_histogram(&other.classification);
        self.scan_angle_rank.add_histogram(&other.scan_angle_rank);
        self.user_data.add_histogram(&other.user_data);
        self.point_source_id.add_histogram(&other.point_source_id);
        self.color_r.add_histogram(&other.color_r);
        self.color_g.add_histogram(&other.color_g);
        self.color_b.add_histogram(&other.color_b);
    }

    pub fn is_attribute_range_in_histograms(&self, attribute_bounds: &LasPointAttributeBounds) -> bool {
        self.intensity.range_contains_values(attribute_bounds.intensity.unwrap()) &&
        self.return_number.range_contains_values(attribute_bounds.return_number.unwrap()) &&
        self.number_of_returns.range_contains_values(attribute_bounds.number_of_returns.unwrap()) &&
        self.classification.range_contains_values(attribute_bounds.classification.unwrap()) &&
        self.scan_angle_rank.range_contains_values(attribute_bounds.scan_angle_rank.unwrap()) &&
        self.user_data.range_contains_values(attribute_bounds.user_data.unwrap()) &&
        self.point_source_id.range_contains_values(attribute_bounds.point_source_id.unwrap()) &&
        self.color_r.range_contains_values(attribute_bounds.color_r.unwrap()) &&
        self.color_g.range_contains_values(attribute_bounds.color_g.unwrap()) &&
        self.color_b.range_contains_values(attribute_bounds.color_b.unwrap())
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creation() {
        let histograms = LasPointAttributeHistograms::new();
    }

}