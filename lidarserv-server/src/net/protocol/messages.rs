use lidarserv_common::geometry::coordinate_system::CoordinateSystem;
use lidarserv_common::geometry::grid::LeveledGridCell;
use lidarserv_common::io::pasture::{Compression, PastureIo};
use lidarserv_common::io::InMemoryPointCodec;
use pasture_core::layout::PointAttributeDefinition;
use serde::{Deserialize, Serialize, Serializer};
use std::fmt::{Debug, Formatter};
use std::ops::Deref;

use crate::index::query::Query;

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
        attributes: Vec<PointAttributeDefinition>,
        codec: PointDataCodec,
    },

    /// First command sent from the client to the server after exchanging the hello message.
    /// This permanently sets the connection mode according to the device type and makes the server
    /// initialize the appropriate resources.
    ConnectionMode { device: DeviceType },

    /// Sent from the server, if any kind of error occurred. After this message, the connection
    /// will be terminated.
    Error { message: String },

    /// Sent from client to server in CaptureDevice mode, to insert a batch of new points.
    InsertPoints { data: PointData },

    /// Sent from the client to server in Viewer mode, to set or update the query.
    Query { query: Query, config: QueryConfig },

    /// Sent from the server to the client with some update to the current query result.
    /// The node should be updated (or added, if it is new) in the query result with the given point buffer.
    /// If the point buffer is None, then the node shall be deleted.
    Node {
        node: LeveledGridCell,
        points: Option<PointData>,
        update_number: u64,
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
pub struct QueryConfig {
    pub one_shot: bool,
    pub point_filtering: bool
}

/// Just a wrapper around Vec<u8>, with a custom Debug impl, so that not the full binary file is
/// printed in the debug output.
#[derive(Serialize, Deserialize, Clone)]
pub struct PointData(pub Vec<u8>);

impl Debug for PointData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Don't include the actual point data. It would just clutter the debug output.
        f.serialize_unit_struct("[Point Data]")
    }
}

impl Deref for PointData {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum PointDataCodec {
    Pasture { compression: bool },
    // todo: Las {compression: bool, point_format: u8, },
}

impl PointDataCodec {
    pub fn instance(&self) -> Box<dyn InMemoryPointCodec + Send> {
        match *self {
            PointDataCodec::Pasture { compression } => Box::new(PastureIo::new(if compression {
                Compression::Lz4
            } else {
                Compression::None
            })),
        }
    }
}
