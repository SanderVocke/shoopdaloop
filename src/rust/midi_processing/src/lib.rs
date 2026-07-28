#![cfg(not(feature = "prebuild"))]

pub use shoop_engine::midi::MidiEvent;

pub struct Note {
    pub start_t: i32,
    pub end_t: i32,
    pub note: u8,
    pub channel: u8,
}

pub fn channel(msg_data: &[u8]) -> u8 {
    msg_data[0] & 0x0F
}

pub fn note(msg_data: &[u8]) -> u8 {
    msg_data[1]
}

pub fn velocity(msg_data: &[u8]) -> u8 {
    msg_data[2]
}

pub fn is_note_on(msg_data: &[u8]) -> bool {
    (msg_data[0] & 0xF0) == 0x90
}

pub fn make_note_on(channel: u8, note: u8, velocity: u8) -> Vec<u8> {
    vec![0x90 + channel, note, velocity]
}

pub fn is_note_off(msg_data: &[u8]) -> bool {
    (msg_data[0] & 0xF0) == 0x80
}

pub fn make_note_off(channel: u8, note: u8, velocity: u8) -> Vec<u8> {
    vec![0x80 + channel, note, velocity]
}

pub fn is_cc(msg_data: &[u8]) -> bool {
    (msg_data[0] & 0xF0) == 0xB0
}

pub fn is_all_notes_off(msg_data: &[u8]) -> bool {
    is_cc(msg_data) && msg_data[1] == 123
}

pub fn is_all_sound_off(msg_data: &[u8]) -> bool {
    is_cc(msg_data) && msg_data[1] == 120
}

