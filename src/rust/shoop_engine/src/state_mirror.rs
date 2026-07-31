use crate::channel_mode::ChannelMode;
use crate::loop_mode::LoopMode;
use crate::midi_event::MidiEvent;
use crate::state::{AudioChannelState, LoopState, MidiChannelState};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

const NO_MODE: i32 = -1;
const NO_DELAY: u64 = u64::MAX;
const NO_SAMPLE: i32 = i32::MIN;

#[derive(Debug)]
pub struct LoopStateMirror {
    mode: AtomicI32,
    length: AtomicU32,
    position: AtomicU32,
    next_mode: AtomicI32,
    next_delay: AtomicU64,
}

impl Default for LoopStateMirror {
    fn default() -> Self {
        Self {
            mode: AtomicI32::new(LoopMode::Stopped as i32),
            length: AtomicU32::new(0),
            position: AtomicU32::new(0),
            next_mode: AtomicI32::new(NO_MODE),
            next_delay: AtomicU64::new(NO_DELAY),
        }
    }
}

impl LoopStateMirror {
    pub fn publish(
        &self,
        mode: LoopMode,
        length: u32,
        position: u32,
        next: Option<(LoopMode, u32)>,
    ) {
        self.mode.store(mode as i32, Ordering::Relaxed);
        self.length.store(length, Ordering::Relaxed);
        self.position.store(position, Ordering::Relaxed);
        self.next_mode.store(
            next.map(|(mode, _)| mode as i32).unwrap_or(NO_MODE),
            Ordering::Relaxed,
        );
        self.next_delay.store(
            next.map(|(_, delay)| delay as u64).unwrap_or(NO_DELAY),
            Ordering::Relaxed,
        );
    }

    pub fn read(&self) -> LoopState {
        let next_mode = self.next_mode.load(Ordering::Relaxed);
        let next_delay = self.next_delay.load(Ordering::Relaxed);
        LoopState {
            mode: LoopMode::try_from(self.mode.load(Ordering::Relaxed))
                .unwrap_or(LoopMode::Unknown),
            length: self.length.load(Ordering::Relaxed),
            position: self.position.load(Ordering::Relaxed),
            maybe_next_mode: (next_mode != NO_MODE)
                .then(|| LoopMode::try_from(next_mode).unwrap_or(LoopMode::Unknown)),
            maybe_next_mode_delay: (next_delay != NO_DELAY).then_some(next_delay as u32),
        }
    }
}

#[derive(Debug)]
pub struct AudioChannelStateMirror {
    complex_data_enabled: AtomicBool,
    mode: AtomicI32,
    gain: AtomicU32,
    output_peak: AtomicU32,
    length: AtomicU32,
    start_offset: AtomicI32,
    played_back_sample: AtomicI32,
    n_preplay_samples: AtomicU32,
    data_sequence: AtomicU64,
    data: Mutex<Vec<f32>>,
}

impl Default for AudioChannelStateMirror {
    fn default() -> Self {
        Self {
            complex_data_enabled: AtomicBool::new(false),
            mode: AtomicI32::new(ChannelMode::Disabled as i32),
            gain: AtomicU32::new(0.0f32.to_bits()),
            output_peak: AtomicU32::new(0.0f32.to_bits()),
            length: AtomicU32::new(0),
            start_offset: AtomicI32::new(0),
            played_back_sample: AtomicI32::new(NO_SAMPLE),
            n_preplay_samples: AtomicU32::new(0),
            data_sequence: AtomicU64::new(0),
            data: Mutex::new(Vec::new()),
        }
    }
}

impl AudioChannelStateMirror {
    pub fn enable_complex_data(&self) {
        self.complex_data_enabled.store(true, Ordering::Relaxed);
    }

    pub fn complex_data_enabled(&self) -> bool {
        self.complex_data_enabled.load(Ordering::Relaxed)
    }

