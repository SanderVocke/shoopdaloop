//! Audio port used for routing entirely inside the engine.
//!
//! Owns its own buffer rather than borrowing one from a driver, so it can carry
//! signal between graph nodes — for example from a loop's output into a plugin's
//! input. It has no external connections at all.
//!
//! Directions are stated from the engine's point of view, so a hosted effect's
//! *inputs* are engine *outputs*.

use crate::port::{AudioPort, PortConnectability, PortDataType};

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("internal ports cannot be connected externally")]
pub struct NotExternallyConnectable;

#[derive(Debug)]
pub struct InternalAudioPort {
    name: String,
    buffer: Vec<f32>,
    input_connectability: PortConnectability,
    output_connectability: PortConnectability,
    audio: AudioPort,
}

impl InternalAudioPort {
    pub fn new(
        name: impl Into<String>,
        n_frames: usize,
        input_connectability: PortConnectability,
        output_connectability: PortConnectability,
        ringbuffer_buffer_size: usize,
    ) -> Self {
        Self {
            name: name.into(),
            buffer: vec![0.0; n_frames],
            input_connectability,
            output_connectability,
            audio: AudioPort::new(ringbuffer_buffer_size),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn data_type(&self) -> PortDataType {
        PortDataType::Audio
    }
    pub fn input_connectability(&self) -> PortConnectability {
        self.input_connectability
    }
    pub fn output_connectability(&self) -> PortConnectability {
        self.output_connectability
    }

    /// Internal ports are readable and writable from inside the engine, and get
    /// their data from nowhere else.
    pub fn has_internal_read_access(&self) -> bool {
        true
    }
    pub fn has_internal_write_access(&self) -> bool {
        true
    }
    pub fn has_implicit_input_source(&self) -> bool {
        false
    }
    pub fn has_implicit_output_sink(&self) -> bool {
        false
    }

    /// Always empty: there is nothing external to connect to.
    pub fn external_connection_status(&self) -> Vec<(String, bool)> {
        Vec::new()
    }
    pub fn connect_external(&mut self, _name: &str) -> Result<(), NotExternallyConnectable> {
        Err(NotExternallyConnectable)
    }
    pub fn disconnect_external(&mut self, _name: &str) -> Result<(), NotExternallyConnectable> {
        Err(NotExternallyConnectable)
    }

    pub fn audio(&self) -> &AudioPort {
        &self.audio
    }
    pub fn audio_mut(&mut self) -> &mut AudioPort {
        &mut self.audio
    }

    /// The port's buffer, grown if this cycle needs more room.
    pub fn buffer(&mut self, n_frames: usize) -> &mut [f32] {
        if n_frames > self.buffer.len() || self.buffer.is_empty() {
            crate::realtime_allow_alloc_once!("InternalAudioPort::buffer resize", || {
                self.buffer.resize(n_frames.max(1), 0.0)
            });
        }
        &mut self.buffer[..n_frames]
    }

    /// Start of cycle: clear the buffer so writers accumulate into silence.
    pub fn prepare(&mut self, n_frames: usize) {
        for s in self.buffer(n_frames) {
            *s = 0.0;
        }
    }

    /// End of cycle: apply gain/muting, meter and capture.
    pub fn process(&mut self, n_frames: usize) {
        if n_frames > self.buffer.len() || self.buffer.is_empty() {
            crate::realtime_allow_alloc_once!("InternalAudioPort::process buffer resize", || {
                self.buffer.resize(n_frames.max(1), 0.0)
            });
        }
        let (buf, audio) = (&mut self.buffer[..n_frames], &mut self.audio);
        audio.process(buf);
    }

    pub fn close(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::{check, let_assert};

    fn port(n: usize) -> InternalAudioPort {
        InternalAudioPort::new(
            "p1",
            n,
            PortConnectability::INTERNAL,
            PortConnectability::INTERNAL,
            4,
        )
    }

    #[test]
    fn reports_its_identity() {
        let p = port(4);
        check!(p.name() == "p1");
        check!(p.data_type() == PortDataType::Audio);
        check!(p.input_connectability() == PortConnectability::INTERNAL);
        check!(p.output_connectability() == PortConnectability::INTERNAL);
    }

    #[test]
    fn is_internally_accessible_only() {
        let p = port(4);
        check!(p.has_internal_read_access());
        check!(p.has_internal_write_access());
        // Nothing feeds or drains it implicitly; the graph does that explicitly.
        check!(!p.has_implicit_input_source());
        check!(!p.has_implicit_output_sink());
    }

    #[test]
    fn cannot_be_connected_externally() {
        let mut p = port(4);
        check!(p.external_connection_status().is_empty());
        check!(p.connect_external("system:capture_1") == Err(NotExternallyConnectable));
        check!(p.disconnect_external("system:capture_1") == Err(NotExternallyConnectable));
    }

    #[test]
    fn prepare_clears_the_buffer() {
        let mut p = port(4);
        p.buffer(4).copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        p.prepare(4);
        check!(p.buffer(4) == [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn buffer_grows_for_a_larger_cycle() {
        let mut p = port(2);
        check!(p.buffer(2).len() == 2);
        check!(p.buffer(8).len() == 8);
        // And keeps the larger allocation for smaller cycles.
        check!(p.buffer(2).len() == 2);
    }

    #[test]
    fn a_zero_frame_port_still_yields_a_buffer() {
        let mut p = port(0);
        check!(p.buffer(0).is_empty());
        // Asking for frames grows it.
        check!(p.buffer(3).len() == 3);
    }

    #[test]
    fn process_applies_gain_to_what_was_written() {
        let mut p = port(4);
        p.prepare(4);
        p.buffer(4).copy_from_slice(&[0.1, 0.2, 0.3, 0.4]);
        p.audio_mut().set_gain(2.0);
        p.process(4);
        check!(p.buffer(4) == [0.2, 0.4, 0.6, 0.8]);
    }

    #[test]
    fn process_meters_and_captures() {
        let mut p = port(4);
        p.audio_mut().set_ringbuffer_n_samples(8);
        p.prepare(4);
        p.buffer(4).copy_from_slice(&[0.5, -0.25, 0.0, 0.0]);
        p.process(4);
        check!(p.audio().input_peak() == 0.5);
        let snap = p.audio().ringbuffer_contents();
        check!(snap.contiguous() == vec![0.5, -0.25, 0.0, 0.0]);
    }

    #[test]
    fn muting_silences_the_routed_signal() {
        let mut p = port(4);
        p.audio_mut().set_muted(true);
        p.prepare(4);
        p.buffer(4).copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
        p.process(4);
        check!(p.buffer(4) == [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn a_full_cycle_accumulates_from_silence() {
        let mut p = port(4);
        // Two writers adding into the same port, as the graph does.
        p.prepare(4);
        for (i, s) in p.buffer(4).iter_mut().enumerate() {
            *s += i as f32;
        }
        for s in p.buffer(4).iter_mut() {
            *s += 10.0;
        }
        p.process(4);
        check!(p.buffer(4) == [10.0, 11.0, 12.0, 13.0]);

        // Next cycle starts from silence again.
        p.prepare(4);
        check!(p.buffer(4) == [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn process_grows_the_buffer_if_needed() {
        let mut p = port(2);
        // Process a longer cycle than the port was built for.
        let_assert!(() = p.process(8));
        check!(p.buffer(8).len() == 8);
    }

    #[test]
    fn close_is_harmless() {
        let mut p = port(4);
        p.close();
        check!(p.name() == "p1");
    }
}
