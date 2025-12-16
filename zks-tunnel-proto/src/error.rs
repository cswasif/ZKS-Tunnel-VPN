//! Protocol error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtoError {
    #[error("Invalid command byte: {0}")]
    InvalidCommand(u8),

    #[error("Empty message")]
    EmptyMessage,

    #[error("Insufficient data in message")]
    InsufficientData,

    #[error("Invalid UTF-8 in message")]
    InvalidUtf8,

    #[error("Frame too large: {0} bytes (max {1})")]
    FrameTooLarge(usize, usize),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
