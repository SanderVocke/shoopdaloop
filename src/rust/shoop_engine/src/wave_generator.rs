//! A built-in polyphonic oscillator, so the engine can make a sound on its own.
//!
//! The C++ gets its instruments from LV2 or Carla plugins. That is still the eventual
//! plan, but hosting a plugin is a large piece of work and nothing can be played
//! without one, so this is a small synth with no external dependency. It is enough to
//! drive the engine from a keyboard and hear loops play back.
//!
//! Realtime-safe by construction: a fixed voice pool allocated up front, so a flurry of
//! notes steals a voice rather than allocating one. Stealing the oldest is the usual
//! choice and the least surprising -- a held chord keeps sounding while a new note
//! displaces whatever has been ringing longest.
//!
//! The envelope exists only to stop clicks. A note that starts or ends at full
//! amplitude steps the output, which is audible; a few milliseconds of ramp is not.

use crate::midi;
use crate::midi_storage::MidiStorageElem;

use std::f32::consts::TAU;

/// Voices allocated up front. Enough for ten fingers and a sustain pedal.
pub const N_VOICES: usize = 32;

/// Ramp applied at note start and end, in seconds.
const ENVELOPE_SECONDS: f32 = 0.005;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Waveform {
    #[default]
    Sine,
    Square,
    Saw,
    Triangle,
}

impl Waveform {
    /// One cycle, for `phase` in `[0, 1)`.
    fn sample(self, phase: f32) -> f32 {
        match self {
            Waveform::Sine => (phase * TAU).sin(),
            // Naive shapes: they alias, which for a test instrument is acceptable and
            // for a musical one would want band limiting.
            Waveform::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Waveform::Saw => 2.0 * phase - 1.0,
            Waveform::Triangle => 4.0 * (phase - 0.5).abs() - 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Voice {
    channel: u8,
    note: u8,
    phase: f32,
    velocity: f32,
    /// Current envelope level, ramping towards `target`.
    level: f32,
    target: f32,
    /// Cycle in which this voice was started, for stealing the oldest.
    age: u64,
    active: bool,
}

#[derive(Debug)]
pub struct WaveGenerator {
    voices: [Voice; N_VOICES],
    waveform: Waveform,
    sample_rate: f32,
    gain: f32,
    /// Increments per note, so "oldest" is well defined without a clock.
    next_age: u64,
    n_stolen: u32,
}

impl Default for WaveGenerator {
    fn default() -> Self {
        Self {
            voices: [Voice::default(); N_VOICES],
            waveform: Waveform::default(),
            sample_rate: 48000.0,
            gain: 0.2,
            next_age: 0,
            n_stolen: 0,
        }
    }
}

impl WaveGenerator {
    pub fn waveform(&self) -> Waveform {
        self.waveform
    }
    pub fn set_waveform(&mut self, w: Waveform) {
        self.waveform = w;
    }
    pub fn gain(&self) -> f32 {
        self.gain
    }
    /// Output level. Defaults low, because several voices sum.
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(0.0, 4.0);
    }
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
    pub fn set_sample_rate(&mut self, sr: u32) {
        if sr > 0 {
            self.sample_rate = sr as f32;
        }
    }
    /// Voices currently sounding, including those releasing.
    pub fn n_active_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }
    /// Notes that displaced a sounding voice because the pool was full.
    pub fn n_stolen(&self) -> u32 {
        self.n_stolen
    }

    /// Silences everything at once, without a release ramp.
    pub fn reset(&mut self) {
        for v in self.voices.iter_mut() {
            *v = Voice::default();
        }
    }

    fn envelope_step(&self) -> f32 {
        1.0 / (ENVELOPE_SECONDS * self.sample_rate).max(1.0)
    }

    fn note_on(&mut self, channel: u8, note: u8, velocity: u8) {
        // A note already sounding is retriggered rather than doubled, which is what a
        // keyboard sending a repeat expects.
        if let Some(v) = self
            .voices
            .iter_mut()
            .find(|v| v.active && v.channel == channel && v.note == note)
        {
            v.velocity = velocity as f32 / 127.0;
            v.target = 1.0;
            return;
        }

        let age = self.next_age;
        self.next_age += 1;

        let free = self.voices.iter().position(|v| !v.active);
        let idx = match free {
            Some(i) => i,
            None => {
                self.n_stolen += 1;
                // Oldest, so a long-held drone yields before a fresh chord.
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.age)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            }
        };
        self.voices[idx] = Voice {
            channel,
            note,
            phase: 0.0,
            velocity: velocity as f32 / 127.0,
            level: 0.0,
            target: 1.0,
            age,
            active: true,
        };
    }

