use std::collections::HashMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use ciborium::de::from_reader;
use log::{debug};
use crate::geometry::grid::{GridCell, LodLevel};
use crate::index::octree::attribute_bounds::LasPointAttributeBounds;
use crate::las::LasPointAttributes;

/// Holds attribute bounds for grid cells.
/// Elements of vector correspond to LOD levels.
/// HashMaps map grid cells to attribute bounds.
pub struct AttributeIndex {
    index: Arc<Vec<RwLock<HashMap<GridCell, LasPointAttributeBounds>>>>,
    file_name: PathBuf,
}

impl AttributeIndex {
    /// Creates a new attribute index
    pub fn new(num_lods: usize, file_name: PathBuf) -> Self {
        if let Ok(index) = Self::load_from_file(num_lods, &file_name) {
            // index exists, load it
            debug!("Loaded attribute index from file {:?}", file_name);
            return AttributeIndex {
                index,
                file_name,
            };
        } else {
            // index does not exist, create new one
            debug!("Created new attribute index at {:?}", file_name);
            let mut index = Vec::with_capacity(num_lods);
            for _ in 0..num_lods {
                index.push(RwLock::new(HashMap::new()));
            }
            AttributeIndex {
                index: Arc::new(index),
                file_name,
            }
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

    fn load_from_file(num_lods: usize, file_name: &Path) -> Result<Arc<Vec<RwLock<HashMap<GridCell, LasPointAttributeBounds>>>>, std::io::Error> {

        // check existence of file and open it
        if !file_name.exists() {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "File does not exist"));
        }
        let f = File::open(file_name)?;

        // read from file
        let decoded: Vec<HashMap<GridCell, LasPointAttributeBounds>> = from_reader(&f).expect("Error while reading attribute index");

        // convert to Vec<RwLock<HashMap<GridCell, LasPointAttributeBounds>>> and return
        let mut vector : Vec<RwLock<HashMap<GridCell, LasPointAttributeBounds>>> = Vec::with_capacity(num_lods);
        for i in 0..num_lods {
            vector.push(RwLock::new(decoded[i].clone()));
        }
        Ok(Arc::new(vector))
    }

    pub fn write_to_file(&self) -> Result<(), std::io::Error> {

        // create file
        let f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.file_name)?;

        // convert into vector without mutex and arc
        let mut vector : Vec<HashMap<GridCell, LasPointAttributeBounds>> = Vec::with_capacity(self.index.len());
        for lock in self.index.iter() {
            let index = lock.read().unwrap();
            vector.push(index.clone());
        }

        // write to file
        ciborium::ser::into_writer(&vector, &f).expect("Error while writing attribute index");
        f.sync_all()?;
        Ok(())
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
        }
    }

    #[test]
    fn test_attribute_index_update() {

        // create attribute index
        let attribute_index = AttributeIndex::new(1, PathBuf::from("test.bin"));
        let lod = LodLevel::base();
        let grid_cell = GridCell{ x: 0, y: 0, z: 0};

        // update with values
        attribute_index.update_by_attributes(lod, &grid_cell, &create_attribute_1());
        attribute_index.update_by_attributes(lod, &grid_cell, &create_attribute_2());

        // check if values are correct
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

        // delete file if it exists
        let file_name = PathBuf::from("test.bin");
        if file_name.exists() {
            std::fs::remove_file(file_name).unwrap();
        }
    }

    #[test]
    fn load_and_save() {
        // create attribute index
        let attribute_index = AttributeIndex::new(1, PathBuf::from("test.bin"));
        let lod = LodLevel::base();
        let grid_cell = GridCell{ x: 0, y: 0, z: 0};

        // update with values
        attribute_index.update_by_attributes(lod, &grid_cell, &create_attribute_1());
        attribute_index.update_by_attributes(lod, &grid_cell, &create_attribute_2());

        // write to file
        println!("Writing index to file");
        let write_result = attribute_index.write_to_file();
        assert!(write_result.is_ok());

        // read from file
        println!("Reading index from file test.bin");
        let attribute_index = AttributeIndex::new(1, PathBuf::from("test.bin"));
        println!("Index: {:?}", attribute_index.index);

        // check if values are correct
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

        // delete file
        std::fs::remove_file("test.bin").unwrap();
    }


}