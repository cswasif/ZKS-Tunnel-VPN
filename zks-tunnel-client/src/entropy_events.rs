//! Entropy message types for Swarm Entropy collection via relay

use serde::{Deserialize, Serialize};

/// Entropy-related events sent via WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EntropyEvent {
    /// Commitment phase: peer sends hash of their entropy
    #[serde(rename = "entropy_commit")]
    Commit {
        peer_id: String,
        commitment: String, // hex-encoded SHA256 hash
    },

    /// Reveal phase: peer sends actual entropy after all committed
    #[serde(rename = "entropy_reveal")]
    Reveal {
        peer_id: String,
        entropy: String, // hex-encoded 32 bytes
    },

    /// Server notification: all entropies collected
    #[serde(rename = "entropy_ready")]
    Ready { peer_count: usize },
}

impl EntropyEvent {
    /// Create commitment message
    pub fn commit(peer_id: String, commitment: [u8; 32]) -> Self {
        Self::Commit {
            peer_id,
            commitment: hex::encode(commitment),
        }
    }

    /// Create reveal message
    pub fn reveal(peer_id: String, entropy: [u8; 32]) -> Self {
        Self::Reveal {
            peer_id,
            entropy: hex::encode(entropy),
        }
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Parse from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy_commit_serialization() {
        let commitment = [0xAB; 32];
        let event = EntropyEvent::commit("peer1".to_string(), commitment);

        let json = event.to_json().unwrap();
        let parsed = EntropyEvent::from_json(&json).unwrap();

        match parsed {
            EntropyEvent::Commit {
                peer_id,
                commitment: c,
            } => {
                assert_eq!(peer_id, "peer1");
                assert_eq!(c, hex::encode([0xAB; 32]));
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_entropy_reveal_serialization() {
        let entropy = [0xCD; 32];
        let event = EntropyEvent::reveal("peer2".to_string(), entropy);

        let json = event.to_json().unwrap();
        let parsed = EntropyEvent::from_json(&json).unwrap();

        match parsed {
            EntropyEvent::Reveal {
                peer_id,
                entropy: e,
            } => {
                assert_eq!(peer_id, "peer2");
                assert_eq!(e, hex::encode([0xCD; 32]));
            }
            _ => panic!("Wrong event type"),
        }
    }
}
