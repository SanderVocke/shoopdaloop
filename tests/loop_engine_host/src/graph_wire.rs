//! Graph command envelopes, separate from descriptor payload records and primitives.
use crate::{
    graph_transfer::{Counts, Id},
    wire::{Error, Phase},
};
use rt_runtime::{graph::activation::StageId, Epoch};

pub const CREDITS: usize = 2;
pub const REQUEST_BYTES: usize = 96;
pub const RESPONSE_BYTES: usize = 128;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    Stage {
        bank: Id,
        slot: u32,
        epoch: Epoch,
    },
    Activate {
        graph: StageId,
        at: u64,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
    },
    Release {
        graph: StageId,
    },
}
impl Operation {
    pub fn kind(self) -> u8 {
        match self {
            Self::Stage { .. } => 1,
            Self::Activate { .. } => 2,
            Self::Release { .. } => 3,
        }
    }
    pub fn at(self) -> u64 {
        match self {
            Self::Activate { at, .. } => at,
            _ => 0,
        }
    }
    pub fn graph(self) -> Option<StageId> {
        match self {
            Self::Activate { graph, .. } | Self::Release { graph } => Some(graph),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Request {
    pub session: u64,
    pub sequence: u64,
    pub slot: usize,
    pub cancelled: bool,
    pub operation: Operation,
}
fn get32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn get64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}
fn put32(b: &mut [u8], at: usize, value: u32) {
    b[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(b: &mut [u8], at: usize, value: u64) {
    b[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn graph(b: &[u8], at: usize) -> Result<StageId, Error> {
    StageId::from_parts(get32(b, at), get32(b, at + 4), get32(b, at + 8))
        .map_err(|_| Error::Identity)
}
fn put_graph(b: &mut [u8], at: usize, id: StageId) {
    for (i, n) in id.parts().into_iter().enumerate() {
        put32(b, at + i * 4, n);
    }
}
fn header(b: &[u8], size: usize, magic: &[u8]) -> Result<(), Error> {
    if b.len() != size {
        return Err(Error::Size);
    }
    if &b[..4] != magic {
        return Err(Error::Format);
    }
    if b[4..6] != 1u16.to_le_bytes() {
        return Err(Error::Version);
    }
    Ok(())
}
impl Request {
    pub fn encode(self) -> Result<[u8; REQUEST_BYTES], Error> {
        if self.session == 0 || self.sequence == 0 {
            return Err(Error::Identity);
        }
        if self.slot >= CREDITS {
            return Err(Error::Slot);
        }
        let mut b = [0; REQUEST_BYTES];
        b[..4].copy_from_slice(b"LLG1");
        b[4..6].copy_from_slice(&1u16.to_le_bytes());
        b[6] = self.operation.kind();
        b[7] = u8::from(self.cancelled);
        put64(&mut b, 8, self.session);
        put64(&mut b, 16, self.sequence);
        put32(&mut b, 24, self.slot as u32);
        match self.operation {
            Operation::Stage { bank, slot, epoch } => {
                if bank.generation == 0 {
                    return Err(Error::Identity);
                }
                put32(&mut b, 28, bank.slot);
                put32(&mut b, 32, bank.generation);
                put32(&mut b, 36, slot);
                put64(&mut b, 40, epoch.0);
            }
            Operation::Activate {
                graph,
                at,
                graph_epoch,
                topology_epoch,
            } => {
                put64(&mut b, 40, graph_epoch.0);
                put64(&mut b, 48, topology_epoch.0);
                put64(&mut b, 56, at);
                put_graph(&mut b, 64, graph);
            }
            Operation::Release { graph } => put_graph(&mut b, 64, graph),
        }
        Ok(b)
    }
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        header(b, REQUEST_BYTES, b"LLG1")?;
        if b[7] > 1 {
            return Err(Error::Reserved);
        }
        let operation = match b[6] {
            1 => Operation::Stage {
                bank: Id {
                    slot: get32(b, 28),
                    generation: get32(b, 32),
                },
                slot: get32(b, 36),
                epoch: Epoch(get64(b, 40)),
            },
            2 => Operation::Activate {
                graph: graph(b, 64)?,
                at: get64(b, 56),
                graph_epoch: Epoch(get64(b, 40)),
                topology_epoch: Epoch(get64(b, 48)),
            },
            3 => Operation::Release {
                graph: graph(b, 64)?,
            },
            _ => return Err(Error::Operation),
        };
        let request = Self {
            session: get64(b, 8),
            sequence: get64(b, 16),
            slot: get32(b, 24) as usize,
            cancelled: b[7] != 0,
            operation,
        };
        if request.encode()? != b {
            return Err(Error::Reserved);
        }
        Ok(request)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Status {
    Staged = 1,
    Released = 2,
    Admitted = 3,
    Activated = 4,
    Cancelled = 5,
    Rejected = 6,
    Failed = 7,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bank {
    pub id: Id,
    pub capacity: Counts,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Response {
    pub session: u64,
    pub sequence: u64,
    pub slot: usize,
    pub kind: u8,
    pub phase: Phase,
    pub status: Status,
    pub runtime_sequence: u64,
    pub requested: u64,
    pub effective: u64,
    pub delivered: Option<u64>,
    pub observed: u64,
    pub error: u32,
    pub graph: Option<StageId>,
    pub graph_epoch: Epoch,
    pub topology_epoch: Epoch,
    pub bank: Option<Bank>,
}
impl Response {
    fn validate(self) -> Result<(), Error> {
        if self.session == 0 || self.sequence == 0 {
            return Err(Error::Identity);
        }
        if self.slot >= CREDITS {
            return Err(Error::Slot);
        }
        if !(1..=3).contains(&self.kind) {
            return Err(Error::Operation);
        }
        if self.kind != 2 && (self.requested != 0 || self.effective != 0) {
            return Err(Error::State);
        }
        if self.phase == Phase::Admission && self.delivered.is_some() {
            return Err(Error::State);
        }
        if self.phase == Phase::Execution
            && (self.kind != 2
                || self.runtime_sequence == 0
                || self.effective < self.requested
                || self
                    .delivered
                    .is_some_and(|at| at < self.effective || at > self.observed))
        {
            return Err(Error::State);
        }
        if !matches!(self.status, Status::Staged | Status::Activated)
            && (self.graph_epoch != Epoch(0) || self.topology_epoch != Epoch(0))
        {
            return Err(Error::State);
        }
        if self.bank.is_some_and(|bank| {
            bank.id.generation == 0
                && bank.capacity
                    != (Counts {
                        steps: 0,
                        edges: 0,
                        bindings: 0,
                    })
        }) {
            return Err(Error::State);
        }
        let valid = match self.status {
            Status::Staged => {
                self.phase == Phase::Admission
                    && self.kind == 1
                    && self.runtime_sequence != 0
                    && self.error == 0
                    && self.graph.is_some()
                    && self.bank.is_some_and(|b| b.id.generation != 0)
                    && self.topology_epoch == Epoch(0)
            }
            Status::Released => {
                self.phase == Phase::Admission
                    && self.kind == 3
                    && self.runtime_sequence != 0
                    && self.error == 0
                    && self.graph.is_some()
                    && self.bank.is_some()
            }
            Status::Admitted => {
                self.phase == Phase::Admission
                    && self.kind == 2
                    && self.runtime_sequence != 0
                    && self.effective >= self.requested
                    && self.error == 0
                    && self.graph.is_some()
                    && self.bank.is_none()
            }
            Status::Activated => {
                self.phase == Phase::Execution
                    && self.delivered.is_some()
                    && self.error == 0
                    && self.graph.is_some()
                    && self.bank.is_none()
            }
            Status::Cancelled => {
                self.runtime_sequence != 0
                    && self.error == 0
                    && self.graph.is_none()
                    && ((self.kind == 1) == self.bank.is_some())
            }
            Status::Rejected => {
                (1..=20).contains(&self.error)
                    && self.graph.is_none()
                    && ((self.kind == 1) == self.bank.is_some())
            }
            Status::Failed => {
                self.phase == Phase::Execution
                    && ((257..=268).contains(&self.error) || (513..=532).contains(&self.error))
                    && self.delivered.is_some()
                    && self.graph.is_none()
                    && self.bank.is_none()
            }
        };
        if valid {
            Ok(())
        } else {
            Err(Error::State)
        }
    }
    pub fn encode(self) -> Result<[u8; RESPONSE_BYTES], Error> {
        self.validate()?;
        let mut b = [0; RESPONSE_BYTES];
        b[..4].copy_from_slice(b"LLS1");
        b[4..6].copy_from_slice(&1u16.to_le_bytes());
        b[6] = self.phase as u8;
        b[7] = u8::from(self.delivered.is_some())
            | (u8::from(self.graph.is_some()) << 1)
            | (u8::from(self.bank.is_some()) << 2);
        put64(&mut b, 8, self.session);
        put64(&mut b, 16, self.sequence);
        put32(&mut b, 24, self.slot as u32);
        put32(&mut b, 28, self.status as u32);
        for (at, n) in [
            (32, self.runtime_sequence),
            (40, self.requested),
            (48, self.effective),
            (56, self.delivered.unwrap_or(0)),
            (64, self.observed),
            (88, self.graph_epoch.0),
            (96, self.topology_epoch.0),
        ] {
            put64(&mut b, at, n);
        }
        put32(&mut b, 72, self.error);
        if let Some(id) = self.graph {
            put_graph(&mut b, 76, id);
        }
        if let Some(bank) = self.bank {
            put32(&mut b, 104, bank.id.slot);
            put32(&mut b, 108, bank.id.generation);
            put32(&mut b, 112, bank.capacity.steps);
            put32(&mut b, 116, bank.capacity.edges);
            put32(&mut b, 120, bank.capacity.bindings);
        }
        b[124] = self.kind;
        Ok(b)
    }
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        header(b, RESPONSE_BYTES, b"LLS1")?;
        if b[7] & !7 != 0 {
            return Err(Error::Reserved);
        }
        let response = Self {
            session: get64(b, 8),
            sequence: get64(b, 16),
            slot: get32(b, 24) as usize,
            kind: b[124],
            phase: match b[6] {
                1 => Phase::Admission,
                2 => Phase::Execution,
                _ => return Err(Error::Operation),
            },
            status: match get32(b, 28) {
                1 => Status::Staged,
                2 => Status::Released,
                3 => Status::Admitted,
                4 => Status::Activated,
                5 => Status::Cancelled,
                6 => Status::Rejected,
                7 => Status::Failed,
                _ => return Err(Error::Operation),
            },
            runtime_sequence: get64(b, 32),
            requested: get64(b, 40),
            effective: get64(b, 48),
            delivered: (b[7] & 1 != 0).then(|| get64(b, 56)),
            observed: get64(b, 64),
            error: get32(b, 72),
            graph: if b[7] & 2 != 0 {
                Some(graph(b, 76)?)
            } else {
                None
            },
            graph_epoch: Epoch(get64(b, 88)),
            topology_epoch: Epoch(get64(b, 96)),
            bank: (b[7] & 4 != 0).then(|| Bank {
                id: Id {
                    slot: get32(b, 104),
                    generation: get32(b, 108),
                },
                capacity: Counts {
                    steps: get32(b, 112),
                    edges: get32(b, 116),
                    bindings: get32(b, 120),
                },
            }),
        };
        if response.encode()? != b {
            return Err(Error::Reserved);
        }
        Ok(response)
    }
}
#[derive(Clone, Copy)]
struct Pending {
    request: Request,
    admission: Option<Response>,
}
pub struct Client {
    session: u64,
    next_sequence: Option<u64>,
    pending: [Option<Pending>; CREDITS],
    leases: [Option<(StageId, Id)>; crate::graph_transfer::BANKS],
}
impl Client {
    pub fn new(session: u64) -> Result<Self, Error> {
        if session == 0 {
            return Err(Error::Session);
        }
        Ok(Self {
            session,
            next_sequence: Some(1),
            pending: [None; CREDITS],
            leases: [None; crate::graph_transfer::BANKS],
        })
    }
    pub fn prepare(
        &mut self,
        operation: Operation,
        cancelled: bool,
    ) -> Result<[u8; REQUEST_BYTES], Error> {
        let slot = self
            .pending
            .iter()
            .position(Option::is_none)
            .ok_or(Error::Busy)?;
        let sequence = self.next_sequence.ok_or(Error::Exhausted)?;
        let request = Request {
            session: self.session,
            sequence,
            slot,
            cancelled,
            operation,
        };
        let bytes = request.encode()?;
        self.pending[slot] = Some(Pending {
            request,
            admission: None,
        });
        self.next_sequence = sequence.checked_add(1);
        Ok(bytes)
    }
    pub fn receive(&mut self, bytes: &[u8]) -> Result<Response, Error> {
        let response = Response::decode(bytes)?;
        if response.session != self.session {
            return Err(Error::Session);
        }
        let pending = self.pending[response.slot].ok_or(Error::State)?;
        if response.sequence != pending.request.sequence
            || response.kind != pending.request.operation.kind()
            || response.requested != pending.request.operation.at()
        {
            return Err(Error::Sequence);
        }
        if matches!(
            response.status,
            Status::Released | Status::Admitted | Status::Activated
        ) && response.graph != pending.request.operation.graph()
        {
            return Err(Error::Identity);
        }
        let mut lease_change = None;
        if response.status == Status::Staged {
            let Operation::Stage { bank, epoch, slot } = pending.request.operation else {
                return Err(Error::State);
            };
            let graph = response.graph.ok_or(Error::Identity)?;
            if response.bank.map(|b| b.id) != Some(bank)
                || response.graph_epoch != epoch
                || graph.parts()[1] != slot
            {
                return Err(Error::Identity);
            }
            if self
                .leases
                .iter()
                .flatten()
                .any(|(id, existing)| *id == graph || existing.slot == bank.slot)
            {
                return Err(Error::State);
            }
            let index = self
                .leases
                .iter()
                .position(Option::is_none)
                .ok_or(Error::Busy)?;
            lease_change = Some((index, Some((graph, bank))));
        }
        if matches!(response.status, Status::Cancelled | Status::Rejected) {
            if let Operation::Stage { bank, .. } = pending.request.operation {
                let returned = response.bank.ok_or(Error::Identity)?.id;
                if returned
                    != (Id {
                        slot: bank.slot,
                        generation: bank.generation.checked_add(1).unwrap_or(0),
                    })
                {
                    return Err(Error::Identity);
                }
            }
        }
        if response.status == Status::Released {
            let graph = response.graph.ok_or(Error::Identity)?;
            let index = self
                .leases
                .iter()
                .position(|lease| lease.is_some_and(|(id, _)| id == graph))
                .ok_or(Error::Identity)?;
            let (_, bank) = self.leases[index].unwrap();
            if response.bank.ok_or(Error::Identity)?.id
                != (Id {
                    slot: bank.slot,
                    generation: bank.generation.checked_add(1).unwrap_or(0),
                })
            {
                return Err(Error::Identity);
            }
            lease_change = Some((index, None));
        }
        if response.status == Status::Activated {
            let Operation::Activate {
                graph_epoch,
                topology_epoch,
                ..
            } = pending.request.operation
            else {
                return Err(Error::State);
            };
            if graph_epoch.0.checked_add(1) != Some(response.graph_epoch.0)
                || topology_epoch != response.topology_epoch
            {
                return Err(Error::Identity);
            }
        }
        match response.phase {
            Phase::Admission => {
                if pending.admission.is_some() {
                    return Err(Error::State);
                }
                if response.status == Status::Admitted {
                    self.pending[response.slot] = Some(Pending {
                        admission: Some(response),
                        ..pending
                    });
                } else {
                    self.pending[response.slot] = None;
                }
            }
            Phase::Execution => {
                let admission = pending.admission.ok_or(Error::State)?;
                if response.runtime_sequence != admission.runtime_sequence
                    || response.effective != admission.effective
                {
                    return Err(Error::Sequence);
                }
                self.pending[response.slot] = None;
            }
        }
        if let Some((index, lease)) = lease_change {
            self.leases[index] = lease;
        }
        Ok(response)
    }
    pub fn retained_graphs(&self) -> usize {
        self.leases.iter().filter(|lease| lease.is_some()).count()
    }
    pub fn outstanding(&self) -> usize {
        self.pending.iter().filter(|p| p.is_some()).count()
    }
}

#[cfg(test)]
mod tests;
