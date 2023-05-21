use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use crate::geometry::grid::{GridCell, LodLevel};
use crate::las::LasPointAttributes;

/// Holds attribute bounds for grid cells
/// Elements of vector correspond to LOD levels
/// HashMaps map grid cells to attribute bounds
pub struct AttributeIndex {
    index: Arc<Vec<Mutex<HashMap<GridCell, LasPointAttributeBounds>>>>,
}

/// Defines Min and Max of all attributes
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
            index.push(Mutex::new(HashMap::new()));
        }
        AttributeIndex {
            index: Arc::new(index),
        }
    }

    /// Updates attribute bounds for a grid cell
    pub fn update(&self, lod: LodLevel, grid_cell: &GridCell, attributes: &LasPointAttributes) {
        let mut index = self.index[lod.level() as usize].lock().unwrap();
        let bounds = index.entry(grid_cell.clone()).or_insert(LasPointAttributeBounds::new());
        bounds.update(attributes);
    }
}

impl fmt::Debug for AttributeIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, mutex) in self.index.iter().enumerate() {
            let index = mutex.lock().unwrap();
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

    /// Updates bounds with attributes of a point
    pub fn update(&mut self, attributes: &LasPointAttributes) {
        self.intensity = self.update_bound(self.intensity, attributes.intensity);
        self.return_number = self.update_bound(self.return_number, attributes.return_number);
        self.number_of_returns = self.update_bound(self.number_of_returns, attributes.number_of_returns);
        self.scan_direction = self.update_bound(self.scan_direction, attributes.scan_direction);
        self.edge_of_flight_line = self.update_bound(self.edge_of_flight_line, attributes.edge_of_flight_line);
        self.classification = self.update_bound(self.classification, attributes.classification);
        self.scan_angle_rank = self.update_bound(self.scan_angle_rank, attributes.scan_angle_rank);
        self.user_data = self.update_bound(self.user_data, attributes.user_data);
        self.point_source_id = self.update_bound(self.point_source_id, attributes.point_source_id);
        self.gps_time = self.update_bound(self.gps_time, attributes.gps_time);
        self.color_r = self.update_bound(self.color_r, attributes.color.0);
        self.color_g = self.update_bound(self.color_g, attributes.color.1);
        self.color_b = self.update_bound(self.color_b, attributes.color.2);
    }

    /// Updates bounds with a single value
    fn update_bound<T: PartialOrd + Copy>(&mut self, current_bound: Option<(T, T)>, value: T) -> Option<(T, T)> {
        match current_bound {
            Some((min_val, max_val)) => {
                let new_min = if value < min_val { value } else { min_val };
                let new_max = if value > max_val { value } else { max_val };
                Some((new_min, new_max))
            }
            None => Some((value, value)),
        }
    }
}

impl fmt::Debug for LasPointAttributeBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "intensity: {:?}, ", self.intensity)?;
        write!(f, "return_number: {:?}, ", self.return_number)?;
        write!(f, "number_of_returns: {:?}, ", self.number_of_returns)?;
        write!(f, "scan_direction: {:?}, ", self.scan_direction)?;
        write!(f, "edge_of_flight_line: {:?}, ", self.edge_of_flight_line)?;
        write!(f, "classification: {:?}, ", self.classification)?;
        write!(f, "scan_angle_rank: {:?}, ", self.scan_angle_rank)?;
        write!(f, "user_data: {:?}, ", self.user_data)?;
        write!(f, "point_source_id: {:?}, ", self.point_source_id)?;
        write!(f, "gps_time: {:?}, ", self.gps_time)?;
        write!(f, "color_r: {:?}", self.color_r)?;
        write!(f, "color_g: {:?}", self.color_g)?;
        write!(f, "color_b: {:?}", self.color_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        attribute_index.update(lod, &grid_cell, &attributes1);
        attribute_index.update(lod, &grid_cell, &attributes2);

        let index = &attribute_index.index[0].lock().unwrap();
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