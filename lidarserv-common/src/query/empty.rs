use super::{ExecutableQuery, NodeQueryResult, Query};
use crate::geometry::grid::LodLevel;
use pasture_core::containers::BorrowedBuffer;
use serde::{Deserialize, Serialize};

/// Query that matches nothing
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct EmptyQuery;

impl Query for EmptyQuery {
    type Executable = Self;

    fn prepare(self, _ctx: &super::QueryContext) -> Self::Executable {
        self
    }
}

impl ExecutableQuery for EmptyQuery {
    fn matches_node(&self, _node: crate::geometry::grid::LeveledGridCell) -> NodeQueryResult {
        NodeQueryResult::Negative
    }

    fn matches_points(
        &self,
        _lod: LodLevel,
        points: &pasture_core::containers::VectorBuffer,
    ) -> Vec<bool> {
        vec![false; points.len()]
    }
}
