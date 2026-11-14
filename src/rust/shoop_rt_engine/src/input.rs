//! Controller ownership of prepared input transactions and terminal cuts.

use crate::endpoint::{
    completion_refusal, failure_refusal, loop_id, timeline_refusal, ControlEndpoint, Purpose,
    RuntimeResult,
};
use alloc::{boxed::Box, vec};
use rt_runtime::{Epoch, Sequence};
use shoop_loops::{
    runtime::{
        CandidateStatus, HistoryId, InputAction, InputEdit as RuntimeEdit, LiveWriterId, Operation,
        Reply, SharedSettings, INPUT_BATCH_RECORD_BYTES,
    },
    Mode, Timing,
};
use shoop_rt_protocol::{
    InputCapacity, InputEdit, InputIntent, InputPhase, InputProgress, LoopMode, Outcome, Refusal,
    Request,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Step {
    History(usize),
    Prepare(usize),
    Status(usize),
    Cancel(usize),
    Commit,
    Abort,
}
#[derive(Clone, Copy)]
struct Entry {
    edit: InputEdit,
    source: Option<HistoryId>,
    writer: Option<LiveWriterId>,
    ready: bool,
}
pub(crate) struct Stage {
    id: Option<u64>,
    last_id: u64,
    entries: Box<[Option<Entry>]>,
    expected: usize,
    received: usize,
    index: usize,
    phase: InputPhase,
    terminal: Option<InputPhase>,
    cleanup: bool,
    outstanding: bool,
    cancel_requested: bool,
    abort_pending: bool,
    abort_sent: bool,
    admitted: bool,
    correlation: Option<u64>,
    cancel_correlation: Option<u64>,
    graph: Epoch,
    topology: Epoch,
    requested: Option<u64>,
    effective: Option<u64>,
    sequence: Option<Sequence>,
    pub payload: Option<Box<[u8]>>,
    capacity: InputCapacity,
}
impl Stage {
    pub(crate) fn idle(&self) -> bool {
        (self.id.is_none() || self.phase.terminal())
            && !self.cleanup
            && !self.outstanding
            && !self.abort_pending
    }
    pub fn capacity(settings: &SharedSettings) -> InputCapacity {
        let Some(midi) = settings.midi_pool else {
            return InputCapacity::default();
        };
        InputCapacity {
            retention_frames: midi.history_frames,
            writer_slots: settings.candidates as u32,
            events_per_source: midi.history_events as u32,
            bytes_per_source: midi.history_bytes as u64,
            events_per_tick: midi.events_per_tick as u32,
            bytes_per_tick: midi.bytes_per_tick as u32,
            maximum_packet: midi.packets.max_packet as u32,
        }
    }
    pub(crate) fn supported(&self) -> bool {
        self.capacity.retention_frames > 0
    }
    pub(crate) fn replacing_supported(&self) -> bool {
        self.supported() && self.capacity.writer_slots > 0
    }
    pub fn new(loops: usize, capacity: InputCapacity) -> Self {
        Self {
            id: None,
            last_id: 0,
            entries: vec![None; loops].into_boxed_slice(),
            expected: 0,
            received: 0,
            index: 0,
            phase: InputPhase::Cancelled,
            terminal: None,
            cleanup: false,
            outstanding: false,
            cancel_requested: false,
            abort_pending: false,
            abort_sent: false,
            admitted: false,
            correlation: None,
            cancel_correlation: None,
            graph: Epoch(0),
            topology: Epoch(0),
            requested: None,
            effective: None,
            sequence: None,
            payload: Some(vec![0; loops * INPUT_BATCH_RECORD_BYTES].into_boxed_slice()),
            capacity,
        }
    }
    fn busy(&self) -> bool {
        !self.phase.terminal()
            || self.cleanup
            || self.outstanding
            || self.abort_pending
            || self.payload.is_none()
    }
    fn progress(&self) -> InputProgress {
        InputProgress {
            id: self.id.unwrap_or(0),
            phase: self.phase,
            received: self.received as u32,
            expected: self.expected as u32,
            requested_frame: self.requested,
            effective_frame: self.effective,
        }
    }
    fn finish(&mut self, phase: InputPhase) {
        self.terminal = Some(phase);
        self.cleanup = true;
        self.index = 0;
        if phase != InputPhase::Applied {
            self.phase = InputPhase::Cancelling;
        }
    }
}
fn mode(mode: LoopMode) -> Mode {
    match mode {
        LoopMode::Stopped => Mode::Stopped,
        LoopMode::Playing => Mode::Playing,
        LoopMode::Recording => Mode::Recording,
        LoopMode::Replacing => Mode::Replacing,
        LoopMode::PlayingDryThroughWet => Mode::PlayingDryThroughWet,
        LoopMode::RecordingDryIntoWet => Mode::RecordingDryIntoWet,
    }
}
impl ControlEndpoint {
    pub(crate) fn input_request(
        &mut self,
        correlation: u64,
        request: Request,
    ) -> Result<Option<Outcome>, Refusal> {
        if request == Request::InputCapacity {
            return Ok(Some(Outcome::InputCapacity(self.input.capacity)));
        }
        if let Request::InputBegin { id, count } = request {
            if self.input.id == Some(id) && count as usize == self.input.expected {
                return Ok(Some(Outcome::Input(self.input.progress())));
            }
            if self.job.is_some()
                || self.family.id.is_some()
                || self.replacement.id.is_some()
                || self.input.busy()
            {
                return Err(Refusal::Busy);
            }
            if self.input.capacity.retention_frames == 0 {
                return Err(Refusal::Unsupported);
            }
            if id == 0 || id <= self.input.last_id {
                return Err(Refusal::InvalidId);
            }
            if count == 0 || count as usize > self.input.entries.len() {
                return Err(Refusal::Capacity);
            }
            self.input.id = Some(id);
            self.input.last_id = id;
            self.input.expected = count as usize;
            self.input.received = 0;
            self.input.entries.fill(None);
            self.input.index = 0;
            self.input.phase = InputPhase::Receiving;
            self.input.terminal = None;
            self.input.cleanup = false;
            self.input.sequence = None;
            self.input.requested = None;
            self.input.effective = None;
            self.input.admitted = false;
            self.input.cancel_requested = false;
            self.input.abort_sent = false;
            self.input.graph = self.graph_epoch;
            self.input.topology = self.topology_epoch;
            return Ok(Some(Outcome::Input(self.input.progress())));
        }
        let id = match request {
            Request::InputAppend { id, .. }
            | Request::InputPrepare { id }
            | Request::InputCommit { id, .. }
            | Request::InputCancel { id }
            | Request::InputStatus { id } => id,
            _ => return Err(Refusal::Internal),
        };
        if request == (Request::InputStatus { id: 0 }) {
            return Ok(Some(Outcome::Input(self.input.progress())));
        }
        if self.input.id != Some(id) {
            return Err(Refusal::InvalidId);
        }
        match request {
            Request::InputStatus { .. } => Ok(Some(Outcome::Input(self.input.progress()))),
            Request::InputAppend { edit, .. } => {
                if self.input.phase != InputPhase::Receiving {
                    return Err(Refusal::Busy);
                }
                self.model
                    .find_loop(edit.loop_id)
                    .ok_or(Refusal::InvalidId)?;
                if let Some(old) = self
                    .input
                    .entries
                    .iter()
                    .flatten()
                    .find(|old| old.edit.loop_id == edit.loop_id)
                {
                    return if old.edit == edit {
                        Ok(Some(Outcome::Input(self.input.progress())))
                    } else {
                        Err(Refusal::InvalidId)
                    };
                }
                match edit.intent {
                    InputIntent::Mode {
                        mode: LoopMode::PlayingDryThroughWet | LoopMode::RecordingDryIntoWet,
                        ..
                    } => return Err(Refusal::Unsupported),
                    InputIntent::Mode { mode, position }
                        if position.is_some()
                            && matches!(mode, LoopMode::Stopped | LoopMode::Recording) =>
                    {
                        return Err(Refusal::InvalidValue)
                    }
                    InputIntent::Mode {
                        position: Some(position),
                        ..
                    } if position % u64::from(self.limits.quantum) != 0 => {
                        return Err(Refusal::InvalidValue)
                    }
                    InputIntent::Gesture { sync, .. }
                    | InputIntent::Grab {
                        sync: Some(sync), ..
                    } if self.model.find_loop(sync).is_none() => return Err(Refusal::InvalidId),
                    InputIntent::Gesture {
                        target: Some(target),
                        ..
                    } if self.model.find_loop(target).is_none() => return Err(Refusal::InvalidId),
                    InputIntent::Grab {
                        mode,
                        maximum_frames,
                        ..
                    } if mode != LoopMode::Recording && maximum_frames != 0 => {
                        return Err(Refusal::InvalidValue)
                    }
                    _ => {}
                }
                if self.input.received == self.input.expected {
                    return Err(Refusal::Capacity);
                }
                self.input.entries[self.input.received] = Some(Entry {
                    edit,
                    source: None,
                    writer: None,
                    ready: false,
                });
                self.input.received += 1;
                Ok(Some(Outcome::Input(self.input.progress())))
            }
            Request::InputPrepare { .. } => {
                if self.input.phase == InputPhase::Ready {
                    return Ok(Some(Outcome::Input(self.input.progress())));
                }
                if self.input.phase != InputPhase::Receiving
                    || self.input.received != self.input.expected
                {
                    return Err(Refusal::Busy);
                }
                self.input.correlation = Some(correlation);
                self.input.phase = InputPhase::Preparing;
                self.input.index = 0;
                self.advance_input();
                Ok(None)
            }
            Request::InputCommit { delay, .. } => {
                if self.input.phase == InputPhase::Committing || self.input.phase.terminal() {
                    return Ok(Some(Outcome::Input(self.input.progress())));
                }
                if self.input.phase != InputPhase::Ready || self.input.cleanup {
                    return Err(Refusal::Busy);
                }
                if self.input.graph != self.graph_epoch
                    || self.input.topology != self.topology_epoch
                {
                    return Err(Refusal::Busy);
                }
                if !self.room(1) {
                    return Err(Refusal::Overloaded);
                }
                let mut timing = Timing::At;
                if let Some(skip) = delay {
                    if self.input.entries.iter().flatten().any(|entry| {
                        matches!(
                            entry.edit.intent,
                            InputIntent::Grab { .. } | InputIntent::Gesture { .. }
                        )
                    }) {
                        return Err(Refusal::InvalidValue);
                    }
                    let first = self.input.entries[0].ok_or(Refusal::Internal)?.edit.loop_id;
                    let owner = self.model.find_loop(first).ok_or(Refusal::InvalidId)?;
                    if owner.sync.is_some() || self.observed_mode(first) == Mode::Playing {
                        timing = Timing::AfterTicks {
                            source: loop_id(owner.sync.unwrap_or(first)),
                            skip: u64::from(skip),
                        };
                    }
                }
                if matches!(timing, Timing::AfterTicks { .. })
                    && self.input.entries[..self.input.received]
                        .iter()
                        .flatten()
                        .any(|entry| {
                            self.planned[entry.edit.loop_id.0.index as usize]
                                .iter()
                                .all(Option::is_some)
                        })
                {
                    return Err(Refusal::Capacity);
                }
                let at = self.at();
                let mut payload = self.input.payload.take().ok_or(Refusal::Busy)?;
                payload.fill(0);
                let encoded = (|| {
                    for (index, entry) in self.input.entries[..self.input.received]
                        .iter()
                        .flatten()
                        .enumerate()
                    {
                        let action = match entry.edit.intent {
                            InputIntent::Gesture {
                                sync,
                                target,
                                synced,
                                cycles,
                                play_after_record,
                            } => {
                                let owner = self
                                    .model
                                    .find_loop(entry.edit.loop_id)
                                    .ok_or(Refusal::InvalidId)?;
                                InputAction::Gesture {
                                    source: owner.strip.ok_or(Refusal::InvalidId)?.0.index,
                                    history: entry.source,
                                    sync: loop_id(sync),
                                    target: target.map(loop_id),
                                    spec: shoop_loops::grab::GestureSpec {
                                        synced,
                                        cycles,
                                        play_after_record,
                                    },
                                }
                            }
                            InputIntent::Mode {
                                mode: value,
                                position,
                            } => InputAction::Mode {
                                mode: mode(value),
                                position,
                            },
                            InputIntent::Grab {
                                sync,
                                reverse_start_cycle,
                                cycles_length,
                                go_to_cycle,
                                mode: value,
                                maximum_frames,
                            } => {
                                let owner = self
                                    .model
                                    .find_loop(entry.edit.loop_id)
                                    .ok_or(Refusal::InvalidId)?;
                                InputAction::Grab {
                                    source: owner.strip.ok_or(Refusal::Unsupported)?.0.index,
                                    history: entry.source,
                                    sync: sync.map(loop_id),
                                    window: shoop_loops::grab::WindowSpec {
                                        reverse_start_cycle,
                                        cycles_length,
                                        go_to_cycle,
                                        continue_recording: value == LoopMode::Recording,
                                    },
                                    mode: mode(value),
                                    maximum_frames,
                                }
                            }
                        };
                        RuntimeEdit {
                            target: loop_id(entry.edit.loop_id),
                            action,
                        }
                        .encode(
                            &mut payload[index * INPUT_BATCH_RECORD_BYTES
                                ..(index + 1) * INPUT_BATCH_RECORD_BYTES],
                        )
                        .map_err(|_| Refusal::InvalidValue)?;
                    }
                    Ok(())
                })();
                if let Err(refusal) = encoded {
                    self.input.payload = Some(payload);
                    return Err(refusal);
                }
                let command = self.loops.command_with_payload(
                    at,
                    Operation::InputBatch {
                        count: self.input.received,
                        timing,
                        graph: self.input.graph,
                        topology: self.input.topology,
                    },
                    payload,
                );
                let sequence = self.submit(
                    command,
                    Purpose::Input {
                        id,
                        step: Step::Commit,
                    },
                    true,
                )?;
                self.input.sequence = Some(sequence);
                self.input.requested = Some(at.0);
                self.input.correlation = Some(correlation);
                self.input.phase = InputPhase::Committing;
                self.input.outstanding = true;
                if let Timing::AfterTicks { source, skip } = timing {
                    let source_slot = source.slot() as usize;
                    let baseline = self.cells.loops[source_slot]
                        .pulses
                        .load(core::sync::atomic::Ordering::Relaxed);
                    for entry in self.input.entries[..self.input.received].iter().flatten() {
                        if let InputIntent::Mode { mode, .. } = entry.edit.intent {
                            let slot = entry.edit.loop_id.0.index as usize;
                            if let Some(free) =
                                self.planned[slot].iter_mut().find(|entry| entry.is_none())
                            {
                                *free = Some(crate::endpoint::Planned {
                                    sequence,
                                    mode,
                                    delay: skip as u32,
                                    source: source_slot,
                                    baseline,
                                });
                                self.dirty[slot] = true;
                            }
                        }
                    }
                }
                Ok(None)
            }
            Request::InputCancel { .. } => {
                if self.input.phase.terminal() && !self.input.cleanup {
                    return Ok(Some(Outcome::Input(self.input.progress())));
                }
                if self.input.cancel_correlation.is_some() {
                    return Err(Refusal::Busy);
                }
                self.input.cancel_correlation = Some(correlation);
                self.input.cancel_requested = true;
                if self.input.phase != InputPhase::Committing {
                    self.input.finish(InputPhase::Cancelled);
                }
                self.advance_input();
                Ok(None)
            }
            _ => Err(Refusal::Internal),
        }
    }
    pub(crate) fn disconnect_input(&mut self) {
        self.input.correlation = None;
        self.input.cancel_correlation = None;
        if !self.input.phase.terminal() && self.input.phase != InputPhase::Committing {
            self.input.finish(InputPhase::Cancelled);
        }
    }
    fn input_fail(&mut self, refusal: Refusal) {
        self.input.finish(InputPhase::Failed(refusal));
    }
    fn input_submit(&mut self, operation: Operation, step: Step) -> bool {
        if !self.room(1) {
            return false;
        }
        let Some(id) = self.input.id else {
            return false;
        };
        match self.submit_loop(self.at(), operation, Purpose::Input { id, step }) {
            Ok(_) => {
                self.input.outstanding = true;
                true
            }
            Err(Refusal::Overloaded | Refusal::Capacity) => false,
            Err(reason) => {
                self.input_fail(reason);
                false
            }
        }
    }
    pub(crate) fn advance_input(&mut self) {
        if self.input.phase == InputPhase::Committing
            && !self.input.cleanup
            && self.input.cancel_requested
            && self.input.admitted
            && !self.input.abort_sent
            && self.room(1)
        {
            if let (Some(id), Some(sequence)) = (self.input.id, self.input.sequence) {
                if self
                    .submit_loop(
                        self.at(),
                        Operation::Cancel { sequence },
                        Purpose::Input {
                            id,
                            step: Step::Abort,
                        },
                    )
                    .is_ok()
                {
                    self.input.abort_sent = true;
                    self.input.abort_pending = true;
                }
            }
        }
        if self.input.outstanding || self.input.abort_pending {
            return;
        }
        if self.input.cleanup {
            while self.input.index < self.input.received {
                let index = self.input.index;
                if let Some(writer) = self.input.entries[index].and_then(|entry| entry.writer) {
                    self.input_submit(Operation::WriterCancel { id: writer }, Step::Cancel(index));
                    return;
                }
                if self.input.terminal == Some(InputPhase::Applied) {
                    let entry = self.input.entries[index].expect("received entry");
                    if self.model.find_loop(entry.edit.loop_id).is_some() {
                        if !self.timing_cached_after(
                            entry.edit.loop_id,
                            self.input.effective.unwrap_or(0),
                        ) {
                            return;
                        }
                    }
                }
                self.input.index += 1;
            }
            self.input.phase = self
                .input
                .terminal
                .take()
                .unwrap_or(InputPhase::Failed(Refusal::Internal));
            self.input.cleanup = false;
            let outcome = Outcome::Input(self.input.progress());
            if let Some(correlation) = self.input.correlation.take() {
                self.answer(correlation, outcome);
            }
            if let Some(correlation) = self.input.cancel_correlation.take() {
                self.answer(correlation, outcome);
            }
            return;
        }
        if matches!(self.input.phase, InputPhase::Preparing | InputPhase::Ready)
            && (self.input.graph != self.graph_epoch || self.input.topology != self.topology_epoch)
        {
            self.input_fail(Refusal::Busy);
            return;
        }
        if self.input.phase != InputPhase::Preparing {
            return;
        }
        while self.input.index < self.input.received {
            let index = self.input.index;
            let entry = self.input.entries[index].expect("received entry");
            let needs_source = matches!(
                entry.edit.intent,
                InputIntent::Grab { .. }
                    | InputIntent::Gesture { .. }
                    | InputIntent::Mode {
                        mode: LoopMode::Replacing,
                        ..
                    }
            );
            if needs_source && entry.source.is_none() {
                let Some(owner) = self.model.find_loop(entry.edit.loop_id) else {
                    self.input_fail(Refusal::InvalidId);
                    return;
                };
                let Some(strip) = owner.strip else {
                    self.input_fail(Refusal::Unsupported);
                    return;
                };
                self.input_submit(
                    Operation::HistoryStatus {
                        source: strip.0.index,
                    },
                    Step::History(index),
                );
                return;
            }
            if matches!(
                entry.edit.intent,
                InputIntent::Mode {
                    mode: LoopMode::Replacing,
                    ..
                }
            ) && !entry.ready
            {
                if let Some(writer) = entry.writer {
                    self.input_submit(Operation::WriterStatus { id: writer }, Step::Status(index));
                } else {
                    self.input_submit(
                        Operation::WriterPrepareReplacing {
                            target: loop_id(entry.edit.loop_id),
                            source: entry.source.expect("prepared source"),
                        },
                        Step::Prepare(index),
                    );
                }
                return;
            }
            self.input.index += 1;
        }
        self.input.phase = InputPhase::Ready;
        if let Some(correlation) = self.input.correlation.take() {
            self.answer(correlation, Outcome::Input(self.input.progress()));
        }
    }
    pub(crate) fn input_result(&mut self, id: u64, step: Step, result: RuntimeResult) {
        let current = self.input.id == Some(id);
        let execution = match result {
            RuntimeResult::Completion(outcome, command, retired) => {
                self.reclaim(command);
                self.retire(retired);
                if !current {
                    return;
                }
                if let Some(reason) = completion_refusal(outcome) {
                    if step == Step::Abort {
                        self.input.abort_pending = false;
                        self.input.abort_sent = false;
                    } else {
                        self.input.outstanding = false;
                        self.input_fail(reason);
                    }
                } else if step == Step::Commit {
                    self.input.admitted = true;
                }
                return;
            }
            RuntimeResult::Execution(execution) => execution,
        };
        if !current {
            return;
        }
        if step == Step::Abort {
            self.input.abort_pending = false;
            if execution.result.is_err()
                || !matches!(execution.operation_result, Some(Reply::Cancelled { .. }))
            {
                self.input.abort_sent = false;
            }
            return;
        }
        self.input.outstanding = false;
        if step == Step::Commit {
            self.input.requested = Some(execution.requested_frame.0);
            self.input.effective = Some(execution.actual_frame.0);
        }
        if let Err(error) = execution.result {
            self.input_fail(failure_refusal(shoop_loops::runtime::Failure::Runtime(
                error,
            )));
            return;
        }
        let reply = execution.operation_result;
        match (step, reply) {
            (Step::History(index), Some(Reply::History(status))) => {
                if let Some(error) = status.failure {
                    self.input_fail(failure_refusal(shoop_loops::runtime::Failure::Runtime(
                        error,
                    )));
                } else {
                    self.input.entries[index].as_mut().unwrap().source = Some(status.id);
                }
            }
            (Step::Prepare(index), Some(Reply::ReplacingPrepared { id, status })) => {
                self.input.entries[index].as_mut().unwrap().writer = id;
                self.input_candidate(index, status);
            }
            (Step::Status(index), Some(Reply::Writer { status, .. })) => {
                self.input_candidate(index, status)
            }
            (Step::Cancel(index), Some(Reply::Done)) => {
                self.input.entries[index].as_mut().unwrap().writer = None
            }
            (
                Step::Cancel(index),
                Some(Reply::Failed(shoop_loops::runtime::Failure::Runtime(
                    rt_runtime::Error::InvalidId | rt_runtime::Error::Busy,
                ))),
            ) => {
                self.input.entries[index].as_mut().unwrap().writer = None;
            }
            (Step::Commit, Some(Reply::Cancelled { .. })) => {
                self.input.finish(InputPhase::Cancelled)
            }
            (
                Step::Commit,
                Some(Reply::InputBatch {
                    requested,
                    effective,
                    outcome,
                    ..
                }),
            ) => {
                self.input.requested = Some(requested.0);
                self.input.effective = Some(effective.0);
                for slot in 0..self.planned.len() {
                    self.forget_planned(slot, execution.sequence);
                }
                let phase = match outcome {
                    shoop_loops::Outcome::Applied { .. } => InputPhase::Applied,
                    shoop_loops::Outcome::Cancelled { .. } => InputPhase::Cancelled,
                    shoop_loops::Outcome::Failed { reason, .. } => {
                        InputPhase::Failed(timeline_refusal(reason))
                    }
                    _ => InputPhase::Failed(Refusal::Busy),
                };
                self.input.finish(phase);
            }
            (_, Some(Reply::Failed(reason))) => self.input_fail(failure_refusal(reason)),
            _ => self.input_fail(Refusal::Internal),
        }
    }
    fn input_candidate(&mut self, index: usize, status: CandidateStatus) {
        match status {
            CandidateStatus::Ready => self.input.entries[index].as_mut().unwrap().ready = true,
            CandidateStatus::Building => {}
            CandidateStatus::Failed(error) => self.input_fail(failure_refusal(
                shoop_loops::runtime::Failure::Runtime(error),
            )),
        }
    }
}
