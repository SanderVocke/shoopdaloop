use crate::midi;
use crate::midi_storage::MidiStorageElem;

const CHANNELS: usize = midi::N_CHANNELS;
const CCS: usize = 120;
pub const MAX_PENDING_MIDI_CONTROLS: usize = CHANNELS * (CCS + 2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMidiControlState {
    cc: [[Option<u8>; CCS]; CHANNELS],
    channel_pressure: [Option<u8>; CHANNELS],
    pitch_wheel: [Option<u16>; CHANNELS],
}

impl Default for PendingMidiControlState {
    fn default() -> Self {
        Self {
            cc: [[None; CCS]; CHANNELS],
            channel_pressure: [None; CHANNELS],
            pitch_wheel: [None; CHANNELS],
        }
    }
}

impl PendingMidiControlState {
    pub fn supports(data: &[u8]) -> bool {
        ControlKey::from_message(data).is_some()
    }

    pub fn process(&mut self, data: &[u8]) -> bool {
        let Some(key) = ControlKey::from_message(data) else {
            return false;
        };
        match key {
            ControlKey::Cc {
                channel,
                controller,
            } => {
                self.cc[channel][controller] = Some(data[2]);
            }
            ControlKey::ChannelPressure { channel } => {
                self.channel_pressure[channel] = Some(data[1]);
            }
            ControlKey::PitchWheel { channel } => {
                self.pitch_wheel[channel] = Some((data[1] as u16) | ((data[2] as u16) << 7));
            }
        }
        true
    }

    pub fn clear_message(&mut self, data: &[u8]) -> bool {
        let Some(key) = ControlKey::from_message(data) else {
            return false;
        };
        match key {
            ControlKey::Cc {
                channel,
                controller,
            } => self.cc[channel][controller] = None,
            ControlKey::ChannelPressure { channel } => self.channel_pressure[channel] = None,
            ControlKey::PitchWheel { channel } => self.pitch_wheel[channel] = None,
        }
        true
    }

    pub fn append_messages(&self, out: &mut Vec<MidiStorageElem>, limit: usize) {
        let limit = limit.min(out.capacity().saturating_sub(out.len()));
        if limit == 0 {
            return;
        }
        let target_len = out.len().saturating_add(limit);
        for channel in 0..CHANNELS {
            for controller in 0..CCS {
                let Some(value) = self.cc[channel][controller] else {
                    continue;
                };
                if out.len() == target_len {
                    return;
                }
                if let Some(event) =
                    MidiStorageElem::new(0, &midi::cc(channel as u8, controller as u8, value))
                {
                    out.push(event);
                }
            }
            if let Some(value) = self.channel_pressure[channel] {
                if out.len() == target_len {
                    return;
                }
                if let Some(event) =
                    MidiStorageElem::new(0, &midi::channel_pressure(channel as u8, value))
                {
                    out.push(event);
                }
            }
            if let Some(value) = self.pitch_wheel[channel] {
                if out.len() == target_len {
                    return;
                }
                if let Some(event) =
                    MidiStorageElem::new(0, &midi::pitch_wheel(channel as u8, value))
                {
                    out.push(event);
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.cc.iter().flatten().all(Option::is_none)
            && self.channel_pressure.iter().all(Option::is_none)
            && self.pitch_wheel.iter().all(Option::is_none)
    }
}

#[derive(Clone, Copy)]
enum ControlKey {
    Cc { channel: usize, controller: usize },
    ChannelPressure { channel: usize },
    PitchWheel { channel: usize },
}

impl ControlKey {
    fn from_message(data: &[u8]) -> Option<Self> {
        if midi::is_cc(data) && data[1] < CCS as u8 {
            return Some(Self::Cc {
                channel: midi::channel(data) as usize,
                controller: data[1] as usize,
            });
        }
        if midi::is_channel_pressure(data) {
            return Some(Self::ChannelPressure {
                channel: midi::channel(data) as usize,
            });
        }
        if midi::is_pitch_wheel(data) {
            return Some(Self::PitchWheel {
                channel: midi::channel(data) as usize,
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(events: &[MidiStorageElem]) -> Vec<Vec<u8>> {
        events.iter().map(|event| event.data().to_vec()).collect()
    }

    #[test]
    fn supports_only_absolute_fx_controls() {
        let mut pending = PendingMidiControlState::default();
        assert!(pending.process(&midi::cc(15, 119, 0)));
        assert!(pending.process(&midi::channel_pressure(4, 55)));
        assert!(pending.process(&midi::pitch_wheel(3, 1_000)));
        assert!(!pending.process(&midi::cc(0, 120, 0)));
        assert!(!pending.process(&midi::program_change(0, 3)));
        assert!(!pending.process(&midi::note_on(0, 60, 100)));
        assert!(!pending.process(&[0xf8]));
        assert!(!pending.process(&[0xb0, 7]));
    }

    #[test]
    fn latest_value_replaces_pending_and_zero_is_explicit() {
        let mut pending = PendingMidiControlState::default();
        pending.process(&midi::cc(2, 7, 99));
        pending.process(&midi::cc(2, 7, 0));
        let mut out = Vec::with_capacity(MAX_PENDING_MIDI_CONTROLS);
        pending.append_messages(&mut out, usize::MAX);
        assert_eq!(bytes(&out), vec![midi::cc(2, 7, 0).to_vec()]);
        assert!(!pending.is_empty());
        pending.clear_message(&midi::cc(2, 7, 0));
        assert!(pending.is_empty());
    }

    #[test]
    fn partial_drain_is_deterministic_and_clears_only_admitted_values() {
        let mut pending = PendingMidiControlState::default();
        pending.process(&midi::pitch_wheel(1, 2_000));
        pending.process(&midi::cc(0, 9, 9));
        pending.process(&midi::channel_pressure(0, 10));
        let mut out = Vec::with_capacity(MAX_PENDING_MIDI_CONTROLS);
        pending.append_messages(&mut out, 2);
        assert_eq!(
            bytes(&out),
            vec![
                midi::cc(0, 9, 9).to_vec(),
                midi::channel_pressure(0, 10).to_vec()
            ]
        );
        for event in &out {
            pending.clear_message(event.data());
        }
        assert!(!pending.is_empty());
        out.clear();
        pending.append_messages(&mut out, 1);
        assert_eq!(bytes(&out), vec![midi::pitch_wheel(1, 2_000).to_vec()]);
        pending.clear_message(out[0].data());
        assert!(pending.is_empty());
    }

    #[test]
    fn current_message_can_remove_a_stale_pending_key() {
        let mut pending = PendingMidiControlState::default();
        pending.process(&midi::cc(0, 7, 1));
        assert!(pending.clear_message(&midi::cc(0, 7, 2)));
        assert!(pending.is_empty());
        assert!(!pending.clear_message(&midi::note_on(0, 60, 1)));
    }
}
