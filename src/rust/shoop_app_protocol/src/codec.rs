//! Bounded app records with a distinct protocol discriminator and version.

use crate::{SessionSnapshot, WireMessage, MAX_SNAPSHOT_BYTES, VERSION};
use shoop_transport::{CodecError, MAX_RECORD_BYTES};

const HEADER_BYTES: usize = 8;
const MAGIC: [u8; 4] = *b"SAPP";

pub fn encode(message: &WireMessage, output: &mut [u8]) -> Result<usize, CodecError> {
    if !message.valid() {
        return Err(CodecError::Invalid);
    }
    let limit = output.len().min(MAX_RECORD_BYTES);
    if limit < HEADER_BYTES {
        return Err(CodecError::Overflow);
    }
    let used = bincode::serde::encode_into_slice(
        message,
        &mut output[HEADER_BYTES..limit],
        bincode::config::standard().with_limit::<MAX_RECORD_BYTES>(),
    )
    .map_err(|_| CodecError::Overflow)?;
    let length = HEADER_BYTES + used;
    output[..2].copy_from_slice(&(length as u16).to_le_bytes());
    output[2..6].copy_from_slice(&MAGIC);
    output[6..8].copy_from_slice(&VERSION.to_le_bytes());
    Ok(length)
}

pub fn decode(record: &[u8]) -> Result<WireMessage, CodecError> {
    if record.len() < HEADER_BYTES {
        return Err(CodecError::Truncated);
    }
    let length = usize::from(u16::from_le_bytes([record[0], record[1]]));
    if !(HEADER_BYTES..=MAX_RECORD_BYTES).contains(&length) {
        return Err(CodecError::Invalid);
    }
    if record.len() < length {
        return Err(CodecError::Truncated);
    }
    if record.len() != length {
        return Err(CodecError::Trailing);
    }
    if record[2..6] != MAGIC || u16::from_le_bytes([record[6], record[7]]) != VERSION {
        return Err(CodecError::Invalid);
    }
    let (message, consumed): (WireMessage, usize) = bincode::serde::decode_from_slice(
        &record[HEADER_BYTES..],
        bincode::config::standard().with_limit::<MAX_RECORD_BYTES>(),
    )
    .map_err(|_| CodecError::Invalid)?;
    if consumed != length - HEADER_BYTES {
        return Err(CodecError::Trailing);
    }
    if !message.valid() {
        return Err(CodecError::Invalid);
    }
    Ok(message)
}

struct SnapshotWriter(Vec<u8>);

impl bincode::enc::write::Writer for SnapshotWriter {
    fn write(&mut self, bytes: &[u8]) -> Result<(), bincode::error::EncodeError> {
        let end = self
            .0
            .len()
            .checked_add(bytes.len())
            .filter(|end| *end <= MAX_SNAPSHOT_BYTES)
            .ok_or(bincode::error::EncodeError::UnexpectedEnd)?;
        if end > self.0.capacity() {
            let capacity = end
                .max(self.0.capacity().max(2048) * 2)
                .min(MAX_SNAPSHOT_BYTES);
            self.0
                .try_reserve_exact(capacity - self.0.len())
                .map_err(|_| bincode::error::EncodeError::Other("snapshot allocation failed"))?;
        }
        self.0.extend_from_slice(bytes);
        Ok(())
    }
}

pub fn encode_snapshot(state: &SessionSnapshot) -> Result<Vec<u8>, CodecError> {
    if !state.valid() {
        return Err(CodecError::Invalid);
    }
    let mut output = SnapshotWriter(Vec::new());
    bincode::serde::encode_into_writer(
        state,
        &mut output,
        bincode::config::standard().with_limit::<MAX_SNAPSHOT_BYTES>(),
    )
    .map_err(|_| CodecError::Overflow)?;
    Ok(output.0)
}

pub fn decode_snapshot(bytes: &[u8]) -> Result<SessionSnapshot, CodecError> {
    if bytes.is_empty() || bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(CodecError::Invalid);
    }
    let (state, used): (SessionSnapshot, usize) = bincode::serde::decode_from_slice(
        bytes,
        bincode::config::standard().with_limit::<MAX_SNAPSHOT_BYTES>(),
    )
    .map_err(|_| CodecError::Invalid)?;
    if used != bytes.len() {
        return Err(CodecError::Trailing);
    }
    if !state.valid() {
        return Err(CodecError::Invalid);
    }
    Ok(state)
}
