//! Bounded persistence ingress. File codecs and engine resource ownership stay outside it.

use alloc::vec::Vec;

use crate::{Authority, Refusal, Revision, TransferId, CHUNK_BYTES, MAX_SNAPSHOT_BYTES};
use serde::{Deserialize, Serialize};
use shoop_app_api::LoopId;

pub const MAX_SESSION_MEDIA_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_SESSION_METADATA_BYTES: u64 = MAX_SNAPSHOT_BYTES as u64;
pub const MAX_SESSION_FILE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_SESSION_OBJECTS: u32 = 4096;
pub const MAX_SESSION_MIDI_EVENTS: u64 = 1_000_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Key {
    pub transfer: TransferId,
    pub epoch: u64,
}
impl Key {
    pub fn valid(self) -> bool {
        self.transfer.0 != 0 && self.epoch != 0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Target {
    Session {
        revision: Revision,
    },
    Loop {
        loop_id: LoopId,
        content_revision: u64,
    },
}
impl Target {
    pub fn valid(self) -> bool {
        match self {
            Self::Session { revision } => revision.0 != 0,
            Self::Loop { loop_id, .. } => loop_id.is_valid(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ObjectKind {
    Metadata,
    Audio,
    Midi,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Description {
    pub key: Key,
    pub target: Target,
    pub objects: u32,
    pub metadata_bytes: u64,
    pub media_bytes: u64,
}
impl Description {
    pub fn valid(self) -> bool {
        self.key.valid()
            && self.target.valid()
            && (1..=MAX_SESSION_OBJECTS).contains(&self.objects)
            && (1..=MAX_SESSION_METADATA_BYTES).contains(&self.metadata_bytes)
            && self.media_bytes <= MAX_SESSION_MEDIA_BYTES
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObjectDescription {
    pub index: u32,
    pub kind: ObjectKind,
    pub bytes: u64,
}
impl ObjectDescription {
    pub fn valid(self) -> bool {
        self.index < MAX_SESSION_OBJECTS
            && (self.kind != ObjectKind::Audio || self.bytes % 4 == 0)
            && self.bytes
                <= match self.kind {
                    ObjectKind::Metadata => MAX_SESSION_METADATA_BYTES,
                    _ => MAX_SESSION_MEDIA_BYTES,
                }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Request {
    Begin(Description),
    Object {
        key: Key,
        object: ObjectDescription,
    },
    Chunk {
        key: Key,
        object: u32,
        offset: u64,
        bytes: Vec<u8>,
    },
    FinishObject {
        key: Key,
        object: u32,
    },
    Finish {
        key: Key,
    },
    Cancel {
        key: Key,
    },
    Capture {
        key: Key,
        target: Target,
    },
    Read {
        key: Key,
        object: u32,
        offset: u64,
    },
    Describe {
        key: Key,
        object: u32,
    },
}
impl Request {
    pub fn key(&self) -> Key {
        match self {
            Self::Begin(description) => description.key,
            Self::Object { key, .. }
            | Self::Chunk { key, .. }
            | Self::FinishObject { key, .. }
            | Self::Finish { key }
            | Self::Cancel { key }
            | Self::Capture { key, .. }
            | Self::Read { key, .. } => *key,
            Self::Describe { key, .. } => *key,
        }
    }
    pub fn valid(&self) -> bool {
        self.key().valid()
            && match self {
                Self::Begin(description) => description.valid(),
                Self::Object { object, .. } => object.valid(),
                Self::Chunk {
                    object,
                    offset,
                    bytes,
                    ..
                } => {
                    *object < MAX_SESSION_OBJECTS
                        && !bytes.is_empty()
                        && bytes.len() <= CHUNK_BYTES
                        && offset
                            .checked_add(bytes.len() as u64)
                            .is_some_and(|end| end <= MAX_SESSION_MEDIA_BYTES)
                }
                Self::FinishObject { object, .. }
                | Self::Read { object, .. }
                | Self::Describe { object, .. } => {
                    *object < MAX_SESSION_OBJECTS
                        && !matches!(self, Self::Read { offset, .. } if *offset > MAX_SESSION_MEDIA_BYTES)
                }
                Self::Capture { target, .. } => target.valid(),
                _ => true,
            }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Reply {
    Begun {
        key: Key,
    },
    Object {
        key: Key,
        object: ObjectDescription,
    },
    Ack {
        key: Key,
        object: u32,
        next_offset: u64,
    },
    Ready {
        key: Key,
    },
    Captured(Description),
    Chunk {
        key: Key,
        object: u32,
        offset: u64,
        bytes: Vec<u8>,
    },
    Closed {
        key: Key,
    },
    Refused {
        key: Key,
        reason: Refusal,
    },
}
impl Reply {
    pub fn key(&self) -> Key {
        match self {
            Self::Captured(description) => description.key,
            Self::Begun { key }
            | Self::Object { key, .. }
            | Self::Ack { key, .. }
            | Self::Ready { key }
            | Self::Chunk { key, .. }
            | Self::Closed { key }
            | Self::Refused { key, .. } => *key,
        }
    }
    pub fn valid(&self) -> bool {
        self.key().valid()
            && match self {
                Self::Captured(description) => description.valid(),
                Self::Object { object, .. } => object.valid(),
                Self::Ack {
                    object,
                    next_offset,
                    ..
                } => *object < MAX_SESSION_OBJECTS && *next_offset <= MAX_SESSION_MEDIA_BYTES,
                Self::Chunk {
                    object,
                    offset,
                    bytes,
                    ..
                } => {
                    *object < MAX_SESSION_OBJECTS
                        && !bytes.is_empty()
                        && bytes.len() <= CHUNK_BYTES
                        && offset
                            .checked_add(bytes.len() as u64)
                            .is_some_and(|end| end <= MAX_SESSION_MEDIA_BYTES)
                }
                _ => true,
            }
    }
}

pub struct Object {
    pub description: ObjectDescription,
    pub bytes: Vec<u8>,
}

/// One caller-owned stage; admission must reserve its aggregate budget first.
pub struct Receiver {
    authority: Authority,
    description: Description,
    objects: Vec<Object>,
    active: Option<Object>,
    metadata: u64,
    media: u64,
    ready: bool,
    last_chunk: Option<(u64, usize)>,
}
impl Receiver {
    pub fn new(authority: Authority, description: Description) -> Result<Self, Refusal> {
        Self::with_limits(
            authority,
            description,
            shoop_app_api::HostResourceLimits::default(),
        )
    }
    pub fn with_limits(
        authority: Authority,
        description: Description,
        limits: shoop_app_api::HostResourceLimits,
    ) -> Result<Self, Refusal> {
        if !authority.is_valid() || !description.valid() {
            return Err(Refusal::Invalid);
        }
        if !limits.is_valid()
            || description.media_bytes > limits.media_bytes
            || description.metadata_bytes > limits.metadata_bytes
            || description.objects > limits.objects
        {
            return Err(Refusal::Overloaded);
        }
        Ok(Self {
            authority,
            description,
            objects: Vec::new(),
            active: None,
            metadata: 0,
            media: 0,
            ready: false,
            last_chunk: None,
        })
    }
    pub fn description(&self) -> Description {
        self.description
    }
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }
    pub fn into_objects(self) -> Result<Vec<Object>, Refusal> {
        if !self.ready {
            return Err(Refusal::NotReady);
        }
        Ok(self.objects)
    }
    pub fn ready(&self) -> bool {
        self.ready
    }
    pub fn check(&self, authority: Authority, key: Key) -> Result<(), Refusal> {
        if authority != self.authority {
            return Err(Refusal::StaleAuthority);
        }
        if key != self.description.key {
            return Err(Refusal::Identity);
        }
        Ok(())
    }
    pub fn apply(&mut self, authority: Authority, request: Request) -> Result<Reply, Refusal> {
        self.check(authority, request.key())?;
        if !request.valid() {
            return Err(Refusal::Invalid);
        }
        let key = self.description.key;
        match request {
            Request::Object { object, .. } => {
                self.begin_object(object)?;
                Ok(Reply::Object { key, object })
            }
            Request::Chunk {
                object,
                offset,
                bytes,
                ..
            } => {
                let next_offset = self.append(object, offset, &bytes)?;
                Ok(Reply::Ack {
                    key,
                    object,
                    next_offset,
                })
            }
            Request::FinishObject { object, .. } => {
                self.finish_object(object)?;
                Ok(Reply::Ack {
                    key,
                    object,
                    next_offset: 0,
                })
            }
            Request::Finish { .. } => {
                self.finish()?;
                Ok(Reply::Ready { key })
            }
            _ => Err(Refusal::Invalid),
        }
    }
    fn begin_object(&mut self, object: ObjectDescription) -> Result<(), Refusal> {
        if self.ready || self.active.is_some() {
            return Err(Refusal::Busy);
        }
        if !object.valid()
            || object.index as usize != self.objects.len()
            || object.index >= self.description.objects
            || (object.index == 0) != (object.kind == ObjectKind::Metadata)
        {
            return Err(Refusal::Invalid);
        }
        let (used, limit) = match object.kind {
            ObjectKind::Metadata => (self.metadata, self.description.metadata_bytes),
            _ => (self.media, self.description.media_bytes),
        };
        if object.bytes > limit.saturating_sub(used) {
            return Err(Refusal::Overloaded);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(object.bytes as usize)
            .map_err(|_| Refusal::Overloaded)?;
        self.active = Some(Object {
            description: object,
            bytes,
        });
        self.last_chunk = None;
        Ok(())
    }
    /// Exact retransmission of the last chunk is acknowledged without applying it twice.
    fn append(&mut self, object: u32, offset: u64, bytes: &[u8]) -> Result<u64, Refusal> {
        let active = self.active.as_mut().ok_or(Refusal::NotReady)?;
        if object != active.description.index || bytes.is_empty() || bytes.len() > CHUNK_BYTES {
            return Err(Refusal::Invalid);
        }
        let end = offset
            .checked_add(bytes.len() as u64)
            .ok_or(Refusal::Invalid)?;
        let received = active.bytes.len() as u64;
        if self.last_chunk == Some((offset, bytes.len()))
            && end == received
            && active.bytes.get(offset as usize..) == Some(bytes)
        {
            return Ok(received);
        }
        if offset != received || end > active.description.bytes {
            return Err(Refusal::Invalid);
        }
        active.bytes.extend_from_slice(bytes);
        self.last_chunk = Some((offset, bytes.len()));
        Ok(end)
    }
    fn finish_object(&mut self, object: u32) -> Result<(), Refusal> {
        let active = self.active.as_ref().ok_or(Refusal::NotReady)?;
        if object != active.description.index
            || active.bytes.len() as u64 != active.description.bytes
        {
            return Err(Refusal::Invalid);
        }
        self.objects
            .try_reserve(1)
            .map_err(|_| Refusal::Overloaded)?;
        let active = self.active.take().expect("checked active object");
        match active.description.kind {
            ObjectKind::Metadata => self.metadata += active.description.bytes,
            _ => self.media += active.description.bytes,
        }
        self.objects.push(active);
        Ok(())
    }
    fn finish(&mut self) -> Result<(), Refusal> {
        if self.active.is_some()
            || self.objects.len() != self.description.objects as usize
            || self.metadata != self.description.metadata_bytes
            || self.media != self.description.media_bytes
        {
            return Err(Refusal::Invalid);
        }
        self.ready = true;
        Ok(())
    }
}
