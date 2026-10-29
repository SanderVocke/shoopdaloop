use crate::{
    graph_transfer::{self, Counts, Id, Receiver},
    graph_wire::{self, Bank, Operation, Request, Response, Status, CREDITS},
    scenario::Mix,
    wire::{self, Error as WireError, Phase},
};
use rt_runtime::{
    executor::{Command, Retired},
    schedule::{Action, Execution},
    transport::{Completion, Lane, Outcome, Ticket},
    Epoch, Error, Frame, PreparedConfig, Sequence,
};
use shoop_loops::runtime::{
    Controller, Failure, LoopPlanner, Node, Operation as DomainOperation, Reply,
};

type Returned = Completion<Command<Node<Mix>, LoopPlanner>, Retired<Node<Mix>>>;
struct Slot {
    request: Request,
    ticket: Option<Ticket>,
    admission: Option<Response>,
    terminal: Option<Response>,
    sent: bool,
    effective: u64,
    capacity: Option<Counts>,
}
impl Slot {
    fn response(&self, phase: Phase, observed: u64) -> Response {
        Response {
            session: self.request.session,
            sequence: self.request.sequence,
            slot: self.request.slot,
            kind: self.request.operation.kind(),
            phase,
            status: Status::Rejected,
            runtime_sequence: self.ticket.as_ref().map_or(0, |t| t.sequence().0),
            requested: self.request.operation.at(),
            effective: self.effective,
            delivered: None,
            observed,
            error: 0,
            graph: None,
            graph_epoch: Epoch(0),
            topology_epoch: Epoch(0),
            bank: None,
        }
    }
}
#[derive(Clone, Copy)]
struct Lease {
    bank: Id,
    graph: rt_runtime::graph::activation::StageId,
}
pub(crate) struct Graphs {
    pool: Receiver,
    session: u64,
    last_sequence: u64,
    slots: [Option<Slot>; CREDITS],
    leases: [Option<Lease>; graph_transfer::BANKS],
    orphan: Option<Command<Node<Mix>, LoopPlanner>>,
    pub(crate) fatal: bool,
}
impl Graphs {
    pub(crate) fn new(session: u64, config: PreparedConfig) -> Result<Self, Error> {
        Ok(Self {
            pool: Receiver::new(session, config)?,
            session,
            last_sequence: 0,
            slots: std::array::from_fn(|_| None),
            leases: [None; graph_transfer::BANKS],
            orphan: None,
            fatal: false,
        })
    }
    pub(crate) fn payload(&mut self, bytes: &[u8]) -> Result<graph_transfer::Phase, Error> {
        self.pool.ingest(bytes)
    }
    pub(crate) fn outstanding(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }
    pub(crate) fn retained(&self) -> usize {
        self.leases.iter().filter(|l| l.is_some()).count()
    }
    pub(crate) fn matches(&self, sequence: Sequence) -> bool {
        self.index(sequence).is_some()
    }
    fn index(&self, sequence: Sequence) -> Option<usize> {
        self.slots.iter().position(|s| {
            s.as_ref()
                .is_some_and(|s| s.ticket.as_ref().is_some_and(|t| t.sequence() == sequence))
        })
    }
    fn returned_bank(&self, original: Id) -> Bank {
        let id = self.pool.id(original.slot as usize).unwrap_or(Id {
            slot: original.slot,
            generation: 0,
        });
        let capacity = self.pool.capacities(id).unwrap_or(Counts {
            steps: 0,
            edges: 0,
            bindings: 0,
        });
        Bank { id, capacity }
    }
    pub(crate) fn ingest(
        &mut self,
        control: &mut Controller<Mix>,
        bytes: &[u8],
        now: u64,
    ) -> Result<(), WireError> {
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
        let capacity = if let Operation::Stage { bank, epoch, .. } = request.operation {
            if self.pool.phase(bank).map_err(|_| WireError::Identity)?
                != graph_transfer::Phase::Sealed
            {
                return Err(WireError::State);
            }
            if self
                .pool
                .expected_epoch(bank)
                .map_err(|_| WireError::Identity)?
                != epoch
            {
                return Err(WireError::State);
            }
            Some(self.pool.capacities(bank).map_err(|_| WireError::State)?)
        } else {
            None
        };
        let mut pending = Slot {
            request,
            ticket: None,
            admission: None,
            terminal: None,
            sent: false,
            effective: request.operation.at(),
            capacity,
        };
        let submitted = match request.operation {
            Operation::Stage { bank, slot, epoch } => {
                let loan = self.pool.take(bank).map_err(|_| WireError::State)?;
                control.stage_graph(slot as usize, epoch, loan.plan)
            }
            Operation::Activate {
                graph,
                at,
                graph_epoch,
                topology_epoch,
            } => control.submit(
                Lane::Normal,
                Frame(at),
                DomainOperation::GraphActivate {
                    id: graph,
                    graph_epoch,
                    topology_epoch,
                },
            ),
            Operation::Release { graph } => control.release_graph(graph),
        };
        match submitted {
            Ok(ticket) => {
                if request.cancelled {
                    ticket.cancel();
                }
                pending.ticket = Some(ticket);
            }
            Err(refusal) => {
                let mut response = pending.response(Phase::Admission, now);
                response.error = wire::runtime_error(refusal.error);
                let mut command = refusal.value;
                if let Operation::Stage { bank, .. } = request.operation {
                    let Command::StageGraph { plan, .. } = &mut command else {
                        self.orphan = Some(command);
                        self.fatal = true;
                        return Err(WireError::State);
                    };
                    let Some(owner) = plan.take() else {
                        self.orphan = Some(command);
                        self.fatal = true;
                        return Err(WireError::State);
                    };
                    if let Err((_, owner)) = self.pool.recycle(bank, owner) {
                        *plan = Some(owner);
                        self.orphan = Some(command);
                        self.fatal = true;
                        return Err(WireError::State);
                    }
                    response.bank = Some(self.returned_bank(bank));
                }
                pending.admission = Some(response);
                // Controlled Stage/Release/empty Schedule shapes contain no remaining owners.
                drop(command);
            }
        }
        self.last_sequence = request.sequence;
        self.slots[request.slot] = Some(pending);
        Ok(())
    }
    /// Unexpected owning completions return intact to the outer quarantine.
    pub(crate) fn completion(&mut self, mut completion: Returned) -> Result<(), Returned> {
        let Some(index) = self.index(completion.sequence) else {
            return Err(completion);
        };
        let pending = self.slots[index].as_ref().unwrap();
        if pending.admission.is_some() || pending.sent {
            return Err(completion);
        }
        let mut response = pending.response(Phase::Admission, completion.frame.0);
        let operation = pending.request.operation;
        let shape = match (operation, &completion.request) {
            (
                Operation::Stage { slot, epoch, .. },
                Command::StageGraph {
                    slot: actual,
                    expected_epoch,
                    ..
                },
            ) => {
                slot as usize == *actual && epoch == *expected_epoch && completion.retired.is_none()
            }
            (Operation::Release { graph }, Command::ReleaseGraph { id }) => graph == *id,
            (
                Operation::Activate {
                    graph,
                    at,
                    graph_epoch,
                    topology_epoch,
                },
                Command::Schedule {
                    at: actual,
                    action:
                        Action::Planner {
                            operation:
                                DomainOperation::GraphActivate {
                                    id,
                                    graph_epoch: ge,
                                    topology_epoch: te,
                                },
                            ..
                        },
                    payload,
                },
            ) => {
                at == actual.0
                    && graph == *id
                    && graph_epoch == *ge
                    && topology_epoch == *te
                    && payload.is_empty()
                    && completion.retired.is_none()
            }
            _ => false,
        };
        if !shape {
            return Err(completion);
        }
        match (operation, completion.outcome) {
            (Operation::Stage { bank, slot, epoch }, Outcome::Applied) => {
                let Command::StageGraph {
                    id: Some(id),
                    plan: None,
                    ..
                } = &completion.request
                else {
                    return Err(completion);
                };
                if id.parts()[1] != slot || self.leases[bank.slot as usize].is_some() {
                    return Err(completion);
                }
                response.status = Status::Staged;
                response.graph = Some(*id);
                response.graph_epoch = epoch;
                response.bank = Some(Bank {
                    id: bank,
                    capacity: pending.capacity.unwrap(),
                });
                self.leases[bank.slot as usize] = Some(Lease { bank, graph: *id });
            }
            (Operation::Stage { bank, .. }, Outcome::Cancelled | Outcome::Rejected(_)) => {
                let Command::StageGraph {
                    id: None,
                    plan: Some(_),
                    ..
                } = &completion.request
                else {
                    return Err(completion);
                };
                let Command::StageGraph { plan, .. } = &mut completion.request else {
                    unreachable!()
                };
                let owner = plan.take().unwrap();
                if let Err((_, owner)) = self.pool.recycle(bank, owner) {
                    *plan = Some(owner);
                    return Err(completion);
                }
                response.bank = Some(self.returned_bank(bank));
                match completion.outcome {
                    Outcome::Cancelled => response.status = Status::Cancelled,
                    Outcome::Rejected(e) => response.error = wire::runtime_error(e),
                    _ => unreachable!(),
                }
            }
            (Operation::Release { graph }, Outcome::Applied) => {
                let Some(lease_index) = self
                    .leases
                    .iter()
                    .position(|l| l.is_some_and(|l| l.graph == graph))
                else {
                    return Err(completion);
                };
                let Some(Retired::Plan(_)) = &completion.retired else {
                    return Err(completion);
                };
                let lease = self.leases[lease_index].unwrap();
                let Some(Retired::Plan(owner)) = completion.retired.take() else {
                    unreachable!()
                };
                if let Err((_, owner)) = self.pool.recycle(lease.bank, owner) {
                    completion.retired = Some(Retired::Plan(owner));
                    return Err(completion);
                }
                self.leases[lease_index] = None;
                response.status = Status::Released;
                response.graph = Some(graph);
                response.bank = Some(self.returned_bank(lease.bank));
            }
            (Operation::Release { .. }, Outcome::Cancelled | Outcome::Rejected(_)) => {
                if completion.retired.is_some() {
                    return Err(completion);
                }
                match completion.outcome {
                    Outcome::Cancelled => response.status = Status::Cancelled,
                    Outcome::Rejected(e) => response.error = wire::runtime_error(e),
                    _ => unreachable!(),
                }
            }
            (
                Operation::Activate { graph, .. },
                Outcome::Scheduled {
                    effective_frame, ..
                },
            ) => {
                response.status = Status::Admitted;
                response.graph = Some(graph);
                response.effective = effective_frame.0;
            }
            (Operation::Activate { .. }, Outcome::Cancelled) => response.status = Status::Cancelled,
            (Operation::Activate { .. }, Outcome::Rejected(e)) => {
                response.error = wire::runtime_error(e)
            }
            _ => return Err(completion),
        }
        self.slots[index].as_mut().unwrap().effective = response.effective;
        self.slots[index].as_mut().unwrap().admission = Some(response);
        // Any owning plan has moved into a prepared receiver bank or runtime stage.
        drop(completion);
        Ok(())
    }
    pub(crate) fn execution(
        &mut self,
        execution: Execution<DomainOperation, Reply>,
    ) -> Result<(), Error> {
        let index = self.index(execution.sequence).ok_or(Error::InvalidState)?;
        let pending = self.slots[index].as_mut().unwrap();
        let Operation::Activate {
            graph,
            graph_epoch,
            topology_epoch,
            ..
        } = pending.request.operation
        else {
            return Err(Error::InvalidState);
        };
        if pending.terminal.is_some() {
            return Err(Error::InvalidState);
        }
        let mut response = pending.response(Phase::Execution, execution.actual_frame.0);
        response.delivered = execution.delivered_frame.map(|at| at.0);
        match execution.result {
            Err(e) => response.error = wire::runtime_error(e),
            Ok(()) => match execution.operation_result {
                Some(Reply::GraphActivated {
                    id,
                    graph_epoch: ge,
                    topology_epoch: te,
                }) if id == graph
                    && graph_epoch.0.checked_add(1) == Some(ge.0)
                    && topology_epoch == te =>
                {
                    response.status = Status::Activated;
                    response.graph = Some(id);
                    response.graph_epoch = ge;
                    response.topology_epoch = te;
                }
                Some(Reply::Failed(e)) => {
                    response.status = Status::Failed;
                    response.error = match e {
                        Failure::Timeline(e) => wire::loop_error(e),
                        Failure::Runtime(e) => 512 + wire::runtime_error(e),
                    };
                }
                _ => return Err(Error::InvalidState),
            },
        }
        pending.terminal = Some(response);
        Ok(())
    }
    pub(crate) fn cancel(&self, slot: usize, sequence: u64) -> Result<(), WireError> {
        let pending = self
            .slots
            .get(slot)
            .and_then(Option::as_ref)
            .ok_or(WireError::Slot)?;
        if pending.request.sequence != sequence {
            return Err(WireError::Sequence);
        }
        pending.ticket.as_ref().ok_or(WireError::State)?.cancel();
        Ok(())
    }
    pub(crate) fn next_response(
        &mut self,
    ) -> Result<Option<[u8; graph_wire::RESPONSE_BYTES]>, WireError> {
        for index in 0..CREDITS {
            let Some(pending) = self.slots[index].as_mut() else {
                continue;
            };
            if let Some(response) = pending.admission {
                let bytes = response.encode()?;
                if response.status == Status::Admitted {
                    pending.admission = None;
                    pending.sent = true;
                } else {
                    self.slots[index] = None;
                }
                return Ok(Some(bytes));
            }
            if pending.sent {
                if let Some(mut response) = pending.terminal {
                    response.effective = pending.effective;
                    let bytes = response.encode()?;
                    self.slots[index] = None;
                    return Ok(Some(bytes));
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
