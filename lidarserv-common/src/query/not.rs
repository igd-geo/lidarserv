use super::{ExecutableQuery, NodeQueryResult, Query};
use crate::geometry::grid::LodLevel;
use serde::{Deserialize, Serialize};

/// Query that inverses the result of the inner query
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct NotQuery<Inner>(pub Inner);

impl<T> Query for NotQuery<T>
where
    T: Query,
{
    type Executable = NotQuery<T::Executable>;
    type Error = T::Error;

    fn prepare(self, ctx: &super::QueryContext) -> Result<Self::Executable, Self::Error> {
        Ok(NotQuery(self.0.prepare(ctx)?))
    }
}

impl<T> ExecutableQuery for NotQuery<T>
where
    T: ExecutableQuery,
{
    fn matches_node(&self, node: crate::geometry::grid::LeveledGridCell) -> NodeQueryResult {
        self.0.matches_node(node).inverse()
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
