use anyhow::anyhow;
use anyhow::Result;
use atomic_float::AtomicF32;
use audio_midi_io_traits::audio_port::AudioPortImpl;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::multi_buffer::MultiBufferProcessor;

pub struct AudioPortSharedState {
    input_peak: AtomicF32,
    output_peak: AtomicF32,
    gain: AtomicF32,
    muted: AtomicBool,
}

impl Default for AudioPortSharedState {
    fn default() -> Self {
        Self {
            input_peak: AtomicF32::new(0.0),
            output_peak: AtomicF32::new(0.0),
            gain: AtomicF32::new(1.0),
            muted: AtomicBool::new(false),
        }
    }
}

pub struct AudioPortProcessor {
    shared_state: Arc<AudioPortSharedState>,
    ringbuffer: Arc<MultiBufferProcessor>,
}

impl AudioPortProcessor {
    pub fn process(&mut self, buffer: &mut [f32]) -> Result<()> {
        let state = self.shared_state.as_ref();
        let muted = state.muted.load(Ordering::Relaxed);
        let gain = state.gain.load(Ordering::Relaxed);
        let mut input_peak = state.input_peak.load(Ordering::Relaxed);

        for sample in buffer.iter_mut() {
            input_peak = f32::max(input_peak, *sample);
            *sample = match muted {
                true => 0.0,
                false => *sample * gain,
            }
        }

        state.input_peak.store(input_peak, Ordering::Relaxed);
        state
            .output_peak
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |f| {
                Some(f32::max(f, input_peak * gain))
            })
            .map_err(|e| anyhow!(e))?;

        self.ringbuffer.put(buffer)?;

        Ok(())
    }
}

pub struct AudioPort {
    shared_state: Arc<AudioPortSharedState>,
}

impl AudioPortImpl for AudioPort {
    fn set_gain(self: &mut Self, gain: f32) -> Result<()> {
        self.shared_state.gain.store(gain, Ordering::Relaxed);
        Ok(())
    }

    fn get_gain(self: &Self) -> Result<f32> {
        Ok(self.shared_state.gain.load(Ordering::Relaxed))
    }

    fn reset_input_peak(self: &mut Self) -> Result<()> {
        self.shared_state.input_peak.store(0.0, Ordering::Relaxed);
        Ok(())
    }

    fn get_input_peak(self: &Self) -> Result<f32> {
        Ok(self.shared_state.input_peak.load(Ordering::Relaxed))
    }

    fn reset_output_peak(self: &mut Self) -> Result<()> {
        self.shared_state.output_peak.store(0.0, Ordering::Relaxed);
        Ok(())
    }

    fn get_output_peak(self: &Self) -> Result<f32> {
        Ok(self.shared_state.output_peak.load(Ordering::Relaxed))
    }

    fn set_muted(self: &mut Self, muted: bool) -> Result<()> {
        self.shared_state.muted.store(muted, Ordering::Relaxed);
        Ok(())
    }

    fn get_muted(self: &Self) -> Result<bool> {
        Ok(self.shared_state.muted.load(Ordering::Relaxed))
    }

    fn set_n_ringbuffer_samples(self: &mut Self, n_samples: u32) -> Result<()> {
        todo!()
    }

    fn get_n_ringbuffer_samples(self: &Self) -> Result<u32> {
        todo!()
    }
}
