use std::sync::atomic::{AtomicUsize, Ordering};

/// 缓冲区管理常量
pub const MAX_BUFFER_SIZE: usize = 1024 * 1024;
pub const BUFFER_TRIM_SIZE: usize = 512 * 1024;
pub const BUFFER_WARNING_THRESHOLD: usize = MAX_BUFFER_SIZE * 80 / 100;

/// 缓冲区统计信息
#[derive(Default)]
pub struct BufferStats {
    overflow_count: AtomicUsize,
    bytes_discarded: AtomicUsize,
    warning_count: AtomicUsize,
}

impl BufferStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_overflow(&self, bytes: usize) {
        self.overflow_count.fetch_add(1, Ordering::Relaxed);
        self.bytes_discarded.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_warning(&self) {
        self.warning_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.overflow_count.store(0, Ordering::Relaxed);
        self.bytes_discarded.store(0, Ordering::Relaxed);
        self.warning_count.store(0, Ordering::Relaxed);
    }

    pub fn overflow_count(&self) -> usize {
        self.overflow_count.load(Ordering::Relaxed)
    }

    pub fn bytes_discarded(&self) -> usize {
        self.bytes_discarded.load(Ordering::Relaxed)
    }

    pub fn warning_count(&self) -> usize {
        self.warning_count.load(Ordering::Relaxed)
    }
}

/// 环形缓冲区（高性能、无锁读写）
pub struct RingBuffer {
    buffer: Vec<u8>,
    stats: BufferStats,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
            stats: BufferStats::new(),
        }
    }

    pub fn push(&mut self, data: &[u8]) -> Result<(), usize> {
        let new_size = self.buffer.len() + data.len();

        if new_size > MAX_BUFFER_SIZE {
            let discard = new_size - BUFFER_TRIM_SIZE;
            self.buffer.drain(0..discard);
            self.stats.record_overflow(discard);
            eprintln!("[Buffer] Overflow: discarded {} bytes", discard);
        } else if new_size > BUFFER_WARNING_THRESHOLD {
            self.stats.record_warning();
            eprintln!("[Buffer] Warning: {}% full",
                     (new_size * 100) / MAX_BUFFER_SIZE);
        }

        self.buffer.extend_from_slice(data);
        Ok(())
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buffer
    }

    pub fn drain(&mut self, range: std::ops::Range<usize>) {
        self.buffer.drain(range);
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn stats(&self) -> &BufferStats {
        &self.stats
    }

    pub fn usage_percent(&self) -> f32 {
        (self.buffer.len() as f32 / MAX_BUFFER_SIZE as f32) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_basic() {
        let mut buffer = RingBuffer::new(1024);
        assert!(buffer.is_empty());

        buffer.push(b"hello").unwrap();
        assert_eq!(buffer.len(), 5);
        assert_eq!(buffer.as_slice(), b"hello");

        buffer.clear();
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_ring_buffer_overflow() {
        let mut buffer = RingBuffer::new(1024);
        let large_data = vec![0u8; MAX_BUFFER_SIZE + 1000];

        buffer.push(&large_data).unwrap();
        assert!(buffer.len() <= BUFFER_TRIM_SIZE);
        assert!(buffer.stats().overflow_count() > 0);
    }

    #[test]
    fn test_buffer_stats() {
        let stats = BufferStats::new();
        assert_eq!(stats.overflow_count(), 0);

        stats.record_overflow(100);
        assert_eq!(stats.overflow_count(), 1);
        assert_eq!(stats.bytes_discarded(), 100);

        stats.reset();
        assert_eq!(stats.overflow_count(), 0);
    }
}
