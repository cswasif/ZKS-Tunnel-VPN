//! Traffic Shaping Module
#![allow(dead_code)]
//!
//! Implements WTF-PAD (Website Traffic Fingerprinting Protection with Adaptive Defense)
//! for censorship resistance with minimal performance overhead.
//!
//! Features:
//! - Packet size normalization (pad to standard sizes)
//! - Timing obfuscation (async batching)
//! - Burst shaping (token bucket)
//! - Zero-copy buffer pooling
//! - Configurable modes (Fast/Balanced/Stealth)

use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::time::sleep;

/// Standard packet sizes for normalization (mimics HTTPS traffic)
#[allow(dead_code)]
const PACKET_SIZES: [usize; 3] = [
    536,  // TCP MSS for many networks
    1200, // Typical QUIC packet
    1460, // Ethernet MTU - headers
];

/// Traffic shaping configuration
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct TrafficShapingConfig {
    /// Enable packet padding
    pub packet_padding: bool,
    /// Enable timing obfuscation
    pub timing_obfuscation: bool,
    /// Enable burst shaping
    pub burst_shaping: bool,
    /// Target inter-packet delay (microseconds)
    pub target_delay_us: u64,
    /// Batch size for timing obfuscation
    pub batch_size: usize,
    /// Token bucket refill rate (tokens per second)
    pub refill_rate: f64,
}

impl TrafficShapingConfig {
    /// Fast mode: No shaping (maximum performance)
    pub fn fast() -> Self {
        Self {
            packet_padding: false,
            timing_obfuscation: false,
            burst_shaping: false,
            target_delay_us: 0,
            batch_size: 1,
            refill_rate: f64::INFINITY,
        }
    }

    /// Balanced mode: Minimal shaping (recommended)
    pub fn balanced() -> Self {
        Self {
            packet_padding: true,
            timing_obfuscation: false,
            burst_shaping: false,
            target_delay_us: 100, // 100μs = 0.1ms
            batch_size: 4,
            refill_rate: 10000.0, // 10 MB/s
        }
    }

    /// Stealth mode: Full shaping (maximum censorship resistance)
    pub fn stealth() -> Self {
        Self {
            packet_padding: true,
            timing_obfuscation: true,
            burst_shaping: true,
            target_delay_us: 500, // 500μs = 0.5ms
            batch_size: 8,
            refill_rate: 5000.0, // 5 MB/s
        }
    }
}

/// Buffer pool for zero-copy packet padding
#[allow(dead_code)]
pub struct BufferPool {
    /// Pre-allocated padding buffers (one per packet size)
    pools: [Vec<Vec<u8>>; 3],
}

impl BufferPool {
    /// Create new buffer pool with pre-allocated padding
    pub fn new() -> Self {
        let mut pools = [Vec::new(), Vec::new(), Vec::new()];

        // Pre-allocate 10 buffers per size
        for (i, &size) in PACKET_SIZES.iter().enumerate() {
            for _ in 0..10 {
                let mut buf = vec![0u8; size];
                // Fill with random padding
                getrandom::getrandom(&mut buf).ok();
                pools[i].push(buf);
            }
        }

        Self { pools }
    }

    /// Get a buffer of the specified size (reuse if available)
    pub fn get(&mut self, size: usize) -> Vec<u8> {
        let pool_idx = PACKET_SIZES.iter().position(|&s| s == size).unwrap_or(2);

        let mut buf = self.pools[pool_idx]
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(size));

