use crate::las::LasPointAttributes;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// Defines Min and Max of all attributes.
/// Provides methods to check if a point is within the bounds and to update the bounds
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LasPointAttributeBounds {
    pub intensity: Option<(u16, u16)>,
    pub return_number: Option<(u8, u8)>,
    pub number_of_returns: Option<(u8, u8)>,
    pub scan_direction: Option<(bool, bool)>,
    pub edge_of_flight_line: Option<(bool, bool)>,
    pub classification: Option<(u8, u8)>,
    pub scan_angle_rank: Option<(i8, i8)>,
    pub user_data: Option<(u8, u8)>,
    pub point_source_id: Option<(u16, u16)>,
    pub gps_time: Option<(f64, f64)>,
    pub color_r: Option<(u16, u16)>,
    pub color_g: Option<(u16, u16)>,
    pub color_b: Option<(u16, u16)>,
}

impl LasPointAttributeBounds {
    /// Creates a new attribute bounds
    pub fn new() -> Self {
        LasPointAttributeBounds {
            intensity: None,
            return_number: None,
            number_of_returns: None,
            scan_direction: None,
            edge_of_flight_line: None,
            classification: None,
            scan_angle_rank: None,
            user_data: None,
            point_source_id: None,
            gps_time: None,
            color_r: None,
            color_g: None,
            color_b: None,
        }
    }

    /// Creates a new attribute bounds from attributes
    pub fn from_attributes(attributes: &LasPointAttributes) -> Self {
        LasPointAttributeBounds {
            intensity: Some((attributes.intensity, attributes.intensity)),
            return_number: Some((attributes.return_number, attributes.return_number)),
            number_of_returns: Some((attributes.number_of_returns, attributes.number_of_returns)),
            scan_direction: Some((attributes.scan_direction, attributes.scan_direction)),
            edge_of_flight_line: Some((
                attributes.edge_of_flight_line,
                attributes.edge_of_flight_line,
            )),
            classification: Some((attributes.classification, attributes.classification)),
            scan_angle_rank: Some((attributes.scan_angle_rank, attributes.scan_angle_rank)),
            user_data: Some((attributes.user_data, attributes.user_data)),
            point_source_id: Some((attributes.point_source_id, attributes.point_source_id)),
            gps_time: Some((attributes.gps_time, attributes.gps_time)),
            color_r: Some((attributes.color.0, attributes.color.0)),
            color_g: Some((attributes.color.1, attributes.color.1)),
            color_b: Some((attributes.color.2, attributes.color.2)),
        }
    }

    /// Updates bounds with a single value
    fn update_bound_by_value<T: PartialOrd + Copy>(
        &mut self,
        current_bound: Option<(T, T)>,
        value: T,
    ) -> Option<(T, T)> {
        match current_bound {
            Some((min_val, max_val)) => {
                let new_min = if value < min_val { value } else { min_val };
                let new_max = if value > max_val { value } else { max_val };
                Some((new_min, new_max))
            }
            None => Some((value, value)),
        }
    }

    /// Updates bounds with a single bound
    fn update_bound_by_bound<T: PartialOrd + Copy>(
        &mut self,
        current_bound: Option<(T, T)>,
        new_bound: Option<(T, T)>,
    ) -> Option<(T, T)> {
        match (current_bound, new_bound) {
            (Some((min_val, max_val)), Some((new_min, new_max))) => {
                let new_min = if new_min < min_val { new_min } else { min_val };
                let new_max = if new_max > max_val { new_max } else { max_val };
                Some((new_min, new_max))
            }
            (None, Some((new_min, new_max))) => Some((new_min, new_max)),
            (Some((min_val, max_val)), None) => Some((min_val, max_val)),
            (None, None) => None,
        }
    }

    /// Updates bounds with attributes of a point
    pub fn update_by_attributes(&mut self, attributes: &LasPointAttributes) {
        self.intensity = self.update_bound_by_value(self.intensity, attributes.intensity);
        self.return_number =
            self.update_bound_by_value(self.return_number, attributes.return_number);
        self.number_of_returns =
            self.update_bound_by_value(self.number_of_returns, attributes.number_of_returns);
        self.scan_direction =
            self.update_bound_by_value(self.scan_direction, attributes.scan_direction);
        self.edge_of_flight_line =
            self.update_bound_by_value(self.edge_of_flight_line, attributes.edge_of_flight_line);
        self.classification =
            self.update_bound_by_value(self.classification, attributes.classification);
        self.scan_angle_rank =
            self.update_bound_by_value(self.scan_angle_rank, attributes.scan_angle_rank);
        self.user_data = self.update_bound_by_value(self.user_data, attributes.user_data);
        self.point_source_id =
            self.update_bound_by_value(self.point_source_id, attributes.point_source_id);
        self.gps_time = self.update_bound_by_value(self.gps_time, attributes.gps_time);
        self.color_r = self.update_bound_by_value(self.color_r, attributes.color.0);
        self.color_g = self.update_bound_by_value(self.color_g, attributes.color.1);
        self.color_b = self.update_bound_by_value(self.color_b, attributes.color.2);
    }

