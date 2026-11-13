use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

/// Per-channel publication peaks, not an atomic stereo/state transaction.
#[derive(Clone)]
pub struct Meters(pub(crate) Arc<[AtomicU32; 2]>);

impl Meters {
    pub(crate) fn new() -> Self {
        Self(Arc::new([AtomicU32::new(0), AtomicU32::new(0)]))
    }
    /// Reset this channel's published peak. A racing publication belongs to this
    /// or a later read. NaNs are ignored; infinities saturate at f32::MAX.
    pub fn read_peak(&self, channel: usize) -> Option<f32> {
        self.0
            .get(channel)
            .map(|peak| f32::from_bits(peak.swap(0, Ordering::AcqRel)))
    }
    pub fn read_peaks(&self) -> [f32; 2] {
        std::array::from_fn(|channel| f32::from_bits(self.0[channel].swap(0, Ordering::AcqRel)))
    }
    pub(crate) fn publish(&self, peaks: [f32; 2]) {
        for (peak, value) in self.0.iter().zip(peaks) {
            peak.fetch_max(value.to_bits(), Ordering::Relaxed);
        }
    }
}
