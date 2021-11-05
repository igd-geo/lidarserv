use crate::common::las::Las;
use crate::index::DynIndex;
use crate::net::protocol::connection::Connection;
use crate::net::protocol::messages::{CoordinateSystem, DeviceType, Message};
use crate::net::{LidarServerError, PROTOCOL_VERSION};
use lidarserv_common::las::{I32LasReadWrite, LasReadWrite};
use log::info;
use std::io::Cursor;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::broadcast::Receiver;

pub async fn handle_connection(
    con: &mut TcpStream,
    index: Arc<dyn DynIndex>,
    mut shutdown: Receiver<()>,
) -> Result<(), LidarServerError> {
    let addr = con.peer_addr()?;
    info!("New connection: {}", addr);

    // send "Hello" message
    let mut con = Connection::new(con, addr, &mut shutdown).await?;
    con.write_message(&Message::Hello {
        protocol_version: PROTOCOL_VERSION,
    })
    .await?;

    // receive "Hello" message from client and check protocol version compatibility
    let msg = con.read_message(&mut shutdown).await?;
    if let Message::Hello { protocol_version } = msg {
        if protocol_version != PROTOCOL_VERSION {
            return Err(LidarServerError::Protocol(format!(
                "Protocol version mismatch (Server: {}, Client: {}).",
                PROTOCOL_VERSION, protocol_version
            )));
        }
    } else {
        return Err(LidarServerError::Protocol(
            "Expected `Hello` message.".into(),
        ));
    }

    // send index information to client
    con.write_message(&Message::PointCloudInfo {
        coordinate_system: CoordinateSystem::I32CoordinateSystem {
            scale: *index.index_info().scale(),
            offset: *index.index_info().offset(),
        },
    })
    .await?;

    // wait for "Init" message from client.
    let msg = con.read_message(&mut shutdown).await?;
    if let Message::ConnectionMode { device } = msg {
        match device {
            DeviceType::Viewer => {
                todo!()
            }
            DeviceType::CaptureDevice => {
                capture_device_mode(con, index, shutdown).await?;
            }
        }
    } else {
        return Err(LidarServerError::Protocol(
            "Expected `Init` message.".into(),
        ));
    }

    Ok(())
}

async fn capture_device_mode(
    mut con: Connection<&'_ mut TcpStream>,
    index: Arc<dyn DynIndex>,
    mut shutdown: Receiver<()>,
) -> Result<(), LidarServerError> {
    let las_reader = I32LasReadWrite::new(false);
    let mut writer = index.writer();

    // keep receiving 'InsertPoints' messages, until the connection is closed
    while let Some(msg) = con.read_message_or_eof(&mut shutdown).await? {
        let data = if let Message::InsertPoints { data } = msg {
            data
        } else {
            let error = "Expected `InsertPoints` message or EOF.";
            con.write_message(&Message::Error {
                message: error.into(),
            })
            .await?;
            return Err(LidarServerError::Protocol(error.into()));
        };

        // decode las
        let read = Cursor::new(&data.0);
        let Las {
            points,
            coordinate_system,
            ..
        } = match las_reader.read_las(read) {
            Ok(r) => r,
            Err(e) => {
                let error = format!("Could not read LAS data: {}", e);
                con.write_message(&Message::Error {
                    message: error.clone(),
                })
                .await?;
                return Err(LidarServerError::Protocol(error));
            }
        };

        // insert
        if let Err(e) = writer.insert_points(points, &coordinate_system) {
            let message = format!("{}", e);
            con.write_message(&Message::Error {
                message: message.clone(),
            })
            .await?;
            return Err(LidarServerError::Protocol(message));
        }
    }
    Ok(())
}
