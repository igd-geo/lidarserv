use crate::common::las::Las;
use crate::index::DynIndex;
use crate::net::protocol::connection::Connection;
use crate::net::protocol::messages::Message::IncrementalResult;
use crate::net::protocol::messages::{CoordinateSystem, DeviceType, LasPointData, Message, Query};
use crate::net::{LidarServerError, PROTOCOL_VERSION};
use lidarserv_common::geometry::bounding_box::{BaseAABB, OptionAABB};
use lidarserv_common::geometry::grid::LodLevel;
use lidarserv_common::geometry::position::{I32Position, Position};
use lidarserv_common::las::I32LasReadWrite;
use lidarserv_common::nalgebra::Point3;
use lidarserv_common::query::bounding_box::BoundingBoxQuery;
use lidarserv_common::query::view_frustum::ViewFrustumQuery;
use log::{debug, info};
use std::io::Cursor;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use tokio::net::TcpStream;
use tokio::sync::broadcast::Receiver;

pub async fn handle_connection(
    con: TcpStream,
    index: Arc<dyn DynIndex>,
    mut shutdown: Receiver<()>,
) -> Result<(), LidarServerError> {
    let addr = con.peer_addr()?;
    info!("New connection: {}", addr);
    con.set_nodelay(true)?;

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
            scale: *index.index_info().coordinate_system.scale(),
            offset: *index.index_info().coordinate_system.offset(),
        },
    })
    .await?;

    // wait for "Init" message from client.
    let msg = con.read_message(&mut shutdown).await?;
    if let Message::ConnectionMode { device } = msg {
        match device {
            DeviceType::Viewer => {
                viewer_mode(con, index, shutdown, addr).await?;
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
    mut con: Connection<TcpStream>,
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
        let read = Cursor::new(data.0.as_slice());
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

async fn viewer_mode(
    con: Connection<TcpStream>,
    index: Arc<dyn DynIndex>,
    mut shutdown: Receiver<()>,
    addr: SocketAddr,
) -> Result<(), LidarServerError> {
    let (mut con_read, mut con_write) = con.into_split();
    let coordinate_system = index.index_info().coordinate_system.clone();
    let sampling_factory = index.index_info().sampling_factory.clone();
    let (queries_sender, queries_receiver) = crossbeam_channel::unbounded();
    let (updates_sender, mut updates_receiver) = tokio::sync::mpsc::channel(1);
    let (query_ack_sender, query_ack_receiver) = crossbeam_channel::unbounded();

    let send_task = tokio::spawn(async move {
        while let Some(message) = updates_receiver.recv().await {
            match con_write.write_message(&message).await {
                Ok(_) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    });

    // query
    let query_thread = thread::spawn(move || -> Result<(), LidarServerError> {
        let mut sent_updates = 0;
        let mut ackd_updates = 0;
        let mut reader = index.reader();
        let mut queries_receiver = queries_receiver; // just to move it into the thread and make it mutable in here
        'update_loop: while reader.blocking_update(&mut queries_receiver) {
            if let Some((node_id, data)) = reader.load_one() {
                match updates_sender.blocking_send(IncrementalResult {
                    replaces: None,
                    nodes: vec![(node_id, data.into_iter().map(LasPointData).collect())],
                }) {
                    Ok(_) => sent_updates += 1,
                    Err(_) => break 'update_loop,
                }
            }
            if let Some(node_id) = reader.remove_one() {
                match updates_sender.blocking_send(IncrementalResult {
                    replaces: Some(node_id),
                    nodes: vec![],
                }) {
                    Ok(_) => sent_updates += 1,
                    Err(_) => break 'update_loop,
                }
            }
            if let Some((node_id, replacements)) = reader.update_one() {
                match updates_sender.blocking_send(IncrementalResult {
                    replaces: Some(node_id),
                    nodes: replacements
                        .into_iter()
                        .map(|(replacement_node_id, replacement_node_data)| {
                            (
                                replacement_node_id,
                                replacement_node_data
                                    .into_iter()
                                    .map(LasPointData)
                                    .collect(),
                            )
                        })
                        .collect(),
                }) {
                    Ok(_) => sent_updates += 1,
                    Err(_) => break 'update_loop,
                }
            }
            while ackd_updates + 10 < sent_updates {
                ackd_updates = match query_ack_receiver.recv() {
                    Ok(v) => v,
                    Err(_) => break 'update_loop,
                };
            }
        }
        Ok(())
    });

    // read incoming messages and send to queries to query thread
    let receive_queries = async move {
        while let Some(msg) = con_read.read_message_or_eof(&mut shutdown).await? {
            let query = match msg {
                Message::Query(q) => *q,
                Message::ResultAck { update_number } => {
                    query_ack_sender.send(update_number).ok();
                    continue;
                }
                _ => {
                    return Err(LidarServerError::Protocol(
                        "Expected `Query` message or EOF.".into(),
                    ));
                }
            };
            match query {
                Query::AabbQuery {
                    lod_level,
                    min_bounds,
                    max_bounds,
                } => {
                    let mut aabb = OptionAABB::empty();
                    for p in [min_bounds, max_bounds] {
                        let pos = match I32Position::encode(&coordinate_system, &Point3::from(p)) {
                            Ok(pos) => pos,
                            Err(e) => {
                                return Err(LidarServerError::Protocol(format!(
                                    "Received invalid query: {}",
                                    e
                                )));
                            }
                        };
                        aabb.extend(&pos);
                    }
                    let aabb = aabb.into_aabb().unwrap(); // unwrap: we just added two points, so it cannot be empty
                    let lod = LodLevel::from_level(lod_level);
                    let query = BoundingBoxQuery::new(aabb, lod);
                    debug!("{}: Query: {:?}", addr, &query);
                    queries_sender.send(Box::new(query)).unwrap();
                }
                Query::ViewFrustumQuery {
                    view_projection_matrix,
                    view_projection_matrix_inv,
                    window_width_pixels,
                    min_distance_pixels,
                } => {
                    let query = ViewFrustumQuery::new(
                        view_projection_matrix,
                        view_projection_matrix_inv,
                        window_width_pixels,
                        min_distance_pixels,
                        &sampling_factory,
                        &coordinate_system,
                    );
                    debug!("{}: Query: {:?}", addr, &query);
                    queries_sender.send(Box::new(query)).unwrap();
                }
            }
        }
        Ok(())
    };
    let result = receive_queries.await;

    // end query thread and wait for it to stop
    tokio::task::spawn_blocking(move || query_thread.join())
        .await
        .unwrap()
        .unwrap()?;
    send_task.await.unwrap()?;
    result
}
