//! DummyAudioPort - Audio port with sample queue/dequeue logic for testing.
//!
//! Ported from C++ DummyAudioPort.h/cpp
//!
//! Note: Audio processing (gain/mute/peaks/ringbuffer) is handled by the
//! C++ RustAudioPortF32 base class. This Rust core only manages sample
//! queueing/buffering logic.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::port_core::PortCore;

pub struct DummyAudioPort {
    pub port_core: PortCore,

    // Input sample queue: Vec of sample frames to be fed into the port
    queued_data: Vec<Vec<f32>>,

    // Retained samples for dequeue_data
    retained_samples: Vec<f32>,

    // Current buffer (PROC_get_buffer returns a pointer into here)
    buffer_data: Vec<f32>,

    // Requested sample count for dequeue
    n_requested_samples: AtomicU32,
}

impl DummyAudioPort {
    pub fn new(name: &str, is_output: bool, driver_handle: usize) -> Self {
        use crate::port_direction::PortDirection;
        let direction = if is_output {
            PortDirection::Output
        } else {
            PortDirection::Input
        };
        DummyAudioPort {
            port_core: PortCore::new(name, direction, driver_handle),
            queued_data: Vec::new(),
            retained_samples: Vec::new(),
            buffer_data: Vec::new(),
            n_requested_samples: AtomicU32::new(0),
        }
    }

    pub fn name(&self) -> &str {
        self.port_core.name()
    }

    pub fn maybe_driver_handle(&self) -> usize {
        self.port_core.maybe_driver_handle()
    }

    /// Queue data for input ports
    pub fn queue_data(&mut self, n_frames: u32, data: &[f32]) {
        let frames_to_queue = std::cmp::min(n_frames as usize, data.len());
        let v = data[..frames_to_queue].to_vec();
        self.queued_data.push(v);
    }

    /// Check if input queue is empty
    pub fn get_queue_empty(&self) -> bool {
        self.queued_data.is_empty()
    }

    /// Request data for output ports
    pub fn request_data(&mut self, n_frames: u32) {
        self.n_requested_samples
            .fetch_add(n_frames, Ordering::SeqCst);
    }

    /// Dequeue retained samples
    pub fn dequeue_data(&mut self, n: u32) -> Vec<f32> {
        let s = self.retained_samples.len();
        let n = n as usize;
        if n > s {
            panic!("Not enough retained samples");
        }
        let result = self.retained_samples[..n].to_vec();
        self.retained_samples.drain(..n);
        result
    }

    /// Prepare for processing: drain queued data into buffer, zero-fill remainder
    pub fn proc_prepare(&mut self, nframes: u32) {
        let nframes = nframes as usize;
        if self.buffer_data.len() < nframes {
            self.buffer_data.resize(nframes, 0.0);
        }

        let mut filled = 0usize;
        while filled < nframes && !self.queued_data.is_empty() {
            let front = &mut self.queued_data[0];
            let to_copy = std::cmp::min(nframes - filled, front.len());
            self.buffer_data[filled..filled + to_copy].copy_from_slice(&front[..to_copy]);
            filled += to_copy;
            front.drain(..to_copy);
            if front.is_empty() {
                self.queued_data.remove(0);
            }
        }

        // Zero-fill remainder
        for sample in &mut self.buffer_data[filled..nframes] {
            *sample = 0.0;
        }
    }

    /// Process: copy buffer data to retained samples (for output ports)
    pub fn proc_process(&mut self, nframes: u32) {
        let nframes = nframes as usize;
        let to_store = std::cmp::min(
            nframes,
            self.n_requested_samples.load(Ordering::SeqCst) as usize,
        );
        if to_store > 0 {
            self.retained_samples
                .extend_from_slice(&self.buffer_data[..to_store]);
            self.n_requested_samples
                .fetch_sub(to_store as u32, Ordering::SeqCst);
        }
    }

    /// Get buffer pointer (ensure buffer is large enough)
    pub fn proc_get_buffer(&mut self, nframes: u32) -> *mut f32 {
        let nframes = nframes as usize;
        let new_size = std::cmp::max(self.buffer_data.len(), nframes);
        if new_size > self.buffer_data.len() {
            self.buffer_data.resize(new_size, 0.0);
        }
        self.buffer_data.as_mut_ptr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_and_prepare() {
        let mut port = DummyAudioPort::new("test", false, 0x1234);
        port.queue_data(4, &[1.0, 2.0, 3.0, 4.0]);
        port.proc_prepare(4);
        let buf = port.proc_get_buffer(4);
        unsafe {
            let slice = std::slice::from_raw_parts(buf, 4);
            assert_eq!(slice, &[1.0, 2.0, 3.0, 4.0]);
        }
    }

    #[test]
    fn test_queue_partial_prepare() {
        let mut port = DummyAudioPort::new("test", false, 0);
        port.queue_data(4, &[1.0, 2.0, 3.0, 4.0]);
        port.proc_prepare(2);
        let buf = port.proc_get_buffer(2);
        unsafe {
            let slice = std::slice::from_raw_parts(buf, 2);
            assert_eq!(slice, &[1.0, 2.0]);
        }
        port.proc_prepare(2);
        let buf = port.proc_get_buffer(2);
        unsafe {
            let slice = std::slice::from_raw_parts(buf, 2);
            assert_eq!(slice, &[3.0, 4.0]);
        }
    }

    #[test]
    fn test_zero_fill() {
        let mut port = DummyAudioPort::new("test", false, 0);
        port.proc_prepare(4);
        let buf = port.proc_get_buffer(4);
        unsafe {
            let slice = std::slice::from_raw_parts(buf, 4);
            assert_eq!(slice, &[0.0, 0.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn test_request_and_dequeue() {
        let mut port = DummyAudioPort::new("test", true, 0);
        port.request_data(4);
        port.proc_prepare(4);
        let buf = port.proc_get_buffer(4);
        unsafe {
            let slice = std::slice::from_raw_parts_mut(buf, 4);
            slice.copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        }
        port.proc_process(4);
        let dequeued = port.dequeue_data(4);
        assert_eq!(dequeued, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_dequeue_not_enough_panics() {
        let mut port = DummyAudioPort::new("test", true, 0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            port.dequeue_data(1);
        }));
        assert!(result.is_err());
    }
}
