//! Computer keyboard to MIDI notes.
//!
//! The two-row layout every tracker and DAW uses: the bottom letter row is one octave
//! with its black keys on the row above, and the QWERTY row is the octave above that.
//! Familiar enough that anyone who has played a soft synth can use it without being
//! told.

use egui::Key;

/// Semitone offset from the base octave's C, for each mapped key.
///
/// Two ranges: `Z`-row starting at the base C, `Q`-row an octave up. The gaps in the
/// upper rows are the keys where a piano has no black note, which is why the layout is
/// legible at all.
const LAYOUT: &[(Key, i32)] = &[
    // Lower octave: white keys on the Z row, black keys on the A row above.
    (Key::Z, 0),  // C
    (Key::S, 1),  // C#
    (Key::X, 2),  // D
    (Key::D, 3),  // D#
    (Key::C, 4),  // E
    (Key::V, 5),  // F
    (Key::G, 6),  // F#
    (Key::B, 7),  // G
    (Key::H, 8),  // G#
    (Key::N, 9),  // A
    (Key::J, 10), // A#
    (Key::M, 11), // B
    // Upper octave: white keys on the Q row, black keys on the number row.
    (Key::Q, 12),
    (Key::Num2, 13),
    (Key::W, 14),
    (Key::Num3, 15),
    (Key::E, 16),
    (Key::R, 17),
    (Key::Num5, 18),
    (Key::T, 19),
    (Key::Num6, 20),
    (Key::Y, 21),
    (Key::Num7, 22),
    (Key::U, 23),
    // One more, so the upper octave can be completed.
    (Key::I, 24),
];

/// What a non-note key does.
///
/// Kept as data next to the note layout so a collision is impossible to introduce quietly: the note
/// rows and the transport keys are checked against each other in this file's tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Start recording into the selection, or stop a take already running.
    ///
    /// One key for both, as a looper pedal is: the alternative is a key that can only start, which
    /// leaves stopping to the mouse in the middle of playing.
    RecordOrStop,
    /// Play the selection, or stop it if it is already playing.
    PlayOrStop,
    /// Stop everything, at once.
    StopAll,
    /// Silence the selection's bar.
    Clear,
    OctaveDown,
    OctaveUp,
}

/// Transport keys, deliberately none of them letters: the letter rows are the instrument, and a
/// player's hands are already there.
const ACTIONS: &[(Key, Action)] = &[
    (Key::Space, Action::RecordOrStop),
    (Key::Enter, Action::PlayOrStop),
    (Key::Escape, Action::StopAll),
    (Key::Backspace, Action::Clear),
    (Key::ArrowLeft, Action::OctaveDown),
    (Key::ArrowRight, Action::OctaveUp),
];

/// The action a key performs, or `None` if it is not a transport key.
pub fn key_to_action(key: Key) -> Option<Action> {
    ACTIONS.iter().find(|(k, _)| *k == key).map(|(_, a)| *a)
}

/// The transport keys as a hint line, so it cannot drift from the mapping.
pub fn action_hint() -> String {
    ACTIONS
        .iter()
        .map(|(k, a)| {
            let what = match a {
                Action::RecordOrStop => "record/stop",
                Action::PlayOrStop => "play/stop",
                Action::StopAll => "stop all",
                Action::Clear => "clear",
                Action::OctaveDown => "octave -",
                Action::OctaveUp => "octave +",
            };
            format!("{} {}", k.symbol_or_name(), what)
        })
        .collect::<Vec<_>>()
        .join("  ·  ")
}

/// Lowest and highest base octave, so the mapped range stays inside MIDI's 0..128.
pub const MIN_OCTAVE: i32 = 0;
pub const MAX_OCTAVE: i32 = 8;

/// The note a key plays, given the base octave, or `None` if the key is not mapped.
///
/// Octave 4 puts `Z` on middle C (note 60), matching how these layouts are usually
/// labelled.
pub fn key_to_note(key: Key, octave: i32) -> Option<u8> {
    let semitone = LAYOUT.iter().find(|(k, _)| *k == key)?.1;
    let note = (octave + 1) * 12 + semitone;
    (0..128).contains(&note).then_some(note as u8)
}

