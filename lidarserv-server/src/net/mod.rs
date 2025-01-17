pub mod client;
pub mod protocol;
pub mod server;

use thiserror::Error;

const PROTOCOL_VERSION: u32 = 4;

/// Error type for the indexing server.
#[derive(Error, Debug)]
pub enum LidarServerError {
    #[error("Client-Side error: {0}")]
    Client(String),

    #[error("Network error: {0}")]
    Net(#[from] std::io::Error),

    #[error("Wire protocol error: {0}")]
    WireProtocol(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("The connected peer reported an error: {0}")]
    PeerError(String),

    #[error("Operation was cancelled because of an application shutdown.")]
    ServerShutdown,

    #[error("Index error")]
    IndexError, // todo error details

    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}
