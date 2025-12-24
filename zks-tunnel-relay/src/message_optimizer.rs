//! Message priority and optimization utilities for VPN room

use serde::{Deserialize, Serialize};

/// Message priority levels for queue management
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MessagePriority {
    /// Critical: Key exchange, authentication (send immediately)
    Critical = 0,
    /// High: Entropy, peer join/leave (send soon)
    High = 1,
    /// Normal: Chat, data packets (send normally)
    Normal = 2,
    /// Low: Heartbeat, stats (can be delayed)
    Low = 3,
}

impl MessagePriority {
    /// Determine priority from message content
    pub fn from_message(msg: &str) -> Self {
        // Check message type - support both snake_case and PascalCase
        if msg.contains("\"type\":\"auth\"")
            || msg.contains("\"type\":\"auth_init\"")
            || msg.contains("\"type\":\"auth_response\"")
            || msg.contains("\"type\":\"key_exchange\"")
            || msg.contains("KeyExchange")
            || msg.contains("AuthInit")
            || msg.contains("AuthResponse")
        {
            MessagePriority::Critical
        } else if msg.contains("\"type\":\"entropy\"")
            || msg.contains("\"type\":\"entropy_commit\"")
            || msg.contains("\"type\":\"entropy_reveal\"")
            || msg.contains("\"type\":\"peer_join\"")
            || msg.contains("\"type\":\"peer_leave\"")
            || msg.contains("PeerJoined")
            || msg.contains("PeerLeft")
        {
            MessagePriority::High
        } else if msg.contains("\"type\":\"ping\"")
            || msg.contains("\"type\":\"pong\"")
            || msg.contains("Pong")
        {
            MessagePriority::Low
        } else {
            MessagePriority::Normal
        }
    }

    /// Check if message should skip queue (critical)
    pub fn is_critical(&self) -> bool {
        matches!(self, MessagePriority::Critical)
    }
}

/// Compress message if it's large enough to benefit
#[allow(dead_code)]
pub fn maybe_compress(msg: &str) -> (Vec<u8>, bool) {
    const COMPRESSION_THRESHOLD: usize = 1024; // 1KB

    if msg.len() < COMPRESSION_THRESHOLD {
        // Too small, don't compress
        (msg.as_bytes().to_vec(), false)
    } else {
        // Try compression
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        if encoder.write_all(msg.as_bytes()).is_ok() {
            if let Ok(compressed) = encoder.finish() {
                // Only use if actually smaller
                if compressed.len() < msg.len() {
                    return (compressed, true);
                }
            }
        }

        // Compression failed or not beneficial
        (msg.as_bytes().to_vec(), false)
    }
}

/// Decompress message if it was compressed
#[allow(dead_code)]
pub fn maybe_decompress(data: &[u8], was_compressed: bool) -> Result<String, String> {
    if !was_compressed {
        String::from_utf8(data.to_vec()).map_err(|e| format!("UTF-8 decode error: {}", e))
    } else {
        use flate2::read::GzDecoder;
        use std::io::Read;

        let mut decoder = GzDecoder::new(data);
        let mut decompressed = String::new();
        decoder
            .read_to_string(&mut decompressed)
            .map_err(|e| format!("Decompression error: {}", e))?;
        Ok(decompressed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_detection() {
        assert_eq!(
            MessagePriority::from_message(r#"{"type":"auth_init"}"#),
            MessagePriority::Critical
        );

        assert_eq!(
            MessagePriority::from_message(r#"{"type":"entropy_commit"}"#),
            MessagePriority::High
        );

        assert_eq!(
            MessagePriority::from_message(r#"{"type":"ping"}"#),
            MessagePriority::Low
        );

        assert_eq!(
            MessagePriority::from_message(r#"{"type":"chat","msg":"hello"}"#),
            MessagePriority::Normal
        );
    }

    #[test]
    fn test_compression_threshold() {
        // Small message - should not compress
        let small = "hello";
        let (data, compressed) = maybe_compress(small);
        assert!(!compressed);
        assert_eq!(data, small.as_bytes());

        // Large message - should compress
        let large = "x".repeat(2000);
        let (data, compressed) = maybe_compress(&large);
        assert!(compressed);
        assert!(data.len() < large.len());
    }
}
