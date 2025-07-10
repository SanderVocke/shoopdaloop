use crate::ffi::shoop_midi_event_t;

#[derive(Clone, Debug)]
pub struct MidiEvent {
    pub time: i32,
    pub data: Vec<u8>,
}

impl MidiEvent {
    pub fn new(event: &shoop_midi_event_t) -> Self {
        MidiEvent {
            time: event.time,
            data: unsafe {
                std::slice::from_raw_parts(event.data, event.size as usize)
                    .iter()
                    .map(|x| *x)
                    .collect()
            },
        }
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }

    pub fn to_ffi(&self) -> shoop_midi_event_t {
        shoop_midi_event_t {
            time: self.time,
            data: self.data.as_ptr() as *mut u8,
            size: self.data.len() as u32,
        }
    }
}
