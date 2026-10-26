//! Prepared, endpoint-scoped ingestion and validation of immutable timing metadata.

use super::*;
use std::sync::atomic::{AtomicU32, Ordering};
mod codec;
mod validate;
pub use codec::RECORD_BYTES;
use codec::{Counts, Record};
use validate::Validator;

static NEXT_SCOPE: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StageId {
    scope: u32,
    generation: u32,
}
impl StageId {
    pub fn from_parts(scope: u32, generation: u32) -> Result<Self, Error> {
        if scope == 0 || generation == 0 {
            Err(Error::InvalidId)
        } else {
            Ok(Self { scope, generation })
        }
    }
    pub fn parts(self) -> [u32; 2] {
        [self.scope, self.generation]
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Idle,
    Receiving { received: usize, total: usize },
    Validating,
    Ready,
    Taken,
    Failed(Error),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Progress {
    pub used: usize,
    pub total_steps: u64,
    pub ready: bool,
}

pub struct Stager {
    limits: Limits,
    scope: u32,
    generation: u32,
    status: Status,
    topology: Option<PreparedTopology>,
    counts: Counts,
    relations: usize,
    validator: Validator,
    steps: u64,
}
fn next_scope(counter: &AtomicU32) -> Result<u32, Error> {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |scope| {
            scope.checked_add(1)
        })
        .map_err(|_| Error::Exhausted)
}
fn pretouched<T: Clone>(count: usize, value: T) -> Vec<T> {
    let mut data = vec![value; count];
    data.clear();
    data
}
impl Stager {
    /// Includes fixed tables, validation scratch and the inline owner header;
    /// excludes allocator overhead and separately retained active/retired owners.
    pub fn required_storage_bytes(limits: Limits) -> Result<usize, Error> {
        let limits = limits.validate()?;
        let bytes = size_of::<Self>()
            .checked_add(limits.topology_storage_bytes()? - size_of::<PreparedTopology>())
            .and_then(|bytes| bytes.checked_add(Validator::heap_bytes(limits).ok()?))
            .ok_or(Error::Capacity)?;
        crate::config::checked_storage(bytes, 1)
    }
    pub fn prepare(limits: Limits) -> Result<Self, Error> {
        Self::required_storage_bytes(limits)?;
        let scope = next_scope(&NEXT_SCOPE)?;
        let id = LoopId::from_parts(0, 1)?;
        let range = TableRange { start: 0, len: 0 };
        let topology = PreparedTopology {
            limits,
            nodes: pretouched(
                limits.loop_slots,
                LoopSpec {
                    id,
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: None,
                },
            ),
            triggers: pretouched(
                limits.relations,
                Trigger {
                    source: id,
                    target: id,
                    mode: Mode::Stopped,
                },
            ),
            programs: pretouched(
                limits.loop_slots,
                ProgramRecord {
                    source: id,
                    kind: crate::program::ProgramKind::Regular,
                    nominal_unit_frames: 1,
                    iterations: 0,
                    targets: range,
                    segments: range,
                },
            ),
            targets: pretouched(
                limits.program_entries,
                TargetRecord {
                    target: id,
                    segments: range,
                },
            ),
            segments: pretouched(
                limits.program_segments,
                Segment {
                    start: 0,
                    end: 0,
                    mode: Mode::Stopped,
                    occurrence_start: 0,
                    ordinal: 0,
                },
            ),
            components: pretouched(limits.loop_slots, range),
            members: pretouched(limits.loop_slots, id),
            relation_count: 0,
        };
        Ok(Self {
            limits,
            scope,
            generation: 0,
            status: Status::Idle,
            topology: Some(topology),
            counts: Counts([0; 7]),
            relations: 0,
            validator: Validator::prepare(limits),
            steps: 0,
        })
    }
    pub fn status(&self) -> Status {
        self.status
    }
    fn check(&self, id: StageId) -> Result<(), Error> {
        if id.scope != self.scope || id.generation != self.generation {
            Err(Error::InvalidId)
        } else {
            Ok(())
        }
    }
    fn clear(topology: &mut PreparedTopology) {
        topology.nodes.clear();
        topology.triggers.clear();
        topology.programs.clear();
        topology.targets.clear();
        topology.segments.clear();
        topology.components.clear();
        topology.members.clear();
        topology.relation_count = 0;
    }
    pub fn begin(&mut self, header: &[u8; RECORD_BYTES]) -> Result<StageId, Error> {
        if self.status != Status::Idle {
            return Err(Error::InvalidState);
        }
        let (counts, relations) = codec::decode_header(self.limits, header)?;
        let generation = self.generation.checked_add(1).ok_or(Error::Exhausted)?;
        Self::clear(self.topology.as_mut().expect("idle storage"));
        self.validator.reset();
        self.generation = generation;
        self.counts = counts;
        self.relations = relations;
        self.steps = 0;
        self.status = if counts.total() == 0 {
            Status::Validating
        } else {
            Status::Receiving {
                received: 0,
                total: counts.total(),
            }
        };
        Ok(StageId {
            scope: self.scope,
            generation,
        })
    }
    /// Ordered complete records. Syntax/budget/offset failure leaves the whole
    /// chunk unconsumed; cross-record semantic validation is a separate phase.
    pub fn append(
        &mut self,
        id: StageId,
        first_record: usize,
        bytes: &[u8],
        record_budget: usize,
    ) -> Result<(), Error> {
        self.check(id)?;
        let Status::Receiving { received, total } = self.status else {
            return Err(Error::InvalidState);
        };
        if first_record != received {
            return Err(Error::InvalidState);
        }
        if !bytes.len().is_multiple_of(RECORD_BYTES) {
            return Err(Error::InvalidProgram);
        }
        let count = bytes.len() / RECORD_BYTES;
        if count > record_budget || count > total - received {
            return Err(Error::Capacity);
        }
        for (offset, record) in bytes.chunks_exact(RECORD_BYTES).enumerate() {
            let (kind, _) = self
                .counts
                .locate(received + offset)
                .expect("bounded input");
            Record::decode(kind, record)?;
        }
        let topology = self.topology.as_mut().expect("receiving storage");
        for (offset, record) in bytes.chunks_exact(RECORD_BYTES).enumerate() {
            let (kind, _) = self
                .counts
                .locate(received + offset)
                .expect("bounded input");
            Record::decode(kind, record)
                .expect("validated immutable chunk")
                .append(topology);
        }
        self.status = if received + count == total {
            Status::Validating
        } else {
            Status::Receiving {
                received: received + count,
                total,
            }
        };
        Ok(())
    }
    pub fn validate(&mut self, id: StageId, step_budget: usize) -> Result<Progress, Error> {
        self.check(id)?;
        match self.status {
            Status::Ready => {
                return Ok(Progress {
                    used: 0,
                    total_steps: self.steps,
                    ready: true,
                })
            }
            Status::Failed(error) => return Err(error),
            Status::Validating => {}
            _ => return Err(Error::InvalidState),
        }
        let topology = self.topology.as_mut().expect("validating storage");
        let mut used = 0;
        for _ in 0..step_budget {
            used += 1;
            self.steps = self.steps.saturating_add(1);
            match self.validator.step(topology, self.relations) {
                Ok(true) => {
                    topology.relation_count = self.relations;
                    self.status = Status::Ready;
                    break;
                }
                Ok(false) => {}
                Err(error) => {
                    self.status = Status::Failed(error);
                    return Err(error);
                }
            }
        }
        Ok(Progress {
            used,
            total_steps: self.steps,
            ready: self.status == Status::Ready,
        })
    }
    pub fn view(&self, id: StageId) -> Result<&PreparedTopology, Error> {
        self.check(id)?;
        if self.status != Status::Ready {
            return Err(Error::InvalidState);
        }
        Ok(self.topology.as_ref().expect("ready storage"))
    }
    pub fn take(&mut self, id: StageId) -> Result<PreparedTopology, Error> {
        self.check(id)?;
        if self.status != Status::Ready {
            return Err(Error::InvalidState);
        }
        self.status = Status::Taken;
        Ok(self.topology.take().expect("ready storage"))
    }
    pub fn cancel(&mut self, id: StageId) -> Result<(), Error> {
        self.check(id)?;
        if matches!(self.status, Status::Idle | Status::Taken) {
            return Err(Error::InvalidState);
        }
        Self::clear(self.topology.as_mut().expect("owned staging storage"));
        self.status = Status::Idle;
        Ok(())
    }
    pub fn recycle(
        &mut self,
        mut topology: PreparedTopology,
    ) -> Result<(), (Error, PreparedTopology)> {
        if self.status != Status::Taken {
            return Err((Error::InvalidState, topology));
        }
        if topology.limits != self.limits {
            return Err((Error::InvalidConfig, topology));
        }
        let capacities = [
            topology.nodes.capacity(),
            topology.triggers.capacity(),
            topology.programs.capacity(),
            topology.targets.capacity(),
            topology.segments.capacity(),
            topology.components.capacity(),
            topology.members.capacity(),
        ];
        if capacities
            .into_iter()
            .zip(Counts::maximum(self.limits).0)
            .any(|(capacity, required)| capacity < required)
        {
            return Err((Error::Capacity, topology));
        }
        Self::clear(&mut topology);
        self.topology = Some(topology);
        self.status = Status::Idle;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
    #[shoop_test]
    fn stage_generation_and_scope_exhaustion_never_wrap() {
        let counter = AtomicU32::new(u32::MAX - 1);
        assert_eq!(next_scope(&counter), Ok(u32::MAX - 1));
        assert_eq!(next_scope(&counter), Err(Error::Exhausted));
        let limits = Limits::default();
        let topology = compile(limits, &TopologyDescription::default()).unwrap();
        let mut header = [0; RECORD_BYTES];
        topology.encode_header(&mut header);
        let mut stager = Stager::prepare(limits).unwrap();
        stager.generation = u32::MAX - 1;
        assert_no_heap(|| {
            let id = stager.begin(&header).unwrap();
            assert_eq!(id.parts()[1], u32::MAX);
            stager.cancel(id).unwrap();
            assert_eq!(stager.begin(&header), Err(Error::Exhausted));
            assert_eq!(stager.status(), Status::Idle);
        });
    }
}
