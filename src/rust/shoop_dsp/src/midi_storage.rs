pub const MAX_MSG_BYTES: usize = 4;

/// One stored message. `time` is absolute within the storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MidiStorageElem {
    pub time: u32,
    size: u16,
    bytes: [u8; MAX_MSG_BYTES],
}

impl MidiStorageElem {
    /// `None` when the payload is empty or longer than [`MAX_MSG_BYTES`].
    pub fn new(time: u32, data: &[u8]) -> Option<Self> {
        if data.is_empty() || data.len() > MAX_MSG_BYTES {
            return None;
        }
        let mut bytes = [0u8; MAX_MSG_BYTES];
        bytes[..data.len()].copy_from_slice(data);
        Some(Self {
            time,
            size: data.len() as u16,
            bytes,
        })
    }

    /// Same payload, different time.
    pub fn at_time(mut self, time: u32) -> Self {
        self.time = time;
        self
    }

    pub fn data(&self) -> &[u8] {
        &self.bytes[..self.size as usize]
    }
    pub fn size(&self) -> usize {
        self.size as usize
    }
    /// Compares payloads only, ignoring time.
    pub fn contents_equal(&self, other: &Self) -> bool {
        self.data() == other.data()
    }
}
