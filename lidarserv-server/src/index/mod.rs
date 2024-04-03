use crate::common::geometry::sampling::GridCenterSampling;
use crate::common::index::octree::writer::OctreeWriter;
use crate::common::index::Index;
use crate::index::point::LasPoint;
use crate::net::protocol::messages::NodeId;
use crossbeam_channel::Receiver;
use lidarserv_common::geometry::grid::LeveledGridCell;
use lidarserv_common::geometry::position::I32CoordinateSystem;
use lidarserv_common::geometry::sampling::GridCenterSamplingFactory;
use lidarserv_common::index::octree::reader::OctreeReader;
use lidarserv_common::index::octree::Octree;
use lidarserv_common::index::{Query, Reader, Writer as CommonWriter};
use std::error::Error;
use std::sync::Arc;
use thiserror::Error;

pub mod builder;
pub mod point;
pub mod settings;

#[derive(Debug, Error)]
#[error("Coordinate system mismatch.")]
pub struct CoordinateSystemMismatchError;

pub struct IndexInfo<'a> {
    pub coordinate_system: &'a I32CoordinateSystem,
    pub sampling_factory: &'a GridCenterSamplingFactory<LasPoint>,
    pub point_record_format: u8,
}

/// object safe wrapper for a point cloud index, otherwise very similar to [Index].
pub trait DynIndex: Send + Sync {
    fn index_info(&self) -> IndexInfo;
    fn writer(&self) -> Box<dyn DynWriter>;
    fn reader(&self) -> Box<dyn DynReader>;
    fn flush(&mut self) -> Result<(), Box<dyn Error>>;
}

/// object safe wrapper for a point cloud writer, otherwise very similar to [lidarserv_common::index::Writer].
pub trait DynWriter: Send + Sync {
    fn insert_points(
        &mut self,
        points: Vec<LasPoint>,
        coordinate_system: &I32CoordinateSystem,
    ) -> Result<(), CoordinateSystemMismatchError>;
}

pub type NodeData = Vec<Arc<Vec<u8>>>;
pub type Node<Point> = (NodeId, Vec<Point>, I32CoordinateSystem);

pub trait DynReader: Send + Sync {
    /// Blocks until an update is available.
    /// New queries, filters or points trigger an update.
    fn blocking_update(&mut self, queries: &mut Receiver<Query>) -> bool;

    /// Checks if there are updates available.
    /// (This is a non-blocking version of [DynReader::blocking_update])
    fn updates_available(&mut self, queries: &mut Receiver<Query>) -> bool;

    /// Returns a node from the loading queue.
    /// Returns None if the loading queue is empty.
    /// Returns NodeId, Points of Node, CoordinateSystem of Node, if the loading queue is not empty.
    fn load_one(&mut self) -> Option<Node<LasPoint>>;

    /// Removes a node from the removal queue.
    /// Returns NodeId of removed node.
    /// Returns None if the removal queue is empty.
    fn remove_one(&mut self) -> Option<NodeId>;

    /// Reloads a node from the reload queue.
    /// Returns NodeId of old node and a vector of new nodes.
    /// Returns None if the update queue is empty.
    fn update_one(&mut self) -> Option<(NodeId, Vec<Node<LasPoint>>)>;
}

/// for use in the transmission protocol
fn leveled_grid_cell_to_proto_node_id(grid_cell: &LeveledGridCell) -> NodeId {
    NodeId {
        lod_level: grid_cell.lod.level(),
        id: {
            let mut id = [0; 14];
            let bytes_1 = grid_cell.lod.level().to_le_bytes();
            let bytes_2 = grid_cell.pos.x.to_le_bytes();
            let bytes_3 = grid_cell.pos.y.to_le_bytes();
            let bytes_4 = grid_cell.pos.z.to_le_bytes();
            id[0..2].copy_from_slice(&bytes_1);
            id[2..6].copy_from_slice(&bytes_2);
            id[6..10].copy_from_slice(&bytes_3);
            id[10..14].copy_from_slice(&bytes_4);
            id
        },
    }
}

impl DynIndex
    for Octree<LasPoint, GridCenterSampling<LasPoint>, GridCenterSamplingFactory<LasPoint>>
{
    fn index_info(&self) -> IndexInfo {
        IndexInfo {
            coordinate_system: self.coordinate_system(),
            sampling_factory: self.sampling_factory(),
            point_record_format: self.point_record_format(),
        }
    }

    fn writer(&self) -> Box<dyn DynWriter> {
        Box::new((*self.coordinate_system(), Index::writer(self)))
    }

    fn reader(&self) -> Box<dyn DynReader> {
        Box::new(Index::reader(self, Query::default()))
    }

    fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        match Octree::flush(self) {
            Ok(_) => Ok(()),
            Err(e) => Err(Box::new(e)),
        }
    }
}

impl DynWriter for (I32CoordinateSystem, OctreeWriter<LasPoint>) {
    fn insert_points(
        &mut self,
        points: Vec<LasPoint>,
        coordinate_system: &I32CoordinateSystem,
    ) -> Result<(), CoordinateSystemMismatchError> {
        if self.0 != *coordinate_system {
            return Err(CoordinateSystemMismatchError);
        }
        self.1.insert(points);
        Ok(())
    }
}

impl DynReader
    for OctreeReader<LasPoint, GridCenterSampling<LasPoint>, GridCenterSamplingFactory<LasPoint>>
{
    fn blocking_update(&mut self, queries: &mut Receiver<Query>) -> bool {
        Reader::blocking_update(self, queries)
    }

    fn updates_available(&mut self, queries: &mut Receiver<Query>) -> bool {
        Reader::try_update(self, queries)
    }

    fn load_one(&mut self) -> Option<Node<LasPoint>> {
        Reader::load_one(self).map(|(node_id, points, coordinate_system)| {
            let node_id = leveled_grid_cell_to_proto_node_id(&node_id);
            (node_id, points, coordinate_system)
        })
    }

    fn remove_one(&mut self) -> Option<NodeId> {
        Reader::remove_one(self)
            .as_ref()
            .map(leveled_grid_cell_to_proto_node_id)
    }

    fn update_one(&mut self) -> Option<(NodeId, Vec<Node<LasPoint>>)> {
        Reader::update_one(self).map(|(node_id, coordinate_system, replace)| {
            (
                leveled_grid_cell_to_proto_node_id(&node_id),
                replace
                    .into_iter()
                    .map(|(n, o)| (leveled_grid_cell_to_proto_node_id(&n), o, coordinate_system))
                    .collect(),
            )
        })
    }
}
