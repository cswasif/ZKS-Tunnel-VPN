use crossbeam_queue::ArrayQueue;
use std::sync::Arc;

/// A pool of reusable packet buffers to minimize allocations.
///
/// This is critical for high-performance TUN I/O, as allocating a new
/// Vec<u8> for every packet (up to 1Mpps) causes significant GC/allocator pressure.
#[derive(Clone)]
pub struct PacketBufPool {
    pool: Arc<ArrayQueue<Vec<u8>>>,
    buf_size: usize,
}

impl PacketBufPool {
    /// Create a new packet buffer pool
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of buffers to hold in the pool
    /// * `buf_size` - Size of each buffer (typically MTU + overhead, e.g., 2048)
    pub fn new(capacity: usize, buf_size: usize) -> Self {
        Self {
            pool: Arc::new(ArrayQueue::new(capacity)),
            buf_size,
        }
    }

    /// Get a buffer from the pool, or allocate a new one if empty
    pub fn get(&self) -> Vec<u8> {
        match self.pool.pop() {
            Some(mut buf) => {
                // Ensure buffer is clear and has correct capacity
                buf.clear();
                if buf.capacity() < self.buf_size {
                    buf.reserve(self.buf_size - buf.len());
                }
                // Initialize with zeros up to buf_size is NOT needed for read(),
                // but we need to set length to buf_size so read() has space to write.
                // Actually, for read(), we usually pass a slice.
                // Let's just return the Vec with capacity.
                // The caller should resize it as needed.
                // For TUN reads, we typically want a buffer of `buf_size` length.
                unsafe { buf.set_len(self.buf_size) };
                buf
            }
            None => {
                // Pool empty, allocate new
                vec![0u8; self.buf_size]
            }
        }
    }

    /// Return a buffer to the pool
    pub fn return_buf(&self, mut buf: Vec<u8>) {
        // Only return if capacity is sufficient (don't recycle shrunk buffers)
        if buf.capacity() >= self.buf_size {
            // We don't need to zero it out, just clear length
            // But actually, we want to keep the allocation.
            // clear() sets len to 0 but keeps capacity.
            // However, if we push it back, we want to be sure it's ready for reuse.
            // The `get` method handles reset, so we just push it.
            let _ = self.pool.push(buf);
        }
    }
}
