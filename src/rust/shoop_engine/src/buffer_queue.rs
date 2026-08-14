//! Rolling capture buffer: a bounded FIFO of fixed-size sample buffers.
//!
//! Backs the "grab" feature, where a loop is taken retroactively from audio that
//! was always being recorded. Once the queue is at its limit, adding a buffer
//! drops the oldest, so it holds a moving window of recent audio.
//!
//! Every buffer is allocated when the queue is built, so writing never allocates.
//! `BufferPool` to avoid allocating mid-capture, and a fixed ring achieves the same
//! thing without the sharing.

/// A point-in-time copy of the queue's contents.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// Oldest buffer first. The last one is only partly filled.
    pub buffers: Vec<Vec<f32>>,
    pub n_samples: usize,
    pub buffer_size: usize,
}

impl Snapshot {
    /// Contents flattened to the recorded length.
    pub fn contiguous(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.n_samples);
        for b in &self.buffers {
            if out.len() >= self.n_samples {
                break;
            }
            let take = (self.n_samples - out.len()).min(b.len());
            out.extend_from_slice(&b[..take]);
        }
        out
    }
}

#[derive(Debug)]
pub struct BufferQueue {
    /// Fixed ring of buffers, all allocated up front. Writing never allocates:
    /// once the ring is full the oldest buffer is overwritten in place.
    chunks: Vec<Vec<f32>>,
    chunk_size: usize,
    /// Requested limit, which may be zero; the ring always holds at least one.
    max_buffers: usize,
    /// Index of the buffer currently being filled.
    head: usize,
    /// How many buffers hold data.
    n_live: usize,
    /// Fill level of the head buffer. Starts at `chunk_size` so the first write
    /// advances onto a buffer rather than needing a special case.
    active_pos: usize,
}

impl BufferQueue {
    pub fn new(buffer_size: usize, max_buffers: usize) -> Self {
        assert!(buffer_size > 0, "buffer size must be non-zero");
        let capacity = max_buffers.max(1);
        Self {
            chunks: vec![vec![0.0; buffer_size]; capacity],
            chunk_size: buffer_size,
            max_buffers,
            head: 0,
            n_live: 0,
            active_pos: buffer_size,
        }
    }

    pub fn buffer_size(&self) -> usize {
        self.chunk_size
    }
    pub fn max_buffers(&self) -> usize {
        self.max_buffers
    }
    pub fn sample_capacity(&self) -> usize {
        self.max_buffers.saturating_mul(self.chunk_size)
    }
    pub fn n_buffers(&self) -> usize {
        self.n_live
    }

    fn capacity(&self) -> usize {
        self.chunks.len()
    }

    /// Samples currently held.
    pub fn n_samples(&self) -> usize {
        if self.n_live == 0 {
            return 0;
        }
        (self.n_live - 1) * self.chunk_size + self.active_pos
    }

    pub fn visit_range(&self, start: usize, end: usize, mut visit: impl FnMut(&[f32])) {
        let end = end.min(self.n_samples());
        let mut position = start.min(end);
        while position < end {
            let logical_chunk = position / self.chunk_size;
            let offset = position % self.chunk_size;
            let physical_chunk = (self.oldest() + logical_chunk) % self.capacity();
            let chunk_end = if logical_chunk + 1 == self.n_live {
                self.active_pos
            } else {
                self.chunk_size
            };
            let take = (chunk_end - offset).min(end - position);
            visit(&self.chunks[physical_chunk][offset..offset + take]);
            position += take;
        }
    }

    /// Index of the oldest live buffer.
    fn oldest(&self) -> usize {
        (self.head + self.capacity() - (self.n_live - 1)) % self.capacity()
    }

    /// Appends samples, moving onto the next ring buffer as each fills.
    pub fn put(&mut self, mut data: &[f32]) {
        let cap = self.capacity();
        while !data.is_empty() {
            if self.active_pos == self.chunk_size {
                // Advance onto the next buffer, retiring the oldest if full.
                if self.n_live == 0 {
                    self.head = 0;
                    self.n_live = 1;
                } else {
                    self.head = (self.head + 1) % cap;
                    if self.n_live < cap {
                        self.n_live += 1;
                    }
                }
                self.active_pos = 0;
            }
            let space = self.chunk_size - self.active_pos;
            let n = space.min(data.len());
            self.chunks[self.head][self.active_pos..self.active_pos + n]
                .copy_from_slice(&data[..n]);
            self.active_pos += n;
            data = &data[n..];
        }
    }

    /// Copies out the window, oldest buffer first.
    ///
    /// Allocates, so this is a control-thread operation: retroactive recording
    /// asks for it, the audio thread does not.
    pub fn snapshot(&self) -> Snapshot {
        let mut buffers = Vec::with_capacity(self.n_live);
        if self.n_live > 0 {
            let oldest = self.oldest();
            for i in 0..self.n_live {
                buffers.push(self.chunks[(oldest + i) % self.capacity()].clone());
            }
        }
        Snapshot {
            buffers,
            n_samples: self.n_samples(),
            buffer_size: self.chunk_size,
        }
    }

    /// Changes the limit, discarding everything held.
    ///
    /// in a fresh queue rather than trimming, so a resize always restarts the
    /// capture window.
    pub fn set_max_buffers(&mut self, max_buffers: usize) {
        self.max_buffers = max_buffers;
        self.chunks = vec![vec![0.0; self.chunk_size]; max_buffers.max(1)];
        self.head = 0;
        self.n_live = 0;
        self.active_pos = self.chunk_size;
    }

