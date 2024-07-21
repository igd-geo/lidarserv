use serde::{Deserialize, Serialize};

use crate::geometry::grid::LodLevel;

use super::{NodeQueryResult, Query, QueryBuilder};

/// Query that inverses the result of the inner query
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct NotQuery<Inner: QueryBuilder>(pub Inner);

struct NotQueryPrepared<Inner: Query>(Inner);

impl<T> Query for NotQueryPrepared<T>
where
    T: Query,
{
    fn matches_node(&self, node: crate::geometry::grid::LeveledGridCell) -> super::NodeQueryResult {
        match self.0.matches_node(node) {
            NodeQueryResult::Negative => NodeQueryResult::Positive,
            NodeQueryResult::Positive => NodeQueryResult::Negative,
            NodeQueryResult::Partial => NodeQueryResult::Partial,
        }
    }

    fn matches_points(
        &self,
        lod: LodLevel,
        points: &pasture_core::containers::VectorBuffer,
    ) -> Vec<bool> {
        let mut result = self.0.matches_points(lod, points);
        for b in &mut result {
            *b = !*b
        }
        result
    }
}

impl<T> QueryBuilder for NotQuery<T>
where
    T: QueryBuilder,
{
    fn build(self, ctx: &super::QueryContext) -> impl Query {
        NotQueryPrepared(self.0.build(ctx))
    }
}
