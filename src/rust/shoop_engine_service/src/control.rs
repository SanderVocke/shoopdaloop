use crate::{
    dsp, Delivery, DeliveryPhase, Execution, ExecutionStatus, LoopExecution, PreparedRoute,
    Request, RequestId, Work, BULK_SLOTS, DOMAIN_SLOTS, INTERNAL_SLOT, NORMAL_SLOTS, USER_SLOTS,
};
use rt_runtime::{
    executor::{self, Command},
    resource::{self, Cursor},
    transport::{self, Lane, Outcome, SubmitError},
    Epoch, Error, Frame, Sequence,
};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use shoop_loops::{
    runtime::{LoopClient, LoopPlanner, Operation},
    LoopId, Timing, Transition,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Clone, Debug)]
pub struct Ticket {
    id: RequestId,
    flag: Arc<AtomicBool>,
    identity: Arc<()>,
}
impl Ticket {
    pub fn id(&self) -> RequestId {
        self.id
    }
    /// Best effort until service dequeue. Explicit recovery handles forwarded work.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Class {
    Domain,
    Bulk,
    Recovery,
}
#[derive(Clone, Copy)]
struct Credit {
    id: RequestId,
    class: Class,
    delivery: bool,
    execution: bool,
}
struct Packet {
    id: RequestId,
    lane: Lane,
    request: Request,
    flag: Arc<AtomicBool>,
}
struct Returned {
    delivery: Delivery,
    _flag: Option<Arc<AtomicBool>>,
}
struct Executed {
    execution: Execution,
    _flag: Option<Arc<AtomicBool>>,
}

pub struct Client {
    normal: Producer<Packet>,
    recovery: Producer<Packet>,
    returns: Consumer<Returned>,
    executions: Consumer<Executed>,
    credits: [Option<Credit>; USER_SLOTS],
    flags: Box<[Arc<AtomicBool>]>,
    identity: Arc<()>,
    next: u64,
    dsp: dsp::Client,
    meters: dsp::Meters,
}

