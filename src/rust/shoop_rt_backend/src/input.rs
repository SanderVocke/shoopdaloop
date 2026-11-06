//! Application mutation ownership for one atomic input transaction.
use crate::{
    requests::{Pending, Purpose},
    EngineClock, Link, RtBackend,
};
use alloc::{format, string::String, vec::Vec};
use anyhow::{anyhow, Result};
use shoop_backend::{
    BackendGrabRequest, BackendInputEdit, BackendLoopId, BackendMutationCompletion,
    BackendMutationKind,
};
use shoop_rt_protocol::{Capabilities, InputEdit, InputIntent, InputPhase, Outcome, Request};
use shoop_rt_transport::carrier::Carrier;

#[derive(Clone, Copy)]
enum Step {
    Identity,
    Begin,
    Append(usize),
    Prepare,
    Commit,
    Cancel,
}
pub(crate) struct Job {
    pub sequence: u64,
    id: u64,
    edits: Vec<(BackendLoopId, InputEdit)>,
    delay: Option<u32>,
    step: Step,
    outstanding: bool,
    owned: bool,
    cancel: bool,
    cancel_sent: bool,
    failure: Option<String>,
}
impl Job {
    pub(crate) fn touches(&self, target: BackendLoopId) -> bool {
        self.edits.iter().any(|(id, _)| *id == target)
    }
    pub fn blocks(&self, pending: &Pending) -> bool {
        if pending.purpose == Purpose::Input || pending.sequence <= self.sequence {
            return false;
        }
        let target = match pending.request {
            Request::LoopTransition { loop_id, .. }
            | Request::LoopClear { loop_id }
            | Request::LoopSync { loop_id, .. }
            | Request::LoopGain { loop_id, .. }
            | Request::LoopBalance { loop_id, .. } => Some(loop_id),
            _ => None,
        };
        target.is_some_and(|id| self.edits.iter().any(|(_, edit)| edit.loop_id == id))
    }
}
impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn begin_input(
        &mut self,
        edits: Vec<(BackendLoopId, InputEdit)>,
        delay: Option<u32>,
    ) -> Result<()> {
        self.ready()?;
        if !self.capabilities.contains(Capabilities::INPUT_BATCH) {
            return Err(anyhow!("this engine cannot perform input transactions"));
        }
        if self.input.is_some() {
            return Err(anyhow!("an input transaction is pending"));
        }
        if edits.is_empty() {
            return Ok(());
        }
        if edits.len() > self.limits().loops as usize {
            return Err(anyhow!("too many input targets"));
        }
        for (id, edit) in &edits {
            if self.pending_kind_change(*id) {
                return Err(anyhow!("input target identity is changing"));
            }
            let entry = self
                .mirror
                .live_loop(*id)
                .ok_or_else(|| anyhow!("unknown input target"))?;
            if entry.program.is_some()
                && !matches!(
                    edit.intent,
                    InputIntent::Mode {
                        mode: shoop_rt_protocol::LoopMode::Stopped
                            | shoop_rt_protocol::LoopMode::Playing,
                        position: None,
                    }
                )
            {
                return Err(anyhow!("input edits require primitive loops"));
            }
            let changes_media = matches!(
                edit.intent,
                InputIntent::Grab { .. }
                    | InputIntent::Gesture { .. }
                    | InputIntent::Mode {
                        mode: shoop_rt_protocol::LoopMode::Replacing,
                        ..
                    }
            );
            if changes_media
                && entry
                    .track
                    .and_then(|id| self.mirror.tracks.get(&id))
                    .is_none_or(|track| {
                        !matches!(
                            track.topology,
                            shoop_backend::BackendTrackTopology::Direct { .. }
                        )
                    })
            {
                return Err(anyhow!("Grab and Replacing require a direct track"));
            }
        }
        let sequence = self.requests.reserve_mutation();
        self.input = Some(Job {
            sequence,
            id: 0,
            edits,
            delay,
            step: Step::Identity,
            outstanding: false,
            owned: false,
            cancel: false,
            cancel_sent: false,
            failure: None,
        });
        Ok(())
    }
    pub(crate) fn input_edits(
        &mut self,
        requests: &[BackendInputEdit],
        delay: Option<u32>,
    ) -> Result<()> {
        let option = |n: Option<i32>| {
            n.map(|n| u32::try_from(n).map_err(|_| anyhow!("negative Grab cycle")))
                .transpose()
        };
        let mut edits = Vec::with_capacity(requests.len());
        for request in requests {
            let (id, intent) = match *request {
                BackendInputEdit::Mode {
                    loop_id,
                    mode,
                    position,
                } => (
                    loop_id,
                    InputIntent::Mode {
                        mode: crate::backend::protocol_mode(mode)?,
                        position,
                    },
                ),
                BackendInputEdit::Grab(request) => {
                    let sync = self
                        .mirror
                        .loops
                        .get(&request.loop_id)
                        .ok_or_else(|| anyhow!("unknown input target"))?
                        .sync
                        .map(|id| self.live_loop_slot(id))
                        .transpose()?;
                    (
                        request.loop_id,
                        InputIntent::Grab {
                            sync,
                            reverse_start_cycle: option(request.reverse_start_cycle)?,
                            cycles_length: option(request.cycles_length)?,
                            go_to_cycle: option(request.go_to_cycle)?,
                            mode: crate::backend::protocol_mode(request.go_to_mode)?,
                            maximum_frames: 0,
                        },
                    )
                }
                BackendInputEdit::Gesture {
                    loop_id,
                    sync,
                    target,
                    synced,
                    cycles,
                    play_after_record,
                } => (
                    loop_id,
                    InputIntent::Gesture {
                        sync: self.live_loop_slot(sync)?,
                        target: target.map(|id| self.live_loop_slot(id)).transpose()?,
                        synced,
                        cycles,
                        play_after_record,
                    },
                ),
            };
            let slot = self.live_loop_slot(id)?;
            if edits
                .iter()
                .any(|(_, edit): &(BackendLoopId, InputEdit)| edit.loop_id == slot)
            {
                return Err(anyhow!("duplicate input target"));
            }
            edits.push((
                id,
                InputEdit {
                    loop_id: slot,
                    intent,
                },
            ));
        }
        self.begin_input(edits, delay)
    }
    pub(crate) fn grab_input(&mut self, requests: &[BackendGrabRequest]) -> Result<()> {
        self.input_edits(
            &requests
                .iter()
                .copied()
                .map(BackendInputEdit::Grab)
                .collect::<Vec<_>>(),
            None,
        )
    }
    pub(crate) fn cancel_input_for(&mut self, target: BackendLoopId) {
        if let Some(job) = self
            .input
            .as_mut()
            .filter(|job| job.edits.iter().any(|(id, _)| *id == target))
        {
            job.cancel = true;
        }
    }
    pub(crate) fn advance_input(&mut self) {
        if self.link != Link::Ready {
            return;
        }
        let Some(job) = self.input.as_mut() else {
            return;
        };
        if !self.requests.answered_through(job.sequence - 1)
            || self
                .composites
                .jobs
                .iter()
                .any(|old| old.sequence < job.sequence)
            || self
                .composites
                .controls
                .keys()
                .any(|sequence| *sequence < job.sequence)
        {
            return;
        }
        if job.cancel && job.owned && !job.cancel_sent {
            job.cancel_sent = true;
            self.requests
                .push_internal(Request::InputCancel { id: job.id }, Purpose::Input);
            return;
        }
        if job.outstanding {
            return;
        }
        if job.cancel && !job.owned {
            self.finish_input(Err("input transaction cancelled".into()));
            return;
        }
        let id = job.id;
        let request = match job.step {
            Step::Identity => Request::InputStatus { id: 0 },
            Step::Begin => Request::InputBegin {
                id,
                count: job.edits.len() as u32,
            },
            Step::Append(index) => Request::InputAppend {
                id,
                edit: job.edits[index].1,
            },
            Step::Prepare => Request::InputPrepare { id },
            Step::Commit => Request::InputCommit {
                id,
                delay: job.delay,
            },
            Step::Cancel => Request::InputCancel { id },
        };
        job.outstanding = true;
        self.requests.push_internal(request, Purpose::Input);
    }
    pub(crate) fn input_answered(&mut self, request: Request, outcome: Outcome) {
        let Some(job) = self.input.as_mut() else {
            return;
        };
        if request == (Request::InputStatus { id: 0 }) {
            if !matches!(job.step, Step::Identity) {
                return;
            }
            job.outstanding = false;
            match outcome {
                Outcome::Input(progress) if progress.phase.terminal() => {
                    if let Some(id) = progress.id.checked_add(1) {
                        job.id = id;
                        job.step = Step::Begin;
                        return;
                    }
                }
                _ => {}
            }
            self.finish_input(Err(
                "the engine input transaction is occupied or unavailable".into(),
            ));
            return;
        }
        let id = match request {
            Request::InputBegin { id, .. }
            | Request::InputAppend { id, .. }
            | Request::InputPrepare { id }
            | Request::InputCommit { id, .. }
            | Request::InputCancel { id }
            | Request::InputStatus { id } => id,
            _ => return,
        };
        if id != job.id {
            return;
        }
        let outcome = match outcome {
            Outcome::Input(progress) if progress.id == job.id => progress,
            Outcome::Refused(reason) => {
                let error = format!(
                    "input transaction refused: {}",
                    crate::incoming::refusal_text(reason)
                );
                if matches!(job.step, Step::Begin | Step::Cancel)
                    || matches!(request, Request::InputCancel { .. })
                {
                    self.finish_input(Err(error));
                } else {
                    job.failure = Some(error);
                    job.step = Step::Cancel;
                    job.outstanding = false;
                }
                return;
            }
            _ => {
                self.finish_input(Err("unexpected input transaction outcome".into()));
                return;
            }
        };
        if outcome.phase.terminal() {
            let result = match outcome.phase {
                InputPhase::Applied => Ok(()),
                InputPhase::Cancelled => Err(job
                    .failure
                    .take()
                    .unwrap_or_else(|| "input transaction cancelled".into())),
                InputPhase::Failed(reason) => Err(format!(
                    "input transaction refused: {}",
                    crate::incoming::refusal_text(reason)
                )),
                _ => unreachable!(),
            };
            self.finish_input(result);
            return;
        }
        if matches!(request, Request::InputCancel { .. }) {
            return;
        }
        job.outstanding = false;
        job.step = match job.step {
            Step::Begin => {
                job.owned = true;
                Step::Append(0)
            }
            Step::Append(index) if index + 1 < job.edits.len() => Step::Append(index + 1),
            Step::Append(_) => Step::Prepare,
            Step::Prepare if outcome.phase == InputPhase::Ready => Step::Commit,
            // Preparation/commit RPCs normally answer only at their completed cut.
            Step::Prepare => Step::Prepare,
            Step::Commit => {
                job.outstanding = true;
                return;
            }
            Step::Identity | Step::Cancel => return,
        };
    }
    fn finish_input(&mut self, result: core::result::Result<(), String>) {
        let Some(job) = self.input.take() else { return };
        if let Err(message) = &result {
            self.failure(
                BackendMutationKind::LoopControl,
                job.edits.first().map(|(id, _)| id.raw()),
                None,
                message.clone(),
            );
        } else {
            for (id, _) in job.edits {
                self.canonical_content.remove(&id);
                self.canonical_imports.remove(&id);
                self.prepared_content.remove(&id);
                self.reads.forget(id);
            }
        }
        self.mutation_completions.push(BackendMutationCompletion {
            sequence: job.sequence,
            result,
        });
        self.telemetry_fresh = false;
    }
}
