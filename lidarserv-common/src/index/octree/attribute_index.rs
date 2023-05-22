use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};
use log::debug;
use crate::geometry::grid::{GridCell, LodLevel};
use crate::las::LasPointAttributes;

/// Holds attribute bounds for grid cells.
/// Elements of vector correspond to LOD levels.
/// HashMaps map grid cells to attribute bounds.
pub struct AttributeIndex {
    index: Arc<Vec<RwLock<HashMap<GridCell, LasPointAttributeBounds>>>>,
}

/// Defines Min and Max of all attributes.
/// Provides methods to check if a point is within the bounds and to update the bounds
#[derive(Debug, Clone, Copy)]
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

impl AttributeIndex {
    /// Creates a new attribute index
    pub fn new(num_lods: usize) -> Self {
        let mut index = Vec::with_capacity(num_lods);
        for _ in 0..num_lods {
            index.push(RwLock::new(HashMap::new()));
        }
        AttributeIndex {
            index: Arc::new(index),
        }
    }

    /// Updates attribute bounds for a grid cell by attributes
    pub fn update_by_attributes(&self, lod: LodLevel, grid_cell: &GridCell, attributes: &LasPointAttributes) {
        let bounds = LasPointAttributeBounds::from_attributes(attributes);
        self.update_by_bounds(lod, grid_cell, &bounds);
    }

    /// Updates attribute bounds for a grid cell by new bounds
    pub fn update_by_bounds(&self, lod: LodLevel, grid_cell: &GridCell, new_bounds: &LasPointAttributeBounds) {
        // aquire read lock for lod level
        // TODO Measure performance, maybe remove readlock because most times new bounds are NOT in bounds
        let index_read = self.index[lod.level() as usize].read().unwrap();
        let entry = index_read.get_key_value(&grid_cell);
        let _ = match entry {
            Some(bounds) => {
                // if new bounds are within old bounds, do nothing
                if bounds.1.is_bounds_in_bounds(new_bounds) {
                    debug!("Bounds are within old bounds, do nothing (lod {:?} cell {:?})", lod, grid_cell);
                    return;
                }
            },
            None => {},
        };

        // aquire write lock for lod level and update bounds
        drop(index_read);
        let mut index_write = self.index[lod.level() as usize].write().unwrap();
        let bounds = index_write.entry(grid_cell.clone()).or_insert(new_bounds.clone());
        bounds.update_by_bounds(new_bounds);
    }
}