    pub fn publish(
        &self,
        mode: ChannelMode,
        gain: f32,
        length: usize,
        start_offset: i32,
        played_back_sample: Option<i32>,
        n_preplay_samples: u32,
        data_sequence: u64,
    ) {
        self.mode.store(mode as i32, Ordering::Relaxed);
        self.gain.store(gain.to_bits(), Ordering::Relaxed);
        self.length.store(length as u32, Ordering::Relaxed);
        self.start_offset.store(start_offset, Ordering::Relaxed);
        self.played_back_sample
            .store(played_back_sample.unwrap_or(NO_SAMPLE), Ordering::Relaxed);
        self.n_preplay_samples
            .store(n_preplay_samples, Ordering::Relaxed);
        self.data_sequence.store(data_sequence, Ordering::Relaxed);
    }

    pub fn publish_output_peak(&self, peak: f32) {
        atomic_max_f32(&self.output_peak, peak);
    }

    pub fn read(&self, acknowledged_data_sequence: u64) -> AudioChannelState {
        let played = self.played_back_sample.load(Ordering::Relaxed);
        AudioChannelState {
            mode: ChannelMode::try_from(self.mode.load(Ordering::Relaxed))
                .unwrap_or(ChannelMode::Disabled),
            gain: f32::from_bits(self.gain.load(Ordering::Relaxed)),
            output_peak: f32::from_bits(self.output_peak.swap(0.0f32.to_bits(), Ordering::Relaxed)),
            length: self.length.load(Ordering::Relaxed),
            start_offset: self.start_offset.load(Ordering::Relaxed),
            played_back_sample: (played != NO_SAMPLE).then_some(played),
            n_preplay_samples: self.n_preplay_samples.load(Ordering::Relaxed),
            data_dirty: self.data_sequence.load(Ordering::Relaxed) != acknowledged_data_sequence,
        }
    }

    pub fn data_sequence(&self) -> u64 {
        self.data_sequence.load(Ordering::Relaxed)
    }

    pub fn replace_data(&self, data: Vec<f32>) {
        if self.complex_data_enabled() {
            *self.data.lock().unwrap_or_else(|e| e.into_inner()) = data;
        }
    }

    pub fn write_data(&self, offset: usize, source: &[f32], length: usize) {
        if !self.complex_data_enabled() {
            return;
        }
        let mut data = self.data.lock().unwrap_or_else(|e| e.into_inner());
        if data.len() < length {
            data.resize(length, 0.0);
        } else {
            data.truncate(length);
        }
        let end = offset.saturating_add(source.len()).min(data.len());
        if offset < end {
            data[offset..end].copy_from_slice(&source[..end - offset]);
        }
    }

