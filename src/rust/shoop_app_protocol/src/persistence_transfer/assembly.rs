use super::*;
use shoop_app_api::MidiEventState;
use std::collections::VecDeque;

struct Active {
    object: Object,
    offset: usize,
    audio: Vec<f32>,
    midi: Vec<MidiEventState>,
}
/// Converts at most the requested PCM bytes or complete MIDI packets per turn,
/// then validates and indexes the immutable image using the same bounded budget.
pub struct Assembly {
    manifest: Option<Manifest>,
    objects: VecDeque<Object>,
    active: Option<Active>,
    payloads: Vec<Payload>,
    events: u64,
    validation: Option<Validation>,
    finished: bool,
}
impl Assembly {
    pub fn new(objects: Vec<Object>) -> Result<Self, Refusal> {
        if objects.is_empty() || objects.len() > MAX_SESSION_OBJECTS as usize {
            return Err(Refusal::Invalid);
        }
        let mut objects = VecDeque::from(objects);
        let metadata = objects.pop_front().ok_or(Refusal::Invalid)?;
        if metadata.description.kind != ObjectKind::Metadata
            || metadata.description.index != 0
            || metadata.bytes.is_empty()
            || metadata.bytes.len() as u64 > MAX_SESSION_METADATA_BYTES
            || metadata.description.bytes != metadata.bytes.len() as u64
        {
            return Err(Refusal::Invalid);
        }
        let manifest = codec::decode_manifest(&metadata.bytes).map_err(|_| Refusal::Invalid)?;
        let mut total = 0u64;
        for (index, object) in objects.iter().enumerate() {
            if !object.description.valid()
                || object.description.index as usize != index + 1
                || object.description.kind == ObjectKind::Metadata
                || object.description.bytes != object.bytes.len() as u64
            {
                return Err(Refusal::Invalid);
            }
            total = total
                .checked_add(object.description.bytes)
                .ok_or(Refusal::Invalid)?;
            if total > MAX_SESSION_MEDIA_BYTES {
                return Err(Refusal::Overloaded);
            }
        }
        Ok(Self {
            manifest: Some(manifest),
            objects,
            active: None,
            payloads: Vec::new(),
            events: 0,
            validation: None,
            finished: false,
        })
    }
    pub fn step(&mut self, budget: usize) -> Result<Option<ValidatedImage>, Refusal> {
        if self.finished {
            return Err(Refusal::NotReady);
        }
        if let Some(validation) = &mut self.validation {
            if !validation.step(budget)? {
                return Ok(None);
            }
            self.finished = true;
            return self.validation.take().unwrap().finish().map(Some);
        }
        let mut left = budget.max(4);
        while left > 0 {
            if self.active.is_none() {
                let Some(object) = self.objects.pop_front() else {
                    self.validation = Some(Validation::for_transfer(Arc::new(Image {
                        manifest: self.manifest.take().unwrap(),
                        payloads: std::mem::take(&mut self.payloads),
                    }))?);
                    return Ok(None);
                };
                let mut audio = Vec::new();
                if object.description.kind == ObjectKind::Audio {
                    if object.bytes.len() % 4 != 0 {
                        return Err(Refusal::Invalid);
                    }
                    audio
                        .try_reserve_exact(object.bytes.len() / 4)
                        .map_err(|_| Refusal::Overloaded)?;
                }
                self.active = Some(Active {
                    object,
                    offset: 0,
                    audio,
                    midi: Vec::new(),
                });
            }
            let active = self.active.as_mut().unwrap();
            match active.object.description.kind {
                ObjectKind::Audio => {
                    let end = active
                        .object
                        .bytes
                        .len()
                        .min(active.offset + (left / 4).max(1) * 4);
                    for word in active.object.bytes[active.offset..end].chunks_exact(4) {
                        active
                            .audio
                            .push(f32::from_le_bytes(word.try_into().unwrap()));
                    }
                    left = left.saturating_sub(end - active.offset);
                    active.offset = end;
                }
                ObjectKind::Midi => {
                    while left > 0 && active.offset < active.object.bytes.len() {
                        let bytes = &active.object.bytes[active.offset..];
                        let frame = bytes.get(..8).ok_or(Refusal::Invalid)?;
                        let length = u32::from_le_bytes(
                            bytes
                                .get(8..12)
                                .ok_or(Refusal::Invalid)?
                                .try_into()
                                .unwrap(),
                        ) as usize;
                        if length == 0
                            || length > 1024 * 1024
                            || self.events >= MAX_SESSION_MIDI_EVENTS
                        {
                            return Err(Refusal::Invalid);
                        }
                        let packet = bytes.get(12..12 + length).ok_or(Refusal::Invalid)?;
                        active
                            .midi
                            .try_reserve(1)
                            .map_err(|_| Refusal::Overloaded)?;
                        active.midi.push(MidiEventState {
                            frame: u32::try_from(u64::from_le_bytes(frame.try_into().unwrap()))
                                .map_err(|_| Refusal::Invalid)?,
                            data: Arc::from(packet),
                        });
                        self.events += 1;
                        active.offset += 12 + length;
                        left = left.saturating_sub(12 + length);
                    }
                }
                ObjectKind::Metadata => return Err(Refusal::Invalid),
            }
            if active.offset != active.object.bytes.len() {
                break;
            }
            let active = self.active.take().unwrap();
            self.payloads
                .try_reserve(1)
                .map_err(|_| Refusal::Overloaded)?;
            self.payloads.push(match active.object.description.kind {
                ObjectKind::Audio => Payload::Audio(Arc::new(active.audio)),
                ObjectKind::Midi => Payload::Midi(active.midi.into()),
                _ => unreachable!(),
            });
        }
        Ok(None)
    }
}
