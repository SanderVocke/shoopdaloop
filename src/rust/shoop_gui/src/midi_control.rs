//! Binding incoming MIDI to actions, so a controller can drive the grid.
//!
//! The existing GUI has a whole editor for this, with message filters. This is the same
//! idea reduced to what a looper actually needs: a trigger, an action, and a way to say
//! "any channel". Kept separate from the UI because matching is fiddly -- note-on with
//! zero velocity is a note-off, wildcards have to not match too much -- and none of that
//! is visible by looking at a window.
//!
//! Deliberately not stored in the session file yet: a mapping belongs to a controller
//! rather than to a piece of music, and mixing the two would mean a session carrying
//! bindings for hardware the next person does not have.

use crate::selection::Cell;

/// What an incoming message can be matched against.
///
/// `channel` is `None` for "any", which is what most people want: a controller usually
/// sends on one channel and nobody wants to rebind when it changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// A note being pressed. Velocity is ignored except that zero counts as a release.
    Note { channel: Option<u8>, note: u8 },
    /// A controller moving. Matched regardless of value, since the value is the argument.
    Control { channel: Option<u8>, controller: u8 },
    /// A program change, which some controllers send from scene buttons.
    Program { channel: Option<u8>, program: u8 },
}

impl Trigger {
    fn channel_matches(expected: Option<u8>, actual: u8) -> bool {
        expected.is_none_or(|c| c == actual)
    }

    /// Whether `data` fires this trigger, and the value it carries if any.
    ///
    /// The value is what a continuous action needs; a note gives none, because binding a
    /// level to a note's velocity is more surprising than useful.
    pub fn matches(&self, data: &[u8]) -> Option<Option<u8>> {
        if data.len() < 2 {
            return None;
        }
        let channel = data[0] & 0x0F;
        let status = data[0] & 0xF0;
        match *self {
            Trigger::Note { channel: c, note } => {
                // Only presses. A release, including note-on with zero velocity, must not
                // fire an action again -- a bound button would act twice per press.
                let is_press = status == 0x90 && *data.get(2).unwrap_or(&0) > 0;
                (is_press && Self::channel_matches(c, channel) && data[1] == note).then_some(None)
            }
            Trigger::Control {
                channel: c,
                controller,
            } => (status == 0xB0 && Self::channel_matches(c, channel) && data[1] == controller)
                .then(|| Some(*data.get(2).unwrap_or(&0))),
            Trigger::Program {
                channel: c,
                program,
            } => (status == 0xC0 && Self::channel_matches(c, channel) && data[1] == program)
                .then_some(None),
        }
    }
}

/// What a binding does when it fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlAction {
    Record(Cell),
    Play(Cell),
    Stop(Cell),
    Clear(Cell),
    /// Muting a track. Toggled rather than set, so one button works.
    ToggleTrackMute(usize),
    /// A track's level, taken from the message's value.
    SetTrackGain(usize),
    StopAll,
    RunComposite,
    HaltComposite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub trigger: Trigger,
    pub action: ControlAction,
}

/// A fired binding, with the value the message carried.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fired {
    pub action: ControlAction,
    /// Controller value scaled to `0.0..=1.0`, for the actions that take one.
    pub value: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct Mapping {
    pub bindings: Vec<Binding>,
}

impl Mapping {
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Everything `data` fires, in the order the bindings were added.
    ///
    /// All matches rather than the first: binding one button to several actions is a
    /// reasonable thing to want, and silently ignoring the rest would be surprising.
    pub fn resolve(&self, data: &[u8]) -> Vec<Fired> {
        self.bindings
            .iter()
            .filter_map(|b| {
                b.trigger.matches(data).map(|value| Fired {
                    action: b.action,
                    // 127 rather than 128, so a full-scale controller reaches exactly 1.0.
                    value: value.map(|v| v as f32 / 127.0),
                })
            })
            .collect()
    }

    /// A starting mapping for a generic controller, so something works out of the box.
    ///
    /// Notes 36 upwards -- the usual bottom-left pad on a grid controller -- play the grid
    /// row by row, and controllers 1 upwards are track levels.
    pub fn default_for_grid(n_tracks: usize, n_rows: usize) -> Self {
        let mut bindings = Vec::new();
        let mut note = 36u8;
        for track in 0..n_tracks {
            for row in 0..n_rows {
                bindings.push(Binding {
                    trigger: Trigger::Note {
                        channel: None,
                        note,
                    },
                    action: ControlAction::Play(Cell { track, row }),
                });
                note = note.saturating_add(1);
            }
        }
        for track in 0..n_tracks {
            bindings.push(Binding {
                trigger: Trigger::Control {
                    channel: None,
                    controller: 1 + track as u8,
                },
                action: ControlAction::SetTrackGain(track),
            });
        }
        bindings.push(Binding {
            trigger: Trigger::Control {
                channel: None,
                controller: 123,
            },
            action: ControlAction::StopAll,
        });
        Self { bindings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_engine::midi;

    fn cell(track: usize, row: usize) -> Cell {
        Cell { track, row }
    }

    fn note_binding(note: u8, action: ControlAction) -> Binding {
        Binding {
            trigger: Trigger::Note {
                channel: None,
                note,
            },
            action,
        }
    }

    #[test]
    fn a_note_press_fires_its_binding() {
        let m = Mapping {
            bindings: vec![note_binding(60, ControlAction::Play(cell(0, 0)))],
        };
        let fired = m.resolve(&midi::note_on(0, 60, 100));
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].action, ControlAction::Play(cell(0, 0)));
        assert_eq!(fired[0].value, None);
    }

