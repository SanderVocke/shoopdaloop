//! Versioned live state for a separately retained graph configuration.
use crate::{LoopSlotId, StateRecord, Telemetry};
use core::num::NonZeroU16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotDelivery {
    Push { max_hz: NonZeroU16 },
    Pull,
}
impl SnapshotDelivery {
    pub fn push(hz: u16) -> Option<Self> {
        NonZeroU16::new(hz).map(|max_hz| Self::Push { max_hz })
    }
}
impl Default for SnapshotDelivery {
    fn default() -> Self {
        Self::Push {
            max_hz: NonZeroU16::new(60).unwrap(),
        }
    }
}

pub const VERSION: u32 = 1;
pub const HEADER: usize = 44;
pub const CHUNK_BYTES: usize = 1024;

/// Prepared worst-case encoding envelope, including fixed channel-window arrays.
pub fn capacity(strips: usize, loops: usize, buses: usize) -> Option<usize> {
    HEADER
        .checked_add(128)?
        .checked_add(strips.checked_mul(512)?)?
        .checked_add(loops.checked_mul(1024)?)?
        .checked_add(buses.checked_mul(256)?)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chunk {
    pub sequence: u32,
    pub total: u32,
    pub offset: u32,
    pub len: u16,
    pub bytes: [u8; CHUNK_BYTES],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Record {
    Telemetry(Telemetry),
    State(StateRecord),
    LoopLevels {
        id: LoopSlotId,
        gain: f32,
        balance: f32,
        repeat_sync: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Header {
    pub frame: u64,
    pub graph: u64,
    pub topology: u64,
    pub records: u32,
    pub loops: u32,
    pub strips: u32,
    pub buses: u32,
}
impl Header {
    pub fn write(self, bytes: &mut [u8]) {
        bytes[..4].copy_from_slice(&VERSION.to_le_bytes());
        bytes[4..12].copy_from_slice(&self.frame.to_le_bytes());
        bytes[12..20].copy_from_slice(&self.graph.to_le_bytes());
        bytes[20..28].copy_from_slice(&self.topology.to_le_bytes());
        bytes[28..32].copy_from_slice(&self.records.to_le_bytes());
        bytes[32..36].copy_from_slice(&self.loops.to_le_bytes());
        bytes[36..40].copy_from_slice(&self.strips.to_le_bytes());
        bytes[40..44].copy_from_slice(&self.buses.to_le_bytes());
    }
    pub fn read(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < HEADER || u32::from_le_bytes(bytes[..4].try_into().ok()?) != VERSION {
            return None;
        }
        Some(Self {
            frame: u64::from_le_bytes(bytes[4..12].try_into().ok()?),
            graph: u64::from_le_bytes(bytes[12..20].try_into().ok()?),
            topology: u64::from_le_bytes(bytes[20..28].try_into().ok()?),
            records: u32::from_le_bytes(bytes[28..32].try_into().ok()?),
            loops: u32::from_le_bytes(bytes[32..36].try_into().ok()?),
            strips: u32::from_le_bytes(bytes[36..40].try_into().ok()?),
            buses: u32::from_le_bytes(bytes[40..44].try_into().ok()?),
        })
    }
}

pub struct Records<'a>(pub &'a [u8]);
impl Iterator for Records<'_> {
    type Item = Result<Record, crate::codec::CodecError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            return None;
        }
        let size = self
            .0
            .get(..2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
        let Some(size) = size.filter(|&n| n >= 2 && n <= self.0.len()) else {
            self.0 = &[];
            return Some(Err(crate::codec::CodecError::Invalid));
        };
        let (record, rest) = self.0.split_at(size);
        self.0 = rest;
        Some(crate::codec::decode_snapshot(record))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{codec, ClientMessage, EngineMessage, MAX_RECORD_BYTES};
    #[shoop_wasm_test_support::shoop_test]
    fn selection_configuration_and_full_chunks_roundtrip() {
        let mut bytes = [0; MAX_RECORD_BYTES];
        for client in [
            ClientMessage::ObserveSnapshots(true),
            ClientMessage::ConfigureSnapshots(SnapshotDelivery::Pull),
            ClientMessage::ConfigureSnapshots(SnapshotDelivery::push(120).unwrap()),
            ClientMessage::RequestSnapshot { after: 12 },
        ] {
            let len = codec::encode_client(&client, &mut bytes).unwrap();
            assert_eq!(codec::decode_client(&bytes[..len]).unwrap(), client);
        }
        assert!(SnapshotDelivery::push(0).is_none());
        for message in [
            EngineMessage::SnapshotReady { sequence: 12 },
            EngineMessage::SnapshotReply(Chunk {
                sequence: 12,
                total: 4096,
                offset: 1024,
                len: CHUNK_BYTES as u16,
                bytes: [43; CHUNK_BYTES],
            }),
            EngineMessage::SnapshotConfiguration {
                graph: 7,
                topology: 9,
            },
            EngineMessage::Snapshot(Chunk {
                sequence: 12,
                total: 4096,
                offset: 1024,
                len: CHUNK_BYTES as u16,
                bytes: [42; CHUNK_BYTES],
            }),
        ] {
            let len = codec::encode_engine(&message, &mut bytes).unwrap();
            assert_eq!(codec::decode_engine(&bytes[..len]).unwrap(), message);
        }
    }
}
