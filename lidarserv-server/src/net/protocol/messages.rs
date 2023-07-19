use lidarserv_common::nalgebra::{Matrix4, Vector3};
use serde::{Deserialize, Serialize, Serializer};
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use lidarserv_common::geometry::position::I32CoordinateSystem;
use lidarserv_common::index::octree::attribute_bounds::LasPointAttributeBounds;
use crate::index::point::LasPoint;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    /// First message, that both the client and the server send to each other.
    /// Contains the protocol version number, that each of them is speaking, so they can check
    /// if they are compatible to each other.
    Hello { protocol_version: u32 },

    /// Sent from the server to each client, after the connection got established,
    /// contains some general information about the point cloud, that is managed by the server.
    PointCloudInfo {
        coordinate_system: CoordinateSystem,
        point_record_format: u8,
    },

    /// First command sent from the client to the server after exchanging the hello message.
    /// This permanently sets the connection mode according to the device type and makes the server
    /// initialize the appropriate resources.
    ConnectionMode { device: DeviceType },

    /// Sent from the server, if any kind of error occurred. After this message, the connection
    /// will be terminated.
    Error { message: String },

    /// Sent from client to server in CaptureDevice mode, to insert a batch of new points.
    InsertPoints { data: LasPointData },

    /// Sent from the client to server in Viewer mode, to set or update the query.
    Query{
        query: Box<Query>,
        filter: Option<LasPointAttributeBounds>,
        enable_attribute_acceleration: bool,
        enable_histogram_acceleration: bool,
        enable_point_filtering: bool,
    },

    /// Sent from the server to the client with some update to the current query result.
    /// "replaces" is the node id of the node, that is replaced by the new nodes.
    /// "nodes" is a list of nodes, that are added to the current query result.
    /// it contains the node id, the points and the coordinate system of each node.
    IncrementalResult {
        replaces: Option<NodeId>,
        nodes: Vec<(NodeId, Vec<LasPoint>, I32CoordinateSystem)>,
    },

    /// Sent from the server to the client, to indicate that the current query result is complete.
    /// This message is sent after the last IncrementalResult message.
    ResultComplete,

    /// Sent from the client to the server, as an acknowledgement of the update(s) it has processed so far
    /// So that the server can slow down, if the client is too slow.
    ResultAck { update_number: u64 },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DeviceType {
    CaptureDevice,
    Viewer,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CoordinateSystem {
    I32CoordinateSystem {
        scale: Vector3<f64>,
        offset: Vector3<f64>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Query {
    AabbQuery {
        min_bounds: Vector3<f64>,
        max_bounds: Vector3<f64>,
        lod_level: u16,
    },
    ViewFrustumQuery {
        view_projection_matrix: Matrix4<f64>,
        view_projection_matrix_inv: Matrix4<f64>,
        window_width_pixels: f64,
        min_distance_pixels: f64,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
pub struct NodeId {
    pub lod_level: u16,
    pub id: [u8; 14],
}

/// Just a wrapper around Vec<u8>, with a custom Debug impl, so that not the full binary file is
/// printed in the debug output.
#[derive(Serialize, Deserialize, Clone)]
pub struct LasPointData(pub Arc<Vec<u8>>);

impl Debug for LasPointData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Don't include the actual point data. It would just clutter the debug output.
        f.serialize_unit_struct("[Las Point Data]")
    }
}
