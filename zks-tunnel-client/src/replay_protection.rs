//! Replay Attack Protection
//!
//! Prevents attackers from capturing and replaying encrypted messages.
//! Uses a time-based nonce window to track seen nonces.

// NOTE: This module is not yet integrated into P2P relay
// Suppress dead code warnings until integration is complete
#![allow(dead_code)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Maximum age for nonces (5 minutes)
const MAX_NONCE_AGE: Duration = Duration::from_secs(300);

/// Cleanup interval (1 minute)
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// Replay protection using nonce tracking
pub struct ReplayProtection {
    /// Map of seen nonces to their timestamp
    seen_nonces: HashMap<[u8; 12], Instant>,
    /// Maximum age for nonces
    max_age: Duration,
    /// Last cleanup time
    last_cleanup: Instant,
}

impl ReplayProtection {
    /// Create new replay protection
    pub fn new() -> Self {
        Self {
            seen_nonces: HashMap::new(),
            max_age: MAX_NONCE_AGE,
            last_cleanup: Instant::now(),
        }
    }

    /// Create with custom max age
    pub fn with_max_age(max_age: Duration) -> Self {
        Self {
            seen_nonces: HashMap::new(),
            max_age,
            last_cleanup: Instant::now(),
        }
    }

    /// Check if nonce is fresh and record it
    /// Returns true if nonce is fresh (not seen before)
    /// Returns false if nonce is a replay
    pub fn check_and_record(&mut self, nonce: &[u8; 12]) -> bool {
        let now = Instant::now();

        // Cleanup old nonces periodically (cleanup interval is half of max_age, capped at CLEANUP_INTERVAL)
        let cleanup_interval = self.max_age.min(CLEANUP_INTERVAL) / 2;
        if now.duration_since(self.last_cleanup) > cleanup_interval {
            self.cleanup_old_nonces();
            self.last_cleanup = now;
        }

        // Check if we've seen this nonce before
        if self.seen_nonces.contains_key(nonce) {
            // Replay detected!
            return false;
        }

        // Record this nonce
        self.seen_nonces.insert(*nonce, now);
        true
    }

    /// Remove nonces older than max_age
    fn cleanup_old_nonces(&mut self) {
        let now = Instant::now();
        self.seen_nonces
            .retain(|_, &mut timestamp| now.duration_since(timestamp) < self.max_age);
    }

    /// Get number of tracked nonces
    pub fn len(&self) -> usize {
        self.seen_nonces.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.seen_nonces.is_empty()
    }
}

impl Default for ReplayProtection {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ReplayProtection {
    fn drop(&mut self) {
        // Clear nonces from memory
        self.seen_nonces.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fresh_nonce() {
        let mut rp = ReplayProtection::new();
        let nonce = [1u8; 12];

        assert!(rp.check_and_record(&nonce));
        assert_eq!(rp.len(), 1);
    }

    #[test]
    fn test_replay_detection() {
        let mut rp = ReplayProtection::new();
        let nonce = [2u8; 12];

        // First time should succeed
        assert!(rp.check_and_record(&nonce));

        // Second time should fail (replay)
        assert!(!rp.check_and_record(&nonce));
    }

    #[test]
    fn test_different_nonces() {
        let mut rp = ReplayProtection::new();
        let nonce1 = [1u8; 12];
        let nonce2 = [2u8; 12];

        assert!(rp.check_and_record(&nonce1));
        assert!(rp.check_and_record(&nonce2));
        assert_eq!(rp.len(), 2);
    }

    #[test]
    fn test_cleanup() {
        let mut rp = ReplayProtection::with_max_age(Duration::from_millis(100));
        let nonce = [3u8; 12];

        rp.check_and_record(&nonce);
        assert_eq!(rp.len(), 1);

        // Wait for nonce to expire
        std::thread::sleep(Duration::from_millis(300));

        // Trigger cleanup by checking another nonce
        let nonce2 = [4u8; 12];
        rp.check_and_record(&nonce2);

        // Old nonce should be cleaned up
        assert_eq!(rp.len(), 1);
    }
}
