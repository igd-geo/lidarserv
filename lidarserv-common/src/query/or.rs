use pasture_core::containers::BorrowedBuffer;
use serde::{Deserialize, Serialize};

use super::{ExecutableQuery, NodeQueryResult, Query};

/// Query that is true, if ANY of the internal queries is also true.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct OrQuery<Q>(pub Vec<Q>);

impl<Q> Query for OrQuery<Q>
where
    Q: Query,
{
    type Executable = OrQuery<Q::Executable>;

    fn prepare(self, ctx: &super::QueryContext) -> Self::Executable {
        OrQuery(self.0.into_iter().map(|q| q.prepare(ctx)).collect())
    }
}

impl<Q> ExecutableQuery for OrQuery<Q>
where
    Q: ExecutableQuery,
{
    fn matches_node(&self, node: crate::geometry::grid::LeveledGridCell) -> super::NodeQueryResult {
        let mut result = NodeQueryResult::Negative;
        for inner in &self.0 {
            let one_result = inner.matches_node(node);
            match one_result {
                NodeQueryResult::Negative => (),
                NodeQueryResult::Partial => result = NodeQueryResult::Partial,
                NodeQueryResult::Positive => return NodeQueryResult::Positive,
            }
        }
        result
    }

    fn matches_points(
        &self,
        lod: crate::geometry::grid::LodLevel,
        points: &pasture_core::containers::VectorBuffer,
    ) -> Vec<bool> {
        let mut iter = self.0.iter().map(|q| q.matches_points(lod, points));
        let Some(mut result) = iter.next() else {
            let nr_points = points.len();
            return vec![false; nr_points];
        };
        for fold_in in iter {
            assert!(fold_in.len() == result.len());
            for i in 0..result.len() {
                result[i] = result[i] || fold_in[i];
            }
        }
        result
    }
}