    /// Updates bounds with bounds of another point
    pub fn update_by_bounds(&mut self, bounds: &LasPointAttributeBounds) {
        self.intensity = self.update_bound_by_bound(self.intensity, bounds.intensity);
        self.return_number = self.update_bound_by_bound(self.return_number, bounds.return_number);
        self.number_of_returns =
            self.update_bound_by_bound(self.number_of_returns, bounds.number_of_returns);
        self.scan_direction =
            self.update_bound_by_bound(self.scan_direction, bounds.scan_direction);
        self.edge_of_flight_line =
            self.update_bound_by_bound(self.edge_of_flight_line, bounds.edge_of_flight_line);
        self.classification =
            self.update_bound_by_bound(self.classification, bounds.classification);
        self.scan_angle_rank =
            self.update_bound_by_bound(self.scan_angle_rank, bounds.scan_angle_rank);
        self.user_data = self.update_bound_by_bound(self.user_data, bounds.user_data);
        self.point_source_id =
            self.update_bound_by_bound(self.point_source_id, bounds.point_source_id);
        self.gps_time = self.update_bound_by_bound(self.gps_time, bounds.gps_time);
        self.color_r = self.update_bound_by_bound(self.color_r, bounds.color_r);
        self.color_g = self.update_bound_by_bound(self.color_g, bounds.color_g);
        self.color_b = self.update_bound_by_bound(self.color_b, bounds.color_b);
    }

    /// Check, if a value is contained in the bound
    fn is_value_in_bound<T: PartialOrd + Copy>(
        &self,
        current_bound: Option<(T, T)>,
        value: T,
    ) -> bool {
        match current_bound {
            Some((min_val, max_val)) => value >= min_val && value <= max_val,
            None => true,
        }
    }

    /// Check, if a bound is contained in the bound
    /// If one of the bounds is None, it is considered to be contained
    fn is_bound_in_bound<T: PartialOrd + Copy>(
        &self,
        current_bound: Option<(T, T)>,
        new_bound: Option<(T, T)>,
    ) -> bool {
        match (current_bound, new_bound) {
            (Some((min_val, max_val)), Some((new_min, new_max))) => {
                new_min >= min_val && new_max <= max_val
            }
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (None, None) => true,
        }
    }

    /// Check if a bound is overlapping with another bound
    /// If one of the bounds is None, it is considered to be overlapping
    fn is_bound_overlapping_bound<T: PartialOrd + Copy + Debug>(
        &self,
        current_bound: Option<(T, T)>,
        new_bound: Option<(T, T)>,
    ) -> bool {
        match (current_bound, new_bound) {
            (Some((min_val, max_val)), Some((new_min, new_max))) => {
                new_min <= max_val && min_val <= new_max
            }
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (None, None) => true,
        }
    }

    /// Checks, if ALL OF the given attributes are contained in the bounds of this object
    pub fn is_attributes_in_bounds(&self, attributes: &LasPointAttributes) -> bool {
        self.is_value_in_bound(self.intensity, attributes.intensity)
            && self.is_value_in_bound(self.return_number, attributes.return_number)
            && self.is_value_in_bound(self.number_of_returns, attributes.number_of_returns)
            && self.is_value_in_bound(self.scan_direction, attributes.scan_direction)
            && self.is_value_in_bound(self.edge_of_flight_line, attributes.edge_of_flight_line)
            && self.is_value_in_bound(self.classification, attributes.classification)
            && self.is_value_in_bound(self.scan_angle_rank, attributes.scan_angle_rank)
            && self.is_value_in_bound(self.user_data, attributes.user_data)
            && self.is_value_in_bound(self.point_source_id, attributes.point_source_id)
            && self.is_value_in_bound(self.gps_time, attributes.gps_time)
            && self.is_value_in_bound(self.color_r, attributes.color.0)
            && self.is_value_in_bound(self.color_g, attributes.color.1)
            && self.is_value_in_bound(self.color_b, attributes.color.2)
    }

