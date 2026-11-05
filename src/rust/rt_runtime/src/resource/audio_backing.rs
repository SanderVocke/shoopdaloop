//! Shared audio behind the ordinary resource/snapshot ownership boundary.
use super::*;
use core::ops::Range;

/// Prepared for the lifetime of its resource store; never serialized.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioStoreId {
    scope: u32,
    slot: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioHistoryId {
    store: AudioStoreId,
    history: shared_audio::HistoryId,
}

#[derive(Clone, Copy)]
pub(super) struct Take {
    pub store: AudioStoreId,
    pub take: shared_audio::TakeId,
}

#[derive(Clone, Copy)]
pub(super) struct Export {
    pub store: AudioStoreId,
    pub export: shared_audio::ExportId,
    pub index: shared_audio::ReadIndex,
}

impl ResourceStore {
    pub fn audio_store_capacity(
        &mut self,
        id: AudioStoreId,
    ) -> Result<shared_audio::Capacity, Error> {
        self.with_audio(id, |_, store| Ok(store.capacity()))
    }
    /// Host preparation only. The store and its reservation outlive all resources
    /// and snapshots created from it. Cleanup releases pages, never RT allocations.
    pub fn prepare_audio_store(
        &mut self,
        config: shared_audio::Config,
        origin: Frame,
    ) -> Result<AudioStoreId, Error> {
        self.prepare_audio_pool(config, origin, 1)
    }

    pub fn prepare_audio_pool(
        &mut self,
        config: shared_audio::Config,
        origin: Frame,
        histories: usize,
    ) -> Result<AudioStoreId, Error> {
        let slot = self.audio_stores.len();
        if slot == self.config.limits().resource_slots {
            return Err(Error::Capacity);
        }
        let audio = shared_audio::AudioStore::prepare_histories(self, config, origin, histories)?;
        self.audio_stores.push(Some(Box::new(audio)));
        Ok(AudioStoreId {
            scope: self.scope,
            slot,
        })
    }

    pub(super) fn with_audio<T>(
        &mut self,
        id: AudioStoreId,
        operation: impl FnOnce(&mut ResourceStore, &mut shared_audio::AudioStore) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        let mut audio = self
            .audio_stores
            .get_mut(id.slot)
            .and_then(Option::take)
            .ok_or(Error::InvalidId)?;
        let result = operation(self, &mut audio);
        self.audio_stores[id.slot] = Some(audio);
        result
    }

