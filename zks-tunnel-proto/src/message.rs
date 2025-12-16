//! ZKS-Tunnel Protocol Messages
//!
//! Binary protocol for efficient tunneling:
//! - CONNECT: Request to open a TCP connection to a target
//! - DATA: Tunneled data (ZKS-encrypted payload)
//! - CLOSE: Close a stream
//! - ERROR: Error response

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::io::Cursor;

/// Maximum size of a single frame (1MB)
pub const MAX_FRAME_SIZE: usize = 1024 * 1024;

/// Command types for the tunnel protocol
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandType {
    /// Request to connect to a target address
    Connect = 0x01,
    /// Data frame (bidirectional)
    Data = 0x02,
    /// Close a stream
    Close = 0x03,
    /// Error response
    ErrorReply = 0x04,
    /// Ping/keepalive
    Ping = 0x05,
    /// Pong response
    Pong = 0x06,
}

impl TryFrom<u8> for CommandType {
    type Error = crate::ProtoError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(Self::Connect),
            0x02 => Ok(Self::Data),
            0x03 => Ok(Self::Close),
            0x04 => Ok(Self::ErrorReply),
            0x05 => Ok(Self::Ping),
            0x06 => Ok(Self::Pong),
            _ => Err(crate::ProtoError::InvalidCommand(value)),
        }
    }
}

/// Stream identifier for multiplexing connections
pub type StreamId = u32;

/// Protocol message types
#[derive(Debug, Clone)]
pub enum TunnelMessage {
    /// Connect to target: hostname:port
    Connect {
        stream_id: StreamId,
        host: String,
        port: u16,
    },
    /// Data payload for a stream
    Data {
        stream_id: StreamId,
        payload: Bytes,
    },
    /// Close a stream
    Close {
        stream_id: StreamId,
    },
    /// Error on a stream
    ErrorReply {
        stream_id: StreamId,
        code: u16,
        message: String,
    },
    /// Ping
    Ping,
    /// Pong
    Pong,
}

impl TunnelMessage {
    /// Encode message to binary format
    ///
    /// Format:
    /// - CONNECT: [cmd:1][stream_id:4][port:2][host_len:2][host:N]
    /// - DATA:    [cmd:1][stream_id:4][payload_len:4][payload:N]
    /// - CLOSE:   [cmd:1][stream_id:4]
    /// - ERROR:   [cmd:1][stream_id:4][code:2][msg_len:2][msg:N]
    /// - PING:    [cmd:1]
    /// - PONG:    [cmd:1]
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(256);

        match self {
            TunnelMessage::Connect { stream_id, host, port } => {
                buf.put_u8(CommandType::Connect as u8);
                buf.put_u32(*stream_id);
                buf.put_u16(*port);
                buf.put_u16(host.len() as u16);
                buf.put_slice(host.as_bytes());
            }
            TunnelMessage::Data { stream_id, payload } => {
                buf.put_u8(CommandType::Data as u8);
                buf.put_u32(*stream_id);
                buf.put_u32(payload.len() as u32);
                buf.put_slice(payload);
            }
            TunnelMessage::Close { stream_id } => {
                buf.put_u8(CommandType::Close as u8);
                buf.put_u32(*stream_id);
            }
            TunnelMessage::ErrorReply { stream_id, code, message } => {
                buf.put_u8(CommandType::ErrorReply as u8);
                buf.put_u32(*stream_id);
                buf.put_u16(*code);
                buf.put_u16(message.len() as u16);
                buf.put_slice(message.as_bytes());
            }
            TunnelMessage::Ping => {
                buf.put_u8(CommandType::Ping as u8);
            }
            TunnelMessage::Pong => {
                buf.put_u8(CommandType::Pong as u8);
            }
        }

        buf.freeze()
    }

    /// Decode message from binary format
    pub fn decode(data: &[u8]) -> Result<Self, crate::ProtoError> {
        if data.is_empty() {
            return Err(crate::ProtoError::EmptyMessage);
        }

        let mut cursor = Cursor::new(data);
        let cmd = CommandType::try_from(cursor.get_u8())?;

        match cmd {
            CommandType::Connect => {
                if cursor.remaining() < 8 {
                    return Err(crate::ProtoError::InsufficientData);
                }
                let stream_id = cursor.get_u32();
                let port = cursor.get_u16();
                let host_len = cursor.get_u16() as usize;
                
                if cursor.remaining() < host_len {
                    return Err(crate::ProtoError::InsufficientData);
                }
                let mut host_bytes = vec![0u8; host_len];
                cursor.copy_to_slice(&mut host_bytes);
                let host = String::from_utf8(host_bytes)
                    .map_err(|_| crate::ProtoError::InvalidUtf8)?;

                Ok(TunnelMessage::Connect { stream_id, host, port })
            }
            CommandType::Data => {
                if cursor.remaining() < 8 {
                    return Err(crate::ProtoError::InsufficientData);
                }
                let stream_id = cursor.get_u32();
                let payload_len = cursor.get_u32() as usize;
                
                if cursor.remaining() < payload_len {
                    return Err(crate::ProtoError::InsufficientData);
                }
                let payload = Bytes::copy_from_slice(&data[cursor.position() as usize..][..payload_len]);

                Ok(TunnelMessage::Data { stream_id, payload })
            }
            CommandType::Close => {
                if cursor.remaining() < 4 {
                    return Err(crate::ProtoError::InsufficientData);
                }
                let stream_id = cursor.get_u32();
                Ok(TunnelMessage::Close { stream_id })
            }
            CommandType::ErrorReply => {
                if cursor.remaining() < 8 {
                    return Err(crate::ProtoError::InsufficientData);
                }
                let stream_id = cursor.get_u32();
                let code = cursor.get_u16();
                let msg_len = cursor.get_u16() as usize;
                
                if cursor.remaining() < msg_len {
                    return Err(crate::ProtoError::InsufficientData);
                }
                let mut msg_bytes = vec![0u8; msg_len];
                cursor.copy_to_slice(&mut msg_bytes);
                let message = String::from_utf8(msg_bytes)
                    .map_err(|_| crate::ProtoError::InvalidUtf8)?;

                Ok(TunnelMessage::ErrorReply { stream_id, code, message })
            }
            CommandType::Ping => Ok(TunnelMessage::Ping),
            CommandType::Pong => Ok(TunnelMessage::Pong),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_roundtrip() {
        let msg = TunnelMessage::Connect {
            stream_id: 42,
            host: "google.com".to_string(),
            port: 443,
        };
        let encoded = msg.encode();
        let decoded = TunnelMessage::decode(&encoded).unwrap();
        
        match decoded {
            TunnelMessage::Connect { stream_id, host, port } => {
                assert_eq!(stream_id, 42);
                assert_eq!(host, "google.com");
                assert_eq!(port, 443);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_data_roundtrip() {
        let payload = Bytes::from("Hello, World!");
        let msg = TunnelMessage::Data {
            stream_id: 1,
            payload: payload.clone(),
        };
        let encoded = msg.encode();
        let decoded = TunnelMessage::decode(&encoded).unwrap();
        
        match decoded {
            TunnelMessage::Data { stream_id, payload: p } => {
                assert_eq!(stream_id, 1);
                assert_eq!(p, payload);
            }
            _ => panic!("Wrong message type"),
        }
    }
}
