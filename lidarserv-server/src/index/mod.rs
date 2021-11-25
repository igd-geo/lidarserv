use crate::common::geometry::sampling::GridCenterSampling;
use crate::common::index::octree::writer::OctreeWriter;
use crate::common::index::Index;
use crate::index::point::LasPoint;
use crate::net::protocol::messages::NodeId;
use crossbeam_channel::Receiver;
use lidarserv_common::geometry::grid::{I32Grid, I32GridHierarchy, LeveledGridCell};
use lidarserv_common::geometry::position::{I32CoordinateSystem, I32Position};
use lidarserv_common::geometry::sampling::GridCenterSamplingFactory;
use lidarserv_common::index::octree::reader::OctreeReader;
use lidarserv_common::index::octree::Octree;
use lidarserv_common::index::sensor_pos::meta_tree::MetaTreeNodeId;
use lidarserv_common::index::sensor_pos::reader::{SensorPosNode, SensorPosReader};
use lidarserv_common::index::sensor_pos::writer::SensorPosWriter;
use lidarserv_common::index::sensor_pos::SensorPosIndex;
use lidarserv_common::index::Node as IndexNode;
use lidarserv_common::index::{Reader, Writer as CommonWriter};
use lidarserv_common::las::I32LasReadWrite;
use lidarserv_common::query::empty::EmptyQuery;
use lidarserv_common::query::Query;
use std::error::Error;
use thiserror::Error;

pub mod builder;
pub mod point;
pub mod settings;

#[derive(Debug, Error)]
#[error("Coordinate system mismatch.")]
pub struct CoordinateSystemMismatchError;

pub struct IndexInfo<'a> {
    pub coordinate_system: &'a I32CoordinateSystem,
    pub sampling_factory:
        &'a GridCenterSamplingFactory<I32GridHierarchy, LasPoint, I32Position, i32>,
}

/// object safe wrapper for a point cloud index, otherwise very similar to [lidarserv_common::index::Index].
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

pub type NodeData = Vec<Vec<u8>>;
pub type Node = (NodeId, NodeData);

pub trait DynReader: Send + Sync {
    fn blocking_update(
        &mut self,
        queries: &mut crossbeam_channel::Receiver<
            Box<dyn Query<I32Position, I32CoordinateSystem> + Send + Sync>,
        >,
    ) -> bool;

    fn load_one(&mut self) -> Option<Node>;
    fn remove_one(&mut self) -> Option<NodeId>;
    fn update_one(&mut self) -> Option<(NodeId, Vec<Node>)>;
}

impl DynIndex
    for SensorPosIndex<
        I32GridHierarchy,
        GridCenterSamplingFactory<I32GridHierarchy, LasPoint, I32Position, i32>,
        i32,
        I32LasReadWrite,
        I32CoordinateSystem,
    >
{
    fn index_info(&self) -> IndexInfo {
        IndexInfo {
            coordinate_system: self.coordinate_system(),
            sampling_factory: self.sampling_factory(),
        }
    }

    fn writer(&self) -> Box<dyn DynWriter> {
        let wr = lidarserv_common::index::Index::writer(self);
        Box::new(wr)
    }

    fn reader(&self) -> Box<dyn DynReader> {
        let rd = lidarserv_common::index::Index::reader(self, EmptyQuery);
        Box::new(rd)
    }

    fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        SensorPosIndex::flush(self).map_err(|e| Box::new(e) as Box<dyn Error>)
    }
}

impl DynWriter for SensorPosWriter<LasPoint, I32CoordinateSystem> {
    fn insert_points(
        &mut self,
        points: Vec<LasPoint>,
        coordinate_system: &I32CoordinateSystem,
    ) -> Result<(), CoordinateSystemMismatchError> {
        if *self.coordinate_system() != *coordinate_system {
            return Err(CoordinateSystemMismatchError);
        }
        self.insert(points);
        Ok(())
    }
}

