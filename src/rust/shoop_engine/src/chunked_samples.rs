//! Chunked sample storage backing audio channels.
//!
//! Samples live in fixed-size chunks so a growing recording never reallocates
//! and copies existing audio: growth appends a chunk. Chunk size is fixed at
//! construction, which lets index maths be a divide and a modulo.
//!
//! Pool-backed chunk allocation (the former pool crate) is not wired in
//! yet; chunks are owned `Vec`s. Recording growth therefore still allocates on
//! the audio thread, which the pool exists to avoid.

/// Chunks kept in reserve by default, so a growing recording does not allocate.
pub const DEFAULT_SPARE_CHUNKS: usize = 64;

/// Fixed-chunk sample store addressed by a flat sample offset.
///
/// Retired chunks go to a spare list rather than being dropped, and growth takes
/// from that list rather than allocating. Both matter on the audio thread, where
/// freeing is as costly as allocating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkedSamples<T> {
    chunk_size: usize,
    chunks: Vec<Vec<T>>,
    /// Pre-allocated chunks available for growth.
    spare: Vec<Vec<T>>,
    /// How often growth had to allocate because the spare list was empty.
    n_allocations: u32,
    /// Optional hard ceiling used by real-time recording storage.
    max_chunks: Option<usize>,
}

impl<T: Copy + Default> ChunkedSamples<T> {
    /// Starts with a single chunk and [`DEFAULT_SPARE_CHUNKS`] in reserve.
    pub fn with_chunk_size(chunk_size: usize) -> Self {
        Self::with_reserve(chunk_size, DEFAULT_SPARE_CHUNKS)
    }

    /// Starts with a single chunk and `n_spare` still in reserve after that.
    ///
    /// The reserve bounds how far a recording can grow without allocating; past it
    /// growth still works but allocates, which [`Self::n_allocations`] reports.
    pub fn with_reserve(chunk_size: usize, n_spare: usize) -> Self {
        assert!(chunk_size > 0, "chunk size must be non-zero");
        let mut s = Self {
            chunk_size,
            chunks: Vec::with_capacity(n_spare + 2),
            // One extra: the initial chunk is itself taken from the reserve, so
            // `n_spare` is what remains available for growth afterwards.
            spare: (0..n_spare + 1)
                .map(|_| vec![T::default(); chunk_size])
                .collect(),
            n_allocations: 0,
            max_chunks: None,
        };
        s.reset();
        s
    }

    /// Preallocates a hard-bounded store that never grows beyond `capacity` samples.
    pub fn with_bounded_capacity(chunk_size: usize, capacity: usize) -> Self {
        let mut result = Self::with_bounded_capacity_unprepared(chunk_size, capacity);
        result.prepare_bounded_capacity();
        result
    }

    /// Creates a hard-bounded store while deferring its sample reserve.
    pub fn with_bounded_capacity_unprepared(chunk_size: usize, capacity: usize) -> Self {
        let chunk_size = chunk_size.max(1);
        let n_chunks = capacity.max(1).div_ceil(chunk_size);
        let mut result = Self::with_reserve(chunk_size, 0);
        result.chunks.reserve(n_chunks.saturating_sub(1));
        result.spare.reserve(n_chunks.saturating_sub(1));
        result.max_chunks = Some(n_chunks);
        result
    }

    /// Allocates every chunk permitted by a bounded store.
    pub fn prepare_bounded_capacity(&mut self) {
        let Some(max_chunks) = self.max_chunks else {
            return;
        };
        let missing = max_chunks.saturating_sub(self.chunks.len() + self.spare.len());
        self.spare
            .extend((0..missing).map(|_| vec![T::default(); self.chunk_size]));
    }

    pub fn chunk_size(&self) -> usize {
        self.chunk_size
    }
    pub fn n_chunks(&self) -> usize {
        self.chunks.len()
    }
    pub fn n_spare(&self) -> usize {
        self.spare.len()
    }
    /// Times growth had to allocate. Non-zero means the reserve was too small.
    pub fn n_allocations(&self) -> u32 {
        self.n_allocations
    }
    /// Total addressable samples, i.e. capacity rather than recorded length.
    pub fn n_samples(&self) -> usize {
        self.chunks.len() * self.chunk_size
    }

