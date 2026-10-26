//! Fixed media ownership for prepared content, playback, completed takes and exports.
//!
//! This ledger has no musical clock. Callers transfer snapshot ownership on
//! successful adoption/publication and must not release those snapshots separately.
//! All methods after preparation use fixed storage. Quiescent cleanup is explicit.
use crate::LoopId;
use rt_runtime::{resource::*, Error, Frame, Revision};
use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;

static NEXT_SCOPE: AtomicU32 = AtomicU32::new(1);
macro_rules! key {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $name {
            scope: u32,
            slot: u32,
            generation: u32,
        }
        impl $name {
            pub fn parts(self) -> [u32; 3] {
                [self.scope, self.slot, self.generation]
            }
            pub fn from_parts([scope, slot, generation]: [u32; 3]) -> Result<Self, Error> {
                if scope == 0 || generation == 0 {
                    return Err(Error::InvalidId);
                }
                Ok(Self {
                    scope,
                    slot,
                    generation,
                })
            }
        }
    };
}
key!(MediaId);
key!(ExportId);
key!(PinId);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Description {
    pub owner: LoopId,
    pub revision: Revision,
    pub frames: u64,
    pub channels: u32,
    pub midi: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Source {
    pub snapshot: Snapshot,
    pub index: ReadIndex,
}
/// Snapshot ownership transfers only on success. A failed operation returns this input.
#[derive(Debug)]
pub struct Input {
    pub description: Description,
    pub audio: Option<Source>,
    pub midi: Option<Source>,
}
#[derive(Debug)]
pub struct Rejected {
    pub error: Error,
    pub input: Input,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct View {
    pub description: Description,
    pub audio: Option<Source>,
    pub midi: Option<Source>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Export {
    pub media: MediaId,
    pub requested_cut: Frame,
    pub view: View,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Usage {
    pub free_media: usize,
    pub free_exports: usize,
    pub free_pins: usize,
    pub references: usize,
}

#[derive(Clone, Copy)]
struct Media {
    view: View,
    ready: bool,
    resident: bool,
    references: u32,
}
#[derive(Clone, Copy)]
struct Slot<T: Copy> {
    generation: u32,
    value: Option<T>,
}
#[derive(Clone, Copy)]
struct ExportRef {
    media: MediaId,
    cut: Frame,
}
#[derive(Clone, Copy)]
struct Owner {
    id: LoopId,
    playback: Option<MediaId>,
    latest: Option<MediaId>,
}
#[derive(Clone, Copy)]
struct OwnerSlot {
    watermark: u32,
    value: Option<Owner>,
}

/// Each media slot owns its underlying audio/MIDI leases once. Internal playback
/// and latest-take references cannot be consumed by external export saturation.
/// A resident reference belongs to the adopter/reserver until `retire` succeeds.
pub struct Ledger {
    scope: u32,
    media: Box<[Slot<Media>]>,
    exports: Box<[Slot<ExportRef>]>,
    pins: Box<[Slot<MediaId>]>,
    owners: Box<[OwnerSlot]>,
    free_media: Vec<usize>,
    free_exports: Vec<usize>,
    free_pins: Vec<usize>,
}
impl Ledger {
    pub fn prepare(
        loop_slots: usize,
        media_slots: usize,
        pin_slots: usize,
        export_slots: usize,
    ) -> Result<Self, Error> {
        let bytes = loop_slots
            .checked_mul(std::mem::size_of::<OwnerSlot>())
            .and_then(|n| {
                media_slots
                    .checked_mul(std::mem::size_of::<Slot<Media>>() + std::mem::size_of::<usize>())
                    .and_then(|m| n.checked_add(m))
            })
            .and_then(|n| {
                export_slots
                    .checked_mul(
                        std::mem::size_of::<Slot<ExportRef>>() + std::mem::size_of::<usize>(),
                    )
                    .and_then(|m| n.checked_add(m))
            })
            .and_then(|n| {
                pin_slots
                    .checked_mul(
                        std::mem::size_of::<Slot<MediaId>>() + std::mem::size_of::<usize>(),
                    )
                    .and_then(|m| n.checked_add(m))
            })
            .ok_or(Error::Capacity)?;
        if loop_slots == 0
            || media_slots == 0
            || bytes > i32::MAX as usize
            || [loop_slots, media_slots, pin_slots, export_slots]
                .into_iter()
                .any(|n| n > u32::MAX as usize)
        {
            return Err(Error::Capacity);
        }
        let scope = NEXT_SCOPE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Exhausted)?;
        let mut ledger = Self {
            scope,
            media: vec![
                Slot {
                    generation: 1,
                    value: None
                };
                media_slots
            ]
            .into_boxed_slice(),
            exports: vec![
                Slot {
                    generation: 1,
                    value: None
                };
                export_slots
            ]
            .into_boxed_slice(),
            pins: vec![
                Slot {
                    generation: 1,
                    value: None
                };
                pin_slots
            ]
            .into_boxed_slice(),
            owners: vec![
                OwnerSlot {
                    watermark: 0,
                    value: None
                };
                loop_slots
            ]
            .into_boxed_slice(),
            free_media: (0..media_slots).rev().collect(),
            free_exports: (0..export_slots).rev().collect(),
            free_pins: (0..pin_slots).rev().collect(),
        };
        for value in ledger.media.iter_mut() {
            std::hint::black_box(value);
        }
        for value in ledger.exports.iter_mut() {
            std::hint::black_box(value);
        }
        for value in ledger.pins.iter_mut() {
            std::hint::black_box(value);
        }
        for value in ledger.owners.iter_mut() {
            std::hint::black_box(value);
        }
        Ok(ledger)
    }
    pub fn bind(&mut self, id: LoopId) -> Result<(), Error> {
        let slot = self
            .owners
            .get_mut(id.slot() as usize)
            .ok_or(Error::InvalidId)?;
        if slot.value.is_some() || id.generation() <= slot.watermark {
            return Err(Error::InvalidId);
        }
        slot.watermark = id.generation();
        slot.value = Some(Owner {
            id,
            playback: None,
            latest: None,
        });
        Ok(())
    }
    fn owner(&self, id: LoopId) -> Result<Owner, Error> {
        self.owners
            .get(id.slot() as usize)
            .and_then(|s| s.value)
            .filter(|o| o.id == id)
            .ok_or(Error::InvalidId)
    }
    fn media(&self, id: MediaId) -> Result<Media, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        self.media
            .get(id.slot as usize)
            .filter(|s| s.generation == id.generation)
            .and_then(|s| s.value)
            .ok_or(Error::InvalidId)
    }
    fn export_ref(&self, id: ExportId) -> Result<ExportRef, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        self.exports
            .get(id.slot as usize)
            .filter(|s| s.generation == id.generation)
            .and_then(|s| s.value)
            .ok_or(Error::InvalidId)
    }
    fn allocate(&mut self, view: View, ready: bool) -> Result<MediaId, Error> {
        self.owner(view.description.owner)?;
        let slot = self.free_media.pop().ok_or(Error::Capacity)?;
        self.media[slot].value = Some(Media {
            view,
            ready,
            resident: true,
            references: 1,
        });
        Ok(MediaId {
            scope: self.scope,
            slot: slot as u32,
            generation: self.media[slot].generation,
        })
    }
    /// Reserve metadata before promising a new take. Its final duration is supplied at publication.
    pub fn reserve(&mut self, description: Description) -> Result<MediaId, Error> {
        if description.frames != 0 {
            return Err(Error::InvalidShape);
        }
        self.allocate(
            View {
                description,
                audio: None,
                midi: None,
            },
            false,
        )
    }
    fn validate(&self, io: &mut ResourceIo<'_>, input: &Input) -> Result<(), Error> {
        for slot in &self.media {
            if let Some(media) = slot.value {
                Self::check_sources(io, media)?;
                for existing in [media.view.audio, media.view.midi].into_iter().flatten() {
                    if [input.audio, input.midi]
                        .into_iter()
                        .flatten()
                        .any(|s| s.snapshot.id == existing.snapshot.id)
                    {
                        return Err(Error::InvalidState);
                    }
                }
            }
        }
        let description = input.description;
        for (source, format, required) in [
            (
                input.audio,
                Format::Audio {
                    channels: description.channels,
                },
                description.channels > 0,
            ),
            (input.midi, Format::Midi, description.midi),
        ] {
            match source {
                None if required && description.frames > 0 => return Err(Error::InvalidShape),
                Some(_) if !required => return Err(Error::InvalidShape),
                Some(source) => {
                    if io.render().snapshot(source.index)? != source.snapshot {
                        return Err(Error::InvalidId);
                    }
                    if source.snapshot.description.format != format
                        || source.snapshot.description.frames != description.frames
                    {
                        return Err(Error::InvalidShape);
                    }
                    if format == Format::Midi {
                        io.render().midi_seek(source.index, 0)?;
                    }
                }
                None => {}
            }
        }
        Ok(())
    }
    pub fn adopt(&mut self, io: &mut ResourceIo<'_>, input: Input) -> Result<MediaId, Rejected> {
        let result = (|| {
            self.owner(input.description.owner)?;
            if self.free_media.is_empty() {
                return Err(Error::Capacity);
            }
            self.validate(io, &input)?;
            self.allocate(
                View {
                    description: input.description,
                    audio: input.audio,
                    midi: input.midi,
                },
                true,
            )
        })();
        result.map_err(|error| Rejected { error, input })
    }
    pub fn publish(
        &mut self,
        io: &mut ResourceIo<'_>,
        id: MediaId,
        input: Input,
    ) -> Result<(), Rejected> {
        let result = (|| {
            let media = self.media(id)?;
            if media.ready || !media.resident {
                return Err(Error::InvalidState);
            }
            if (Description {
                frames: input.description.frames,
                ..media.view.description
            }) != input.description
            {
                return Err(Error::InvalidShape);
            }
            self.validate(io, &input)?;
            self.media[id.slot as usize].value = Some(Media {
                view: View {
                    description: input.description,
                    audio: input.audio,
                    midi: input.midi,
                },
                ready: true,
                ..media
            });
            Ok(())
        })();
        result.map_err(|error| Rejected { error, input })
    }
    pub fn view(&self, id: MediaId) -> Result<View, Error> {
        let media = self.media(id)?;
        if !media.ready {
            return Err(Error::InvalidState);
        }
        Ok(media.view)
    }
    fn check_sources(io: &mut ResourceIo<'_>, media: Media) -> Result<(), Error> {
        for source in [media.view.audio, media.view.midi].into_iter().flatten() {
            if io.render().snapshot(source.index)? != source.snapshot {
                return Err(Error::InvalidId);
            }
        }
        Ok(())
    }
    fn check_drop(&self, io: &mut ResourceIo<'_>, id: MediaId) -> Result<(), Error> {
        let media = self.media(id)?;
        if media.references == 1 {
            Self::check_sources(io, media)?;
        }
        Ok(())
    }
    fn add_ref(&mut self, id: MediaId) -> Result<(), Error> {
        let mut media = self.media(id)?;
        media.references = media.references.checked_add(1).ok_or(Error::Exhausted)?;
        self.media[id.slot as usize].value = Some(media);
        Ok(())
    }
    fn drop_ref(&mut self, io: &mut ResourceIo<'_>, id: MediaId) -> Result<(), Error> {
        let mut media = self.media(id)?;
        if media.references > 1 {
            media.references -= 1;
            self.media[id.slot as usize].value = Some(media);
            return Ok(());
        }
        Self::check_sources(io, media)?;
        for source in [media.view.audio, media.view.midi].into_iter().flatten() {
            io.release_snapshot(source.snapshot.id)?;
        }
        let slot = &mut self.media[id.slot as usize];
        slot.value = None;
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            self.free_media.push(id.slot as usize);
        } else {
            slot.generation = 0;
        }
        Ok(())
    }
    /// Release the single resident/prepared reference, never a playback/export reference.
    pub fn retire(&mut self, io: &mut ResourceIo<'_>, id: MediaId) -> Result<(), Error> {
        let mut media = self.media(id)?;
        if !media.resident {
            return Err(Error::InvalidState);
        }
        self.check_drop(io, id)?;
        media.resident = false;
        self.media[id.slot as usize].value = Some(media);
        self.drop_ref(io, id)
    }
    fn select(
        &mut self,
        io: &mut ResourceIo<'_>,
        owner: LoopId,
        id: Option<MediaId>,
        latest: bool,
    ) -> Result<(), Error> {
        let mut row = self.owner(owner)?;
        let old = if latest { row.latest } else { row.playback };
        if let Some(id) = id {
            if self.view(id)?.description.owner != owner {
                return Err(Error::InvalidId);
            }
        }
        if old == id {
            return Ok(());
        }
        if let Some(old) = old {
            self.check_drop(io, old)?;
        }
        if let Some(id) = id {
            self.add_ref(id)?;
        }
        if latest {
            row.latest = id;
        } else {
            row.playback = id;
        }
        self.owners[owner.slot() as usize].value = Some(row);
        if let Some(old) = old {
            self.drop_ref(io, old)?;
        }
        Ok(())
    }
    pub fn set_playback(
        &mut self,
        io: &mut ResourceIo<'_>,
        owner: LoopId,
        id: Option<MediaId>,
    ) -> Result<(), Error> {
        self.select(io, owner, id, false)
    }
    pub fn set_latest(
        &mut self,
        io: &mut ResourceIo<'_>,
        owner: LoopId,
        id: Option<MediaId>,
    ) -> Result<(), Error> {
        self.select(io, owner, id, true)
    }
    pub fn playback(&self, owner: LoopId) -> Result<Option<MediaId>, Error> {
        Ok(self.owner(owner)?.playback)
    }
    pub fn latest(&self, owner: LoopId) -> Result<Option<MediaId>, Error> {
        Ok(self.owner(owner)?.latest)
    }
    /// Retain pending-request media independently of external export credits.
    pub fn pin(&mut self, id: MediaId) -> Result<PinId, Error> {
        self.view(id)?;
        let slot = *self.free_pins.last().ok_or(Error::Capacity)?;
        self.add_ref(id)?;
        self.free_pins.pop();
        self.pins[slot].value = Some(id);
        Ok(PinId {
            scope: self.scope,
            slot: slot as u32,
            generation: self.pins[slot].generation,
        })
    }
    pub fn pinned(&self, id: PinId) -> Result<MediaId, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        self.pins
            .get(id.slot as usize)
            .filter(|s| s.generation == id.generation)
            .and_then(|s| s.value)
            .ok_or(Error::InvalidId)
    }
    pub fn release_pin(&mut self, io: &mut ResourceIo<'_>, id: PinId) -> Result<(), Error> {
        let media = self.pinned(id)?;
        self.check_drop(io, media)?;
        self.drop_ref(io, media)?;
        let slot = &mut self.pins[id.slot as usize];
        slot.value = None;
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            self.free_pins.push(id.slot as usize);
        } else {
            slot.generation = 0;
        }
        Ok(())
    }
    pub fn export(&mut self, id: MediaId, requested_cut: Frame) -> Result<ExportId, Error> {
        self.view(id)?;
        let slot = *self.free_exports.last().ok_or(Error::Capacity)?;
        self.add_ref(id)?;
        self.free_exports.pop();
        self.exports[slot].value = Some(ExportRef {
            media: id,
            cut: requested_cut,
        });
        Ok(ExportId {
            scope: self.scope,
            slot: slot as u32,
            generation: self.exports[slot].generation,
        })
    }
    pub fn exported(&self, id: ExportId) -> Result<Export, Error> {
        let export = self.export_ref(id)?;
        Ok(Export {
            media: export.media,
            requested_cut: export.cut,
            view: self.view(export.media)?,
        })
    }
    pub fn release_export(&mut self, io: &mut ResourceIo<'_>, id: ExportId) -> Result<(), Error> {
        let export = self.export_ref(id)?;
        self.check_drop(io, export.media)?;
        self.drop_ref(io, export.media)?;
        let slot = &mut self.exports[id.slot as usize];
        slot.value = None;
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            self.free_exports.push(id.slot as usize);
        } else {
            slot.generation = 0;
        }
        Ok(())
    }
    /// Remove internal roles. Resident content and outstanding exports remain owned.
    pub fn unbind(&mut self, io: &mut ResourceIo<'_>, id: LoopId) -> Result<(), Error> {
        let owner = self.owner(id)?;
        for media in [owner.playback, owner.latest].into_iter().flatten() {
            Self::check_sources(io, self.media(media)?)?;
        }
        self.set_playback(io, id, None)?;
        self.set_latest(io, id, None)?;
        self.owners[id.slot() as usize].value = None;
        Ok(())
    }
    /// Explicit quiescent cleanup. This does not stop music or send MIDI cleanup.
    pub fn release_all(&mut self, io: &mut ResourceIo<'_>) -> Result<(), Error> {
        for slot in &self.media {
            if let Some(media) = slot.value {
                Self::check_sources(io, media)?;
            }
        }
        for i in 0..self.owners.len() {
            if let Some(owner) = self.owners[i].value {
                self.unbind(io, owner.id)?;
            }
        }
        for i in 0..self.exports.len() {
            if self.exports[i].value.is_some() {
                self.release_export(
                    io,
                    ExportId {
                        scope: self.scope,
                        slot: i as u32,
                        generation: self.exports[i].generation,
                    },
                )?;
            }
        }
        for i in 0..self.pins.len() {
            if self.pins[i].value.is_some() {
                self.release_pin(
                    io,
                    PinId {
                        scope: self.scope,
                        slot: i as u32,
                        generation: self.pins[i].generation,
                    },
                )?;
            }
        }
        for i in 0..self.media.len() {
            if self.media[i].value.is_some_and(|m| m.resident) {
                self.retire(
                    io,
                    MediaId {
                        scope: self.scope,
                        slot: i as u32,
                        generation: self.media[i].generation,
                    },
                )?;
            }
        }
        Ok(())
    }
    pub fn usage(&self) -> Usage {
        Usage {
            free_media: self.free_media.len(),
            free_exports: self.free_exports.len(),
            free_pins: self.free_pins.len(),
            references: self
                .media
                .iter()
                .filter_map(|s| s.value)
                .map(|m| m.references as usize)
                .sum(),
        }
    }
}
