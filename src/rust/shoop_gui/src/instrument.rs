//! The built-in instrument, as the audio callback sees it.
//!
//! The synth lives on the audio thread, so the UI reaches it the same way it reaches the
//! engine: notes over a lock-free queue, settings through atomics. Nothing here locks or
//! allocates once running.
//!
//! Its output is staged into a session input port, which makes it indistinguishable from
//! a device: a loop channel recording that port records the instrument, and the same
//! wiring would serve a real input later.

use shoop_engine::midi_storage::MidiStorageElem;
use shoop_engine::session::{Port, Session};
use shoop_engine::wave_generator::{WaveGenerator, Waveform};

use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Notes in flight between the UI and the callback. Generous: a key press is two bytes
/// of traffic and dropping one is a stuck note.
const NOTE_QUEUE: usize = 1024;

/// Frames the render buffer is sized for. Devices ask for far less; sizing for the worst
/// case means the callback never grows it.
const MAX_FRAMES: usize = 8192;

/// Settings the UI can change while the synth is running.
///
/// Atomics rather than queued messages: these are levels and choices, where the newest
/// value is the only one that matters and a missed intermediate is invisible.
#[derive(Debug)]
pub struct Settings {
    waveform: AtomicU32,
    /// Gain as bits, since there is no portable atomic `f32`.
    gain_bits: AtomicU32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            waveform: AtomicU32::new(0),
            gain_bits: AtomicU32::new(0.2f32.to_bits()),
        }
    }
}