    /// Takes a zeroed chunk from the reserve, allocating only if it is empty.
    fn take_chunk(&mut self) -> Vec<T> {
        match self.spare.pop() {
            Some(mut c) => {
                c.fill(T::default());
                c
            }
            None => {
                self.n_allocations += 1;
                vec![T::default(); self.chunk_size]
            }
        }
    }

    /// Returns to a single chunk, recycling the rest rather than freeing them.
    pub fn reset(&mut self) {
        while self.chunks.len() > 1 {
            let c = self.chunks.pop().expect("len > 1");
            self.spare.push(c);
        }
        match self.chunks.first_mut() {
            Some(c) => c.fill(T::default()),
            None => {
                let c = self.take_chunk();
                self.chunks.push(c);
            }
        }
    }

    pub fn get(&self, offset: usize) -> Option<&T> {
        self.chunks
            .get(offset / self.chunk_size)
            .map(|c| &c[offset % self.chunk_size])
    }

    pub fn get_mut(&mut self, offset: usize) -> Option<&mut T> {
        let (idx, head) = (offset / self.chunk_size, offset % self.chunk_size);
        self.chunks.get_mut(idx).map(|c| &mut c[head])
    }

    pub fn can_ensure_available(&self, offset: usize) -> bool {
        self.max_chunks
            .is_none_or(|max_chunks| offset / self.chunk_size < max_chunks)
    }

    /// Grows so `offset` is addressable. Returns whether chunks were added.
    pub fn ensure_available(&mut self, offset: usize) -> bool {
        let needed = offset / self.chunk_size;
        if !self.can_ensure_available(offset) {
            return false;
        }
        let changed = self.chunks.len() <= needed;
        while self.chunks.len() <= needed {
            let c = self.take_chunk();
            self.chunks.push(c);
        }
        changed
    }

    /// Samples remaining in the chunk containing `offset`. Callers use this to
    /// split a copy at chunk boundaries.
    pub fn space_for_sample(&self, offset: usize) -> usize {
        self.chunk_size - (offset % self.chunk_size)
    }

    /// Contiguous slice of the chunk containing `offset`, from `offset` to the
    /// end of that chunk. `None` when the offset is not addressable.
    pub fn chunk_slice(&self, offset: usize) -> Option<&[T]> {
        let (idx, head) = (offset / self.chunk_size, offset % self.chunk_size);
        self.chunks.get(idx).map(|c| &c[head..])
    }

    pub fn chunk_slice_mut(&mut self, offset: usize) -> Option<&mut [T]> {
        let (idx, head) = (offset / self.chunk_size, offset % self.chunk_size);
        self.chunks.get_mut(idx).map(|c| &mut c[head..])
    }

    /// Overwrites the first `length` samples with `value`, growing to fit.
    ///
    /// Allocation-free once the chunks exist, so it is safe to reach from the audio thread.
    pub fn fill(&mut self, length: usize, value: T)
    where
        T: Copy,
    {
        if length > 0 {
            self.ensure_available(length - 1);
        }
        let mut pos = 0;
        while pos < length {
            let want = self.space_for_sample(pos).min(length - pos);
            let Some(chunk) = self.chunk_slice_mut(pos) else {
                break;
            };
            let n = want.min(chunk.len());
            if n == 0 {
                break;
            }
            chunk[..n].fill(value);
            pos += n;
        }
    }

    /// Flattens up to `max_length` samples into one vector.
    pub fn contiguous_copy(&self, max_length: usize) -> Vec<T> {
        let mut remaining = self.n_samples().min(max_length);
        let mut out = Vec::with_capacity(remaining);
        for chunk in &self.chunks {
            if remaining == 0 {
                break;
            }
            let step = remaining.min(chunk.len());
            out.extend_from_slice(&chunk[..step]);
            remaining -= step;
        }
        out
    }

