//! Port identity and the audio port's signal path.
//!
//! A port is how data moves in and out of the engine. Per cycle it is used in a
//! fixed order: prepare (acquire buffers), write, process (gain, metering,
//! capture), read.
//!
//! Only the parts that are independent of any audio driver live here. Naming,
//! external connections and buffer acquisition are driver-specific and belong
//! with the driver implementations.

use crate::buffer_queue::{BufferQueue, Snapshot};
use enum_iterator::Sequence;
use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Kind of data a port carries. Discriminants match `shoop_port_data_type_t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum PortDataType {
    Audio = 0,
    Midi = 1,
    Any = 2,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum PortConnectabilityKind {
    None = 0,
    Internal = 1,
    External = 2,
}

/// Discriminants match `shoop_port_direction_t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum PortDirection {
    Input = 0,
    Output = 1,
    Any = 2,
}

/// What a port may be connected to. Discriminants match
/// `shoop_port_connectability_t` and combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PortConnectability(pub u32);

impl PortConnectability {
    pub const NONE: Self = Self(0);
    pub const INTERNAL: Self = Self(1);
    pub const EXTERNAL: Self = Self(2);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
    pub fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    pub fn from_ffi(v: u32) -> Self {
        Self(v & (Self::INTERNAL.0 | Self::EXTERNAL.0))
    }
    pub fn to_ffi(&self) -> u32 {
        self.0
    }
}

/// Gain, muting, metering and always-on capture for an audio port.
///
/// The C++ `AudioPort` obtained its buffer through a virtual
/// `PROC_get_buffer`; here the buffer is passed in, so this type needs no
/// knowledge of where it came from.
#[derive(Debug)]
pub struct AudioPort {
    gain: f32,
    muted: bool,
    passthrough_muted: bool,
    input_peak: f32,
    output_peak: f32,
    ringbuffer: BufferQueue,
    /// An effect inserted on this port. Boxed because most ports have none and a chain carries a
    /// delay line.
    fx: Option<Box<crate::fx_chain::FxChain>>,
    /// Reused so an insert does not allocate per cycle.
    fx_scratch: Vec<f32>,
}

impl AudioPort {
    /// `ringbuffer_buffer_size` of zero disables always-on capture, matching a
    /// C++ port constructed without a buffer pool.
    pub fn new(ringbuffer_buffer_size: usize) -> Self {
        Self {
            gain: 1.0,
            muted: false,
            passthrough_muted: false,
            input_peak: 0.0,
            output_peak: 0.0,
            fx: None,
            fx_scratch: Vec::new(),
            // 32 buffers is the C++ initial reservation.
            ringbuffer: BufferQueue::new(ringbuffer_buffer_size.max(1), 32),
        }
    }

    /// A port with no capture buffer at all.
    pub fn without_ringbuffer() -> Self {
        let mut p = Self::new(1);
        p.ringbuffer.set_max_buffers(0);
        p
    }

    pub fn data_type(&self) -> PortDataType {
        PortDataType::Audio
    }