        // Ensure buffer is correct size and filled with random data
        // (reused buffers are empty but have capacity)
        buf.resize(size, 0);
        getrandom::getrandom(&mut buf).ok();
        buf
    }

    /// Return a buffer to the pool for reuse
    pub fn return_buf(&mut self, mut buf: Vec<u8>) {
        let size = buf.capacity();
        if let Some(pool_idx) = PACKET_SIZES.iter().position(|&s| s == size) {
            buf.clear();
            // Limit pool size to prevent memory bloat
            if self.pools[pool_idx].len() < 20 {
                self.pools[pool_idx].push(buf);
            }
        }
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Traffic shaper for packet size normalization
#[allow(dead_code)]
pub struct TrafficShaper {
    /// Configuration
    config: TrafficShapingConfig,
    /// Buffer pool for zero-copy padding
    buffer_pool: BufferPool,
}

impl TrafficShaper {
    /// Create new traffic shaper with configuration
    pub fn new(config: TrafficShapingConfig) -> Self {
        Self {
            config,
            buffer_pool: BufferPool::new(),
        }
    }

    /// Pad packet to standard size (in-place, zero-copy)
    pub fn pad_packet(&mut self, packet: &mut Vec<u8>) {
        if !self.config.packet_padding {
            return;
        }

        let current_size = packet.len();
        let target_size = self.select_target_size(current_size);

        if target_size > current_size {
            // Extend with random padding (zero-copy from pool)
            let _padding_len = target_size - current_size;
            let padding_buf = self.buffer_pool.get(target_size);
            packet.extend_from_slice(&padding_buf[current_size..target_size]);
            self.buffer_pool.return_buf(padding_buf);
        }
    }

    /// Select target size for packet (smallest size that fits)
    fn select_target_size(&self, current: usize) -> usize {
        PACKET_SIZES
            .iter()
            .find(|&&size| size >= current)
            .copied()
            .unwrap_or(PACKET_SIZES[2])
    }
}

/// Timing shaper for inter-packet delay obfuscation
#[allow(dead_code)]
pub struct TimingShaper {
    /// Configuration
    config: TrafficShapingConfig,
    /// Last packet send time
    last_send: Instant,
    /// Batch buffer (amortize timing overhead)
    batch: VecDeque<Vec<u8>>,
}

impl TimingShaper {
    /// Create new timing shaper with configuration
    pub fn new(config: TrafficShapingConfig) -> Self {
        Self {
            config,
            last_send: Instant::now(),
            batch: VecDeque::with_capacity(config.batch_size),
        }
    }

    /// Send packet with timing obfuscation (async batching)
    pub async fn send_with_shaping<W: AsyncWrite + Unpin>(
        &mut self,
        writer: &mut W,
        packet: Vec<u8>,
    ) -> Result<(), std::io::Error> {
        if !self.config.timing_obfuscation {
            // Fast path: no batching
            writer.write_all(&packet).await?;
            return Ok(());
        }

        // Add to batch
        self.batch.push_back(packet);

        // Send batch when full (amortize delay overhead)
        if self.batch.len() >= self.config.batch_size {
            self.flush_batch(writer).await?;
        }

        Ok(())
    }

    /// Flush batch with timing delay
    async fn flush_batch<W: AsyncWrite + Unpin>(
        &mut self,
        writer: &mut W,
    ) -> Result<(), std::io::Error> {
        if self.batch.is_empty() {
            return Ok(());
        }

        // Calculate delay to match target rate
        let elapsed = self.last_send.elapsed();
        let target = Duration::from_micros(self.config.target_delay_us * self.batch.len() as u64);

        if elapsed < target {
            // Async sleep (non-blocking)
            sleep(target - elapsed).await;
        }

        // Send all packets in batch
        while let Some(packet) = self.batch.pop_front() {
            writer.write_all(&packet).await?;
        }

        self.last_send = Instant::now();
        Ok(())
    }

    /// Force flush remaining packets in batch
    pub async fn flush<W: AsyncWrite + Unpin>(
        &mut self,
        writer: &mut W,
    ) -> Result<(), std::io::Error> {
        self.flush_batch(writer).await
    }
}

/// Burst shaper using token bucket algorithm
#[allow(dead_code)]
pub struct BurstShaper {
    /// Configuration
    config: TrafficShapingConfig,
    /// Current token count
    tokens: f64,
    /// Maximum tokens
    max_tokens: f64,
    /// Last refill time
    last_refill: Instant,
}

impl BurstShaper {
    /// Create new burst shaper with configuration
    pub fn new(config: TrafficShapingConfig) -> Self {
        let max_tokens = config.refill_rate * 2.0; // 2 seconds worth of tokens
        Self {
            config,
            tokens: max_tokens,
            max_tokens,
            last_refill: Instant::now(),
        }
    }

    /// Shape burst (async wait if insufficient tokens)
    pub async fn shape_burst(&mut self, packet_size: usize) {
        if !self.config.burst_shaping {
            return;
        }

        let tokens_needed = packet_size as f64 / 1000.0; // 1 token per KB

        // Refill tokens
        let elapsed = self.last_refill.elapsed().as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.config.refill_rate).min(self.max_tokens);
        self.last_refill = Instant::now();

        // Wait if insufficient tokens (async, non-blocking)
        if self.tokens < tokens_needed {
            let wait_time = (tokens_needed - self.tokens) / self.config.refill_rate;
            sleep(Duration::from_secs_f64(wait_time)).await;
            self.tokens = 0.0;
        } else {
            self.tokens -= tokens_needed;
        }
    }
}