    fn note_off(&mut self, channel: u8, note: u8) {
        for v in self.voices.iter_mut() {
            if v.active && v.channel == channel && v.note == note {
                // Released rather than cut, so it ramps down instead of clicking.
                v.target = 0.0;
            }
        }
    }

    fn all_off(&mut self, channel: u8) {
        for v in self.voices.iter_mut() {
            if v.active && v.channel == channel {
                v.target = 0.0;
            }
        }
    }

    /// Applies one message.
    pub fn handle(&mut self, data: &[u8]) {
        if data.len() < 2 {
            return;
        }
        let channel = data[0] & 0x0F;
        match data[0] & 0xF0 {
            0x90 => {
                let velocity = *data.get(2).unwrap_or(&0);
                // Velocity zero is a note-off, as the MIDI spec allows.
                if velocity == 0 {
                    self.note_off(channel, data[1]);
                } else {
                    self.note_on(channel, data[1], velocity);
                }
            }
            0x80 => self.note_off(channel, data[1]),
            // All Notes Off and All Sound Off both stop everything here; they differ over
            // tails, and this synth has none beyond its release ramp.
            0xB0 if midi::all_notes_off_channel(data).is_some()
                || midi::all_sound_off_channel(data).is_some() =>
            {
                self.all_off(channel)
            }
            _ => {}
        }
    }

    /// Renders `out.len()` frames, applying `input` at its message times.
    ///
    /// Messages are applied at the frame they carry, so a note lands where it was
    /// played rather than at the start of the buffer. `input` must be ordered by time,
    /// which every port in this engine guarantees.
    ///
    /// Adds to `out` rather than overwriting it, so several sources can share a port,
    /// which is how the rest of the engine treats an output buffer.
    pub fn process(&mut self, input: &[MidiStorageElem], out: &mut [f32]) {
        let step = self.envelope_step();
        let mut next = 0;

        for (f, slot) in out.iter_mut().enumerate() {
            // Everything timed at or before this frame, so a burst at one time all lands.
            while next < input.len() && (input[next].time as usize) <= f {
                let data = input[next].data();
                self.handle(data);
                next += 1;
            }
            *slot += self.render_frame(step);
        }

        // Anything timed past the buffer still gets applied, rather than being lost: a
        // port may hand over a message for a frame this cycle does not reach.
        while next < input.len() {
            self.handle(input[next].data());
            next += 1;
        }
    }

    fn render_frame(&mut self, step: f32) -> f32 {
        let (waveform, sample_rate, gain) = (self.waveform, self.sample_rate, self.gain);
        let mut sum = 0.0;
        for v in self.voices.iter_mut() {
            if !v.active {
                continue;
            }
            if v.level < v.target {
                v.level = (v.level + step).min(v.target);
            } else if v.level > v.target {
                v.level = (v.level - step).max(v.target);
                if v.level <= 0.0 {
                    // Fully released, so the voice is free again.
                    v.active = false;
                    continue;
                }
            }

            sum += waveform.sample(v.phase) * v.level * v.velocity;

            let hz = note_to_hz(v.note);
            v.phase += hz / sample_rate;
            if v.phase >= 1.0 {
                v.phase -= v.phase.floor();
            }
        }
        sum * gain
    }
}

