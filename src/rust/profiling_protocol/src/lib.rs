//! Bounded, payload-independent diagnostic streams and producer controls.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

pub mod memory;
pub use memory::{MemoryReport, MemoryResource, MemoryUnit, MAX_MEMORY_RESOURCES};

pub const CHUNK_BYTES: usize = 1024;
pub const MAX_REPORT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_SOURCES: usize = 8;
pub const MAX_CONTROL_BYTES: usize = 128;
pub const MAX_RECORD_BYTES: usize = 1200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bytes<const N: usize> {
    bytes: [u8; N],
    len: usize,
}
impl<const N: usize> Default for Bytes<N> {
    fn default() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }
}
impl<const N: usize> Bytes<N> {
    pub fn new(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > N {
            return None;
        }
        let mut result = Self::default();
        result.bytes[..bytes.len()].copy_from_slice(bytes);
        result.len = bytes.len();
        Some(result)
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
impl<const N: usize> Serialize for Bytes<N> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.as_slice())
    }
}
impl<'de, const N: usize> Deserialize<'de> for Bytes<N> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<const N: usize>;
        impl<'de, const N: usize> serde::de::Visitor<'de> for Visitor<N> {
            type Value = Bytes<N>;
            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "at most {N} bytes")
            }
            fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
                Bytes::new(bytes).ok_or_else(|| E::custom("diagnostic payload too large"))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut result = Bytes::default();
                while let Some(byte) = seq.next_element::<u8>()? {
                    if result.len == N {
                        return Err(serde::de::Error::custom("diagnostic payload too large"));
                    }
                    result.bytes[result.len] = byte;
                    result.len += 1;
                }
                Ok(result)
            }
        }
        deserializer.deserialize_bytes(Visitor::<N>)
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct Name(Bytes<64>);
impl Name {
    pub fn new(text: &str) -> Option<Self> {
        Bytes::new(text.as_bytes()).map(Self)
    }
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(self.0.as_slice()).unwrap_or("")
    }
    pub fn valid(&self) -> bool {
        core::str::from_utf8(self.0.as_slice()).is_ok()
    }
}

