//! ZKS-Tunnel Protocol Definitions
//!
//! This crate defines the binary protocol for communication between
//! the ZKS-Tunnel client and worker.

mod message;
mod error;

pub use message::*;
pub use error::*;
