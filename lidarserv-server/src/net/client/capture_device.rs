use crate::common::las::Las;
use crate::index::point::{GlobalPoint, LasPoint};
use crate::net::protocol::connection::Connection;
use crate::net::protocol::messages::{CoordinateSystem, DeviceType, LasPointData, Message};
use crate::net::{LidarServerError, PROTOCOL_VERSION};
use lidarserv_common::geometry::bounding_box::OptionAABB;
use lidarserv_common::geometry::position::I32CoordinateSystem;
use lidarserv_common::las::I32LasReadWrite;
use std::sync::Arc;
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::sync::broadcast::Receiver;

/// A client that can send points to the server.
pub struct CaptureDeviceClient {
    use_compression: bool,
    connection: Connection<TcpStream>,
    coordinate_system: CoordinateSystem,
}

impl CaptureDeviceClient {
    pub async fn connect<A>(
        addr: A,
        shutdown: &mut Receiver<()>,
        use_compression: bool,
    ) -> Result<Self, LidarServerError>
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
                        protocol_version, PROTOCOL_VERSION
                    )));
                }
            }
            _ => {
                return Err(LidarServerError::Protocol(
                    "Expected a `Hello` as the first message.".to_string(),
                ))
            }
        };

        // tell the server that we are a capture device, that would like to insert points.
        connection
            .write_message(&Message::ConnectionMode {
                device: DeviceType::CaptureDevice,
            })
            .await?;

        // wait for the point cloud info.
        // We need that first, before we can start inserting points,
        // because it tells us how to encode the points (E.g. the las transformation (scale+offset))
        let pc_info = connection.read_message(shutdown).await?;
        let coordinate_system = match pc_info {
            Message::PointCloudInfo { coordinate_system } => coordinate_system,
            _ => {
                return Err(LidarServerError::Protocol(
                    "Expected a `PointCloudInfo` message.".to_string(),
                ));
            }
        };

        Ok(CaptureDeviceClient {
            use_compression,
            connection,
            coordinate_system,
        })
    }

    pub async fn insert_points(
        &mut self,
        points: Vec<GlobalPoint>,
    ) -> Result<(), LidarServerError> {
        let data = match &self.coordinate_system {
            CoordinateSystem::I32CoordinateSystem { offset, scale } => {
                // convert to las points
                let coordinate_system = I32CoordinateSystem::from_las_transform(*scale, *offset);
                let las_points = points
                    .into_iter()
                    .map(|p| p.into_las_point(&coordinate_system))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| LidarServerError::Other(Box::new(e)))?;

                // encode as las
                let encoder = I32LasReadWrite::new(self.use_compression);
                encoder.write_las::<LasPoint, _>(Las {
                    points: las_points.iter(),
                    bounds: OptionAABB::empty(), // these bounds are technically wrong, but they do not matter for just sending them to the server.
                    non_bogus_points: None,
                    coordinate_system,
                })
            }
        };
        self.insert_raw_point_data(data).await
    }

    pub async fn insert_raw_point_data(&mut self, data: Vec<u8>) -> Result<(), LidarServerError> {
        self.connection
            .write_message(&Message::InsertPoints {
                data: LasPointData(Arc::new(data)),
            })
            .await?;

        Ok(())
    }
}
