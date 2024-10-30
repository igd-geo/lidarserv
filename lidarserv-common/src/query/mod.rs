use std::{fmt::Debug, sync::Arc};

use crate::{
    geometry::{
        coordinate_system::CoordinateSystem,
        grid::{GridHierarchy, LeveledGridCell, LodLevel},
        position::PositionComponentType,
    },
    index::attribute_index::AttributeIndex,
};
use pasture_core::{containers::VectorBuffer, layout::PointLayout};

pub mod aabb;
pub mod and;
pub mod attribute;
pub mod empty;
pub mod full;
pub mod lod;
pub mod not;
pub mod or;
pub mod view_frustum;

/// Execution Context for queries.
/// Contains everything the query might need to determine its result. E.g. details about the coordinate system.
#[derive(Clone)]
pub struct QueryContext {
    pub node_hierarchy: GridHierarchy,
    pub point_hierarchy: GridHierarchy,
    pub coordinate_system: CoordinateSystem,
    pub component_type: PositionComponentType,
    pub attribute_index: Arc<AttributeIndex>,
    pub point_layout: PointLayout,
}

/// Describes, how an octree node matches a query.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum NodeQueryResult {
    /// The node does not match the query.
    /// Don't accept any points into the query result.
    /// Don't recurse into child nodes.
    Negative,

    /// The node matches the query.
    /// Accept all points in the node into the query result without further filtering.
    /// Recurse into the child nodes.
    Positive,

    /// Some points in the node are expected to match the query.
    /// Use point-based filtering to determine which points to accept into the query result. (If PB filtering is enabled)
    /// Recurse into the child nodes.
    Partial,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum LoadKind {
    Full,
    Filter,
}

/// Filter the point cloud based on some criterion, after
/// initializing with some query context.
pub trait Query: Send + Sync + Debug + 'static {
    /// The type returned by build.
    type Executable: ExecutableQuery;
    type Error;

    /// Prepares the query for execution.
    ///
    /// (i.e.
    ///  - convert coordinates into the local coordinate system,
    ///  - open external index files
    ///  - ...
    ///    )
    fn prepare(self, ctx: &QueryContext) -> Result<Self::Executable, Self::Error>;
}

/// Filter the point cloud based on some criterion.
pub trait ExecutableQuery: Send + Sync + 'static {
    /// Checks, if the node matches the query
    fn matches_node(&self, node: LeveledGridCell) -> NodeQueryResult;

    /// Checks each point in the buffer if they match the query.
    ///
    /// Todo - consider some kind of bitvec, where 8 booleans are packed into a byte.
    /// Hypothesis: This will allow to combine query results faster (AND, OR, NOT), but
    /// the actual terminal conditions will have some overhead for setting the correct bit in a byte
    fn matches_points(&self, lod: LodLevel, points: &VectorBuffer) -> Vec<bool>;
}

impl<T> Query for Box<T>
where
    T: Query,
{
    type Executable = T::Executable;
    type Error = T::Error;

    fn prepare(self, ctx: &QueryContext) -> Result<Self::Executable, Self::Error> {
        (*self).prepare(ctx)
    }
}

impl<T> ExecutableQuery for Box<T>
where
    T: ExecutableQuery + ?Sized,
{
    fn matches_node(&self, node: LeveledGridCell) -> NodeQueryResult {
        self.as_ref().matches_node(node)
    }

    fn matches_points(&self, lod: LodLevel, points: &VectorBuffer) -> Vec<bool> {
        self.as_ref().matches_points(lod, points)
    }
}

impl NodeQueryResult {
    pub fn should_load(&self, point_filtering: bool) -> Option<LoadKind> {
        match self {
            NodeQueryResult::Negative => None,
            NodeQueryResult::Positive => Some(LoadKind::Full),
            NodeQueryResult::Partial => {
                if point_filtering {
                    Some(LoadKind::Filter)
                } else {
                    Some(LoadKind::Full)
                }
            }
        }
    }

    pub fn inverse(self) -> Self {
        match self {
            NodeQueryResult::Negative => NodeQueryResult::Positive,
            NodeQueryResult::Positive => NodeQueryResult::Negative,
            NodeQueryResult::Partial => NodeQueryResult::Partial,
        }
    }

    pub fn and(self, other: Self) -> Self {
        match (self, other) {
            (NodeQueryResult::Negative, NodeQueryResult::Negative) => NodeQueryResult::Negative,
            (NodeQueryResult::Negative, NodeQueryResult::Positive) => NodeQueryResult::Negative,
            (NodeQueryResult::Negative, NodeQueryResult::Partial) => NodeQueryResult::Negative,
            (NodeQueryResult::Positive, NodeQueryResult::Negative) => NodeQueryResult::Negative,
            (NodeQueryResult::Positive, NodeQueryResult::Positive) => NodeQueryResult::Positive,
            (NodeQueryResult::Positive, NodeQueryResult::Partial) => NodeQueryResult::Partial,
            (NodeQueryResult::Partial, NodeQueryResult::Negative) => NodeQueryResult::Negative,
            (NodeQueryResult::Partial, NodeQueryResult::Positive) => NodeQueryResult::Partial,
            (NodeQueryResult::Partial, NodeQueryResult::Partial) => NodeQueryResult::Partial,
        }
    }

    pub fn or(self, other: Self) -> Self {
        match (self, other) {
            (NodeQueryResult::Negative, NodeQueryResult::Negative) => NodeQueryResult::Negative,
            (NodeQueryResult::Negative, NodeQueryResult::Positive) => NodeQueryResult::Positive,
            (NodeQueryResult::Negative, NodeQueryResult::Partial) => NodeQueryResult::Partial,
            (NodeQueryResult::Positive, NodeQueryResult::Negative) => NodeQueryResult::Positive,
            (NodeQueryResult::Positive, NodeQueryResult::Positive) => NodeQueryResult::Positive,
            (NodeQueryResult::Positive, NodeQueryResult::Partial) => NodeQueryResult::Positive,
            (NodeQueryResult::Partial, NodeQueryResult::Negative) => NodeQueryResult::Partial,
            (NodeQueryResult::Partial, NodeQueryResult::Positive) => NodeQueryResult::Positive,
            (NodeQueryResult::Partial, NodeQueryResult::Partial) => NodeQueryResult::Partial,
        }
    }
}