    /// Replaces contents, growing as needed. Trailing space in the last chunk is
    /// left at its default value.
    pub fn set_contents(&mut self, samples: &[T]) {
        self.reset();
        if samples.is_empty() {
            return;
        }
        let n_chunks = samples.len().div_ceil(self.chunk_size);
        self.ensure_available((n_chunks - 1) * self.chunk_size);
        for (i, chunk) in samples.chunks(self.chunk_size).enumerate() {
            self.chunks[i][..chunk.len()].copy_from_slice(chunk);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    #[shoop_wasm_test_support::shoop_test]
    fn starts_with_one_chunk() {
        let s = ChunkedSamples::<f32>::with_chunk_size(4);
        check!(s.n_chunks() == 1);
        check!(s.n_samples() == 4);
        check!(s.chunk_size() == 4);
        check!(s.get(0) == Some(&0.0));
        check!(s.get(3) == Some(&0.0));
        check!(s.get(4) == None);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn ensure_available_grows_by_chunk() {
        let mut s = ChunkedSamples::<f32>::with_chunk_size(4);
        check!(s.ensure_available(3) == false); // already addressable
        check!(s.n_chunks() == 1);
        check!(s.ensure_available(4) == true);
        check!(s.n_chunks() == 2);
        check!(s.n_samples() == 8);
        check!(s.ensure_available(4) == false);
        // Jumping several chunks ahead fills the gap.
        check!(s.ensure_available(20) == true);
        check!(s.n_chunks() == 6);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn indexing_spans_chunks() {
        let mut s = ChunkedSamples::<f32>::with_chunk_size(4);
        s.ensure_available(9);
        for i in 0..12 {
            assert2::assert!(let Some(v) = s.get_mut(i));
            *v = i as f32;
        }
        for i in 0..12 {
            check!(s.get(i) == Some(&(i as f32)));
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn space_for_sample_is_distance_to_chunk_end() {
        let s = ChunkedSamples::<f32>::with_chunk_size(4);
        check!(s.space_for_sample(0) == 4);
        check!(s.space_for_sample(1) == 3);
        check!(s.space_for_sample(3) == 1);
        check!(s.space_for_sample(4) == 4);
        check!(s.space_for_sample(5) == 3);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn chunk_slice_stops_at_chunk_boundary() {
        let mut s = ChunkedSamples::<f32>::with_chunk_size(4);
        s.ensure_available(7);
        assert2::assert!(let Some(sl) = s.chunk_slice(2));
        check!(sl.len() == 2);
        assert2::assert!(let Some(sl) = s.chunk_slice(4));
        check!(sl.len() == 4);
        check!(s.chunk_slice(8) == None);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn contiguous_copy_respects_max_length() {
        let mut s = ChunkedSamples::<f32>::with_chunk_size(4);
        s.set_contents(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        check!(s.n_chunks() == 2);
        check!(s.contiguous_copy(6) == vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        check!(s.contiguous_copy(3) == vec![1.0, 2.0, 3.0]);
        check!(s.contiguous_copy(0) == Vec::<f32>::new());
        // Beyond recorded content, capacity is returned (trailing defaults).
        check!(s.contiguous_copy(100) == vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 0.0, 0.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn set_contents_then_reset() {
        let mut s = ChunkedSamples::<f32>::with_chunk_size(4);
        s.set_contents(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        check!(s.n_chunks() == 2);
        check!(s.get(4) == Some(&5.0));
        check!(s.get(5) == Some(&0.0)); // padding
        s.reset();
        check!(s.n_chunks() == 1);
        check!(s.get(0) == Some(&0.0));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn set_contents_empty_keeps_one_chunk() {
        let mut s = ChunkedSamples::<f32>::with_chunk_size(4);
        s.set_contents(&[]);
        check!(s.n_chunks() == 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn exact_multiple_does_not_over_allocate() {
        let mut s = ChunkedSamples::<f32>::with_chunk_size(4);
        s.set_contents(&[1.0, 2.0, 3.0, 4.0]);
        check!(s.n_chunks() == 1);
        check!(s.n_samples() == 4);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn growth_takes_from_the_reserve_without_allocating() {
        let mut s = ChunkedSamples::<f32>::with_reserve(4, 3);
        // One chunk in use, the requested three still in reserve.
        check!(s.n_chunks() == 1);
        check!(s.n_spare() == 3);

        s.ensure_available(11); // needs 3 chunks, so two more
        check!(s.n_chunks() == 3);
        check!(s.n_spare() == 1);
        check!(s.n_allocations() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn growth_past_the_reserve_allocates_and_says_so() {
        let mut s = ChunkedSamples::<f32>::with_reserve(4, 1);
        check!(s.n_spare() == 1);
        // One spare covers a second chunk; a third has to allocate.
        s.ensure_available(11);
        check!(s.n_chunks() == 3);
        check!(s.n_allocations() == 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn bounded_capacity_refuses_growth_without_allocating() {
        let mut samples = ChunkedSamples::<f32>::with_bounded_capacity(4, 8);
        assert!(samples.can_ensure_available(7));
        assert!(!samples.can_ensure_available(8));
        samples.ensure_available(7);
        let allocations = samples.n_allocations();
        assert!(!samples.ensure_available(8));
        assert_eq!(samples.n_chunks(), 2);
        assert_eq!(samples.n_allocations(), allocations);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn bounded_capacity_can_be_prepared_on_demand() {
        let mut samples = ChunkedSamples::<f32>::with_bounded_capacity_unprepared(4, 12);
        assert_eq!(samples.n_chunks(), 1);
        assert_eq!(samples.n_spare(), 0);

        samples.prepare_bounded_capacity();
        assert_eq!(samples.n_spare(), 2);
        samples.ensure_available(11);
        assert_eq!(samples.n_chunks(), 3);
        assert_eq!(samples.n_allocations(), 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn reset_recycles_chunks_instead_of_freeing_them() {
        let mut s = ChunkedSamples::<f32>::with_reserve(4, 3);
        s.ensure_available(11);
        check!(s.n_chunks() == 3);
        check!(s.n_spare() == 1);

        s.reset();
        check!(s.n_chunks() == 1);
        // The two retired chunks went back to the reserve.
        check!(s.n_spare() == 3);

        // Growing again reuses them.
        s.ensure_available(11);
        check!(s.n_allocations() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn recycled_chunks_are_cleared_before_reuse() {
        let mut s = ChunkedSamples::<f32>::with_reserve(4, 3);
        s.ensure_available(7);
        for i in 0..8 {
            *s.get_mut(i).unwrap() = 9.0;
        }
        s.reset();
        s.ensure_available(7);
        // Stale samples must not reappear in a fresh recording.
        for i in 0..8 {
            check!(s.get(i) == Some(&0.0));
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn set_contents_reuses_the_reserve() {
        let mut s = ChunkedSamples::<f32>::with_reserve(4, 4);
        s.set_contents(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        check!(s.n_chunks() == 2);
        check!(s.contiguous_copy(5) == vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        check!(s.n_allocations() == 0);

        // Shorter contents release chunks back.
        s.set_contents(&[7.0]);
        check!(s.n_chunks() == 1);
        check!(s.get(0) == Some(&7.0));
        // And the tail of the reused chunk is clean.
        check!(s.get(1) == Some(&0.0));
    }

    #[shoop_wasm_test_support::shoop_test]
    #[should_panic(expected = "chunk size must be non-zero")]
    fn zero_chunk_size_rejected() {
        ChunkedSamples::<f32>::with_chunk_size(0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fill_overwrites_across_chunk_boundaries() {
        let mut c: ChunkedSamples<f32> = ChunkedSamples::with_chunk_size(4);
        c.set_contents(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]);

        // Spans three chunks and stops part-way through the last.
        c.fill(9, 0.0);

        check!(c.contiguous_copy(10) == vec![0.0; 9].into_iter().chain([10.0]).collect::<Vec<_>>());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fill_grows_to_the_length_asked_for() {
        let mut c: ChunkedSamples<f32> = ChunkedSamples::with_chunk_size(4);
        c.fill(6, 0.5);
        check!(c.contiguous_copy(6) == vec![0.5; 6]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn filling_nothing_leaves_the_contents_alone() {
        let mut c: ChunkedSamples<f32> = ChunkedSamples::with_chunk_size(4);
        c.set_contents(&[1.0, 2.0]);
        c.fill(0, 0.0);
        check!(c.contiguous_copy(2) == vec![1.0, 2.0]);
    }
}
