//! Fixed MIDI 1.0 channel/key projection for initial states and checkpoints.
//! This records protocol state, not synthesizer voices or vendor SysEx state.
use crate::{
    midi::{Message, MessageKind},
    Error,
};

#[allow(dead_code)] // Ordered semantic store integration is staged separately.
pub(crate) mod indexed;
#[allow(dead_code)] // Interval-index writer integration is staged separately.
pub(crate) mod interval;

fn reset_preserves(cc: u8) -> bool {
    matches!(cc, 0|32|7|39|10|42|70..=79|91..=95|122|124..=127)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NoteState {
    velocity: u8,
    flags: u8,
    pressure: u8,
}
impl Default for NoteState {
    fn default() -> Self {
        Self {
            velocity: 0,
            flags: 0,
            pressure: 128,
        }
    }
}
impl NoteState {
    pub fn velocity(self) -> u8 {
        self.velocity
    }
    pub fn held(self) -> bool {
        self.flags & 1 != 0
    }
    pub fn sostenuto(self) -> bool {
        self.flags & 2 != 0
    }
    pub fn pressure(self) -> Option<u8> {
        (self.pressure < 128).then_some(self.pressure)
    }
    fn set_held(&mut self, held: bool) {
        self.flags = (self.flags & !1) | u8::from(held);
    }
    fn set_sostenuto(&mut self, sostenuto: bool) {
        self.flags = (self.flags & !2) | (u8::from(sostenuto) << 1);
    }
}
/// Last observed selector family; a reset establishes the null RPN baseline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParameterFamily {
    Rpn,
    Nrpn,
}

