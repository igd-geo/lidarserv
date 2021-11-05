use std::io::{Cursor, ErrorKind};
use std::net::SocketAddr;

use bytes::{Buf, BytesMut};
use log::trace;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::broadcast::Receiver;

use crate::net::protocol::messages::Message;
use crate::net::LidarServerError;

pub struct Connection<Stream> {
    stream: Stream,
    peer_addr: SocketAddr,
    buffer: BytesMut,
}

const HEADER_SIZE: usize = 8;
const MAGIC_NUMBER: &[u8] = "LidarServ Protocol".as_bytes();

#[derive(Error, Debug)]
#[error("The connection was closed unexpectedly.")]
struct ConnectionClosedError;

#[derive(Error, Debug)]
#[error("Protocol mismatch. The peer does not seem to speak the LidarServ protocol.")]
struct ProtocolMismatchError;

impl<Stream> Connection<Stream>
where
    Stream: AsyncRead + AsyncWrite + Unpin,
{
    pub async fn new(
        stream: Stream,
        peer_addr: SocketAddr,
        shutdown: &mut Receiver<()>,
    ) -> Result<Self, LidarServerError> {
        let mut con = Connection {
            stream,
            peer_addr,
            buffer: BytesMut::new(),
        };
        con.write_magic_number().await?;
        con.read_magic_number(shutdown).await?;
        Ok(con)
    }

    async fn read_magic_number(
        &mut self,
        shutdown: &mut Receiver<()>,
    ) -> Result<(), LidarServerError> {
        // receive magic number
        let mut read_buffer = vec![0; MAGIC_NUMBER.len()];
        tokio::select! {
            result = self.stream.read_exact(read_buffer.as_mut_slice()) => match result {
                Ok(_) => {}
                Err(e) => {
                    if e.kind() == ErrorKind::UnexpectedEof {
                        return Err(LidarServerError::WireProtocol(Box::new(e)));
                    } else {
                        return Err(LidarServerError::Net(e));
                    }
                }
            },
            _ = shutdown.recv() => return Err(LidarServerError::ServerShutdown),
        };

        // check received bytes
        if MAGIC_NUMBER != read_buffer {
            return Err(LidarServerError::WireProtocol(Box::new(
                ProtocolMismatchError,
            )));
        }

        Ok(())
    }

    async fn write_magic_number(&mut self) -> Result<(), LidarServerError> {
        self.stream.write_all(MAGIC_NUMBER).await?;
        Ok(())
    }

    fn try_read_frame(&mut self) -> Option<Result<Message, LidarServerError>> {
        // header: just a single u64, indicating the length of the full message.
        let len = if self.buffer.len() >= HEADER_SIZE {
            let mut len_bytes = [0_u8; HEADER_SIZE];
            len_bytes.copy_from_slice(&self.buffer[..HEADER_SIZE]);
            u64::from_le_bytes(len_bytes) as usize
        } else {
            return None;
        };

        // remaining bytes: raw message data
        if self.buffer.len() < len {
            return None;
        }
        let data = &self.buffer[HEADER_SIZE..len];

        // parse cbor message
        let message: Result<Message, _> = ciborium::de::from_reader(data)
            .map_err(|e| LidarServerError::WireProtocol(Box::new(e)));

        // pop message of the buffer
        self.buffer.advance(len);

        // treat any message of type [Message::Error] as an error.
        trace!("{}: Receive message: {:?}", &self.peer_addr, &message);
        if let Ok(Message::Error { message }) = message {
            Some(Err(LidarServerError::PeerError(message)))
        } else {
            Some(message)
        }
    }

    pub async fn read_message_or_eof(
        &mut self,
        shutdown: &mut Receiver<()>,
    ) -> Result<Option<Message>, LidarServerError> {
        loop {
            if let Some(result) = self.try_read_frame() {
                return result.map(Some);
            }

            // load more data
            let bytes_read = tokio::select! {
                read_buf_result = self.stream.read_buf(&mut self.buffer) => read_buf_result?,
                _ = shutdown.recv() => return Err(LidarServerError::ServerShutdown),
            };
            if bytes_read == 0 {
                return if !self.buffer.is_empty() {
                    // connection was closed in the middle of an incoming message - this is an error
                    Err(LidarServerError::WireProtocol(Box::new(
                        ConnectionClosedError,
                    )))
                } else {
                    // connection was closed after the last message was received completely
                    Ok(None)
                };
            }
        }
    }

    pub async fn read_message(
        &mut self,
        shutdown: &mut Receiver<()>,
    ) -> Result<Message, LidarServerError> {
        if let Some(msg) = self.read_message_or_eof(shutdown).await? {
            Ok(msg)
        } else {
            Err(LidarServerError::Protocol(format!(
                "{}",
                ConnectionClosedError
            )))
        }
    }

    pub async fn write_message(&mut self, message: &Message) -> Result<(), LidarServerError> {
        trace!("Send to {}: {:?}", &self.peer_addr, &message);
        let mut data = Vec::new();
        {
            let mut writer = Cursor::new(&mut data);

            // reserve space for header
            let empty_header = [0_u8; HEADER_SIZE];
            std::io::Write::write_all(&mut writer, &empty_header[..])?;

            // serialize data
            ciborium::ser::into_writer(message, &mut writer)
                .map_err(|e| LidarServerError::WireProtocol(Box::new(e)))?;
        }

        // overwrite header with actual message size
        let len = data.len() as u64;
        let len_bytes = len.to_le_bytes();
        data[0..HEADER_SIZE].copy_from_slice(&len_bytes);

        // send
        self.stream.write_all(&data[..]).await?;
        Ok(())
    }
}