    pub fn audio_coverage(&self, id: AudioStoreId) -> Result<Range<Frame>, Error> {
        self.audio(id)?.coverage()
    }
    fn audio(&self, id: AudioStoreId) -> Result<&shared_audio::AudioStore, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        self.audio_stores
            .get(id.slot)
            .and_then(Option::as_ref)
            .map(Box::as_ref)
            .ok_or(Error::InvalidId)
    }
    pub fn audio_history(&self, store: AudioStoreId, slot: usize) -> Result<AudioHistoryId, Error> {
        let history = self.audio(store)?.history_id(slot)?;
        Ok(AudioHistoryId { store, history })
    }
    pub fn audio_source_coverage(&self, id: AudioHistoryId) -> Result<Range<Frame>, Error> {
        self.audio(id.store)?.history_coverage(id.history)
    }

    fn audio_resource(&mut self, audio: Take) -> Result<ResourceId, Error> {
        let info = self.with_audio(audio.store, |_, store| {
            store.info(store.take_index(audio.take)?)
        })?;
        let resource = self.create()?;
        let slot = self
            .free_versions
            .pop()
            .expect("preflighted version credit");
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
                format: Format::Audio {
                    channels: info.channels,
                },
                frames: info.frames,
                bytes: info.frames as usize * info.channels as usize * 4,
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
            audio: Some(audio),
            shared_midi: None,
            indexed_midi: None,
        });
        self.resources[resource.slot() as usize].value = Some(Resource {
            active: Some(active),
            revision,
            claim: None,
        });
        Ok(resource)
    }

    fn audio_resource_credit(&self) -> Result<(), Error> {
        if self.free_resources.is_empty() || self.free_versions.is_empty() {
            Err(Error::Capacity)
        } else {
            Ok(())
        }
    }

    /// A new ordinary resource owning a COW take; no sample copy or history walk.
    pub fn grab_audio(
        &mut self,
        store: AudioStoreId,
        range: Range<Frame>,
    ) -> Result<ResourceId, Error> {
        self.grab_audio_source(self.audio_history(store, 0)?, range)
    }
    pub fn grab_audio_source(
        &mut self,
        id: AudioHistoryId,
        range: Range<Frame>,
    ) -> Result<ResourceId, Error> {
        self.audio_resource_credit()?;
        let take = self.with_audio(id.store, |resources, audio| {
            audio.grab_history(&mut resources.render_io(), id.history, range)
        })?;
        self.audio_resource(Take {
            store: id.store,
            take,
        })
    }

    /// Loaded PCM remains an immutable base. Only edited pages are copied. A
    /// shared snapshot must come from this prepared audio store.
    pub fn fork_audio(
        &mut self,
        store: AudioStoreId,
        snapshot: SnapshotId,
    ) -> Result<ResourceId, Error> {
        self.audio_resource_credit()?;
        let lease = get(&self.snapshots, snapshot.slot, snapshot.generation)?;
        let take = if let Some(source) = lease.audio {
            if source.store != store {
                return Err(Error::InvalidShape);
            }
            self.with_audio(store, |_, audio| audio.fork_export(source.export))?
        } else {
            self.with_audio(store, |resources, audio| {
                audio.import_snapshot(resources, snapshot)
            })?
        };
        self.audio_resource(Take { store, take })
    }

    pub fn cleanup_audio_store(&mut self, id: AudioStoreId) -> Result<(), Error> {
        self.with_audio(id, |_, audio| audio.begin_cleanup())
    }

    pub(super) fn maintain_audio(&mut self) -> bool {
        let count = self.audio_stores.len();
        for _ in 0..count {
            let slot = self.audio_maintenance;
            self.audio_maintenance = (slot + 1) % count;
            if self.audio_stores[slot].is_some()
                && self
                    .with_audio(
                        AudioStoreId {
                            scope: self.scope,
                            slot,
                        },
                        |store, audio| audio.maintain_limit(store, 1),
                    )
                    .expect("prepared audio store")
                    > 0
            {
                return true;
            }
        }
        false
    }

    /// Independently retain exactly this revision, even after resource removal.
    pub fn retain_snapshot(&mut self, id: SnapshotId) -> Result<Snapshot, Error> {
        let mut lease = get(&self.snapshots, id.slot, id.generation)?;
        let mut version = get(&self.versions, lease.version.slot, lease.version.generation)?;
        let references = version.references.checked_add(1).ok_or(Error::Exhausted)?;
        let slot = *self.free_snapshots.last().ok_or(Error::Capacity)?;
        if let Some(audio) = lease.audio {
            lease.audio = Some(self.with_audio(audio.store, |_, store| {
                let export = store.retain_export(audio.export)?;
                Ok(Export {
                    store: audio.store,
                    export,
                    index: store.export_index(export)?,
                })
            })?);
        } else if let Some(midi) = lease.shared_midi {
            lease.shared_midi = Some(self.with_midi(midi.store, |_, store| {
                let export = store.retain_export(midi.export)?;
                Ok(midi_backing::Export {
                    store: midi.store,
                    export,
                    index: store.export_index(export)?,
                })
            })?);
        } else if let Some(take) = lease.indexed_midi {
            lease.indexed_midi = Some(self.with_indexed_midi(take.store, |_, pool| {
                pool.retain(take.media).map(|media| indexed_midi::Take {
                    store: take.store,
                    media,
                })
            })?);
        } else if let Some(tail) = lease.private_tail {
            let used = self.pages[tail].used;
            if self.free_pages.len() <= self.reserved_pages {
                return Err(Error::Capacity);
            }
            self.charge_external(used, 0)?;
            let page = self.free_pages.pop().expect("preflighted tail credit");
            let bytes = self.config.limits().page_bytes;
            self.data
                .copy_within(tail * bytes..tail * bytes + used, page * bytes);
            self.pages[page] = Page {
                used,
                ..Page::default()
            };
            lease.private_tail = Some(page);
        }
        self.free_snapshots.pop();
        lease.snapshot.id = SnapshotId {
            slot: slot as u32,
            generation: self.snapshots[slot].generation,
        };
        self.snapshots[slot].value = Some(lease);
        version.references = references;
        self.versions[lease.version.slot as usize].value = Some(version);
        Ok(lease.snapshot)
    }

    // The caller preflights and charges the complete copy. This is restricted to
    // ordinary immutable bases, so nested audio-store borrows cannot occur.
    pub(super) fn copy_linear_span(
        &mut self,
        index: ReadIndex,
        offset: usize,
        output: &mut [u8],
        class: WorkClass,
    ) -> Result<(), Error> {
        let lease = self.index_lease(index)?;
        if lease.audio.is_some() || lease.shared_midi.is_some() || lease.indexed_midi.is_some() {
            return Err(Error::InvalidShape);
        }
        if offset
            .checked_add(output.len())
            .is_none_or(|end| end > lease.snapshot.description.bytes)
        {
            return Err(Error::InvalidOffset);
        }
        let mut cursor = self.cursor_at_with(index, offset, class)?;
        let mut copied = 0;
        let page_bytes = self.config.limits().page_bytes;
        while copied < output.len() {
            let page = lease
                .private_tail
                .filter(|_| cursor.position >= lease.tail_start)
                .or(cursor.page)
                .ok_or(Error::InvalidState)?;
            let local = cursor.position % page_bytes;
            let count = (output.len() - copied).min(self.pages[page].used - local);
            if count == 0 {
                return Err(Error::InvalidState);
            }
            let start = page * page_bytes + local;
            output[copied..copied + count].copy_from_slice(&self.data[start..start + count]);
            copied += count;
            cursor.position += count;
            if cursor.position % page_bytes == 0 {
                cursor.page = self.pages[page].next;
            }
        }
        Ok(())
    }
}

