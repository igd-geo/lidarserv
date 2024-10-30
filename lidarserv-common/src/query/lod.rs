use std::convert::Infallible;

use super::{ExecutableQuery, NodeQueryResult, Query};
use crate::geometry::grid::LodLevel;
use pasture_core::containers::BorrowedBuffer;
use serde::{Deserialize, Serialize};

/// Query that matches everything up to a certen level of detail.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LodQuery(pub LodLevel);

impl Query for LodQuery {
    type Executable = Self;
    type Error = Infallible;

    fn prepare(self, _ctx: &super::QueryContext) -> Result<Self::Executable, Self::Error> {
        Ok(self)
    }
}

impl ExecutableQuery for LodQuery {
    fn matches_node(&self, node: crate::geometry::grid::LeveledGridCell) -> NodeQueryResult {
        if node.lod <= self.0 {
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
        if lod <= self.0 {
            vec![true; points.len()]
        } else {
            vec![false; points.len()]
        }
    }
}
