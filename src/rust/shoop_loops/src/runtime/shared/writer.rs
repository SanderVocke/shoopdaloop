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
    pub windows: Windows,
    audio_frames: u64,
    midi_frames: u64,
    audio: Option<rt_runtime::ResourceId>,
    midi: Option<rt_runtime::ResourceId>,
    audio_revision: Revision,
    audio_export: Option<ReservedSnapshotId>,
    midi_export: Option<ReservedSnapshotId>,
    import: Option<indexed_midi::ImportId>,
    writer: Option<indexed_midi::WriterId>,
    pub ready: bool,
    pub active: bool,
    pub following_recording: Option<Revision>,
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
                slot.value.is_some_and(|job| {
                    job.fence.target == target && (!job.active || job.following_recording.is_some())
                })
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
        growth_owners: usize,
        windows: Windows,
        frame: Frame,
        io: &mut ResourceIo<'_>,
    ) -> Result<LiveWriterId, Error> {
        let mut snapshots = [None; 2];
        let result = (|| {
            for (member, resource) in [take.audio, take.midi].into_iter().enumerate() {
                if let Some(resource) = resource {
                    snapshots[member] = Some(io.snapshot(resource, frame)?);
                }
            }
            self.begin_live_snapshots(
                fence,
                source,
                take.frames,
                snapshots,
                kind,
                growth_owners,
                windows,
                frame,
                io,
            )
        })();
        for snapshot in snapshots.into_iter().flatten() {
            io.release_snapshot(snapshot.id)?;
        }
        result
    }
    pub fn begin_live_snapshots(
        &mut self,
        fence: Fence,
        source: HistoryId,
        frames: u64,
        snapshots: [Option<Snapshot>; 2],
        kind: indexed_midi::WriteKind,
        growth_owners: usize,
        windows: Windows,
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
        if matches!(kind, indexed_midi::WriteKind::Fixed) && frames == 0 {
            return Err(Error::InvalidShape);
        }
        let h = self.source(source.source)?;
        let audio_store = h.audio;
        let channels = if snapshots[0].is_some() {
            h.settings.audio.map_or(0, |audio| audio.channels as usize)
        } else {
            0
        };
        let midi_store = h.indexed_midi.map(|id| id.store());
        if snapshots[0].is_some() && audio_store.is_none()
            || snapshots[1].is_some() && midi_store.is_none()
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
            frames: frames,
            position: 0,
            windows,
            audio_frames: 0,
            midi_frames: 0,
            audio: None,
            midi: None,
            audio_revision: Revision(0),
            audio_export: None,
            midi_export: None,
            import: None,
            writer: None,
            ready: false,
            active: false,
            following_recording: None,
            failure: None,
        };
        let mut maximum = match kind {
            indexed_midi::WriteKind::Fixed => frames,
            indexed_midi::WriteKind::Growing { maximum_frames: 0 } => u64::MAX,
            indexed_midi::WriteKind::Growing { maximum_frames } => maximum_frames,
        };
        job.kind = if matches!(kind, indexed_midi::WriteKind::Growing { .. }) {
            indexed_midi::WriteKind::Growing {
                maximum_frames: maximum,
            }
        } else {
            kind
        };
        let result = (|| {
            if let Some(snapshot) = snapshots[0] {
                if matches!(kind, indexed_midi::WriteKind::Growing { .. })
                    && snapshot.description.frames != frames
                {
                    return Err(Error::InvalidShape);
                }
                job.audio_frames = snapshot.description.frames;
                let resource =
                    io.fork_audio(audio_store.ok_or(Error::InvalidShape)?, snapshot.id)?;
                job.audio = Some(resource);
                job.audio_revision = io.info(resource)?.revision;
                if matches!(kind, indexed_midi::WriteKind::Growing { .. }) {
                    if matches!(kind, indexed_midi::WriteKind::Growing { maximum_frames: 0 }) {
                        let quantum = self.max_frames as u64;
                        maximum =
                            io.audio_growth_limit(resource, growth_owners)? / quantum * quantum;
                        if maximum < frames.checked_add(quantum).ok_or(Error::Capacity)? {
                            return Err(Error::Capacity);
                        }
                        job.kind = indexed_midi::WriteKind::Growing {
                            maximum_frames: maximum,
                        };
                    }
                    io.prepare_audio_growth(resource, job.audio_revision, maximum)?;
                }
                let maximum = if kind == indexed_midi::WriteKind::Fixed {
                    job.audio_frames
                } else {
                    maximum
                };
                job.audio_export = Some(io.reserve_snapshot(resource, frame, maximum)?);
            }
            if let Some(snapshot) = snapshots[1] {
                let store = midi_store.ok_or(Error::InvalidShape)?;
                if matches!(kind, indexed_midi::WriteKind::Growing { .. })
                    && snapshot.description.frames != frames
                {
                    return Err(Error::InvalidShape);
                }
                job.midi_frames = snapshot.description.frames;
                match io.fork_indexed_midi(store, snapshot.id) {
                    Ok(resource) => job.midi = Some(resource),
                    Err(Error::InvalidShape) => {
                        let index = io.read_index(snapshot.id)?;
                        let initial = io.render().midi_seek(index, 0)?.state;
                        job.import =
                            Some(io.prepare_indexed_midi_import(store, snapshot.id, &initial)?);
                    }
                    Err(error) => return Err(error),
                }
            }
            if kind == indexed_midi::WriteKind::Fixed {
                if channels > crate::windows::MAX_AUDIO_CHANNELS {
                    return Err(Error::Capacity);
                }
                windows
                    .validate_alignment(
                        channels,
                        job.midi.is_some() || job.import.is_some(),
                        job.audio_frames,
                        job.midi_frames,
                        frames,
                    )
                    .map_err(runtime_error)?;
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
    pub fn begin_recording(
        &mut self,
        fence: Fence,
        source: HistoryId,
        revision: Revision,
        frames: u64,
        windows: Windows,
        midi: Option<Snapshot>,
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
        if status.end != frame {
            return Err(Error::Busy);
        }
        let h = self.source(source.source)?;
        let channels = h.settings.audio.map_or(0, |audio| audio.channels as usize);
        if windows
            .audio
            .iter()
            .take(channels)
            .chain(midi.map(|_| &windows.midi))
            .any(|window| window.origin() != 0 || window.raw_frames.is_some())
        {
            return Err(Error::InvalidShape);
        }
        let store = h.indexed_midi.map(|source| source.store());
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
            kind: indexed_midi::WriteKind::Growing {
                maximum_frames: u64::MAX,
            },
            frames,
            position: frames,
            windows,
            audio_frames: 0,
            midi_frames: frames,
            audio: None,
            midi: None,
            audio_revision: Revision(0),
            audio_export: None,
            midi_export: None,
            import: None,
            writer: None,
            ready: midi.is_none(),
            active: true,
            following_recording: Some(revision),
            failure: None,
        };
        let result = (|| {
            if let Some(snapshot) = midi {
                if snapshot.description.frames != frames {
                    return Err(Error::InvalidShape);
                }
                let index = io.read_index(snapshot.id)?;
                let initial = io.render().midi_seek(index, 0)?.state;
                let (resource, writer, import) = io.prepare_indexed_midi_recording(
                    store.ok_or(Error::InvalidShape)?,
                    snapshot.id,
                    &initial,
                )?;
                job.midi = Some(resource);
                job.writer = Some(writer);
                job.import = Some(import);
                job.midi_export = Some(io.reserve_snapshot(resource, frame, u64::MAX)?);
            }
            Ok(())
        })();
        self.live[slot].value = Some(job);
        if let Err(error) = result {
            self.cancel_live(id, io)?;
            return Err(error);
        }
        Ok(id)
    }
    fn attach_recording_audio(
        &mut self,
        id: LiveWriterId,
        snapshot: Option<Snapshot>,
        frame: Frame,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        let job = self.live_job(id)?;
        if !job.ready || job.following_recording.is_none() {
            return Err(Error::Busy);
        }
        if let Some(snapshot) = snapshot {
            if snapshot.description.frames != job.frames {
                return Err(Error::InvalidShape);
            }
            let store = self
                .source(job.source.source)?
                .audio
                .ok_or(Error::InvalidShape)?;
            let resource = io.fork_audio(store, snapshot.id)?;
            let result = (|| {
                let revision = io.info(resource)?.revision;
                let export = io.reserve_snapshot(resource, frame, job.frames)?;
                Ok((revision, export))
            })();
            match result {
                Ok((revision, export)) => {
                    let job = self.live[id.slot as usize].value.as_mut().unwrap();
                    job.audio = Some(resource);
                    job.audio_revision = revision;
                    job.audio_export = Some(export);
                }
                Err(error) => {
                    io.remove(resource)?;
                    return Err(error);
                }
            }
        }
        let job = self.live[id.slot as usize].value.as_mut().unwrap();
        job.audio_frames = job.frames;
        job.midi_frames = job.frames;
        Ok(())
    }
    fn progress_live_job(
        job: &mut LiveJob,
        frame: Frame,
        steps: usize,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        if job.ready || job.active && job.following_recording.is_none() || job.failure.is_some() {
            return Ok(());
        }
        if let Some(import) = job.import {
            if io.progress_indexed_midi_import(import, steps)? != indexed_midi::Status::Ready {
                return Ok(());
            }
            if job.following_recording.is_some() {
                io.finish_indexed_midi_recording_import(import)?;
            } else {
                job.midi = Some(io.finish_indexed_midi_import(import)?);
            }
            job.import = None;
        }
        if let Some(resource) = job.midi {
            if job.writer.is_none() {
                let revision = io.info(resource)?.revision;
                job.writer = Some(if job.kind == indexed_midi::WriteKind::Fixed {
                    io.prepare_indexed_midi_window_writer(resource, revision)?
                } else {
                    io.prepare_indexed_midi_writer(resource, revision, job.kind)?
                });
                let maximum = match job.kind {
                    indexed_midi::WriteKind::Fixed => job.midi_frames,
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
                if self.live[slot].value.is_some_and(|job| {
                    !job.ready
                        && (!job.active || job.following_recording.is_some())
                        && job.failure.is_none()
                }) {
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
        if job.active && job.following_recording.is_none() || !job.ready {
            return Err(Error::Busy);
        }
        if let Some(error) = job.failure {
            return Err(error);
        }
        if let Some(writer) = job.writer {
            if job.following_recording.is_some() {
                io.fix_indexed_midi_writer_window(writer)?;
            } else {
                io.start_indexed_midi_writer(
                    writer,
                    if job.kind == indexed_midi::WriteKind::Fixed {
                        0
                    } else {
                        position
                    },
                )?;
            }
        }
        let job = self.live[id.slot as usize].value.as_mut().unwrap();
        if job.following_recording.take().is_some() {
            job.kind = indexed_midi::WriteKind::Fixed;
        }
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
                    if job.kind == indexed_midi::WriteKind::Fixed {
                        let mut spans = [shared_audio::ChannelWrite::default();
                            crate::windows::MAX_AUDIO_CHANNELS];
                        for (channel, destination) in spans[..channels].iter_mut().enumerate() {
                            let span = crate::windows::write_span(
                                job.position,
                                job.windows.audio[channel],
                                q,
                                job.audio_frames,
                            );
                            *destination = shared_audio::ChannelWrite {
                                channel,
                                position: span.position,
                                offset: span.offset,
                                frames: span.frames,
                            };
                        }
                        Some(io.render().stage_audio_window_write(
                            resource,
                            job.audio_revision,
                            &h.audio_scratch[..q * channels],
                            &spans[..channels],
                        )?)
                    } else {
                        Some(io.render().stage_audio_write(
                            resource,
                            job.audio_revision,
                            None,
                            &h.audio_scratch[..q * channels],
                        )?)
                    }
                } else {
                    None
                };
                if let Some(writer) = job.writer {
                    let result = if job.kind == indexed_midi::WriteKind::Fixed {
                        let span = crate::windows::write_span(
                            job.position,
                            job.windows.midi,
                            q,
                            job.midi_frames,
                        );
                        io.render().write_indexed_midi_window_tick(
                            writer,
                            midi_source.ok_or(Error::InvalidShape)?,
                            frame,
                            indexed_midi::WriteSpan {
                                position: span.position,
                                offset: span.offset,
                                frames: span.frames,
                            },
                        )
                    } else {
                        io.render()
                            .write_indexed_midi_tick(
                                writer,
                                midi_source.ok_or(Error::InvalidShape)?,
                                frame,
                            )
                            .map(|_| ())
                    };
                    if let Err(error) = result {
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
    pub fn seek_live(&mut self, id: LiveWriterId, position: u64) -> Result<(), Error> {
        let job = self.live_job(id)?;
        if !job.active || job.kind != indexed_midi::WriteKind::Fixed || position >= job.frames {
            return Err(Error::InvalidFrame);
        }
        if job.position == position {
            return Ok(());
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
            1,
            job.windows,
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
        if job.active && job.following_recording.is_none() {
            return Ok(None);
        }
        if job.following_recording.is_some() && decision.state.mode != Mode::Replacing {
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
        if (job.kind == indexed_midi::WriteKind::Fixed || job.following_recording.is_some())
            && job.frames != decision.state.content_length
        {
            return Err(Error::InvalidShape);
        }
        if job.following_recording.is_some_and(|revision| {
            before.mode != Mode::Recording
                || before.capture_revision != revision
                || before.capture_frames != job.frames
        }) {
            return Err(Error::StaleRevision);
        }
        if job.following_recording.is_none()
            && !matches!(
                (job.kind, decision.state.mode),
                (indexed_midi::WriteKind::Fixed, Mode::Replacing)
                    | (indexed_midi::WriteKind::Growing { .. }, Mode::Recording)
            )
            || decision.state.mode == Mode::Recording && job.frames != decision.state.capture_frames
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
        if job.following_recording.is_some() {
            let result = (|| {
                let audio = if row.binding.channels > 0 {
                    Some(
                        io.snapshot(
                            row.resources[row.active.ok_or(Error::InvalidState)?.bank][0]
                                .ok_or(Error::InvalidState)?,
                            self.resolver.frame(),
                        )?,
                    )
                } else {
                    None
                };
                let result = self
                    .shared
                    .as_mut()
                    .ok_or(Error::InvalidState)?
                    .attach_recording_audio(id, audio, self.resolver.frame(), io);
                if let Some(audio) = audio {
                    io.release_snapshot(audio.id)?;
                }
                result
            })();
            if let Err(error) = result {
                self.ledger.retire(io, media)?;
                return Err(error);
            }
        }
        Ok(Some(Take {
            windows: job.windows,
            writer: Some(id),
            revision: decision.state.capture_revision,
            media,
            bank: 0,
            token: None,
        }))
    }
}