    #[test]
    fn a_note_release_does_not_fire() {
        let m = Mapping {
            bindings: vec![note_binding(60, ControlAction::Play(cell(0, 0)))],
        };
        assert!(m.resolve(&midi::note_off(0, 60, 64)).is_empty());
        // Note-on with zero velocity is a release too, and would otherwise fire twice per
        // press on controllers that send it.
        assert!(m.resolve(&midi::note_on(0, 60, 0)).is_empty());
    }

    #[test]
    fn a_different_note_does_not_fire() {
        let m = Mapping {
            bindings: vec![note_binding(60, ControlAction::Play(cell(0, 0)))],
        };
        assert!(m.resolve(&midi::note_on(0, 61, 100)).is_empty());
    }

    #[test]
    fn any_channel_matches_but_a_named_channel_does_not() {
        let any = Mapping {
            bindings: vec![note_binding(60, ControlAction::StopAll)],
        };
        assert_eq!(any.resolve(&midi::note_on(9, 60, 100)).len(), 1);

        let only_three = Mapping {
            bindings: vec![Binding {
                trigger: Trigger::Note {
                    channel: Some(3),
                    note: 60,
                },
                action: ControlAction::StopAll,
            }],
        };
        assert_eq!(only_three.resolve(&midi::note_on(3, 60, 100)).len(), 1);
        assert!(only_three.resolve(&midi::note_on(4, 60, 100)).is_empty());
    }

    #[test]
    fn a_controller_carries_its_value_scaled() {
        let m = Mapping {
            bindings: vec![Binding {
                trigger: Trigger::Control {
                    channel: None,
                    controller: 7,
                },
                action: ControlAction::SetTrackGain(2),
            }],
        };

        let fired = m.resolve(&midi::cc(0, 7, 127));
        assert_eq!(fired.len(), 1);
        // Full scale reaches exactly one, which is why the divisor is 127.
        assert_eq!(fired[0].value, Some(1.0));

        let fired = m.resolve(&midi::cc(0, 7, 0));
        assert_eq!(fired[0].value, Some(0.0));

        let fired = m.resolve(&midi::cc(0, 7, 64));
        assert!((fired[0].value.unwrap() - 0.5039).abs() < 0.001);
    }

    #[test]
    fn a_controller_fires_at_every_value_including_zero() {
        // A level going to zero must still be delivered, or a fader could never reach
        // silence.
        let m = Mapping {
            bindings: vec![Binding {
                trigger: Trigger::Control {
                    channel: None,
                    controller: 1,
                },
                action: ControlAction::SetTrackGain(0),
            }],
        };
        assert_eq!(m.resolve(&midi::cc(0, 1, 0)).len(), 1);
    }

    #[test]
    fn a_program_change_fires() {
        let m = Mapping {
            bindings: vec![Binding {
                trigger: Trigger::Program {
                    channel: None,
                    program: 5,
                },
                action: ControlAction::RunComposite,
            }],
        };
        assert_eq!(m.resolve(&midi::program_change(0, 5)).len(), 1);
        assert!(m.resolve(&midi::program_change(0, 6)).is_empty());
    }

    #[test]
    fn one_message_can_fire_several_bindings() {
        let m = Mapping {
            bindings: vec![
                note_binding(60, ControlAction::Play(cell(0, 0))),
                note_binding(60, ControlAction::Play(cell(1, 0))),
            ],
        };
        assert_eq!(m.resolve(&midi::note_on(0, 60, 100)).len(), 2);
    }

    #[test]
    fn unrelated_messages_fire_nothing() {
        let m = Mapping::default_for_grid(4, 4);
        // Pitch wheel is not bound by the default mapping.
        assert!(m.resolve(&midi::pitch_wheel(0, 1000)).is_empty());
        // Nor is a note below the pad range.
        assert!(m.resolve(&midi::note_on(0, 20, 100)).is_empty());
    }

    #[test]
    fn a_truncated_message_is_ignored_rather_than_panicking() {
        let m = Mapping::default_for_grid(4, 4);
        assert!(m.resolve(&[]).is_empty());
        assert!(m.resolve(&[0x90]).is_empty());
    }

    #[test]
    fn the_default_mapping_covers_the_grid_and_the_levels() {
        let m = Mapping::default_for_grid(4, 4);
        // Sixteen pads, four levels, one panic button.
        assert_eq!(m.bindings.len(), 16 + 4 + 1);

        // Note 36 is the first cell, and the grid fills row by row within a track.
        let fired = m.resolve(&midi::note_on(0, 36, 100));
        assert_eq!(fired[0].action, ControlAction::Play(cell(0, 0)));
        let fired = m.resolve(&midi::note_on(0, 37, 100));
        assert_eq!(fired[0].action, ControlAction::Play(cell(0, 1)));
        let fired = m.resolve(&midi::note_on(0, 40, 100));
        assert_eq!(fired[0].action, ControlAction::Play(cell(1, 0)));

        // All Notes Off doubles as stop-everything, which is what a panicking player hits.
        let fired = m.resolve(&midi::cc(0, 123, 0));
        assert_eq!(fired[0].action, ControlAction::StopAll);
    }
}
