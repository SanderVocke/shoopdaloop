//! Prepared descriptor payloads. This is not a transport/admission/result envelope.
use rt_runtime::{
    graph::{Connection, Endpoint, NodeStep, PortType, PreparedPlan},
    planner::{DecisionBinding, PlannerSpec, PlanningStep},
    Epoch, Error, NodeId, PlannerId, PreparedConfig,
};

pub const RECORD_BYTES: usize = 96;
pub const BANKS: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Id {
    pub slot: u32,
    pub generation: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Counts {
    pub steps: u32,
    pub edges: u32,
    pub bindings: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Payload {
    Begin {
        epoch: Epoch,
        counts: Counts,
        planned: bool,
    },
    Step {
        offset: u32,
        step: NodeStep,
    },
    Edge {
        offset: u32,
        edge: Connection,
    },
    Planning(PlanningStep),
    Binding {
        offset: u32,
        binding: DecisionBinding,
    },
    Seal,
    Abort,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Record {
    pub session: u64,
    pub sequence: u64,
    pub id: Id,
    pub payload: Payload,
}
fn get32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn get64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}
fn index(b: &[u8], at: usize) -> Result<usize, Error> {
    usize::try_from(get64(b, at)).map_err(|_| Error::Capacity)
}
fn put32(b: &mut [u8], at: usize, value: u32) {
    b[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(b: &mut [u8], at: usize, value: u64) {
    b[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn endpoint(b: &[u8], at: usize) -> Result<Endpoint, Error> {
    let slot = get32(b, at);
    let generation = get32(b, at + 4);
    Ok(Endpoint {
        node: if generation == 0 {
            if slot != 0 {
                return Err(Error::InvalidId);
            }
            None
        } else {
            Some(NodeId::from_parts(slot, generation)?)
        },
        kind: match b[at + 8] {
            0 => PortType::Audio,
            1 => PortType::Midi,
            _ => return Err(Error::PortType),
        },
        port: get32(b, at + 12),
    })
}
fn put_endpoint(b: &mut [u8], at: usize, endpoint: Endpoint) {
    if let Some(node) = endpoint.node {
        put32(b, at, node.slot());
        put32(b, at + 4, node.generation());
    }
    b[at + 8] = match endpoint.kind {
        PortType::Audio => 0,
        PortType::Midi => 1,
    };
    put32(b, at + 12, endpoint.port);
}
impl Record {
    pub fn encode(self) -> Result<[u8; RECORD_BYTES], Error> {
        if self.session == 0 || self.sequence == 0 || self.id.generation == 0 {
            return Err(Error::InvalidId);
        }
        let mut b = [0; RECORD_BYTES];
        b[..4].copy_from_slice(b"LGP1");
        b[4..6].copy_from_slice(&1u16.to_le_bytes());
        put64(&mut b, 8, self.session);
        put64(&mut b, 16, self.sequence);
        put32(&mut b, 24, self.id.slot);
        put32(&mut b, 28, self.id.generation);
        b[6] = match self.payload {
            Payload::Begin {
                epoch,
                counts,
                planned,
            } => {
                put64(&mut b, 32, epoch.0);
                put32(&mut b, 40, counts.steps);
                put32(&mut b, 44, counts.edges);
                put32(&mut b, 48, counts.bindings);
                b[52] = u8::from(planned);
                1
            }
            Payload::Step { offset, step } => {
                put32(&mut b, 32, offset);
                put32(&mut b, 36, step.node.slot());
                put32(&mut b, 40, step.node.generation());
                put64(&mut b, 44, step.input_end as u64);
                2
            }
            Payload::Edge { offset, edge } => {
                put32(&mut b, 32, offset);
                put_endpoint(&mut b, 36, edge.source);
                put_endpoint(&mut b, 52, edge.destination);
                3
            }
            Payload::Planning(step) => {
                put32(&mut b, 32, step.planner.id.slot());
                put32(&mut b, 36, step.planner.id.generation());
                for (at, value) in [
                    (40, step.planner.decision_slots),
                    (48, step.planner.observation_capacity),
                    (56, step.planner.operation_bytes),
                    (64, step.before_step),
                ] {
                    put64(&mut b, at, value as u64);
                }
                4
            }
            Payload::Binding { offset, binding } => {
                put32(&mut b, 32, offset);
                put32(&mut b, 36, binding.planner.slot());
                put32(&mut b, 40, binding.planner.generation());
                put32(&mut b, 44, binding.consumer.slot());
                put32(&mut b, 48, binding.consumer.generation());
                put64(&mut b, 52, binding.decision_slot as u64);
                5
            }
            Payload::Seal => 6,
            Payload::Abort => 7,
        };
        Ok(b)
    }
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.len() != RECORD_BYTES {
            return Err(Error::InvalidShape);
        }
        if &b[..4] != b"LGP1" || b[4..6] != 1u16.to_le_bytes() {
            return Err(Error::InvalidConfig);
        }
        let payload = match b[6] {
            1 if b[52] <= 1 => Payload::Begin {
                epoch: Epoch(get64(b, 32)),
                counts: Counts {
                    steps: get32(b, 40),
                    edges: get32(b, 44),
                    bindings: get32(b, 48),
                },
                planned: b[52] == 1,
            },
            2 => Payload::Step {
                offset: get32(b, 32),
                step: NodeStep {
                    node: NodeId::from_parts(get32(b, 36), get32(b, 40))?,
                    input_end: index(b, 44)?,
                },
            },
            3 => Payload::Edge {
                offset: get32(b, 32),
                edge: Connection {
                    source: endpoint(b, 36)?,
                    destination: endpoint(b, 52)?,
                },
            },
            4 => Payload::Planning(PlanningStep {
                planner: PlannerSpec {
                    id: PlannerId::from_parts(get32(b, 32), get32(b, 36))?,
                    decision_slots: index(b, 40)?,
                    observation_capacity: index(b, 48)?,
                    operation_bytes: index(b, 56)?,
                },
                before_step: index(b, 64)?,
            }),
            5 => Payload::Binding {
                offset: get32(b, 32),
                binding: DecisionBinding {
                    planner: PlannerId::from_parts(get32(b, 36), get32(b, 40))?,
                    consumer: NodeId::from_parts(get32(b, 44), get32(b, 48))?,
                    decision_slot: index(b, 52)?,
                },
            },
            6 => Payload::Seal,
            7 => Payload::Abort,
            _ => return Err(Error::InvalidEvent),
        };
        let record = Self {
            session: get64(b, 8),
            sequence: get64(b, 16),
            id: Id {
                slot: get32(b, 24),
                generation: get32(b, 28),
            },
            payload,
        };
        if record.encode()? != b {
            return Err(Error::InvalidShape);
        }
        Ok(record)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Idle,
    Receiving,
    Sealed,
    Loaned,
}
struct Bank {
    generation: u32,
    phase: Phase,
    owner: Option<PreparedPlan>,
    epoch: Epoch,
    counts: Counts,
    planned: bool,
}
/// An owning handoff local to one receiver. Neither this value nor its plan goes on wire.
pub struct Loan {
    pub id: Id,
    pub epoch: Epoch,
    pub plan: PreparedPlan,
}
/// Prepared bounded payload receiver. Successful Seal is not runtime validation/admission.
pub struct Receiver {
    session: u64,
    last_sequence: u64,
    config: PreparedConfig,
    plan_bytes: usize,
    banks: [Bank; BANKS],
}
impl Receiver {
    pub fn new(session: u64, config: PreparedConfig) -> Result<Self, Error> {
        if session == 0 {
            return Err(Error::InvalidId);
        }
        let first = PreparedPlan::with_capacity(config)?;
        let plan_bytes = first.storage_bytes()?;
        if plan_bytes
            .checked_mul(BANKS)
            .and_then(|n| n.checked_add(size_of::<Self>()))
            .is_none_or(|n| n > i32::MAX as usize)
        {
            return Err(Error::Capacity);
        }
        let mut owners = [Some(first), Some(PreparedPlan::with_capacity(config)?)];
        Ok(Self {
            session,
            last_sequence: 0,
            config,
            plan_bytes,
            banks: std::array::from_fn(|index| Bank {
                generation: 1,
                phase: Phase::Idle,
                owner: owners[index].take(),
                epoch: Epoch(0),
                counts: Counts {
                    steps: 0,
                    edges: 0,
                    bindings: 0,
                },
                planned: false,
            }),
        })
    }
    pub fn id(&self, slot: usize) -> Result<Id, Error> {
        let bank = self.banks.get(slot).ok_or(Error::InvalidId)?;
        if bank.generation == 0 {
            return Err(Error::Exhausted);
        }
        Ok(Id {
            slot: slot as u32,
            generation: bank.generation,
        })
    }
    fn bank(&self, id: Id) -> Result<&Bank, Error> {
        self.banks
            .get(id.slot as usize)
            .filter(|b| id.generation != 0 && b.generation == id.generation)
            .ok_or(Error::InvalidId)
    }
    pub fn phase(&self, id: Id) -> Result<Phase, Error> {
        Ok(self.bank(id)?.phase)
    }
    pub fn expected_epoch(&self, id: Id) -> Result<Epoch, Error> {
        Ok(self.bank(id)?.epoch)
    }
    /// Borrowed receiver-local state. It cannot outlive a mutation or own a runtime graph.
    pub fn plan(&self, id: Id) -> Result<&PreparedPlan, Error> {
        self.bank(id)?.owner.as_ref().ok_or(Error::Busy)
    }
    pub fn capacities(&self, id: Id) -> Result<Counts, Error> {
        let plan = self.plan(id)?;
        let limits = self.config.limits();
        Ok(Counts {
            steps: plan.descriptor().steps.capacity().min(limits.max_nodes) as u32,
            edges: plan
                .descriptor()
                .connections
                .capacity()
                .min(limits.max_edges) as u32,
            bindings: plan.planning().bindings.capacity().min(limits.max_nodes) as u32,
        })
    }
    pub fn last_sequence(&self) -> u64 {
        self.last_sequence
    }
    /// One finite external record per call. Every rejection leaves all banks and watermark unchanged.
    pub fn ingest(&mut self, bytes: &[u8]) -> Result<Phase, Error> {
        let record = Record::decode(bytes)?;
        if record.session != self.session {
            return Err(Error::InvalidId);
        }
        if record.sequence <= self.last_sequence {
            return Err(Error::InvalidState);
        }
        let bank = self.bank(record.id)?;
        match record.payload {
            Payload::Begin {
                counts, planned, ..
            } => {
                if bank.phase != Phase::Idle {
                    return Err(Error::Busy);
                }
                let capacity = self.capacities(record.id)?;
                if counts.steps > capacity.steps
                    || counts.edges > capacity.edges
                    || counts.bindings > capacity.bindings
                {
                    return Err(Error::Capacity);
                }
                if !planned && counts.bindings != 0 {
                    return Err(Error::InvalidShape);
                }
            }
            Payload::Abort => {
                if !matches!(bank.phase, Phase::Receiving | Phase::Sealed) {
                    return Err(Error::Busy);
                }
            }
            payload => {
                if bank.phase != Phase::Receiving {
                    return Err(Error::Busy);
                }
                let plan = bank.owner.as_ref().ok_or(Error::InvalidState)?;
                let steps = plan.descriptor().steps.len();
                let edges = plan.descriptor().connections.len();
                let bindings = plan.planning().bindings.len();
                let complete_edges =
                    steps == bank.counts.steps as usize && edges == bank.counts.edges as usize;
                let valid = match payload {
                    Payload::Step { offset, .. } => {
                        offset as usize == steps && steps < bank.counts.steps as usize
                    }
                    Payload::Edge { offset, .. } => {
                        steps == bank.counts.steps as usize
                            && offset as usize == edges
                            && edges < bank.counts.edges as usize
                    }
                    Payload::Planning(_) => {
                        complete_edges && bank.planned && plan.planning().step.is_none()
                    }
                    Payload::Binding { offset, .. } => {
                        complete_edges
                            && plan.planning().step.is_some()
                            && offset as usize == bindings
                            && bindings < bank.counts.bindings as usize
                    }
                    Payload::Seal => {
                        complete_edges
                            && bindings == bank.counts.bindings as usize
                            && plan.planning().step.is_some() == bank.planned
                    }
                    _ => false,
                };
                if !valid {
                    return Err(Error::InvalidOffset);
                }
            }
        }
        let bank = &mut self.banks[record.id.slot as usize];
        match record.payload {
            Payload::Begin {
                epoch,
                counts,
                planned,
            } => {
                bank.owner.as_mut().unwrap().begin_descriptor()?;
                bank.epoch = epoch;
                bank.counts = counts;
                bank.planned = planned;
                bank.phase = Phase::Receiving;
            }
            Payload::Step { step, .. } => bank.owner.as_mut().unwrap().push_step(step)?,
            Payload::Edge { edge, .. } => bank.owner.as_mut().unwrap().push_connection(edge)?,
            Payload::Planning(step) => bank.owner.as_mut().unwrap().set_planning_step(Some(step)),
            Payload::Binding { binding, .. } => {
                bank.owner.as_mut().unwrap().push_binding(binding)?
            }
            Payload::Seal => bank.phase = Phase::Sealed,
            Payload::Abort => {
                bank.generation = bank.generation.checked_add(1).unwrap_or(0);
                bank.phase = Phase::Idle;
            }
        }
        self.last_sequence = record.sequence;
        Ok(bank.phase)
    }
    pub fn take(&mut self, id: Id) -> Result<Loan, Error> {
        if self.bank(id)?.phase != Phase::Sealed {
            return Err(Error::Busy);
        }
        let bank = &mut self.banks[id.slot as usize];
        bank.phase = Phase::Loaned;
        Ok(Loan {
            id,
            epoch: bank.epoch,
            plan: bank.owner.take().unwrap(),
        })
    }
    /// Rejected/cancelled candidates or runtime-retired plans return here. Invalid
    /// returns retain their owning input for the caller to quarantine off-render.
    /// A retired plan may have smaller capacities; Begin checks the actual next owner.
    pub fn recycle(&mut self, id: Id, plan: PreparedPlan) -> Result<(), (Error, PreparedPlan)> {
        let error = match self.bank(id) {
            Err(error) => Some(error),
            Ok(bank) if bank.phase != Phase::Loaned => Some(Error::Busy),
            Ok(_) if plan.config() != self.config => Some(Error::InvalidConfig),
            Ok(_) => match plan.storage_bytes() {
                Ok(bytes) if bytes <= self.plan_bytes => None,
                Ok(_) => Some(Error::Capacity),
                Err(error) => Some(error),
            },
        };
        if let Some(error) = error {
            return Err((error, plan));
        }
        let bank = &mut self.banks[id.slot as usize];
        bank.owner = Some(plan);
        bank.phase = Phase::Idle;
        bank.generation = bank.generation.checked_add(1).unwrap_or(0);
        Ok(())
    }
}

#[cfg(test)]
mod model;
#[cfg(test)]
pub(crate) mod tests;
