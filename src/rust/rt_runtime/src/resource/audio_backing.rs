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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioChunkRef {
    pub(super) store: AudioStoreId,
    pub(super) lease: super::shared_pages::LeaseId,
    pub(super) first: usize,
    pub(super) stride: usize,
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

/// Staged COW samples stay private until commit. Cancel on any paired-media failure.
#[must_use]
pub struct AudioWrite {
    resource: ResourceId,
    expected: Revision,
    active: UploadId,
    revision: Revision,
    store: AudioStoreId,
    write: shared_audio::Write,
}

impl ResourceStore {
    pub(super) fn chunk_capture_store(&self, channels: u32) -> Result<Option<AudioStoreId>, Error> {
        if !self.audio_render_mode().chunked() {
            return Ok(None);
        }
        self.audio_stores
            .iter()
            .enumerate()
            .find(|(_, audio)| {
                audio.as_ref().is_some_and(|audio| {
                    audio.channels() == channels && audio.capture_slots(0).is_ok()
                })
            })
            .map(|(slot, _)| {
                Some(AudioStoreId {
                    scope: self.scope,
                    slot,
                })
            })
            .ok_or(Error::InvalidState)
    }
    pub fn audio_history_sequence(
        &self,
        id: AudioHistoryId,
        frame: Frame,
        channel: usize,
    ) -> Result<Option<crate::audio_refs::AudioSequence>, Error> {
        self.audio(id.store)?
            .history_sequence(id.store, id.history, frame, channel)
    }

    pub fn audio_chunk_sequence(
        &self,
        index: ReadIndex,
        position: u64,
        frames: usize,
        channel: usize,
    ) -> Result<Option<crate::audio_refs::AudioSequence>, Error> {
        let lease = self.index_lease(index)?;
        let Some(audio) = lease.audio else {
            return Ok(None);
        };
        self.audio(audio.store)?
            .chunk_sequence(audio.store, audio.index, position, frames, channel)
    }

    pub fn visit_audio_chunk(
        &self,
        reference: AudioChunkRef,
        frames: usize,
        consume: impl FnOnce(&[f32], usize, usize),
    ) -> Result<(), Error> {
        let samples = self
            .audio(reference.store)?
            .chunk_samples(reference.lease)?;
        if frames > 0
            && reference
                .first
                .checked_add((frames - 1) * reference.stride)
                .is_none_or(|last| last >= samples.len())
        {
            return Err(Error::InvalidOffset);
        }
        consume(samples, reference.first, reference.stride);
        Ok(())
    }

    pub fn retain_audio_chunk(
        &mut self,
        mut reference: AudioChunkRef,
    ) -> Result<AudioChunkRef, Error> {
        reference.lease = self.with_audio(reference.store, |_, audio| {
            audio.retain_chunk(reference.lease)
        })?;
        Ok(reference)
    }

    pub fn release_audio_chunk(&mut self, reference: AudioChunkRef) -> Result<(), Error> {
        self.with_audio(reference.store, |store, audio| {
            audio.release_chunk(store, reference.lease)
        })
    }
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

    pub fn prepare_audio_pool_windowed(
        &mut self,
        config: shared_audio::Config,
        origin: Frame,
        histories: usize,
    ) -> Result<AudioStoreId, Error> {
        let slot = self.audio_stores.len();
        if slot == self.config.limits().resource_slots {
            return Err(Error::Capacity);
        }
        let audio =
            shared_audio::AudioStore::prepare_histories_windowed(self, config, origin, histories)?;
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

    pub fn grab_audio_window(
        &mut self,
        source: AudioHistoryId,
        range: Range<Frame>,
        frames: u64,
    ) -> Result<ResourceId, Error> {
        self.audio_resource_credit()?;
        let take = self.with_audio(source.store, |resources, audio| {
            audio.grab_history_window(&mut resources.render_io(), source.history, range, frames)
        })?;
        self.audio_resource(Take {
            store: source.store,
            take,
        })
    }

    pub fn audio_snapshot_store(
        &self,
        snapshot: SnapshotId,
    ) -> Result<Option<AudioStoreId>, Error> {
        let lease = get(&self.snapshots, snapshot.slot, snapshot.generation)?;
        Ok(lease.audio.map(|source| source.store))
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

    pub fn empty_audio(&mut self, store: AudioStoreId) -> Result<ResourceId, Error> {
        self.audio_resource_credit()?;
        let take = self.with_audio(store, |_, audio| audio.empty_take())?;
        self.audio_resource(Take { store, take })
    }
    pub fn audio_growth_limit(&self, resource: ResourceId, owners: usize) -> Result<u64, Error> {
        let owner = self.check_writer(resource, None)?;
        let active = owner.active.ok_or(Error::InvalidState)?;
        let take = get(&self.versions, active.slot, active.generation)?
            .audio
            .ok_or(Error::InvalidShape)?;
        self.audio(take.store)?.growth_limit(take.take, owners)
    }
    pub fn prepare_audio_growth(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        maximum_frames: u64,
    ) -> Result<(), Error> {
        let owner = self.check_writer(resource, None)?;
        if owner.revision != expected {
            return Err(Error::StaleRevision);
        }
        let active = owner.active.ok_or(Error::InvalidState)?;
        let take = get(&self.versions, active.slot, active.generation)?
            .audio
            .ok_or(Error::InvalidShape)?;
        self.with_audio(take.store, |_, audio| {
            audio.reserve_growth(take.take, maximum_frames)
        })
    }
    pub fn finish_audio_growth(
        &mut self,
        resource: ResourceId,
        expected: Revision,
    ) -> Result<(), Error> {
        let owner = self.check_writer(resource, None)?;
        if owner.revision != expected {
            return Err(Error::StaleRevision);
        }
        let active = owner.active.ok_or(Error::InvalidState)?;
        let take = get(&self.versions, active.slot, active.generation)?
            .audio
            .ok_or(Error::InvalidShape)?;
        self.with_audio(take.store, |_, audio| audio.finish_growth(take.take))
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
        if lease.reservation.is_some() {
            return Err(Error::InvalidState);
        }
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

    pub fn append_audio_take(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        samples: &[f32],
    ) -> Result<Revision, Error> {
        let mut owner = self.store.check_writer(resource, None)?;
        if owner.revision != expected {
            return Err(Error::StaleRevision);
        }
        let active = owner.active.ok_or(Error::InvalidState)?;
        let mut version = get(&self.store.versions, active.slot, active.generation)?;
        let take = version.audio.ok_or(Error::InvalidShape)?;
        let revision = if samples.is_empty() {
            owner.revision
        } else {
            owner.revision.checked_next()?
        };
        let info = self.store.with_audio(take.store, |resources, audio| {
            audio.append_take(&mut resources.render_io(), take.take, samples)?;
            audio.info(audio.take_index(take.take)?)
        })?;
        owner.revision = revision;
        version.revision = revision;
        version.description.frames = info.frames;
        version.description.bytes = info.frames as usize * info.channels as usize * 4;
        self.store.resources[resource.slot() as usize].value = Some(owner);
        self.store.versions[active.slot as usize].value = Some(version);
        Ok(revision)
    }

    pub fn stage_audio_write(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        position: Option<u64>,
        samples: &[f32],
    ) -> Result<AudioWrite, Error> {
        let owner = self.store.check_writer(resource, None)?;
        if owner.revision != expected {
            return Err(Error::StaleRevision);
        }
        let active = owner.active.ok_or(Error::InvalidState)?;
        let version = get(&self.store.versions, active.slot, active.generation)?;
        let take = version.audio.ok_or(Error::InvalidShape)?;
        let revision = if samples.is_empty() {
            owner.revision
        } else {
            owner.revision.checked_next()?
        };
        let write = self.store.with_audio(take.store, |store, audio| {
            audio.stage_write(&mut store.render_io(), take.take, position, samples)
        })?;
        Ok(AudioWrite {
            resource,
            expected,
            active,
            revision,
            store: take.store,
            write,
        })
    }
    pub fn stage_audio_source_append(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        source: AudioHistoryId,
        frame: Frame,
    ) -> Result<Option<AudioWrite>, Error> {
        let Some(sequence) = self.store.audio_history_sequence(source, frame, 0)? else {
            return Ok(None);
        };
        let crate::audio_refs::ChunkRef::Persistent(reference) = sequence.spans[0].chunk else {
            return Ok(None);
        };
        let owner = self.store.check_writer(resource, None)?;
        if owner.revision != expected {
            return Err(Error::StaleRevision);
        }
        let active = owner.active.ok_or(Error::InvalidState)?;
        let version = get(&self.store.versions, active.slot, active.generation)?;
        let take = version.audio.ok_or(Error::InvalidShape)?;
        if take.store != source.store {
            return Ok(None);
        }
        let revision = owner.revision.checked_next()?;
        let write = self.store.with_audio(take.store, |store, audio| {
            audio.stage_reference_append(&mut store.render_io(), take.take, reference.lease)
        })?;
        Ok(write.map(|write| AudioWrite {
            resource,
            expected,
            active,
            revision,
            store: take.store,
            write,
        }))
    }

    pub fn stage_audio_window_write(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        samples: &[f32],
        spans: &[shared_audio::ChannelWrite],
    ) -> Result<AudioWrite, Error> {
        let owner = self.store.check_writer(resource, None)?;
        if owner.revision != expected {
            return Err(Error::StaleRevision);
        }
        let active = owner.active.ok_or(Error::InvalidState)?;
        let version = get(&self.store.versions, active.slot, active.generation)?;
        let take = version.audio.ok_or(Error::InvalidShape)?;
        let revision = if spans.iter().any(|span| span.frames > 0) {
            owner.revision.checked_next()?
        } else {
            owner.revision
        };
        let write = self.store.with_audio(take.store, |store, audio| {
            audio.stage_window_write(&mut store.render_io(), take.take, samples, spans)
        })?;
        Ok(AudioWrite {
            resource,
            expected,
            active,
            revision,
            store: take.store,
            write,
        })
    }
    pub fn cancel_audio_write(&mut self, write: AudioWrite) -> Result<(), Error> {
        self.store.with_audio(write.store, |store, audio| {
            audio.cancel_write(&mut store.render_io(), write.write)
        })
    }
    pub fn commit_audio_write(&mut self, write: AudioWrite) -> Result<Revision, Error> {
        let checked = self
            .store
            .check_writer(write.resource, None)
            .and_then(|owner| {
                if owner.revision != write.expected || owner.active != Some(write.active) {
                    Err(Error::StaleRevision)
                } else {
                    Ok(owner)
                }
            });
        let mut owner = match checked {
            Ok(owner) => owner,
            Err(error) => {
                self.cancel_audio_write(write)?;
                return Err(error);
            }
        };
        let mut version = get(
            &self.store.versions,
            write.active.slot,
            write.active.generation,
        )?;
        let info = self.store.with_audio(write.store, |store, audio| {
            audio.commit_write(&mut store.render_io(), write.write)
        })?;
        owner.revision = write.revision;
        version.revision = owner.revision;
        version.description.frames = info.frames;
        version.description.bytes = info.frames as usize * info.channels as usize * 4;
        self.store.resources[write.resource.slot() as usize].value = Some(owner);
        self.store.versions[write.active.slot as usize].value = Some(version);
        Ok(owner.revision)
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

    /// Edit one lane of a private COW candidate without changing its other lanes.
    pub fn replace_audio_channel(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        channel: usize,
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
            audio.replace_channel(
                &mut store.render_io(),
                take.take,
                channel,
                position,
                samples,
            )
        })?;
        owner.revision = revision;
        version.revision = revision;
        self.store.resources[resource.slot() as usize].value = Some(owner);
        self.store.versions[active.slot as usize].value = Some(version);
        Ok(revision)
    }
}

impl ResourceIo<'_> {
    pub fn empty_audio(&mut self, store: AudioStoreId) -> Result<ResourceId, Error> {
        self.store.empty_audio(store)
    }
    pub fn audio_snapshot_store(
        &self,
        snapshot: SnapshotId,
    ) -> Result<Option<AudioStoreId>, Error> {
        self.store.audio_snapshot_store(snapshot)
    }
    pub fn audio_growth_limit(&self, resource: ResourceId, owners: usize) -> Result<u64, Error> {
        self.store.audio_growth_limit(resource, owners)
    }
    pub fn prepare_audio_growth(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        maximum_frames: u64,
    ) -> Result<(), Error> {
        self.store
            .prepare_audio_growth(resource, expected, maximum_frames)
    }
    pub fn finish_audio_growth(
        &mut self,
        resource: ResourceId,
        expected: Revision,
    ) -> Result<(), Error> {
        self.store.finish_audio_growth(resource, expected)
    }

    pub fn grab_audio_window(
        &mut self,
        source: AudioHistoryId,
        range: Range<Frame>,
        frames: u64,
    ) -> Result<ResourceId, Error> {
        self.store.grab_audio_window(source, range, frames)
    }

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