impl Client {
    pub fn connected(&self) -> bool {
        !self.normal.is_abandoned()
    }
    pub fn outstanding(&self) -> usize {
        self.credits.iter().flatten().count()
    }
    pub fn meters(&self) -> &dsp::Meters {
        &self.meters
    }
    pub fn dsp(&mut self) -> &mut dsp::Client {
        &mut self.dsp
    }
    pub(crate) fn identity(&self) -> Arc<()> {
        Arc::clone(&self.identity)
    }
    fn submit(&mut self, lane: Lane, request: Request) -> Result<Ticket, SubmitError<Request>> {
        let class = if lane == Lane::Recovery {
            Class::Recovery
        } else if matches!(request, Request::Loop { .. }) {
            Class::Domain
        } else {
            Class::Bulk
        };
        let fail = |error, value| SubmitError { error, value };
        if !self.connected() {
            return Err(fail(Error::Disconnected, request));
        }
        let limit = match class {
            Class::Domain => DOMAIN_SLOTS,
            Class::Bulk => BULK_SLOTS,
            Class::Recovery => 1,
        };
        if self
            .credits
            .iter()
            .flatten()
            .filter(|c| c.class == class)
            .count()
            >= limit
        {
            return Err(fail(Error::Full, request));
        }
        let range = if lane == Lane::Normal {
            0..NORMAL_SLOTS
        } else {
            NORMAL_SLOTS..USER_SLOTS
        };
        let slot = range
            .filter(|&i| self.credits[i].is_none())
            .find(|&i| Arc::strong_count(&self.flags[i]) == 1);
        let Some(slot) = slot else {
            return Err(fail(Error::Busy, request));
        };
        let Some(next) = self.next.checked_add(1) else {
            return Err(fail(Error::Capacity, request));
        };
        let id = RequestId(self.next);
        let flag = Arc::clone(&self.flags[slot]);
        flag.store(false, Ordering::Release);
        let packet = Packet {
            id,
            lane,
            request,
            flag: Arc::clone(&flag),
        };
        let result = match lane {
            Lane::Normal => self.normal.push(packet),
            Lane::Recovery => self.recovery.push(packet),
        };
        if let Err(PushError::Full(packet)) = result {
            return Err(fail(Error::Full, packet.request));
        }
        self.next = next;
        self.credits[slot] = Some(Credit {
            id,
            class,
            delivery: false,
            execution: false,
        });
        Ok(Ticket {
            id,
            flag,
            identity: Arc::clone(&self.identity),
        })
    }
    pub fn loop_at(
        &mut self,
        at: Frame,
        operation: Operation,
    ) -> Result<Ticket, SubmitError<Request>> {
        let request = Request::Loop { at, operation };
        if !matches!(
            operation,
            Operation::Transition { .. }
                | Operation::Select { .. }
                | Operation::Gain { .. }
                | Operation::State { .. }
                | Operation::Export { .. }
                | Operation::Release { .. }
                | Operation::Retire { .. }
                | Operation::Cursor { .. }
                | Operation::MidiInitial { .. }
                | Operation::GraphStatus { .. }
                | Operation::GraphActivate { .. }
        ) {
            return Err(SubmitError {
                error: Error::InvalidEvent,
                value: request,
            });
        }
        self.submit(Lane::Normal, request)
    }
    /// Recovery is stamped at the next service callback, never at a future frame.
    pub fn stop(&mut self) -> Result<Ticket, SubmitError<Request>> {
        self.submit(Lane::Recovery, Request::Stop)
    }
    pub fn cancel_pending(&mut self, ticket: &Ticket) -> Result<Ticket, SubmitError<Request>> {
        let request = Request::Cancel { target: ticket.id };
        if !Arc::ptr_eq(&ticket.identity, &self.identity)
            || !self.credits.iter().flatten().any(|c| c.id == ticket.id)
        {
            return Err(SubmitError {
                error: Error::InvalidId,
                value: request,
            });
        }
        let recovery = self.submit(Lane::Recovery, request)?;
        ticket.cancel();
        Ok(recovery)
    }
    pub fn read(
        &mut self,
        cursor: Cursor,
        bytes: Box<[u8]>,
    ) -> Result<Ticket, SubmitError<Request>> {
        self.submit(Lane::Normal, Request::Read { cursor, bytes })
    }
    pub fn stage_route(
        &mut self,
        route: PreparedRoute,
        expected_epoch: Epoch,
    ) -> Result<Ticket, SubmitError<Request>> {
        if !Arc::ptr_eq(&route.identity, &self.identity) {
            return Err(SubmitError {
                error: Error::InvalidId,
                value: Request::StageRoute {
                    route,
                    expected_epoch,
                },
            });
        }
        self.submit(
            Lane::Normal,
            Request::StageRoute {
                route,
                expected_epoch,
            },
        )
    }
    pub fn release_route(
        &mut self,
        id: rt_runtime::graph::activation::StageId,
    ) -> Result<Ticket, SubmitError<Request>> {
        self.submit(Lane::Normal, Request::ReleaseRoute { id })
    }
    fn observed(&mut self, id: RequestId, delivery: bool, execution: bool) {
        if let Some(slot) = self
            .credits
            .iter_mut()
            .find(|c| c.is_some_and(|c| c.id == id))
        {
            let credit = slot.as_mut().expect("occupied credit");
            credit.delivery |= delivery;
            credit.execution |= execution;
            if credit.delivery && credit.execution {
                *slot = None;
            }
        }
    }
    pub fn try_delivery(&mut self) -> Option<Delivery> {
        let returned = self.returns.pop().ok()?;
        self.observed(returned.delivery.id, true, returned.delivery.terminal);
        Some(returned.delivery)
    }
    pub fn try_execution(&mut self) -> Option<Execution> {
        let executed = self.executions.pop().ok()?;
        if let Some(id) = executed.execution.id {
            self.observed(id, false, true);
        }
        Some(executed.execution)
    }
}

struct Pending {
    id: Option<RequestId>,
    lane: Lane,
    sequence: Sequence,
    ticket: Option<transport::Ticket>,
    flag: Option<Arc<AtomicBool>>,
    domain: bool,
    delivered: bool,
    executed: bool,
}

