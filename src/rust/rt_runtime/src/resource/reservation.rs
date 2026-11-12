//! Snapshot publication credits owned before a continuous writer begins.
use super::*;
use crate::Sample;

/// Owns a private snapshot slot and future backing owner. Copying the token does
/// not acquire ownership; publish or cancel exactly once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReservedSnapshotId {
    scope: u32,
    snapshot: SnapshotId,
}
#[derive(Clone, Copy)]
enum Backing {
    Audio(AudioStoreId, shared_audio::ExportId),
    Midi(indexed_midi::StoreId, indexed_midi::Key),
}
#[derive(Clone, Copy)]
pub(super) struct Reservation {
    backing: Backing,
    maximum_frames: u64,
}
impl<S: Sample> ResourceStore<S> {
    /// Pin the current version and reserve the final snapshot/view capacity.
    /// The resource may change its complete content in place, within the supplied
    /// maximum duration; replacing its version invalidates publication.
    pub fn reserve_writer_snapshot(
        &mut self,
        resource: ResourceId,
        cut: Frame,
        maximum_frames: u64,
    ) -> Result<ReservedSnapshotId, Error> {
        let owner = self.check_writer(resource, None)?;
        let active = owner.active.ok_or(Error::InvalidState)?;
        let version = get(&self.versions, active.slot, active.generation)?;
        if version.state != State::Active || version.capture.is_some() {
            return Err(Error::InvalidState);
        }
        if version.description.frames > maximum_frames {
            return Err(Error::InvalidShape);
        }
        if version.audio.is_none() && version.indexed_midi.is_none() {
            return Err(Error::InvalidShape);
        }
        let snapshot = self.snapshot(resource, cut)?;
        let backing = if let Some(take) = version.audio {
            self.with_audio(take.store, |_, audio| {
                audio.reserve_export(take.take, maximum_frames)
            })
            .map(|id| Backing::Audio(take.store, id))
        } else {
            let take = version.indexed_midi.expect("validated backing");
            self.with_indexed_midi(take.store, |_, pool| pool.reserve_export())
                .map(|id| Backing::Midi(take.store, id))
        };
        let backing = match backing {
            Ok(backing) => backing,
            Err(error) => {
                self.release_snapshot(snapshot.id)?;
                return Err(error);
            }
        };
        let lease = self.snapshots[snapshot.id.slot as usize]
            .value
            .as_mut()
            .expect("owned snapshot");
        lease.reservation = Some(Reservation {
            backing,
            maximum_frames,
        });
        Ok(ReservedSnapshotId {
            scope: self.scope,
            snapshot: snapshot.id,
        })
    }
    fn reserved_lease(&self, id: ReservedSnapshotId) -> Result<Lease, Error> {
        if id.scope != self.scope {
            return Err(Error::InvalidId);
        }
        let lease = get(&self.snapshots, id.snapshot.slot, id.snapshot.generation)?;
        if lease.reservation.is_none() {
            return Err(Error::InvalidState);
        }
        Ok(lease)
    }
    pub fn cancel_reserved_snapshot(&mut self, id: ReservedSnapshotId) -> Result<(), Error> {
        self.reserved_lease(id)?;
        self.release_snapshot(id.snapshot)
    }
    pub(super) fn cancel_backing_reservation(
        &mut self,
        reservation: Reservation,
    ) -> Result<(), Error> {
        match reservation.backing {
            Backing::Audio(store, export) => {
                self.with_audio(store, |_, audio| audio.release_export(export))
            }
            Backing::Midi(store, key) => self.with_indexed_midi(store, |_, pool| pool.release(key)),
        }
    }
    /// Retarget the private reservation to a complete current revision without a
    /// new snapshot, view, version or payload allocation. An older export remains
    /// independently owned and is retired through normal bounded maintenance.
    pub fn publish_reserved_snapshot(
        &mut self,
        id: ReservedSnapshotId,
        cut: Frame,
    ) -> Result<Snapshot, Error> {
        let mut lease = self.reserved_lease(id)?;
        let owner = self.check_writer(lease.snapshot.resource, None)?;
        if owner.active != Some(lease.version) {
            return Err(Error::StaleRevision);
        }
        let version = get(&self.versions, lease.version.slot, lease.version.generation)?;
        let reservation = lease.reservation.expect("validated reservation");
        if version.state != State::Active || version.capture.is_some() {
            return Err(Error::InvalidState);
        }
        if cut < lease.snapshot.cut_frame {
            return Err(Error::InvalidFrame);
        }
        if version.description.frames > reservation.maximum_frames {
            return Err(Error::Capacity);
        }
        match reservation.backing {
            Backing::Audio(store, export) => {
                let take = version
                    .audio
                    .filter(|take| take.store == store)
                    .ok_or(Error::InvalidShape)?;
                let old = lease.audio.ok_or(Error::InvalidShape)?;
                lease.audio = Some(self.with_audio(store, |_, audio| {
                    audio.publish_reserved_export(export, take.take)?;
                    let index = audio.export_index(export)?;
                    audio.release_export(old.export)?;
                    Ok(audio_backing::Export {
                        store,
                        export,
                        index,
                    })
                })?);
            }
            Backing::Midi(store, media) => {
                let take = version
                    .indexed_midi
                    .filter(|take| take.store == store)
                    .ok_or(Error::InvalidShape)?;
                let old = lease.indexed_midi.ok_or(Error::InvalidShape)?;
                self.with_indexed_midi(store, |resources, pool| {
                    pool.publish_export(media, take.media)?;
                    pool.release(old.media)?;
                    resources.charge_work(
                        WorkClass::Render,
                        size_of::<crate::midi_state::MidiState>(),
                        0,
                    )
                })?;
                lease.indexed_midi = Some(indexed_midi::Take { store, media });
            }
        }
        lease.snapshot.revision = version.revision;
        lease.snapshot.description = version.description;
        lease.snapshot.cut_frame = cut;
        lease.snapshot.capture = version.capture;
        lease.reservation = None;
        self.snapshots[id.snapshot.slot as usize].value = Some(lease);
        Ok(lease.snapshot)
    }
}

impl<S: Sample> ResourceIo<'_, S> {
    pub fn reserve_writer_snapshot(
        &mut self,
        resource: ResourceId,
        cut: Frame,
        maximum_frames: u64,
    ) -> Result<ReservedSnapshotId, Error> {
        self.store
            .reserve_writer_snapshot(resource, cut, maximum_frames)
    }
    pub fn cancel_reserved_snapshot(&mut self, id: ReservedSnapshotId) -> Result<(), Error> {
        self.store.cancel_reserved_snapshot(id)
    }
    pub fn publish_reserved_snapshot(
        &mut self,
        id: ReservedSnapshotId,
        cut: Frame,
    ) -> Result<Snapshot, Error> {
        self.store.publish_reserved_snapshot(id, cut)
    }
}

#[cfg(test)]
mod tests;
