use super::*;
use rt_runtime::Sample;

impl<S: Sample> LoopPlanner<S> {
    fn validate_graph(&self, graph: &PreparedPlan) -> Result<(), Error> {
        for row in self.rows.iter().flatten() {
            for consumer in [row.binding.playback, row.binding.capture] {
                if graph.contains(consumer) != row.assigned {
                    return Err(Error::InvalidShape);
                }
            }
        }
        Ok(())
    }
    fn activate_graph(
        &mut self,
        graph: GraphStageId,
        topology: Option<StageId>,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
        graphs: &mut rt_runtime::graph::activation::GraphAccess<'_>,
    ) -> Result<(Epoch, Epoch), Failure> {
        if topology_epoch != self.resolver.epoch() {
            return Err(crate::Error::InvalidState.into());
        }
        let permit = graphs.prepare_activation(graph, graph_epoch)?;
        self.validate_graph(permit.candidate())?;
        if let Some(id) = topology {
            self.topologies
                .activate(id, topology_epoch, &mut self.resolver)?;
        }
        self.graph_epoch = permit.commit();
        Ok((self.graph_epoch, self.resolver.epoch()))
    }
    pub(super) fn row(&self, target: LoopId) -> Result<Row<S>, Error> {
        self.rows
            .get(target.slot() as usize)
            .and_then(|r| *r)
            .filter(|r| r.assigned && r.binding.target == target)
            .ok_or(Error::InvalidId)
    }
    /// Borrow a render-owned state cut at a settled boundary without acquiring media leases.
    pub fn cut(&self, target: LoopId, frame: Frame) -> Result<Cut<S>, Failure> {
        let state = self.resolver.state(target)?;
        if let Ok(row) = self.row(target) {
            let media = self
                .ledger
                .playback(target)?
                .map(|id| self.ledger.view(id))
                .transpose()?;
            Ok(Cut {
                raw_audio_frames: media
                    .and_then(|m| m.audio)
                    .map_or(0, |source| source.snapshot.description.frames),
                raw_midi_frames: media
                    .and_then(|m| m.midi)
                    .map_or(0, |source| source.snapshot.description.frames),
                windows: row.windows,
                timing_revision: row.timing_revision,
                frame,
                graph_epoch: self.graph_epoch,
                topology_epoch: self.resolver.epoch(),
                state,
                playback: self.ledger.playback(target)?,
                latest: self.ledger.latest(target)?,
                gain: row.binding.gain,
                balance: row.balance,
                smoothing: row.binding.smoothing,
                capture_failure: row.failure,
                midi_overflows: row.overflows,
            })
        } else {
            Ok(Cut {
                raw_audio_frames: 0,
                raw_midi_frames: 0,
                windows: Windows::default(),
                timing_revision: 0,
                frame,
                graph_epoch: self.graph_epoch,
                topology_epoch: self.resolver.epoch(),
                state,
                playback: None,
                latest: None,
                gain: S::ONE,
                balance: S::ZERO,
                smoothing: crate::audio::Smoothing::None,
                capture_failure: None,
                midi_overflows: 0,
            })
        }
    }
    fn pending_slot(&self, recovery: bool) -> Result<usize, Error> {
        if recovery {
            let slot = self.settings.normal_requests;
            if self.pending[slot].is_none() {
                Ok(slot)
            } else {
                Err(Error::Capacity)
            }
        } else {
            self.pending[..self.settings.normal_requests]
                .iter()
                .position(Option::is_none)
                .ok_or(Error::Capacity)
        }
    }
    fn cancel_ticks(&mut self, io: &mut PlannerResourceIo<'_, S>) -> Result<(), Error> {
        for slot in 0..self.rows.len() {
            if let Some(row) = self.rows[slot] {
                if let Some(interval) = row.interval {
                    io.capture().cancel_tick(interval)?;
                    self.rows[slot].as_mut().expect("row").interval = None;
                }
            }
        }
        Ok(())
    }
    fn cancel_prepared(
        &mut self,
        slot: usize,
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<(), Error> {
        if let Some(take) = self.rows[slot]
            .as_ref()
            .and_then(|r| r.prepared.as_ref())
            .copied()
        {
            if let Some(id) = take.writer {
                self.shared
                    .as_mut()
                    .ok_or(Error::InvalidState)?
                    .cancel_live(id, io)?;
            }
            if let Some(token) = take.token {
                io.capture().cancel_take(token)?;
            }
            Self::release_edit(take, io)?;
            self.ledger.retire(io, take.media)?;
            self.rows[slot].as_mut().expect("row").prepared = None;
        }
        Ok(())
    }
    fn release_edit(take: Take, io: &mut ResourceIo<'_, S>) -> Result<(), Error> {
        for source in [take.auxiliary, take.midi].into_iter().flatten() {
            io.release_snapshot(source.snapshot.id)?;
        }
        if let Some((resource, _)) = take.edit {
            io.remove(resource)?;
        }
        if let Some(credit) = take.snapshot_credit {
            io.cancel_snapshot_reservation(credit)?;
        }
        Ok(())
    }
    fn prepare_wet_edit(
        &mut self,
        slot: usize,
        decision: Decision,
        id: media::MediaId,
        bank: usize,
        io: &mut ResourceIo<'_, S>,
    ) -> Result<Take, Error> {
        let row = self.rows[slot].ok_or(Error::InvalidId)?;
        if !row.dry_wet {
            return Err(Error::InvalidState);
        }
        let view = self.ledger.view(
            self.ledger
                .playback(row.binding.target)?
                .ok_or(Error::InvalidState)?,
        )?;
        let store = self
            .wet_edits
            .or_else(|| {
                self.shared
                    .as_ref()
                    .and_then(|shared| shared.audio_store(row.history_source).ok())
            })
            .ok_or(Error::InvalidState)?;
        let audio = view.audio.ok_or(Error::InvalidShape)?;
        if audio.snapshot.description.frames < decision.state.content_length {
            return Err(Error::InvalidShape);
        }
        let mut windows = row.windows;
        for window in
            &mut windows.audio[row.audio_widths[1]..row.audio_widths[1] + row.audio_widths[0]]
        {
            if i128::from(window.start_offset) + i128::from(decision.state.content_length)
                > i128::from(
                    window
                        .raw_frames
                        .unwrap_or(audio.snapshot.description.frames),
                )
            {
                return Err(Error::InvalidShape);
            }
            window.capture_alignment = 0;
        }
        let credit = io.reserve_snapshot()?;
        let resource = match io.fork_audio(store, audio.snapshot.id) {
            Ok(resource) => resource,
            Err(error) => {
                io.cancel_snapshot_reservation(credit)?;
                return Err(error);
            }
        };
        let mut take = Take {
            started_at: self.resolver.frame(),
            writer: None,
            windows,
            revision: decision.state.capture_revision,
            media: id,
            bank,
            token: None,
            edit: Some((resource, io.info(resource)?.revision)),
            snapshot_credit: Some(credit),
            auxiliary: None,
            midi: None,
        };
        let result = (|| {
            if let Some(source) = view.auxiliary {
                let snapshot = io.retain_snapshot(source.snapshot.id)?;
                take.auxiliary = Some(media::Source {
                    snapshot,
                    index: io.read_index(snapshot.id)?,
                });
            }
            if let Some(source) = view.midi {
                let snapshot = io.retain_snapshot(source.snapshot.id)?;
                take.midi = Some(media::Source {
                    snapshot,
                    index: io.read_index(snapshot.id)?,
                });
            }
            Ok(take)
        })();
        if result.is_err() {
            Self::release_edit(take, io)?;
        }
        result
    }
    fn prepare_take(
        &mut self,
        slot: usize,
        decision: Decision,
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<(), Error> {
        let row = self.rows[slot].ok_or(Error::InvalidId)?;
        if let Some(take) = self.prepare_live_take(slot, decision, io)? {
            self.rows[slot].as_mut().unwrap().prepared = Some(take);
            return Ok(());
        }
        if decision.state.mode == Mode::Replacing || decision.state.capture_frames != 0 {
            return Err(Error::Capacity);
        }
        let id = self.ledger.reserve(media::Description {
            auxiliary_channels: if row.dry_wet {
                row.binding.auxiliary_channels as u32
            } else {
                0
            },
            owner: row.binding.target,
            revision: decision.state.capture_revision,
            frames: 0,
            channels: row.binding.channels as u32,
            midi: row.binding.midi,
        })?;
        let bank = row.active.map_or(row.next_bank, |take| 1 - take.bank);
        if decision.state.mode == Mode::RecordingDryIntoWet {
            match self.prepare_wet_edit(slot, decision, id, bank, io) {
                Ok(take) => self.rows[slot].as_mut().expect("row").prepared = Some(take),
                Err(error) => {
                    self.ledger.retire(io, id)?;
                    return Err(error);
                }
            }
            return Ok(());
        }
        let token = if let Some(group) = row.groups[bank][usize::from(row.dry_wet)] {
            match io.capture().prepare(
                group,
                if row.binding.midi {
                    core::slice::from_ref(&self.inputs[slot])
                } else {
                    &[]
                },
            ) {
                Ok(token) => Some(token),
                Err(error) => {
                    self.ledger.retire(io, id)?;
                    return Err(error);
                }
            }
        } else {
            None
        };
        let mut windows = row.windows;
        for channel in windows
            .audio
            .iter_mut()
            .chain(core::iter::once(&mut windows.midi))
        {
            if !row.dry_wet {
                channel.start_offset = 0;
            }
            channel.capture_alignment = 0;
            channel.raw_frames = None;
        }
        self.rows[slot].as_mut().expect("row").prepared = Some(Take {
            started_at: self.resolver.frame(),
            windows,
            writer: None,
            revision: decision.state.capture_revision,
            media: id,
            bank,
            token,
            edit: None,
            snapshot_credit: None,
            auxiliary: None,
            midi: None,
        });
        Ok(())
    }
    fn latch(
        &mut self,
        slot: usize,
        decision: Decision,
        error: Error,
        starting: bool,
    ) -> Result<(), Error> {
        let continuous = self.rows[slot]
            .and_then(|row| row.active)
            .is_some_and(|take| take.writer.is_some());
        if !matches!(error, Error::Capacity | Error::Exhausted) && !starting && !continuous {
            return Err(error);
        }
        let target = decision.state.id;
        let observation = if starting {
            decision.origin.map_or(
                crate::Observation::CaptureUnavailable {
                    target,
                    reason: timeline_error(error),
                },
                |request| crate::Observation::Refused {
                    request,
                    reason: timeline_error(error),
                },
            )
        } else {
            crate::Observation::CaptureExhausted { target }
        };
        let duplicate = self.failures.iter().any(|old| match (*old, observation) {
            (
                crate::Observation::CaptureExhausted { target: a },
                crate::Observation::CaptureExhausted { target: b },
            )
            | (
                crate::Observation::CaptureUnavailable { target: a, .. },
                crate::Observation::CaptureUnavailable { target: b, .. },
            ) => a == b,
            (
                crate::Observation::Refused { request: a, .. },
                crate::Observation::Refused { request: b, .. },
            ) => a == b,
            _ => false,
        });
        if duplicate || self.failures.len() == self.failures.capacity() {
            return Err(Error::InvalidState);
        }
        self.failures.push(observation);
        self.rows[slot].as_mut().expect("row").failure = Some(error);
        Ok(())
    }
    fn preflight(
        &mut self,
        frame: Frame,
        end: Frame,
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<bool, Error> {
        if frame.checked_add(self.resolver.quantum().frames())? != end {
            return Err(Error::InvalidFrame);
        }
        if !self.preflight_input_batch()? {
            return Ok(false);
        }
        for slot in 0..self.rows.len() {
            // Borrow rather than copy: rows are large and this runs every quantum.
            let Some(current) = self.rows[slot].as_ref().filter(|row| row.assigned) else {
                continue;
            };
            let decision = self.views[slot].decision;
            let recording = decision.state.writes();
            let starting = recording
                && current
                    .active
                    .as_ref()
                    .is_none_or(|take| take.revision != decision.state.capture_revision);
            if current
                .prepared
                .as_ref()
                .is_some_and(|take| !starting || take.revision != decision.state.capture_revision)
            {
                self.cancel_prepared(slot, io)?;
            }
            if !recording {
                continue;
            }
            let mut row = self.rows[slot].expect("row");
            if starting && row.prepared.is_none() {
                if let Err(error) = self.prepare_take(slot, decision, io) {
                    self.latch(slot, decision, error, true)?;
                    return Ok(false);
                }
                row = self.rows[slot].expect("row");
            }
            let take =
                if starting { row.prepared } else { row.active }.ok_or(Error::InvalidState)?;
            if let Some(id) = take.writer {
                if let Some(error) = self
                    .shared
                    .as_ref()
                    .ok_or(Error::InvalidState)?
                    .live_job(id)?
                    .failure
                {
                    self.latch(slot, decision, error, starting)?;
                    return Ok(false);
                }
            }
            if let Some(token) = take.token {
                let result = if starting {
                    io.capture().reserve_prepared(token, frame)
                } else {
                    io.capture().reserve(token, frame)
                };
                match result {
                    Ok(interval) => {
                        self.rows[slot].as_mut().expect("row").interval = Some(interval)
                    }
                    Err(error) => {
                        self.latch(slot, decision, error, starting)?;
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }
    fn input_from_frozen(
        &self,
        slot: usize,
        take: Take,
        frames: u64,
        io: &mut ResourceIo<'_, S>,
    ) -> Result<media::Input, Error> {
        let row = self.rows[slot].ok_or(Error::InvalidId)?;
        let mut position = 0;
        let mut source = || -> Result<Option<media::Source>, Error> {
            let snapshot = row.frozen[position].ok_or(Error::InvalidState)?;
            position += 1;
            Ok(Some(media::Source {
                snapshot,
                index: io.read_index(snapshot.id)?,
            }))
        };
        if let Some((resource, _)) = take.edit {
            let snapshot = io.snapshot_reserved(
                take.snapshot_credit.ok_or(Error::InvalidState)?,
                resource,
                self.resolver.frame(),
            )?;
            return Ok(media::Input {
                description: media::Description {
                    owner: row.binding.target,
                    revision: take.revision,
                    frames: self
                        .resolver
                        .state(row.binding.target)
                        .map_err(runtime_error)?
                        .content_length,
                    channels: row.binding.channels as u32,
                    auxiliary_channels: row.binding.auxiliary_channels as u32,
                    midi: row.binding.midi,
                },
                audio: Some(media::Source {
                    snapshot,
                    index: io.read_index(snapshot.id)?,
                }),
                auxiliary: take.auxiliary,
                midi: take.midi,
            });
        }
        let audio = if row.binding.channels > 0 {
            source()?
        } else {
            None
        };
        let auxiliary = if row.dry_wet && row.binding.auxiliary_channels > 0 {
            source()?
        } else {
            None
        };
        let midi = if row.binding.midi { source()? } else { None };
        Ok(media::Input {
            auxiliary,
            description: media::Description {
                auxiliary_channels: if row.dry_wet {
                    row.binding.auxiliary_channels as u32
                } else {
                    0
                },
                owner: row.binding.target,
                revision: take.revision,
                frames,
                channels: row.binding.channels as u32,
                midi: row.binding.midi,
            },
            audio,
            midi,
        })
    }
    fn freeze(
        &mut self,
        slot: usize,
        frame: Frame,
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<media::MediaId, Error> {
        let row = self.rows[slot].ok_or(Error::InvalidId)?;
        let take = row.active.ok_or(Error::InvalidState)?;
        let incomplete=self.failures.iter().any(|o|matches!(o,crate::Observation::CaptureExhausted { target } if *target==row.binding.target));
        if let Some(id) = take.writer {
            let (frames, sources) = self
                .shared
                .as_mut()
                .ok_or(Error::InvalidState)?
                .finish_live(id, frame, io)?;
            let expected = self
                .resolver
                .state(row.binding.target)
                .map_err(runtime_error)?
                .capture_frames;
            if frames != expected {
                return Err(Error::InvalidState);
            }
            let frozen = &mut self.rows[slot].as_mut().unwrap().frozen;
            *frozen = [None; 3];
            for (index, source) in sources.into_iter().flatten().enumerate() {
                frozen[index] = Some(source);
            }
        }
        if let Some(token) = take.token {
            let mut output = [None; 3];
            let members = usize::from(row.binding.channels > 0)
                + usize::from(row.dry_wet && row.binding.auxiliary_channels > 0)
                + usize::from(row.binding.midi);
            io.capture().freeze(
                token,
                frame,
                if incomplete { row.failure } else { None },
                &mut output[..members],
            )?;
            self.rows[slot].as_mut().expect("row").frozen = output;
        }
        self.rows[slot].as_mut().expect("row").active = None;
        let frames = self
            .resolver
            .state(row.binding.target)
            .map_err(runtime_error)?
            .capture_frames;
        let input = self.input_from_frozen(slot, take, frames, io)?;
        self.ledger
            .publish_windowed(io, take.media, input, take.windows)
            .map_err(|e| e.error)?;
        if let Some((resource, _)) = take.edit {
            io.remove(resource)?;
        }
        self.rows[slot].as_mut().expect("row").frozen = [None; 3];
        self.ledger
            .set_latest(io, row.binding.target, Some(take.media))?;
        self.ledger.retire(io, take.media)?;
        Ok(take.media)
    }
    fn settle(&mut self, frame: Frame, io: &mut PlannerResourceIo<'_, S>) -> Result<(), Error> {
        for slot in 0..self.rows.len() {
            // Read only what is needed: copying whole rows every quantum is costly.
            let Some(current) = self.rows[slot].as_ref().filter(|row| row.assigned) else {
                continue;
            };
            let (aborted, target) = (current.aborted, current.binding.target);
            let active = current.active.as_ref().copied();
            let mut decision = self.views[slot].decision;
            if aborted {
                if let Some(take) = active {
                    if let Some(token) = take.token {
                        io.capture().abort(token)?;
                    }
                    Self::release_edit(take, io)?;
                    self.ledger.retire(io, take.media)?;
                    self.rows[slot].as_mut().expect("row").active = None;
                }
                self.rows[slot].as_mut().expect("row").aborted = false;
            } else if let Some(take) = active {
                if !decision.state.writes() || decision.state.capture_revision != take.revision {
                    let media = self.freeze(slot, frame, io)?;
                    if decision.state.content_revision == take.revision {
                        self.ledger.set_playback(io, target, Some(media))?;
                        let row = self.rows[slot].as_mut().expect("completed capture");
                        row.windows = take.windows;
                        row.length_override = None;
                        if take.edit.is_some() {
                            self.refresh_wet_reservation(slot, decision, frame, io)?;
                        }
                    }
                }
            }
            if let Some(origin) = decision.origin {
                if let Some(pin) = self.input_batch_pin(origin) {
                    let media = self.ledger.pinned(pin)?;
                    self.ledger.set_playback(io, target, Some(media))?;
                    let windows = self.ledger.view(media)?.windows.unwrap_or_default();
                    let row = self.rows[slot].as_mut().expect("validated batch row");
                    row.windows = windows;
                    row.length_override = None;
                }
                for pending in self.pending.iter().flatten() {
                    if let PendingKind::Transition {
                        request,
                        pin: Some(pin),
                        clear,
                    } = pending.kind
                    {
                        if request == origin {
                            let media = self.ledger.pinned(pin)?;
                            if self.ledger.view(media)?.description.owner == target {
                                self.ledger.set_playback(io, target, Some(media))?;
                                let windows = self.ledger.view(media)?.windows.unwrap_or_default();
                                let row = self.rows[slot].as_mut().expect("validated row");
                                row.windows = windows;
                                row.length_override = None;
                                if clear {
                                    self.ledger.set_latest(io, row.binding.target, None)?;
                                    let row = self.rows[slot].as_mut().expect("validated row");
                                    row.windows = Windows::default();
                                    row.length_override = None;
                                }
                            }
                        }
                    }
                }
            }
            if let Some(take) = self.rows[slot]
                .as_ref()
                .and_then(|r| r.prepared.as_ref())
                .copied()
            {
                if let Some(id) = take.writer {
                    let job = self
                        .shared
                        .as_ref()
                        .ok_or(Error::InvalidState)?
                        .live_job(id)?;
                    let position = if job.kind == indexed_midi::WriteKind::Fixed
                        || job.following_recording.is_some()
                    {
                        decision.state.position
                    } else {
                        decision.state.capture_frames
                    };
                    self.shared.as_mut().unwrap().start_live(id, position, io)?;
                }
                if let Some(token) = take.token {
                    io.capture().activate(token, frame)?;
                }
                let row = self.rows[slot].as_mut().expect("row");
                if decision.state.mode == Mode::Recording && !decision.state.monitor_previous {
                    row.windows = take.windows;
                }
                row.active = Some(take);
                row.prepared = None;
                row.next_bank = 1 - take.bank;
                row.failure = None;
            }
            if let Some(shared) = &self.shared {
                if let Some(id) = shared.live_for(decision.state.id) {
                    let job = shared.live_job(id)?;
                    if job.following_recording.is_some_and(|revision| {
                        decision.state.mode != Mode::Recording
                            || revision != decision.state.capture_revision
                    }) {
                        if !(job.autonomous
                            && self.replacing_targets[slot]
                            && self
                                .seal_recording_reservation(slot, id, decision, frame, io)
                                .is_ok())
                        {
                            self.shared.as_mut().unwrap().cancel_live(id, io)?;
                        }
                    }
                }
            }
            let row = self.rows[slot].as_ref().expect("row");
            if let Some(writer) = row
                .active
                .as_ref()
                .and_then(|take| take.writer)
                .filter(|_| decision.state.mode == Mode::Replacing)
            {
                self.shared
                    .as_mut()
                    .ok_or(Error::InvalidState)?
                    .seek_live(writer, decision.state.position)?;
            }
            let media = self
                .ledger
                .playback(row.binding.target)?
                .map(|id| self.ledger.view_ref(id))
                .transpose()?;
            if media.is_none_or(|m| {
                row.length_override
                    .filter(|(revision, _)| *revision == decision.state.content_revision)
                    .map_or(m.description.frames, |(_, length)| length)
                    != decision.state.content_length
                    || m.description.revision != decision.state.content_revision
            }) {
                return Err(Error::InvalidState);
            }
            let next = self
                .resolver
                .prerouting_transition(row.binding.target)
                .map_err(runtime_error)?;
            let preplay = self.resolver.preplay_eta(row.binding.target);
            let eta = preplay.map(|(eta, _)| eta);
            let dry_playing = (decision.state.plays() && decision.state.mode.dry_playback())
                || matches!(preplay, Some((_, Transition::PlayDry { .. })));
            let playing = decision.state.plays() || eta.is_some();
            let previous = &self.views[slot];
            if previous.lead
                && eta.is_none()
                && decision.state.plays()
                && previous.raw_position + self.resolver.quantum().frames() as i64
                    == decision.state.position as i64
                && previous.windows == row.windows
                && previous.audio == media.and_then(|m| m.audio).map(|s| s.index)
                && previous.midi == media.and_then(|m| m.midi).map(|s| s.index)
            {
                decision.playback_discontinuity = false;
            }
            self.views[slot] = View {
                audio_widths: row.audio_widths,
                windows: row.windows,
                raw_position: eta.map_or(decision.state.position as i64, |eta| {
                    -(eta.min(i64::MAX as u64) as i64)
                }),
                lead: eta.is_some(),
                decision,
                dry_wet: row.dry_wet,
                pre_recording: matches!(
                    next,
                    Some(Transition::Record { .. } | Transition::ReplaceInput { .. })
                ),
                pre_rerecording: matches!(next, Some(Transition::Rerecord)),
                replace: row.active.as_ref().and_then(|take| take.edit),
                auxiliary: media
                    .filter(|_| dry_playing)
                    .and_then(|m| m.auxiliary)
                    .map(|s| s.index),
                audio: media
                    .filter(|_| playing && (!row.dry_wet || !dry_playing))
                    .and_then(|m| m.audio)
                    .map(|s| s.index),
                midi: media
                    .filter(|_| playing && (!row.dry_wet || dry_playing))
                    .and_then(|m| m.midi)
                    .map(|s| s.index),
                capture: row.interval,
                gain: row.binding.gain,
                balance: row.balance,
                smoothing: row.binding.smoothing,
                playback_node: row.binding.playback,
                capture_node: row.binding.capture,
                reset_input: row.reset_input,
            };
        }
        Ok(())
    }
    fn export_recording(
        &mut self,
        target: LoopId,
        frame: Frame,
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<media::ExportId, Error> {
        let row = self.row(target)?;
        let take = row.active.ok_or(Error::InvalidState)?;
        let state = self.resolver.state(target).map_err(runtime_error)?;
        if self.ledger.usage().free_exports == 0 {
            return Err(Error::Capacity);
        }
        let id = self.ledger.reserve(media::Description {
            auxiliary_channels: if row.dry_wet {
                row.binding.auxiliary_channels as u32
            } else {
                0
            },
            owner: target,
            revision: take.revision,
            frames: 0,
            channels: row.binding.channels as u32,
            midi: row.binding.midi,
        })?;
        if let Some((resource, _)) = take.edit {
            let mut sources = [None; 3];
            let result = (|| {
                let wet = io.snapshot(resource, frame)?;
                sources[0] = Some(wet);
                let mut retain = |source: Option<media::Source>,
                                  index: usize|
                 -> Result<Option<media::Source>, Error> {
                    let Some(source) = source else {
                        return Ok(None);
                    };
                    let snapshot = io.retain_snapshot(source.snapshot.id)?;
                    sources[index] = Some(snapshot);
                    Ok(Some(media::Source {
                        snapshot,
                        index: io.read_index(snapshot.id)?,
                    }))
                };
                let auxiliary = retain(take.auxiliary, 1)?;
                let midi = retain(take.midi, 2)?;
                let wet_index = io.read_index(wet.id)?;
                self.ledger
                    .publish(
                        io,
                        id,
                        media::Input {
                            description: media::Description {
                                owner: target,
                                revision: take.revision,
                                frames: state.content_length,
                                channels: row.binding.channels as u32,
                                auxiliary_channels: row.binding.auxiliary_channels as u32,
                                midi: row.binding.midi,
                            },
                            audio: Some(media::Source {
                                snapshot: wet,
                                index: wet_index,
                            }),
                            auxiliary,
                            midi,
                        },
                    )
                    .map_err(|error| error.error)?;
                sources = [None; 3];
                self.ledger.export(id, frame)
            })();
            for snapshot in sources.into_iter().flatten() {
                io.release_snapshot(snapshot.id)?;
            }
            self.ledger.retire(io, id)?;
            return result;
        }
        let mut sources = [None; 3];
        let result = (|| {
            if let Some(writer) = take.writer {
                sources = self
                    .shared
                    .as_mut()
                    .ok_or(Error::InvalidState)?
                    .snapshot_live(writer, frame, io)?;
            } else {
                for (member, resource) in row.resources[take.bank][usize::from(row.dry_wet)]
                    .iter()
                    .enumerate()
                {
                    if member == 1 && !row.dry_wet {
                        continue;
                    }
                    if let Some(resource) = resource {
                        let snapshot = io.snapshot(*resource, frame)?;
                        sources[member] = Some(snapshot);
                    }
                }
            }
            let audio = sources[0]
                .map(|snapshot| {
                    io.read_index(snapshot.id)
                        .map(|index| media::Source { snapshot, index })
                })
                .transpose()?;
            let auxiliary = sources[1]
                .map(|snapshot| {
                    io.read_index(snapshot.id)
                        .map(|index| media::Source { snapshot, index })
                })
                .transpose()?;
            let midi = sources[2]
                .map(|snapshot| {
                    io.read_index(snapshot.id)
                        .map(|index| media::Source { snapshot, index })
                })
                .transpose()?;
            self.ledger
                .publish_windowed(
                    io,
                    id,
                    media::Input {
                        auxiliary,
                        description: media::Description {
                            auxiliary_channels: if row.dry_wet {
                                row.binding.auxiliary_channels as u32
                            } else {
                                0
                            },
                            owner: target,
                            revision: take.revision,
                            frames: state.capture_frames,
                            channels: row.binding.channels as u32,
                            midi: row.binding.midi,
                        },
                        audio,
                        midi,
                    },
                    take.windows,
                )
                .map_err(|e| e.error)?;
            sources = [None; 3];
            self.ledger.export(id, frame)
        })();
        for snapshot in sources.into_iter().flatten() {
            io.release_snapshot(snapshot.id)?;
        }
        self.ledger.retire(io, id)?;
        result
    }
    fn query(
        &mut self,
        operation: Operation<S>,
        frame: Frame,
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<Reply<S>, Failure> {
        match operation {
            Operation::State { target } => Ok(Reply::State(self.cut(target, frame)?)),
            Operation::Export { target, selection } => {
                let cut = self.cut(target, frame)?;
                let id = match selection {
                    Selection::Recording => self.export_recording(target, frame, io)?,
                    Selection::Playback | Selection::Latest => {
                        let id = if selection == Selection::Playback {
                            cut.playback
                        } else {
                            cut.latest
                        }
                        .ok_or(Error::InvalidState)?;
                        self.ledger.export(id, frame)?
                    }
                };
                Ok(Reply::Export {
                    id,
                    cut,
                    media: self.ledger.exported(id)?,
                })
            }
            _ => Err(Error::InvalidEvent.into()),
        }
    }
    fn complete(
        &mut self,
        frame: Frame,
        io: &mut PlannerResourceIo<'_, S>,
        receipts: &mut schedule::ReceiptWriter<'_, Operation<S>, Reply<S>>,
    ) -> Result<(), Error> {
        for index in 0..self.pending.len() {
            let Some(pending) = self.pending[index] else {
                continue;
            };
            if pending.forwarded {
                continue;
            }
            let reply = match pending.kind {
                PendingKind::InputBatch => self.input_batch_reply()?,
                PendingKind::Program {
                    id, request, graph, ..
                } => self
                    .resolver
                    .completion(request)
                    .map_err(runtime_error)?
                    .map(|completion| match graph {
                        Some(graph) => Reply::ProgramGraphChanged {
                            id,
                            graph,
                            graph_epoch: self.graph_epoch,
                            topology_epoch: self.resolver.epoch(),
                            completion,
                        },
                        None => Reply::ProgramChanged {
                            id,
                            graph_epoch: self.graph_epoch,
                            topology_epoch: self.resolver.epoch(),
                            completion,
                        },
                    }),
                PendingKind::Transition { request, .. } => self
                    .resolver
                    .completion(request)
                    .map_err(runtime_error)?
                    .map(Reply::Transition),
                PendingKind::Query(operation) => Some(
                    self.query(operation, frame, io)
                        .unwrap_or_else(Reply::Failed),
                ),
            };
            if let Some(reply) = reply {
                receipts.complete(pending.token, Ok(reply))?;
                self.pending[index].as_mut().expect("pending").forwarded = true;
            }
        }
        Ok(())
    }
    /// Permanently quiesce and explicitly release domain owners off-render, after
    /// callbacks stopped and the runtime handed back its planner and store.
    /// Repeatable after success or partial cleanup. Does not advance/finish
    /// unrendered time or resume execution. Imported resource identities remain
    /// the caller's responsibility.
    pub fn cleanup(&mut self, resources: &mut ResourceStore<S>) -> Result<(), Error> {
        self.resolver.quiesce();
        self.node_pairs.cleanup(&mut resources.io())?;
        self.topologies
            .cleanup(&mut self.resolver)
            .map_err(|error| match error {
                Failure::Runtime(error) => error,
                Failure::Timeline(error) => runtime_error(error),
            })?;
        for slot in 0..self.rows.len() {
            let Some(row) = self.rows[slot] else { continue };
            if let Some(interval) = row.interval {
                resources.cancel_capture_tick(interval)?;
                self.rows[slot].as_mut().expect("row").interval = None;
            }
            if let Some(take) = row.prepared {
                if let Some(token) = take.token {
                    resources.cancel_capture_take(token)?;
                }
                Self::release_edit(take, &mut resources.io())?;
                self.rows[slot].as_mut().expect("row").prepared = None;
            }
            if let Some(take) = row.active {
                if let Some(token) = take.token {
                    let info = resources.capture_group_info(
                        row.groups[take.bank][usize::from(row.dry_wet)]
                            .ok_or(Error::InvalidState)?,
                    )?;
                    let mut output = [None; 3];
                    let members = usize::from(row.binding.channels > 0)
                        + usize::from(row.dry_wet && row.binding.auxiliary_channels > 0)
                        + usize::from(row.binding.midi);
                    resources.freeze_capture_take(
                        token,
                        info.next_frame.ok_or(Error::InvalidState)?,
                        None,
                        &mut output[..members],
                    )?;
                    self.rows[slot].as_mut().expect("row").frozen = output;
                }
                Self::release_edit(take, &mut resources.io())?;
                self.rows[slot].as_mut().expect("row").active = None;
            }
            for member in 0..3 {
                if let Some(snapshot) = self.rows[slot].expect("row").frozen[member] {
                    resources.release_snapshot(snapshot.id)?;
                    self.rows[slot].as_mut().expect("row").frozen[member] = None;
                }
            }
        }
        self.release_input_batch(&mut resources.io())?;
        self.collect_automatic_finishes()?;
        self.ledger.release_all(&mut resources.io())?;
        for row in self.rows.iter_mut().flatten() {
            for bank in 0..2 {
                for group in &mut row.groups[bank] {
                    if let Some(id) = *group {
                        resources.release_capture_group(id)?;
                        *group = None;
                    }
                }
                for resource in row.resources[bank].iter_mut().flatten() {
                    if let Some(id) = *resource {
                        resources.remove(id)?;
                        *resource = None;
                    }
                }
            }
        }
        if let Some(store) = self.wet_edits {
            resources.cleanup_audio_store(store)?;
            self.wet_edits = None;
        }
        self.pending.fill(None);
        if let Some(shared) = &mut self.shared {
            shared.cleanup(resources)?;
        }
        Ok(())
    }
}
impl<S: Sample> Planner<S> for LoopPlanner<S> {
    fn validate_quantum(&self, quantum: rt_runtime::Quantum) -> Result<(), Error> {
        if quantum != self.resolver.quantum() {
            return Err(Error::InvalidConfig);
        }
        Ok(())
    }
    fn begin_tick(
        &mut self,
        mut context: PlanContext<'_, Operation<S>, Reply<S>, S>,
    ) -> Result<(), Error> {
        let _ = self.prepare_program_inputs(context.frame, &mut context.resources);
        if let Some(shared) = &mut self.shared {
            shared.progress(context.callback_start, &mut context.resources)?;
        }
        let q = self.resolver.quantum();
        if context.frame.checked_add(q.frames())? != context.ceiling {
            return Err(Error::InvalidFrame);
        }
        let end = context.ceiling;
        if self.settle_boundary(context)? != end {
            return Err(Error::InvalidFrame);
        }
        Ok(())
    }
    fn finish_tick(
        &mut self,
        mut context: FinishContext<'_, Operation<S>, Reply<S>, S>,
    ) -> Result<(), Error> {
        let end = context.interval.end;
        self.finish_boundary(&mut context)?;
        if let Some(shared) = &mut self.shared {
            shared.finish(end, self.graph_epoch, &mut context.resources)?;
        }
        Ok(())
    }
    type Operation = Operation<S>;
    type Result = Reply<S>;
    fn id(&self) -> Option<PlannerId> {
        Some(self.settings.planner)
    }
    fn operation_bytes(&self) -> usize {
        self.topologies
            .operation_bytes()
            .max(self.rows.len() * CONTENT_FAMILY_RECORD_BYTES)
    }
    fn admit_graph(&self, candidate: &PreparedPlan) -> Result<(), Error> {
        self.validate_graph(candidate)
    }
    fn admit_processor_change(&self, node: NodeId) -> Result<(), Error> {
        if self.rows.iter().flatten().any(|row| {
            row.binding.playback.slot() == node.slot() || row.binding.capture.slot() == node.slot()
        }) {
            Err(Error::Busy)
        } else {
            Ok(())
        }
    }
    fn recovery_safe(operation: Operation<S>) -> bool {
        matches!(
            operation,
            Operation::Cancel { .. }
                | Operation::TopologyCancel { .. }
                | Operation::NodePairCancel { .. }
                | Operation::NodePairSourceRelease { .. }
                | Operation::Transition {
                    timing: Timing::At,
                    transition: Transition::Stop,
                    ..
                }
        )
    }
    fn operation(
        &mut self,
        operation: Operation<S>,
        context: OperationContext<'_, Operation<S>, Reply<S>, S>,
    ) -> Result<Disposition<Reply<S>>, Error> {
        self.execute_operation(operation, context)
    }
}

impl<S: Sample> LoopPlanner<S> {
    fn midi_initial(
        &self,
        resources: &mut ResourceIo<'_, S>,
        export: media::ExportId,
        channel: u8,
    ) -> Result<Reply<S>, Error> {
        if channel >= 16 {
            return Err(Error::InvalidShape);
        }
        let source = self
            .ledger
            .exported(export)?
            .view
            .midi
            .ok_or(Error::InvalidShape)?;
        let (state, system_reset_seen) = resources
            .render()
            .midi_initial_channel(source.index, channel)?;
        Ok(Reply::MidiInitial {
            channel,
            state,
            system_reset_seen,
        })
    }

    fn execute_operation(
        &mut self,
        operation: Operation<S>,
        mut context: OperationContext<'_, Operation<S>, Reply<S>, S>,
    ) -> Result<Disposition<Reply<S>>, Error> {
        self.graph_epoch = context.graph_epoch;
        self.topologies.callback(context.callback_start);
        self.node_pairs.callback(context.callback_start);
        if let Operation::InputBatch {
            count,
            timing,
            graph,
            topology,
        } = operation
        {
            let result = (|| -> Result<Disposition<Reply<S>>, Failure> {
                if graph != context.graph_epoch || topology != self.resolver.epoch() {
                    return Err(Error::StaleEpoch.into());
                }
                let slot = self.pending_slot(context.recovery)?;
                self.begin_input_batch(
                    count,
                    timing,
                    context.payload,
                    context.requested_frame,
                    context.token.sequence(),
                    context.frame,
                    &mut context.resources,
                )?;
                self.pending[slot] = Some(Pending {
                    token: context.token,
                    kind: PendingKind::InputBatch,
                    forwarded: false,
                });
                Ok(Disposition::Pending)
            })();
            return Ok(result.unwrap_or_else(|error| Disposition::Complete(Reply::Failed(error))));
        }
        if matches!(
            operation,
            Operation::HistoryStatus { .. }
                | Operation::HistoryGrab { .. }
                | Operation::HistoryWindow { .. }
                | Operation::WriterBegin { .. }
                | Operation::WriterPrepareRecording { .. }
                | Operation::WriterPrepareReplacing { .. }
                | Operation::WriterStatus { .. }
                | Operation::WriterCancel { .. }
                | Operation::CandidateBegin { .. }
                | Operation::CandidateAudio { .. }
                | Operation::CandidateMidi { .. }
                | Operation::CandidateStatus { .. }
                | Operation::CandidateCancel { .. }
                | Operation::CandidatePublish { .. }
                | Operation::SharedWork
                | Operation::SharedCapacity
        ) {
            if !context.payload.is_empty() {
                return Ok(Disposition::Complete(Reply::Failed(
                    Error::InvalidShape.into(),
                )));
            }
            let result = self
                .shared_operation(
                    operation,
                    context.frame,
                    context.graph_epoch,
                    &mut context.resources,
                )
                .map(Disposition::Complete);
            return Ok(result.unwrap_or_else(|error| Disposition::Complete(Reply::Failed(error))));
        }
        let result = (|| -> Result<Disposition<Reply<S>>, Failure> {
            if !context.payload.is_empty()
                && !matches!(
                    operation,
                    Operation::InputBatch { .. }
                        | Operation::TopologyBegin { .. }
                        | Operation::TopologyAppend { .. }
                        | Operation::SelectFamily { .. }
                )
            {
                return Err(Error::InvalidShape.into());
            }
            let topology_status = match operation {
                Operation::SelectFamily {
                    root,
                    topology_epoch,
                } => {
                    self.select_family(
                        root,
                        topology_epoch,
                        context.payload,
                        &mut context.resources,
                    )?;
                    return Ok(Disposition::Complete(Reply::Done));
                }
                Operation::NodePairSourceWindows {
                    id,
                    frames,
                    windows,
                } => {
                    if frames % self.resolver.quantum().frames() as u64 != 0 {
                        return Err(Error::InvalidFrame.into());
                    }
                    let source = self.node_pairs.source(id)?;
                    if source.snapshot.description.format == Format::Midi {
                        validate_midi_window(
                            &mut context.resources,
                            source.index,
                            windows.midi,
                            source.snapshot.description.frames,
                        )?;
                    }
                    self.node_pairs.set_windows(id, frames, windows)?;
                    return Ok(Disposition::Complete(Reply::Done));
                }
                Operation::NodePairSourceCursor { id } => {
                    let source = self.node_pairs.source(id)?;
                    return Ok(Disposition::Complete(Reply::Cursor(
                        context.resources.cursor(source.snapshot.id)?,
                    )));
                }
                Operation::NodePairSnapshot { resource } => {
                    let (id, snapshot) = self.node_pairs.snapshot(
                        resource,
                        context.frame,
                        &mut context.resources,
                    )?;
                    return Ok(Disposition::Complete(Reply::NodePairSource {
                        id,
                        snapshot,
                        status: self.node_pairs.source_status(id)?,
                    }));
                }
                Operation::NodePairSourceStatus { id } => {
                    return Ok(Disposition::Complete(Reply::NodePairSourceStatus {
                        id,
                        status: self.node_pairs.source_status(id)?,
                    }))
                }
                Operation::NodePairSourceValidate { id } => {
                    return Ok(Disposition::Complete(Reply::NodePairSourceStatus {
                        id,
                        status: self
                            .node_pairs
                            .validate_source(id, &mut context.resources)?,
                    }))
                }
                Operation::NodePairSourceRelease { id } => {
                    self.node_pairs.release_source(id, &mut context.resources)?;
                    return Ok(Disposition::Complete(Reply::NodePairSourceReleased { id }));
                }
                Operation::NodePairBegin { slot, changes } => {
                    let id = self.node_pairs.begin(slot, changes)?;
                    return Ok(Disposition::Complete(Reply::NodePairStatus {
                        id,
                        status: self.node_pairs.status(id)?,
                    }));
                }
                Operation::NodePairAppend {
                    id,
                    previous,
                    initial,
                } => {
                    self.append_node_pair(id, previous, initial, &mut context.resources)?;
                    return Ok(Disposition::Complete(Reply::NodePairStatus {
                        id,
                        status: self.node_pairs.status(id)?,
                    }));
                }
                Operation::NodePairStatus { id } => {
                    return Ok(Disposition::Complete(Reply::NodePairStatus {
                        id,
                        status: self.node_pairs.status(id)?,
                    }))
                }
                Operation::NodePairCancel { id } => {
                    self.node_pairs.cancel(id, &mut context.resources)?;
                    return Ok(Disposition::Complete(Reply::NodePairStatus {
                        id,
                        status: self.node_pairs.status(id)?,
                    }));
                }
                Operation::NodePairRecycle { id } => {
                    self.node_pairs.recycle(id)?;
                    return Ok(Disposition::Complete(Reply::NodePairStatus {
                        id,
                        status: self.node_pairs.status(id)?,
                    }));
                }
                Operation::ConfigurationActivate {
                    node_pairs,
                    topology,
                    graph,
                    graph_epoch,
                    topology_epoch,
                }
                | Operation::ConfigurationReplace {
                    node_pairs,
                    topology,
                    graph,
                    graph_epoch,
                    topology_epoch,
                }
                | Operation::ConvertEmpty {
                    node_pairs,
                    topology,
                    graph,
                    graph_epoch,
                    topology_epoch,
                    ..
                } => {
                    if let Operation::ConvertEmpty { previous, .. } = operation {
                        let state = self.resolver.state(previous)?;
                        if state.composite.is_some()
                            || state.mode != Mode::Stopped
                            || state.content_length != 0
                        {
                            return Err(crate::Error::InvalidState.into());
                        }
                    }
                    let (graph_epoch, topology_epoch) = self.activate_configuration(
                        node_pairs,
                        topology,
                        graph,
                        graph_epoch,
                        topology_epoch,
                        matches!(operation, Operation::ConfigurationReplace { .. }),
                        &mut context.graphs,
                        &mut context.resources,
                    )?;
                    return Ok(Disposition::Complete(Reply::ConfigurationActivated {
                        node_pairs,
                        topology,
                        graph,
                        graph_epoch,
                        topology_epoch,
                    }));
                }
                Operation::GraphStatus { id } => {
                    return Ok(Disposition::Complete(Reply::GraphStatus {
                        id,
                        status: context.graphs.status(id)?,
                    }));
                }
                Operation::GraphActivate {
                    id,
                    graph_epoch,
                    topology_epoch,
                } => {
                    let (graph_epoch, topology_epoch) = self.activate_graph(
                        id,
                        None,
                        graph_epoch,
                        topology_epoch,
                        &mut context.graphs,
                    )?;
                    return Ok(Disposition::Complete(Reply::GraphActivated {
                        id,
                        graph_epoch,
                        topology_epoch,
                    }));
                }
                Operation::TopologyGraphActivate {
                    id,
                    graph,
                    graph_epoch,
                    topology_epoch,
                } => {
                    let (graph_epoch, topology_epoch) = self.activate_graph(
                        graph,
                        Some(id),
                        graph_epoch,
                        topology_epoch,
                        &mut context.graphs,
                    )?;
                    return Ok(Disposition::Complete(Reply::TopologyGraphActivated {
                        id,
                        graph,
                        graph_epoch,
                        topology_epoch,
                    }));
                }
                Operation::TopologyBegin { slot } => {
                    Some(self.topologies.begin(slot, context.payload)?)
                }
                Operation::TopologyAppend { id, first_record } => {
                    self.topologies.append(id, first_record, context.payload)?;
                    Some(id)
                }
                Operation::TopologyStatus { id } => Some(id),
                Operation::TopologyValidate { id } => {
                    let mut progress = self.topologies.validate(id)?;
                    if progress.ready {
                        progress.ready = self
                            .prepare_program_inputs(context.frame, &mut context.resources)?
                            && self.prepare_candidate_inputs(
                                id,
                                context.frame,
                                &mut context.resources,
                            )?;
                    }
                    return Ok(Disposition::Complete(Reply::TopologyProgress {
                        id,
                        progress,
                    }));
                }
                Operation::TopologyCancel { id } => {
                    self.topologies.cancel(id, &mut self.resolver)?;
                    Some(id)
                }
                Operation::TopologyRecycle { id } => {
                    self.topologies.recycle(id)?;
                    Some(id)
                }
                Operation::TopologyActivate {
                    id,
                    graph_epoch,
                    topology_epoch,
                } => {
                    if graph_epoch != context.graph_epoch {
                        return Err(Error::InvalidState.into());
                    }
                    let topology_epoch =
                        self.topologies
                            .activate(id, topology_epoch, &mut self.resolver)?;
                    return Ok(Disposition::Complete(Reply::TopologyActivated {
                        id,
                        graph_epoch,
                        topology_epoch,
                    }));
                }
                _ => None,
            };
            if let Some(id) = topology_status {
                let mut status = self.topologies.status(id)?;
                if status == crate::topology::staging::Status::Ready
                    && !(self.prepare_program_inputs(context.frame, &mut context.resources)?
                        && self.prepare_candidate_inputs(
                            id,
                            context.frame,
                            &mut context.resources,
                        )?)
                {
                    status = crate::topology::staging::Status::Validating;
                }
                return Ok(Disposition::Complete(Reply::TopologyStatus { id, status }));
            }
            match operation {
                Operation::ProgramActivate {
                    id,
                    target,
                    graph_epoch,
                    topology_epoch,
                }
                | Operation::ProgramGraphActivate {
                    id,
                    target,
                    graph_epoch,
                    topology_epoch,
                    ..
                } => {
                    if graph_epoch != context.graph_epoch {
                        return Err(Error::InvalidState.into());
                    }
                    let graph = if let Operation::ProgramGraphActivate { graph, .. } = operation {
                        let permit = context.graphs.prepare_activation(graph, graph_epoch)?;
                        self.validate_graph(permit.candidate())?;
                        Some(graph)
                    } else {
                        None
                    };
                    let slot = self.pending_slot(context.recovery)?;
                    let request = self.topologies.stage_program(
                        id,
                        target,
                        topology_epoch,
                        context.token.sequence(),
                        context.requested_frame,
                        &mut self.resolver,
                    )?;
                    if graph.is_some() {
                        self.resolver.defer_program_until_restart(request);
                    }
                    self.pending[slot] = Some(Pending {
                        token: context.token,
                        kind: PendingKind::Program {
                            id,
                            request,
                            graph_epoch,
                            graph,
                        },
                        forwarded: false,
                    });
                    Ok(Disposition::Pending)
                }
                Operation::Cancel { sequence } => {
                    if let Some(pending) = self
                        .pending
                        .iter()
                        .flatten()
                        .find(|p| p.token.sequence() == sequence)
                        .copied()
                    {
                        if let PendingKind::Transition { request, .. }
                        | PendingKind::Program { request, .. } = pending.kind
                        {
                            self.resolver.cancel(request)?;
                        } else if let PendingKind::InputBatch = pending.kind {
                            let request = self
                                .input_batch_first_request()
                                .ok_or(Error::InvalidState)?;
                            self.resolver.cancel(request)?;
                        } else {
                            return Err(Error::InvalidState.into());
                        }
                        Ok(Disposition::Complete(Reply::Cancelled {
                            sequence,
                            delivered: true,
                        }))
                    } else {
                        context.receipts.cancel_queued(
                            sequence,
                            Reply::Cancelled {
                                sequence,
                                delivered: false,
                            },
                        )?;
                        Ok(Disposition::Complete(Reply::Cancelled {
                            sequence,
                            delivered: false,
                        }))
                    }
                }
                Operation::EditWindows {
                    target,
                    expected_revision,
                    expected_content,
                    edit,
                } => {
                    let row = self.row(target)?;
                    if row.timing_revision != expected_revision {
                        return Err(Error::StaleRevision.into());
                    }
                    let windows = row.windows.edited(
                        edit,
                        row.audio_widths.iter().sum(),
                        row.binding.midi,
                    )?;
                    let state = self.resolver.state(target)?;
                    if state.content_revision != expected_content {
                        return Err(Error::StaleRevision.into());
                    }
                    let media = self
                        .ledger
                        .playback(target)?
                        .map(|id| self.ledger.view(id))
                        .transpose()?;
                    let audio_frames = media
                        .and_then(|m| m.audio)
                        .map_or(0, |s| s.snapshot.description.frames);
                    let midi_frames = media
                        .and_then(|m| m.midi)
                        .map_or(0, |s| s.snapshot.description.frames);
                    windows.validate_alignment(
                        row.audio_widths.iter().sum(),
                        row.binding.midi,
                        audio_frames,
                        midi_frames,
                        edit.length.unwrap_or(state.content_length),
                    )?;
                    if row
                        .active
                        .into_iter()
                        .chain(row.prepared)
                        .any(|take| take.edit.is_some())
                    {
                        for window in
                            &windows.audio[row.audio_widths[1]..row.audio_widths.iter().sum()]
                        {
                            if i128::from(window.start_offset)
                                + i128::from(edit.length.unwrap_or(state.content_length))
                                > i128::from(window.raw_frames.unwrap_or(audio_frames))
                            {
                                return Err(Error::InvalidShape.into());
                            }
                        }
                    }
                    if let Some(source) = media.and_then(|view| view.midi) {
                        validate_midi_window(
                            &mut context.resources,
                            source.index,
                            windows.midi,
                            midi_frames,
                        )?;
                    }
                    let revision = row.timing_revision.checked_add(1).ok_or(Error::Exhausted)?;
                    let active_windows = row
                        .active
                        .map(|take| {
                            take.windows.edited(
                                edit,
                                row.audio_widths.iter().sum(),
                                row.binding.midi,
                            )
                        })
                        .transpose()?;
                    let prepared_windows = row
                        .prepared
                        .map(|take| {
                            take.windows.edited(
                                edit,
                                row.audio_widths.iter().sum(),
                                row.binding.midi,
                            )
                        })
                        .transpose()?;
                    if let Some(length) = edit.length {
                        self.resolver.edit_length(target, length)?;
                    }
                    let row = self.rows[target.slot() as usize]
                        .as_mut()
                        .expect("validated row");
                    row.windows = windows;
                    if let (Some(take), Some(windows)) = (&mut row.active, active_windows) {
                        take.windows = windows;
                    }
                    if let (Some(take), Some(windows)) = (&mut row.prepared, prepared_windows) {
                        take.windows = windows;
                    }
                    row.timing_revision = revision;
                    self.resolver.publish_timing_revision(target, revision);
                    if let Some(length) = edit.length {
                        row.length_override = Some((state.content_revision, length));
                    }
                    Ok(Disposition::Complete(Reply::Timing(
                        self.cut(target, context.frame)?,
                    )))
                }
                Operation::Gain { target, value } => {
                    self.row(target)?;
                    if !value.is_finite() {
                        return Err(Error::InvalidParameter.into());
                    }
                    self.rows[target.slot() as usize]
                        .as_mut()
                        .expect("row")
                        .binding
                        .gain = value;
                    Ok(Disposition::Complete(Reply::Done))
                }
                Operation::RepeatSync { target, enabled } => {
                    self.resolver.set_repeat_sync(target, enabled)?;
                    Ok(Disposition::Complete(Reply::Done))
                }
                Operation::Balance { target, value } => {
                    self.row(target)?;
                    if !(-S::ONE..=S::ONE).contains(&value) {
                        return Err(Error::InvalidParameter.into());
                    }
                    self.rows[target.slot() as usize]
                        .as_mut()
                        .expect("row")
                        .balance = value;
                    Ok(Disposition::Complete(Reply::Done))
                }
                Operation::Smoothing { target, smoothing } => {
                    self.row(target)?;
                    self.rows[target.slot() as usize]
                        .as_mut()
                        .expect("row")
                        .binding
                        .smoothing = smoothing;
                    Ok(Disposition::Complete(Reply::Done))
                }
                Operation::Clear { target } => {
                    let row = self.row(target)?;
                    let slot = self.pending_slot(context.recovery)?;
                    let revision = self
                        .resolver
                        .state(target)?
                        .revision_watermark
                        .checked_next()
                        .map_err(|_| crate::Error::Exhausted)?;
                    let media = self
                        .ledger
                        .adopt(
                            &mut context.resources,
                            media::Input {
                                auxiliary: None,
                                description: media::Description {
                                    auxiliary_channels: if row.dry_wet {
                                        row.binding.auxiliary_channels as u32
                                    } else {
                                        0
                                    },
                                    owner: target,
                                    revision,
                                    frames: 0,
                                    channels: row.binding.channels as u32,
                                    midi: row.binding.midi,
                                },
                                audio: None,
                                midi: None,
                            },
                        )
                        .map_err(|e| e.error)?;
                    let pin = self.ledger.pin(media);
                    // The pending pin, then the playback reference, own the empty media.
                    self.ledger.retire(&mut context.resources, media)?;
                    let pin = pin?;
                    let request = match self.resolver.submit(crate::Request {
                        target,
                        sequence: context.token.sequence(),
                        at: context.requested_frame,
                        timing: Timing::At,
                        transition: Transition::Replace {
                            length: 0,
                            revision,
                            preserve_position: false,
                        },
                    }) {
                        Ok(request) => request,
                        Err(error) => {
                            self.ledger.release_pin(&mut context.resources, pin)?;
                            return Err(error.into());
                        }
                    };
                    self.pending[slot] = Some(Pending {
                        token: context.token,
                        kind: PendingKind::Transition {
                            request,
                            pin: Some(pin),
                            clear: true,
                        },
                        forwarded: false,
                    });
                    Ok(Disposition::Pending)
                }
                Operation::Release { export } => {
                    self.ledger.release_export(&mut context.resources, export)?;
                    Ok(Disposition::Complete(Reply::Done))
                }
                Operation::Retire { media } => {
                    self.ledger.view(media)?;
                    self.ledger.retire(&mut context.resources, media)?;
                    Ok(Disposition::Complete(Reply::Done))
                }
                Operation::Adopt {
                    target,
                    frames,
                    auxiliary,
                    audio,
                    midi,
                } => {
                    if frames % self.resolver.quantum().frames() as u64 != 0 {
                        return Err(Error::InvalidFrame.into());
                    }
                    let row = self.row(target)?;
                    let state = self.resolver.state(target)?;
                    if state.writes() {
                        return Err(Error::Busy.into());
                    }
                    let revision = state
                        .revision_watermark
                        .checked_next()
                        .map_err(|_| Error::Exhausted)?;
                    let description = media::Description {
                        auxiliary_channels: if row.dry_wet {
                            row.binding.auxiliary_channels as u32
                        } else {
                            0
                        },
                        owner: target,
                        revision,
                        frames,
                        channels: row.binding.channels as u32,
                        midi: row.binding.midi,
                    };
                    let media = self.node_pairs.adopt(
                        description,
                        audio,
                        auxiliary,
                        midi,
                        row.binding.packet_bytes,
                        &mut self.ledger,
                        &mut context.resources,
                    )?;
                    Ok(Disposition::Complete(Reply::Adopted { media, revision }))
                }
                Operation::MidiInitial { export, channel } => Ok(Disposition::Complete(
                    self.midi_initial(&mut context.resources, export, channel)?,
                )),
                Operation::AudioRoleCursors { export } => {
                    let view = self.ledger.exported(export)?.view;
                    let dry = view.auxiliary.ok_or(Error::InvalidShape)?;
                    let wet = view.audio.ok_or(Error::InvalidShape)?;
                    Ok(Disposition::Complete(Reply::AudioRoleCursors([
                        context.resources.cursor(dry.snapshot.id)?,
                        context.resources.cursor(wet.snapshot.id)?,
                    ])))
                }
                Operation::Cursor { export, midi } => {
                    let view = self.ledger.exported(export)?.view;
                    let source =
                        if midi { view.midi } else { view.audio }.ok_or(Error::InvalidShape)?;
                    Ok(Disposition::Complete(Reply::Cursor(
                        context.resources.cursor(source.snapshot.id)?,
                    )))
                }
                Operation::State { .. } | Operation::Export { .. } => {
                    let slot = self.pending_slot(context.recovery)?;
                    self.pending[slot] = Some(Pending {
                        token: context.token,
                        kind: PendingKind::Query(operation),
                        forwarded: false,
                    });
                    Ok(Disposition::Pending)
                }
                Operation::Transition { target, timing, .. }
                | Operation::Select { target, timing, .. } => {
                    let slot = self.pending_slot(context.recovery)?;
                    let (transition, pin) = match operation {
                        Operation::Select {
                            media,
                            preserve_position,
                            ..
                        } => {
                            let view = self.ledger.view(media)?;
                            Self::validate_media_shape(self.row(target)?, view)?;
                            if view.description.owner != target {
                                return Err(Error::InvalidId.into());
                            }
                            (
                                Transition::Replace {
                                    length: view.description.frames,
                                    revision: view.description.revision,
                                    preserve_position,
                                },
                                Some(self.ledger.pin(media)?),
                            )
                        }
                        Operation::Transition { transition, .. } => {
                            if matches!(
                                transition,
                                Transition::PlayDry { .. } | Transition::Rerecord
                            ) && !self.row(target)?.dry_wet
                            {
                                return Err(Error::InvalidState.into());
                            }
                            if matches!(
                                transition,
                                Transition::Program
                                    | Transition::Replace { .. }
                                    | Transition::Adopt { .. }
                            ) {
                                return Err(Error::InvalidEvent.into());
                            }
                            (transition, None)
                        }
                        _ => unreachable!(),
                    };
                    let request = match self.resolver.submit(crate::Request {
                        target,
                        sequence: context.token.sequence(),
                        at: context.requested_frame,
                        timing,
                        transition,
                    }) {
                        Ok(request) => request,
                        Err(error) => {
                            if let Some(pin) = pin {
                                self.ledger.release_pin(&mut context.resources, pin)?;
                            }
                            return Err(error.into());
                        }
                    };
                    self.pending[slot] = Some(Pending {
                        token: context.token,
                        kind: PendingKind::Transition {
                            request,
                            pin,
                            clear: false,
                        },
                        forwarded: false,
                    });
                    Ok(Disposition::Pending)
                }
                Operation::InputBatch { .. } => unreachable!("input operation handled"),
                Operation::NodePairSourceWindows { .. }
                | Operation::HistoryStatus { .. }
                | Operation::HistoryGrab { .. }
                | Operation::HistoryWindow { .. }
                | Operation::WriterBegin { .. }
                | Operation::WriterPrepareRecording { .. }
                | Operation::WriterPrepareReplacing { .. }
                | Operation::WriterStatus { .. }
                | Operation::WriterCancel { .. }
                | Operation::CandidateBegin { .. }
                | Operation::CandidateAudio { .. }
                | Operation::CandidateMidi { .. }
                | Operation::CandidateStatus { .. }
                | Operation::CandidateCancel { .. }
                | Operation::CandidatePublish { .. }
                | Operation::SharedWork
                | Operation::SharedCapacity
                | Operation::NodePairSourceCursor { .. }
                | Operation::NodePairSnapshot { .. }
                | Operation::NodePairSourceStatus { .. }
                | Operation::NodePairSourceValidate { .. }
                | Operation::NodePairSourceRelease { .. }
                | Operation::NodePairBegin { .. }
                | Operation::NodePairAppend { .. }
                | Operation::NodePairStatus { .. }
                | Operation::NodePairCancel { .. }
                | Operation::NodePairRecycle { .. }
                | Operation::ConfigurationActivate { .. }
                | Operation::ConvertEmpty { .. }
                | Operation::SelectFamily { .. }
                | Operation::ConfigurationReplace { .. }
                | Operation::GraphStatus { .. }
                | Operation::GraphActivate { .. }
                | Operation::TopologyGraphActivate { .. }
                | Operation::TopologyBegin { .. }
                | Operation::TopologyAppend { .. }
                | Operation::TopologyValidate { .. }
                | Operation::TopologyStatus { .. }
                | Operation::TopologyCancel { .. }
                | Operation::TopologyRecycle { .. }
                | Operation::TopologyActivate { .. } => unreachable!("topology operation handled"),
            }
        })();
        Ok(result.unwrap_or_else(|error| Disposition::Complete(Reply::Failed(error))))
    }
    fn settle_boundary(
        &mut self,
        mut context: PlanContext<'_, Operation<S>, Reply<S>, S>,
    ) -> Result<Frame, Error> {
        self.graph_epoch = context.graph_epoch;
        self.topologies.callback(context.callback_start);
        if context.frame != self.resolver.frame() {
            return Err(Error::InvalidFrame);
        }
        self.failures.clear();
        for row in self.rows.iter().flatten() {
            if row.aborted {
                self.failures.push(crate::Observation::CaptureAborted {
                    target: row.binding.target,
                    reason: timeline_error(row.failure.unwrap_or(Error::InvalidState)),
                });
            } else if row.active.as_ref().is_some_and(|take| take.edit.is_some())
                && row.failure.is_some()
            {
                self.failures.push(crate::Observation::CaptureExhausted {
                    target: row.binding.target,
                });
            }
        }
        for pending in self.pending.iter().flatten() {
            if let PendingKind::Program {
                request,
                graph_epoch,
                graph,
                ..
            } = pending.kind
            {
                if self
                    .resolver
                    .completion(request)
                    .map_err(runtime_error)?
                    .is_none()
                    && (graph_epoch != context.graph_epoch
                        || graph.is_some_and(|id| {
                            context.graphs.status(id)
                                != Ok(GraphStageStatus::Ready {
                                    validated_epoch: graph_epoch,
                                })
                        }))
                {
                    self.failures.push(crate::Observation::Refused {
                        request,
                        reason: crate::Error::InvalidState,
                    });
                }
            }
        }
        loop {
            self.cancel_ticks(&mut context.resources)?;
            let interval = self
                .resolver
                .preview_tick(&self.failures)
                .map_err(runtime_error)?;
            for decision in self.resolver.decisions().map_err(runtime_error)? {
                self.views[decision.state.id.slot() as usize].decision = *decision;
            }
            let selected = self
                .resolver
                .preview_program_change()
                .map_err(runtime_error)?;
            let graph = selected.and_then(|selected| {
                self.pending
                    .iter()
                    .flatten()
                    .find_map(|pending| match pending.kind {
                        PendingKind::Program {
                            request,
                            graph: Some(graph),
                            graph_epoch,
                            ..
                        } if selected == request => Some((request, graph, graph_epoch)),
                        _ => None,
                    })
            });
            let permit = if let Some((request, graph, epoch)) = graph {
                match context.graphs.prepare_activation(graph, epoch) {
                    Ok(permit) => Some(permit),
                    Err(error) => {
                        if self.failures.len() == self.failures.capacity() {
                            return Err(Error::InvalidState);
                        }
                        self.failures.push(crate::Observation::Refused {
                            request,
                            reason: timeline_error(error),
                        });
                        continue;
                    }
                }
            } else {
                None
            };
            if !self.preflight(context.frame, interval.end, &mut context.resources)? {
                continue;
            }
            self.settle(context.frame, &mut context.resources)?;
            self.resolver.commit().map_err(runtime_error)?;
            let _ = self.prepare_program_inputs(context.frame, &mut context.resources);
            if let Some(permit) = permit {
                self.graph_epoch = permit.commit();
            }
            self.complete(context.frame, &mut context.resources, &mut context.receipts)?;
            self.observer.topology_epoch(self.resolver.epoch());
            self.observer.settled(&self.resolver);
            self.observer.boundary(
                context.frame,
                self.resolver.decisions().map_err(runtime_error)?,
                self.resolver.events(),
                self.resolver.dropped_events(),
            );
            return Ok(interval.end);
        }
    }
    fn finish_boundary(
        &mut self,
        context: &mut FinishContext<'_, Operation<S>, Reply<S>, S>,
    ) -> Result<(), Error> {
        for row in self.rows.iter_mut().flatten().filter(|row| row.assigned) {
            if let Some(interval) = row.interval {
                if row.aborted {
                    context.resources.capture().cancel_tick(interval)?;
                } else {
                    context.resources.capture().commit(interval)?;
                }
                row.interval = None;
            }
            row.reset_input = false;
        }
        self.resolver.finish().map_err(runtime_error)?;
        self.collect_automatic_finishes()?;
        for row in self.rows.iter().flatten() {
            if let Some(take) = row.active.as_ref().filter(|take| take.writer.is_some()) {
                let job = self
                    .shared
                    .as_ref()
                    .ok_or(Error::InvalidState)?
                    .live_job(take.writer.unwrap())?;
                if matches!(job.kind, indexed_midi::WriteKind::Growing { .. }) {
                    self.resolver
                        .confirm_capture_frames(row.binding.target, take.revision, job.frames)
                        .map_err(runtime_error)?;
                }
            }
        }
        for index in 0..self.pending.len() {
            if let Some(pending) = self.pending[index].filter(|p| p.forwarded) {
                if let PendingKind::InputBatch = pending.kind {
                    self.collect_input_batch(&mut context.resources)?;
                }
                if let PendingKind::Transition { request, pin, .. } = pending.kind {
                    self.resolver
                        .take_completion(request)
                        .map_err(runtime_error)?
                        .ok_or(Error::InvalidState)?;
                    if let Some(pin) = pin {
                        self.ledger.release_pin(&mut context.resources, pin)?;
                    }
                }
                if let PendingKind::Program { id, request, .. } = pending.kind {
                    self.topologies
                        .collect_program(id, request, &mut self.resolver)
                        .map_err(|error| match error {
                            Failure::Runtime(error) => error,
                            Failure::Timeline(error) => runtime_error(error),
                        })?;
                }
                self.pending[index] = None;
            }
        }
        Ok(())
    }
}

fn validate_midi_window<S: Sample>(
    resources: &mut rt_runtime::resource::ResourceIo<'_, S>,
    index: ReadIndex,
    window: crate::windows::ChannelWindow,
    frames: u64,
) -> Result<(), Error> {
    for position in [window.origin(), window.origin() - i64::from(window.preplay)] {
        let position = position.max(0) as u64;
        if position < frames {
            if resources
                .render()
                .midi_parameter_history_untracked(index, position)?
            {
                return Err(Error::InvalidParameter);
            }
        }
    }
    Ok(())
}
