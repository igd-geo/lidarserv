use crate::geometry::grid::{GridCell, LeveledGridCell, LodLevel};
use crate::lru_cache::pager::PageDirectory;
use serde_json::json;
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Directory of all existing grid cells.
/// Only keeps track of existence of grid cells, not their content.
pub struct GridCellDirectory {
    // each element of vector is a lod level
    cells: Vec<HashSet<GridCell>>,
    file_name: PathBuf,
    dirty: bool,
}

#[derive(Error, Debug)]
pub enum GridCellIoError {
    #[error("Error reading/writing file.")]
    Io,
    #[error("The file contents could not be parsed.")]
    InvalidFile,
}

impl<T> From<ciborium::ser::Error<T>> for GridCellIoError {
    fn from(e: ciborium::ser::Error<T>) -> Self {
        match e {
            ciborium::ser::Error::Io(_) => GridCellIoError::Io,
            ciborium::ser::Error::Value(_) => GridCellIoError::InvalidFile,
        }
    }
}

impl<T> From<ciborium::de::Error<T>> for GridCellIoError {
    fn from(e: ciborium::de::Error<T>) -> Self {
        match e {
            ciborium::de::Error::Io(_) => GridCellIoError::Io,
            ciborium::de::Error::Syntax(_) => GridCellIoError::InvalidFile,
            ciborium::de::Error::Semantic(_, _) => GridCellIoError::InvalidFile,
            ciborium::de::Error::RecursionLimitExceeded => GridCellIoError::InvalidFile,
        }
    }
}

impl From<std::io::Error> for GridCellIoError {
    fn from(_: std::io::Error) -> Self {
        GridCellIoError::Io
    }
}

impl GridCellDirectory {
    pub fn new(max_lod: &LodLevel, file_name: PathBuf) -> Result<Self, GridCellIoError> {
        let nr_levels = max_lod.level() as usize + 1;
        let cells = Self::load_from_file(&file_name, nr_levels)?;
        let result = GridCellDirectory {
            cells,
            file_name,
            dirty: false,
        };
        Ok(result)
    }

    fn load_from_file(
        file_name: &Path,
        nr_levels: usize,
    ) -> Result<Vec<HashSet<GridCell>>, GridCellIoError> {
        if !file_name.exists() {
            return Ok(vec![HashSet::new(); nr_levels]);
        }
        let f = File::open(file_name)?;
        let mut cells: Vec<HashSet<GridCell>> = ciborium::de::from_reader(f)?;
        while nr_levels > cells.len() {
            cells.push(HashSet::new());
        }
        Ok(cells)
    }

    pub fn write_to_file(&mut self) -> Result<(), GridCellIoError> {
        if self.dirty {
            let f = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&self.file_name)?;
            ciborium::ser::into_writer(&self.cells, &f)?;
            f.sync_all()?;
            self.dirty = false;
        }
        Ok(())
    }

    pub fn get_cells_for_lod(&self, lod: &LodLevel) -> Vec<LeveledGridCell> {
        let index = lod.level() as usize;
        self.cells[index]
            .iter()
            .map(|cell_pos| LeveledGridCell {
                lod: *lod,
                pos: *cell_pos,
            })
            .collect()
    }

    pub fn get_root_cells(&self) -> Vec<LeveledGridCell> {
        self.get_cells_for_lod(&LodLevel::base())
    }

    pub fn is_leaf_node(&self, node_id: &LeveledGridCell) -> bool {
        node_id
            .children()
            .into_iter()
            .all(|child| !self.exists(&child))
    }

    pub fn info(&self) -> serde_json::Value {
        let mut num_nodes = 0;
        let mut num_nodes_per_level = Vec::new();
        for level in 0..self.cells.len() {
            let num_nodes_at_level = self.cells[level].len();
            num_nodes += num_nodes_at_level;
            num_nodes_per_level.push(num_nodes_at_level);
        }
        json!(
        {
            "num_nodes": num_nodes,
            "num_nodes_per_level": num_nodes_per_level,
        })
    }
}

impl PageDirectory for GridCellDirectory {
    type Key = LeveledGridCell;

    fn insert(&mut self, key: &Self::Key) {
        let lod = key.lod.level() as usize;
        self.cells[lod].insert(key.pos);
        self.dirty = true;
    }

    fn exists(&self, key: &Self::Key) -> bool {
        let lod = key.lod.level() as usize;
        if lod >= self.cells.len() {
            return false;
        }
        self.cells[lod].contains(&key.pos)
    }
}