impl fmt::Debug for AttributeIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, lock) in self.index.iter().enumerate() {
            let index = lock.read().unwrap();
            writeln!(f, "LOD {}", i)?;
            for (cell, bounds) in index.iter() {
                writeln!(f, "  {:?} {:?}", cell, bounds)?;
            }
        }
        write!(f, "none")
    }
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
            edge_of_flight_line: Some((attributes.edge_of_flight_line, attributes.edge_of_flight_line)),
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
    fn update_bound_by_value<T: PartialOrd + Copy>(&mut self, current_bound: Option<(T, T)>, value: T) -> Option<(T, T)> {
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
    fn update_bound_by_bound<T: PartialOrd + Copy>(&mut self, current_bound: Option<(T, T)>, new_bound: Option<(T, T)>) -> Option<(T, T)> {
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
        self.return_number = self.update_bound_by_value(self.return_number, attributes.return_number);
        self.number_of_returns = self.update_bound_by_value(self.number_of_returns, attributes.number_of_returns);
        self.scan_direction = self.update_bound_by_value(self.scan_direction, attributes.scan_direction);
        self.edge_of_flight_line = self.update_bound_by_value(self.edge_of_flight_line, attributes.edge_of_flight_line);
        self.classification = self.update_bound_by_value(self.classification, attributes.classification);
        self.scan_angle_rank = self.update_bound_by_value(self.scan_angle_rank, attributes.scan_angle_rank);
        self.user_data = self.update_bound_by_value(self.user_data, attributes.user_data);
        self.point_source_id = self.update_bound_by_value(self.point_source_id, attributes.point_source_id);
        self.gps_time = self.update_bound_by_value(self.gps_time, attributes.gps_time);
        self.color_r = self.update_bound_by_value(self.color_r, attributes.color.0);
        self.color_g = self.update_bound_by_value(self.color_g, attributes.color.1);
        self.color_b = self.update_bound_by_value(self.color_b, attributes.color.2);
    }

    /// Updates bounds with bounds of another point
    pub fn update_by_bounds(&mut self, bounds: &LasPointAttributeBounds) {
        self.intensity = self.update_bound_by_bound(self.intensity, bounds.intensity);
        self.return_number = self.update_bound_by_bound(self.return_number, bounds.return_number);
        self.number_of_returns = self.update_bound_by_bound(self.number_of_returns, bounds.number_of_returns);
        self.scan_direction = self.update_bound_by_bound(self.scan_direction, bounds.scan_direction);
        self.edge_of_flight_line = self.update_bound_by_bound(self.edge_of_flight_line, bounds.edge_of_flight_line);
        self.classification = self.update_bound_by_bound(self.classification, bounds.classification);
        self.scan_angle_rank = self.update_bound_by_bound(self.scan_angle_rank, bounds.scan_angle_rank);
        self.user_data = self.update_bound_by_bound(self.user_data, bounds.user_data);
        self.point_source_id = self.update_bound_by_bound(self.point_source_id, bounds.point_source_id);
        self.gps_time = self.update_bound_by_bound(self.gps_time, bounds.gps_time);
        self.color_r = self.update_bound_by_bound(self.color_r, bounds.color_r);
        self.color_g = self.update_bound_by_bound(self.color_g, bounds.color_g);
        self.color_b = self.update_bound_by_bound(self.color_b, bounds.color_b);
    }

    /// Check, if a value is contained in the bound
    fn is_value_in_bound<T: PartialOrd + Copy>(&self, current_bound: Option<(T, T)>, value: T) -> bool {
        match current_bound {
            Some((min_val, max_val)) => {
                value >= min_val && value <= max_val
            }
            None => false,
        }
    }

    /// Check, if a bound is contained in the bound
    fn is_bound_in_bound<T: PartialOrd + Copy>(&self, current_bound: Option<(T, T)>, new_bound: Option<(T, T)>) -> bool {
        match (current_bound, new_bound) {
            (Some((min_val, max_val)), Some((new_min, new_max))) => {
                new_min >= min_val && new_max <= max_val
            }
            (None, Some(_)) => false,
            (Some(_), None) => true,
            (None, None) => true,
        }
    }

    /// Checks, if the given attributes are contained in the bounds of this object
    pub fn is_attributes_in_bounds(&self, attributes: &LasPointAttributes) -> bool {
        self.is_value_in_bound(self.intensity, attributes.intensity) &&
            self.is_value_in_bound(self.return_number, attributes.return_number) &&
            self.is_value_in_bound(self.number_of_returns, attributes.number_of_returns) &&
            self.is_value_in_bound(self.scan_direction, attributes.scan_direction) &&
            self.is_value_in_bound(self.edge_of_flight_line, attributes.edge_of_flight_line) &&
            self.is_value_in_bound(self.classification, attributes.classification) &&
            self.is_value_in_bound(self.scan_angle_rank, attributes.scan_angle_rank) &&
            self.is_value_in_bound(self.user_data, attributes.user_data) &&
            self.is_value_in_bound(self.point_source_id, attributes.point_source_id) &&
            self.is_value_in_bound(self.gps_time, attributes.gps_time) &&
            self.is_value_in_bound(self.color_r, attributes.color.0) &&
            self.is_value_in_bound(self.color_g, attributes.color.1) &&
            self.is_value_in_bound(self.color_b, attributes.color.2)
    }

    /// Checks, if the given bounds are contained in the bounds of this object
    pub fn is_bounds_in_bounds(&self, bounds: &LasPointAttributeBounds) -> bool {
        self.is_bound_in_bound(self.intensity, bounds.intensity) &&
            self.is_bound_in_bound(self.return_number, bounds.return_number) &&
            self.is_bound_in_bound(self.number_of_returns, bounds.number_of_returns) &&
            self.is_bound_in_bound(self.scan_direction, bounds.scan_direction) &&
            self.is_bound_in_bound(self.edge_of_flight_line, bounds.edge_of_flight_line) &&
            self.is_bound_in_bound(self.classification, bounds.classification) &&
            self.is_bound_in_bound(self.scan_angle_rank, bounds.scan_angle_rank) &&
            self.is_bound_in_bound(self.user_data, bounds.user_data) &&
            self.is_bound_in_bound(self.point_source_id, bounds.point_source_id) &&
            self.is_bound_in_bound(self.gps_time, bounds.gps_time) &&
            self.is_bound_in_bound(self.color_r, bounds.color_r) &&
            self.is_bound_in_bound(self.color_g, bounds.color_g) &&
            self.is_bound_in_bound(self.color_b, bounds.color_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounds() {
        let mut bounds = LasPointAttributeBounds::new();
        let attributes = LasPointAttributes {
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
        };
        bounds.update_by_attributes(&attributes);
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

        let attributes = LasPointAttributes {
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
        };
        bounds.update_by_attributes(&attributes);
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

        let attributes2 = LasPointAttributes {
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
        };
        let bounds2 = LasPointAttributeBounds::from_attributes(&attributes2);
        assert_eq!(bounds.is_bounds_in_bounds(&bounds2), true);
        assert_eq!(bounds.is_attributes_in_bounds(&attributes2), true);

    }

    #[test]
    fn test_attribute_index_update() {
        let attribute_index = AttributeIndex::new(1);
        let lod = LodLevel::base();
        let grid_cell = GridCell{ x: 0, y: 0, z: 0};
        let attributes1 = LasPointAttributes {
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
        };

        let attributes2 = LasPointAttributes {
            intensity: 0,
            return_number: 2,
            number_of_returns: 3,
            scan_direction: false,
            edge_of_flight_line: false,
            classification: 10,
            scan_angle_rank: -100,
            user_data: 0,
            point_source_id: 1234,
            gps_time: 234.567,
            color: (0, 255, 0),
        };

        attribute_index.update_by_attributes(lod, &grid_cell, &attributes1);
        attribute_index.update_by_attributes(lod, &grid_cell, &attributes2);

        let index = &attribute_index.index[0].read().unwrap();
        let bounds = index.get(&grid_cell).unwrap();

        assert_eq!(bounds.intensity, Some((0, 10)));
        assert_eq!(bounds.return_number, Some((1, 2)));
        assert_eq!(bounds.number_of_returns, Some((1, 3)));
        assert_eq!(bounds.scan_direction, Some((false, true)));
        assert_eq!(bounds.edge_of_flight_line, Some((false, false)));
        assert_eq!(bounds.classification, Some((2, 10)));
        assert_eq!(bounds.scan_angle_rank, Some((-100, -5)));
        assert_eq!(bounds.user_data, Some((0, 0)));
        assert_eq!(bounds.point_source_id, Some((123, 1234)));
        assert_eq!(bounds.gps_time, Some((123.456, 234.567)));
        assert_eq!(bounds.color_r, Some((0, 255)));
        assert_eq!(bounds.color_g, Some((0, 255)));
        assert_eq!(bounds.color_b, Some((0, 0)));
    }

}