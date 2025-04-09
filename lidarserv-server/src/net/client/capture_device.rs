use crate::net::protocol::connection::Connection;
use crate::net::protocol::messages::{DeviceType, Header, PointDataCodec};
use crate::net::{LidarServerError, PROTOCOL_VERSION};
use lidarserv_common::geometry::coordinate_system::{CoordinateSystem, CoordinateSystemError};
use lidarserv_common::geometry::position::{Component, WithComponentTypeOnce};
use lidarserv_common::tracy_client::span;
use nalgebra::{Point3, Vector3};
use pasture_core::containers::{
    BorrowedBuffer, BorrowedBufferExt, BorrowedMutBufferExt, InterleavedBuffer,
    InterleavedBufferMut, MakeBufferFromLayout, OwningBuffer, VectorBuffer,
};
use pasture_core::layout::{PointAttributeDefinition, PointLayout};
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::sync::broadcast::Receiver;

/// A client that can send points to the server.
pub struct CaptureDeviceClient {
    connection: Connection<TcpStream>,
    coordinate_system: CoordinateSystem,
    attributes: Vec<PointAttributeDefinition>,
    codec: PointDataCodec,
}

impl CaptureDeviceClient {
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
            .write_message(
                &Header::Hello {
                    protocol_version: PROTOCOL_VERSION,
                },
                &[],
            )
            .await?;
        let hello = connection.read_message(shutdown).await?;
        match hello.header {
            Header::Hello { protocol_version } => {
                if protocol_version != PROTOCOL_VERSION {
                    return Err(LidarServerError::Protocol(format!(
                        "Protocol version mismatch (Server: {}, Client: {}).",
                        protocol_version, PROTOCOL_VERSION
                    )));
                }
            }
            _ => {
                return Err(LidarServerError::Protocol(
                    "Expected a `Hello` as the first message.".to_string(),
                ));
            }
        };

        // tell the server that we are a capture device, that would like to insert points.
        connection
            .write_message(
                &Header::ConnectionMode {
                    device: DeviceType::CaptureDevice,
                },
                &[],
            )
            .await?;

        // wait for the point cloud info.
        // We need that first, before we can start inserting points,
        // because it tells us how to encode the points (E.g. the las transformation (scale+offset))
        let pc_info = connection.read_message(shutdown).await?;
        let (coordinate_system, attributes, codec, _) = match pc_info.header {
            Header::PointCloudInfo {
                coordinate_system,
                attributes,
                codec,
                current_bounding_box,
            } => (coordinate_system, attributes, codec, current_bounding_box),
            _ => {
                return Err(LidarServerError::Protocol(
                    "Expected a `PointCloudInfo` message.".to_string(),
                ));
            }
        };

        Ok(CaptureDeviceClient {
            connection,
            coordinate_system,
            attributes,
            codec,
        })
    }

    pub fn coordinate_system(&self) -> CoordinateSystem {
        self.coordinate_system
    }

    pub fn attributes(&self) -> &[PointAttributeDefinition] {
        &self.attributes
    }

    pub fn codec(&self) -> PointDataCodec {
        self.codec
    }

    pub async fn insert_points_global_coordinates(
        &mut self,
        points: &VectorBuffer,
    ) -> Result<(), LidarServerError> {
        let global_position_attr = f64::position_attribute();
        let target_layout = PointLayout::from_attributes(&self.attributes);
        let mut target_buffer = VectorBuffer::new_from_layout(target_layout.clone());
        target_buffer.resize(points.len());
        for attribute in &self.attributes {
            if *attribute == global_position_attr {
                struct Wct<'a> {
                    src_buffer: &'a VectorBuffer,
                    target_buffer: &'a mut VectorBuffer,
                    coordinate_system: CoordinateSystem,
                }
                impl WithComponentTypeOnce for Wct<'_> {
                    type Output = Result<(), CoordinateSystemError>;

                    fn run_once<C: Component>(self) -> Self::Output {
                        let Self {
                            target_buffer,
                            src_buffer,
                            coordinate_system,
                        } = self;

                        let global_positions =
                            src_buffer.view_attribute::<Vector3<f64>>(&f64::position_attribute());
                        let mut local_positions = target_buffer
                            .view_attribute_mut::<C::PasturePrimitive>(&C::position_attribute());
                        for i in 0..src_buffer.len() {
                            let pos_global: Point3<f64> = global_positions.at(i).into();
                            let pos_local: Point3<C> =
                                coordinate_system.encode_position(pos_global)?;
                            local_positions.set_at(i, C::position_to_pasture(pos_local));
                        }
                        Ok(())
                    }
                }
                let result = Wct {
                    src_buffer: points,
                    target_buffer: &mut target_buffer,
                    coordinate_system: self.coordinate_system,
                }
                .for_layout_once(&target_layout);
                if let Err(e) = result {
                    return Err(LidarServerError::Client(format!("{e}")));
                }
            } else {
                let Some(src_attr_member) = points.point_layout().get_attribute(attribute) else {
                    return Err(LidarServerError::Client(format!(
                        "Missing attribute: {attribute:?}"
                    )));
                };
                let dst_attr_member = target_buffer
                    .point_layout()
                    .get_attribute(attribute)
                    .expect("created like this")
                    .clone();
                let src_view = points.view_raw_attribute(src_attr_member);
                let mut dst_view = target_buffer.view_raw_attribute_mut(&dst_attr_member);
                for i in 0..points.len() {
                    dst_view[i].copy_from_slice(&src_view[i]);
                }
            }
        }

        self.insert_points_local_coordinates(&target_buffer).await
    }

    pub async fn insert_points_local_coordinates(
        &mut self,
        points: &VectorBuffer,
    ) -> Result<(), LidarServerError> {
        // check attributes
        for attr in &self.attributes {
            if !points.point_layout().has_attribute(attr) {
                return Err(LidarServerError::Client(format!(
                    "Missing point attribute {attr:?}"
                )));
            }
        }

        let mut data = Vec::new();
        let _s1 = span!("CaptureDeviceClient::insert_points_local_coordinates encode point data");
        if let Err(e) = self.codec.instance().write_points(points, &mut data) {
            return Err(LidarServerError::Client(format!("Encoding error: {e}")));
        }
        drop(_s1);
        self.insert_raw_point_data(&data).await
    }

    pub async fn insert_raw_point_data(&mut self, data: &[u8]) -> Result<(), LidarServerError> {
        self.connection
            .write_message(&Header::InsertPoints, data)
            .await?;

        Ok(())
    }
}
