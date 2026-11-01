//! Segmented ownership-preserving adapters for transports that require bytes.
use crate::{codec, persistence::*, persistence_image::*, Refusal, CHUNK_BYTES};
use shoop_app_api::MidiEventState;
use std::sync::Arc;

pub struct Source {
    pub image: Arc<Image>,
    metadata: Vec<u8>,
    midi_offsets: Vec<Vec<u64>>,
    media_bytes: u64,
}
impl Source {
    pub fn new(image: Arc<Image>) -> Result<Self, Refusal> {
        let media_bytes = image.validate()?;
        let metadata = codec::encode_manifest(&image.manifest).map_err(|_| Refusal::Invalid)?;
        let mut midi_offsets = Vec::new();
        midi_offsets
            .try_reserve_exact(image.payloads.len())
            .map_err(|_| Refusal::Overloaded)?;
        for payload in &image.payloads {
            let mut offsets = Vec::new();
            if let Payload::Midi(events) = payload {
                offsets
                    .try_reserve_exact(events.len() + 1)
                    .map_err(|_| Refusal::Overloaded)?;
                offsets.push(0);
                for event in events.iter() {
                    offsets.push(offsets.last().copied().unwrap() + 12 + event.data.len() as u64);
                }
            }
            midi_offsets.push(offsets);
        }
        Ok(Self {
            image,
            metadata,
            midi_offsets,
            media_bytes,
        })
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

pub fn assemble(objects: Vec<Object>) -> Result<Arc<Image>, Refusal> {
    let mut objects = objects.into_iter();
    let metadata = objects.next().ok_or(Refusal::Invalid)?;
    if metadata.description.kind != ObjectKind::Metadata || metadata.description.index != 0 {
        return Err(Refusal::Invalid);
    }
    let manifest = codec::decode_manifest(&metadata.bytes).map_err(|_| Refusal::Invalid)?;
    let mut payloads = Vec::new();
    for object in objects {
        if object.description.index as usize != payloads.len() + 1
            || object.description.bytes != object.bytes.len() as u64
        {
            return Err(Refusal::Invalid);
        }
        payloads.try_reserve(1).map_err(|_| Refusal::Overloaded)?;
        payloads.push(match object.description.kind {
            ObjectKind::Audio => {
                if object.bytes.len() % 4 != 0 {
                    return Err(Refusal::Invalid);
                }
                let mut samples = Vec::new();
                samples
                    .try_reserve_exact(object.bytes.len() / 4)
                    .map_err(|_| Refusal::Overloaded)?;
                for word in object.bytes.chunks_exact(4) {
                    samples.push(f32::from_le_bytes(word.try_into().unwrap()));
                }
                Payload::Audio(samples.into())
            }
            ObjectKind::Midi => {
                let mut events = Vec::new();
                let mut bytes = object.bytes.as_slice();
                while !bytes.is_empty() {
                    let frame = bytes.get(..8).ok_or(Refusal::Invalid)?;
                    let len = u32::from_le_bytes(
                        bytes
                            .get(8..12)
                            .ok_or(Refusal::Invalid)?
                            .try_into()
                            .unwrap(),
                    ) as usize;
                    let packet = bytes.get(12..12 + len).ok_or(Refusal::Invalid)?;
                    if len == 0 || events.len() as u64 >= MAX_SESSION_MIDI_EVENTS {
                        return Err(Refusal::Invalid);
                    }
                    events.try_reserve(1).map_err(|_| Refusal::Overloaded)?;
                    events.push(MidiEventState {
                        frame: u32::try_from(u64::from_le_bytes(frame.try_into().unwrap()))
                            .map_err(|_| Refusal::Invalid)?,
                        data: Arc::from(packet),
                    });
                    bytes = &bytes[12 + len..];
                }
                Payload::Midi(events.into())
            }
            ObjectKind::Metadata => return Err(Refusal::Invalid),
        });
    }
    let image = Arc::new(Image { manifest, payloads });
    image.validate()?;
    Ok(image)
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
