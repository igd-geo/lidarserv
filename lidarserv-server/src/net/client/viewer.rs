use crate::index::point::{GlobalPoint};
use crate::net::protocol::connection::Connection;
use crate::net::protocol::messages::{DeviceType, Message, NodeId, Query};
use crate::net::{LidarServerError, PROTOCOL_VERSION};
use lidarserv_common::geometry::bounding_box::AABB;
use lidarserv_common::geometry::grid::LodLevel;
use lidarserv_common::geometry::position::{F64CoordinateSystem, F64Position, Position};
use lidarserv_common::nalgebra::Matrix4;
use std::fmt::{Debug, Formatter};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::sync::broadcast::Receiver;
use lidarserv_common::index::octree::attribute_bounds::LasPointAttributeBounds;

pub struct ViewerClient<Stream> {
    connection: Connection<Stream>,
    received_updates: Arc<Mutex<u64>>,
}

impl ViewerClient<TcpStream> {
    pub async fn connect<A>(addr: A, shutdown: &mut Receiver<()>) -> Result<Self, LidarServerError>
    where
        A: ToSocketAddrs,
    {
        let tcp_con = TcpStream::connect(addr).await?;
        let peer_addr = tcp_con.peer_addr()?;
        tcp_con.set_nodelay(true)?;
        let mut connection = Connection::new(tcp_con, peer_addr, shutdown).await?;

        // exchange hello messages and check each others protocol compatibility
        connection
            .write_message(&Message::Hello {
                protocol_version: PROTOCOL_VERSION,
            })
            .await?;
        let hello = connection.read_message(shutdown).await?;
        match hello {
            Message::Hello { protocol_version } => {
                if protocol_version != PROTOCOL_VERSION {
                    return Err(LidarServerError::Protocol(format!(
                        "Protocol version mismatch (Server: {}, Client: {}).",
                        protocol_version, protocol_version
                    )));
                }
            }
            _ => {
                return Err(LidarServerError::Protocol(
                    "Expected a `Hello` as the first message.".to_string(),
                ))
            }
        };

        // tell the server that we are a viewer, that will query points.
        connection
            .write_message(&Message::ConnectionMode {
                device: DeviceType::Viewer,
            })
            .await?;

        // wait for the point cloud info.
        // (we don't need that info at the moment, so all we do with it is ignoring it...)
        let pc_info = connection.read_message(shutdown).await?;
        match pc_info {
            Message::PointCloudInfo { .. } => (),
            _ => {
                return Err(LidarServerError::Protocol(
                    "Expected a `PointCloudInfo` message.".to_string(),
                ));
            }
        };

        Ok(ViewerClient {
            connection,
            received_updates: Arc::new(Mutex::new(0)),
        })
    }

    pub fn into_split(self) -> (ViewerClient<OwnedReadHalf>, ViewerClient<OwnedWriteHalf>) {
        let (read_half, write_half) = self.connection.into_split();
        (
            ViewerClient {
                connection: read_half,
                received_updates: Arc::clone(&self.received_updates),
            },
            ViewerClient {
                connection: write_half,
                received_updates: self.received_updates,
            },
        )
    }
}

pub struct ParsedNode {
    pub node_id: NodeId,
    pub points: Vec<GlobalPoint>,
}

#[derive(Debug)]
pub struct IncrementalUpdate {
    pub remove: Option<NodeId>,
    pub insert: Vec<ParsedNode>,
    pub result_complete: bool,
}

impl Debug for ParsedNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedNode")
            .field("node_id", &self.node_id)
            .field("points.len()", &self.points.len())
            .finish()
    }
}

impl<WriteStream> ViewerClient<WriteStream>
where
    WriteStream: AsyncWrite + Unpin,
{
    pub async fn ack(&mut self) -> Result<(), LidarServerError> {
        let update_number = *self.received_updates.lock().unwrap();
        self.connection
            .write_message(&Message::ResultAck { update_number })
            .await
    }

    pub async fn query_aabb(
        &mut self,
        global_aabb: &AABB<f64>,
        lod: &LodLevel,
        filter: Option<LasPointAttributeBounds>,
        enable_attribute_acceleration: bool,
        enable_histogram_acceleration: bool,
        enable_point_filtering: bool,
    ) -> Result<(), LidarServerError> {
        let csys = F64CoordinateSystem::new();
        let min = global_aabb.min::<F64Position>().decode(&csys);
        let max = global_aabb.max::<F64Position>().decode(&csys);
        self.connection
            .write_message(&Message::Query{
                query: Box::new(Query::AabbQuery {
                    min_bounds: min.coords,
                    max_bounds: max.coords,
                    lod_level: lod.level(),
                }),
                filter,
                enable_attribute_acceleration,
                enable_histogram_acceleration,
                enable_point_filtering,
            }).await
    }

    pub async fn query_view_frustum(
        &mut self,
        view_projection_matrix: Matrix4<f64>,
        view_projection_matrix_inv: Matrix4<f64>,
        window_width_pixels: f64,
        min_distance_pixels: f64,
        filter: Option<LasPointAttributeBounds>,
        enable_attribute_acceleration: bool,
        enable_histogram_acceleration: bool,
        enable_point_filtering: bool,
    ) -> Result<(), LidarServerError> {
        self.connection
            .write_message(&Message::Query{
                query: Box::new(Query::ViewFrustumQuery {
                    view_projection_matrix,
                    view_projection_matrix_inv,
                    window_width_pixels,
                    min_distance_pixels,
                }),
                filter,
                enable_attribute_acceleration,
                enable_histogram_acceleration,
                enable_point_filtering,
            }).await
    }
}

impl<WriteStream> ViewerClient<WriteStream>
where
    WriteStream: AsyncRead + Unpin,
{
    pub async fn receive_update(
        &mut self,
        shutdown: &mut Receiver<()>,
    ) -> Result<IncrementalUpdate, LidarServerError> {
        match self.connection.read_message(shutdown).await? {
            Message::IncrementalResult { replaces, nodes} => {
                // read points
                *self.received_updates.lock().unwrap() += 1;
                let mut insert_nodes = Vec::new();
                for (insert_node_id, points, coordinate_system) in nodes {
                    // convert all points to global points
                    let las_points = points.into_iter().map(|point| {
                        GlobalPoint::from_las_point(point, &coordinate_system)
                    }).collect();
                    // add the node to the list of nodes to insert
                    insert_nodes.push(ParsedNode {
                        node_id: insert_node_id,
                        points: las_points,
                    });
                }
                Ok(IncrementalUpdate {
                    remove: replaces,
                    insert: insert_nodes,
                    result_complete: false,
                })
            }
            Message::ResultComplete => Ok(IncrementalUpdate {
                remove: None,
                insert: Vec::new(),
                result_complete: true,
            }),
            _ => Err(LidarServerError::Protocol(
                "Expected an `IncrementalResult` message.".to_string(),
            )),
        }
    }
}
