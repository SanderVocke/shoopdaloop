use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LiveWriterId {
    scope: u32,
    slot: u32,
    generation: u32,
}
#[derive(Clone, Copy)]
pub(in crate::runtime) struct LiveJob {
    pub fence: Fence,
    pub source: HistoryId,
    pub kind: indexed_midi::WriteKind,
    pub frames: u64,
    pub position: u64,
    audio: Option<rt_runtime::ResourceId>,
    midi: Option<rt_runtime::ResourceId>,
    audio_revision: Revision,
    audio_export: Option<ReservedSnapshotId>,
    midi_export: Option<ReservedSnapshotId>,
    import: Option<indexed_midi::ImportId>,
    writer: Option<indexed_midi::WriterId>,
    pub ready: bool,
    pub active: bool,
    pub failure: Option<Error>,
}
pub(super) struct LiveSlot {
    pub generation: u32,
    pub value: Option<LiveJob>,
}
impl Shared {
    pub fn live_job(&self, id: LiveWriterId) -> Result<LiveJob, Error> {
        if id.scope != self.scope || self.closed {
            return Err(Error::InvalidId);
        }
        self.live
            .get(id.slot as usize)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.value)
            .ok_or(Error::InvalidId)
    }
    pub fn live_for(&self, target: LoopId) -> Option<LiveWriterId> {
        self.live
            .iter()
            .enumerate()
            .find(|(_, slot)| {
                slot.value
                    .is_some_and(|job| job.fence.target == target && !job.active)
            })
            .map(|(slot, entry)| LiveWriterId {
                scope: self.scope,
                slot: slot as u32,
                generation: entry.generation,
            })
    }
    pub fn begin_live(
        &mut self,
        fence: Fence,
        source: HistoryId,
        take: SharedTake,
        kind: indexed_midi::WriteKind,
        frame: Frame,
        io: &mut ResourceIo<'_>,
    ) -> Result<LiveWriterId, Error> {
        if self.live_for(fence.target).is_some() {
            return Err(Error::Busy);
        }
        let status = self.status(source.source, fence.graph, io)?;
        if status.id != source {
            return Err(Error::InvalidId);
        }
        if let Some(error) = status.failure {
            return Err(error);
        }
        if matches!(kind, indexed_midi::WriteKind::Fixed) && take.frames == 0 {
            return Err(Error::InvalidShape);
        }
        let h = self.source(source.source)?;
        let audio_store = h.audio;
        let midi_store = h.indexed_midi.map(|id| id.store());
        if take.audio.is_some() && audio_store.is_none()
            || take.midi.is_some() && midi_store.is_none()
        {
            return Err(Error::InvalidShape);
        }
        let slot = self
            .live
            .iter()
            .position(|slot| slot.generation > 0 && slot.value.is_none())
            .ok_or(Error::Capacity)?;
        let id = LiveWriterId {
            scope: self.scope,
            slot: slot as u32,
            generation: self.live[slot].generation,
        };
        let mut job = LiveJob {
            fence,
            source,
            kind,
            frames: take.frames,
            position: 0,
            audio: None,
            midi: None,
            audio_revision: Revision(0),
            audio_export: None,
            midi_export: None,
            import: None,
            writer: None,
            ready: false,
            active: false,
            failure: None,
        };
        let maximum = match kind {
            indexed_midi::WriteKind::Fixed => take.frames,
            indexed_midi::WriteKind::Growing { maximum_frames } => maximum_frames,
        };
        let result = (|| {
            if let Some(resource) = take.audio {
                let snapshot = io.snapshot(resource, frame)?;
                if snapshot.description.frames != take.frames {
                    io.release_snapshot(snapshot.id)?;
                    return Err(Error::InvalidShape);
                }
                let fork = io.fork_audio(audio_store.ok_or(Error::InvalidShape)?, snapshot.id);
                io.release_snapshot(snapshot.id)?;
                let resource = fork?;
                job.audio = Some(resource);
                job.audio_revision = io.info(resource)?.revision;
                if matches!(kind, indexed_midi::WriteKind::Growing { .. }) {
                    io.prepare_audio_growth(resource, job.audio_revision, maximum)?;
                }
                job.audio_export = Some(io.reserve_snapshot(resource, frame, maximum)?);
            }
            if let Some(resource) = take.midi {
                let store = midi_store.ok_or(Error::InvalidShape)?;
                let snapshot = io.snapshot(resource, frame)?;
                if snapshot.description.frames != take.frames {
                    io.release_snapshot(snapshot.id)?;
                    return Err(Error::InvalidShape);
                }
                let result = if take.indexed_midi_store == Some(store) {
                    io.fork_indexed_midi(store, snapshot.id)
                        .map(|resource| job.midi = Some(resource))
                } else {
                    let index = io.read_index(snapshot.id)?;
                    let initial = io.render().midi_seek(index, 0)?.state;
                    io.prepare_indexed_midi_import(store, snapshot.id, &initial)
                        .map(|import| job.import = Some(import))
                };
                io.release_snapshot(snapshot.id)?;
                result?;
            }
            Self::progress_live_job(&mut job, frame, 1, io)
        })();
        self.live[slot].value = Some(job);
        if let Err(error) = result {
            self.cancel_live(id, io)?;
            return Err(error);
        }
        Ok(id)
    }
    fn progress_live_job(
        job: &mut LiveJob,
        frame: Frame,
        steps: usize,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        if job.ready || job.active || job.failure.is_some() {
            return Ok(());
        }
        if let Some(import) = job.import {
            if io.progress_indexed_midi_import(import, steps)? != indexed_midi::Status::Ready {
                return Ok(());
            }
            job.midi = Some(io.finish_indexed_midi_import(import)?);
            job.import = None;
        }
        if let Some(resource) = job.midi {
            if job.writer.is_none() {
                job.writer = Some(io.prepare_indexed_midi_writer(
                    resource,
                    io.info(resource)?.revision,
                    job.kind,
                )?);
                let maximum = match job.kind {
                    indexed_midi::WriteKind::Fixed => job.frames,
                    indexed_midi::WriteKind::Growing { maximum_frames } => maximum_frames,
                };
                job.midi_export = Some(io.reserve_snapshot(resource, frame, maximum)?);
            }
            if io.progress_indexed_midi_writer(job.writer.unwrap(), steps)?
                != indexed_midi::Status::Ready
            {
                return Ok(());
            }
        }
        job.ready = true;
        Ok(())
    }
    pub fn progress_live(&mut self, frame: Frame, io: &mut ResourceIo<'_>) {
        let budget = if self.building > 0 {
            self.remaining.div_ceil(2)
        } else {
            self.remaining
        };
        for _ in 0..budget {
            let mut selected = None;
            for _ in 0..self.live.len() {
                let slot = self.live_turn;
                self.live_turn = (slot + 1) % self.live.len();
                self.work.jobs_scanned = self.work.jobs_scanned.saturating_add(1);
                if self.live[slot]
                    .value
                    .is_some_and(|job| !job.ready && !job.active && job.failure.is_none())
                {
                    selected = Some(slot);
                    break;
                }
            }
            let Some(slot) = selected else { break };
            self.remaining -= 1;
            self.work.edit_steps += 1;
            let job = self.live[slot].value.as_mut().unwrap();
            if let Err(error) = Self::progress_live_job(job, frame, 1, io) {
                job.failure = Some(error);
            }
        }
    }
    pub fn start_live(
        &mut self,
        id: LiveWriterId,
        position: u64,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        let job = self.live_job(id)?;
        if job.active || !job.ready {
            return Err(Error::Busy);
        }
        if let Some(error) = job.failure {
            return Err(error);
        }
        if let Some(writer) = job.writer {
            io.start_indexed_midi_writer(writer, position)?;
        }
        let job = self.live[id.slot as usize].value.as_mut().unwrap();
        job.active = true;
        job.position = position;
        Ok(())
    }
    pub fn write_live(
        &mut self,
        source: u32,
        frame: Frame,
        q: usize,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        let h = self.source(source)?;
        let generation = h.generation;
        let staged = h.staged;
        let failure = h.failure;
        let midi_source = h.indexed_midi;
        let slot = self.lookup[source as usize].unwrap();
        let h = &self.histories[slot];
        for entry in &mut self.live {
            let Some(job) = entry
                .value
                .as_mut()
                .filter(|job| job.active && job.failure.is_none() && job.source.source == source)
            else {
                continue;
            };
            self.work.writer_ticks = self.work.writer_ticks.saturating_add(1);
            let result = (|| {
                if generation != job.source.generation || staged != Some(frame.checked_add(q)?) {
                    return Err(failure.unwrap_or(Error::StaleRevision));
                }
                let staged = if let Some(resource) = job.audio {
                    let channels = h.settings.audio.ok_or(Error::InvalidShape)?.channels as usize;
                    Some(io.render().stage_audio_write(
                        resource,
                        job.audio_revision,
                        matches!(job.kind, indexed_midi::WriteKind::Fixed).then_some(job.position),
                        &h.audio_scratch[..q * channels],
                    )?)
                } else {
                    None
                };
                if let Some(writer) = job.writer {
                    if let Err(error) = io.render().write_indexed_midi_tick(
                        writer,
                        midi_source.ok_or(Error::InvalidShape)?,
                        frame,
                    ) {
                        if let Some(write) = staged {
                            io.render().cancel_audio_write(write)?;
                        }
                        return Err(error);
                    }
                }
                if let Some(write) = staged {
                    job.audio_revision = io.render().commit_audio_write(write)?;
                }
                match job.kind {
                    indexed_midi::WriteKind::Fixed => {
                        job.position = (job.position + q as u64) % job.frames
                    }
                    indexed_midi::WriteKind::Growing { .. } => {
                        job.frames += q as u64;
                        job.position = job.frames;
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                job.failure = Some(error);
                self.work.writer_failures = self.work.writer_failures.saturating_add(1);
            }
        }
        Ok(())
    }
    pub fn seek_live(
        &mut self,
        id: LiveWriterId,
        position: u64,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        let job = self.live_job(id)?;
        if !job.active || job.kind != indexed_midi::WriteKind::Fixed || position >= job.frames {
            return Err(Error::InvalidFrame);
        }
        if job.position == position {
            return Ok(());
        }
        if let Some(writer) = job.writer {
            io.seek_indexed_midi_writer(writer, position)?;
        }
        self.live[id.slot as usize].value.as_mut().unwrap().position = position;
        Ok(())
    }
    fn prepare_fixed_from_live(
        &mut self,
        id: LiveWriterId,
        fence: Fence,
        frame: Frame,
        io: &mut ResourceIo<'_>,
    ) -> Result<LiveWriterId, Error> {
        let job = self.live_job(id)?;
        if !job.active || !matches!(job.kind, indexed_midi::WriteKind::Growing { .. }) {
            return Err(Error::InvalidState);
        }
        if let Some(error) = job.failure {
            return Err(error);
        }
        let h = self.source(job.source.source)?;
        let audio_store = h.audio;
        let indexed_midi_store = h.indexed_midi.map(|id| id.store());
        let midi = job
            .writer
            .map(|writer| io.snapshot_indexed_midi_writer(writer))
            .transpose()?;
        let result = self.begin_live(
            fence,
            job.source,
            SharedTake {
                frames: job.frames,
                audio: job.audio,
                midi,
                audio_store,
                midi_store: None,
                indexed_midi_store,
            },
            indexed_midi::WriteKind::Fixed,
            frame,
            io,
        );
        if let Some(resource) = midi {
            io.remove(resource)?;
        }
        result
    }
    pub fn snapshot_live(
        &mut self,
        id: LiveWriterId,
        frame: Frame,
        io: &mut ResourceIo<'_>,
    ) -> Result<[Option<Snapshot>; 2], Error> {
        let job = self.live_job(id)?;
        if !job.active {
            return Err(Error::Busy);
        }
        let audio = job
            .audio
            .map(|resource| io.snapshot(resource, frame))
            .transpose()?;
        let midi = (|| {
            let Some(writer) = job.writer else {
                return Ok(None);
            };
            let resource = io.snapshot_indexed_midi_writer(writer)?;
            let snapshot = io.snapshot(resource, frame);
            io.remove(resource)?;
            snapshot.map(Some)
        })();
        match midi {
            Ok(midi) => Ok([audio, midi]),
            Err(error) => {
                if let Some(audio) = audio {
                    io.release_snapshot(audio.id)?;
                }
                Err(error)
            }
        }
    }
    pub fn finish_live(
        &mut self,
        id: LiveWriterId,
        frame: Frame,
        io: &mut ResourceIo<'_>,
    ) -> Result<(u64, [Option<Snapshot>; 2]), Error> {
        let mut job = self.live_job(id)?;
        if !job.active {
            return Err(Error::InvalidState);
        }
        if let Some(writer) = job.writer.take() {
            io.finish_indexed_midi_writer(writer)?;
        }
        if let Some(resource) = job
            .audio
            .filter(|_| matches!(job.kind, indexed_midi::WriteKind::Growing { .. }))
        {
            io.finish_audio_growth(resource, job.audio_revision)?;
        }
        let audio = job
            .audio_export
            .take()
            .map(|id| io.publish_reserved_snapshot(id, frame))
            .transpose()?;
        let midi = job
            .midi_export
            .take()
            .map(|id| io.publish_reserved_snapshot(id, frame))
            .transpose()?;
        let frames = job.frames;
        self.live[id.slot as usize].value = Some(job);
        self.cancel_live(id, io)?;
        Ok((frames, [audio, midi]))
    }
    pub fn cancel_live(&mut self, id: LiveWriterId, io: &mut ResourceIo<'_>) -> Result<(), Error> {
        let job = self.live_job(id)?;
        if let Some(import) = job.import {
            io.cancel_indexed_midi_import(import)?;
        }
        if let Some(writer) = job.writer {
            io.cancel_indexed_midi_writer(writer)?;
        }
        for id in [job.audio_export, job.midi_export].into_iter().flatten() {
            io.cancel_reserved_snapshot(id)?;
        }
        for resource in [job.audio, job.midi].into_iter().flatten() {
            io.remove(resource)?;
        }
        let slot = &mut self.live[id.slot as usize];
        slot.value = None;
        slot.generation = slot.generation.checked_add(1).unwrap_or(0);
        Ok(())
    }
    pub fn cancel_all_live(&mut self, io: &mut ResourceIo<'_>) -> Result<(), Error> {
        for index in 0..self.live.len() {
            if self.live[index].value.is_some() {
                self.cancel_live(
                    LiveWriterId {
                        scope: self.scope,
                        slot: index as u32,
                        generation: self.live[index].generation,
                    },
                    io,
                )?;
            }
        }
        Ok(())
    }
}

impl LoopPlanner {
    pub(in crate::runtime) fn prepare_live_take(
        &mut self,
        slot: usize,
        decision: Decision,
        io: &mut ResourceIo<'_>,
    ) -> Result<Option<Take>, Error> {
        if decision.state.mode == Mode::Replacing {
            let row = self.rows[slot].ok_or(Error::InvalidId)?;
            if let Some(writer) = row.active.and_then(|take| take.writer) {
                let before = self
                    .resolver
                    .state(decision.state.id)
                    .map_err(runtime_error)?;
                if before.mode == Mode::Recording {
                    let fence = Fence {
                        timing_revision: row.timing_revision,
                        target: before.id,
                        revision: before.content_revision,
                        length: before.content_length,
                        graph: self.graph_epoch,
                        topology: self.resolver.epoch(),
                    };
                    let shared = self.shared.as_mut().ok_or(Error::InvalidState)?;
                    if shared.live_for(before.id).is_none() {
                        shared.prepare_fixed_from_live(writer, fence, self.resolver.frame(), io)?;
                    }
                }
            }
        }
        let Some(shared) = &self.shared else {
            return Ok(None);
        };
        let Some(id) = shared.live_for(decision.state.id) else {
            return Ok(None);
        };
        let job = shared.live_job(id)?;
        if job.active {
            return Ok(None);
        }
        if let Some(error) = job.failure {
            return Err(error);
        }
        if !job.ready {
            return Err(Error::Busy);
        }
        let before = self
            .resolver
            .state(decision.state.id)
            .map_err(runtime_error)?;
        let row = self.rows[slot].ok_or(Error::InvalidId)?;
        let status = shared.status(job.source.source, self.graph_epoch, io)?;
        if status.id != job.source {
            return Err(Error::StaleRevision);
        }
        if let Some(error) = status.failure {
            return Err(error);
        }
        if job.fence.graph != self.graph_epoch
            || job.fence.topology != self.resolver.epoch()
            || job.fence.revision != before.content_revision
            || job.fence.length != before.content_length
            || job.fence.timing_revision != row.timing_revision
        {
            return Err(Error::StaleRevision);
        }
        if job.kind == indexed_midi::WriteKind::Fixed && job.frames != decision.state.content_length
        {
            return Err(Error::InvalidShape);
        }
        if !matches!(
            (job.kind, decision.state.mode),
            (indexed_midi::WriteKind::Fixed, Mode::Replacing)
                | (indexed_midi::WriteKind::Growing { .. }, Mode::Recording)
        ) || decision.state.mode == Mode::Recording
            && job.frames != decision.state.capture_frames
        {
            return Err(Error::InvalidShape);
        }
        let media = self.ledger.reserve(media::Description {
            owner: decision.state.id,
            revision: decision.state.capture_revision,
            frames: 0,
            channels: row.binding.channels as u32,
            midi: row.binding.midi,
        })?;
        Ok(Some(Take {
            writer: Some(id),
            revision: decision.state.capture_revision,
            media,
            bank: 0,
            token: None,
        }))
    }
}
