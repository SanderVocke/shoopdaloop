//! Transient PCM playback advanced by the engine's audio clock.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub struct AudioPreview {
    pub(crate) outputs: Vec<usize>,
    samples: Arc<[f32]>,
    position: usize,
    request_id: u64,
    finished: Arc<AtomicU64>,
}

impl AudioPreview {
    pub fn new(outputs: Vec<usize>, finished: Arc<AtomicU64>) -> Self {
        Self {
            outputs,
            samples: Arc::from([]),
            position: 0,
            request_id: 0,
            finished,
        }
    }

    pub fn replace(&mut self, request_id: u64, samples: &mut Arc<[f32]>) {
        std::mem::swap(&mut self.samples, samples);
        self.position = 0;
        self.request_id = request_id;
        self.finished.store(0, Ordering::Release);
    }

    pub(crate) fn next_block(&mut self, n_frames: usize) -> &[f32] {
        let _span = shoop_tracing::realtime_span!("engine.rt.preview");
        let start = self.position;
        self.position = start.saturating_add(n_frames).min(self.samples.len());
        if self.position == self.samples.len() && self.request_id != 0 {
            self.finished.store(self.request_id, Ordering::Release);
        }
        &self.samples[start..self.position]
    }
}
