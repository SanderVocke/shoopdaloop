use backend_bindings::MidiEvent;

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

    fn is_note_active(event: &MidiEvent, active_note_times: &mut Vec<Option<i32>>) -> bool {
        active_note_times[note_idx(event)].is_some()
    }

    fn terminate_note(
        start: i32,
        end: i32,
        channel: u8,
        note: u8,
        active_note_times: &mut Vec<Option<i32>>,
        notes: &mut Vec<Note>,
    ) {
        notes.push(Note {
            start_t: start,
            end_t: end,
            note: note,
            channel: channel,
        });
        active_note_times[channel as usize * 128 + note as usize] = None;
    }

    fn terminate_note_by_msg(
        event: &MidiEvent,
        active_note_times: &mut Vec<Option<i32>>,
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
        active_note_times: &mut Vec<Option<i32>>,
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
        } else if is_all_notes_off(&event.data) {
            terminate_channel_notes(
                channel(&event.data),
                event.time,
                &mut active_note_times,
                &mut notes,
            );
        } else if is_all_sound_off(&event.data) {
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
