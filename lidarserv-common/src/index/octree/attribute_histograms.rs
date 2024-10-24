use crate::las::LasPointAttributes;
use serde::{Deserialize, Serialize};
use crate::index::octree::histogram::Histogram;
use crate::index::octree::attribute_bounds::LasPointAttributeBounds;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistogramSettings {
    pub bin_count_intensity: usize,
    pub bin_count_return_number: usize,
    pub bin_count_classification: usize,
    pub bin_count_scan_angle_rank: usize,
    pub bin_count_user_data: usize,
    pub bin_count_point_source_id: usize,
    pub bin_count_color: usize,
}

impl Default for HistogramSettings {
    fn default() -> Self {
        Self {
            bin_count_intensity: 25,
            bin_count_return_number: 8,
            bin_count_classification: 256,
            bin_count_scan_angle_rank: 25,
            bin_count_user_data: 25,
            bin_count_point_source_id: 25,
            bin_count_color: 25,
        }
    }
}

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
    /// Creates a new histogram with the given settings.
    /// Bounds are set to the values from the LAS specification.
    pub fn new(settings: &HistogramSettings) -> Self {
        Self {
            intensity: Histogram::<u16>::new(0,65535,settings.bin_count_intensity),
            return_number: Histogram::<u8>::new(0,7, settings.bin_count_return_number),
            number_of_returns: Histogram::<u8>::new(0,7,settings.bin_count_return_number),
            classification: Histogram::<u8>::new(0,255,settings.bin_count_classification),
            scan_angle_rank: Histogram::<i8>::new(-128,127,settings.bin_count_scan_angle_rank),
            user_data: Histogram::<u8>::new(0,255,settings.bin_count_user_data),
            point_source_id: Histogram::<u16>::new(0,65535,settings.bin_count_point_source_id),
            color_r: Histogram::<u16>::new(0,65535,settings.bin_count_color),
            color_g: Histogram::<u16>::new(0,65535,settings.bin_count_color),
            color_b: Histogram::<u16>::new(0,65535,settings.bin_count_color),
        }
    }

    /// Inserts the attributes of a point into the histograms.
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

    /// Adds the values of another histogram to this one.
    /// All histograms must have the same bin count and bounds.
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

    /// Returns true if the given attribute bounds are contained in the histograms.
    /// If the attribute bounds are None, the corresponding attribute is ignored.
    pub fn is_attribute_range_in_histograms(&self, attribute_bounds: &LasPointAttributeBounds) -> bool {
        self.intensity.range_contains_values(attribute_bounds.intensity.unwrap_or((0,65535))) &&
        self.return_number.range_contains_values(attribute_bounds.return_number.unwrap_or((0,7))) &&
        self.number_of_returns.range_contains_values(attribute_bounds.number_of_returns.unwrap_or((0,7))) &&
        self.classification.range_contains_values(attribute_bounds.classification.unwrap_or((0,255))) &&
        self.scan_angle_rank.range_contains_values(attribute_bounds.scan_angle_rank.unwrap_or((-90,90))) &&
        self.user_data.range_contains_values(attribute_bounds.user_data.unwrap_or((0,255))) &&
        self.point_source_id.range_contains_values(attribute_bounds.point_source_id.unwrap_or((0,65535))) &&
        self.color_r.range_contains_values(attribute_bounds.color_r.unwrap_or((0,65535))) &&
        self.color_g.range_contains_values(attribute_bounds.color_g.unwrap_or((0,65535))) &&
        self.color_b.range_contains_values(attribute_bounds.color_b.unwrap_or((0,65535)))
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_attribute_1() -> LasPointAttributes {
        LasPointAttributes {
            intensity: 10,
            return_number: 1,
            number_of_returns: 1,
            scan_direction: true,
            edge_of_flight_line: false,
            classification: 2,
            scan_angle_rank: -5,
            user_data: 0,
            point_source_id: 123,
            gps_time: 123.456,
            color: (255, 0, 0),
        }
    }

    fn create_attribute_2() -> LasPointAttributes {
        LasPointAttributes {
            intensity: 100,
            return_number: 2,
            number_of_returns: 2,
            scan_direction: false,
            edge_of_flight_line: true,
            classification: 3,
            scan_angle_rank: 5,
            user_data: 255,
            point_source_id: 456,
            gps_time: 789.123,
            color: (0, 255, 0),
        }
    }

    #[test]
    fn test_creation() {
        let _histograms = LasPointAttributeHistograms::new(&HistogramSettings::default());
    }

    #[test]
    fn test_fill_with() {

        // create histograms
        let settings = HistogramSettings {
            bin_count_intensity: 10,
            bin_count_return_number: 8,
            bin_count_classification: 256,
            bin_count_scan_angle_rank: 10,
            bin_count_user_data: 10,
            bin_count_point_source_id: 10,
            bin_count_color: 10,
        };
        let mut histograms = LasPointAttributeHistograms::new(&settings);

        // fill with attributes
        histograms.fill_with(&create_attribute_1());

        // check correct creation
        assert_eq!(histograms.intensity.num_bins(), 10);
        assert_eq!(histograms.return_number.num_bins(), 8);
        assert_eq!(histograms.number_of_returns.num_bins(), 8);
        assert_eq!(histograms.classification.num_bins(), 256);
        assert_eq!(histograms.scan_angle_rank.num_bins(), 10);
        assert_eq!(histograms.user_data.num_bins(), 10);
        assert_eq!(histograms.point_source_id.num_bins(), 10);
        assert_eq!(histograms.color_r.num_bins(), 10);
        assert_eq!(histograms.color_g.num_bins(), 10);
        assert_eq!(histograms.color_b.num_bins(), 10);

        // check correct binning
        assert_eq!(histograms.intensity.get_bin_index(10), 0);
        assert_eq!(histograms.classification.get_bin_index(0), 0);
        assert_eq!(histograms.classification.get_bin_index(42), 42);
        assert_eq!(histograms.classification.get_bin_index(255), 255);

        // check correct filling
        let attribute_1_bounds = LasPointAttributeBounds::from_attributes(&create_attribute_1());
        let attribute_2_bounds = LasPointAttributeBounds::from_attributes(&create_attribute_2());
        assert!(histograms.is_attribute_range_in_histograms(&attribute_1_bounds));
        assert!(!histograms.is_attribute_range_in_histograms(&attribute_2_bounds));
        assert_eq!(histograms.intensity.get_bin_count(0), Some(1));
        assert_eq!(histograms.intensity.get_bin_count(1), Some(0));

        // add histograms
        let histograms_2 = histograms.clone();
        histograms.add_histograms(&histograms_2);

        // check correct addition
        assert_eq!(histograms.intensity.get_bin_count(0), Some(2));
        assert_eq!(histograms.classification.get_bin_count(2), Some(2));
    }

}