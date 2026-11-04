//! Shared MIDI revisions behind ordinary resource and immutable snapshot IDs.
use super::*;
use crate::{buffer::MidiBuffer, midi_state::MidiState};
use core::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiStoreId {
    pub(super) scope: u32,
    pub(super) slot: usize,
}
#[derive(Clone, Copy)]
pub(super) struct Take {
    pub store: MidiStoreId,
    pub take: shared_midi::TakeId,
}
#[derive(Clone, Copy)]
pub(super) struct Export {
    pub store: MidiStoreId,
    pub export: shared_midi::ExportId,
    pub index: shared_midi::ReadIndex,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiEditId {
    store: MidiStoreId,
    edit: shared_midi::EditId,
    resource: ResourceId,
    version: UploadId,
    expected: Revision,
    revision: Revision,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiImportId {
    store: MidiStoreId,
    import: shared_midi::ImportId,
}

impl ResourceStore {
    pub fn midi_coverage(&self, id: MidiStoreId) -> Result<Range<Frame>, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        self.midi_stores
            .get(id.slot)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidId)?
            .coverage()
    }
    pub fn midi_store_capacity(&mut self, id: MidiStoreId) -> Result<shared_midi::Capacity, Error> {
        self.with_midi(id, |_, midi| Ok(midi.capacity()))
    }
    pub fn midi_store_usage(&mut self, id: MidiStoreId) -> Result<shared_midi::Usage, Error> {
        self.with_midi(id, |_, midi| Ok(midi.usage()))
    }
    pub fn midi_store_work(&mut self, id: MidiStoreId) -> Result<shared_midi::Work, Error> {
        self.with_midi(id, |_, midi| Ok(midi.work()))
    }
    pub fn prepare_midi_import(
        &mut self,
        store: MidiStoreId,
        snapshot: SnapshotId,
    ) -> Result<MidiImportId, Error> {
        let import = self.with_midi(store, |resources, midi| {
            midi.prepare_import(&mut resources.render_io(), snapshot)
        })?;
        Ok(MidiImportId { store, import })
    }
    pub fn midi_import_status(
        &mut self,
        id: MidiImportId,
    ) -> Result<shared_midi::EditStatus, Error> {
        self.with_midi(id.store, |_, midi| midi.import_status(id.import))
    }
    pub fn cancel_midi_import(&mut self, id: MidiImportId) -> Result<(), Error> {
        self.with_midi(id.store, |_, midi| midi.cancel_import(id.import))
    }
    pub fn finish_midi_import(&mut self, id: MidiImportId) -> Result<ResourceId, Error> {
        self.midi_resource_credit()?;
        let take = self.with_midi(id.store, |_, midi| midi.finish_import(id.import))?;
        self.midi_resource(Take {
            store: id.store,
            take,
        })
    }
    pub fn prepare_midi_store(
        &mut self,
        config: shared_midi::Config,
        origin: Frame,
        initial: &MidiState,
    ) -> Result<MidiStoreId, Error> {
        let slot = self.midi_stores.len();
        if slot == self.config.limits().resource_slots {
            return Err(Error::Capacity);
        }
        let midi = shared_midi::MidiStore::prepare(self, config, origin, initial)?;
        self.midi_stores.push(Some(Box::new(midi)));
        Ok(MidiStoreId {
            scope: self.scope,
            slot,
        })
    }
    pub(super) fn with_midi<T>(
        &mut self,
        id: MidiStoreId,
        operation: impl FnOnce(&mut ResourceStore, &mut shared_midi::MidiStore) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        let mut midi = self
            .midi_stores
            .get_mut(id.slot)
            .and_then(Option::take)
            .ok_or(Error::InvalidId)?;
        let result = operation(self, &mut midi);
        self.midi_stores[id.slot] = Some(midi);
        result
    }
    pub fn midi_coverage(&self, id: MidiStoreId) -> Result<Range<Frame>, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        self.midi_stores
            .get(id.slot)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidId)?
            .coverage()
    }
    pub fn cleanup_midi_store(&mut self, id: MidiStoreId) -> Result<(), Error> {
        self.with_midi(id, |_, midi| midi.begin_cleanup())
    }
    fn midi_resource_credit(&self) -> Result<(), Error> {
        if self.free_resources.is_empty() || self.free_versions.is_empty() {
            Err(Error::Capacity)
        } else {
            Ok(())
        }
    }
    fn midi_resource(&mut self, take: Take) -> Result<ResourceId, Error> {
        let info = self.with_midi(take.store, |_, midi| midi.info(midi.take_index(take.take)?))?;
        let resource = self.create()?;
        let slot = self.free_versions.pop().expect("preflighted version");
        let active = UploadId {
            slot: slot as u32,
            generation: self.versions[slot].generation,
        };
        let revision = Revision(1);
        self.versions[slot].value = Some(Version {
            resource,
            expected_revision: Revision(0),
            revision,
            description: Description {
                format: Format::Midi,
                frames: info.frames,
                bytes: info.bytes,
            },
            written: 0,
            reserved: 0,
            head: None,
            tail: None,
            references: 1,
            parser: Parser::default(),
            state: State::Active,
            capture: None,
            index: VersionIndex::default(),
            midi: None,
            audio: None,
            shared_midi: Some(take),
        });
        self.resources[resource.slot() as usize].value = Some(Resource {
            active: Some(active),
            revision,
            claim: None,
        });
        Ok(resource)
    }
    pub fn grab_midi(
        &mut self,
        store: MidiStoreId,
        range: Range<Frame>,
    ) -> Result<ResourceId, Error> {
        self.midi_resource_credit()?;
        let take = self.with_midi(store, |resources, midi| {
            midi.grab(&mut resources.render_io(), range)
        })?;
        self.midi_resource(Take { store, take })
    }
    pub fn fork_midi(
        &mut self,
        store: MidiStoreId,
        snapshot: SnapshotId,
    ) -> Result<ResourceId, Error> {
        self.midi_resource_credit()?;
        let lease = get(&self.snapshots, snapshot.slot, snapshot.generation)?;
        let source = lease.shared_midi.ok_or(Error::InvalidShape)?;
        if source.store != store {
            return Err(Error::InvalidShape);
        }
        let take = self.with_midi(store, |_, midi| midi.fork_export(source.export))?;
        self.midi_resource(Take { store, take })
    }
    pub fn midi_edit_status(&mut self, id: MidiEditId) -> Result<shared_midi::EditStatus, Error> {
        self.with_midi(id.store, |_, midi| midi.edit_status(id.edit))
    }
    pub fn cancel_midi_edit(&mut self, id: MidiEditId) -> Result<(), Error> {
        self.with_midi(id.store, |_, midi| midi.cancel_edit(id.edit))
    }
    pub(super) fn maintain_midi(&mut self) -> bool {
        let count = self.midi_stores.len();
        for _ in 0..count {
            let slot = self.midi_maintenance;
            self.midi_maintenance = (slot + 1) % count;
            if self.midi_stores[slot].is_some()
                && self
                    .with_midi(
                        MidiStoreId {
                            scope: self.scope,
                            slot,
                        },
                        |store, midi| midi.maintain_limit(store, 1),
                    )
                    .expect("prepared MIDI store")
                    > 0
            {
                return true;
            }
        }
        false
    }
}

