use alloc::vec;
use alloc::vec::Vec;

use super::*;
use crate::{persistence::*, Refusal};
use alloc::collections::BTreeSet;

/// Validation evidence retains the immutable image it describes.
pub struct ValidatedImage {
    pub(crate) image: Arc<Image>,
    pub(crate) media_bytes: u64,
    pub(crate) midi_offsets: Option<Vec<Vec<u64>>>,
}
impl ValidatedImage {
    pub fn image(&self) -> &Arc<Image> {
        &self.image
    }
    pub fn media_bytes(&self) -> u64 {
        self.media_bytes
    }
    pub fn into_image(self) -> Arc<Image> {
        self.image
    }
}
struct Progress {
    frames: Vec<u64>,
    payload: usize,
    position: usize,
    previous: u32,
    bytes: u64,
    offsets: Option<Vec<Vec<u64>>>,
}
impl Progress {
    fn new(image: &Image, index: bool) -> Result<Self, Refusal> {
        if image.payloads.len() >= MAX_SESSION_OBJECTS as usize {
            return Err(Refusal::Invalid);
        }
        let mut frames = vec![0; image.payloads.len()];
        let mut referenced = BTreeSet::new();
        let mut check = |channel: &Channel, midi: bool| -> Result<(), Refusal> {
            if !channel.gain.is_finite()
                || channel.frames > u32::MAX as u64
                || i32::try_from(channel.start_offset).is_err()
                || i32::try_from(channel.capture_alignment).is_err()
                || u32::try_from(channel.preplay).is_err()
                || channel
                    .recording_started_at
                    .as_ref()
                    .is_some_and(|v| v.len() > 128)
            {
                return Err(Refusal::Invalid);
            }
            if let Some(object) = channel.object {
                if object == 0 || !referenced.insert(object) {
                    return Err(Refusal::Identity);
                }
                match image.payloads.get(object as usize - 1) {
                    Some(Payload::Audio(samples))
                        if !midi && samples.len() as u64 == channel.frames => {}
                    Some(Payload::Midi(_)) if midi => {}
                    _ => return Err(Refusal::Invalid),
                }
                frames[object as usize - 1] = channel.frames;
            } else if !midi && channel.frames != 0 {
                return Err(Refusal::Invalid);
            }
            Ok(())
        };
        let loops: Vec<&Loop> = match &image.manifest {
            Manifest::FamilyUpdate {
                format,
                root,
                members,
            } => {
                let ids = members
                    .iter()
                    .map(|member| member.loop_id)
                    .collect::<BTreeSet<_>>();
                if !format.is_valid()
                    || !root.is_valid()
                    || members.is_empty()
                    || members.len() > 256
                    || ids.len() != members.len()
                    || !members
                        .iter()
                        .any(|member| member.loop_id == *root && member.content.is_some())
                    || members.iter().any(|member| {
                        !member.loop_id.is_valid()
                            || member.sync_source.is_some_and(|source| !source.is_valid())
                            || member.expected_frames > u32::MAX as u64
                            || (member.restart && member.content.is_none())
                            || member.content.as_ref().is_some_and(|content| {
                                !member.fence_content
                                    || content.id != member.loop_id
                                    || content.frames % u64::from(format.quantum_frames) != 0
                            })
                    })
                {
                    return Err(Refusal::Invalid);
                }
                members
                    .iter()
                    .filter_map(|member| member.content.as_ref())
                    .collect()
            }
            Manifest::Session(session) => {
                if session.sample_rate == 0
                    || session.tracks.len() > 64
                    || session.tracks.iter().map(|t| t.loops.len()).sum::<usize>() > 256
                {
                    return Err(Refusal::Invalid);
                }
                if session.buses.len() > 8
                    || session
                        .buses
                        .iter()
                        .any(|bus| bus.channels.is_empty() || bus.channels.len() > 8)
                    || session.bus_display_order.len() != session.buses.len()
                {
                    return Err(Refusal::Invalid);
                }
                let ids = session.buses.iter().map(|b| b.id).collect::<BTreeSet<_>>();
                let order = session
                    .bus_display_order
                    .iter()
                    .copied()
                    .collect::<BTreeSet<_>>();
                if ids.len() != session.buses.len()
                    || ids != order
                    || session.preview_bus.is_some_and(|id| !ids.contains(&id))
                {
                    return Err(Refusal::Identity);
                }
                let loops = session
                    .tracks
                    .iter()
                    .flat_map(|t| &t.loops)
                    .collect::<Vec<_>>();
                validate_relations(&loops, session.sample_rate)?;
                loops
            }
            Manifest::Loop {
                sample_rate,
                content,
            } => {
                if *sample_rate == 0 {
                    return Err(Refusal::Invalid);
                }
                vec![content]
            }
            Manifest::Update {
                sample_rate,
                frames,
                audio,
                midi,
            } => {
                if *sample_rate == 0
                    || frames.is_some_and(|f| f > u32::MAX as u64)
                    || audio.len() > 2
                    || midi.len() > 1
                {
                    return Err(Refusal::Invalid);
                }
                for channel in audio {
                    check(&channel.content, false)?;
                }
                for channel in midi {
                    check(&channel.content.channel, true)?;
                }
                Vec::new()
            }
        };
        for content in loops {
            if !content.id.is_valid()
                || content.composite.is_none() && content.frames > u32::MAX as u64
                || !content.gain.is_finite()
                || !(-1.0..=1.0).contains(&content.balance)
                || content.name.len() > 128
                || content.audio.len() > 2
                || content.midi.len() > 1
            {
                return Err(Refusal::Invalid);
            }
            if let Some(composite) = &content.composite {
                if !composite.valid()
                    || !content.audio.is_empty()
                    || !content.midi.is_empty()
                    || composite
                        .prepared
                        .unit_frames
                        .checked_mul(composite.prepared.iterations)
                        != Some(content.frames)
                {
                    return Err(Refusal::Invalid);
                }
            }
            for channel in &content.audio {
                check(channel, false)?;
            }
            for channel in &content.midi {
                check(&channel.channel, true)?;
            }
        }
        if referenced.len() != image.payloads.len() {
            return Err(Refusal::Invalid);
        }
        let mut event_count = 0u64;
        let mut offsets = index.then(Vec::new);
        if let Some(offsets) = &mut offsets {
            offsets
                .try_reserve_exact(image.payloads.len())
                .map_err(|_| Refusal::Overloaded)?;
        }
        for payload in &image.payloads {
            let mut rows = Vec::new();
            if let Payload::Midi(events) = payload {
                event_count = event_count
                    .checked_add(events.len() as u64)
                    .ok_or(Refusal::Invalid)?;
                if event_count > MAX_SESSION_MIDI_EVENTS {
                    return Err(Refusal::Overloaded);
                }
                if index {
                    rows.try_reserve_exact(events.len() + 1)
                        .map_err(|_| Refusal::Overloaded)?;
                    rows.push(0);
                }
            }
            if let Some(offsets) = &mut offsets {
                offsets.push(rows);
            }
        }
        Ok(Self {
            frames,
            payload: 0,
            position: 0,
            previous: 0,
            bytes: 0,
            offsets,
        })
    }
    fn step(&mut self, image: &Image, budget: usize) -> Result<bool, Refusal> {
        let mut left = budget.max(4);
        while self.payload < image.payloads.len() && left > 0 {
            match &image.payloads[self.payload] {
                Payload::Audio(samples) => {
                    let end = samples.len().min(self.position + (left / 4).max(1));
                    if samples[self.position..end].iter().any(|s| !s.is_finite()) {
                        return Err(Refusal::Invalid);
                    }
                    let used = (end - self.position) * 4;
                    self.bytes = self
                        .bytes
                        .checked_add(used as u64)
                        .ok_or(Refusal::Invalid)?;
                    left = left.saturating_sub(used);
                    self.position = end;
                    if end != samples.len() {
                        break;
                    }
                }
                Payload::Midi(events) => {
                    while self.position < events.len() && left > 0 {
                        let event = &events[self.position];
                        if event.frame < self.previous
                            || u64::from(event.frame) >= self.frames[self.payload]
                            || event.data.is_empty()
                            || event.data.len() > 1024 * 1024
                        {
                            return Err(Refusal::Invalid);
                        }
                        let used = 12 + event.data.len();
                        // A packet is the atomic unit, capped at one MiB by the protocol.
                        self.bytes = self
                            .bytes
                            .checked_add(used as u64)
                            .ok_or(Refusal::Invalid)?;
                        if let Some(offsets) = &mut self.offsets {
                            let rows = &mut offsets[self.payload];
                            rows.push(rows.last().copied().unwrap() + used as u64);
                        }
                        self.previous = event.frame;
                        self.position += 1;
                        left = left.saturating_sub(used);
                    }
                    if self.position != events.len() {
                        break;
                    }
                }
            }
            if self.bytes > MAX_SESSION_MEDIA_BYTES {
                return Err(Refusal::Overloaded);
            }
            self.payload += 1;
            self.position = 0;
            self.previous = 0;
        }
        if self.bytes > MAX_SESSION_MEDIA_BYTES {
            return Err(Refusal::Overloaded);
        }
        Ok(self.payload == image.payloads.len())
    }
}

