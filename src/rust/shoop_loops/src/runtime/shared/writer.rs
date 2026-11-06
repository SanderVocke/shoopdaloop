use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LiveWriterId {
    scope: u32,
    slot: u32,
    generation: u32,
}
#[derive(Clone, Copy)]
struct WetWriter {
    resource: rt_runtime::ResourceId,
    revision: Revision,
    frames: u64,
    export: Option<ReservedSnapshotId>,
    windows: Windows,
    channels: usize,
}
#[derive(Clone, Copy)]
pub(in crate::runtime) struct LiveJob {
    pub autonomous: bool,
    processed: bool,
    wet: Option<WetWriter>,
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
impl LiveJob {
    pub(super) fn preparing(&self) -> bool {
        !self.ready
            && (!self.active || self.following_recording.is_some())
            && self.failure.is_none()
    }
}
pub(super) struct LiveSlot {
    pub generation: u32,
    pub reservation: Option<LoopId>,
    pub value: Option<LiveJob>,
}
impl Shared {
    pub(in crate::runtime) fn reserve_autonomous(&mut self, target: LoopId) -> Result<(), Error> {
        if self
            .live
            .iter()
            .any(|slot| slot.reservation == Some(target))
        {
            return Ok(());
        }
        let index = self
            .live
            .iter()
            .position(|slot| slot.value.is_some_and(|job| job.fence.target == target))
            .or_else(|| self.available_live_slot(target))
            .ok_or(Error::Capacity)?;
        self.live[index].reservation = Some(target);
        Ok(())
    }
    pub(in crate::runtime) fn release_unused_reservations(
        &mut self,
        needed: impl Fn(LoopId) -> bool,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        for index in 0..self.live.len() {
            let slot = &self.live[index];
            if slot.reservation.is_some_and(|target| !needed(target)) {
                if slot.value.is_some_and(|job| job.autonomous && !job.active) {
                    self.cancel_live(
                        LiveWriterId {
                            scope: self.scope,
                            slot: index as u32,
                            generation: slot.generation,
                        },
                        io,
                    )?;
                }
                self.live[index].reservation = None;
            }
        }
        Ok(())
    }
    fn available_live_slot(&self, target: LoopId) -> Option<usize> {
        let vacant = |slot: &LiveSlot| slot.generation > 0 && slot.value.is_none();
        self.live
            .iter()
            .position(|slot| vacant(slot) && slot.reservation == Some(target))
            .or_else(|| {
                self.live
                    .iter()
                    .position(|slot| vacant(slot) && slot.reservation.is_none())
            })
    }
    pub(in crate::runtime) fn rebind_prepared(
        &mut self,
        id: LiveWriterId,
        source: HistoryId,
        graph: Epoch,
        topology: Epoch,
    ) -> Result<(), Error> {
        let job = self.live_job(id)?;
        if !job.autonomous || job.active || job.source.source != source.source {
            return Err(Error::InvalidState);
        }
        let job = self.live[id.slot as usize].value.as_mut().unwrap();
        job.source = source;
        job.fence.graph = graph;
        job.fence.topology = topology;
        Ok(())
    }
    pub(in crate::runtime) fn configure_processed(
        &mut self,
        id: LiveWriterId,
        snapshot: Option<Snapshot>,
        windows: Windows,
        channels: usize,
        frame: Frame,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        let job = self.live_job(id)?;
        if job.wet.is_some()
            || job.kind != indexed_midi::WriteKind::Fixed && job.following_recording.is_none()
        {
            return Err(Error::InvalidState);
        }
        if let Some(snapshot) = snapshot {
            windows
                .validate_alignment(channels, false, snapshot.description.frames, 0, job.frames)
                .map_err(runtime_error)?;
            let store = io
                .audio_snapshot_store(snapshot.id)?
                .unwrap_or(self.audio_store(job.source.source)?);
            let resource = if snapshot.description.frames == 0 {
                io.empty_audio(store)?
            } else {
                io.fork_audio(store, snapshot.id)?
            };
            let export =
                match io.reserve_writer_snapshot(resource, frame, snapshot.description.frames) {
                    Ok(export) => export,
                    Err(error) => {
                        io.remove(resource)?;
                        return Err(error);
                    }
                };
            self.live[id.slot as usize].value.as_mut().unwrap().wet = Some(WetWriter {
                resource,
                revision: io.info(resource)?.revision,
                frames: snapshot.description.frames,
                export: Some(export),
                windows,
                channels,
            });
        }
        self.live[id.slot as usize]
            .value
            .as_mut()
            .unwrap()
            .processed = true;
        Ok(())
    }

