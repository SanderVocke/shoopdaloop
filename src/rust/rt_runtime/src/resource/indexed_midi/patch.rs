use super::*;

impl<S: Sample> ResourceStore<S> {
    pub fn prepare_indexed_midi_patch(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        position: u64,
        frames: usize,
        input: &MidiBuffer,
        initial: &MidiState,
    ) -> Result<PatchId, Error> {
        let owner = self.check_writer(resource, None)?;
        if owner.revision != expected {
            return Err(Error::StaleRevision);
        }
        let version = owner.active.ok_or(Error::InvalidState)?;
        let take = get(&self.versions, version.slot, version.generation)?
            .indexed_midi
            .ok_or(Error::InvalidShape)?;
        self.with_indexed_midi(take.store, |resources, pool| {
            pool.patches.available()?;
            if pool
                .patches
                .entries
                .iter()
                .filter_map(|e| e.value.as_ref())
                .any(|j| j.resource == resource)
                || pool
                    .writers
                    .entries
                    .iter()
                    .filter_map(|e| e.value.as_ref())
                    .any(|j| j.resource == resource)
            {
                return Err(Error::Busy);
            }
            let edit = pool.core.begin_patch(
                &mut resources.render_io(),
                pool.views.get(take.media)?,
                position,
                frames,
                input,
                initial,
            )?;
            Ok(PatchId {
                store: take.store,
                key: pool.patches.insert(PatchJob {
                    edit,
                    resource,
                    expected,
                    version,
                    original: take.media,
                    ready: false,
                }),
            })
        })
    }
    pub fn progress_indexed_midi_patch(
        &mut self,
        id: PatchId,
        steps: usize,
    ) -> Result<Status, Error> {
        self.with_indexed_midi(id.store, |resources, pool| {
            let job = pool.patches.get_mut(id.key)?;
            let owner = resources.check_writer(job.resource, None)?;
            if owner.revision != job.expected || owner.active != Some(job.version) {
                return Err(Error::StaleRevision);
            }
            job.ready =
                pool.core
                    .progress_patch(&mut resources.render_io(), &mut job.edit, steps)?;
            Ok(if job.ready {
                Status::Ready
            } else {
                Status::Building
            })
        })
    }
    pub fn commit_indexed_midi_patch(&mut self, id: PatchId) -> Result<Revision, Error> {
        self.with_indexed_midi(id.store, |resources, pool| {
            let job = pool.patches.get(id.key)?;
            if !job.ready {
                return Err(Error::Busy);
            }
            let mut owner = resources.check_writer(job.resource, None)?;
            if owner.revision != job.expected || owner.active != Some(job.version) {
                return Err(Error::StaleRevision);
            }
            let revision = owner.revision.checked_next()?;
            let job = pool.patches.remove(id.key)?;
            let media = match pool.core.finish_patch(job.edit) {
                Ok(media) => media,
                Err(_) => unreachable!("ready patch"),
            };
            let bytes = pool.core.totals(&mut resources.render_io(), &media)?.bytes;
            let old = core::mem::replace(pool.views.get_mut(job.original)?, media);
            pool.core.release(old)?;
            let mut version = get(
                &resources.versions,
                job.version.slot,
                job.version.generation,
            )?;
            owner.revision = revision;
            version.revision = revision;
            version.description.bytes = bytes;
            resources.resources[job.resource.slot() as usize].value = Some(owner);
            resources.versions[job.version.slot as usize].value = Some(version);
            Ok(revision)
        })
    }
    pub fn cancel_indexed_midi_patch(&mut self, id: PatchId) -> Result<(), Error> {
        self.with_indexed_midi(id.store, |_, pool| {
            pool.core.cancel_patch(pool.patches.remove(id.key)?.edit)
        })
    }
}
impl<S: Sample> ResourceIo<'_, S> {
    pub fn prepare_indexed_midi_patch(
        &mut self,
        resource: ResourceId,
        expected: Revision,
        position: u64,
        frames: usize,
        input: &MidiBuffer,
        initial: &MidiState,
    ) -> Result<PatchId, Error> {
        self.store
            .prepare_indexed_midi_patch(resource, expected, position, frames, input, initial)
    }
    pub fn progress_indexed_midi_patch(
        &mut self,
        id: PatchId,
        steps: usize,
    ) -> Result<Status, Error> {
        self.store.progress_indexed_midi_patch(id, steps)
    }
    pub fn commit_indexed_midi_patch(&mut self, id: PatchId) -> Result<Revision, Error> {
        self.store.commit_indexed_midi_patch(id)
    }
    pub fn cancel_indexed_midi_patch(&mut self, id: PatchId) -> Result<(), Error> {
        self.store.cancel_indexed_midi_patch(id)
    }
}
