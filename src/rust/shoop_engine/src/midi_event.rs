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

    pub fn size(&self) -> usize {
        self.data.len()
    }
}
