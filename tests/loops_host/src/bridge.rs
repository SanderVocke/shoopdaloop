//! Render-instance adapter. Fixed wire credits retain both result phases.
use crate::{
    scenario::{self, Mix},
    wire::{self, Error as WireError, Phase, Request, Response, Status, SLOTS},
};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    executor::{Command, Retired},
    transport::{Completion, Lane, Outcome, Ticket},
    Error, Frame,
};
use shoop_loops::{
    runtime::{Failure, LoopPlanner, Operation, Reply},
    LoopId, Outcome as DomainOutcome, Timing,
};
use shoop_loops_test_support::{LoopControl, LoopRuntime, StoppedLoops, TestNode};

type Returned = Completion<Command<TestNode<Mix>, LoopPlanner>, Retired<TestNode<Mix>>>;
struct Slot {
    request: Request,
    ticket: Option<Ticket>,
    admission: Option<Response>,
    terminal: Option<Response>,
    admission_sent: bool,
    effective: u64,
}
impl Slot {
    fn response(&self, phase: Phase, observed: u64) -> Response {
        Response {
            session: self.request.session,
            sequence: self.request.sequence,
            slot: self.request.slot,
            phase,
            status: Status::Rejected,
            runtime_sequence: self.ticket.as_ref().map_or(0, |ticket| ticket.sequence().0),
            requested: self.request.at,
            effective: self.effective,
            delivered: None,
            observed,
            error: 0,
        }
    }
}