impl RenderIo<'_> {
    pub fn progress_midi_import(
        &mut self,
        id: MidiImportId,
        quantum: usize,
    ) -> Result<shared_midi::EditStatus, Error> {
        self.store.with_midi(id.store, |store, midi| {
            midi.progress_imported(&mut store.render_io(), id.import, quantum)
        })
    }
    pub fn append_midi_history(
        &mut self,
        id: MidiStoreId,
        frame: Frame,
        frames: usize,
        input: &MidiBuffer,
    ) -> Result<(), Error> {
        self.store.with_midi(id, |store, midi| {
            midi.append(&mut store.render_io(), frame, frames, input)
        })
    }
    pub fn midi_history_discontinuity(
        &mut self,
        id: MidiStoreId,
        next: Frame,
        observed: &MidiState,
    ) -> Result<(), Error> {
        self.store.with_midi(id, |store, midi| {
            midi.discontinuity(&mut store.render_io(), next, observed)
        })
    }
    pub fn prepare_midi_replace(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        position: u64,
        frames: usize,
        input: &MidiBuffer,
        initial: &MidiState,
    ) -> Result<MidiEditId, Error> {
        let owner = self.store.check_writer(resource, None)?;
        if owner.revision != expected {
            return Err(Error::StaleRevision);
        }
        let version = owner.active.ok_or(Error::InvalidState)?;
        let take = get(&self.store.versions, version.slot, version.generation)?
            .shared_midi
            .ok_or(Error::InvalidState)?;
        let revision = if frames == 0 {
            expected
        } else {
            expected.checked_next()?
        };
        let edit = self.store.with_midi(take.store, |store, midi| {
            let expected = midi.info(midi.take_index(take.take)?)?.revision;
            midi.prepare_replace(
                &mut store.render_io(),
                take.take,
                expected,
                position,
                frames,
                input,
                initial,
            )
        })?;
        Ok(MidiEditId {
            store: take.store,
            edit,
            resource,
            version,
            expected,
            revision,
        })
    }
    pub fn progress_midi_edit(
        &mut self,
        id: MidiEditId,
        quantum: usize,
    ) -> Result<shared_midi::EditStatus, Error> {
        self.store.with_midi(id.store, |store, midi| {
            midi.progress_edit(&mut store.render_io(), id.edit, quantum)
        })
    }
    pub fn commit_midi_edit(&mut self, id: MidiEditId) -> Result<Revision, Error> {
        let mut owner = self.store.check_writer(id.resource, None)?;
        if owner.active != Some(id.version) || owner.revision != id.expected {
            return Err(Error::StaleRevision);
        }
        let mut version = get(&self.store.versions, id.version.slot, id.version.generation)?;
        let take = version.shared_midi.ok_or(Error::InvalidState)?;
        let info = self.store.with_midi(id.store, |store, midi| {
            midi.commit_edit(&mut store.render_io(), id.edit)?;
            midi.info(midi.take_index(take.take)?)
        })?;
        owner.revision = id.revision;
        version.revision = id.revision;
        version.description.bytes = info.bytes;
        self.store.resources[id.resource.slot() as usize].value = Some(owner);
        self.store.versions[id.version.slot as usize].value = Some(version);
        Ok(id.revision)
    }
}
impl ResourceIo<'_> {
    pub fn cancel_midi_edit(&mut self, id: MidiEditId) -> Result<(), Error> {
        self.store.cancel_midi_edit(id)
    }
    pub fn midi_coverage(&self, id: MidiStoreId) -> Result<Range<Frame>, Error> {
        self.store.midi_coverage(id)
    }
    pub fn prepare_midi_import(
        &mut self,
        store: MidiStoreId,
        snapshot: SnapshotId,
    ) -> Result<MidiImportId, Error> {
        self.store.prepare_midi_import(store, snapshot)
    }
    pub fn finish_midi_import(&mut self, id: MidiImportId) -> Result<ResourceId, Error> {
        self.store.finish_midi_import(id)
    }
    pub fn cancel_midi_import(&mut self, id: MidiImportId) -> Result<(), Error> {
        self.store.cancel_midi_import(id)
    }
    pub fn grab_midi(
        &mut self,
        store: MidiStoreId,
        range: Range<Frame>,
    ) -> Result<ResourceId, Error> {
        self.store.grab_midi(store, range)
    }
    pub fn fork_midi(
        &mut self,
        store: MidiStoreId,
        snapshot: SnapshotId,
    ) -> Result<ResourceId, Error> {
        self.store.fork_midi(store, snapshot)
    }
}

#[cfg(test)]
mod tests;
