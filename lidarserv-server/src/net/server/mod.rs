use crate::net::LidarServerError;
use crate::net::protocol::messages::PointDataCodec;
use crate::net::server::connection::handle_connection;
use lidarserv_common::index::Octree;
use log::{error, info};
use std::sync::Arc;
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio::sync::broadcast::Receiver;

mod connection;

pub async fn serve<A>(
    addr: A,
    index: Octree,
    mut shutdown_receiver: Receiver<u32>,
) -> Result<(), LidarServerError>
where
    A: ToSocketAddrs,
{
    let listener = TcpListener::bind(addr).await?;
    info!("Ready to accept connections at: {}", listener.local_addr()?);

    let mut index = Arc::new(index);

    let (connections_alive_sender, mut connections_alive_receiver) =
        tokio::sync::mpsc::channel::<()>(1);
    let (connection_shutdown_broadcast, _) = tokio::sync::broadcast::channel(1);

    // todo - either make this configable or use the same codec used for the storage by the octree.
    let codec = PointDataCodec::Pasture { compression: false };

    loop {
        let accepted = tokio::select! {
            _ = shutdown_receiver.recv() => {

                // stop listening
                info!("Server shutdown: Stop accepting new connections.");
                drop(listener);

                // wait for all connections to be closed
                info!("Server shutdown: Waiting for connected clients to disconnect. Press Ctrl-C again, to force-close all connections.");
                drop(connections_alive_sender);
                loop {
                    tokio::select! {
                        _ = connections_alive_receiver.recv() => {
                            info!("Server shutdown: All connections closed.");

                            info!("Server shutdown: Flush to disk.");
                            // unwrap: all connections are closed, so this is the last ARC pointing to the index
                            let index = Arc::get_mut(&mut index).unwrap();
                            index.flush().map_err(|_| LidarServerError::IndexError)?;

                            info!("Server shutdown: Server stopped.");
                            return Ok(())
                        },
                        _ = shutdown_receiver.recv() => {
                            info!("Server shutdown: Force shutdown.");
                            connection_shutdown_broadcast.send(()).unwrap();
                        },
                    };
                }
            }
            a = listener.accept() => a
        };

        let (connection, addr) = accepted?;
        {
            let index = Arc::clone(&index);
            let connections_alive_sender = connections_alive_sender.clone();
            let connection_shutdown_receiver = connection_shutdown_broadcast.subscribe();
            tokio::spawn(async move {
                // handle connection
                let result =
                    handle_connection(connection, index, codec, connection_shutdown_receiver).await;
                if let Err(e) = result {
                    error!("{}: {}", addr, e);
                }

                // log eof
                info!("{}: Disconnect", addr);

                // dropping the connections_alive_sender tells the connections_alive_receiver,
                // that the connection is closed. THe application will only exit, once all
                // connections are closed.
                drop(connections_alive_sender);
            });
        }
    }
}
