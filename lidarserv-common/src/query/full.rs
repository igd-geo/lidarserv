use pasture_core::containers::BorrowedBuffer;
use serde::{Deserialize, Serialize};

use crate::geometry::grid::LodLevel;

use super::{NodeQueryResult, Query};

/// Query that matches everything
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueryFull;

impl Query for QueryFull {
    fn matches_node(&self, _node: crate::geometry::grid::LeveledGridCell) -> NodeQueryResult {
        NodeQueryResult::Positive
    }

    fn matches_points(
        &self,
        _lod: LodLevel,
        points: &pasture_core::containers::VectorBuffer,
    ) -> Vec<bool> {
        vec![true; points.len()]
    }
}
