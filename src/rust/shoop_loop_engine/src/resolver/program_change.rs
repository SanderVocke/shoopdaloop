use super::*;

#[derive(Debug)]
pub struct ProgramChangeResult {
    pub completion: Completion,
    /// Displaced topology on activation, otherwise the rejected/cancelled candidate.
    pub topology: PreparedTopology,
    pub activated_epoch: Option<Epoch>,
}

pub(super) struct ProgramChange {
    pub id: RequestId,
    pub target: LoopId,
    pub expected: Epoch,
    pub candidate: PreparedTopology,
    pub activated_epoch: Option<Epoch>,
}

impl Boundary<'_> {
    pub(super) fn prepare_replacement(&mut self, change: Replacement) -> Result<(), Error> {
        let old = self.replaced.expect("selected old program");
        let new = self
            .topology
            .program(change.target)
            .expect("selected new program");
        self.scratch.states[change.target.slot() as usize]
            .as_mut()
            .expect("composite state")
            .content_length = self.topology.node(change.target)?.length;
        self.event(change.target, EventKind::Transition(Transition::Program));
        if change.was_playing {
            for (index, timeline) in old.targets().iter().enumerate() {
                if new.target(timeline.target).is_none()
                    && (change.authoritative
                        || timeline
                            .at(change.previous)
                            .is_some_and(|s| s.mode != Mode::Stopped))
                {
                    self.offer(
                        timeline.target.slot() as usize,
                        Intent {
                            transition: Transition::Stop,
                            priority: Priority::Action {
                                source: change.target,
                                ordinal: self.topology.limits().program_entries + index,
                                inherited: new.kind == ProgramKind::Regular,
                            },
                            request: None,
                            origin: Some(change.id),
                        },
                    );
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(super) struct Replacement {
    pub id: RequestId,
    pub target: LoopId,
    pub was_playing: bool,
    pub previous: u64,
    pub authoritative: bool,
}

impl Resolver {
    /// Retain one owning program change until its eligible boundary and result collection.
    ///
    /// Only this composite's program/compiled nominal length may change. Logical identities,
    /// sync/trigger relations and other programs stay fixed. Running changes wait for
    /// zero entry (or a stopped cut); future requests wait until their effective frame.
    /// Admission reserves a normal request slot and the displaced owner's return slot.
    pub fn stage_program(
        &mut self,
        expected: Epoch,
        target: LoopId,
        sequence: Sequence,
        at: Frame,
        candidate: PreparedTopology,
    ) -> Result<RequestId, (Error, PreparedTopology)> {
        let result = (|| {
            if self.quiescent || self.committed || self.epoch != expected {
                return Err(Error::InvalidState);
            }
            if self.program_change.is_some() {
                return Err(Error::Capacity);
            }
            self.epoch.checked_next().map_err(|_| Error::Exhausted)?;
            let metadata_bytes = self
                .topology
                .storage_bytes()
                .checked_add(candidate.storage_bytes())
                .and_then(|bytes| bytes.checked_add(size_of::<ProgramChange>()))
                .ok_or(Error::Capacity)?;
            checked_storage(metadata_bytes, 1)?;
            if self.topology.node(target)?.kind != LoopKind::Composite {
                return Err(Error::InvalidProgram);
            }
            if candidate.limits() != self.topology.limits()
                || candidate.nodes().len() != self.topology.nodes().len()
                || candidate.triggers() != self.topology.triggers()
                || candidate.program(target).is_none()
                || self.topology.program(target).is_none()
            {
                return Err(Error::InvalidProgram);
            }
            for (new, old) in candidate.nodes().iter().zip(self.topology.nodes()) {
                if new.id != old.id
                    || new.kind != old.kind
                    || new.sync_source != old.sync_source
                    || (new.id != target
                        && (new.length != old.length
                            || candidate.program(new.id) != self.topology.program(old.id)))
                {
                    return Err(Error::InvalidProgram);
                }
            }
            self.submit_record(Request {
                target,
                sequence,
                at,
                timing: Timing::At,
                transition: Transition::Program,
            })
        })();
        match result {
            Ok(id) => {
                self.program_change = Some(ProgramChange {
                    id,
                    target,
                    expected,
                    candidate,
                    activated_epoch: None,
                });
                Ok(id)
            }
            Err(error) => Err((error, candidate)),
        }
    }

    pub(crate) fn preview_program_change(&self) -> Result<Option<RequestId>, Error> {
        if self.preview.is_none() || self.committed {
            return Err(Error::InvalidState);
        }
        Ok(self.preview_program_epoch.map(|_| {
            self.program_change
                .as_ref()
                .expect("selected owner retained")
                .id
        }))
    }

    /// Collect a terminal receipt with its owner; destruction remains the caller's job.
    pub fn take_program_change(
        &mut self,
        id: RequestId,
    ) -> Result<Option<ProgramChangeResult>, Error> {
        if self.committed {
            return Err(Error::InvalidState);
        }
        if self.program_change.as_ref().map(|change| change.id) != Some(id) {
            return Err(Error::InvalidId);
        }
        let Some(completion) = self.completion(id)? else {
            return Ok(None);
        };
        let change = self.program_change.take().expect("validated owner");
        self.take_completion(id)
            .expect("validated receipt")
            .expect("terminal receipt");
        Ok(Some(ProgramChangeResult {
            completion,
            topology: change.candidate,
            activated_epoch: change.activated_epoch,
        }))
    }

    pub(super) fn select_program(
        &mut self,
        ceiling: Frame,
        observations: &[Observation],
    ) -> Result<Option<Replacement>, Error> {
        let Some(change) = self.program_change.as_ref() else {
            return Ok(None);
        };
        let record = self.record(change.id)?;
        if change.activated_epoch.is_some()
            || record.terminal.is_some()
            || record.effective > self.frame
        {
            return Ok(None);
        }
        let id = change.id;
        let target = change.target;
        let refused = if change.expected != self.epoch {
            Some(Error::InvalidState)
        } else {
            observations
                .iter()
                .find_map(|observation| match *observation {
                    Observation::Refused { request, reason } if request == id => Some(reason),
                    _ => None,
                })
        };
        if let Some(reason) = refused {
            self.scratch.records[id.slot() as usize]
                .as_mut()
                .expect("retained program request")
                .terminal = Some(Outcome::Failed {
                frame: self.frame,
                reason,
            });
            return Ok(None);
        }
        let before = self.state(target)?;
        let probe = self.probe_program_boundary(target, observations)?;
        let terminal = self.scratch.records[id.slot() as usize]
            .expect("retained program request")
            .terminal;
        let authoritative = self.scratch.program_actions[target.slot() as usize]
            != ProgramAction::None
            || (before.mode == Mode::Playing && probe.state.mode == Mode::Stopped);
        self.begin_preview(ceiling, observations)?;
        if terminal.is_some() {
            self.scratch.records[id.slot() as usize]
                .as_mut()
                .expect("retained program request")
                .terminal = terminal;
            return Ok(None);
        }
        if probe.state.mode != Mode::Stopped && !probe.enters_zero() {
            return Ok(None);
        }
        self.preview_program_epoch = Some(self.epoch.checked_next().map_err(|_| Error::Exhausted)?);
        self.scratch.records[id.slot() as usize]
            .as_mut()
            .expect("retained program request")
            .terminal = Some(Outcome::Applied { frame: self.frame });
        Ok(Some(Replacement {
            id,
            target,
            was_playing: before.mode == Mode::Playing,
            previous: before.composite.expect("composite").iteration,
            authoritative,
        }))
    }
}
