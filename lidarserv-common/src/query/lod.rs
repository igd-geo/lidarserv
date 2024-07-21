use pasture_core::containers::BorrowedBuffer;
use serde::{Deserialize, Serialize};

use crate::geometry::grid::LodLevel;

use super::{NodeQueryResult, Query};

/// Query that matches everything up to a certen level of detail.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueryLod {
    max_lod: LodLevel,
}

impl Query for QueryLod {
    fn matches_node(&self, node: crate::geometry::grid::LeveledGridCell) -> NodeQueryResult {
        if node.lod <= self.max_lod {
            NodeQueryResult::Positive
        } else {
            NodeQueryResult::Negative
        }
    }

    fn matches_points(
        &self,
        lod: LodLevel,
        points: &pasture_core::containers::VectorBuffer,
    ) -> Vec<bool> {
        if lod <= self.max_lod {
            vec![true; points.len()]
        } else {
            vec![false; points.len()]
        }
    }
}
