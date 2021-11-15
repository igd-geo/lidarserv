use crate::common::geometry::sampling::GridCenterSampling;
use crate::common::index::octree::writer::OctreeWriter;
use crate::common::index::Index;
use crate::index::point::LasPoint;
use crate::net::protocol::messages::NodeId;
use crossbeam_channel::Receiver;
use lidarserv_common::geometry::grid::{I32Grid, I32GridHierarchy};
use lidarserv_common::geometry::position::{I32CoordinateSystem, I32Position};
use lidarserv_common::geometry::sampling::GridCenterSamplingFactory;
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

/// object safe wrapper for a point cloud index, otherwise very similar to [lidarserv_common::index::Index].
pub trait DynIndex: Send + Sync {
    fn index_info(&self) -> &I32CoordinateSystem; // maybe return more info in future...
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
        queries: &mut crossbeam_channel::Receiver<Box<dyn Query<I32Position> + Send + Sync>>,
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
    fn index_info(&self) -> &I32CoordinateSystem {
        self.coordinate_system()
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
        Ok(())
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
    >
{
    fn blocking_update(
        &mut self,
        queries: &mut Receiver<Box<dyn Query<I32Position> + Send + Sync>>,
    ) -> bool {
        Reader::<LasPoint>::blocking_update(self, queries)
    }

    fn load_one(&mut self) -> Option<Node> {
        Reader::<LasPoint>::load_one(self).map(|(node_id, node)| {
            let node_id = meta_tree_node_id_to_proto_node_id(&node_id);
            let node_data = sensor_pos_node_to_protocol_node_data(&node);
            (node_id, node_data)
        })
    }

    fn remove_one(&mut self) -> Option<NodeId> {
        Reader::<LasPoint>::remove_one(self)
            .as_ref()
            .map(meta_tree_node_id_to_proto_node_id)
    }

    fn update_one(&mut self) -> Option<(NodeId, Vec<Node>)> {
        Reader::<LasPoint>::update_one(self).map(|(node_id, replacements)| {
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
    fn index_info(&self) -> &I32CoordinateSystem {
        self.coordinate_system()
    }

    fn writer(&self) -> Box<dyn DynWriter> {
        Box::new((self.coordinate_system().clone(), Index::writer(self)))
    }

    fn reader(&self) -> Box<dyn DynReader> {
        Box::new(( /* todo */))
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

impl DynReader for () {
    fn blocking_update(
        &mut self,
        _queries: &mut Receiver<Box<dyn Query<I32Position> + Send + Sync>>,
    ) -> bool {
        todo!()
    }

    fn load_one(&mut self) -> Option<Node> {
        todo!()
    }

    fn remove_one(&mut self) -> Option<NodeId> {
        todo!()
    }

    fn update_one(&mut self) -> Option<(NodeId, Vec<Node>)> {
        todo!()
    }
}