impl Settings {
    pub fn waveform(&self) -> Waveform {
        match self.waveform.load(Ordering::Relaxed) {
            1 => Waveform::Square,
            2 => Waveform::Saw,
            3 => Waveform::Triangle,
            _ => Waveform::Sine,
        }
    }
    pub fn set_waveform(&self, w: Waveform) {
        let v = match w {
            Waveform::Sine => 0,
            Waveform::Square => 1,
            Waveform::Saw => 2,
            Waveform::Triangle => 3,
        };
        self.waveform.store(v, Ordering::Relaxed);
    }
    pub fn gain(&self) -> f32 {
        f32::from_bits(self.gain_bits.load(Ordering::Relaxed))
    }
    pub fn set_gain(&self, gain: f32) {
        self.gain_bits
            .store(gain.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
}

/// The UI's end: sends notes and reports what could not be sent.
pub struct Keys {
    tx: Producer<MidiStorageElem>,
    n_dropped: u32,
}

impl Keys {
    /// Queues a message for the synth. Refuses silently if the queue is full, counting it.
    pub fn send(&mut self, data: &[u8]) {
        match MidiStorageElem::new(0, data) {
            Some(e) => {
                if self.tx.push(e).is_err() {
                    self.n_dropped += 1;
                }
            }
            None => self.n_dropped += 1,
        }
    }

    pub fn n_dropped(&self) -> u32 {
        self.n_dropped
    }
}

/// The callback's end: renders the instrument into a session port each cycle.
pub struct Voice {
    synth: WaveGenerator,
    rx: Consumer<MidiStorageElem>,
    settings: Arc<Settings>,
    /// This cycle's notes, reused so draining does not allocate.
    pending: Vec<MidiStorageElem>,
    /// This cycle's audio, reused for the same reason.
    render: Vec<f32>,
    /// Session port the instrument feeds.
    port: usize,
    /// Published so the UI can show how many notes are sounding.
    n_voices: Arc<AtomicU32>,
}

impl Voice {
    /// Renders one cycle and stages it into the instrument's port.
    ///
    /// Runs on the audio thread. Every buffer it touches is already sized.
    pub fn render_into(&mut self, session: &mut Session, n_frames: usize) {
        self.synth.set_waveform(self.settings.waveform());
        self.synth.set_gain(self.settings.gain());

        self.pending.clear();
        while let Ok(e) = self.rx.pop() {
            if self.pending.len() == self.pending.capacity() {
                // Queue is longer than one cycle's worth; the rest arrives next cycle
                // rather than growing this buffer.
                break;
            }
            self.pending.push(e);
        }

        let n = n_frames.min(self.render.len());
        let buf = &mut self.render[..n];
        buf.fill(0.0);
        self.synth.process(&self.pending, buf);

        self.n_voices
            .store(self.synth.n_active_voices() as u32, Ordering::Relaxed);

        if let Some(p) = session.port_mut(self.port).and_then(Port::as_external_mut) {
            p.stage_input(buf);
        }
    }

    pub fn set_sample_rate(&mut self, sr: u32) {
        self.synth.set_sample_rate(sr);
    }
}

/// Builds the two halves, plus the shared settings and voice count.
pub fn split(port: usize, sample_rate: u32) -> (Keys, Voice, Arc<Settings>, Arc<AtomicU32>) {
    let (tx, rx) = RingBuffer::new(NOTE_QUEUE);
    let settings = Arc::new(Settings::default());
    let n_voices = Arc::new(AtomicU32::new(0));

    let mut synth = WaveGenerator::default();
    synth.set_sample_rate(sample_rate);

    (
        Keys { tx, n_dropped: 0 },
        Voice {
            synth,
            rx,
            settings: Arc::clone(&settings),
            pending: Vec::with_capacity(256),
            render: vec![0.0; MAX_FRAMES],
            port,
            n_voices: Arc::clone(&n_voices),
        },
        settings,
        n_voices,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_engine::external_audio_port::ExternalAudioPort;
    use shoop_engine::midi;
    use shoop_engine::port::PortDirection;

    fn session_with_port() -> (Session, usize) {
        let mut s = Session::default();
        let p = s.add_port(Port::External(ExternalAudioPort::new(
            "instrument",
            PortDirection::Input,
            0,
        )));
        s.apply_graph_changes().expect("schedule");
        (s, p)
    }

    #[test]
    fn settings_round_trip() {
        let s = Settings::default();
        assert_eq!(s.waveform(), Waveform::Sine);

        s.set_waveform(Waveform::Saw);
        assert_eq!(s.waveform(), Waveform::Saw);

        s.set_gain(0.75);
        assert!((s.gain() - 0.75).abs() < 1e-6);

        // Out of range is clamped, not wrapped.
        s.set_gain(5.0);
        assert!((s.gain() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_note_reaches_the_port() {
        let (mut session, port) = session_with_port();
        let (mut keys, mut voice, _settings, n_voices) = split(port, 48000);

        keys.send(&midi::note_on(0, 69, 127));
        voice.render_into(&mut session, 512);

        assert_eq!(n_voices.load(Ordering::Relaxed), 1);

        // Staged, so the port hands it over when the cycle prepares.
        session.process(512).expect("cycle");
        let staged = session
            .port(port)
            .and_then(Port::as_external)
            .map(|p| p.output(512).iter().fold(0.0f32, |a, b| a.max(b.abs())))
            .expect("port");
        assert!(staged > 0.0, "the instrument was silent at the port");
    }

    #[test]
    fn silence_when_nothing_is_played() {
        let (mut session, port) = session_with_port();
        let (_keys, mut voice, _settings, n_voices) = split(port, 48000);

        voice.render_into(&mut session, 256);
        assert_eq!(n_voices.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_full_queue_is_counted_rather_than_growing() {
        let (_session, port) = session_with_port();
        let (mut keys, _voice, _s, _n) = split(port, 48000);

        // Far more than the queue holds.
        for _ in 0..(NOTE_QUEUE + 50) {
            keys.send(&midi::note_on(0, 60, 100));
        }
        assert!(keys.n_dropped() >= 50);
    }

    #[test]
    fn an_oversized_message_is_refused() {
        let (_session, port) = session_with_port();
        let (mut keys, _voice, _s, _n) = split(port, 48000);
        keys.send(&[0xF0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(keys.n_dropped(), 1);
    }

    #[test]
    fn a_cycle_larger_than_the_render_buffer_is_clamped() {
        let (mut session, port) = session_with_port();
        let (mut keys, mut voice, _s, _n) = split(port, 48000);
        keys.send(&midi::note_on(0, 60, 100));
        // Asking for more than MAX_FRAMES must not panic or reallocate.
        voice.render_into(&mut session, MAX_FRAMES * 2);
    }
}