pub(crate) struct Ingress {
    control: executor::Control<crate::ServiceNode, LoopPlanner>,
    loops: LoopClient,
    normal: Consumer<Packet>,
    recovery: Consumer<Packet>,
    returns: Producer<Returned>,
    executions: Producer<Executed>,
    pending: Box<[Option<Pending>]>,
    identity: Arc<()>,
    owner_spill: Option<Returned>,
    execution_spill: Option<Executed>,
    internal_owner: Option<Delivery>,
    pub(crate) disconnected: bool,
    pub(crate) stop_applied: bool,
    pub(crate) stop_failed: bool,
    target: LoopId,
    pub(crate) work: Work,
}

pub(crate) fn channel(
    control: executor::Control<crate::ServiceNode, LoopPlanner>,
    loops: LoopClient,
    target: LoopId,
    dsp: dsp::Client,
    meters: dsp::Meters,
) -> (Client, Ingress) {
    let (normal_tx, normal_rx) = RingBuffer::new(NORMAL_SLOTS);
    let (recovery_tx, recovery_rx) = RingBuffer::new(1);
    let (returns_tx, returns_rx) = RingBuffer::new(USER_SLOTS);
    let (executions_tx, executions_rx) = RingBuffer::new(DOMAIN_SLOTS + 2);
    let identity = Arc::new(());
    let flags = (0..USER_SLOTS)
        .map(|_| Arc::new(AtomicBool::new(false)))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let pending = std::iter::repeat_with(|| None)
        .take(USER_SLOTS + 1)
        .collect::<Vec<_>>()
        .into_boxed_slice();
    (
        Client {
            normal: normal_tx,
            recovery: recovery_tx,
            returns: returns_rx,
            executions: executions_rx,
            credits: [None; USER_SLOTS],
            flags,
            identity: Arc::clone(&identity),
            next: 1,
            dsp,
            meters,
        },
        Ingress {
            control,
            loops,
            normal: normal_rx,
            recovery: recovery_rx,
            returns: returns_tx,
            executions: executions_tx,
            pending,
            identity,
            owner_spill: None,
            execution_spill: None,
            internal_owner: None,
            disconnected: false,
            stop_applied: false,
            stop_failed: false,
            target,
            work: Work::default(),
        },
    )
}