    /// Checks, if ALL OF the given bounds are FULLY CONTAINED in the bounds of this object
    pub fn is_bounds_in_bounds(&self, bounds: &LasPointAttributeBounds) -> bool {
        self.is_bound_in_bound(self.intensity, bounds.intensity)
            && self.is_bound_in_bound(self.return_number, bounds.return_number)
            && self.is_bound_in_bound(self.number_of_returns, bounds.number_of_returns)
            && self.is_bound_in_bound(self.scan_direction, bounds.scan_direction)
            && self.is_bound_in_bound(self.edge_of_flight_line, bounds.edge_of_flight_line)
            && self.is_bound_in_bound(self.classification, bounds.classification)
            && self.is_bound_in_bound(self.scan_angle_rank, bounds.scan_angle_rank)
            && self.is_bound_in_bound(self.user_data, bounds.user_data)
            && self.is_bound_in_bound(self.point_source_id, bounds.point_source_id)
            && self.is_bound_in_bound(self.gps_time, bounds.gps_time)
            && self.is_bound_in_bound(self.color_r, bounds.color_r)
            && self.is_bound_in_bound(self.color_g, bounds.color_g)
            && self.is_bound_in_bound(self.color_b, bounds.color_b)
    }

    /// Checks, if AT LEAST ONE of the given bounds OVERLAPS with the bounds of this object
    pub fn is_bounds_overlapping_bounds(&self, bounds: &LasPointAttributeBounds) -> bool {
        self.is_bound_overlapping_bound(self.intensity, bounds.intensity)
            && self.is_bound_overlapping_bound(self.return_number, bounds.return_number)
            && self.is_bound_overlapping_bound(self.number_of_returns, bounds.number_of_returns)
            && self.is_bound_overlapping_bound(self.scan_direction, bounds.scan_direction)
            && self.is_bound_overlapping_bound(self.edge_of_flight_line, bounds.edge_of_flight_line)
            && self.is_bound_overlapping_bound(self.classification, bounds.classification)
            && self.is_bound_overlapping_bound(self.scan_angle_rank, bounds.scan_angle_rank)
            && self.is_bound_overlapping_bound(self.user_data, bounds.user_data)
            && self.is_bound_overlapping_bound(self.point_source_id, bounds.point_source_id)
            && self.is_bound_overlapping_bound(self.gps_time, bounds.gps_time)
            && self.is_bound_overlapping_bound(self.color_r, bounds.color_r)
            && self.is_bound_overlapping_bound(self.color_g, bounds.color_g)
            && self.is_bound_overlapping_bound(self.color_b, bounds.color_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::las::LasPointAttributes;

    fn attributes_1() -> LasPointAttributes {
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

    fn attributes_2() -> LasPointAttributes {
        LasPointAttributes {
            intensity: 0,
            return_number: 2,
            number_of_returns: 3,
            scan_direction: false,
            edge_of_flight_line: true,
            classification: 3,
            scan_angle_rank: 5,
            user_data: 255,
            point_source_id: 456,
            gps_time: 456.789,
            color: (0, 255, 0),
        }
    }

    fn max_bounds() -> LasPointAttributeBounds {
        let mut bounds = LasPointAttributeBounds::new();
        bounds.intensity = Some((0, 65535));
        bounds.return_number = Some((0, 255));
        bounds.number_of_returns = Some((0, 255));
        bounds.scan_direction = Some((false, true));
        bounds.edge_of_flight_line = Some((false, true));
        bounds.classification = Some((0, 255));
        bounds.scan_angle_rank = Some((-128, 127));
        bounds.user_data = Some((0, 255));
        bounds.point_source_id = Some((0, 65535));
        bounds.gps_time = Some((-1.7976931348623157e308, 1.7976931348623157e308));
        bounds.color_r = Some((0, 65535));
        bounds.color_g = Some((0, 65535));
        bounds.color_b = Some((0, 65535));
        bounds
    }

    fn smaller_bounds() -> LasPointAttributeBounds {
        let mut bounds = LasPointAttributeBounds::new();
        bounds.intensity = Some((3, 19));
        bounds.return_number = Some((0, 2));
        bounds.number_of_returns = Some((2, 4));
        bounds.scan_direction = Some((false, true));
        bounds.edge_of_flight_line = Some((false, true));
        bounds.classification = Some((1, 5));
        bounds.scan_angle_rank = Some((-22, 2));
        bounds.user_data = Some((0, 35));
        bounds.point_source_id = Some((27, 29));
        bounds.gps_time = Some((61869.3669254723, 62336.55417299696));
        bounds.color_r = Some((0, 0));
        bounds.color_g = Some((0, 0));
        bounds.color_b = Some((0, 0));
        bounds
    }

    fn not_overlapping_bounds() -> LasPointAttributeBounds {
        let mut bounds = LasPointAttributeBounds::new();
        bounds.intensity = Some((20, 65535));
        bounds.return_number = Some((0, 255));
        bounds.number_of_returns = Some((0, 255));
        bounds.scan_direction = Some((false, true));
        bounds.edge_of_flight_line = Some((false, true));
        bounds.classification = Some((30, 255));
        bounds.scan_angle_rank = Some((-128, 127));
        bounds.user_data = Some((0, 255));
        bounds.point_source_id = Some((0, 65535));
        bounds.gps_time = Some((-1.7976931348623157e308, 1.7976931348623157e308));
        bounds.color_r = Some((0, 65535));
        bounds.color_g = Some((0, 65535));
        bounds.color_b = Some((0, 65535));
        bounds
    }

    #[test]
    fn test_bounds() {
        let mut bounds = LasPointAttributeBounds::new();
        bounds.update_by_attributes(&attributes_1());
        assert_eq!(bounds.intensity, Some((10, 10)));
        assert_eq!(bounds.return_number, Some((1, 1)));
        assert_eq!(bounds.number_of_returns, Some((1, 1)));
        assert_eq!(bounds.scan_direction, Some((true, true)));
        assert_eq!(bounds.edge_of_flight_line, Some((false, false)));
        assert_eq!(bounds.classification, Some((2, 2)));
        assert_eq!(bounds.scan_angle_rank, Some((-5, -5)));
        assert_eq!(bounds.user_data, Some((0, 0)));
        assert_eq!(bounds.point_source_id, Some((123, 123)));
        assert_eq!(bounds.gps_time, Some((123.456, 123.456)));
        assert_eq!(bounds.color_r, Some((255, 255)));
        assert_eq!(bounds.color_g, Some((0, 0)));
        assert_eq!(bounds.color_b, Some((0, 0)));

        bounds.update_by_attributes(&attributes_2());
        assert_eq!(bounds.intensity, Some((0, 10)));
        assert_eq!(bounds.return_number, Some((1, 2)));
        assert_eq!(bounds.number_of_returns, Some((1, 3)));
        assert_eq!(bounds.scan_direction, Some((false, true)));
        assert_eq!(bounds.edge_of_flight_line, Some((false, true)));
        assert_eq!(bounds.classification, Some((2, 3)));
        assert_eq!(bounds.scan_angle_rank, Some((-5, 5)));
        assert_eq!(bounds.user_data, Some((0, 255)));
        assert_eq!(bounds.point_source_id, Some((123, 456)));
        assert_eq!(bounds.gps_time, Some((123.456, 456.789)));
        assert_eq!(bounds.color_r, Some((0, 255)));
        assert_eq!(bounds.color_g, Some((0, 255)));
        assert_eq!(bounds.color_b, Some((0, 0)));

        let bounds2 = LasPointAttributeBounds::from_attributes(&attributes_2());
        assert_eq!(bounds.is_bounds_in_bounds(&bounds2), true);
        assert_eq!(bounds.is_attributes_in_bounds(&attributes_2()), true);
    }

    #[test]
    fn test_bounds_in_bounds() {
        assert!(max_bounds().is_bounds_in_bounds(&smaller_bounds()));
    }

    #[test]
    fn test_overlap_single_bound() {
        let bounds = LasPointAttributeBounds::new();

        let bound1: Option<(u16, u16)> = Some((0, 10));
        let bound2: Option<(u16, u16)> = Some((5, 15));
        let bound3: Option<(u16, u16)> = Some((11, 20));

        assert_eq!(bounds.is_bound_overlapping_bound(bound1, bound2), true);
        assert_eq!(bounds.is_bound_overlapping_bound(bound1, bound3), false);
    }

    #[test]
    fn test_overlap_bounds() {
        assert_eq!(
            max_bounds().is_bounds_overlapping_bounds(&smaller_bounds()),
            true
        );
        assert_eq!(
            smaller_bounds().is_bounds_overlapping_bounds(&not_overlapping_bounds()),
            false
        );
    }
}