struct Keys(u128);
impl Iterator for Keys {
    type Item = u8;
    fn next(&mut self) -> Option<Self::Item> {
        if self.0 == 0 {
            return None;
        }
        let key = self.0.trailing_zeros() as u8;
        self.0 &= self.0 - 1;
        Some(key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelState {
    notes: [NoteState; 128],
    controllers: [u8; 128],
    note_keys: u128,
    controller_keys: u128,
    program: Option<u8>,
    program_bank: [Option<u8>; 2],
    pitch_bend: u16,
    pressure: Option<u8>,
    reset_controllers: bool,
    parameter_family: Option<ParameterFamily>,
    parameter_history_untracked: bool,
}
impl ChannelState {
    const INITIAL: Self = {
        Self {
            notes: [NoteState {
                velocity: 0,
                flags: 0,
                pressure: 128,
            }; 128],
            controllers: [0; 128],
            note_keys: 0,
            controller_keys: (1 << 64) | (1 << 66) | (1 << 69),
            program: None,
            program_bank: [None; 2],
            pitch_bend: 8192,
            pressure: None,
            reset_controllers: false,
            parameter_family: None,
            parameter_history_untracked: false,
        }
    };
}
impl Default for ChannelState {
    fn default() -> Self {
        Self::INITIAL
    }
}
impl ChannelState {
    pub fn note(&self, key: u8) -> Option<NoteState> {
        self.notes.get(key as usize).copied()
    }
    pub fn controller(&self, controller: u8) -> Option<u8> {
        let value = *self.controllers.get(controller as usize)?;
        (self.controller_keys & (1u128 << controller) != 0).then_some(value)
    }
    /// Non-default notes in ascending key order, including known zero pressure
    /// and pedal-latched notes. Omitted keys still have the default projection.
    pub fn projected_notes(&self) -> impl Iterator<Item = (u8, NoteState)> + '_ {
        Keys(self.note_keys).map(|key| (key, self.notes[key as usize]))
    }
    /// Known controls in ascending number order, including known zero values.
    pub fn known_controllers(&self) -> impl Iterator<Item = (u8, u8)> + '_ {
        Keys(self.controller_keys).map(|cc| (cc, self.controllers[cc as usize]))
    }
    fn refresh_note(&mut self, key: u8) {
        let note = self.notes[key as usize];
        let bit = 1u128 << key;
        if note.velocity != 0 || note.held() || note.sostenuto() || note.pressure().is_some() {
            self.note_keys |= bit;
        } else {
            self.note_keys &= !bit;
        }
    }
    fn refresh_notes(&mut self) {
        for key in 0..128 {
            self.refresh_note(key);
        }
    }
    fn set_controller(&mut self, cc: u8, value: Option<u8>) {
        self.controllers[cc as usize] = value.unwrap_or(0);
        let bit = 1u128 << cc;
        if value.is_some() {
            self.controller_keys |= bit;
        } else {
            self.controller_keys &= !bit;
        }
    }
    pub fn program(&self) -> Option<u8> {
        self.program
    }
    /// Bank selectors observed when the current program was selected. Later
    /// bank messages may be pending for a future program change.
    pub fn program_bank(&self) -> [Option<u8>; 2] {
        self.program_bank
    }
    pub fn pitch_bend(&self) -> u16 {
        self.pitch_bend
    }
    pub fn pressure(&self) -> Option<u8> {
        self.pressure
    }
    /// Restoration must establish this baseline before replaying known controls.
    pub fn reset_controllers_seen(&self) -> bool {
        self.reset_controllers
    }
    pub fn parameter_family(&self) -> Option<ParameterFamily> {
        self.parameter_family
    }
    /// Parameter data/relative commands occurred; their parameter-value history
    /// is not represented by this channel/key projection.
    pub fn parameter_history_untracked(&self) -> bool {
        self.parameter_history_untracked
    }
    fn down(&self, controller: usize) -> bool {
        self.controller(controller as u8).is_some_and(|v| v >= 64)
    }
    fn release_unheld(&mut self) {
        if self.down(64) || self.down(69) {
            return;
        }
        for note in &mut self.notes {
            if !note.held() && !note.sostenuto() {
                note.velocity = 0;
            }
        }
        self.refresh_notes();
    }
    fn notes_off(&mut self, sound_off: bool) {
        for note in &mut self.notes {
            note.set_held(false);
            if sound_off {
                note.velocity = 0;
                note.set_sostenuto(false);
            }
        }
        self.refresh_notes();
        self.release_unheld();
    }
    fn reset_controls(&mut self) {
        // Retain controls that RP-015 leaves unchanged. Device-specific defaults
        // remain unknown; the reset marker preserves their baseline operation.
        for cc in 0..128 {
            if !reset_preserves(cc as u8) {
                self.set_controller(cc as u8, None);
            }
        }
        for cc in [1, 64, 65, 66, 67, 69] {
            self.set_controller(cc, Some(0));
        }
        self.set_controller(11, Some(127));
        for cc in 98..=101 {
            self.set_controller(cc, Some(127));
        }
        self.pitch_bend = 8192;
        self.pressure = Some(0);
        self.reset_controllers = true;
        self.parameter_family = Some(ParameterFamily::Rpn);
        for note in &mut self.notes {
            note.set_sostenuto(false);
            note.pressure = 0;
        }
        self.note_keys = u128::MAX;
        self.release_unheld();
    }
    fn control(&mut self, cc: u8, value: u8) {
        match cc {
            96 | 97 => self.parameter_history_untracked = true,
            120 => self.notes_off(true),
            121 => self.reset_controls(),
            123 => self.notes_off(false),
            66 => {
                let was_down = self.down(66);
                self.set_controller(66, Some(value));
                if value < 64 && was_down {
                    for note in &mut self.notes {
                        note.set_sostenuto(false);
                    }
                    self.refresh_notes();
                    self.release_unheld();
                } else if value >= 64 && !was_down {
                    for note in &mut self.notes {
                        note.set_sostenuto(note.held());
                    }
                    self.refresh_notes();
                }
            }
            _ => {
                let released_hold = matches!(cc, 64 | 69) && self.down(cc as usize) && value < 64;
                self.set_controller(cc, Some(value));
                match cc {
                    6 | 38 => self.parameter_history_untracked = true,
                    98 | 99 => self.parameter_family = Some(ParameterFamily::Nrpn),
                    100 | 101 => self.parameter_family = Some(ParameterFamily::Rpn),
                    124 => self.set_controller(125, None),
                    125 => self.set_controller(124, None),
                    126 => self.set_controller(127, None),
                    127 => self.set_controller(126, None),
                    _ => {}
                }
                if matches!(cc, 124..=127) {
                    self.notes_off(false);
                } else if released_hold {
                    self.release_unheld();
                }
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiState {
    channels: [ChannelState; 16],
    system_reset: bool,
}
impl Default for MidiState {
    fn default() -> Self {
        Self {
            channels: [ChannelState::INITIAL; 16],
            system_reset: false,
        }
    }
}
impl MidiState {
    /// Reassemble exported opaque channel projections. Callers must use channels
    /// and the reset marker from the same immutable state cut.
    pub fn from_channels(channels: [ChannelState; 16], system_reset_seen: bool) -> Self {
        Self {
            channels,
            system_reset: system_reset_seen,
        }
    }
    pub fn system_reset_seen(&self) -> bool {
        self.system_reset
    }
    pub fn channel(&self, channel: u8) -> Option<&ChannelState> {
        self.channels.get(channel as usize)
    }
    pub fn apply_packet(&mut self, data: &[u8], max_bytes: usize) -> Result<(), Error> {
        let message = Message::parse(data, max_bytes)?;
        self.apply(message);
        Ok(())
    }
    /// Repeated note-ons retain the most recent velocity for that key. Packets
    /// remain distinct in the event stream; this is not a per-voice synth model.
    pub fn apply(&mut self, message: Message<'_>) {
        let data = message.data();
        if data[0] == 0xff {
            *self = Self::default();
            self.system_reset = true;
            return;
        }
        let channel = &mut self.channels[(data[0] & 15) as usize];
        match message.kind() {
            MessageKind::NoteOn { key, velocity, .. } => {
                channel.notes[key as usize].velocity = velocity;
                channel.notes[key as usize].set_held(true);
                channel.refresh_note(key);
            }
            MessageKind::NoteOff { key, .. } => {
                let sustain = channel.down(64) || channel.down(69);
                let note = &mut channel.notes[key as usize];
                note.set_held(false);
                if !sustain && !note.sostenuto() {
                    note.velocity = 0;
                }
                channel.refresh_note(key);
            }
            MessageKind::ControlChange {
                controller, value, ..
            } => channel.control(controller, value),
            _ => match data[0] & 0xf0 {
                0xa0 => {
                    channel.notes[data[1] as usize].pressure = data[2];
                    channel.refresh_note(data[1]);
                }
                0xc0 => {
                    channel.program = Some(data[1]);
                    channel.program_bank = [channel.controller(0), channel.controller(32)];
                }
                0xd0 => channel.pressure = Some(data[1]),
                0xe0 => channel.pitch_bend = u16::from(data[1]) | (u16::from(data[2]) << 7),
                _ => {}
            },
        }
    }
}
