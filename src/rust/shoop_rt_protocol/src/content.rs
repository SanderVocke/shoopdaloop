//! Content preparation tokens and bounded, allocation-free upload records.

pub const MAX_CONTENT_BYTES: u64 = 256 * 1024 * 1024;
pub const CONTENT_CHUNK_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentDescription {
    pub id: u64,
    pub midi: bool,
    pub frames: u64,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentChunk {
    len: u16,
    bytes: [u8; CONTENT_CHUNK_BYTES],
}
impl ContentChunk {
    pub fn new(data: &[u8]) -> Option<Self> {
        if data.is_empty() || data.len() > CONTENT_CHUNK_BYTES {
            return None;
        }
        let mut bytes = [0; CONTENT_CHUNK_BYTES];
        bytes[..data.len()].copy_from_slice(data);
        Some(Self {
            len: data.len() as u16,
            bytes,
        })
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}
