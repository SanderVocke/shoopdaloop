use super::*;
use rt_runtime::buffer::AudioPorts;

pub(super) fn mark_replacing_targets(
    topology: &PreparedTopology,
    rows: &[Option<Row>],
    targets: &mut [bool],
) {
    for node in topology.nodes() {
        if let Some(program) = topology.program(node.id) {
            for target in program.targets() {
                if rows[target.target.slot() as usize]
                    .is_some_and(|row| row.assigned && row.binding.target == target.target)
                    && target
                        .segments()
                        .iter()
                        .any(|segment| segment.mode == Mode::Replacing)
                {
                    targets[target.target.slot() as usize] = true;
                }
            }
        }
    }
}

impl LoopPlanner {
    pub(super) fn prepare_candidate_inputs(
        &mut self,
        id: StageId,
        frame: Frame,
        io: &mut ResourceIo<'_>,
    ) -> Result<bool, Failure> {
        let mut ready = true;
        for slot in 0..self.rows.len() {
            let candidate = self.topologies.candidate(id)?;
            let target = candidate
                .nodes()
                .iter()
                .find(|node| node.id.slot() as usize == slot)
                .map(|node| node.id);
            let Some(target) = target else { continue };
            if self.rows[slot].is_none() || self.resolver.state(target).is_ok() {
                continue;
            }
            let needed = candidate
                .nodes()
                .iter()
                .filter_map(|node| candidate.program(node.id))
                .any(|program| {
                    program.target(target).is_some_and(|target| {
                        target
                            .segments()
                            .iter()
                            .any(|segment| segment.mode == Mode::Replacing)
                    })
                });
            if !needed {
                continue;
            }
            let Some(initial) = self.node_pairs.prepared_initial(target) else {
                ready = false;
                continue;
            };
            self.shared
                .as_mut()
                .ok_or(Error::InvalidState)?
                .reserve_autonomous(target)?;
            if initial.frames == 0 {
                continue;
            }
            if let Some(writer) = self
                .shared
                .as_ref()
                .and_then(|shared| shared.live_for(target))
            {
                let job = self.shared.as_ref().unwrap().live_job(writer)?;
                if let Some(error) = job.failure {
                    return Err(error.into());
                }
                ready &= job.ready;
                continue;
            }
            let view = self
                .node_pairs
                .prepared_view(target)
                .ok_or(Error::InvalidState)?;
            let audio = if initial.dry_wet {
                view.auxiliary
            } else {
                view.audio
            };
            let midi = view.midi;
            let original = view.windows.unwrap_or_default();
            let widths = initial.audio_widths.unwrap_or([0; 2]);
            let mut windows = original;
            if initial.dry_wet {
                windows.audio[widths[1] as usize..].fill(Default::default());
            }
            let source = self.history_status(initial.history_source, io)?.id;
            let shared = self.shared.as_mut().ok_or(Error::InvalidState)?;
            let writer = shared.begin_live_snapshots(
                super::shared::Fence {
                    target,
                    timing_revision: 0,
                    revision: Revision(0),
                    length: initial.frames,
                    graph: self.graph_epoch,
                    topology: self.resolver.epoch(),
                },
                source,
                initial.frames,
                [
                    audio.map(|source| source.snapshot),
                    midi.map(|source| source.snapshot),
                ],
                indexed_midi::WriteKind::Fixed,
                1,
                windows,
                frame,
                io,
                true,
            )?;
            let result = (|| {
                if initial.dry_wet {
                    let wet = view.audio;
                    let mut windows = Windows::default();
                    let width = widths[0] as usize;
                    windows.audio[..width].copy_from_slice(
                        &original.audio[widths[1] as usize..widths[1] as usize + width],
                    );
                    shared.configure_processed(
                        writer,
                        wet.map(|source| source.snapshot),
                        windows,
                        width,
                        frame,
                        io,
                    )?;
                }
                shared.mark_autonomous(writer)?;
                Ok::<_, Error>(shared.live_job(writer)?.ready)
            })();
            match result {
                Ok(current) => ready &= current,
                Err(error) => {
                    shared.cancel_live(writer, io)?;
                    return Err(error.into());
                }
            }
        }
        Ok(ready)
    }

    pub(super) fn write_processed_capture(
        &mut self,
        slot: usize,
        frame: Frame,
        q: usize,
        ports: &AudioPorts<'_>,
        io: &mut ResourceIo<'_>,
    ) -> Result<(), Error> {
        let row = self.rows[slot].ok_or(Error::InvalidId)?;
        if let Some(writer) = row
            .active
            .filter(|_| row.dry_wet)
            .and_then(|take| take.writer)
        {
            self.shared
                .as_mut()
                .ok_or(Error::InvalidState)?
                .write_processed(writer, frame, q, ports, io)?;
        }
        Ok(())
    }

