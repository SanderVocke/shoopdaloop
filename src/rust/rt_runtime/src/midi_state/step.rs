use super::{reset_preserves, ChannelState, MidiState};
use crate::midi::ShortMessage;
use crate::Error;

const CHANNEL_CELLS: usize = 128 + 128 + 8;
pub const BASIC_STATE_CELLS: usize = 16 * CHANNEL_CELLS + 1;
const DEFAULT_CONTROLS: [(u8, u8); 7] = [
    (1, 0),
    (64, 0),
    (65, 0),
    (66, 0),
    (67, 0),
    (69, 0),
    (11, 127),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NoteAction {
    ReleaseUnheld,
    Latch,
    ClearLatch,
    NotesOff,
    SoundOff,
    ResetControls,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Start,
    Controls(u8),
    Defaults(u8),
    Notes { action: NoteAction, key: u8 },
    Reset(usize),
    Done,
}

#[derive(Clone, Copy, Debug)]
pub struct BasicUpdate {
    message: ShortMessage,
    phase: Phase,
}
impl BasicUpdate {
    pub fn begin(message: ShortMessage) -> Result<Self, Error> {
        ShortMessage::managed(message.as_bytes()).map_err(Error::from)?;
        Ok(Self {
            message,
            phase: Phase::Start,
        })
    }
    pub fn complete(&self) -> bool {
        self.phase == Phase::Done
    }
    pub fn step(&mut self, state: &mut MidiState) -> bool {
        let data = self.message.as_bytes();
        let channel = usize::from(data[0] & 15);
        match self.phase {
            Phase::Done => return true,
            Phase::Reset(cell) => {
                state.copy_basic_cell(None, cell);
                self.phase = if cell + 1 == BASIC_STATE_CELLS {
                    state.system_reset = true;
                    Phase::Done
                } else {
                    Phase::Reset(cell + 1)
                };
            }
            Phase::Controls(cc) => {
                if !reset_preserves(cc) {
                    state.channels[channel].set_controller(cc, None);
                }
                self.phase = if cc == 127 {
                    Phase::Defaults(0)
                } else {
                    Phase::Controls(cc + 1)
                };
            }
            Phase::Defaults(index) => {
                let (cc, value) = DEFAULT_CONTROLS[usize::from(index)];
                state.channels[channel].set_controller(cc, Some(value));
                self.phase = if usize::from(index) + 1 == DEFAULT_CONTROLS.len() {
                    let state = &mut state.channels[channel];
                    state.pitch_bend = 8192;
                    state.pressure = Some(0);
                    state.reset_controllers = true;
                    state.parameter_family = None;
                    state.parameter_history_untracked = false;
                    Phase::Notes {
                        action: NoteAction::ResetControls,
                        key: 0,
                    }
                } else {
                    Phase::Defaults(index + 1)
                };
            }
            Phase::Notes { action, key } => {
                let state = &mut state.channels[channel];
                let holds = state.down(64) || state.down(69);
                let note = &mut state.notes[usize::from(key)];
                match action {
                    NoteAction::Latch => note.set_sostenuto(note.held()),
                    NoteAction::ClearLatch | NoteAction::ResetControls => note.set_sostenuto(false),
                    NoteAction::NotesOff | NoteAction::SoundOff => note.set_held(false),
                    NoteAction::ReleaseUnheld => {}
                }
                if action == NoteAction::SoundOff {
                    note.velocity = 0;
                    note.set_sostenuto(false);
                } else if !note.held() && !note.sostenuto() && !holds {
                    note.velocity = 0;
                }
                if action == NoteAction::ResetControls {
                    note.pressure = 0;
                }
                state.refresh_note(key);
                self.phase = if key == 127 {
                    Phase::Done
                } else {
                    Phase::Notes {
                        action,
                        key: key + 1,
                    }
                };
            }
            Phase::Start => {
                self.phase = Phase::Done;
                if data[0] == 0xff {
                    self.phase = Phase::Reset(0);
                    return false;
                }
                if data[0] & 0xf0 != 0xb0 {
                    state.apply(
                        crate::midi::Message::parse(data, 4).expect("managed short message"),
                    );
                    return true;
                }
                let (cc, value) = (data[1], data[2]);
                let state = &mut state.channels[channel];
                let action = match cc {
                    120 => Some(NoteAction::SoundOff),
                    121 => {
                        self.phase = Phase::Controls(0);
                        None
                    }
                    123 => Some(NoteAction::NotesOff),
                    66 => {
                        let old = state.down(66);
                        state.set_controller(cc, Some(value));
                        if !old && value >= 64 {
                            Some(NoteAction::Latch)
                        } else if old && value < 64 {
                            Some(NoteAction::ClearLatch)
                        } else {
                            None
                        }
                    }
                    _ => {
                        let released =
                            matches!(cc, 64 | 69) && state.down(cc as usize) && value < 64;
                        state.set_controller(cc, Some(value));
                        match cc {
                            124 => state.set_controller(125, None),
                            125 => state.set_controller(124, None),
                            126 => state.set_controller(127, None),
                            127 => state.set_controller(126, None),
                            _ => {}
                        }
                        if matches!(cc, 124..=127) {
                            Some(NoteAction::NotesOff)
                        } else if released {
                            Some(NoteAction::ReleaseUnheld)
                        } else {
                            None
                        }
                    }
                };
                if let Some(action) = action {
                    self.phase = Phase::Notes { action, key: 0 };
                }
            }
        }
        self.complete()
    }
}

impl MidiState {
    pub fn copy_basic_cell(&mut self, source: Option<&Self>, cell: usize) -> bool {
        assert!(cell < BASIC_STATE_CELLS);
        if cell == BASIC_STATE_CELLS - 1 {
            self.system_reset = source.is_some_and(|state| state.system_reset);
            return true;
        }
        let channel = cell / CHANNEL_CELLS;
        let field = cell % CHANNEL_CELLS;
        let initial = &ChannelState::INITIAL;
        let source = source.map_or(initial, |state| &state.channels[channel]);
        let target = &mut self.channels[channel];
        match field {
            0..128 => {
                target.notes[field] = source.notes[field];
                target.refresh_note(field as u8);
            }
            128..256 => {
                let cc = (field - 128) as u8;
                let value = if matches!(cc, 6 | 38 | 96..=101) {
                    None
                } else {
                    source.controller(cc)
                };
                target.set_controller(cc, value);
            }
            256 => target.program = source.program,
            257 => target.program_bank[0] = source.program_bank[0],
            258 => target.program_bank[1] = source.program_bank[1],
            259 => target.pitch_bend = source.pitch_bend,
            260 => target.pressure = source.pressure,
            261 => target.reset_controllers = source.reset_controllers,
            262 => target.parameter_family = None,
            263 => target.parameter_history_untracked = false,
            _ => unreachable!(),
        }
        false
    }
    pub fn basic_projection(&self) -> Self {
        let mut result = Self::default();
        for cell in 0..BASIC_STATE_CELLS {
            result.copy_basic_cell(Some(self), cell);
        }
        result
    }
}
