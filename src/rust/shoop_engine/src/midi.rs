//! MIDI message inspection and construction.

/// Number of MIDI channels.
pub const N_CHANNELS: usize = 16;
/// Number of note / controller numbers per channel.
pub const N_NOTES: usize = 128;

pub fn channel(data: &[u8]) -> u8 {
    data[0] & 0x0F
}
pub fn status(data: &[u8]) -> u8 {
    data[0] & 0xF0
}
pub fn note(data: &[u8]) -> u8 {
    data[1]
}
pub fn velocity(data: &[u8]) -> u8 {
    data[2]
}

/// A note-on with zero velocity is a note-off, per the MIDI spec.
pub fn is_note_on(data: &[u8]) -> bool {
    data.len() >= 3 && status(data) == 0x90 && data[2] != 0
}
pub fn is_note_off(data: &[u8]) -> bool {
    data.len() >= 3 && (status(data) == 0x80 || (status(data) == 0x90 && data[2] == 0))
}
pub fn is_cc(data: &[u8]) -> bool {
    data.len() >= 3 && status(data) == 0xB0
}
pub fn is_program(data: &[u8]) -> bool {
    data.len() >= 2 && status(data) == 0xC0
}
pub fn is_channel_pressure(data: &[u8]) -> bool {
    data.len() >= 2 && status(data) == 0xD0
}
pub fn is_pitch_wheel(data: &[u8]) -> bool {
    data.len() >= 3 && status(data) == 0xE0
}

/// Channel addressed by an "all notes off" control change, if this is one.
pub fn all_notes_off_channel(data: &[u8]) -> Option<u8> {
    (is_cc(data) && data[1] == 123).then(|| channel(data))
}
/// Channel addressed by an "all sound off" control change, if this is one.
pub fn all_sound_off_channel(data: &[u8]) -> Option<u8> {
    (is_cc(data) && data[1] == 120).then(|| channel(data))
}

pub fn note_on(ch: u8, note: u8, velocity: u8) -> [u8; 3] {
    [0x90 | (ch & 0x0F), note, velocity]
}
pub fn note_off(ch: u8, note: u8, velocity: u8) -> [u8; 3] {
    [0x80 | (ch & 0x0F), note, velocity]
}
pub fn cc(ch: u8, controller: u8, value: u8) -> [u8; 3] {
    [0xB0 | (ch & 0x0F), controller, value]
}
pub fn program_change(ch: u8, program: u8) -> [u8; 2] {
    [0xC0 | (ch & 0x0F), program]
}
pub fn channel_pressure(ch: u8, value: u8) -> [u8; 2] {
    [0xD0 | (ch & 0x0F), value]
}
/// 14-bit pitch wheel, LSB first.
pub fn pitch_wheel(ch: u8, value: u16) -> [u8; 3] {
    [
        0xE0 | (ch & 0x0F),
        (value & 0x7F) as u8,
        ((value >> 7) & 0x7F) as u8,
    ]
}
pub fn all_sound_off(ch: u8) -> [u8; 3] {
    cc(ch, 120, 0)
}

/// All Notes Off. Lets sounding notes release; All Sound Off cuts them.
pub fn all_notes_off(ch: u8) -> [u8; 3] {
    cc(ch, 123, 0)
}

/// Expected length of a message from its status byte.
pub fn message_len(status_byte: u8) -> usize {
    match status_byte & 0xF0 {
        0xC0 | 0xD0 => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    #[shoop_wasm_test_support::shoop_test]
    fn note_on_with_zero_velocity_is_a_note_off() {
        check!(is_note_on(&note_on(0, 60, 100)));
        check!(!is_note_off(&note_on(0, 60, 100)));

        let zero_vel = note_on(0, 60, 0);
        check!(!is_note_on(&zero_vel));
        check!(is_note_off(&zero_vel));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn classifies_message_types() {
        check!(is_note_off(&note_off(1, 60, 0)));
        check!(is_cc(&cc(2, 7, 100)));
        check!(is_program(&program_change(3, 5)));
        check!(is_channel_pressure(&channel_pressure(4, 64)));
        check!(is_pitch_wheel(&pitch_wheel(5, 8192)));
        check!(!is_cc(&note_on(0, 60, 1)));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn extracts_channel_and_note() {
        let m = note_on(7, 64, 99);
        check!(channel(&m) == 7);
        check!(note(&m) == 64);
        check!(velocity(&m) == 99);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn channel_is_masked_to_four_bits() {
        check!(channel(&note_on(0xFF, 1, 1)) == 0x0F);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pitch_wheel_is_lsb_first() {
        let m = pitch_wheel(0, 8192);
        check!(m[1] == 0x00);
        check!(m[2] == 0x40);
        // Round-trips through the 7-bit halves.
        let recombined = (m[1] as u16) | ((m[2] as u16) << 7);
        check!(recombined == 8192);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn recognises_channel_mode_messages() {
        check!(all_notes_off_channel(&cc(3, 123, 0)) == Some(3));
        check!(all_notes_off_channel(&cc(3, 7, 0)) == None);
        check!(all_sound_off_channel(&cc(9, 120, 0)) == Some(9));
        check!(all_sound_off_channel(&note_on(0, 1, 1)) == None);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn message_len_from_status() {
        check!(message_len(0x90) == 3);
        check!(message_len(0xB0) == 3);
        check!(message_len(0xE0) == 3);
        check!(message_len(0xC0) == 2);
        check!(message_len(0xD0) == 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn short_messages_are_not_misclassified() {
        // A truncated buffer must not be read past its end.
        check!(!is_note_on(&[0x90]));
        check!(!is_note_on(&[0x90, 60]));
        check!(!is_cc(&[0xB0, 7]));
        check!(!is_pitch_wheel(&[0xE0, 0]));
        check!(is_program(&[0xC0, 5]));
    }
}

/// A MIDI message with an owned payload, for the control path.
///
/// Distinct from [`crate::midi_storage::MidiStorageElem`], which is fixed-size so the
/// audio thread can hold it without allocating. This one owns its bytes and carries a
/// signed time, because the callers that want it -- drawing notes, editing a recording
/// -- work with times relative to a position that can be negative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiEvent {
    pub time: i32,
    pub data: Vec<u8>,
}

impl MidiEvent {
    pub fn new(time: i32, data: impl Into<Vec<u8>>) -> Self {
        Self {
            time,
            data: data.into(),
        }
    }
}

impl From<&crate::midi_storage::MidiStorageElem> for MidiEvent {
    fn from(e: &crate::midi_storage::MidiStorageElem) -> Self {
        Self {
            time: e.time as i32,
            data: e.data().to_vec(),
        }
    }
}
