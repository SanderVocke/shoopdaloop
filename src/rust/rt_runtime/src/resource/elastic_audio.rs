#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ElasticAudioWork {
    pub slots: usize,
    pub free_slots: usize,
    pub capacity_slots: usize,
    pub arena_growths: u64,
    pub read_samples: u64,
    pub captured_samples: u64,
    pub reclaimed_chunks: u64,
    pub reserved_chunks: usize,
}
