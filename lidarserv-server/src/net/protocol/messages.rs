use lidarserv_common::nalgebra::Vector3;
use serde::{Deserialize, Serialize, Serializer};
use std::fmt::{Debug, Formatter};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    /// First message, that both the client and the server send to each other.
    /// Contains the protocol version number, that each of them is speaking, so they can check
    /// if they are compatible to each other.
    Hello { protocol_version: u32 },

    /// Sent from the server to each client, after the connection got established,
    /// contains some general information about the point cloud, that is managed by the server.
    PointCloudInfo { coordinate_system: CoordinateSystem },

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
    Query(Query),

    /// Sent from the server to the client with some update to the current query result.
    IncrementalResult {
        replaces: Option<NodeId>,
        nodes: Vec<(NodeId, Vec<LasPointData>)>,
    },
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
pub enum Query {
    AabbQuery {
        min_bounds: Vector3<f64>,
        max_bounds: Vector3<f64>,
        lod_level: u16,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeId {
    pub lod_level: u16,
    pub id: [u8; 14],
}

/// Just a wrapper around Vec<u8>, with a custom Debug impl, so that not the full binary file is
/// printed in the debug output.
#[derive(Serialize, Deserialize, Clone)]
pub struct LasPointData(pub Vec<u8>);

impl Debug for LasPointData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Don't include the actual point data. It would just clutter the debug output.
        f.serialize_unit_struct("[Las Point Data]")
    }
}
