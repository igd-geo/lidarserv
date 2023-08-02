use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use log::{debug, info, trace};
use csv::Writer;
use crate::geometry::grid::{GridCell, LodLevel};
use crate::index::octree::attribute_bounds::LasPointAttributeBounds;
use crate::index::octree::attribute_histograms::LasPointAttributeHistograms;
use crate::las::LasPointAttributes;

/// Vec for LOD levels
/// RwLock for concurrent access
/// HashMap holds grid cells
/// (LasPointAttributeBounds, Option<LasPointAttributeHistograms>) holds attribute bounds and histograms for each cell
type Index = Vec<RwLock<HashMap<GridCell, (LasPointAttributeBounds, Option<LasPointAttributeHistograms>)>>>;

/// Holds attribute bounds for grid cells.
/// Elements of vector correspond to LOD levels.
/// HashMaps map grid cells to attribute bounds.
pub struct AttributeIndex {
    index: Arc<Index>,
    enable_histograms: bool,
    file_name: PathBuf,
    dirty: Arc<RwLock<bool>>,
}

impl AttributeIndex {
    /// Creates a new attribute index
    /// If an index file (file_name) exists, it is loaded, otherwise a new one is created
    pub fn new(num_lods: usize, file_name: PathBuf) -> Self {
        if let Ok(index) = Self::load_from_file(num_lods + 1, &file_name) {
            // index exists, load it
            debug!("Loaded attribute index from file {:?}", file_name);
            return AttributeIndex {
                index,
                enable_histograms: false,
                file_name,
                dirty: Arc::new(RwLock::new(false))
            };
        } else {
            // index does not exist, create new one
            debug!("Created new attribute index at {:?}", file_name);
            let mut index = Vec::with_capacity(num_lods + 1);
            for _ in 0..num_lods+1 {
                index.push(RwLock::new(HashMap::new()));
            }
            AttributeIndex {
                index: Arc::new(index),
                enable_histograms: false,
                file_name,
                dirty: Arc::new(RwLock::new(true))
            }
        }
    }

    /// Checks, if index has been updated since last save
    pub fn is_dirty(&self) -> bool {
        *self.dirty.read().unwrap()
    }

    /// Sets dirty flag of index
    pub fn set_dirty(&self, dirty: bool) {
        *self.dirty.write().unwrap() = dirty;
    }

    /// sets the histogram acceleration flag
    pub fn set_histogram_acceleration(&mut self, enable: bool) {
        self.enable_histograms = enable;
        self.set_dirty(true);
    }

    /// Updates attribute bounds and histograms for a grid cell using new bounds and histograms
    pub fn update_bounds_and_histograms(
        &self,
        lod: LodLevel,
        grid_cell: &GridCell,
        new_bounds: &LasPointAttributeBounds,
        new_histogram: &Option<LasPointAttributeHistograms>)
    {
        // aquire write lock for lod level
        let mut index_write = self.index[lod.level() as usize].write().unwrap();
        let (bounds, histogram) = index_write.entry(grid_cell.clone()).or_insert((new_bounds.clone(), new_histogram.clone()));

        // update bounds and optionally histograms
        bounds.update_by_bounds(&new_bounds);
        if new_histogram.is_some() && self.enable_histograms {
            if histogram.is_none() {
                debug!("Creating new histogram for cell {:?}", grid_cell);
                debug!("New histogram: {:?}", new_histogram);
                *histogram = new_histogram.clone();
            } else {
                debug!("Updating histogram for cell {:?}", grid_cell);
                debug!("Old histogram: {:?}", histogram);
                debug!("New histogram: {:?}", new_histogram);
                histogram.as_mut().unwrap().add_histograms(&new_histogram.as_ref().unwrap());
            }
        }
        self.set_dirty(true);
    }

    /// Updates attribute bounds for a grid cell by attributes
    pub fn update_by_attributes(&mut self, lod: LodLevel, grid_cell: &GridCell, attributes: &LasPointAttributes) {
        let bounds = LasPointAttributeBounds::from_attributes(attributes);
        self.update_bounds_and_histograms(lod, grid_cell, &bounds, &None);
    }