/// Combined traffic shaper with all features
#[allow(dead_code)]
pub struct CombinedTrafficShaper {
    traffic_shaper: TrafficShaper,
    timing_shaper: TimingShaper,
    burst_shaper: BurstShaper,
}

impl CombinedTrafficShaper {
    /// Create new combined traffic shaper
    pub fn new(config: TrafficShapingConfig) -> Self {
        Self {
            traffic_shaper: TrafficShaper::new(config),
            timing_shaper: TimingShaper::new(config),
            burst_shaper: BurstShaper::new(config),
        }
    }

    /// Send packet with full traffic shaping
    pub async fn send_shaped<W: AsyncWrite + Unpin>(
        &mut self,
        writer: &mut W,
        mut packet: Vec<u8>,
    ) -> Result<(), std::io::Error> {
        // 1. Pad packet to standard size
        self.traffic_shaper.pad_packet(&mut packet);

        // 2. Shape burst
        self.burst_shaper.shape_burst(packet.len()).await;

        // 3. Send with timing obfuscation
        self.timing_shaper.send_with_shaping(writer, packet).await
    }

    /// Flush any pending packets
    pub async fn flush<W: AsyncWrite + Unpin>(
        &mut self,
        writer: &mut W,
    ) -> Result<(), std::io::Error> {
        self.timing_shaper.flush(writer).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_size_selection() {
        let config = TrafficShapingConfig::balanced();
        let shaper = TrafficShaper::new(config);

        assert_eq!(shaper.select_target_size(100), 536);
        assert_eq!(shaper.select_target_size(536), 536);
        assert_eq!(shaper.select_target_size(537), 1200);
        assert_eq!(shaper.select_target_size(1200), 1200);
        assert_eq!(shaper.select_target_size(1201), 1460);
        assert_eq!(shaper.select_target_size(1460), 1460);
        assert_eq!(shaper.select_target_size(2000), 1460); // Max size
    }

    #[test]
    fn test_buffer_pool() {
        let mut pool = BufferPool::new();

        // Get buffer
        let buf1 = pool.get(536);
        assert_eq!(buf1.len(), 536);

        // Return buffer
        pool.return_buf(buf1);

        // Get again (should reuse)
        let buf2 = pool.get(536);
        assert_eq!(buf2.len(), 536);
    }

    #[tokio::test]
    async fn test_timing_shaper() {
        let config = TrafficShapingConfig::stealth();
        let mut shaper = TimingShaper::new(config);

        let mut output = Vec::new();
        let packet = vec![1, 2, 3, 4];

        // Send packet
        shaper
            .send_with_shaping(&mut output, packet.clone())
            .await
            .unwrap();

        // Should be batched (not sent yet)
        assert_eq!(output.len(), 0);

        // Send more packets to fill batch
        for _ in 0..config.batch_size {
            shaper
                .send_with_shaping(&mut output, packet.clone())
                .await
                .unwrap();
        }

        // Should be sent now
        assert!(output.len() > 0);
    }
}