pub struct Bridge {
    control: LoopControl<Mix>,
    engine: LoopRuntime<Mix>,
    target: LoopId,
    session: u64,
    last_sequence: u64,
    slots: [Option<Slot>; SLOTS],
    quarantine: Option<Returned>,
    faulted: bool,
    graphs: Option<Box<crate::graph_bridge::Graphs>>,
    exports: Option<Box<crate::export_bridge::Exports>>,
    staging: Option<Box<crate::staging_bridge::Staging>>,
    imported: [Option<rt_runtime::ResourceId>; 4],
    fault_witness: std::sync::Arc<std::sync::atomic::AtomicU32>,
}
impl Bridge {
    /// Allocating startup. Session routing is a host convention, not authentication.
    pub fn new(session: u64) -> Result<Self, WireError> {
        Self::with_fault(session, 0)
    }
    pub(crate) fn with_fault(session: u64, fault: u8) -> Result<Self, WireError> {
        Self::prepare(session, fault, 0)
    }
    fn prepare(session: u64, fault: u8, mode: u8) -> Result<Self, WireError> {
        if session == 0 {
            return Err(WireError::Session);
        }
        if fault > 2 {
            return Err(WireError::Operation);
        }
        let fault_witness = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let mut imported = [None; 4];
        let (control, engine, target) = if mode == 3 {
            let (control, engine, target, resources) =
                scenario::media_case::prepare(fault_witness.clone());
            imported = resources.map(Some);
            (control, engine, target)
        } else if mode == 2 {
            scenario::dense::prepare(fault_witness.clone())
        } else if mode == 1 {
            scenario::prepare_staged(fault_witness.clone())
        } else {
            scenario::prepare_with_fault(fault, fault_witness.clone())
        };
        let mut staging = (mode == 1 || mode == 3)
            .then(|| Box::new(crate::staging_bridge::Staging::new(session)));
        if mode == 3 {
            staging
                .as_mut()
                .unwrap()
                .set_catalog([0, 7].map(|revision| {
                    Some(
                        control
                            .media()
                            .iter()
                            .find(|m| {
                                m.target == target && m.revision == rt_runtime::Revision(revision)
                            })
                            .unwrap()
                            .media,
                    )
                }));
        }
        Ok(Self {
            control,
            engine,
            target,
            session,
            last_sequence: 0,
            slots: std::array::from_fn(|_| None),
            quarantine: None,
            faulted: false,
            graphs: None,
            exports: (mode == 3).then(|| {
                Box::new(crate::export_bridge::Exports::new(
                    session,
                    (scenario::media_case::PAGES * 64) as u32,
                ))
            }),
            staging,
            imported,
            fault_witness,
        })
    }
    /// Opt-in prepared graph owners and return credits; allocating startup only.
    pub fn with_graphs(session: u64) -> Result<Self, WireError> {
        Self::with_graphs_fault(session, 0)
    }
    pub(crate) fn with_graphs_fault(session: u64, fault: u8) -> Result<Self, WireError> {
        let mut bridge = Self::with_fault(session, fault)?;
        let config = bridge
            .control
            .compile_graph(&scenario::routes(false))
            .map_err(|_| WireError::State)?
            .config();
        bridge
            .engine
            .prepare_graph_staging(crate::graph_transfer::BANKS)
            .map_err(|_| WireError::State)?;
        bridge.graphs = Some(Box::new(
            crate::graph_bridge::Graphs::new(session, config).map_err(|_| WireError::State)?,
        ));
        Ok(bridge)
    }
    pub fn with_staging(session: u64) -> Result<Self, WireError> {
        Self::prepare(session, 0, 1)
    }
    pub fn with_dense(session: u64) -> Result<Self, WireError> {
        Self::prepare(session, 0, 2)
    }
    pub fn with_media(session: u64) -> Result<Self, WireError> {
        Self::prepare(session, 0, 3)
    }
    pub(crate) fn imported_resources(&self) -> [Option<rt_runtime::ResourceId>; 4] {
        self.imported
    }
    pub fn staging_ingest(&mut self, bytes: &[u8]) -> Result<(), WireError> {
        crate::guarded(|| {
            if self.faulted {
                return Err(WireError::State);
            }
            let now = self.next_frame();
            self.staging
                .as_mut()
                .ok_or(WireError::State)?
                .ingest(&mut self.control, bytes, now)
        })
    }
    pub fn next_staging_response(
        &mut self,
    ) -> Result<Option<[u8; crate::staging_wire::RESPONSE_BYTES]>, WireError> {
        crate::guarded(|| {
            self.staging
                .as_mut()
                .ok_or(WireError::State)?
                .next_response()
        })
    }
    pub fn retained_staging(&self) -> usize {
        self.staging.as_ref().map_or(0, |s| s.retained())
    }
    pub fn with_exports(session: u64) -> Result<Self, WireError> {
        let mut bridge = Self::new(session)?;
        let config = bridge
            .control
            .compile_graph(&scenario::routes(false))
            .map_err(|_| WireError::State)?
            .config();
        let resource_bytes =
            u32::try_from(config.resource_bytes()).map_err(|_| WireError::State)?;
        bridge.exports = Some(Box::new(crate::export_bridge::Exports::new(
            session,
            resource_bytes,
        )));
        Ok(bridge)
    }
    pub fn export_ingest(&mut self, bytes: &[u8]) -> Result<(), WireError> {
        crate::guarded(|| {
            if self.faulted {
                return Err(WireError::State);
            }
            let now = self.next_frame();
            self.exports
                .as_mut()
                .ok_or(WireError::State)?
                .ingest(&mut self.control, bytes, now)
        })
    }
    pub fn next_export_response(
        &mut self,
    ) -> Result<Option<[u8; crate::export_wire::RESPONSE_BYTES]>, WireError> {
        crate::guarded(|| {
            self.exports
                .as_mut()
                .ok_or(WireError::State)?
                .next_response()
        })
    }
    pub fn export_resource_bytes(&self) -> u32 {
        self.exports
            .as_ref()
            .map_or(0, |exports| exports.resource_bytes)
    }
    pub fn retained_exports(&self) -> usize {
        self.exports
            .as_ref()
            .map_or(0, |exports| exports.retained())
    }
    pub fn graph_payload(&mut self, bytes: &[u8]) -> Result<crate::graph_transfer::Phase, Error> {
        crate::guarded(|| {
            if self.faulted {
                return Err(Error::InvalidState);
            }
            self.graphs
                .as_mut()
                .ok_or(Error::InvalidState)?
                .payload(bytes)
        })
    }
    pub fn graph_ingest(&mut self, bytes: &[u8]) -> Result<(), WireError> {
        crate::guarded(|| {
            if self.faulted {
                return Err(WireError::State);
            }
            let now = self.next_frame();
            let graphs = self.graphs.as_mut().ok_or(WireError::State)?;
            let result = graphs.ingest(&mut self.control, bytes, now);
            self.faulted |= graphs.fatal;
            result
        })
    }
    pub fn next_graph_response(
        &mut self,
    ) -> Result<Option<[u8; crate::graph_wire::RESPONSE_BYTES]>, WireError> {
        crate::guarded(|| {
            self.graphs
                .as_mut()
                .ok_or(WireError::State)?
                .next_response()
        })
    }
    pub fn graph_cancel(&self, slot: usize, sequence: u64) -> Result<(), WireError> {
        crate::guarded(|| {
            self.graphs
                .as_ref()
                .ok_or(WireError::State)?
                .cancel(slot, sequence)
        })
    }
    pub fn retained_graphs(&self) -> usize {
        self.graphs.as_ref().map_or(0, |graphs| graphs.retained())
    }
    pub(crate) fn fault_witness(&self) -> u32 {
        self.fault_witness
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn target(&self) -> LoopId {
        self.target
    }
    pub fn next_frame(&self) -> u64 {
        self.engine.next_frame().0
    }
    pub fn outstanding(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
            + self
                .graphs
                .as_ref()
                .map_or(0, |graphs| graphs.outstanding())
            + self
                .exports
                .as_ref()
                .map_or(0, |exports| exports.outstanding())
            + self
                .staging
                .as_ref()
                .map_or(0, |staging| staging.outstanding())
    }

    /// Consume one well-formed request into its reserved slot. Refusal leaves the
    /// sequence watermark and all owners unchanged, so a Busy request can retry.
    pub fn ingest(&mut self, bytes: &[u8]) -> Result<(), WireError> {
        crate::guarded(|| self.ingest_inner(bytes))
    }
    fn ingest_inner(&mut self, bytes: &[u8]) -> Result<(), WireError> {
        if self.faulted {
            return Err(WireError::State);
        }
        let request = Request::decode(bytes)?;
        if request.session != self.session {
            return Err(WireError::Session);
        }
        if request.sequence <= self.last_sequence {
            return Err(WireError::Sequence);
        }
        if self.slots[request.slot].is_some() {
            return Err(WireError::Busy);
        }
        let mut slot = Slot {
            request,
            ticket: None,
            admission: None,
            terminal: None,
            admission_sent: false,
            effective: request.at,
        };
        let lane = if request.recovery {
            Lane::Recovery
        } else {
            Lane::Normal
        };
        match self.control.submit(
            lane,
            Frame(request.at),
            Operation::Transition {
                target: request.target,
                timing: Timing::At,
                transition: request.transition,
            },
        ) {
            Ok(ticket) => {
                if request.cancelled {
                    ticket.cancel();
                }
                slot.ticket = Some(ticket);
            }
            Err(refusal) => {
                let mut response = slot.response(Phase::Admission, self.next_frame());
                response.error = wire::runtime_error(refusal.error);
                slot.admission = Some(response);
                // Only the empty-payload transition command was constructed above.
                drop(refusal.value);
            }
        }
        self.last_sequence = request.sequence;
        self.slots[request.slot] = Some(slot);
        Ok(())
    }

    /// Best effort until the original transport dequeue; the admission response,
    /// not this return value, decides whether cancellation took effect.
    pub fn cancel(&mut self, slot: usize, sequence: u64) -> Result<(), WireError> {
        crate::guarded(|| {
            let slot = self
                .slots
                .get(slot)
                .and_then(Option::as_ref)
                .ok_or(WireError::Slot)?;
            if slot.request.sequence != sequence {
                return Err(WireError::Sequence);
            }
            let ticket = slot.ticket.as_ref().ok_or(WireError::State)?;
            ticket.cancel();
            Ok(())
        })
    }

    pub fn process(&mut self, audio: AudioBlock<'_, '_>, midi: MidiBlock<'_>) -> Result<(), Error> {
        crate::guarded(|| {
            if self.faulted {
                return Err(Error::InvalidState);
            }
            let nonempty = audio.frames != 0;
            let rendered = self.engine.process(audio, midi);
            let collected = if nonempty { self.collect() } else { Ok(()) };
            let result = rendered.and(collected);
            if result.is_err() {
                self.faulted = true;
            }
            result
        })
    }
    fn collect(&mut self) -> Result<(), Error> {
        // Fixed external credit bounds, never a quota on internal musical work.
        let credits = SLOTS
            + self
                .graphs
                .as_ref()
                .map_or(0, |_| crate::graph_wire::CREDITS)
            + self
                .exports
                .as_ref()
                .map_or(0, |_| crate::export_wire::CREDITS)
            + self
                .staging
                .as_ref()
                .map_or(0, |_| crate::staging_wire::CREDITS);
        for _ in 0..credits {
            let Some(completion) = self.control.try_completion() else {
                break;
            };
            if let Some(staging) = self
                .staging
                .as_mut()
                .filter(|s| s.matches(completion.sequence))
            {
                if let Err(completion) = staging.completion(completion) {
                    self.quarantine = Some(completion);
                    return Err(Error::InvalidState);
                }
                continue;
            }
            if let Some(exports) = self
                .exports
                .as_mut()
                .filter(|exports| exports.matches(completion.sequence))
            {
                if let Err(completion) = exports.completion(completion) {
                    self.quarantine = Some(completion);
                    return Err(Error::InvalidState);
                }
                continue;
            }
            if let Some(graphs) = self
                .graphs
                .as_mut()
                .filter(|graphs| graphs.matches(completion.sequence))
            {
                if let Err(completion) = graphs.completion(completion) {
                    self.quarantine = Some(completion);
                    return Err(Error::InvalidState);
                }
                continue;
            }
            let index = self.slots.iter().position(|slot| {
                slot.as_ref().is_some_and(|slot| {
                    slot.ticket
                        .as_ref()
                        .is_some_and(|ticket| ticket.sequence() == completion.sequence)
                })
            });
            let supported = completion.retired.is_none()
                && matches!(&completion.request, Command::Schedule { payload, .. } if payload.is_empty())
                && completion.outcome != Outcome::Applied;
            let Some(index) = index.filter(|_| supported) else {
                self.quarantine = Some(completion);
                return Err(Error::InvalidState);
            };
            let slot = self.slots[index].as_mut().unwrap();
            if slot.admission.is_some() || slot.admission_sent {
                self.quarantine = Some(completion);
                return Err(Error::InvalidState);
            }
            let mut response = slot.response(Phase::Admission, completion.frame.0);
            match completion.outcome {
                Outcome::Scheduled {
                    effective_frame, ..
                } => {
                    response.status = Status::Admitted;
                    slot.effective = effective_frame.0;
                    response.effective = slot.effective;
                }
                Outcome::Cancelled => response.status = Status::Cancelled,
                Outcome::Rejected(error) => response.error = wire::runtime_error(error),
                Outcome::Applied => unreachable!(),
            }
            slot.admission = Some(response);
            // Empty payloads own no allocation; cancellation flags retain their
            // prepared base owner and slot ticket. Unexpected owners are quarantined.
            drop(completion);
        }
        for _ in 0..credits {
            let Some(execution) = self.control.try_execution() else {
                break;
            };
            if let Some(staging) = self
                .staging
                .as_mut()
                .filter(|s| s.matches(execution.sequence))
            {
                staging.execution(execution)?;
                continue;
            }
            if let Some(exports) = self
                .exports
                .as_mut()
                .filter(|exports| exports.matches(execution.sequence))
            {
                exports.execution(execution)?;
                continue;
            }
            if let Some(graphs) = self
                .graphs
                .as_mut()
                .filter(|graphs| graphs.matches(execution.sequence))
            {
                graphs.execution(execution)?;
                continue;
            }
            let slot = self
                .slots
                .iter_mut()
                .flatten()
                .find(|slot| {
                    slot.ticket
                        .as_ref()
                        .is_some_and(|ticket| ticket.sequence() == execution.sequence)
                })
                .ok_or(Error::InvalidState)?;
            if slot.terminal.is_some() {
                return Err(Error::InvalidState);
            }
            let mut response = slot.response(Phase::Execution, execution.actual_frame.0);
            response.delivered = execution.delivered_frame.map(|frame| frame.0);
            match execution.result {
                Err(error) => response.error = wire::runtime_error(error),
                Ok(()) => match execution.operation_result {
                    Some(Reply::Transition(completion)) => match completion.outcome {
                        DomainOutcome::Applied { .. } => response.status = Status::Applied,
                        DomainOutcome::Superseded { .. } => response.status = Status::Superseded,
                        DomainOutcome::Cancelled { .. } => response.status = Status::Cancelled,
                        DomainOutcome::Failed { reason, .. } => {
                            response.status = Status::Failed;
                            response.error = wire::loop_error(reason);
                        }
                    },
                    Some(Reply::Failed(failure)) => {
                        response.status = Status::Failed;
                        response.error = match failure {
                            Failure::Timeline(error) => wire::loop_error(error),
                            Failure::Runtime(error) => 512 + wire::runtime_error(error),
                        };
                    }
                    _ => return Err(Error::InvalidState),
                },
            }
            slot.terminal = Some(response);
        }
        Ok(())
    }

    /// Return one fixed response. Admission always precedes execution for a slot.
    /// A slot remains occupied until its last response is consumed.
    pub fn next_response(&mut self) -> Result<Option<[u8; wire::RESPONSE_BYTES]>, WireError> {
        crate::guarded(|| {
            for index in 0..SLOTS {
                let Some(slot) = self.slots[index].as_mut() else {
                    continue;
                };
                if let Some(response) = slot.admission {
                    let bytes = response.encode()?;
                    if response.status == Status::Admitted {
                        slot.admission = None;
                        slot.admission_sent = true;
                    } else {
                        self.slots[index] = None;
                    }
                    return Ok(Some(bytes));
                }
                if slot.admission_sent {
                    if let Some(mut response) = slot.terminal {
                        response.effective = slot.effective;
                        let bytes = response.encode()?;
                        self.slots[index] = None;
                        return Ok(Some(bytes));
                    }
                }
            }
            Ok(None)
        })
    }

    /// Off-render stopped extraction. Pending wire results are abandoned, not
    /// fabricated as success; explicit stopped cleanup owns the remaining media.
    pub fn into_stopped(self) -> StoppedLoops {
        self.engine.into_stopped()
    }
}