impl RenderIo<'_> {
    pub fn audio_source_discontinuity(
        &mut self,
        id: AudioHistoryId,
        next: Frame,
    ) -> Result<(), Error> {
        self.store.with_audio(id.store, |store, audio| {
            audio.history_discontinuity(&mut store.render_io(), id.history, next)
        })
    }
    pub fn append_audio_source(
        &mut self,
        id: AudioHistoryId,
        frame: Frame,
        samples: &[f32],
    ) -> Result<(), Error> {
        self.store.with_audio(id.store, |store, audio| {
            audio.append_history(&mut store.render_io(), id.history, frame, samples)
        })
    }
    pub fn audio_history_discontinuity(
        &mut self,
        id: AudioStoreId,
        next: Frame,
    ) -> Result<(), Error> {
        self.store.with_audio(id, |store, audio| {
            audio.discontinuity(&mut store.render_io(), next)
        })
    }
    pub fn append_audio_history(
        &mut self,
        id: AudioStoreId,
        frame: Frame,
        samples: &[f32],
    ) -> Result<(), Error> {
        self.store.with_audio(id, |store, audio| {
            audio.append(&mut store.render_io(), frame, samples)
        })
    }

    /// Required COW edit of a resource; immutable generic snapshots stay pinned.
    pub fn replace_audio(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        position: u64,
        samples: &[f32],
    ) -> Result<Revision, Error> {
        let mut owner = self.store.check_writer(resource, None)?;
        if owner.revision != expected {
            return Err(Error::StaleRevision);
        }
        let active = owner.active.ok_or(Error::InvalidState)?;
        let mut version = get(&self.store.versions, active.slot, active.generation)?;
        let take = version.audio.ok_or(Error::InvalidState)?;
        let revision = if samples.is_empty() {
            owner.revision
        } else {
            owner.revision.checked_next()?
        };
        self.store.with_audio(take.store, |store, audio| {
            audio.replace(&mut store.render_io(), take.take, position, samples)
        })?;
        owner.revision = revision;
        version.revision = revision;
        self.store.resources[resource.slot() as usize].value = Some(owner);
        self.store.versions[active.slot as usize].value = Some(version);
        Ok(revision)
    }
}

impl ResourceIo<'_> {
    pub fn audio_source_coverage(&self, id: AudioHistoryId) -> Result<Range<Frame>, Error> {
        self.store.audio_source_coverage(id)
    }
    pub fn grab_audio_source(
        &mut self,
        id: AudioHistoryId,
        range: Range<Frame>,
    ) -> Result<ResourceId, Error> {
        self.store.grab_audio_source(id, range)
    }
    pub fn audio_coverage(&self, id: AudioStoreId) -> Result<Range<Frame>, Error> {
        self.store.audio_coverage(id)
    }
    pub fn grab_audio(
        &mut self,
        store: AudioStoreId,
        range: Range<Frame>,
    ) -> Result<ResourceId, Error> {
        self.store.grab_audio(store, range)
    }
    pub fn fork_audio(
        &mut self,
        store: AudioStoreId,
        snapshot: SnapshotId,
    ) -> Result<ResourceId, Error> {
        self.store.fork_audio(store, snapshot)
    }
}

#[cfg(test)]
mod tests;