/// Equal temperament, A4 = 440 Hz at note 69.
pub fn note_to_hz(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    fn ev(time: u32, data: &[u8]) -> MidiStorageElem {
        MidiStorageElem::new(time, data).expect("valid")
    }

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0f32, |a, b| a.max(b.abs()))
    }

    fn gen() -> WaveGenerator {
        let mut g = WaveGenerator::default();
        g.set_sample_rate(48000);
        g
    }

    #[test]
    fn silence_until_a_note_arrives() {
        let mut g = gen();
        let mut out = vec![0.0f32; 256];
        g.process(&[], &mut out);
        check!(peak(&out) == 0.0);
        check!(g.n_active_voices() == 0);
    }

    #[test]
    fn a_note_makes_a_sound() {
        let mut g = gen();
        let mut out = vec![0.0f32; 2048];
        g.process(&[ev(0, &midi::note_on(0, 69, 127))], &mut out);

        check!(g.n_active_voices() == 1);
        check!(peak(&out) > 0.0);
    }

    #[test]
    fn a_note_starts_at_the_frame_it_was_played() {
        let mut g = gen();
        let mut out = vec![0.0f32; 512];
        // Timed halfway through the buffer, so the first half must stay silent.
        g.process(&[ev(256, &midi::note_on(0, 69, 127))], &mut out);

        check!(peak(&out[..256]) == 0.0);
        check!(peak(&out[256..]) > 0.0);
    }

    #[test]
    fn releasing_a_note_frees_its_voice() {
        let mut g = gen();
        let mut out = vec![0.0f32; 512];
        g.process(&[ev(0, &midi::note_on(0, 60, 100))], &mut out);
        check!(g.n_active_voices() == 1);

        g.process(&[ev(0, &midi::note_off(0, 60, 64))], &mut out);
        // The release ramp is a few milliseconds, so give it long enough to finish.
        let mut tail = vec![0.0f32; 4096];
        g.process(&[], &mut tail);
        check!(g.n_active_voices() == 0);
    }

    #[test]
    fn a_note_on_with_zero_velocity_is_a_note_off() {
        let mut g = gen();
        let mut out = vec![0.0f32; 512];
        g.process(&[ev(0, &midi::note_on(0, 60, 100))], &mut out);
        check!(g.n_active_voices() == 1);

        g.process(&[ev(0, &midi::note_on(0, 60, 0))], &mut out);
        let mut tail = vec![0.0f32; 4096];
        g.process(&[], &mut tail);
        check!(g.n_active_voices() == 0);
    }

    #[test]
    fn a_retriggered_note_does_not_take_a_second_voice() {
        let mut g = gen();
        let mut out = vec![0.0f32; 256];
        g.process(&[ev(0, &midi::note_on(0, 60, 100))], &mut out);
        g.process(&[ev(0, &midi::note_on(0, 60, 120))], &mut out);
        check!(g.n_active_voices() == 1);
    }

    #[test]
    fn the_same_note_on_two_channels_is_two_voices() {
        let mut g = gen();
        let mut out = vec![0.0f32; 256];
        g.process(
            &[
                ev(0, &midi::note_on(0, 60, 100)),
                ev(0, &midi::note_on(1, 60, 100)),
            ],
            &mut out,
        );
        check!(g.n_active_voices() == 2);
    }

    #[test]
    fn all_notes_off_silences_the_channel_only() {
        let mut g = gen();
        let mut out = vec![0.0f32; 256];
        g.process(
            &[
                ev(0, &midi::note_on(0, 60, 100)),
                ev(0, &midi::note_on(1, 64, 100)),
            ],
            &mut out,
        );
        check!(g.n_active_voices() == 2);

        g.process(&[ev(0, &midi::all_notes_off(0))], &mut out);
        let mut tail = vec![0.0f32; 4096];
        g.process(&[], &mut tail);
        // Channel 1's note is untouched.
        check!(g.n_active_voices() == 1);
    }

    #[test]
    fn a_full_pool_steals_the_oldest_voice() {
        let mut g = gen();
        let mut out = vec![0.0f32; 64];
        for i in 0..N_VOICES {
            g.process(&[ev(0, &midi::note_on(0, 40 + i as u8, 100))], &mut out);
        }
        check!(g.n_active_voices() == N_VOICES);
        check!(g.n_stolen() == 0);

        g.process(&[ev(0, &midi::note_on(0, 100, 100))], &mut out);
        check!(g.n_stolen() == 1);
        // Still full rather than over, and the new note is sounding.
        check!(g.n_active_voices() == N_VOICES);
    }

    #[test]
    fn a_note_ramps_rather_than_stepping() {
        let mut g = gen();
        let mut out = vec![0.0f32; 480];
        g.process(&[ev(0, &midi::note_on(0, 69, 127))], &mut out);

        // The envelope is 5 ms, so the first sample must be far below the peak. A step
        // straight to full amplitude is what clicks.
        check!(out[0].abs() < peak(&out) * 0.1);
    }

    #[test]
    fn output_is_added_so_sources_can_share_a_buffer() {
        let mut g = gen();
        let mut out = vec![0.5f32; 128];
        g.process(&[], &mut out);
        // Nothing sounding, so what was already there survives.
        check!(out.iter().all(|&v| v == 0.5));
    }

    #[test]
    fn every_waveform_makes_a_sound() {
        for w in [
            Waveform::Sine,
            Waveform::Square,
            Waveform::Saw,
            Waveform::Triangle,
        ] {
            let mut g = gen();
            g.set_waveform(w);
            let mut out = vec![0.0f32; 2048];
            g.process(&[ev(0, &midi::note_on(0, 69, 127))], &mut out);
            check!(peak(&out) > 0.0, "{w:?} was silent");
        }
    }

    #[test]
    fn concert_a_is_440_hz() {
        check!((note_to_hz(69) - 440.0).abs() < 0.001);
        check!((note_to_hz(81) - 880.0).abs() < 0.001);
        check!((note_to_hz(57) - 220.0).abs() < 0.001);
    }

    /// The frequency actually rendered, counted from zero crossings, so the oscillator is
    /// checked rather than only the note table.
    #[test]
    fn a_rendered_note_has_the_right_frequency() {
        let mut g = gen();
        g.set_waveform(Waveform::Sine);
        let mut out = vec![0.0f32; 48000];
        g.process(&[ev(0, &midi::note_on(0, 69, 127))], &mut out);

        // Skipping the attack ramp.
        let steady = &out[1000..];
        let crossings = steady
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        let seconds = steady.len() as f32 / 48000.0;
        let hz = crossings as f32 / seconds;
        check!((hz - 440.0).abs() < 2.0, "measured {hz} Hz");
    }

    #[test]
    fn reset_silences_everything_at_once() {
        let mut g = gen();
        let mut out = vec![0.0f32; 256];
        g.process(&[ev(0, &midi::note_on(0, 60, 100))], &mut out);
        g.reset();
        check!(g.n_active_voices() == 0);

        let mut after = vec![0.0f32; 256];
        g.process(&[], &mut after);
        check!(peak(&after) == 0.0);
    }

    /// A note switched on and off at the same instant is silent, which is what the metronome was
    /// doing: both messages carried timestamp 0, so the voice was released before its attack had
    /// produced anything.
    #[test]
    fn a_note_released_at_the_same_timestamp_makes_no_sound() {
        let mut g = WaveGenerator::default();
        g.set_sample_rate(48000);
        g.set_gain(1.0);

        let on = MidiStorageElem::new(0, &crate::midi::note_on(0, 69, 127)).unwrap();
        let off = MidiStorageElem::new(0, &crate::midi::note_off(0, 69, 64)).unwrap();

        let mut out = vec![0.0f32; 512];
        g.process(&[on, off], &mut out);

        let peak = out.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(peak < 1e-3, "expected silence, got peak {peak}");
    }

    /// Held for a cycle and released on the next, it sounds -- which is the fix.
    #[test]
    fn a_note_held_for_a_cycle_sounds() {
        let mut g = WaveGenerator::default();
        g.set_sample_rate(48000);
        g.set_gain(1.0);

        let on = MidiStorageElem::new(0, &crate::midi::note_on(0, 69, 127)).unwrap();
        let mut out = vec![0.0f32; 512];
        g.process(&[on], &mut out);

        let peak = out.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(peak > 0.01, "the note never sounded: peak {peak}");
    }
}