    /// Checks if a grid cell OVERLAPS with the given attribute bounds
    /// Also checks the histogram, if enabled
    pub fn cell_overlaps_with_bounds(&self, lod: LodLevel, grid_cell: &GridCell, bounds: &LasPointAttributeBounds, check_histogram: bool) -> bool {
        // aquire read lock for lod level
        let index_read = self.index[lod.level() as usize].read().unwrap();
        let entry = index_read.get_key_value(&grid_cell);

        // check if cell is in bounds
        let _ = match entry {
            Some((_, (cell_bounds, histograms))) => {
                // check bounds
                let is_in_bounds = bounds.is_bounds_overlapping_bounds(&cell_bounds);
                trace!("Cell {:?} overlaps with bounds: {}", grid_cell, is_in_bounds);

                // also check histograms if enabled
                return if is_in_bounds && check_histogram && histograms.is_some() {
                    let histogram_check = histograms.as_ref().unwrap().is_attribute_range_in_histograms(bounds);
                    trace!("Cell {:?} overlaps with histogram: {}", grid_cell, histogram_check);
                    if !histogram_check {
                        trace!("Cell {:?} overlaps with bounds, but not with histogram", grid_cell);
                    }
                    histogram_check
                } else {
                    trace!("Returning: {}", is_in_bounds);
                    is_in_bounds
                }
            },
            None => {
                true
            },
        };
        false
    }

