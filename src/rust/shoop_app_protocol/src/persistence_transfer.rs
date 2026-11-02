//! Segmented ownership-preserving adapters for transports that require bytes.
use alloc::vec;
use alloc::vec::Vec;

use crate::{codec, persistence::*, persistence_image::*, Refusal, CHUNK_BYTES};
use alloc::sync::Arc;

pub struct Source {
    image: Arc<Image>,
    metadata: Vec<u8>,
    midi_offsets: Vec<Vec<u64>>,
    media_bytes: u64,
}
impl Source {
    pub fn image(&self) -> &Arc<Image> {
        &self.image
    }
    pub fn new(image: Arc<Image>) -> Result<Self, Refusal> {
        let mut validation = Validation::for_transfer(image)?;
        while !validation.step(64 * 1024)? {}
        Self::from_validated(validation.finish()?)
    }
    pub fn from_validated(validated: ValidatedImage) -> Result<Self, Refusal> {
        let ValidatedImage {
            image,
            media_bytes,
            midi_offsets,
        } = validated;
        let Some(midi_offsets) = midi_offsets else {
            return Self::new(image);
        };
        let metadata = codec::encode_manifest(&image.manifest).map_err(|_| Refusal::Invalid)?;
        Ok(Self {
            image,
            metadata,
            midi_offsets,
            media_bytes,
        })
    }
    pub fn from_validated_with_limits(
        validated: ValidatedImage,
        limits: shoop_app_api::HostResourceLimits,
    ) -> Result<Self, Refusal> {
        if !limits.is_valid() {
            return Err(Refusal::Invalid);
        }
        let image = validated.image();
        let events = image
            .payloads
            .iter()
            .try_fold(0usize, |total, payload| {
                total.checked_add(match payload {
                    Payload::Midi(events) => events.len(),
                    _ => 0,
                })
            })
            .ok_or(Refusal::Overloaded)?;
        if validated.media_bytes() > limits.media_bytes
            || image.payloads.len() >= limits.objects as usize
            || events > limits.midi_events
        {
            return Err(Refusal::Overloaded);
        }
        let source = Self::from_validated(validated)?;
        if source.metadata.len() as u64 > limits.metadata_bytes {
            return Err(Refusal::Overloaded);
        }
        Ok(source)
    }
    pub fn description(&self, key: Key, target: Target) -> Description {
        Description {
            key,
            target,
            objects: self.image.payloads.len() as u32 + 1,
            metadata_bytes: self.metadata.len() as u64,
            media_bytes: self.media_bytes,
        }
    }
    pub fn object(&self, object: u32) -> Result<ObjectDescription, Refusal> {
        let (kind, bytes) = if object == 0 {
            (ObjectKind::Metadata, self.metadata.len() as u64)
        } else {
            match self
                .image
                .payloads
                .get(object as usize - 1)
                .ok_or(Refusal::Identity)?
            {
                Payload::Audio(samples) => (ObjectKind::Audio, samples.len() as u64 * 4),
                Payload::Midi(_) => (
                    ObjectKind::Midi,
                    *self.midi_offsets[object as usize - 1]
                        .last()
                        .expect("MIDI prefix index"),
                ),
            }
        };
        Ok(ObjectDescription {
            index: object,
            kind,
            bytes,
        })
    }
    pub fn chunk(&self, object: u32, offset: u64) -> Result<Vec<u8>, Refusal> {
        let description = self.object(object)?;
        if offset >= description.bytes {
            return Err(Refusal::Invalid);
        }
        let length = (description.bytes - offset).min(CHUNK_BYTES as u64) as usize;
        let mut bytes = vec![0; length];
        if object == 0 {
            bytes.copy_from_slice(&self.metadata[offset as usize..offset as usize + length]);
        } else {
            match &self.image.payloads[object as usize - 1] {
                Payload::Audio(samples) => {
                    for (index, byte) in bytes.iter_mut().enumerate() {
                        let position = offset as usize + index;
                        *byte = samples[position / 4].to_le_bytes()[position % 4];
                    }
                }
                Payload::Midi(events) => {
                    let offsets = &self.midi_offsets[object as usize - 1];
                    let mut event = offsets.partition_point(|value| *value <= offset) - 1;
                    let mut position = offset;
                    for byte in &mut bytes {
                        let row = &events[event];
                        let index = (position - offsets[event]) as usize;
                        *byte = match index {
                            0..=7 => u64::from(row.frame).to_le_bytes()[index],
                            8..=11 => (row.data.len() as u32).to_le_bytes()[index - 8],
                            _ => row.data[index - 12],
                        };
                        position += 1;
                        if position == offsets[event + 1] {
                            event += 1;
                        }
                    }
                }
            }
        }
        Ok(bytes)
    }
}

mod assembly;
pub use assembly::Assembly;

pub fn assemble(objects: Vec<Object>) -> Result<Arc<Image>, Refusal> {
    let mut assembly = Assembly::new(objects)?;
    loop {
        if let Some(validated) = assembly.step(64 * 1024)? {
            return Ok(validated.into_image());
        }
    }
}

pub struct UploadStream {
    source: Source,
    description: Description,
    phase: u8,
    object: u32,
    offset: u64,
}
impl UploadStream {
    pub fn description(&self) -> Description {
        self.description
    }
    pub fn new(image: Arc<Image>, key: Key, target: Target) -> Result<Self, Refusal> {
        let source = Source::new(image)?;
        Self::from_source(source, key, target)
    }
    pub fn from_validated(
        image: ValidatedImage,
        key: Key,
        target: Target,
    ) -> Result<Self, Refusal> {
        Self::from_source(Source::from_validated(image)?, key, target)
    }
    fn from_source(source: Source, key: Key, target: Target) -> Result<Self, Refusal> {
        let description = source.description(key, target);
        if !description.valid() {
            return Err(Refusal::Invalid);
        }
        Ok(Self {
            source,
            description,
            phase: 0,
            object: 0,
            offset: 0,
        })
    }
    pub fn next_request(&mut self) -> Result<Option<Request>, Refusal> {
        let key = self.description.key;
        let object = self.object;
        Ok(Some(match self.phase {
            0 => {
                self.phase = 1;
                Request::Begin(self.description)
            }
            1 => {
                self.phase = 2;
                Request::Object {
                    key,
                    object: self.source.object(object)?,
                }
            }
            2 => {
                let description = self.source.object(object)?;
                if self.offset == description.bytes {
                    self.phase = 3;
                    return self.next_request();
                }
                let offset = self.offset;
                let bytes = self.source.chunk(object, offset)?;
                self.offset += bytes.len() as u64;
                Request::Chunk {
                    key,
                    object,
                    offset,
                    bytes,
                }
            }
            3 => {
                self.object += 1;
                self.offset = 0;
                self.phase = if self.object == self.description.objects {
                    4
                } else {
                    1
                };
                Request::FinishObject { key, object }
            }
            4 => {
                self.phase = 5;
                Request::Finish { key }
            }
            _ => return Ok(None),
        }))
    }
}
