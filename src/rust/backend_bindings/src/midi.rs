pub struct MidiEvent {
    pub time: i32,
    pub data: Vec<u8>,
}

impl MidiEvent {
    pub fn new(time: i32, data: Vec<u8>) -> Self {
        MidiEvent { time, data }
    }
}