    /// Sets the limit to hold at least `n` samples.
    pub fn set_min_n_samples(&mut self, n: usize) {
        let bufs = n.div_ceil(self.chunk_size);
        self.set_max_buffers(bufs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    fn ramp(from: usize, n: usize) -> Vec<f32> {
        (from..from + n).map(|i| i as f32).collect()
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn starts_empty() {
        let q = BufferQueue::new(4, 3);
        check!(q.n_samples() == 0);
        check!(q.n_buffers() == 0);
        check!(q.snapshot().contiguous().is_empty());
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn first_write_allocates_a_buffer() {
        let mut q = BufferQueue::new(4, 3);
        q.put(&ramp(0, 2));
        check!(q.n_buffers() == 1);
        check!(q.n_samples() == 2);
        check!(q.snapshot().contiguous() == ramp(0, 2));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn writes_fill_then_spill_into_new_buffers() {
        let mut q = BufferQueue::new(4, 8);
        q.put(&ramp(0, 10));
        check!(q.n_buffers() == 3);
        check!(q.n_samples() == 10);
        check!(q.snapshot().contiguous() == ramp(0, 10));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn successive_writes_are_contiguous() {
        let mut q = BufferQueue::new(4, 8);
        q.put(&ramp(0, 3));
        q.put(&ramp(3, 3));
        q.put(&ramp(6, 5));
        check!(q.n_samples() == 11);
        check!(q.snapshot().contiguous() == ramp(0, 11));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn exactly_filling_a_buffer_does_not_allocate_early() {
        let mut q = BufferQueue::new(4, 8);
        q.put(&ramp(0, 4));
        check!(q.n_buffers() == 1);
        check!(q.n_samples() == 4);
        // The next sample is what triggers the new buffer.
        q.put(&ramp(4, 1));
        check!(q.n_buffers() == 2);
        check!(q.n_samples() == 5);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn oldest_buffers_are_retired_at_the_limit() {
        let mut q = BufferQueue::new(4, 2);
        q.put(&ramp(0, 12));
        check!(q.n_buffers() == 2);
        // Only the most recent 8 samples survive.
        check!(q.n_samples() == 8);
        check!(q.snapshot().contiguous() == ramp(4, 8));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_single_buffer_limit_keeps_only_the_newest() {
        let mut q = BufferQueue::new(4, 1);
        q.put(&ramp(0, 10));
        check!(q.n_buffers() == 1);
        check!(q.n_samples() == 2);
        check!(q.snapshot().contiguous() == ramp(8, 2));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn zero_limit_still_keeps_one_buffer() {
        // a zero limit is unsafe there. Here one buffer is always retained.
        let mut q = BufferQueue::new(4, 0);
        q.put(&ramp(0, 6));
        check!(q.n_buffers() == 1);
        check!(q.n_samples() == 2);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn snapshot_last_buffer_is_only_partly_filled() {
        let mut q = BufferQueue::new(4, 8);
        q.put(&ramp(0, 6));
        let s = q.snapshot();
        check!(s.buffers.len() == 2);
        check!(s.n_samples == 6);
        check!(s.buffer_size == 4);
        // The tail of the last buffer is vacant, so contiguous() trims it.
        check!(s.buffers[1].len() == 4);
        check!(s.contiguous().len() == 6);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn range_visitor_crosses_wrapped_chunks_without_copying() {
        let mut q = BufferQueue::new(4, 2);
        q.put(&ramp(0, 14));
        let mut visited = Vec::new();
        q.visit_range(1, 5, |samples| visited.extend_from_slice(samples));
        check!(visited == ramp(9, 4));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn snapshot_is_unaffected_by_later_writes() {
        let mut q = BufferQueue::new(4, 8);
        q.put(&ramp(0, 4));
        let s = q.snapshot();
        q.put(&ramp(100, 4));
        check!(s.n_samples == 4);
        check!(s.contiguous() == ramp(0, 4));
        check!(q.n_samples() == 8);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn set_max_buffers_discards_contents() {
        let mut q = BufferQueue::new(4, 8);
        q.put(&ramp(0, 10));
        q.set_max_buffers(2);
        check!(q.n_buffers() == 0);
        check!(q.n_samples() == 0);
        check!(q.max_buffers() == 2);
        // And refills from scratch.
        q.put(&ramp(0, 3));
        check!(q.snapshot().contiguous() == ramp(0, 3));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn set_min_n_samples_rounds_up() {
        let mut q = BufferQueue::new(4, 1);
        q.set_min_n_samples(9);
        check!(q.max_buffers() == 3); // ceil(9/4)
        q.set_min_n_samples(8);
        check!(q.max_buffers() == 2); // exact multiple
        q.set_min_n_samples(0);
        check!(q.max_buffers() == 0);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn writing_more_than_the_whole_window_at_once() {
        let mut q = BufferQueue::new(4, 2);
        // One write larger than the entire capacity: only the tail survives.
        q.put(&ramp(0, 20));
        check!(q.n_buffers() == 2);
        check!(q.n_samples() == 8);
        check!(q.snapshot().contiguous() == ramp(12, 8));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn empty_write_is_a_no_op() {
        let mut q = BufferQueue::new(4, 2);
        q.put(&[]);
        check!(q.n_buffers() == 0);
        check!(q.n_samples() == 0);
    }

    #[test]
    #[should_panic(expected = "buffer size must be non-zero")]
    fn zero_buffer_size_rejected() {
        BufferQueue::new(0, 4);
    }
}