    pub(in crate::runtime) fn cancel_orphaned_autonomous(
        &mut self,
        live_target: impl Fn(LoopId) -> bool,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        for index in 0..self.live.len() {
            let slot = &self.live[index];
            if slot
                .value
                .is_some_and(|job| job.autonomous && !live_target(job.fence.target))
            {
                let id = LiveWriterId {
                    scope: self.scope,
                    slot: index as u32,
                    generation: slot.generation,
                };
                self.cancel_live(id, io)?;
            }
        }
        Ok(())
    }

    pub(in crate::runtime) fn mark_autonomous(&mut self, id: LiveWriterId) -> Result<(), Error> {
        self.live_job(id)?;
        self.live[id.slot as usize]
            .value
            .as_mut()
            .unwrap()
            .autonomous = true;
        Ok(())
    }

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
                false,
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
        unobserved_source: bool,
    ) -> Result<LiveWriterId, Error> {
        if self.live_for(fence.target).is_some() {
            return Err(Error::Busy);
        }
        let status = self.status(source.source, fence.graph, io)?;
        if status.id != source {
            return Err(Error::InvalidId);
        }
        if let Some(error) = status.failure.filter(|_| !unobserved_source) {
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
            .available_live_slot(fence.target)
            .ok_or(Error::Capacity)?;
        let id = LiveWriterId {
            scope: self.scope,
            slot: slot as u32,
            generation: self.live[slot].generation,
        };
        let mut job = LiveJob {
            autonomous: false,
            processed: false,
            wet: None,
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
                let store = io
                    .audio_snapshot_store(snapshot.id)?
                    .or(audio_store)
                    .ok_or(Error::InvalidShape)?;
                let resource = if snapshot.description.frames == 0 {
                    io.empty_audio(store)?
                } else {
                    io.fork_audio(store, snapshot.id)?
                };
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
                job.audio_export = Some(io.reserve_writer_snapshot(resource, frame, maximum)?);
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
        self.live_building += usize::from(job.preparing());
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
            .available_live_slot(fence.target)
            .ok_or(Error::Capacity)?;
        let id = LiveWriterId {
            scope: self.scope,
            slot: slot as u32,
            generation: self.live[slot].generation,
        };
        let mut job = LiveJob {
            autonomous: false,
            processed: false,
            wet: None,
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
                job.midi_export = Some(io.reserve_writer_snapshot(resource, frame, u64::MAX)?);
            }
            Self::progress_live_job(&mut job, frame, 1, io)?;
            Ok(())
        })();
        self.live_building += usize::from(job.preparing());
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
            let resource = if snapshot.description.frames == 0 {
                io.empty_audio(store)?
            } else {
                io.fork_audio(store, snapshot.id)?
            };
            let result = (|| {
                let revision = io.info(resource)?.revision;
                let export = io.reserve_writer_snapshot(resource, frame, job.frames)?;
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
                job.midi_export = Some(io.reserve_writer_snapshot(resource, frame, maximum)?);
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
        if self.live_building == 0 {
            return;
        }
        let budget = if self.building > 0 {
            self.remaining.div_ceil(2)
        } else {
            self.remaining
        };
        for _ in 0..budget {
            if self.live_building == 0 {
                break;
            }
            let mut selected = None;
            for _ in 0..self.live.len() {
                let slot = self.live_turn;
                self.live_turn = (slot + 1) % self.live.len();
                self.work.jobs_scanned = self.work.jobs_scanned.saturating_add(1);
                if self.live[slot].value.is_some_and(|job| job.preparing()) {
                    selected = Some(slot);
                    break;
                }
            }
            let slot = selected.expect("counted live admission");
            self.remaining -= 1;
            self.work.edit_steps += 1;
            let job = self.live[slot].value.as_mut().unwrap();
            if let Err(error) = Self::progress_live_job(job, frame, 1, io) {
                job.failure = Some(error);
            }
            if !job.preparing() {
                self.live_building -= 1;
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
        self.write_live_targets(source, frame, q, None, io)
    }

    pub(in crate::runtime) fn write_processed(
        &mut self,
        id: LiveWriterId,
        frame: Frame,
        q: usize,
        ports: &AudioPorts<'_>,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        let job = self.live_job(id)?;
        if !job.processed {
            return Ok(());
        }
        let slot = self.lookup[job.source.source as usize].ok_or(Error::InvalidId)?;
        let h = &mut self.histories[slot];
        let channels = h
            .settings
            .audio
            .map_or(0, |config| config.channels as usize);
        for channel in 0..channels {
            let input = ports.input(channel).ok_or(Error::InvalidShape)?;
            if input.len() != q {
                return Err(Error::InvalidShape);
            }
            for (frame, sample) in input.iter().enumerate() {
                h.wet_scratch[frame * channels + channel] = *sample;
            }
        }
        self.write_live_targets(job.source.source, frame, q, Some(job.fence.target), io)
    }

    fn write_live_targets(
        &mut self,
        source: u32,
        frame: Frame,
        q: usize,
        target: Option<LoopId>,
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
            let Some(job) = entry.value.as_mut().filter(|job| {
                job.active
                    && job.failure.is_none()
                    && job.source.source == source
                    && match target {
                        Some(target) => job.processed && job.fence.target == target,
                        None => !job.processed,
                    }
            }) else {
                continue;
            };
            self.work.writer_ticks = self.work.writer_ticks.saturating_add(1);
            let result = (|| {
                if generation != job.source.generation || staged != Some(frame.checked_add(q)?) {
                    return Err(failure.unwrap_or(Error::StaleRevision));
                }
                if job.kind == indexed_midi::WriteKind::Fixed && job.position == job.frames {
                    // Observe MIDI input while waiting for the next sync pulse.
                    if let Some(writer) = job.writer {
                        io.render().write_indexed_midi_window_tick(
                            writer,
                            midi_source.ok_or(Error::InvalidShape)?,
                            frame,
                            indexed_midi::WriteSpan {
                                position: 0,
                                offset: 0,
                                frames: 0,
                            },
                        )?;
                    }
                    return Ok(());
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
                let wet_staged = (|| {
                    let Some(wet) = job.wet else { return Ok(None) };
                    let channels = h.settings.audio.ok_or(Error::InvalidShape)?.channels as usize;
                    let mut spans =
                        [shared_audio::ChannelWrite::default(); crate::windows::MAX_AUDIO_CHANNELS];
                    for (channel, destination) in spans[..wet.channels].iter_mut().enumerate() {
                        let span = crate::windows::write_span(
                            job.position,
                            wet.windows.audio[channel],
                            q,
                            wet.frames,
                        );
                        *destination = shared_audio::ChannelWrite {
                            channel,
                            position: span.position,
                            offset: span.offset,
                            frames: span.frames,
                        };
                    }
                    io.render()
                        .stage_audio_window_write(
                            wet.resource,
                            wet.revision,
                            &h.wet_scratch[..q * channels],
                            &spans[..wet.channels],
                        )
                        .map(Some)
                })();
                let wet_staged = match wet_staged {
                    Ok(write) => write,
                    Err(error) => {
                        if let Some(write) = staged {
                            io.render().cancel_audio_write(write)?;
                        }
                        return Err(error);
                    }
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
                        if let Some(write) = wet_staged {
                            io.render().cancel_audio_write(write)?;
                        }
                        if let Some(write) = staged {
                            io.render().cancel_audio_write(write)?;
                        }
                        return Err(error);
                    }
                }
                if let Some(write) = staged {
                    job.audio_revision = io.render().commit_audio_write(write)?;
                }
                if let Some(write) = wet_staged {
                    job.wet.as_mut().ok_or(Error::InvalidState)?.revision =
                        io.render().commit_audio_write(write)?;
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
                self.live_building -= usize::from(job.preparing());
                job.failure = Some(error);
                self.work.writer_failures = self.work.writer_failures.saturating_add(1);
            }
        }
        Ok(())
    }
    pub fn seek_live(&mut self, id: LiveWriterId, position: u64) -> Result<(), Error> {
        let job = self.live_job(id)?;
        if !job.active || job.kind != indexed_midi::WriteKind::Fixed || position > job.frames {
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
    ) -> Result<[Option<Snapshot>; 3], Error> {
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
        let midi = match midi {
            Ok(midi) => midi,
            Err(error) => {
                if let Some(audio) = audio {
                    io.release_snapshot(audio.id)?;
                }
                return Err(error);
            }
        };
        let wet = job
            .wet
            .map(|wet| io.snapshot(wet.resource, frame))
            .transpose();
        match wet {
            Ok(wet) if job.processed => Ok([wet, audio, midi]),
            Ok(_) => Ok([audio, None, midi]),
            Err(error) => {
                for snapshot in [audio, midi].into_iter().flatten() {
                    io.release_snapshot(snapshot.id)?;
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
    ) -> Result<(u64, [Option<Snapshot>; 3]), Error> {
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
        let wet = job
            .wet
            .as_mut()
            .and_then(|wet| wet.export.take())
            .map(|export| io.publish_reserved_snapshot(export, frame))
            .transpose()?;
        let frames = job.frames;
        self.live[id.slot as usize].value = Some(job);
        self.cancel_live(id, io)?;
        Ok((
            frames,
            if job.processed {
                [wet, audio, midi]
            } else {
                [audio, None, midi]
            },
        ))
    }
    pub fn cancel_live(&mut self, id: LiveWriterId, io: &mut ResourceIo<'_>) -> Result<(), Error> {
        let job = self.live_job(id)?;
        if let Some(wet) = job.wet {
            if let Some(export) = wet.export {
                io.cancel_reserved_snapshot(export)?;
            }
            io.remove(wet.resource)?;
        }
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
        self.live_building -= usize::from(job.preparing());
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
            self.live[index].reservation = None;
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
            || !job.autonomous && job.fence.topology != self.resolver.epoch()
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
            auxiliary_channels: if row.dry_wet {
                row.binding.auxiliary_channels as u32
            } else {
                0
            },
            owner: decision.state.id,
            revision: decision.state.capture_revision,
            frames: 0,
            channels: row.binding.channels as u32,
            midi: row.binding.midi,
        })?;
        if job.following_recording.is_some() {
            let result = (|| {
                if row.dry_wet && !job.processed {
                    let snapshot = if row.binding.channels > 0 {
                        Some(
                            io.snapshot(
                                row.resources[row.active.ok_or(Error::InvalidState)?.bank][1][0]
                                    .ok_or(Error::InvalidState)?,
                                self.resolver.frame(),
                            )?,
                        )
                    } else {
                        None
                    };
                    let mut wet = Windows::default();
                    let width = row.audio_widths[0];
                    wet.audio[..width].copy_from_slice(
                        &row.windows.audio[row.audio_widths[1]..row.audio_widths[1] + width],
                    );
                    let result = self.shared.as_mut().unwrap().configure_processed(
                        id,
                        snapshot,
                        wet,
                        width,
                        self.resolver.frame(),
                        io,
                    );
                    if let Some(snapshot) = snapshot {
                        io.release_snapshot(snapshot.id)?;
                    }
                    result?;
                }
                let audio = if if row.dry_wet {
                    row.binding.auxiliary_channels > 0
                } else {
                    row.binding.channels > 0
                } {
                    Some(
                        io.snapshot(
                            row.resources[row.active.ok_or(Error::InvalidState)?.bank]
                                [usize::from(row.dry_wet)][usize::from(row.dry_wet)]
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
            started_at: self.resolver.frame(),
            windows: if row.dry_wet {
                row.windows
            } else {
                job.windows
            },
            writer: Some(id),
            edit: None,
            snapshot_credit: None,
            auxiliary: None,
            midi: None,
            revision: decision.state.capture_revision,
            media,
            bank: 0,
            token: None,
        }))
    }
}
