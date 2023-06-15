use crate::common::geometry::sampling::GridCenterSampling;
use crate::common::index::octree::writer::OctreeWriter;
use crate::common::index::Index;
use crate::index::point::LasPoint;
use crate::net::protocol::messages::NodeId;
use crossbeam_channel::Receiver;
use lidarserv_common::geometry::grid::LeveledGridCell;
use lidarserv_common::geometry::position::{I32CoordinateSystem, I32Position};
use lidarserv_common::geometry::sampling::GridCenterSamplingFactory;
use lidarserv_common::index::octree::reader::OctreeReader;
use lidarserv_common::index::octree::Octree;
use lidarserv_common::index::Node as IndexNode;
use lidarserv_common::index::{Reader, Writer as CommonWriter};
use lidarserv_common::query::empty::EmptyQuery;
use lidarserv_common::query::Query;
use std::error::Error;
use std::sync::Arc;
use thiserror::Error;
use lidarserv_common::geometry::points::{PointType, WithAttr};
use lidarserv_common::index::octree::attribute_bounds::LasPointAttributeBounds;
use lidarserv_common::las::LasPointAttributes;

pub mod builder;
pub mod point;
pub mod settings;

#[derive(Debug, Error)]
#[error("Coordinate system mismatch.")]
pub struct CoordinateSystemMismatchError;

pub struct IndexInfo<'a> {
    pub coordinate_system: &'a I32CoordinateSystem,
    pub sampling_factory: &'a GridCenterSamplingFactory<LasPoint>,
    pub use_point_colors: bool,
    pub use_point_times: bool,
}

/// object safe wrapper for a point cloud index, otherwise very similar to [lidarserv_common::index::Index].
pub trait DynIndex: Send + Sync
{
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

pub trait DynReader: Send + Sync
{
    fn blocking_update(
        &mut self,
        queries: &mut crossbeam_channel::Receiver<Box<dyn Query + Send + Sync>>,
        filters: &mut crossbeam_channel::Receiver<Option<LasPointAttributeBounds>>,
    ) -> bool;
    fn updates_available(
        &mut self,
        queries: &mut crossbeam_channel::Receiver<Box<dyn Query + Send + Sync>>,
        filters: &mut crossbeam_channel::Receiver<Option<LasPointAttributeBounds>>,
    ) -> bool;
    fn load_one(&mut self) -> Option<Node<LasPoint>>;
    fn remove_one(&mut self) -> Option<NodeId>;
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
            use_point_colors: self.use_point_colors(),
            use_point_times: self.use_point_times(),
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
    fn blocking_update(&mut self, queries: &mut Receiver<Box<dyn Query + Send + Sync>>, filters: &mut Receiver<Option<LasPointAttributeBounds>>) -> bool {
        Reader::blocking_update(self, queries, filters)
    }

    fn updates_available(&mut self, queries: &mut Receiver<Box<dyn Query + Send + Sync>>, filters: &mut Receiver<Option<LasPointAttributeBounds>>) -> bool {
        Reader::updates_available(self, queries, filters)
    }

    fn load_one(&mut self) -> Option<Node<LasPoint>> {
        Reader::load_one(self).map(|(node_id, data, coordinate_system)| {
            let node_id = leveled_grid_cell_to_proto_node_id(&node_id);
            let points = data.points();
            (node_id, points, coordinate_system)
        })
    }

    fn remove_one(&mut self) -> Option<NodeId> {
        Reader::remove_one(self)
            .as_ref()
            .map(leveled_grid_cell_to_proto_node_id)
    }

    fn update_one(&mut self) -> Option<(NodeId, Vec<Node<LasPoint>>)>
    {
        Reader::update_one(self).map(|(node_id, coordinate_system, replace)| {
            (
                leveled_grid_cell_to_proto_node_id(&node_id),
                replace
                    .into_iter()
                    .map(|(n, o)| (leveled_grid_cell_to_proto_node_id(&n), o.points(), coordinate_system))
                    .collect(),
            )
        })
    }
}