    pub fn data(&self) -> Vec<f32> {
        self.data.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

#[derive(Debug)]
pub struct MidiChannelStateMirror {
    complex_data_enabled: AtomicBool,
    mode: AtomicI32,
    n_events_triggered: AtomicU32,
    n_notes_active: AtomicU32,
    length: AtomicU32,
    start_offset: AtomicI32,
    played_back_sample: AtomicI32,
    n_preplay_samples: AtomicU32,
    data_sequence: AtomicU64,
    data: Mutex<Vec<MidiEvent>>,
}

impl Default for MidiChannelStateMirror {
    fn default() -> Self {
        Self {
            complex_data_enabled: AtomicBool::new(false),
            mode: AtomicI32::new(ChannelMode::Disabled as i32),
            n_events_triggered: AtomicU32::new(0),
            n_notes_active: AtomicU32::new(0),
            length: AtomicU32::new(0),
            start_offset: AtomicI32::new(0),
            played_back_sample: AtomicI32::new(NO_SAMPLE),
            n_preplay_samples: AtomicU32::new(0),
            data_sequence: AtomicU64::new(0),
            data: Mutex::new(Vec::new()),
        }
    }
}

impl MidiChannelStateMirror {
    pub fn enable_complex_data(&self) {
        self.complex_data_enabled.store(true, Ordering::Relaxed);
    }

    pub fn complex_data_enabled(&self) -> bool {
        self.complex_data_enabled.load(Ordering::Relaxed)
    }

    pub fn publish(
        &self,
        mode: ChannelMode,
        n_notes_active: u32,
        length: u32,
        start_offset: i32,
        played_back_sample: Option<i32>,
        n_preplay_samples: u32,
        data_sequence: u64,
    ) {
        self.mode.store(mode as i32, Ordering::Relaxed);
        self.n_notes_active.store(n_notes_active, Ordering::Relaxed);
        self.length.store(length, Ordering::Relaxed);
        self.start_offset.store(start_offset, Ordering::Relaxed);
        self.played_back_sample
            .store(played_back_sample.unwrap_or(NO_SAMPLE), Ordering::Relaxed);
        self.n_preplay_samples
            .store(n_preplay_samples, Ordering::Relaxed);
        self.data_sequence.store(data_sequence, Ordering::Relaxed);
    }

    pub fn record_triggered_event(&self) {
        self.n_events_triggered.fetch_add(1, Ordering::Relaxed);
    }

    pub fn read(&self, acknowledged_data_sequence: u64) -> MidiChannelState {
        let played = self.played_back_sample.load(Ordering::Relaxed);
        MidiChannelState {
            mode: ChannelMode::try_from(self.mode.load(Ordering::Relaxed))
                .unwrap_or(ChannelMode::Disabled),
            n_events_triggered: self.n_events_triggered.swap(0, Ordering::Relaxed),
            n_notes_active: self.n_notes_active.load(Ordering::Relaxed),
            length: self.length.load(Ordering::Relaxed),
            start_offset: self.start_offset.load(Ordering::Relaxed),
            played_back_sample: (played != NO_SAMPLE).then_some(played),
            n_preplay_samples: self.n_preplay_samples.load(Ordering::Relaxed),
            data_dirty: self.data_sequence.load(Ordering::Relaxed) != acknowledged_data_sequence,
        }
    }

    pub fn data_sequence(&self) -> u64 {
        self.data_sequence.load(Ordering::Relaxed)
    }

    pub fn replace_data(&self, data: Vec<MidiEvent>) {
        if self.complex_data_enabled() {
            *self.data.lock().unwrap_or_else(|e| e.into_inner()) = data;
        }
    }

    pub fn data(&self) -> Vec<MidiEvent> {
        self.data.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

fn atomic_max_f32(target: &AtomicU32, value: f32) {
    if !value.is_finite() || value <= 0.0 {
        return;
    }
    let mut current = target.load(Ordering::Relaxed);
    while value > f32::from_bits(current) {
        match target.compare_exchange_weak(
            current,
            value.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    #[test]
    fn channel_accumulators_are_consumed_without_commands() {
        let audio = AudioChannelStateMirror::default();
        audio.publish_output_peak(0.25);
        audio.publish_output_peak(0.75);
        check!(audio.read(0).output_peak == 0.75);
        check!(audio.read(0).output_peak == 0.0);

        let midi = MidiChannelStateMirror::default();
        midi.record_triggered_event();
        midi.record_triggered_event();
        check!(midi.read(0).n_events_triggered == 2);
        check!(midi.read(0).n_events_triggered == 0);
    }

    #[test]
    fn channel_data_sequences_support_local_acknowledgement() {
        let audio = AudioChannelStateMirror::default();
        audio.publish(ChannelMode::Direct, 1.0, 4, 0, None, 0, 3);
        check!(audio.read(0).data_dirty);
        check!(!audio.read(3).data_dirty);

        let midi = MidiChannelStateMirror::default();
        midi.publish(ChannelMode::Direct, 0, 4, 0, None, 0, 7);
        check!(midi.read(0).data_dirty);
        check!(!midi.read(7).data_dirty);
    }

    #[test]
    fn loop_state_fields_are_independently_published() {
        let mirror = LoopStateMirror::default();
        check!(mirror.read().mode == LoopMode::Stopped);

        mirror.publish(LoopMode::Playing, 128, 17, Some((LoopMode::Recording, 2)));
        let state = mirror.read();
        check!(state.mode == LoopMode::Playing);
        check!(state.length == 128);
        check!(state.position == 17);
        check!(state.maybe_next_mode == Some(LoopMode::Recording));
        check!(state.maybe_next_mode_delay == Some(2));

        mirror.publish(LoopMode::Stopped, 0, 0, None);
        let state = mirror.read();
        check!(state.maybe_next_mode.is_none());
        check!(state.maybe_next_mode_delay.is_none());
    }
}
