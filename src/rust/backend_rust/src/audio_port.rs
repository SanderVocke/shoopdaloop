//! AudioPort - Audio port implementation in Rust.
//!
//! Provides audio port functionality with:
//! - Atomic peak meters (input/output)
//! - Atomic gain control
//! - Atomic mute state
//! - Ringbuffer (AudioBufferQueue) for retroactive recording
//!
//! The actual audio buffer is managed by C++ code. This Rust implementation
//! receives buffer pointers via FFI for processing (peak tracking, gain/mute).
//!
//! Ported from C++ AudioPort.h/cpp

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::audio_buffer_queue::AudioBufferQueue;

#[derive(Debug)]
pub struct AtomicF32 {
    inner: AtomicU32,
}

impl AtomicF32 {
    pub fn new(val: f32) -> Self {
        AtomicF32 {
            inner: AtomicU32::new(val.to_bits()),
        }
    }

    pub fn load(&self, order: Ordering) -> f32 {
        f32::from_bits(self.inner.load(order))
    }

    pub fn store(&self, val: f32, order: Ordering) {
        self.inner.store(val.to_bits(), order);
    }
}

impl Default for AtomicF32 {
    fn default() -> Self {
        Self::new(0.0)
    }
}

pub struct AudioPort {
    ringbuffer: AudioBufferQueue,
    input_peak: AtomicF32,
    output_peak: AtomicF32,
    gain: AtomicF32,
    muted: AtomicBool,
    single_buffer_size: usize,
}

impl AudioPort {
    pub fn new(
        pool_capacity: usize,
        low_water_mark: usize,
        buffer_size: usize,
        max_buffers: u32,
    ) -> Self {
        let ringbuffer =
            AudioBufferQueue::new(pool_capacity, low_water_mark, buffer_size, max_buffers);
        AudioPort {
            ringbuffer,
            input_peak: AtomicF32::new(0.0),
            output_peak: AtomicF32::new(0.0),
            gain: AtomicF32::new(1.0),
            muted: AtomicBool::new(false),
            single_buffer_size: buffer_size,
        }
    }

    pub unsafe fn process_raw(&mut self, buffer: *mut f32, n_frames: u32) {
        if n_frames == 0 || buffer.is_null() {
            return;
        }

        let buf = unsafe { std::slice::from_raw_parts_mut(buffer, n_frames as usize) };

        let muted = self.muted.load(Ordering::SeqCst);
        let gain = self.gain.load(Ordering::SeqCst);
        let mut input_peak_val: f32 = self.input_peak.load(Ordering::SeqCst);

        for sample in buf.iter_mut() {
            let abs_val = sample.abs();
            if abs_val > input_peak_val {
                input_peak_val = abs_val;
            }

            if muted {
                *sample = 0.0;
            } else {
                *sample *= gain;
            }
        }

        self.input_peak.store(input_peak_val, Ordering::SeqCst);

        let output_peak_val = if muted { 0.0 } else { input_peak_val * gain };
        let prev_output = self.output_peak.load(Ordering::SeqCst);
        self.output_peak
            .store(output_peak_val.max(prev_output), Ordering::SeqCst);

        if self.single_buffer_size > 0 && n_frames > 0 {
            let ringbuf_slice = &buf[..n_frames as usize];
            self.ringbuffer.put(ringbuf_slice);
        }
    }

    pub fn set_gain(&self, gain: f32) {
        self.gain.store(gain, Ordering::SeqCst);
    }

    pub fn get_gain(&self) -> f32 {
        self.gain.load(Ordering::SeqCst)
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::SeqCst);
    }

    pub fn get_muted(&self) -> bool {
        self.muted.load(Ordering::SeqCst)
    }

    pub fn get_input_peak(&self) -> f32 {
        self.input_peak.load(Ordering::SeqCst)
    }

    pub fn reset_input_peak(&self) {
        self.input_peak.store(0.0, Ordering::SeqCst);
    }

    pub fn get_output_peak(&self) -> f32 {
        self.output_peak.load(Ordering::SeqCst)
    }

    pub fn reset_output_peak(&self) {
        self.output_peak.store(0.0, Ordering::SeqCst);
    }

    pub fn set_ringbuffer_n_samples(&mut self, n: u32) {
        self.ringbuffer.set_min_n_samples(n);
    }

    pub fn get_ringbuffer_n_samples(&self) -> u32 {
        self.ringbuffer.n_samples()
    }

    pub fn get_single_buffer_size(&self) -> u32 {
        self.single_buffer_size as u32
    }

    pub fn get_ringbuffer_n_buffers(&self) -> usize {
        self.ringbuffer.n_buffers()
    }

    pub fn get_ringbuffer_contents_ptrs(&self) -> Vec<(*const f32, usize)> {
        let mut result = Vec::new();
        for buf in self.ringbuffer.iter_buffers() {
            if buf.len() > 0 {
                result.push((buf.data_ptr(), buf.len()));
            }
        }
        result
    }
}

#[repr(C)]
pub struct BufferPtrInfo {
    pub data_ptr: *const f32,
    pub len: usize,
}
