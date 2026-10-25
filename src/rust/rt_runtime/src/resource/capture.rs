use super::*;
use crate::buffer::MidiBuffer;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureId {
    pub slot: u32,
    pub generation: u32,
}
impl CaptureId {
    fn version(self) -> UploadId {
        UploadId {
            slot: self.slot,
            generation: self.generation,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureInfo {
    pub start_frame: Frame,
    pub next_frame: Frame,
    pub active: bool,
    pub incomplete: Option<Error>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceInfo {
    pub revision: Revision,
    pub description: Description,
    pub capture: Option<CaptureInfo>,
}

impl ResourceStore {
    pub fn info(&self, resource: ResourceId) -> Result<ResourceInfo, Error> {
        let active = get(&self.resources, resource.slot(), resource.generation())?
            .active
            .ok_or(Error::InvalidState)?;
        let version = get(&self.versions, active.slot, active.generation)?;
        Ok(ResourceInfo {
            revision: version.revision,
            description: version.description,
            capture: version.capture,
        })
    }
    pub fn active_capture(&self, resource: ResourceId) -> Result<CaptureId, Error> {
        let active = get(&self.resources, resource.slot(), resource.generation())?
            .active
            .ok_or(Error::InvalidState)?;
        let version = get(&self.versions, active.slot, active.generation)?;
        if version.state != State::Capturing {
            return Err(Error::InvalidState);
        }
        Ok(CaptureId {
            slot: active.slot,
            generation: active.generation,
        })
    }
    /// Begin a fresh capture revision; any previously active content remains
    /// available to its existing leases until explicitly released.
    pub fn begin_capture(
        &mut self,
        resource: ResourceId,
        expected_revision: Revision,
        format: Format,
        frame: Frame,
    ) -> Result<CaptureId, Error> {
        expected_revision.checked_next()?;
        let id = self.begin_upload(
            resource,
            expected_revision,
            Description {
                format,
                frames: 0,
                bytes: 0,
            },
        )?;
        self.finish_upload(id)?;
        self.commit(id)?;
        let mut version = get(&self.versions, id.slot, id.generation)?;
        version.state = State::Capturing;
        version.capture = Some(CaptureInfo {
            start_frame: frame,
            next_frame: frame,
            active: true,
            incomplete: None,
        });
        self.versions[id.slot as usize].value = Some(version);
        Ok(CaptureId {
            slot: id.slot,
            generation: id.generation,
        })
    }
    fn capture_version(&self, id: CaptureId) -> Result<Version, Error> {
        let version = get(&self.versions, id.slot, id.generation)?;
        let resource = get(
            &self.resources,
            version.resource.slot(),
            version.resource.generation(),
        )?;
        if resource.active != Some(id.version()) || version.capture.is_none() {
            return Err(Error::InvalidId);
        }
        Ok(version)
    }
    fn running_capture(&self, id: CaptureId, frame: Frame) -> Result<Version, Error> {
        let version = self.capture_version(id)?;
        let capture = version.capture.ok_or(Error::InvalidState)?;
        if capture.incomplete.is_some() {
            return Err(Error::Incomplete);
        }
        if !capture.active || version.state != State::Capturing {
            return Err(Error::InvalidState);
        }
        if capture.next_frame != frame {
            return Err(Error::InvalidFrame);
        }
        Ok(version)
    }
    pub fn stop_capture(&mut self, id: CaptureId) -> Result<Revision, Error> {
        let mut version = self.capture_version(id)?;
        version.capture.as_mut().ok_or(Error::InvalidState)?.active = false;
        version.state = State::Active;
        self.versions[id.slot as usize].value = Some(version);
        Ok(version.revision)
    }
    fn halt_capture(&mut self, id: CaptureId, error: Error) -> Error {
        if let Some(version) = self.versions[id.slot as usize].value.as_mut() {
            version.state = State::Active;
            if let Some(capture) = &mut version.capture {
                capture.active = false;
                capture.incomplete = Some(error);
            }
        }
        error
    }
    fn reserve_capture(
        &mut self,
        id: CaptureId,
        mut version: Version,
        frame: Frame,
        frames: usize,
        bytes: usize,
        class: WorkClass,
    ) -> Result<Version, Error> {
        let next_frame = frame
            .checked_add(frames)
            .map_err(|error| self.halt_capture(id, error))?;
        let duration = version
            .description
            .frames
            .checked_add(frames as u64)
            .ok_or_else(|| self.halt_capture(id, Error::Exhausted))?;
        let revision = version
            .revision
            .checked_next()
            .map_err(|error| self.halt_capture(id, error))?;
        let length = version
            .written
            .checked_add(bytes)
            .ok_or_else(|| self.halt_capture(id, Error::Capacity))?;
        let page_bytes = self.config.limits().page_bytes;
        let pages = length.div_ceil(page_bytes) - version.written.div_ceil(page_bytes);
        if pages > self.free_pages.len() - self.reserved_pages {
            return Err(self.halt_capture(id, Error::Capacity));
        }
        self.charge_work(class, bytes, 0)
            .map_err(|error| self.halt_capture(id, error))?;
        self.reserved_pages += pages;
        version.reserved += pages;
        version.description.bytes = length;
        version.description.frames = duration;
        version.revision = revision;
        version
            .capture
            .as_mut()
            .ok_or(Error::InvalidState)?
            .next_frame = next_frame;
        Ok(version)
    }
    fn append_reserved(&mut self, version: &mut Version, mut bytes: &[u8], class: WorkClass) {
        let page_bytes = self.config.limits().page_bytes;
        while !bytes.is_empty() {
            if version
                .tail
                .is_none_or(|page| self.pages[page].used == page_bytes)
            {
                let page = self.free_pages.pop().expect("reserved page credit");
                self.pages[page] = Page::default();
                self.index_append(version, page, class);
                if let Some(tail) = version.tail {
                    self.pages[tail].next = Some(page);
                } else {
                    version.head = Some(page);
                }
                version.tail = Some(page);
                version.reserved -= 1;
                self.reserved_pages -= 1;
            }
            let page = version.tail.expect("materialized tail");
            let count = bytes.len().min(page_bytes - self.pages[page].used);
            let start = page * page_bytes + self.pages[page].used;
            self.data[start..start + count].copy_from_slice(&bytes[..count]);
            self.pages[page].used += count;
            version.written += count;
            bytes = &bytes[count..];
        }
    }
    fn publish_capture(&mut self, id: CaptureId, version: Version) -> Revision {
        self.versions[id.slot as usize].value = Some(version);
        self.resources[version.resource.slot() as usize]
            .value
            .as_mut()
            .expect("active capture identity")
            .revision = version.revision;
        version.revision
    }
    /// Append a complete interleaved audio block atomically. Storage/copy-budget
    /// exhaustion stops capture with an explicit incomplete prefix, never overwrite.
    pub fn capture_audio(
        &mut self,
        id: CaptureId,
        frame: Frame,
        samples: &[f32],
    ) -> Result<Revision, Error> {
        self.capture_audio_with(id, frame, samples, WorkClass::Background)
    }
    pub(super) fn capture_audio_with(
        &mut self,
        id: CaptureId,
        frame: Frame,
        samples: &[f32],
        class: WorkClass,
    ) -> Result<Revision, Error> {
        self.note_operation(class);
        let version = self.running_capture(id, frame)?;
        let Format::Audio { channels } = version.description.format else {
            return Err(Error::InvalidShape);
        };
        if samples.len() % channels as usize != 0 {
            return Err(Error::InvalidShape);
        }
        let frames = samples.len() / channels as usize;
        if frames > self.config.limits().max_frames {
            return Err(Error::InvalidShape);
        }
        if frames == 0 {
            return Ok(version.revision);
        }
        let bytes = samples
            .len()
            .checked_mul(4)
            .ok_or_else(|| self.halt_capture(id, Error::Capacity))?;
        let mut version = self.reserve_capture(id, version, frame, frames, bytes, class)?;
        for sample in samples {
            self.append_reserved(&mut version, &sample.to_le_bytes(), class);
        }
        Ok(self.publish_capture(id, version))
    }
    /// MIDI input is already a validated arena. Offset/length checks take one
    /// validation step per event; serialized header and payload copies share the
    /// same byte budget as audio, uploads, snapshot tails, and reads.
    pub fn capture_midi(
        &mut self,
        id: CaptureId,
        frame: Frame,
        frames: usize,
        input: &MidiBuffer,
    ) -> Result<Revision, Error> {
        self.capture_midi_with(id, frame, frames, input, WorkClass::Background)
    }
    pub(super) fn capture_midi_with(
        &mut self,
        id: CaptureId,
        frame: Frame,
        frames: usize,
        input: &MidiBuffer,
        class: WorkClass,
    ) -> Result<Revision, Error> {
        self.note_operation(class);
        let version = self.running_capture(id, frame)?;
        if version.description.format != Format::Midi || frames > self.config.limits().max_frames {
            return Err(Error::InvalidShape);
        }
        if frames == 0 && !input.is_empty() {
            return Err(Error::InvalidEvent);
        }
        if frames == 0 {
            return Ok(version.revision);
        }
        let limits = self.config.limits();
        if input.len()
            > limits
                .midi_events_per_port
                .max(crate::midi::ResetState::EVENTS)
            || input.byte_len()
                > limits
                    .midi_bytes_per_port
                    .max(crate::midi::ResetState::BYTES)
        {
            return Err(Error::InvalidEvent);
        }
        self.charge_work(class, 0, input.len())
            .map_err(|error| self.halt_capture(id, error))?;
        let mut bytes = 0usize;
        for index in 0..input.len() {
            let (offset, data) = input.get(index).ok_or(Error::InvalidEvent)?;
            if offset as usize >= frames || data.len() > limits.midi_bytes_per_port {
                return Err(Error::InvalidEvent);
            }
            bytes = bytes
                .checked_add(12)
                .and_then(|n| n.checked_add(data.len()))
                .ok_or_else(|| self.halt_capture(id, Error::Capacity))?;
        }
        let base = version.description.frames;
        let mut version = self.reserve_capture(id, version, frame, frames, bytes, class)?;
        for index in 0..input.len() {
            let (offset, data) = input.get(index).expect("validated MIDI event");
            let mut header = [0u8; 12];
            header[..8].copy_from_slice(&(base + u64::from(offset)).to_le_bytes());
            header[8..].copy_from_slice(&(data.len() as u32).to_le_bytes());
            self.append_reserved(&mut version, &header, class);
            self.append_reserved(&mut version, data, class);
        }
        Ok(self.publish_capture(id, version))
    }
}
