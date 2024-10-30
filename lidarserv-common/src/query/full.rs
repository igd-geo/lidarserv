use std::convert::Infallible;

use pasture_core::containers::BorrowedBuffer;
use serde::{Deserialize, Serialize};

use crate::geometry::grid::LodLevel;

use super::{ExecutableQuery, NodeQueryResult, Query};

/// Query that matches everything
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FullQuery;

impl Query for FullQuery {
    type Executable = Self;
    type Error = Infallible;

    fn prepare(self, _ctx: &super::QueryContext) -> Result<Self::Executable, Self::Error> {
        Ok(self)
    }
}

impl ExecutableQuery for FullQuery {
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
