use crate::net::protocol::connection::Connection;
use crate::net::protocol::messages::Message::{self, Node, ResultComplete};
use crate::net::protocol::messages::{DeviceType, PointData, PointDataCodec, QueryConfig};
use crate::net::{LidarServerError, PROTOCOL_VERSION};
use crossbeam_channel::{RecvError, TryRecvError};
use lidarserv_common::index::Octree;
use lidarserv_common::query::empty::EmptyQuery;
use log::{debug, info, warn};
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use tokio::net::TcpStream;
use tokio::sync::broadcast::Receiver;

pub async fn handle_connection(
    con: TcpStream,
    index: Arc<Octree>,
    codec: PointDataCodec,
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
        coordinate_system: index.coordinate_system(),
        attributes: index
            .point_layout()
            .attributes()
            .map(|a| a.attribute_definition().clone())
            .collect(),
        codec,
    })
    .await?;

    // wait for "Init" message from client.
    let msg = match con.read_message_or_eof(&mut shutdown).await? {
        Some(msg) => msg,
        None => return Ok(()),
    };
    if let Message::ConnectionMode { device } = msg {
        match device {
            DeviceType::Viewer => {
                viewer_mode(con, index, shutdown, codec, addr).await?;
            }
            DeviceType::CaptureDevice => {
                capture_device_mode(con, index, codec, shutdown).await?;
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
    index: Arc<Octree>,
    codec: PointDataCodec,
    mut shutdown: Receiver<()>,
) -> Result<(), LidarServerError> {
    let codec = codec.instance();
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
        let points = match codec.read_points(&data, index.point_layout()) {
            Ok((points, _rest)) => points,
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
        writer.insert(&points);
    }
    Ok(())
}

/// Handle a connection in viewer mode (serverside).
/// This function will spawn a new thread that will handle the connection.
/// The thread will check for updates in the index and send them to the client.
async fn viewer_mode(
    con: Connection<TcpStream>,
    index: Arc<Octree>,
    mut shutdown: Receiver<()>,
    codec: PointDataCodec,
    addr: SocketAddr,
) -> Result<(), LidarServerError> {
    let (mut con_read, mut con_write) = con.into_split();
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
        let codec = codec.instance();
        let mut sent_updates = 0;
        let mut ackd_updates = 0;

        let mut query_config = QueryConfig { one_shot: true, point_filtering: true };
        let mut query_done = true;

        let mut reader = index.reader(EmptyQuery);
        let queries_receiver = queries_receiver; // just to move it into the thread and make it mutable in here
        let mut at_least_one_update = true;

        let error_msg = 'update_loop: loop {
            // update
            let maybe_new_query = if query_done {
                Some(queries_receiver.recv())
            } else if !query_config.one_shot && !at_least_one_update {
                reader.wait_update_or(&queries_receiver)
            } else {
                match queries_receiver.try_recv() {
                    Ok(ok) => Some(Ok(ok)),
                    Err(TryRecvError::Disconnected) => Some(Err(RecvError)),
                    Err(TryRecvError::Empty) => None,
                }
            };
            match maybe_new_query {
                Some(Ok((q, c))) => {
                    query_done = false;
                    query_config = c;
                    reader.update();
                    reader.set_query(q, query_config.point_filtering)
                }
                Some(Err(RecvError)) => return Ok(()),
                None => (),
            };

            at_least_one_update = false;

            // check for new nodes to load
            if let Some((node_id, points)) = reader.load_one() {
                at_least_one_update = true;
                sent_updates += 1;
                let mut data = Vec::new();
                if let Err(e) = codec.write_points(&points, &mut data) {
                    break 'update_loop format!("{e}");
                }
                if let Err(e) = updates_sender.blocking_send(Node {
                    node: node_id,
                    points: Some(PointData(data)),
                    update_number: sent_updates,
                }) {
                    break 'update_loop format!("{e}");
                }
            }

            // check for new nodes to remove
            if let Some(node_id) = reader.remove_one() {
                at_least_one_update = true;
                sent_updates += 1;
                if let Err(e) = updates_sender.blocking_send(Node {
                    node: node_id,
                    points: None,
                    update_number: sent_updates,
                }) {
                    break 'update_loop format!("{e}");
                }
            }

            // check for new nodes to update
            if let Some((node_id, points)) = reader.reload_one() {
                at_least_one_update = true;
                sent_updates += 1;
                let mut data = Vec::new();
                if let Err(e) = codec.write_points(&points, &mut data) {
                    break 'update_loop format!("{e}");
                }
                if let Err(e) = updates_sender.blocking_send(Node {
                    node: node_id,
                    points: Some(PointData(data)),
                    update_number: sent_updates,
                }) {
                    break 'update_loop format!("{e}");
                }
            }

            // notify client of EOF
            if query_config.one_shot && !at_least_one_update {
                query_done = true;
                if let Err(e) = updates_sender.blocking_send(ResultComplete) {
                    break 'update_loop format!("{e}");
                }
            }

            // wait for acks
            let max_nr_inflight_updates = if query_config.one_shot { 100 } else { 10 };
            while ackd_updates + max_nr_inflight_updates < sent_updates {
                ackd_updates = match query_ack_receiver.recv() {
                    Ok(v) => v,
                    Err(RecvError) => return Ok(()),
                };
            }
        };
        warn!("{addr}: Query thread finished unexpectedly: {error_msg}");
        updates_sender
            .blocking_send(Message::Error { message: error_msg })
            .ok();
        Ok(())
    });

    // read incoming messages and send to queries to query thread
    let receive_queries = async move {
        while let Some(msg) = con_read.read_message_or_eof(&mut shutdown).await? {
            let (query, query_config) = match msg {
                Message::Query { query, config } => (query, config),
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
            debug!("{addr}: Received Query: {:?}", query);
            debug!("{addr}: Config: {:?}", query_config);
            queries_sender.send((query, query_config)).unwrap();
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