fn validate_relations(loops: &[&Loop], sample_rate: u32) -> Result<(), Refusal> {
    use alloc::collections::BTreeMap;
    let indexed = loops
        .iter()
        .map(|content| (content.id, *content))
        .collect::<BTreeMap<_, _>>();
    if indexed.len() != loops.len() {
        return Err(Refusal::Identity);
    }
    let mut edges = BTreeMap::<LoopId, BTreeSet<LoopId>>::new();
    let mut quantum = None;
    for content in loops {
        if let Some(source) = content.sync_source {
            if !indexed.contains_key(&source) {
                return Err(Refusal::Identity);
            }
            edges.entry(source).or_default().insert(content.id);
        }
        if let Some(composite) = &content.composite {
            if composite.prepared.sample_rate != sample_rate
                || quantum.is_some_and(|q| q != composite.prepared.quantum)
            {
                return Err(Refusal::Invalid);
            }
            quantum = Some(composite.prepared.quantum);
            for event in &composite.document.instances {
                let target = indexed.get(&event.loop_id).ok_or(Refusal::Identity)?;
                if target.composite.is_some()
                    && !matches!(
                        event.mode,
                        None | Some(LoopMode::Playing | LoopMode::Stopped)
                    )
                {
                    return Err(Refusal::Invalid);
                }
                edges.entry(content.id).or_default().insert(event.loop_id);
            }
        }
    }
    fn visit(
        id: LoopId,
        edges: &BTreeMap<LoopId, BTreeSet<LoopId>>,
        visiting: &mut BTreeSet<LoopId>,
        visited: &mut BTreeSet<LoopId>,
    ) -> bool {
        if visited.contains(&id) {
            return true;
        }
        if !visiting.insert(id) {
            return false;
        }
        if edges.get(&id).is_some_and(|targets| {
            targets
                .iter()
                .any(|id| !visit(*id, edges, visiting, visited))
        }) {
            return false;
        }
        visiting.remove(&id);
        visited.insert(id);
        true
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    if edges
        .keys()
        .any(|id| !visit(*id, &edges, &mut visiting, &mut visited))
    {
        return Err(Refusal::Invalid);
    }
    Ok(())
}

pub struct Validation {
    image: Arc<Image>,
    progress: Progress,
    complete: bool,
}
impl Validation {
    pub fn new(image: Arc<Image>) -> Result<Self, Refusal> {
        Self::begin(image, false)
    }
    pub fn for_transfer(image: Arc<Image>) -> Result<Self, Refusal> {
        Self::begin(image, true)
    }
    fn begin(image: Arc<Image>, index: bool) -> Result<Self, Refusal> {
        let progress = Progress::new(&image, index)?;
        Ok(Self {
            image,
            progress,
            complete: false,
        })
    }
    pub fn step(&mut self, bytes: usize) -> Result<bool, Refusal> {
        self.complete = self.progress.step(&self.image, bytes)?;
        Ok(self.complete)
    }
    pub fn finish(self) -> Result<ValidatedImage, Refusal> {
        if !self.complete {
            return Err(Refusal::NotReady);
        }
        Ok(ValidatedImage {
            image: self.image,
            media_bytes: self.progress.bytes,
            midi_offsets: self.progress.offsets,
        })
    }
}
impl Image {
    pub fn validate(&self) -> Result<u64, Refusal> {
        let mut progress = Progress::new(self, false)?;
        while !progress.step(self, 64 * 1024)? {}
        Ok(progress.bytes)
    }
}
