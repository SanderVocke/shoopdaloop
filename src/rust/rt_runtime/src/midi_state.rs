//! Fixed MIDI 1.0 channel/key projection for initial states and checkpoints.
//! This records protocol state, not synthesizer voices or vendor SysEx state.
use crate::{
    midi::{Message, MessageKind},
    Error,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoteState {
    velocity: u8,
    held: bool,
    sostenuto: bool,
    pressure: Option<u8>,
}
impl NoteState {
    pub fn velocity(self) -> u8 {
        self.velocity
    }
    pub fn held(self) -> bool {
        self.held
    }
    pub fn sostenuto(self) -> bool {
        self.sostenuto
    }
    pub fn pressure(self) -> Option<u8> {
        self.pressure
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelState {
    notes: [NoteState; 128],
    controllers: [Option<u8>; 128],
    program: Option<u8>,
    pitch_bend: u16,
    pressure: Option<u8>,
    reset_controllers: bool,
}
impl Default for ChannelState {
    fn default() -> Self {
        let mut controllers = [None; 128];
        for cc in [64, 66, 69] {
            controllers[cc] = Some(0);
        }
        Self {
            notes: [NoteState::default(); 128],
            controllers,
            program: None,
            pitch_bend: 8192,
            pressure: None,
            reset_controllers: false,
        }
    }
}
impl ChannelState {
    pub fn note(&self, key: u8) -> Option<NoteState> {
        self.notes.get(key as usize).copied()
    }
    pub fn controller(&self, controller: u8) -> Option<u8> {
        self.controllers.get(controller as usize).copied().flatten()
    }
    pub fn program(&self) -> Option<u8> {
        self.program
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
    fn down(&self, controller: usize) -> bool {
        self.controllers[controller].is_some_and(|v| v >= 64)
    }
    fn release_unheld(&mut self) {
        let sustain = self.down(64) || self.down(69);
        for note in &mut self.notes {
            if !note.held && !note.sostenuto && !sustain {
                note.velocity = 0;
            }
        }
    }
    fn notes_off(&mut self, sound_off: bool) {
        for note in &mut self.notes {
            note.held = false;
            if sound_off {
                note.velocity = 0;
                note.sostenuto = false;
            }
        }
        self.release_unheld();
    }
    fn reset_controls(&mut self) {
        // Retain controls that RP-015 leaves unchanged. Device-specific defaults
        // remain unknown; the reset marker preserves their baseline operation.
        for cc in 0..128 {
            if !matches!(cc,0|32|7|39|10|42|70..=79|91..=95|122|124..=127) {
                self.controllers[cc] = None;
            }
        }
        for cc in [1, 64, 65, 66, 67, 69] {
            self.controllers[cc] = Some(0);
        }
        self.controllers[11] = Some(127);
        for cc in 98..=101 {
            self.controllers[cc] = Some(127);
        }
        self.pitch_bend = 8192;
        self.pressure = Some(0);
        self.reset_controllers = true;
        for note in &mut self.notes {
            note.sostenuto = false;
            note.pressure = Some(0);
        }
        self.release_unheld();
    }
    fn control(&mut self, cc: u8, value: u8) {
        match cc {
            120 => self.notes_off(true),
            121 => self.reset_controls(),
            123 => self.notes_off(false),
            66 => {
                let was_down = self.down(66);
                self.controllers[66] = Some(value);
                if value < 64 {
                    for note in &mut self.notes {
                        note.sostenuto = false;
                    }
                } else if !was_down {
                    for note in &mut self.notes {
                        note.sostenuto = note.held;
                    }
                }
                self.release_unheld();
            }
            _ => {
                self.controllers[cc as usize] = Some(value);
                if matches!(cc, 124..=127) {
                    self.notes_off(false);
                } else if matches!(cc, 64 | 69) {
                    self.release_unheld();
                }
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MidiState {
    channels: [ChannelState; 16],
    system_reset: bool,
}
impl MidiState {
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
                channel.notes[key as usize].held = true;
            }
            MessageKind::NoteOff { key, .. } => {
                let sustain = channel.down(64) || channel.down(69);
                let note = &mut channel.notes[key as usize];
                note.held = false;
                if !sustain && !note.sostenuto {
                    note.velocity = 0;
                }
            }
            MessageKind::ControlChange {
                controller, value, ..
            } => channel.control(controller, value),
            _ => match data[0] & 0xf0 {
                0xa0 => channel.notes[data[1] as usize].pressure = Some(data[2]),
                0xc0 => channel.program = Some(data[1]),
                0xd0 => channel.pressure = Some(data[1]),
                0xe0 => channel.pitch_bend = u16::from(data[1]) | (u16::from(data[2]) << 7),
                _ => {}
            },
        }
    }
}
