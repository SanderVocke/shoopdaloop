use super::*;
use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(test)]
#[path = "roles_tests.rs"]
mod tests;

static NEXT_SCOPE: AtomicU32 = AtomicU32::new(1);
macro_rules! identity {
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
identity!(RoleStageId);
identity!(RoleSourceId);
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RoleSettings {
    pub candidates: usize,
    pub sources: usize,
    pub source_packets_per_callback: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoleSourceStatus {
    Validating { packets: usize },
    Ready { max_packet: usize },
    Failed(Error),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoleStatus {
    Idle,
    Receiving { received: usize, expected: usize },
    Ready,
    Taken,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoleInitial {
    pub target: LoopId,
    pub frames: u64,
    pub audio: Option<RoleSourceId>,
    pub midi: Option<RoleSourceId>,
    pub gain: f32,
    pub preserve_input: bool,
}
#[derive(Clone, Copy)]
pub(super) struct Edit {
    pub previous: Option<LoopId>,
    pub initial: Option<RoleInitial>,
}
impl Edit {
    pub(super) fn slot(self) -> Result<usize, Error> {
        match (self.previous, self.initial) {
            (Some(old), Some(new)) if old.slot() != new.target.slot() => Err(Error::InvalidShape),
            (_, Some(new)) => Ok(new.target.slot() as usize),
            (Some(old), None) => Ok(old.slot() as usize),
            _ => Err(Error::InvalidShape),
        }
    }
}
struct SourceSlot {
    generation: u32,
    source: Option<media::Source>,
    cursor: Option<MidiCursor>,
    packets: usize,
    max_packet: usize,
    failure: Option<Error>,
}
pub(super) struct Bank {
    generation: u32,
    status: RoleStatus,
    pub edits: Vec<Edit>,
    pub changes: Vec<media::OwnerChange>,
}
pub(super) struct Roles {
    scope: u32,
    loops: usize,
    banks: Box<[Bank]>,
    sources: Box<[SourceSlot]>,
    settings: RoleSettings,
    callback: Option<Frame>,
    steps: usize,
}
impl Roles {
    pub(super) fn prepare(loops: usize, settings: RoleSettings) -> Result<Self, Error> {
        if (settings.candidates == 0 && settings.sources != 0)
            || (settings.sources == 0) != (settings.source_packets_per_callback == 0)
        {
            return Err(Error::InvalidConfig);
        }
        settings
            .source_packets_per_callback
            .checked_mul(12)
            .filter(|n| *n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let bytes = loops
            .checked_mul(size_of::<Edit>() + size_of::<media::OwnerChange>())
            .and_then(|n| n.checked_add(size_of::<Bank>()))
            .and_then(|n| n.checked_mul(settings.candidates))
            .and_then(|n| {
                settings
                    .sources
                    .checked_mul(size_of::<SourceSlot>())
                    .and_then(|m| n.checked_add(m))
            })
            .filter(|n| *n <= i32::MAX as usize)
            .ok_or(Error::Capacity)?;
        let _ = bytes;
        if settings.candidates > u32::MAX as usize || settings.sources > u32::MAX as usize {
            return Err(Error::Capacity);
        }
        let scope = if settings.candidates == 0 {
            0
        } else {
            NEXT_SCOPE
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .map_err(|_| Error::Exhausted)?
        };
        let mut banks = Vec::with_capacity(settings.candidates);
        for _ in 0..settings.candidates {
            let mut edits = vec![
                Edit {
                    previous: None,
                    initial: None
                };
                loops
            ];
            let mut changes: Vec<_> = (0..loops)
                .map(|_| {
                    media::OwnerChange::remove(LoopId::from_parts(0, 1).expect("fixed identity"))
                })
                .collect();
            for value in &mut edits {
                std::hint::black_box(value);
            }
            for value in &mut changes {
                std::hint::black_box(value);
            }
            edits.clear();
            changes.clear();
            banks.push(Bank {
                generation: 0,
                status: RoleStatus::Idle,
                edits,
                changes,
            });
        }
        let mut sources: Vec<_> = (0..settings.sources)
            .map(|_| SourceSlot {
                generation: 1,
                source: None,
                cursor: None,
                packets: 0,
                max_packet: 0,
                failure: None,
            })
            .collect();
        for value in &mut sources {
            std::hint::black_box(value);
        }
        Ok(Self {
            scope,
            loops,
            banks: banks.into_boxed_slice(),
            sources: sources.into_boxed_slice(),
            settings,
            callback: None,
            steps: 0,
        })
    }
    pub(super) fn source(&self, id: RoleSourceId) -> Result<media::Source, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        self.sources
            .get(id.slot as usize)
            .filter(|s| s.generation == id.generation)
            .and_then(|s| s.source)
            .ok_or(Error::InvalidId)
    }
    pub(super) fn callback(&mut self, frame: Frame) {
        if self.callback != Some(frame) {
            self.callback = Some(frame);
            self.steps = self.settings.source_packets_per_callback;
        }
    }
    pub(super) fn source_status(&self, id: RoleSourceId) -> Result<RoleSourceStatus, Error> {
        self.source(id)?;
        let slot = &self.sources[id.slot as usize];
        Ok(if let Some(error) = slot.failure {
            RoleSourceStatus::Failed(error)
        } else if slot.cursor.is_some() {
            RoleSourceStatus::Validating {
                packets: slot.packets,
            }
        } else {
            RoleSourceStatus::Ready {
                max_packet: slot.max_packet,
            }
        })
    }
    pub(super) fn validate_source(
        &mut self,
        id: RoleSourceId,
        io: &mut ResourceIo<'_>,
    ) -> Result<RoleSourceStatus, Error> {
        self.source(id)?;
        let slot = &mut self.sources[id.slot as usize];
        if let Some(error) = slot.failure {
            return Err(error);
        }
        while self.steps > 0 {
            let Some(cursor) = slot.cursor.as_mut() else {
                break;
            };
            self.steps -= 1;
            match io.render().midi_next(cursor) {
                Ok(Some(event)) => {
                    slot.max_packet = slot.max_packet.max(event.byte_len());
                    slot.packets += 1;
                }
                Ok(None) => slot.cursor = None,
                Err(error) => {
                    slot.failure = Some(error);
                    return Err(error);
                }
            }
        }
        self.source_status(id)
    }
    fn take_source(&mut self, id: RoleSourceId) -> media::Source {
        let slot = &mut self.sources[id.slot as usize];
        let source = slot.source.take().expect("validated source");
        slot.generation = slot.generation.checked_add(1).unwrap_or(0);
        source
    }
    pub(super) fn snapshot(
        &mut self,
        resource: rt_runtime::ResourceId,
        frame: Frame,
        io: &mut ResourceIo<'_>,
    ) -> Result<(RoleSourceId, Snapshot), Error> {
        let index = self
            .sources
            .iter()
            .position(|s| s.generation != 0 && s.source.is_none())
            .ok_or(Error::Capacity)?;
        let snapshot = io.snapshot(resource, frame)?;
        let source = media::Source {
            snapshot,
            index: io.read_index(snapshot.id).expect("new snapshot"),
        };
        let cursor = if snapshot.description.format == Format::Midi {
            if let Err(error) = io.render().midi_seek(source.index, 0) {
                io.release_snapshot(snapshot.id)
                    .expect("new owned snapshot");
                return Err(error);
            }
            Some(
                io.render()
                    .midi_cursor(source.index)
                    .expect("validated MIDI index"),
            )
        } else {
            None
        };
        let slot = &mut self.sources[index];
        slot.source = Some(source);
        slot.cursor = cursor;
        slot.max_packet = 0;
        slot.packets = 0;
        slot.failure = None;
        Ok((
            RoleSourceId {
                scope: self.scope,
                slot: index as u32,
                generation: slot.generation,
            },
            snapshot,
        ))
    }
    pub(super) fn release_source(
        &mut self,
        id: RoleSourceId,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        let source = self.source(id)?;
        if io.render().snapshot(source.index)? != source.snapshot {
            return Err(Error::InvalidId);
        }
        io.release_snapshot(source.snapshot.id)?;
        self.take_source(id);
        Ok(())
    }
    pub(super) fn bank(&self, id: RoleStageId) -> Result<&Bank, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        self.banks
            .get(id.slot as usize)
            .filter(|s| s.generation == id.generation)
            .ok_or(Error::InvalidId)
    }
    pub(super) fn bank_mut(&mut self, id: RoleStageId) -> Result<&mut Bank, Error> {
        self.bank(id)?;
        Ok(&mut self.banks[id.slot as usize])
    }
    pub(super) fn status(&self, id: RoleStageId) -> Result<RoleStatus, Error> {
        Ok(self.bank(id)?.status)
    }
    pub(super) fn begin(&mut self, slot: usize, count: usize) -> Result<RoleStageId, Error> {
        let bank = self.banks.get_mut(slot).ok_or(Error::InvalidId)?;
        if bank.status != RoleStatus::Idle {
            return Err(Error::Busy);
        }
        if count > self.loops {
            return Err(Error::Capacity);
        }
        bank.generation = bank.generation.checked_add(1).ok_or(Error::Exhausted)?;
        bank.status = if count == 0 {
            RoleStatus::Ready
        } else {
            RoleStatus::Receiving {
                received: 0,
                expected: count,
            }
        };
        Ok(RoleStageId {
            scope: self.scope,
            slot: slot as u32,
            generation: bank.generation,
        })
    }
    pub(super) fn append(
        &mut self,
        id: RoleStageId,
        edit: Edit,
        binding: Binding,
        ledger: &mut media::Ledger,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        let bank = self.bank(id)?;
        let RoleStatus::Receiving { received, expected } = bank.status else {
            return Err(Error::InvalidState);
        };
        let slot = edit.slot()?;
        if bank.edits.iter().any(|e| e.slot() == Ok(slot)) {
            return Err(Error::InvalidId);
        }
        let mut change = if let Some(initial) = edit.initial {
            if !initial.gain.is_finite()
                || (initial.audio.is_some() && initial.audio == initial.midi)
            {
                return Err(Error::InvalidShape);
            }
            for source in [initial.audio, initial.midi].into_iter().flatten() {
                let RoleSourceStatus::Ready { max_packet } = self.source_status(source)? else {
                    return Err(Error::InvalidState);
                };
                if max_packet > binding.packet_bytes {
                    return Err(Error::Capacity);
                }
            }
            let audio = initial.audio.map(|id| self.source(id)).transpose()?;
            let midi = initial.midi.map(|id| self.source(id)).transpose()?;
            media::OwnerChange::replace(
                edit.previous,
                media::Input {
                    description: media::Description {
                        owner: initial.target,
                        revision: Revision(0),
                        frames: initial.frames,
                        channels: binding.channels as u32,
                        midi: binding.midi,
                    },
                    audio,
                    midi,
                },
            )
        } else {
            media::OwnerChange::remove(edit.previous.ok_or(Error::InvalidId)?)
        };
        {
            let _permit = ledger.prepare_owner_activation(io, std::slice::from_mut(&mut change))?;
        }
        if let Some(initial) = edit.initial {
            for source in [initial.audio, initial.midi].into_iter().flatten() {
                self.take_source(source);
            }
        }
        let bank = self.bank_mut(id)?;
        bank.edits.push(edit);
        bank.changes.push(change);
        bank.status = if received + 1 == expected {
            RoleStatus::Ready
        } else {
            RoleStatus::Receiving {
                received: received + 1,
                expected,
            }
        };
        Ok(())
    }
    pub(super) fn applied(&mut self, id: RoleStageId) {
        self.bank_mut(id).expect("validated bank").status = RoleStatus::Taken;
    }
    pub(super) fn cancel(&mut self, id: RoleStageId, io: &mut ResourceIo<'_>) -> Result<(), Error> {
        let bank = self.bank_mut(id)?;
        if matches!(bank.status, RoleStatus::Taken | RoleStatus::Idle) {
            return Err(Error::InvalidState);
        }
        for input in bank.changes.iter().filter_map(|c| c.input()) {
            for source in [input.audio, input.midi].into_iter().flatten() {
                if io.render().snapshot(source.index)? != source.snapshot {
                    return Err(Error::InvalidId);
                }
            }
        }
        while let Some(change) = bank.changes.pop() {
            if let Some(input) = change.into_input() {
                for source in [input.audio, input.midi].into_iter().flatten() {
                    io.release_snapshot(source.snapshot.id)
                        .expect("validated owned source");
                }
            }
        }
        bank.edits.clear();
        bank.status = RoleStatus::Idle;
        Ok(())
    }
    pub(super) fn recycle(&mut self, id: RoleStageId) -> Result<(), Error> {
        let bank = self.bank_mut(id)?;
        if bank.status != RoleStatus::Taken {
            return Err(Error::InvalidState);
        }
        bank.changes.clear();
        bank.edits.clear();
        bank.status = RoleStatus::Idle;
        Ok(())
    }
    pub(super) fn cleanup(&mut self, io: &mut ResourceIo<'_>) -> Result<(), Error> {
        for index in 0..self.banks.len() {
            let bank = &self.banks[index];
            let id = RoleStageId {
                scope: self.scope,
                slot: index as u32,
                generation: bank.generation,
            };
            match bank.status {
                RoleStatus::Idle => {}
                RoleStatus::Taken => self.recycle(id)?,
                _ => self.cancel(id, io)?,
            }
        }
        for index in 0..self.sources.len() {
            let slot = &self.sources[index];
            if slot.source.is_some() {
                self.release_source(
                    RoleSourceId {
                        scope: self.scope,
                        slot: index as u32,
                        generation: slot.generation,
                    },
                    io,
                )?;
            }
        }
        Ok(())
    }
}
