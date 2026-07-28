//! Audio port fed by a driver: one buffer of samples per cycle.
//!
//! The staging arrangement is the same as [`crate::external_midi_port`], and for the
//! same reason: `prepare` runs partway through the schedule, ordered against the
//! channels that read the port, and clears the buffer so a cycle nobody fed reads as
//! silence. A driver therefore stages the cycle's input beforehand and `prepare`
//! takes it.
//!
//! Distinct from the internal audio port, which starts every cycle silent and is
//! written by whatever routes into it, and from the dummy port, whose queue spans
//! cycles so a test can set up a whole sequence up front.

use crate::port::{AudioPort, PortConnectability, PortDataType, PortDirection};

#[derive(Debug)]
pub struct ExternalAudioPort {
    name: String,
    direction: PortDirection,
    audio: AudioPort,
    /// Staged by the driver before the cycle; `prepare` moves it into `buffer`.
    staged: Vec<f32>,
    staged_len: usize,
    buffer: Vec<f32>,
    outgoing: Vec<f32>,
    processed_len: usize,
}

impl ExternalAudioPort {
    pub fn new(
        name: impl Into<String>,
        direction: PortDirection,
        ringbuffer_buffer_size: usize,
    ) -> Self {
        Self {
            name: name.into(),
            direction,
            audio: AudioPort::new(ringbuffer_buffer_size),
            staged: Vec::new(),
            staged_len: 0,
            buffer: Vec::new(),
            outgoing: Vec::new(),
            processed_len: 0,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn direction(&self) -> PortDirection {
        self.direction
    }
    pub fn data_type(&self) -> PortDataType {
        PortDataType::Audio
    }
    pub fn audio(&self) -> &AudioPort {
        &self.audio
    }
    pub fn audio_mut(&mut self) -> &mut AudioPort {
        &mut self.audio
    }

    pub fn has_internal_read_access(&self) -> bool {
        self.direction == PortDirection::Input
    }
    pub fn has_internal_write_access(&self) -> bool {
        self.direction == PortDirection::Output
    }
    pub fn has_implicit_input_source(&self) -> bool {
        self.direction == PortDirection::Input
    }
    pub fn has_implicit_output_sink(&self) -> bool {
        self.direction == PortDirection::Output
    }

    pub fn input_connectability(&self) -> PortConnectability {
        if self.direction == PortDirection::Input {
            PortConnectability::EXTERNAL
        } else {
            PortConnectability::INTERNAL
        }
    }
    pub fn output_connectability(&self) -> PortConnectability {
        if self.direction == PortDirection::Input {
            PortConnectability::INTERNAL
        } else {
            PortConnectability::EXTERNAL
        }
    }

    // --- driver interface ---

    /// Stages the next cycle's samples, for `prepare` to pick up.
    ///
    /// Sized on the first call and reused after, so a steady stream of equal-sized
    /// buffers does not allocate.
    pub fn stage_input(&mut self, samples: &[f32]) {
        if self.staged.len() < samples.len() {
            crate::realtime_allow_alloc_once!("ExternalAudioPort::stage_input resize", || {
                self.staged.resize(samples.len(), 0.0)
            });
        }
        self.staged[..samples.len()].copy_from_slice(samples);
        self.staged_len = samples.len();
    }

    /// Stages one channel out of an interleaved buffer.
    ///
    /// For a driver whose device hands over all channels together, as `cpal` does. Takes
    /// every `stride`-th sample starting at `offset`, so no per-channel buffer has to be
    /// built on the callback thread.
    pub fn stage_input_strided(&mut self, interleaved: &[f32], offset: usize, stride: usize) {
        if stride == 0 || offset >= stride {
            return;
        }
        let n = interleaved.len().saturating_sub(offset).div_ceil(stride);
        if self.staged.len() < n {
            crate::realtime_allow_alloc_once!(
                "ExternalAudioPort::stage_input_strided resize",
                || { self.staged.resize(n, 0.0) }
            );
        }
        for (i, slot) in self.staged[..n].iter_mut().enumerate() {
            *slot = interleaved[offset + i * stride];
        }
        self.staged_len = n;
    }

    /// This cycle's output, for the driver to hand to the backend.
    pub fn output(&self, n_frames: usize) -> &[f32] {
        let n = n_frames.min(self.buffer.len());
        &self.buffer[..n]
    }

    pub fn dequeue_output(&mut self, n_frames: usize) -> Vec<f32> {
        if self.direction == PortDirection::Output
            && self.outgoing.len() < n_frames
            && self.processed_len > 0
        {
            let n = self.processed_len.min(self.buffer.len());
            self.outgoing.extend_from_slice(&self.buffer[..n]);
            self.processed_len = 0;
        }
        let n = n_frames.min(self.outgoing.len());
        self.outgoing.drain(..n).collect()
    }

    pub fn clear_output_queue(&mut self) {
        self.outgoing.clear();
        self.processed_len = 0;
    }

    // --- port interface ---

    /// The port's buffer, grown if this cycle needs more room.
    pub fn buffer(&mut self, n_frames: usize) -> &mut [f32] {
        if n_frames > self.buffer.len() || self.buffer.is_empty() {
            crate::realtime_allow_alloc_once!("ExternalAudioPort::buffer resize", || {
                self.buffer.resize(n_frames.max(1), 0.0)
            });
        }
        &mut self.buffer[..n_frames]
    }

    /// Start of cycle: take whatever the driver staged, and silence the rest, so an
    /// unfed cycle is silent rather than a repeat of the last one.
    pub fn prepare(&mut self, n_frames: usize) {
        if self.direction == PortDirection::Output && self.processed_len > 0 {
            let n = self.processed_len.min(self.buffer.len());
            crate::realtime_allow_alloc_once!("ExternalAudioPort::prepare outgoing extend", || {
                self.outgoing.extend_from_slice(&self.buffer[..n])
            });
            self.processed_len = 0;
        }
        let staged = self.staged_len.min(n_frames);
        if n_frames > self.buffer.len() || self.buffer.is_empty() {
            crate::realtime_allow_alloc_once!("ExternalAudioPort::prepare buffer resize", || {
                self.buffer.resize(n_frames.max(1), 0.0)
            });
        }
        self.buffer[..staged].copy_from_slice(&self.staged[..staged]);
        for s in &mut self.buffer[staged..n_frames] {
            *s = 0.0;
        }
        self.staged_len = 0;
    }

    /// End of cycle: apply gain and muting, meter, and capture.
    pub fn process(&mut self, n_frames: usize) {
        if n_frames > self.buffer.len() || self.buffer.is_empty() {
            crate::realtime_allow_alloc_once!("ExternalAudioPort::process buffer resize", || {
                self.buffer.resize(n_frames.max(1), 0.0)
            });
        }
        let (buf, audio) = (&mut self.buffer[..n_frames], &mut self.audio);
        audio.process(buf);
        if self.direction == PortDirection::Output {
            self.processed_len = n_frames;
        }
    }

    pub fn close(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    fn in_port() -> ExternalAudioPort {
        ExternalAudioPort::new("in", PortDirection::Input, 4)
    }
    fn out_port() -> ExternalAudioPort {
        ExternalAudioPort::new("out", PortDirection::Output, 4)
    }

    #[test]
    fn access_follows_direction() {
        let i = in_port();
        check!(i.has_internal_read_access());
        check!(!i.has_internal_write_access());

        let o = out_port();
        check!(!o.has_internal_read_access());
        check!(o.has_internal_write_access());
    }

    #[test]
    fn staged_input_arrives_for_one_cycle_only() {
        let mut p = in_port();

        p.stage_input(&[0.25, 0.5, 0.75, 1.0]);
        p.prepare(4);
        check!(p.buffer(4) == [0.25, 0.5, 0.75, 1.0]);

        // Nothing staged for the next cycle, so it reads as silence rather than
        // repeating.
        p.prepare(4);
        check!(p.buffer(4) == [0.0; 4]);
    }

    #[test]
    fn a_strided_stage_takes_one_channel_out_of_an_interleaved_buffer() {
        let mut p = in_port();
        // Two channels interleaved: 1.0 on the left, 2.0 on the right.
        let interleaved = [1.0, 2.0, 1.0, 2.0, 1.0, 2.0];

        p.stage_input_strided(&interleaved, 0, 2);
        p.prepare(3);
        check!(p.buffer(3) == [1.0, 1.0, 1.0]);

        p.stage_input_strided(&interleaved, 1, 2);
        p.prepare(3);
        check!(p.buffer(3) == [2.0, 2.0, 2.0]);
    }

    #[test]
    fn a_nonsense_stride_stages_nothing() {
        let mut p = in_port();
        // An offset outside the stride would read the wrong channel, so refuse.
        p.stage_input_strided(&[1.0, 2.0], 2, 2);
        p.stage_input_strided(&[1.0, 2.0], 0, 0);
        p.prepare(2);
        check!(p.buffer(2) == [0.0, 0.0]);
    }

    #[test]
    fn a_short_stage_is_padded_with_silence() {
        let mut p = in_port();

        p.stage_input(&[1.0, 1.0]);
        p.prepare(4);
        check!(p.buffer(4) == [1.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn gain_and_muting_apply_on_the_way_out() {
        let mut p = out_port();
        p.audio_mut().set_gain(0.5);

        p.prepare(3);
        p.buffer(3).copy_from_slice(&[0.0, 1.0, 2.0]);
        p.process(3);
        check!(p.output(3) == [0.0, 0.5, 1.0]);

        p.audio_mut().set_muted(true);
        p.prepare(3);
        p.buffer(3).copy_from_slice(&[0.0, 1.0, 2.0]);
        p.process(3);
        check!(p.output(3) == [0.0, 0.0, 0.0]);
    }

    #[test]
    fn what_arrived_is_metered_even_when_muted() {
        let mut p = in_port();
        p.audio_mut().set_muted(true);

        p.stage_input(&[0.0, 0.9, 0.0]);
        p.prepare(3);
        p.process(3);

        check!(p.audio().input_peak() == 0.9);
        check!(p.audio().output_peak() == 0.0);
    }

    #[test]
    fn jack_audio_input_gain_and_mute_equivalent() {
        let mut p = in_port();
        p.audio_mut().set_gain(0.5);
        p.stage_input(&[0.0, 1.0, 2.0]);
        p.prepare(3);
        p.process(3);
        check!(p.buffer(3) == [0.0, 0.5, 1.0]);

        p.audio_mut().set_muted(true);
        p.stage_input(&[0.0, 1.0, 2.0]);
        p.prepare(3);
        p.process(3);
        check!(p.buffer(3) == [0.0, 0.0, 0.0]);
    }

    #[test]
    fn jack_audio_input_ringbuffer_snapshot_equivalent() {
        let mut p = in_port();
        p.audio_mut().set_ringbuffer_n_samples(4);
        p.stage_input(&[0.0, 0.1, 0.2, 0.3]);
        p.prepare(4);
        p.process(4);

        let snap = p.audio().ringbuffer_contents();
        check!(snap.n_samples >= 4);
        let data = snap.contiguous();
        check!(data.ends_with(&[0.0, 0.1, 0.2, 0.3]));
    }

    #[test]
    fn jack_audio_output_starts_next_cycle_silent_equivalent() {
        let mut p = out_port();
        p.prepare(5);
        p.buffer(5).copy_from_slice(&[0.0, 0.5, 0.9, 0.5, 0.0]);
        p.process(5);
        check!(p.output(5) == [0.0, 0.5, 0.9, 0.5, 0.0]);

        p.prepare(5);
        p.process(5);
        check!(p.output(5) == [0.0; 5]);
    }

    #[test]
    fn restaging_the_same_size_does_not_grow_the_buffer() {
        let mut p = in_port();
        p.stage_input(&[1.0, 2.0, 3.0, 4.0]);
        let cap = p.staged.capacity();
        p.prepare(4);
        p.stage_input(&[5.0, 6.0, 7.0, 8.0]);
        check!(p.staged.capacity() == cap);
        p.prepare(4);
        check!(p.buffer(4) == [5.0, 6.0, 7.0, 8.0]);
    }
}
