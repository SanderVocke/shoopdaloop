use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExactMidi {
    pub sample_rate: u32,
    pub length_frames: u64,
    pub start_state: Vec<Vec<u8>>,
    pub events: Vec<ExactMidiEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ExactMidiEvent {
    pub frame: u64,
    pub order: u32,
    pub data: Vec<u8>,
}
