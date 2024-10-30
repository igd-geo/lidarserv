use std::path::PathBuf;

use pasture_core::layout::PointAttributeDefinition;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AttributeIndexConfig {
    pub attribute: PointAttributeDefinition,
    pub path: PathBuf,
    pub index: IndexKind,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum IndexKind {
    RangeIndex,
    SfcIndex(SfcIndexOptions),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SfcIndexOptions {
    pub nr_bins: usize,
}
