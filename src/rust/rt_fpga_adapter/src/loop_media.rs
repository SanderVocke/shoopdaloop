//! Host media identities and leases. Samples and reference counts stay in the device.
use crate::looping::{LoopDevice, MediaCommand};
use rt_runtime::{
    Error, Frame, ResourceId, Revision,
    resource::{CaptureInfo, Cursor, Description, ExternalReadScope, Format, Snapshot, SnapshotId},
};
use shoop_loops::{
    Mode, State,
    audio::Smoothing,
    media,
    runtime::{Cut, Reply, Selection},
    windows::Windows,
};
struct Content {
    id: media::MediaId,
    view: u32,
    source: media::Source,
    description: media::Description,
    references: usize,
}
struct Slot<T> {
    generation: u32,
    value: Option<T>,
}
struct Capture {
    revision: Revision,
    bank: usize,
    base: u64,
    start: Frame,
}
pub struct Media {
    ids: media::IdentityScope,
    reads: ExternalReadScope,
    contents: Vec<Slot<Content>>,
    exports: Vec<Slot<media::MediaId>>,
    latest: Option<media::MediaId>,
    banks: [u64; 2],
    next_bank: usize,
    capture: Option<Capture>,
}
impl Media {
    pub fn new(exports: usize) -> Result<Self, Error> {
        Ok(Self {
            ids: media::IdentityScope::new()?,
            reads: ExternalReadScope::new()?,
            contents: (0..8)
                .map(|_| Slot {
                    generation: 1,
                    value: None,
                })
                .collect(),
            exports: (0..exports)
                .map(|_| Slot {
                    generation: 1,
                    value: None,
                })
                .collect(),
            latest: None,
            banks: [0; 2],
            next_bank: 0,
            capture: None,
        })
    }
    fn content(&self, id: media::MediaId) -> Result<&Content, Error> {
        self.contents
            .iter()
            .filter_map(|s| s.value.as_ref())
            .find(|c| c.id == id)
            .ok_or(Error::InvalidId)
    }
    pub fn observe(
        &mut self,
        state: State,
        frame: Frame,
        device: &mut impl LoopDevice,
    ) -> Result<(), Error> {
        if state.mode == Mode::Recording
            && self
                .capture
                .as_ref()
                .is_none_or(|c| c.revision != state.capture_revision)
        {
            let bank = self.next_bank;
            let base = self.banks[bank].checked_add(1).ok_or(Error::Exhausted)?;
            self.banks[bank] = base;
            self.capture = Some(Capture {
                revision: state.capture_revision,
                bank,
                base,
                start: frame,
            });
            self.next_bank ^= 1;
        }
        if let Some(capture) = &self.capture {
            if capture.revision == state.capture_revision {
                self.banks[capture.bank] = capture
                    .base
                    .checked_add(state.capture_frames / 32)
                    .ok_or(Error::Exhausted)?;
            }
        }
        if state.content_length != 0
            && self.latest.is_none_or(|id| {
                self.content(id)
                    .is_ok_and(|c| c.description.revision != state.content_revision)
            })
        {
            let slot = self
                .contents
                .iter()
                .position(|s| s.value.is_none())
                .ok_or(Error::Capacity)?;
            let capture = self
                .capture
                .as_ref()
                .filter(|c| c.revision == state.content_revision)
                .ok_or(Error::InvalidState)?;
            let id = self
                .ids
                .media(slot as u32, self.contents[slot].generation)?;
            let snapshot = Snapshot {
                id: SnapshotId {
                    slot: slot as u32,
                    generation: self.contents[slot].generation,
                },
                resource: ResourceId::from_parts(capture.bank as u32, 1)?,
                revision: Revision(self.banks[capture.bank]),
                description: Description {
                    format: Format::Audio { channels: 1 },
                    frames: state.content_length,
                    bytes: usize::try_from(state.content_length)
                        .ok()
                        .and_then(|n| n.checked_mul(4))
                        .ok_or(Error::Capacity)?,
                },
                cut_frame: frame,
                capture: Some(CaptureInfo {
                    start_frame: capture.start,
                    next_frame: frame,
                    active: false,
                    incomplete: None,
                }),
            };
            let result = device.media(MediaCommand {
                opcode: 1,
                view: 0,
                start: 0,
                length: state.content_length,
                count: 0,
            })?;
            let source = media::Source {
                snapshot,
                index: self.reads.index(snapshot.id),
            };
            self.contents[slot].value = Some(Content {
                id,
                view: result.view,
                source,
                description: media::Description {
                    owner: state.id,
                    revision: state.content_revision,
                    frames: state.content_length,
                    channels: 1,
                    auxiliary_channels: 0,
                    midi: false,
                },
                references: 0,
            });
            if let Some(old) = self.latest.replace(id) {
                self.retire(old, device)?;
            }
        }
        Ok(())
    }
    fn retire(&mut self, id: media::MediaId, device: &mut impl LoopDevice) -> Result<(), Error> {
        let index = self
            .contents
            .iter()
            .position(|s| s.value.as_ref().is_some_and(|c| c.id == id))
            .ok_or(Error::InvalidId)?;
        let slot = &mut self.contents[index];
        let content = slot.value.as_ref().ok_or(Error::InvalidId)?;
        if Some(id) != self.latest && content.references == 0 {
            let generation = slot.generation.checked_add(1).ok_or(Error::Exhausted)?;
            device.media(MediaCommand {
                opcode: 3,
                view: content.view,
                start: 0,
                length: 0,
                count: 0,
            })?;
            slot.value = None;
            slot.generation = generation;
        }
        Ok(())
    }
    pub fn cut(
        &self,
        state: State,
        frame: Frame,
        epoch: rt_runtime::Epoch,
    ) -> Result<Cut<Q824>, Error> {
        let frames = self
            .latest
            .map(|id| self.content(id).map(|c| c.description.frames))
            .transpose()?
            .unwrap_or(0);
        Ok(Cut {
            raw_audio_frames: frames,
            raw_midi_frames: 0,
            windows: Windows::default(),
            timing_revision: state.timing_revision,
            frame,
            graph_epoch: epoch,
            topology_epoch: rt_runtime::Epoch(0),
            state,
            playback: Some(self.latest.unwrap_or(self.ids.media(u32::MAX, 1)?)),
            latest: self.latest,
            gain: Q824(1 << 24),
            balance: Q824(0),
            smoothing: Smoothing::None,
            capture_failure: None,
            midi_overflows: 0,
        })
    }
    pub fn export(
        &mut self,
        state: State,
        frame: Frame,
        epoch: rt_runtime::Epoch,
        selection: Selection,
    ) -> Result<Reply<Q824>, Error> {
        if selection == Selection::Recording {
            return Err(Error::InvalidParameter);
        }
        let media = self.latest.ok_or(Error::InvalidState)?;
        let slot = self
            .exports
            .iter()
            .position(|s| s.value.is_none())
            .ok_or(Error::Capacity)?;
        let id = self
            .ids
            .export(slot as u32, self.exports[slot].generation)?;
        let cut = self.cut(state, frame, epoch)?;
        let content = self.content(media)?;
        let view = media::View {
            windows: Some(Windows::default()),
            description: content.description,
            audio: Some(content.source),
            auxiliary: None,
            midi: None,
        };
        self.exports[slot].value = Some(media);
        self.contents
            .iter_mut()
            .filter_map(|s| s.value.as_mut())
            .find(|c| c.id == media)
            .ok_or(Error::InvalidId)?
            .references += 1;
        Ok(Reply::Export {
            id,
            cut,
            media: media::Export {
                media,
                requested_cut: frame,
                view,
            },
        })
    }
    fn exported(&self, id: media::ExportId) -> Result<media::MediaId, Error> {
        let parts = id.parts();
        let slot = self
            .exports
            .get(parts[1] as usize)
            .ok_or(Error::InvalidId)?;
        if self.ids.export(parts[1], slot.generation)? != id {
            return Err(Error::InvalidId);
        }
        slot.value.ok_or(Error::InvalidId)
    }
    pub fn cursor(&self, id: media::ExportId) -> Result<Cursor, Error> {
        Ok(self
            .reads
            .cursor(self.content(self.exported(id)?)?.source.snapshot.id))
    }
    pub fn release(
        &mut self,
        id: media::ExportId,
        device: &mut impl LoopDevice,
    ) -> Result<(), Error> {
        let content = self.exported(id)?;
        let slot = &mut self.exports[id.parts()[1] as usize];
        let generation = slot.generation.checked_add(1).ok_or(Error::Exhausted)?;
        slot.value = None;
        slot.generation = generation;
        self.contents
            .iter_mut()
            .filter_map(|s| s.value.as_mut())
            .find(|c| c.id == content)
            .ok_or(Error::InvalidId)?
            .references -= 1;
        self.retire(content, device)
    }
    pub fn read(
        &self,
        cursor: &mut Cursor,
        output: &mut [u8],
        page_bytes: usize,
        device: &mut impl LoopDevice,
    ) -> Result<(usize, bool), Error> {
        let snapshot = self.reads.snapshot(*cursor)?;
        let slot = self
            .contents
            .get(snapshot.slot as usize)
            .ok_or(Error::InvalidId)?;
        let content = slot
            .value
            .as_ref()
            .filter(|_| slot.generation == snapshot.generation)
            .ok_or(Error::InvalidId)?;
        let position = cursor.position();
        let total = content.source.snapshot.description.bytes;
        let count = output
            .len()
            .min(page_bytes - position % page_bytes)
            .min(total.checked_sub(position).ok_or(Error::InvalidOffset)?);
        if count > 0 {
            let start = position / 4;
            let frames = (position % 4 + count).div_ceil(4);
            let samples = device
                .media(MediaCommand {
                    opcode: 6,
                    view: content.view,
                    start: start as u64,
                    length: content.description.frames,
                    count: frames as u32,
                })?
                .samples;
            if samples.len() != frames {
                return Err(Error::InvalidState);
            }
            for (i, byte) in output[..count].iter_mut().enumerate() {
                let offset = position % 4 + i;
                *byte = samples[offset / 4].to_le_bytes()[offset % 4];
            }
        }
        self.reads.advance(cursor, count, total)?;
        Ok((count, cursor.position() == total))
    }
}
use rt_runtime::Q824;