impl core::fmt::Debug for Name {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self.as_str(), f)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct StreamId {
    pub source: u32,
    pub epoch: u64,
    pub report_type: u32,
    pub schema: u16,
}
impl StreamId {
    pub fn valid(self) -> bool {
        self.source != 0 && self.epoch != 0 && self.report_type != 0 && self.schema != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub stream: StreamId,
    pub sequence: u64,
    pub offset: u32,
    pub total: u32,
    pub payload: Bytes<CHUNK_BYTES>,
}
impl Chunk {
    pub fn valid(&self) -> bool {
        self.stream.valid()
            && self.sequence != 0
            && self.total != 0
            && self.total as usize <= MAX_REPORT_BYTES
            && !self.payload.as_slice().is_empty()
            && (self.offset as usize)
                .checked_add(self.payload.as_slice().len())
                .is_some_and(|end| end <= self.total as usize)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Descriptor {
    pub stream: StreamId,
    pub name: Name,
    pub available: bool,
    pub min_interval_ms: u32,
    pub max_interval_ms: u32,
    pub snapshot: bool,
    pub inspect: bool,
}
impl Descriptor {
    pub fn valid(&self) -> bool {
        self.stream.valid()
            && self.name.valid()
            && self.min_interval_ms != 0
            && self.min_interval_ms <= self.max_interval_ms
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Outcome {
    Applied,
    Unsupported,
    Unavailable,
    Capacity,
    Invalid,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub stream: StreamId,
    pub requested: bool,
    pub active: bool,
    pub interval_ms: u32,
    pub outcome: Outcome,
    pub dropped: u64,
    pub coalesced: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Record {
    Descriptor(Descriptor),
    Status(Status),
    Data(Chunk),
}
impl Record {
    pub fn source(&self) -> u32 {
        self.stream().source
    }
    pub fn stream(&self) -> StreamId {
        match self {
            Self::Descriptor(d) => d.stream,
            Self::Status(s) => s.stream,
            Self::Data(c) => c.stream,
        }
    }
    pub fn valid(&self) -> bool {
        match self {
            Self::Descriptor(d) => d.valid(),
            Self::Status(s) => s.stream.valid() && s.interval_ms != 0,
            Self::Data(c) => c.valid(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Discover,
    Subscribe { enabled: bool, interval_ms: u32 },
    Snapshot,
    Inspect(Bytes<MAX_CONTROL_BYTES>),
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub source: u32,
    pub epoch: u64,
    pub action: Action,
}
impl Request {
    pub fn valid(&self) -> bool {
        matches!(self.action, Action::Discover)
            || (self.source != 0
                && self.epoch != 0
                && !matches!(self.action, Action::Subscribe { interval_ms: 0, .. }))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Invalid,
    Capacity,
    Stale,
    Gap,
}

pub fn encode<T: Serialize>(value: &T, output: &mut [u8]) -> Result<usize, Error> {
    bincode::serde::encode_into_slice(
        value,
        output,
        bincode::config::standard().with_limit::<MAX_REPORT_BYTES>(),
    )
    .map_err(|_| Error::Capacity)
}
pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    if bytes.is_empty() || bytes.len() > MAX_REPORT_BYTES {
        return Err(Error::Invalid);
    }
    let (value, used) = bincode::serde::decode_from_slice(
        bytes,
        bincode::config::standard().with_limit::<MAX_REPORT_BYTES>(),
    )
    .map_err(|_| Error::Invalid)?;
    if used != bytes.len() {
        return Err(Error::Invalid);
    }
    Ok(value)
}

/// A complete immutable report. Only its next bounded fragment is copied.
pub struct Transfer {
    stream: StreamId,
    sequence: u64,
    bytes: Vec<u8>,
    offset: usize,
}
impl Transfer {
    pub fn new(stream: StreamId, sequence: u64, bytes: Vec<u8>) -> Result<Self, Error> {
        if !stream.valid()
            || sequence == 0
            || bytes.is_empty()
            || bytes.capacity() > MAX_REPORT_BYTES
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            stream,
            sequence,
            bytes,
            offset: 0,
        })
    }
    pub fn next_chunk(&mut self) -> Option<Chunk> {
        if self.offset == self.bytes.len() {
            return None;
        }
        let end = (self.offset + CHUNK_BYTES).min(self.bytes.len());
        let chunk = Chunk {
            stream: self.stream,
            sequence: self.sequence,
            offset: self.offset as u32,
            total: self.bytes.len() as u32,
            payload: Bytes::new(&self.bytes[self.offset..end]).expect("bounded fragment"),
        };
        self.offset = end;
        Some(chunk)
    }
    pub fn stream(&self) -> StreamId {
        self.stream
    }
    pub fn bytes(&self) -> usize {
        self.bytes.capacity()
    }
}

#[derive(Default)]
pub struct Assembly {
    stream: Option<StreamId>,
    sequence: u64,
    total: usize,
    bytes: Vec<u8>,
    complete: bool,
    pub lost: u64,
}
impl Assembly {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn accept(&mut self, chunk: &Chunk) -> Result<Option<&[u8]>, Error> {
        if !chunk.valid() {
            return Err(Error::Invalid);
        }
        if let Some(stream) = self.stream {
            if stream.source != chunk.stream.source {
                return Err(Error::Invalid);
            }
            if chunk.stream.epoch < stream.epoch
                || (chunk.stream.epoch == stream.epoch && chunk.sequence < self.sequence)
                || (chunk.stream == stream && chunk.sequence == self.sequence && self.complete)
            {
                return Err(Error::Stale);
            }
        }
        if chunk.offset == 0
            && self.stream == Some(chunk.stream)
            && self.sequence == chunk.sequence
            && !self.bytes.is_empty()
        {
            return Err(Error::Stale);
        }
        if chunk.offset == 0 {
            if !self.complete && !self.bytes.is_empty() {
                self.lost = self.lost.saturating_add(1);
            }
            self.bytes.clear();
            self.stream = Some(chunk.stream);
            self.sequence = chunk.sequence;
            self.total = chunk.total as usize;
            self.complete = false;
        }
        if self.stream != Some(chunk.stream)
            || self.sequence != chunk.sequence
            || self.total != chunk.total as usize
            || self.bytes.len() != chunk.offset as usize
            || self.complete
        {
            return Err(Error::Gap);
        }
        self.bytes
            .try_reserve_exact(chunk.payload.as_slice().len())
            .map_err(|_| Error::Capacity)?;
        self.bytes.extend_from_slice(chunk.payload.as_slice());
        self.complete = self.bytes.len() == self.total;
        Ok(self.complete.then_some(self.bytes.as_slice()))
    }
    pub fn bytes(&self) -> usize {
        self.bytes.capacity()
    }
    pub fn release(&mut self) {
        if self.complete {
            self.bytes = Vec::new();
        }
    }
}

pub fn encode_report<T: Serialize>(value: &T) -> Result<Vec<u8>, Error> {
    struct Writer(Vec<u8>);
    impl bincode::enc::write::Writer for Writer {
        fn write(&mut self, bytes: &[u8]) -> Result<(), bincode::error::EncodeError> {
            if self
                .0
                .len()
                .checked_add(bytes.len())
                .is_none_or(|end| end > MAX_REPORT_BYTES)
            {
                return Err(bincode::error::EncodeError::UnexpectedEnd);
            }
            let end = self.0.len() + bytes.len();
            if end > self.0.capacity() {
                self.0
                    .try_reserve_exact(
                        end.max(self.0.capacity().max(2048).saturating_mul(2))
                            .min(MAX_REPORT_BYTES)
                            - self.0.len(),
                    )
                    .map_err(|_| bincode::error::EncodeError::Other("diagnostic capacity"))?;
            }
            self.0.extend_from_slice(bytes);
            Ok(())
        }
    }
    let mut writer = Writer(Vec::new());
    bincode::serde::encode_into_writer(
        value,
        &mut writer,
        bincode::config::standard().with_limit::<MAX_REPORT_BYTES>(),
    )
    .map_err(|_| Error::Capacity)?;
    Ok(writer.0)
}

pub mod broker;