    /// Loads attribute index from file
    fn load_from_file(num_lods: usize, file_name: &Path) -> Result<Arc<Index>, std::io::Error> {

        // check existence of file and open it
        if !file_name.exists() {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "File does not exist"));
        }
        debug!("Loading attribute index from file {:?}", file_name);
        let f = File::open(file_name)?;

        // read from file using bincode
        debug!("Decoding attribute index file");
        let decoded: Vec<HashMap<GridCell, (LasPointAttributeBounds, Option<LasPointAttributeHistograms>)>> =
            bincode::deserialize_from(&f).expect("Error while reading attribute index");

        // convert to Vec<RwLock<HashMap<GridCell, (LasPointAttributeBounds, LasPointAttributeHistograms)>>> and return
        debug!("Converting attribute index to vector");
        let mut vector : Index = Vec::with_capacity(num_lods);
        for i in 0..num_lods {
            vector.push(RwLock::new(decoded[i].clone()));
        }
        Ok(Arc::new(vector))
    }

    /// Writes attribute index to file
    pub fn write_to_file(&self) -> Result<(), std::io::Error> {

        // create file
        debug!("Writing attribute index to file {:?}", self.file_name);
        let f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.file_name)?;

        // convert into vector without mutex and arc
        debug!("Converting attribute index to vector");
        let mut vector : Vec<HashMap<GridCell, (LasPointAttributeBounds, Option<LasPointAttributeHistograms>)>>
            = Vec::with_capacity(self.index.len());
        for lock in self.index.iter() {
            let index = lock.read().unwrap();
            vector.push(index.clone());
        }

        // write to file with bincode
        debug!("Writing file");
        bincode::serialize_into(&f, &vector).expect("Error while writing attribute index");
        f.sync_all()?;

        // DEBUG CSV OUTPUT
        // self.write_to_csv().unwrap();

        Ok(())
    }

    /// Writes attribute index to file if it is dirty
    pub fn write_to_file_if_dirty(&self) -> Result<(), std::io::Error> {
        if self.is_dirty() {
            self.write_to_file()?;
            self.set_dirty(false)
        } else {
            debug!("Attribute index is not dirty, not writing to file");
        }
        Ok(())
    }

    /// Returns the size of the index in bytes
    pub fn size(&self) -> usize {
        let mut size = std::mem::size_of_val(&self.index);
        for lock in self.index.iter() {
            let index = lock.read().unwrap();
            size += index.len() * std::mem::size_of::<GridCell>();
            size += index.len() * std::mem::size_of::<LasPointAttributeBounds>();
            size += index.len() * std::mem::size_of::<LasPointAttributeHistograms>();
        }
        size
    }

    /// Return bounds of a grid cell
    pub fn get_cell_bounds(&self, lod: LodLevel, grid_cell: &GridCell) -> Option<LasPointAttributeBounds> {
        let index_read = self.index[lod.level() as usize].read().unwrap();
        let entry = index_read.get_key_value(&grid_cell);
        match entry {
            Some(bounds) => {
                Some(bounds.1.0.clone())
            },
            None => {
                None
            },
        }
    }

    /// Writes attribute index to human readable file (for debugging)
    pub fn write_to_csv(&self) -> Result<(), std::io::Error> {

        // delete file
        if Path::new("attribute_index.csv").exists() {
            std::fs::remove_file("attribute_index.csv")?;
        }

        // create writer
        let mut wtr = Writer::from_path("attribute_index.csv")?;
        wtr.write_record(&[
            "lod",
            "x",
            "y",
            "z",
            "intensity_min",
            "intensity_max",
            "return_number_min",
            "return_number_max",
            "number_of_returns_min",
            "number_of_returns_max",
            "scan_direction_min",
            "scan_direction_max",
            "edge_of_flight_line_min",
            "edge_of_flight_line_max",
            "classification_min",
            "classification_max",
            "scan_angle_rank_min",
            "scan_angle_rank_max",
            "user_data_min",
            "user_data_max",
            "point_source_id_min",
            "point_source_id_max",
            "gps_time_min",
            "gps_time_max",
            "color_r_min",
            "color_r_max",
            "color_g_min",
            "color_g_max",
            "color_b_min",
            "color_b_max",
        ])?;

        // write to file
        for (lod, lock) in self.index.iter().enumerate() {
            let index = lock.read().unwrap();
            for (grid_cell, bounds) in index.iter() {
                wtr.write_record(&[
                    lod.to_string(),
                    grid_cell.x.to_string(),
                    grid_cell.y.to_string(),
                    grid_cell.z.to_string(),
                    bounds.0.intensity.unwrap().0.to_string(),
                    bounds.0.intensity.unwrap().1.to_string(),
                    bounds.0.return_number.unwrap().0.to_string(),
                    bounds.0.return_number.unwrap().1.to_string(),
                    bounds.0.number_of_returns.unwrap().0.to_string(),
                    bounds.0.number_of_returns.unwrap().1.to_string(),
                    bounds.0.scan_direction.unwrap().0.to_string(),
                    bounds.0.scan_direction.unwrap().1.to_string(),
                    bounds.0.edge_of_flight_line.unwrap().0.to_string(),
                    bounds.0.edge_of_flight_line.unwrap().1.to_string(),
                    bounds.0.classification.unwrap().0.to_string(),
                    bounds.0.classification.unwrap().1.to_string(),
                    bounds.0.scan_angle_rank.unwrap().0.to_string(),
                    bounds.0.scan_angle_rank.unwrap().1.to_string(),
                    bounds.0.user_data.unwrap().0.to_string(),
                    bounds.0.user_data.unwrap().1.to_string(),
                    bounds.0.point_source_id.unwrap().0.to_string(),
                    bounds.0.point_source_id.unwrap().1.to_string(),
                    bounds.0.gps_time.unwrap().0.to_string(),
                    bounds.0.gps_time.unwrap().1.to_string(),
                    bounds.0.color_r.unwrap().0.to_string(),
                    bounds.0.color_r.unwrap().1.to_string(),
                    bounds.0.color_g.unwrap().0.to_string(),
                    bounds.0.color_g.unwrap().1.to_string(),
                    bounds.0.color_b.unwrap().0.to_string(),
                    bounds.0.color_b.unwrap().1.to_string(),
                ])?;
            }
        }
        wtr.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, thread};
    use std::time::Duration;
    use crate::index::octree::attribute_histograms::HistogramSettings;
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
            scan_angle_rank: -90,
            user_data: 0,
            point_source_id: 1234,
            gps_time: 234.567,
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
        bounds.scan_angle_rank = Some((-90, 127));
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
        bounds.scan_angle_rank = Some((-90, 127));
        bounds.user_data = Some((0, 255));
        bounds.point_source_id = Some((0, 65535));
        bounds.gps_time = Some((-1.7976931348623157e308, 1.7976931348623157e308));
        bounds.color_r = Some((0, 65535));
        bounds.color_g = Some((0, 65535));
        bounds.color_b = Some((0, 65535));
        bounds
    }

    fn delete_file(path: &PathBuf) {
        if path.exists() {
            fs::remove_file(path).unwrap();
            thread::sleep(Duration::from_millis(100));
        }
    }

    #[test]
    fn test_attribute_index_update() {

        // delete file if exists
        delete_file(&PathBuf::from("test.bin"));

        // create attribute index
        let mut attribute_index = AttributeIndex::new(1, PathBuf::from("test.bin"));
        let lod = LodLevel::base();
        let grid_cell = GridCell{ x: 0, y: 0, z: 0};

        // update with values
        attribute_index.update_by_attributes(lod, &grid_cell, &create_attribute_1());
        attribute_index.update_by_attributes(lod, &grid_cell, &create_attribute_2());

        // check if values are correct
        let index = &attribute_index.index[0].read().unwrap();
        let bounds = index.get(&grid_cell).unwrap();
        assert_eq!(bounds.0.intensity, Some((0, 10)));
        assert_eq!(bounds.0.return_number, Some((1, 2)));
        assert_eq!(bounds.0.number_of_returns, Some((1, 3)));
        assert_eq!(bounds.0.scan_direction, Some((false, true)));
        assert_eq!(bounds.0.edge_of_flight_line, Some((false, false)));
        assert_eq!(bounds.0.classification, Some((2, 10)));
        assert_eq!(bounds.0.scan_angle_rank, Some((-90, -5)));
        assert_eq!(bounds.0.user_data, Some((0, 0)));
        assert_eq!(bounds.0.point_source_id, Some((123, 1234)));
        assert_eq!(bounds.0.gps_time, Some((123.456, 234.567)));
        assert_eq!(bounds.0.color_r, Some((0, 255)));
        assert_eq!(bounds.0.color_g, Some((0, 255)));
        assert_eq!(bounds.0.color_b, Some((0, 0)));

        attribute_index.size();

        // delete file if exists
        delete_file(&PathBuf::from("test.bin"));
    }

    #[test]
    fn load_and_save() {
        // delete file if exists
        delete_file(&PathBuf::from("test.bin"));

        // create attribute index
        println!("Creating attribute index");
        let mut attribute_index = AttributeIndex::new(1, PathBuf::from("test.bin"));
        let lod = LodLevel::base();
        let grid_cell = GridCell{ x: 0, y: 0, z: 0};

        // creating bounds and histograms
        println!("Creating bounds and histograms");
        let mut bounds = LasPointAttributeBounds::new();
        bounds.update_by_attributes(&create_attribute_1());
        bounds.update_by_attributes(&create_attribute_2());

        let mut histograms = LasPointAttributeHistograms::new(&HistogramSettings::default());
        histograms.fill_with(&create_attribute_1());
        histograms.fill_with(&create_attribute_2());

        // Updating bounds and histograms
        println!("Updating attribute index");
        attribute_index.update_bounds_and_histograms(lod, &grid_cell, &bounds, &Some(histograms));

        // write to file
        println!("Writing attribute index to file");
        let write_result = attribute_index.write_to_file();
        assert!(write_result.is_ok());

        // read from file
        println!("Reading attribute index from file");
        let attribute_index = AttributeIndex::new(1, PathBuf::from("test.bin"));

        // extract bounds and histograms
        println!("Checking attribute index values");
        let index = &attribute_index.index[0].read().unwrap();
        let (bounds, histograms) = index.get(&grid_cell).unwrap();

        // check if bounds are correct
        assert_eq!(bounds.intensity, Some((0, 10)));
        assert_eq!(bounds.return_number, Some((1, 2)));
        assert_eq!(bounds.number_of_returns, Some((1, 3)));
        assert_eq!(bounds.scan_direction, Some((false, true)));
        assert_eq!(bounds.edge_of_flight_line, Some((false, false)));
        assert_eq!(bounds.classification, Some((2, 10)));
        assert_eq!(bounds.scan_angle_rank, Some((-90, -5)));
        assert_eq!(bounds.user_data, Some((0, 0)));
        assert_eq!(bounds.point_source_id, Some((123, 1234)));
        assert_eq!(bounds.gps_time, Some((123.456, 234.567)));
        assert_eq!(bounds.color_r, Some((0, 255)));
        assert_eq!(bounds.color_g, Some((0, 255)));
        assert_eq!(bounds.color_b, Some((0, 0)));

        // check if histograms are correct
        assert!(histograms.is_some());
        let histograms = histograms.as_ref().unwrap();
        assert!(histograms.is_attribute_range_in_histograms(&bounds));

        // delete file if exists
        delete_file(&PathBuf::from("test.bin"));
    }

    #[test]
    fn overlap() {
        // delete file if exists
        delete_file(&PathBuf::from("test.bin"));

        let mut attribute_index = AttributeIndex::new(1, PathBuf::from("test.bin"));
        let lod = LodLevel::base();
        let grid_cell = GridCell{ x: 0, y: 0, z: 0};

        // update with values
        attribute_index.update_bounds_and_histograms(lod, &grid_cell, &smaller_bounds(), &None);

        // check if values are correct
        assert_eq!(attribute_index.cell_overlaps_with_bounds(lod, &grid_cell, &smaller_bounds(), false), true);
        assert_eq!(attribute_index.cell_overlaps_with_bounds(lod, &grid_cell, &max_bounds(), false), true);
        assert_eq!(attribute_index.cell_overlaps_with_bounds(lod, &grid_cell, &not_overlapping_bounds(), false), false);

        // delete file if exists
        delete_file(&PathBuf::from("test.bin"));
    }


}