    pub fn gain(&self) -> f32 {
        self.gain
    }
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain;
    }
    pub fn muted(&self) -> bool {
        self.muted
    }
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }
    pub fn passthrough_muted(&self) -> bool {
        self.passthrough_muted
    }
    pub fn set_passthrough_muted(&mut self, muted: bool) {
        self.passthrough_muted = muted;
    }

    pub fn input_peak(&self) -> f32 {
        self.input_peak
    }
    pub fn reset_input_peak(&mut self) {
        self.input_peak = 0.0;
    }
    pub fn output_peak(&self) -> f32 {
        self.output_peak
    }
    pub fn reset_output_peak(&mut self) {
        self.output_peak = 0.0;
    }

    /// Samples currently retained for retroactive recording.
    pub fn ringbuffer_n_samples(&self) -> usize {
        self.ringbuffer.n_samples()
    }
    /// Sets the retained window. Discards what is held, as the C++ does.
    pub fn set_ringbuffer_n_samples(&mut self, n: usize) {
        self.ringbuffer.set_min_n_samples(n);
    }
    pub fn ringbuffer_contents(&self) -> Snapshot {
        self.ringbuffer.snapshot()
    }

    /// The effect inserted on this port, if any.
    pub fn fx(&self) -> Option<&crate::fx_chain::FxChain> {
        self.fx.as_deref()
    }
    pub fn fx_mut(&mut self) -> Option<&mut crate::fx_chain::FxChain> {
        self.fx.as_deref_mut()
    }
    /// Inserts an effect, or removes one. Allocates, so it is a control-path call.
    pub fn set_fx(&mut self, fx: Option<crate::fx_chain::FxChain>) {
        self.fx = fx.map(Box::new);
    }

    /// Applies gain or muting in place, updates meters, and captures the result.
    ///
    /// Peaks accumulate until explicitly reset, and `output_peak` is derived from
    /// the accumulated `input_peak` rather than this cycle's alone — faithful to
    /// the C++, which means a loud earlier cycle keeps inflating it until reset.
    pub fn process(&mut self, buf: &mut [f32]) {
        // The insert runs before gain and muting, so the fader is post-effect and muting silences
        // the effect's output too rather than leaving a tail audible.
        if let Some(fx) = self.fx.as_deref_mut() {
            // In place: the chain writes what it reads, and a port has one buffer.
            let mut scratch = std::mem::take(&mut self.fx_scratch);
            if scratch.len() < buf.len() {
                // Only when a cycle grows, which is off the steady state.
                scratch.resize(buf.len(), 0.0);
            }
            let n = buf.len();
            scratch[..n].copy_from_slice(&buf[..n]);
            fx.process(&scratch[..n], &mut buf[..n]);
            self.fx_scratch = scratch;
        }

        let mut input_peak = self.input_peak;
        for s in buf.iter_mut() {
            input_peak = input_peak.max(s.abs());
            if self.muted {
                *s = 0.0;
            } else {
                *s *= self.gain;
            }
        }
        self.input_peak = input_peak;
        let candidate = if self.muted {
            0.0
        } else {
            input_peak * self.gain
        };
        self.output_peak = self.output_peak.max(candidate);

        // Capture happens after gain and muting, so what was heard is what is
        // available for retroactive recording.
        if self.ringbuffer.max_buffers() > 0 {
            self.ringbuffer.put(buf);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_inserted_effect_processes_the_signal() {
        use crate::fx_chain::{EffectKind, FxChain};

        let mut p = AudioPort::new(0);
        let mut fx = FxChain::default();
        fx.configure(48000);
        fx.set_kind(EffectKind::Delay);
        fx.set_delay_seconds(0.001); // 48 samples
        fx.set_feedback(0.0);
        p.set_fx(Some(fx));

        let mut buf = vec![0.0f32; 256];
        buf[0] = 1.0;
        p.process(&mut buf);

        // Delayed rather than passed straight through.
        check!(buf[0] == 0.0);
        check!(buf[48] > 0.9);
    }

    #[test]
    fn no_insert_leaves_the_signal_alone() {
        let mut p = AudioPort::new(0);
        let mut buf = vec![0.5f32; 16];
        p.process(&mut buf);
        // Unity gain, unmuted, no effect.
        check!(buf.iter().all(|&v| v == 0.5));
    }

    #[test]
    fn muting_silences_the_effect_too() {
        use crate::fx_chain::{EffectKind, FxChain};

        let mut p = AudioPort::new(0);
        let mut fx = FxChain::default();
        fx.configure(48000);
        fx.set_kind(EffectKind::Delay);
        p.set_fx(Some(fx));
        p.set_muted(true);

        let mut buf = vec![1.0f32; 256];
        p.process(&mut buf);
        // The insert runs first, so muting has to come after it or a tail would leak.
        check!(buf.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn an_insert_can_be_removed() {
        use crate::fx_chain::{EffectKind, FxChain};

        let mut p = AudioPort::new(0);
        let mut fx = FxChain::default();
        fx.configure(48000);
        fx.set_kind(EffectKind::Delay);
        p.set_fx(Some(fx));
        check!(p.fx().is_some());

        p.set_fx(None);
        check!(p.fx().is_none());
        let mut buf = vec![0.25f32; 16];
        p.process(&mut buf);
        check!(buf.iter().all(|&v| v == 0.25));
    }

    use super::*;
    use assert2::check;

    fn port() -> AudioPort {
        AudioPort::new(4)
    }

    #[test]
    fn abi_discriminants_match() {
        check!(PortDataType::Audio as u32 == 0);
        check!(PortDataType::Midi as u32 == 1);
        check!(PortDataType::Any as u32 == 2);
        check!(PortDirection::Input as u32 == 0);
        check!(PortDirection::Output as u32 == 1);
        check!(PortDirection::Any as u32 == 2);
        check!(PortConnectability::INTERNAL.0 == 1);
        check!(PortConnectability::EXTERNAL.0 == 2);
    }

    #[test]
    fn connectability_combines() {
        let both = PortConnectability::INTERNAL.with(PortConnectability::EXTERNAL);
        check!(both.contains(PortConnectability::INTERNAL));
        check!(both.contains(PortConnectability::EXTERNAL));
        check!(!PortConnectability::NONE.contains(PortConnectability::INTERNAL));
    }

    #[test]
    fn defaults_pass_audio_through_unchanged() {
        let mut p = port();
        let mut buf = [0.25, -0.5, 0.75, -1.0];
        p.process(&mut buf);
        check!(buf == [0.25, -0.5, 0.75, -1.0]);
        check!(p.data_type() == PortDataType::Audio);
    }

    #[test]
    fn gain_scales_the_buffer() {
        let mut p = port();
        p.set_gain(2.0);
        let mut buf = [0.1, -0.2, 0.3, 0.0];
        p.process(&mut buf);
        check!(buf == [0.2, -0.4, 0.6, 0.0]);
    }

    #[test]
    fn muting_silences_the_buffer() {
        let mut p = port();
        p.set_muted(true);
        let mut buf = [0.5, -0.5, 1.0, -1.0];
        p.process(&mut buf);
        check!(buf == [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn input_peak_is_measured_before_gain() {
        let mut p = port();
        p.set_gain(10.0);
        let mut buf = [0.5, -0.25, 0.0, 0.0];
        p.process(&mut buf);
        // Peak of the incoming signal, not the amplified one.
        check!(p.input_peak() == 0.5);
    }

    #[test]
    fn input_peak_is_measured_even_when_muted() {
        let mut p = port();
        p.set_muted(true);
        let mut buf = [0.5, -0.9, 0.0, 0.0];
        p.process(&mut buf);
        // The signal still arrived, so metering reflects it.
        check!(p.input_peak() == 0.9);
        // But nothing was emitted.
        check!(p.output_peak() == 0.0);
    }

    #[test]
    fn output_peak_reflects_gain() {
        let mut p = port();
        p.set_gain(2.0);
        let mut buf = [0.25, 0.0, 0.0, 0.0];
        p.process(&mut buf);
        check!(p.input_peak() == 0.25);
        check!(p.output_peak() == 0.5);
    }

    #[test]
    fn peaks_accumulate_until_reset() {
        let mut p = port();
        p.process(&mut [0.8, 0.0, 0.0, 0.0]);
        check!(p.input_peak() == 0.8);
        // A quieter cycle does not lower the reading.
        p.process(&mut [0.1, 0.0, 0.0, 0.0]);
        check!(p.input_peak() == 0.8);

        p.reset_input_peak();
        p.reset_output_peak();
        check!(p.input_peak() == 0.0);
        check!(p.output_peak() == 0.0);
        p.process(&mut [0.1, 0.0, 0.0, 0.0]);
        check!(p.input_peak() == 0.1);
    }

    #[test]
    fn output_peak_holds_when_the_signal_goes_away() {
        let mut p = port();
        p.set_gain(2.0);
        p.process(&mut [0.5, 0.0, 0.0, 0.0]);
        check!(p.output_peak() == 1.0);

        // Muting stops output, but the meter holds its reading until reset.
        p.set_muted(true);
        p.process(&mut [0.5, 0.0, 0.0, 0.0]);
        check!(p.output_peak() == 1.0);

        // So does turning the gain down.
        p.set_muted(false);
        p.set_gain(0.1);
        p.process(&mut [0.5, 0.0, 0.0, 0.0]);
        check!(p.output_peak() == 1.0);

        p.reset_output_peak();
        p.process(&mut [0.5, 0.0, 0.0, 0.0]);
        check!(p.output_peak() < 1.0);
    }

    #[test]
    fn capture_records_the_processed_signal() {
        let mut p = port();
        p.set_ringbuffer_n_samples(8);
        p.set_gain(2.0);
        p.process(&mut [0.1, 0.2, 0.3, 0.4]);
        let snap = p.ringbuffer_contents();
        // Post-gain, matching what was actually heard.
        check!(snap.contiguous() == vec![0.2, 0.4, 0.6, 0.8]);
    }

    #[test]
    fn capture_records_silence_when_muted() {
        let mut p = port();
        p.set_ringbuffer_n_samples(8);
        p.set_muted(true);
        p.process(&mut [0.5, 0.5, 0.5, 0.5]);
        check!(p.ringbuffer_contents().contiguous() == vec![0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn capture_window_drops_the_oldest_audio() {
        let mut p = AudioPort::new(4);
        // Two buffers of four samples.
        p.set_ringbuffer_n_samples(8);
        p.process(&mut [1.0, 2.0, 3.0, 4.0]);
        p.process(&mut [5.0, 6.0, 7.0, 8.0]);
        p.process(&mut [9.0, 10.0, 11.0, 12.0]);
        let snap = p.ringbuffer_contents();
        check!(snap.n_samples == 8);
        check!(snap.contiguous() == vec![5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0]);
    }

    #[test]
    fn setting_the_capture_window_discards_what_was_held() {
        let mut p = port();
        p.set_ringbuffer_n_samples(8);
        p.process(&mut [1.0, 2.0, 3.0, 4.0]);
        check!(p.ringbuffer_n_samples() == 4);
        p.set_ringbuffer_n_samples(16);
        check!(p.ringbuffer_n_samples() == 0);
    }

    #[test]
    fn a_port_without_capture_still_processes() {
        let mut p = AudioPort::without_ringbuffer();
        let mut buf = [0.5, 0.5];
        p.set_gain(2.0);
        p.process(&mut buf);
        check!(buf == [1.0, 1.0]);
        check!(p.ringbuffer_n_samples() == 0);
        check!(p.ringbuffer_contents().n_samples == 0);
    }

    #[test]
    fn empty_buffer_is_harmless() {
        let mut p = port();
        p.process(&mut []);
        check!(p.input_peak() == 0.0);
        check!(p.ringbuffer_n_samples() == 0);
    }
}