pub fn msgs_to_notes(msgs: impl Iterator<Item = MidiEvent>) -> Vec<Note> {
    let mut active_note_times: Vec<Option<i32>> = Vec::default();
    active_note_times.resize(128 * 16, None); // Track all notes per channel
    let mut notes: Vec<Note> = Vec::default();

    fn note_idx(event: &MidiEvent) -> usize {
        (channel(&event.data) as usize) * 128 + note(&event.data) as usize
    }

    fn is_note_active(event: &MidiEvent, active_note_times: &mut [Option<i32>]) -> bool {
        active_note_times[note_idx(event)].is_some()
    }

    fn terminate_note(
        start: i32,
        end: i32,
        channel: u8,
        note: u8,
        active_note_times: &mut [Option<i32>],
        notes: &mut Vec<Note>,
    ) {
        notes.push(Note {
            start_t: start,
            end_t: end,
            note,
            channel,
        });
        active_note_times[channel as usize * 128 + note as usize] = None;
    }

    fn terminate_note_by_msg(
        event: &MidiEvent,
        active_note_times: &mut [Option<i32>],
        notes: &mut Vec<Note>,
    ) {
        terminate_note(
            active_note_times[note_idx(event)].unwrap_or(event.time),
            event.time,
            channel(&event.data),
            note(&event.data),
            active_note_times,
            notes,
        )
    }

    fn terminate_channel_notes(
        channel: u8,
        time: i32,
        active_note_times: &mut [Option<i32>],
        notes: &mut Vec<Note>,
    ) {
        for note in 0..128 {
            if let Some(t) = active_note_times[channel as usize * 128 + note as usize] {
                terminate_note(t, time, channel, note, active_note_times, notes)
            }
        }
    }

    for event in msgs {
        if is_note_on(&event.data) && !is_note_active(&event, &mut active_note_times) {
            active_note_times[note_idx(&event)] = Some(event.time);
        } else if is_note_off(&event.data) && is_note_active(&event, &mut active_note_times) {
            terminate_note_by_msg(&event, &mut active_note_times, &mut notes);
        } else if is_all_notes_off(&event.data) || is_all_sound_off(&event.data) {
            // Both end every sounding note on the channel; they differ only in what they
            // ask a synth to do with the tails, which is not modelled here.
            terminate_channel_notes(
                channel(&event.data),
                event.time,
                &mut active_note_times,
                &mut notes,
            );
        }
    }

    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    fn on(time: i32, channel: u8, note: u8) -> MidiEvent {
        MidiEvent::new(time, make_note_on(channel, note, 100))
    }
    fn off(time: i32, channel: u8, note: u8) -> MidiEvent {
        MidiEvent::new(time, make_note_off(channel, note, 64))
    }
    fn cc(time: i32, channel: u8, controller: u8) -> MidiEvent {
        MidiEvent::new(time, vec![0xB0 + channel, controller, 0])
    }

    fn as_tuples(notes: &[Note]) -> Vec<(i32, i32, u8, u8)> {
        notes
            .iter()
            .map(|n| (n.start_t, n.end_t, n.channel, n.note))
            .collect()
    }

    #[test]
    fn message_accessors_read_the_right_bytes() {
        let m = make_note_on(5, 60, 100);
        check!(is_note_on(&m));
        check!(!is_note_off(&m));
        check!(channel(&m) == 5);
        check!(note(&m) == 60);
        check!(velocity(&m) == 100);

        let m = make_note_off(2, 61, 64);
        check!(is_note_off(&m));
        check!(!is_note_on(&m));
        check!(channel(&m) == 2);
    }

    #[test]
    fn channel_mode_messages_are_recognised() {
        check!(is_all_notes_off(&[0xB0, 123, 0]));
        check!(is_all_sound_off(&[0xB0, 120, 0]));
        // Each is only itself, not the other.
        check!(!is_all_sound_off(&[0xB0, 123, 0]));
        check!(!is_all_notes_off(&[0xB0, 120, 0]));
        // And an ordinary controller is neither.
        check!(!is_all_notes_off(&[0xB0, 7, 0]));
    }

    #[test]
    fn a_note_pair_becomes_one_note() {
        let notes = msgs_to_notes([on(10, 0, 60), off(20, 0, 60)].into_iter());
        check!(as_tuples(&notes) == vec![(10, 20, 0, 60)]);
    }

    #[test]
    fn a_note_left_hanging_is_not_reported() {
        // No note-off, so there is no end time to give it.
        let notes = msgs_to_notes([on(10, 0, 60)].into_iter());
        check!(notes.is_empty());
    }

    #[test]
    fn notes_on_different_channels_do_not_interfere() {
        let notes =
            msgs_to_notes([on(0, 0, 60), on(1, 1, 60), off(10, 0, 60), off(20, 1, 60)].into_iter());
        check!(as_tuples(&notes) == vec![(0, 10, 0, 60), (1, 20, 1, 60)]);
    }

    #[test]
    fn a_repeated_note_on_does_not_restart_the_note() {
        // The second note-on for a sounding note is ignored, so the note keeps its
        // original start time.
        let notes = msgs_to_notes([on(10, 0, 60), on(15, 0, 60), off(20, 0, 60)].into_iter());
        check!(as_tuples(&notes) == vec![(10, 20, 0, 60)]);
    }

    #[test]
    fn a_note_off_for_nothing_sounding_is_ignored() {
        let notes = msgs_to_notes([off(10, 0, 60), on(20, 0, 60), off(30, 0, 60)].into_iter());
        check!(as_tuples(&notes) == vec![(20, 30, 0, 60)]);
    }

    #[test]
    fn all_notes_off_ends_every_note_on_its_channel() {
        let notes =
            msgs_to_notes([on(0, 0, 60), on(2, 0, 62), on(4, 1, 64), cc(10, 0, 123)].into_iter());
        // Both channel-0 notes end at 10; channel 1 is untouched and so goes unreported.
        check!(as_tuples(&notes) == vec![(0, 10, 0, 60), (2, 10, 0, 62)]);
    }

    #[test]
    fn all_sound_off_ends_every_note_on_its_channel() {
        let notes = msgs_to_notes([on(0, 3, 60), cc(8, 3, 120)].into_iter());
        check!(as_tuples(&notes) == vec![(0, 8, 3, 60)]);
    }

    #[test]
    fn a_midi_event_can_be_built_from_a_stored_one() {
        use shoop_engine::midi_storage::MidiStorageElem;
        let stored = MidiStorageElem::new(7, &[0x90, 60, 100]).expect("valid");
        let event = MidiEvent::from(&stored);
        check!(event.time == 7);
        check!(event.data == vec![0x90, 60, 100]);
    }
}
