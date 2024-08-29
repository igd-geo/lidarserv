use pasture_core::containers::BorrowedBuffer;
use serde::{Deserialize, Serialize};

use super::{ExecutableQuery, NodeQueryResult, Query};

/// Query that is true, if ALL the internal queries are also true.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AndQuery<Q>(pub Vec<Q>);

impl<Q> Query for AndQuery<Q>
where
    Q: Query,
{
    type Executable = AndQuery<Q::Executable>;

    fn prepare(self, ctx: &super::QueryContext) -> Self::Executable {
        AndQuery(self.0.into_iter().map(|q| q.prepare(ctx)).collect())
    }
}

impl<Q> ExecutableQuery for AndQuery<Q>
where
    Q: ExecutableQuery,
{
    fn matches_node(&self, node: crate::geometry::grid::LeveledGridCell) -> super::NodeQueryResult {
        let mut result = NodeQueryResult::Positive;
        for inner in &self.0 {
            let one_result = inner.matches_node(node);
            match one_result {
                NodeQueryResult::Negative => return NodeQueryResult::Negative,
                NodeQueryResult::Partial => result = NodeQueryResult::Partial,
                NodeQueryResult::Positive => (),
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
            return vec![true; nr_points];
        };
        for fold_in in iter {
            assert!(fold_in.len() == result.len());
            for i in 0..result.len() {
                result[i] = result[i] && fold_in[i];
            }
        }
        result
    }
}
