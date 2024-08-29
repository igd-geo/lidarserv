use crate::index::query::Query;
use crate::net::protocol::connection::Connection;
use crate::net::protocol::messages::{DeviceType, Message, PointData, PointDataCodec, QueryConfig};
use crate::net::{LidarServerError, PROTOCOL_VERSION};
use lidarserv_common::geometry::coordinate_system::CoordinateSystem;
use lidarserv_common::geometry::grid::LeveledGridCell;
use lidarserv_common::geometry::position::{WithComponentTypeOnce, POSITION_ATTRIBUTE_NAME};
use nalgebra::Vector3;
use pasture_core::containers::{
    BorrowedBuffer, BorrowedMutBuffer, InterleavedBuffer, InterleavedBufferMut,
    MakeBufferFromLayout, OwningBuffer, VectorBuffer,
};
use pasture_core::layout::attributes::POSITION_3D;
use pasture_core::layout::{PointAttributeDefinition, PointLayout};
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::sync::broadcast::Receiver;
use tokio::sync::Mutex;

struct Inner {
    connection: Connection<OwnedWriteHalf>,
    last_ack: u64,
    ack_after: u64,
}

#[derive()]
pub struct ReadViewerClient {
    connection: Connection<OwnedReadHalf>,
    inner: Arc<Mutex<Inner>>,
    codec: PointDataCodec,
    coordinate_system: CoordinateSystem,
    attributes: Vec<PointAttributeDefinition>,
    point_layout: PointLayout,
}

#[derive(Clone)]
pub struct WriteViewerClient {
    inner: Arc<Mutex<Inner>>,
}

pub struct ViewerClient {
    pub read: ReadViewerClient,
    pub write: WriteViewerClient,
}

#[derive(Clone)]
pub enum PartialResult<Points> {
    DeleteNode(LeveledGridCell),
    UpdateNode(NodeUpdate<Points>),
    Complete,
}

impl<P> PartialResult<P> {
    fn map<Q>(self, f: impl Fn(P) -> Q) -> PartialResult<Q> {
        match self {
            PartialResult::DeleteNode(n) => PartialResult::DeleteNode(n),
            PartialResult::UpdateNode(NodeUpdate { node_id, points }) => {
                PartialResult::UpdateNode(NodeUpdate {
                    node_id,
                    points: f(points),
                })
            }
            PartialResult::Complete => PartialResult::Complete,
        }
    }
}

impl<P, E> PartialResult<Result<P, E>> {
    fn result(self) -> Result<PartialResult<P>, E> {
        match self {
            PartialResult::DeleteNode(n) => Ok(PartialResult::DeleteNode(n)),
            PartialResult::UpdateNode(NodeUpdate { node_id, points }) => {
                Ok(PartialResult::UpdateNode(NodeUpdate {
                    node_id,
                    points: points?,
                }))
            }
            PartialResult::Complete => Ok(PartialResult::Complete),
        }
    }
}

#[derive(Clone)]
pub struct NodeUpdate<Points> {
    pub node_id: LeveledGridCell,
    pub points: Points,
}

impl Debug for NodeUpdate<VectorBuffer> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeUpdate")
            .field("node_id", &self.node_id)
            .field("[nr points]", &self.points.len())
            .finish()
    }
}

impl Debug for NodeUpdate<Vec<u8>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeUpdate")
            .field("node_id", &self.node_id)
            .field("[nr bytes]", &self.points.len())
            .finish()
    }
}

impl<P> Debug for PartialResult<P>
where
    NodeUpdate<P>: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeleteNode(arg0) => f
                .debug_tuple("PartialResult::DeleteNode")
                .field(arg0)
                .finish(),
            Self::UpdateNode(arg0) => f
                .debug_tuple("PartialResult::UpdateNode")
                .field(arg0)
                .finish(),
            Self::Complete => write!(f, "PartialResult::Complete"),
        }
    }
}

impl ViewerClient {
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
        let (coordinate_system, codec, attributes) = match pc_info {
            Message::PointCloudInfo {
                coordinate_system,
                codec,
                attributes,
            } => (coordinate_system, codec, attributes),
            _ => {
                return Err(LidarServerError::Protocol(
                    "Expected a `PointCloudInfo` message.".to_string(),
                ));
            }
        };
        let point_layout = PointLayout::from_attributes(&attributes);

        let (con_read, con_write) = connection.into_split();
        let inner = Inner {
            connection: con_write,
            last_ack: 0,
            ack_after: 5,
        };
        let inner = Arc::new(Mutex::new(inner));
        let write = WriteViewerClient {
            inner: Arc::clone(&inner),
        };
        let read = ReadViewerClient {
            connection: con_read,
            inner,
            codec,
            coordinate_system,
            attributes,
            point_layout,
        };

        Ok(ViewerClient { read, write })
    }
}

impl WriteViewerClient {
    async fn query_impl(&self, query: Query, config: QueryConfig) -> Result<(), LidarServerError> {
        let mut lock = self.inner.lock().await;
        lock.ack_after = if config.one_shot { 20 } else { 3 };
        lock.connection
            .write_message(&Message::Query { query, config })
            .await
    }