/// Every key the layout uses, for a caller that wants to show them.
pub fn mapped_keys() -> impl Iterator<Item = Key> {
    LAYOUT.iter().map(|(k, _)| *k)
}

/// The layout as playable rows, for a hint line that cannot drift from the mapping.
pub fn hint() -> String {
    let names: Vec<String> = mapped_keys()
        .map(|k| k.symbol_or_name().to_string())
        .collect();
    let (lower, upper) = names.split_at(12);
    format!("{}  ·  {}", lower.join(" "), upper.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn z_is_middle_c_at_octave_four() {
        assert_eq!(key_to_note(Key::Z, 4), Some(60));
    }

    #[test]
    fn the_lower_row_walks_up_in_semitones() {
        // C, C#, D, D#, E on the conventional zig-zag between the two rows.
        assert_eq!(key_to_note(Key::Z, 4), Some(60));
        assert_eq!(key_to_note(Key::S, 4), Some(61));
        assert_eq!(key_to_note(Key::X, 4), Some(62));
        assert_eq!(key_to_note(Key::D, 4), Some(63));
        assert_eq!(key_to_note(Key::C, 4), Some(64));
    }

    #[test]
    fn the_upper_row_is_an_octave_above_the_lower() {
        assert_eq!(key_to_note(Key::Q, 4), Some(72));
        assert_eq!(
            key_to_note(Key::Q, 4).unwrap() - key_to_note(Key::Z, 4).unwrap(),
            12
        );
    }

    #[test]
    fn an_unmapped_key_plays_nothing() {
        assert_eq!(key_to_note(Key::Space, 4), None);
        assert_eq!(key_to_note(Key::ArrowLeft, 4), None);
    }

    #[test]
    fn notes_outside_midi_range_are_refused_rather_than_wrapped() {
        // The top octave plus two octaves of layout would run past 127.
        assert_eq!(key_to_note(Key::I, MAX_OCTAVE), None);
        // But the bottom of that octave still fits.
        assert!(key_to_note(Key::Z, MAX_OCTAVE).is_some());
    }

    #[test]
    fn every_mapped_key_resolves_in_the_middle_of_the_range() {
        for k in mapped_keys() {
            assert!(key_to_note(k, 4).is_some(), "{k:?} did not map");
        }
    }

    #[test]
    fn no_key_is_mapped_twice() {
        let mut keys: Vec<_> = mapped_keys().collect();
        let before = keys.len();
        keys.sort_by_key(|k| format!("{k:?}"));
        keys.dedup();
        assert_eq!(keys.len(), before, "a key appears more than once");
    }

    #[test]
    fn no_key_both_plays_a_note_and_drives_the_transport() {
        // The collision that would be invisible otherwise: a transport key stealing a note, or a
        // note key silently triggering a recording.
        for k in mapped_keys() {
            assert_eq!(
                key_to_action(k),
                None,
                "{k:?} is both a note and a transport key"
            );
        }
    }

    #[test]
    fn the_transport_keys_are_not_letters() {
        // Letters belong to the instrument; a player's hands are already on them.
        for (k, _) in ACTIONS {
            let name = format!("{k:?}");
            assert!(
                name.len() > 1,
                "{name} is a single letter, which the instrument needs"
            );
        }
    }

    #[test]
    fn every_action_is_reachable() {
        use Action::*;
        for a in [
            RecordOrStop,
            PlayOrStop,
            StopAll,
            Clear,
            OctaveDown,
            OctaveUp,
        ] {
            assert!(ACTIONS.iter().any(|(_, x)| *x == a), "{a:?} has no key");
        }
    }

    #[test]
    fn the_action_hint_names_every_key() {
        let hint = action_hint();
        for (k, _) in ACTIONS {
            assert!(
                hint.contains(k.symbol_or_name()),
                "{hint} does not mention {k:?}"
            );
        }
    }

    #[test]
    fn no_action_key_is_mapped_twice() {
        let mut keys: Vec<String> = ACTIONS.iter().map(|(k, _)| format!("{k:?}")).collect();
        let before = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), before, "a transport key appears more than once");
    }
}