impl Ingress {
    fn publish_owner(&mut self, value: Returned) -> Result<(), Error> {
        match self.returns.push(value) {
            Ok(()) => {
                self.work.owning_returns += 1;
                Ok(())
            }
            Err(PushError::Full(value)) => {
                self.owner_spill = Some(value);
                Err(Error::Full)
            }
        }
    }
    fn publish_execution(&mut self, value: Executed) -> Result<(), Error> {
        match self.executions.push(value) {
            Ok(()) => {
                self.work.executions += 1;
                Ok(())
            }
            Err(PushError::Full(value)) => {
                self.execution_spill = Some(value);
                Err(Error::Full)
            }
        }
    }
    fn native_return(
        &mut self,
        packet: Packet,
        frame: Frame,
        epoch: Epoch,
        phase: DeliveryPhase,
        outcome: Outcome,
    ) -> Result<(), Error> {
        self.publish_owner(Returned {
            delivery: Delivery {
                id: packet.id,
                lane: packet.lane,
                phase,
                frame,
                epoch,
                outcome,
                terminal: true,
                request: Some(packet.request),
                command: None,
                retired: None,
            },
            _flag: Some(packet.flag),
        })
    }
    fn start(&mut self, packet: Packet, frame: Frame, epoch: Epoch) -> Result<(), Error> {
        if self.disconnected {
            return self.native_return(
                packet,
                frame,
                epoch,
                DeliveryPhase::ForwardRefused,
                Outcome::Rejected(Error::Disconnected),
            );
        }
        if packet.flag.load(Ordering::Acquire) {
            return self.native_return(
                packet,
                frame,
                epoch,
                DeliveryPhase::CancelledBeforeForward,
                Outcome::Cancelled,
            );
        }
        let Packet {
            id,
            lane,
            request,
            flag,
        } = packet;
        let domain = matches!(
            request,
            Request::Loop { .. } | Request::Cancel { .. } | Request::Stop
        );
        let result = match request {
            Request::Loop { at, operation } => self.submit_loop(lane, at, operation),
            Request::Read { cursor, bytes } => self.control.submit_prepared(
                Lane::Normal,
                Command::resource(resource::Request::Read { cursor, bytes }),
            ),
            Request::StageRoute {
                route,
                expected_epoch,
            } => {
                if !Arc::ptr_eq(&route.identity, &self.identity) {
                    return self.native_return(
                        Packet {
                            id,
                            lane,
                            flag,
                            request: Request::StageRoute {
                                route,
                                expected_epoch,
                            },
                        },
                        frame,
                        epoch,
                        DeliveryPhase::ForwardRefused,
                        Outcome::Rejected(Error::InvalidId),
                    );
                }
                self.control.submit_prepared(
                    Lane::Normal,
                    Command::stage_graph(0, expected_epoch, route.plan),
                )
            }
            Request::ReleaseRoute { id } => self
                .control
                .submit_prepared(Lane::Normal, Command::ReleaseGraph { id }),
            Request::Stop => self.submit_loop(
                Lane::Recovery,
                frame,
                Operation::Transition {
                    target: self.target,
                    timing: Timing::At,
                    transition: Transition::Stop,
                },
            ),
            Request::Cancel { target } => {
                let sequence = self
                    .pending
                    .iter()
                    .flatten()
                    .find(|p| p.id == Some(target))
                    .map(|p| p.sequence);
                let Some(sequence) = sequence else {
                    return self.native_return(
                        Packet {
                            id,
                            lane,
                            flag,
                            request: Request::Cancel { target },
                        },
                        frame,
                        epoch,
                        DeliveryPhase::ServiceHandled,
                        Outcome::Applied,
                    );
                };
                self.submit_loop(Lane::Recovery, frame, Operation::Cancel { sequence })
            }
        };
        match result {
            Ok(ticket) => {
                let slot = self.pending[..USER_SLOTS]
                    .iter_mut()
                    .find(|p| p.is_none())
                    .ok_or(Error::Capacity)?;
                *slot = Some(Pending {
                    id: Some(id),
                    lane,
                    sequence: ticket.sequence(),
                    ticket: Some(ticket),
                    flag: Some(flag),
                    domain,
                    delivered: false,
                    executed: false,
                });
                self.work.forwarded += 1;
                Ok(())
            }
            Err(error) => self.publish_owner(Returned {
                delivery: Delivery {
                    id,
                    lane,
                    phase: DeliveryPhase::ForwardRefused,
                    frame,
                    epoch,
                    outcome: Outcome::Rejected(error.error),
                    terminal: true,
                    request: None,
                    command: Some(error.value),
                    retired: None,
                },
                _flag: Some(flag),
            }),
        }
    }
    fn submit_loop(
        &mut self,
        lane: Lane,
        at: Frame,
        operation: Operation,
    ) -> Result<transport::Ticket, SubmitError<crate::EngineCommand>> {
        self.control
            .submit_prepared(lane, self.loops.command(at, operation))
    }
    fn disconnect_stop(&mut self, frame: Frame) -> Result<(), Error> {
        if self.pending[INTERNAL_SLOT].is_some() || self.stop_applied || self.stop_failed {
            return Ok(());
        }
        match self.submit_loop(
            Lane::Recovery,
            frame,
            Operation::Transition {
                target: self.target,
                timing: Timing::At,
                transition: Transition::Stop,
            },
        ) {
            Ok(ticket) => {
                self.pending[INTERNAL_SLOT] = Some(Pending {
                    id: None,
                    lane: Lane::Recovery,
                    sequence: ticket.sequence(),
                    ticket: Some(ticket),
                    flag: None,
                    domain: true,
                    delivered: false,
                    executed: false,
                });
                self.work.forwarded += 1;
                Ok(())
            }
            // This locally constructed stop has only an empty payload, never an owning buffer/plan.
            Err(error) if matches!(error.error, Error::Full | Error::Busy) => Ok(()),
            Err(error) => Err(error.error),
        }
    }
    pub(crate) fn begin(&mut self, frame: Frame, epoch: Epoch) -> Result<bool, Error> {
        self.work = Work::default();
        if let Some(value) = self.owner_spill.take() {
            self.publish_owner(value)?;
        }
        if let Some(value) = self.execution_spill.take() {
            self.publish_execution(value)?;
        }
        let newly_disconnected = !self.disconnected && self.normal.is_abandoned();
        self.disconnected |= newly_disconnected;
        if self.disconnected {
            self.disconnect_stop(frame)?;
        }
        let recovery = self.recovery.slots().min(1);
        let normal = self.normal.slots().min(NORMAL_SLOTS);
        for _ in 0..recovery {
            if let Ok(packet) = self.recovery.pop() {
                self.start(packet, frame, epoch)?;
            }
        }
        for _ in 0..normal {
            if let Ok(packet) = self.normal.pop() {
                self.start(packet, frame, epoch)?;
            }
        }
        Ok(newly_disconnected)
    }
    fn reap(&mut self, slot: usize) {
        if self.pending[slot]
            .as_ref()
            .is_some_and(|p| p.delivered && p.executed)
        {
            self.pending[slot] = None;
        }
    }
    pub(crate) fn finish(&mut self) -> Result<(), Error> {
        for _ in 0..USER_SLOTS + 1 {
            let Some(completion) = self.control.try_completion() else {
                break;
            };
            let slot = self
                .pending
                .iter()
                .position(|p| {
                    p.as_ref()
                        .is_some_and(|p| p.sequence == completion.sequence)
                })
                .ok_or(Error::InvalidState)?;
            let pending = self.pending[slot].as_mut().ok_or(Error::InvalidState)?;
            let terminal = !pending.domain
                || matches!(
                    completion.outcome,
                    Outcome::Cancelled | Outcome::Rejected(_)
                );
            pending.delivered = true;
            pending.executed |= terminal;
            pending.ticket = None;
            let flag = if terminal { pending.flag.take() } else { None };
            let id = pending.id;
            // Move public owners, not the private transport cancellation flag, to the UI.
            // The local controller retains its prepared base flag while rendering.
            let delivery = Delivery {
                id: id.unwrap_or(RequestId(0)),
                lane: pending.lane,
                phase: DeliveryPhase::Runtime,
                frame: completion.frame,
                epoch: completion.epoch,
                outcome: completion.outcome,
                terminal,
                request: None,
                command: Some(completion.request),
                retired: completion.retired,
            };
            if id.is_some() {
                self.publish_owner(Returned {
                    delivery,
                    _flag: flag,
                })?;
            } else {
                self.stop_failed |= terminal;
                self.internal_owner = Some(delivery);
            }
            self.reap(slot);
        }
        for _ in 0..DOMAIN_SLOTS + 2 {
            let Some(execution) = self.control.try_execution() else {
                break;
            };
            self.executed(execution)?;
        }
        Ok(())
    }
    fn executed(&mut self, execution: LoopExecution) -> Result<(), Error> {
        let slot = self
            .pending
            .iter()
            .position(|p| p.as_ref().is_some_and(|p| p.sequence == execution.sequence))
            .ok_or(Error::InvalidState)?;
        let pending = self.pending[slot].as_mut().ok_or(Error::InvalidState)?;
        pending.executed = true;
        let id = pending.id;
        let flag = pending.flag.take();
        if id.is_none() {
            self.stop_applied = matches!(execution.operation_result,
                Some(shoop_loops::runtime::Reply::Transition(c)) if matches!(c.outcome, shoop_loops::Outcome::Applied {..}));
            self.stop_failed = !self.stop_applied;
        }
        self.publish_execution(Executed {
            execution: Execution {
                id,
                status: ExecutionStatus::Executed(execution),
            },
            _flag: flag,
        })?;
        self.reap(slot);
        Ok(())
    }
    pub(crate) fn abort(&mut self, frame: Frame, error: Error) -> Result<(), Error> {
        for slot in 0..USER_SLOTS {
            let Some(pending) = &mut self.pending[slot] else {
                continue;
            };
            if !pending.domain || pending.executed {
                continue;
            }
            pending.executed = true;
            let id = pending.id;
            let flag = pending.flag.take();
            self.publish_execution(Executed {
                execution: Execution {
                    id,
                    status: ExecutionStatus::Aborted {
                        decision_frame: frame,
                        error,
                    },
                },
                _flag: flag,
            })?;
            self.reap(slot);
        }
        Ok(())
    }
}
