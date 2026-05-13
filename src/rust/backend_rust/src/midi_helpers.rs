pub fn channel(data: &[u8]) -> u32 {
    (data[0] & 0x0F) as u32
}

pub fn note(data: &[u8]) -> u32 {
    data[1] as u32
}

pub fn velocity(data: &[u8]) -> u32 {
    data[2] as u32
}

pub fn is_note_on(data: &[u8]) -> bool {
    data[0] & 0xF0 == 0x90 && data[2] != 0
}

pub fn is_note_off(data: &[u8]) -> bool {
    data[0] & 0xF0 == 0x80 || (data[0] & 0xF0 == 0x90 && data[2] == 0)
}

pub fn is_cc(data: &[u8]) -> bool {
    data[0] & 0xF0 == 0xB0
}

pub fn is_program(data: &[u8]) -> bool {
    data[0] & 0xF0 == 0xC0
}

pub fn is_channel_pressure(data: &[u8]) -> bool {
    data[0] & 0xF0 == 0xD0
}

pub fn is_pitch_wheel(data: &[u8]) -> bool {
    data[0] & 0xF0 == 0xE0
}

pub fn is_all_notes_off_for_channel(data: &[u8]) -> Option<u32> {
    if is_cc(data) && data[1] == 123 {
        Some(channel(data))
    } else {
        None
    }
}

pub fn is_all_sound_off_for_channel(data: &[u8]) -> Option<u32> {
    if is_cc(data) && data[1] == 120 {
        Some(channel(data))
    } else {
        None
    }
}

pub fn note_on(channel: u8, note: u8, velocity: u8) -> Vec<u8> {
    vec![0x90 | channel, note, velocity]
}

pub fn note_off(channel: u8, note: u8, velocity: u8) -> Vec<u8> {
    vec![0x80 | channel, note, velocity]
}

pub fn cc(channel: u8, controller: u8, value: u8) -> Vec<u8> {
    vec![0xB0 | channel, controller, value]
}

pub fn program_change(channel: u8, program: u8) -> Vec<u8> {
    vec![0xC0 | channel, program]
}

pub fn pitch_wheel_change(channel: u8, value: u16) -> Vec<u8> {
    vec![
        0xE0 | channel,
        (value & 0x7F) as u8,
        ((value >> 7) & 0x7F) as u8,
    ]
}

pub fn channel_pressure(channel: u8, value: u8) -> Vec<u8> {
    vec![0xD0 | channel, value]
}

pub fn midi_message_len(status: u8) -> usize {
    match status & 0xF0 {
        0xC0 | 0xD0 => 2,
        _ => 3,
    }
}