    pub async fn query(&self, query: Query) -> Result<(), LidarServerError> {
        self.query_impl(query, QueryConfig { one_shot: false })
            .await
    }

    pub async fn query_oneshot(&self, query: Query) -> Result<(), LidarServerError> {
        self.query_impl(query, QueryConfig { one_shot: true }).await
    }
}

impl ReadViewerClient {
    pub fn codec(&self) -> PointDataCodec {
        self.codec
    }

    pub fn coordinate_system(&self) -> CoordinateSystem {
        self.coordinate_system
    }

    pub fn attributes(&self) -> &[PointAttributeDefinition] {
        &self.attributes
    }

    pub async fn receive_update_raw(
        &mut self,
        shutdown: &mut Receiver<()>,
    ) -> Result<PartialResult<Vec<u8>>, LidarServerError> {
        match self.connection.read_message(shutdown).await? {
            Message::Node {
                node,
                points,
                update_number,
            } => {
                // send ack
                {
                    let mut lock = self.inner.lock().await;
                    if update_number >= lock.last_ack + lock.ack_after {
                        lock.connection
                            .write_message(&Message::ResultAck { update_number })
                            .await?;
                        lock.last_ack = update_number;
                    }
                }

                // result
                if let Some(PointData(points)) = points {
                    Ok(PartialResult::UpdateNode(NodeUpdate {
                        node_id: node,
                        points,
                    }))
                } else {
                    Ok(PartialResult::DeleteNode(node))
                }
            }
            Message::ResultComplete => Ok(PartialResult::Complete),
            _ => Err(LidarServerError::Protocol(
                "Expected an `IncrementalResult` or an `ResultComplete` message.".to_string(),
            )),
        }
    }

    pub async fn receive_update_local_coordinates(
        &mut self,
        shutdown: &mut Receiver<()>,
    ) -> Result<PartialResult<VectorBuffer>, LidarServerError> {
        self.receive_update_raw(shutdown)
            .await?
            .map(|point_data| {
                match self
                    .codec
                    .instance()
                    .read_points(&point_data, &self.point_layout)
                {
                    Ok((points, _)) => Ok(points),
                    Err(e) => Err(LidarServerError::Protocol(format!(
                        "Received invalid point buffer: {e}"
                    ))),
                }
            })
            .result()
    }

    pub async fn receive_update_global_coordinates(
        &mut self,
        shutdown: &mut Receiver<()>,
    ) -> Result<PartialResult<VectorBuffer>, LidarServerError> {
        let local = self.receive_update_local_coordinates(shutdown).await?;
        local
            .map(|points| {
                // src buffer
                let src_points = points;
                let src_layout = src_points.point_layout();

                // dst buffer
                let dst_attributes: Vec<_> = src_layout
                    .attributes()
                    .map(|a| {
                        if a.name() == POSITION_ATTRIBUTE_NAME {
                            POSITION_3D
                        } else {
                            a.attribute_definition().clone()
                        }
                    })
                    .collect();
                let dst_layout = PointLayout::from_attributes(&dst_attributes);
                let mut dst_points = VectorBuffer::new_from_layout(dst_layout.clone());
                dst_points.resize(src_points.len());

                // copy attributes
                for attribute in dst_attributes {
                    if attribute.name() == POSITION_ATTRIBUTE_NAME {
                        struct Wct<'a> {
                            coordinate_system: CoordinateSystem,
                            src_points: &'a VectorBuffer,
                            dst_points: &'a mut VectorBuffer,
                        }
                        impl<'a> WithComponentTypeOnce for Wct<'a> {
                            type Output = ();

                            fn run_once<C: lidarserv_common::geometry::position::Component>(
                                self,
                            ) -> Self::Output {
                                let Wct {
                                    coordinate_system,
                                    src_points,
                                    dst_points,
                                } = self;
                                let src_view =
                                    src_points.view_attribute::<C::PasturePrimitive>(
                                        &C::position_attribute(),
                                    );
                                let mut dst_view =
                                    dst_points.view_attribute_mut::<Vector3<f64>>(&POSITION_3D);
                                for i in 0..src_points.len() {
                                    let local_pos = C::pasture_to_position(src_view.at(i));
                                    let global_pos = coordinate_system.decode_position(local_pos);
                                    dst_view.set_at(i, global_pos.coords);
                                }
                            }
                        }
                        Wct {
                            coordinate_system: self.coordinate_system,
                            src_points: &src_points,
                            dst_points: &mut dst_points,
                        }
                        .for_layout_once(src_layout);
                    } else {
                        let src_member = src_layout.get_attribute(&attribute).unwrap();
                        let dst_member = dst_layout.get_attribute(&attribute).unwrap();
                        let src_view = src_points.view_raw_attribute(src_member);
                        let mut dst_view = dst_points.view_raw_attribute_mut(dst_member);
                        for i in 0..src_points.len() {
                            dst_view[i].copy_from_slice(&src_view[i]);
                        }
                    }
                }

                Ok(dst_points)
            })
            .result()
    }
}