impl DynReader
    for SensorPosReader<
        I32GridHierarchy,
        GridCenterSamplingFactory<I32GridHierarchy, LasPoint, I32Position, i32>,
        i32,
        I32LasReadWrite,
        I32CoordinateSystem,
        I32Position,
        LasPoint,
    >
{
    fn blocking_update(
        &mut self,
        queries: &mut Receiver<Box<dyn Query<I32Position, I32CoordinateSystem> + Send + Sync>>,
    ) -> bool {
        Reader::<LasPoint, _>::blocking_update(self, queries)
    }

    fn load_one(&mut self) -> Option<Node> {
        Reader::<LasPoint, _>::load_one(self).map(|(node_id, node)| {
            let node_id = meta_tree_node_id_to_proto_node_id(&node_id);
            let node_data = sensor_pos_node_to_protocol_node_data(&node);
            (node_id, node_data)
        })
    }

    fn remove_one(&mut self) -> Option<NodeId> {
        Reader::<LasPoint, _>::remove_one(self)
            .as_ref()
            .map(meta_tree_node_id_to_proto_node_id)
    }

    fn update_one(&mut self) -> Option<(NodeId, Vec<Node>)> {
        Reader::<LasPoint, _>::update_one(self).map(|(node_id, replacements)| {
            let node_id = meta_tree_node_id_to_proto_node_id(&node_id);
            let replacements = replacements
                .into_iter()
                .map(|(replacement_node_id, replacement_node_data)| {
                    let replacement_node_id =
                        meta_tree_node_id_to_proto_node_id(&replacement_node_id);
                    let node_data = sensor_pos_node_to_protocol_node_data(&replacement_node_data);
                    (replacement_node_id, node_data)
                })
                .collect();
            (node_id, replacements)
        })
    }
}

fn meta_tree_node_id_to_proto_node_id(node_id: &MetaTreeNodeId) -> NodeId {
    NodeId {
        lod_level: node_id.lod().level(),
        id: {
            let mut id = [0; 14];
            let bytes_1 = node_id.tree_node().lod.level().to_le_bytes();
            let bytes_2 = node_id.tree_node().pos.x.to_le_bytes();
            let bytes_3 = node_id.tree_node().pos.y.to_le_bytes();
            let bytes_4 = node_id.tree_node().pos.z.to_le_bytes();
            id[0..2].copy_from_slice(&bytes_1);
            id[2..6].copy_from_slice(&bytes_2);
            id[6..10].copy_from_slice(&bytes_3);
            id[10..14].copy_from_slice(&bytes_4);
            id
        },
    }
}

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

fn sensor_pos_node_to_protocol_node_data(node: &SensorPosNode) -> NodeData {
    node.las_files().into_iter().map(Vec::from).collect()
}

impl DynIndex
    for Octree<
        LasPoint,
        I32GridHierarchy,
        I32LasReadWrite,
        GridCenterSampling<I32Grid, LasPoint, I32Position, i32>,
        i32,
        I32CoordinateSystem,
        GridCenterSamplingFactory<I32GridHierarchy, LasPoint, I32Position, i32>,
    >
{
    fn index_info(&self) -> IndexInfo {
        IndexInfo {
            coordinate_system: self.coordinate_system(),
            sampling_factory: self.sampling_factory(),
        }
    }

    fn writer(&self) -> Box<dyn DynWriter> {
        Box::new((self.coordinate_system().clone(), Index::writer(self)))
    }

    fn reader(&self) -> Box<dyn DynReader> {
        Box::new(Index::reader(self, EmptyQuery::new()))
    }

    fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        match Octree::flush(self) {
            Ok(_) => Ok(()),
            Err(e) => Err(Box::new(e)),
        }
    }
}

impl DynWriter
    for (
        I32CoordinateSystem,
        OctreeWriter<LasPoint, I32GridHierarchy>,
    )
{
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
    for OctreeReader<
        LasPoint,
        I32GridHierarchy,
        I32LasReadWrite,
        GridCenterSampling<I32Grid, LasPoint, I32Position, i32>,
        i32,
        I32CoordinateSystem,
        GridCenterSamplingFactory<I32GridHierarchy, LasPoint, I32Position, i32>,
        I32Position,
    >
{
    fn blocking_update(
        &mut self,
        queries: &mut Receiver<Box<dyn Query<I32Position, I32CoordinateSystem> + Send + Sync>>,
    ) -> bool {
        Reader::blocking_update(self, queries)
    }

    fn load_one(&mut self) -> Option<Node> {
        Reader::load_one(self).map(|(node_id, data)| {
            let node_id = leveled_grid_cell_to_proto_node_id(&node_id);
            (node_id, vec![data.data.as_ref().clone()])
        })
    }

    fn remove_one(&mut self) -> Option<NodeId> {
        Reader::remove_one(self)
            .as_ref()
            .map(leveled_grid_cell_to_proto_node_id)
    }

    fn update_one(&mut self) -> Option<(NodeId, Vec<Node>)> {
        Reader::update_one(self).map(|(node_id, replace)| {
            (
                leveled_grid_cell_to_proto_node_id(&node_id),
                replace
                    .into_iter()
                    .map(|(n, o)| {
                        (
                            leveled_grid_cell_to_proto_node_id(&n),
                            vec![o.data.as_ref().clone()],
                        )
                    })
                    .collect(),
            )
        })
    }
}