    pub(super) fn refresh_replacing_targets(&mut self) {
        self.replacing_targets.fill(false);
        mark_replacing_targets(
            self.resolver.topology(),
            &self.rows,
            &mut self.replacing_targets,
        );
        if let Some(candidate) = self.resolver.pending_program_topology() {
            mark_replacing_targets(candidate, &self.rows, &mut self.replacing_targets);
        }
        self.topologies
            .mark_replacing_targets(&self.rows, &mut self.replacing_targets);
    }

    pub(super) fn prepare_program_inputs(
        &mut self,
        frame: Frame,
        io: &mut ResourceIo<'_>,
    ) -> Result<bool, Error> {
        self.refresh_replacing_targets();
        if let Some(shared) = &mut self.shared {
            let rows = &self.rows;
            let pairs = &self.node_pairs;
            let targets = &self.replacing_targets;
            shared.release_unused_reservations(
                |target| {
                    rows.get(target.slot() as usize).is_some_and(|row| {
                        row.is_some_and(|row| {
                            row.assigned
                                && row.binding.target == target
                                && targets[target.slot() as usize]
                        })
                    }) || pairs.prepared_initial(target).is_some()
                },
                io,
            )?;
            shared.cancel_orphaned_autonomous(
                |target| {
                    rows.get(target.slot() as usize).is_some_and(|row| {
                        row.is_some_and(|row| row.assigned && row.binding.target == target)
                    }) || pairs.prepared_initial(target).is_some()
                },
                io,
            )?;
            for (slot, row) in rows.iter().enumerate() {
                if let Some(row) = row.filter(|row| row.assigned && targets[slot]) {
                    shared.reserve_autonomous(row.binding.target)?;
                }
            }
        }
        let mut ready = true;
        for slot in 0..self.rows.len() {
            if !self.replacing_targets[slot] {
                continue;
            }
            let Some(row) = self.rows[slot].filter(|row| row.assigned) else {
                continue;
            };
            let target = row.binding.target;
            let state = self.resolver.state(target).map_err(runtime_error)?;
            let writer = self
                .shared
                .as_ref()
                .and_then(|shared| shared.live_for(target));
            if let Some(writer) = writer {
                let shared = self.shared.as_ref().ok_or(Error::InvalidState)?;
                let job = shared.live_job(writer)?;
                let source = shared.status(row.history_source, self.graph_epoch, io)?;
                if job.autonomous
                    && !job.active
                    && (!self.replacing_targets[slot]
                        || job.fence.revision != state.content_revision
                        || job.fence.length != state.content_length
                        || job.fence.timing_revision != row.timing_revision)
                {
                    self.shared.as_mut().unwrap().cancel_live(writer, io)?;
                } else {
                    if job.autonomous
                        && !job.active
                        && (job.fence.graph != self.graph_epoch || job.source != source.id)
                    {
                        if source.failure.is_some() {
                            ready = false;
                            continue;
                        }
                        self.shared.as_mut().unwrap().rebind_prepared(
                            writer,
                            source.id,
                            self.graph_epoch,
                            self.resolver.epoch(),
                        )?;
                    }
                    if self.replacing_targets[slot] {
                        if let Some(error) = job.failure {
                            return Err(error);
                        }
                        ready &= job.ready;
                    }
                    continue;
                }
            }
            if !self.replacing_targets[slot]
                || state.mode == Mode::Replacing
                || state.content_length == 0 && state.mode != Mode::Recording
                || row.active.is_some_and(|take| take.writer.is_some())
            {
                continue;
            }
            let source = self.history_status(row.history_source, io)?.id;
            let reply = self
                .shared_operation(
                    if state.mode == Mode::Recording {
                        Operation::WriterPrepareRecording { target, source }
                    } else {
                        Operation::WriterPrepareReplacing { target, source }
                    },
                    frame,
                    self.graph_epoch,
                    io,
                )
                .map_err(|failure| match failure {
                    Failure::Runtime(error) => error,
                    Failure::Timeline(error) => runtime_error(error),
                })?;
            let writer = match reply {
                Reply::ReplacingPrepared { id, .. } => id,
                Reply::Writer { id, .. } => Some(id),
                _ => return Err(Error::InvalidState),
            };
            if let Some(writer) = writer {
                let shared = self.shared.as_mut().unwrap();
                shared.mark_autonomous(writer)?;
                ready &= shared.live_job(writer)?.ready;
            }
        }
        Ok(ready)
    }
}
