//! Fixed-capacity scheduling and reliable execution-result credits.
//!
//! Admission conservatively reserves one event and one potential split for every
//! outstanding scheduled operation, regardless of how far in the future it lies.
//! This guarantees bounds for arbitrary host block partitions without predicting
//! their boundaries. Unread execution results retain their slot credit.

use crate::midi::Message;
use crate::{Error, Frame, NodeId, PreparedConfig, Sequence};
use rtrb::{Consumer, Producer, PushError, RingBuffer};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Parameter {
        node: NodeId,
        parameter: u32,
        value: f32,
    },
    Midi {
        node: NodeId,
        port: u32,
    },
}
impl Action {
    pub fn node(self) -> NodeId {
        match self {
            Self::Parameter { node, .. } | Self::Midi { node, .. } => node,
        }
    }
    fn order(self) -> (NodeId, u8, u32) {
        match self {
            Self::Parameter {
                node, parameter, ..
            } => (node, 0, parameter),
            Self::Midi { node, port } => (node, 1, port),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Admission {
    pub sequence: Sequence,
    pub effective_frame: Frame,
    pub late_by: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Execution {
    pub sequence: Sequence,
    pub action: Action,
    pub requested_frame: Frame,
    pub actual_frame: Frame,
    pub result: Result<(), Error>,
}
impl Execution {
    pub fn late_by(self) -> u64 {
        self.actual_frame.0.saturating_sub(self.requested_frame.0)
    }
}

#[derive(Clone, Copy)]
struct Record {
    slot: usize,
    execution: Execution,
}

pub struct Results {
    completed: Consumer<Record>,
    released: Producer<usize>,
    pending_release: Option<usize>,
}
impl Results {
    /// Receipt releases the slot for later reuse. Results contain only Copy data,
    /// so consuming them does not reclaim render-owned heap objects.
    pub fn try_execution(&mut self) -> Option<Execution> {
        if let Some(slot) = self.pending_release.take() {
            if let Err(PushError::Full(slot)) = self.released.push(slot) {
                self.pending_release = Some(slot);
                return None;
            }
        }
        let record = self.completed.pop().ok()?;
        if let Err(PushError::Full(slot)) = self.released.push(record.slot) {
            self.pending_release = Some(slot);
        }
        Some(record.execution)
    }
}

#[derive(Clone, Copy)]
struct Event {
    sequence: Sequence,
    requested_frame: Frame,
    effective_frame: Frame,
    action: Action,
    payload_len: usize,
}
impl Event {
    fn order(self) -> (Frame, (NodeId, u8, u32), Sequence) {
        (self.effective_frame, self.action.order(), self.sequence)
    }
}

pub struct Due<'a> {
    pub sequence: Sequence,
    pub frame: Frame,
    pub action: Action,
    pub payload: &'a [u8],
}

pub struct Scheduler {
    events: Vec<Option<Event>>,
    pending: Vec<usize>,
    free: Vec<usize>,
    payload: Box<[u8]>,
    payload_stride: usize,
    completed: Producer<Record>,
    released: Consumer<usize>,
    pending_result: Option<Record>,
    highest_sequence: Sequence,
    recycle_limit: usize,
}

pub fn scheduler(config: PreparedConfig) -> Result<(Results, Scheduler), Error> {
    let limits = config.limits();
    let capacity = limits
        .scheduled_events
        .min(limits.budgets.events)
        .min(limits.budgets.subranges.saturating_sub(1));
    if capacity == 0 {
        return Err(Error::InvalidConfig);
    }
    let payload_stride = limits
        .midi_bytes_per_port
        .min(limits.budgets.transfer_bytes);
    let bytes = capacity
        .checked_mul(payload_stride)
        .filter(|&n| n <= i32::MAX as usize)
        .ok_or(Error::Capacity)?;
    if capacity
        .checked_mul(size_of::<Option<Event>>())
        .is_none_or(|n| n > i32::MAX as usize)
    {
        return Err(Error::Capacity);
    }
    let mut payload = vec![0; bytes].into_boxed_slice();
    for byte in &mut payload {
        *std::hint::black_box(byte) = 0;
    }
    let (completed, reader) = RingBuffer::new(capacity);
    let (released, recycle) = RingBuffer::new(capacity);
    Ok((
        Results {
            completed: reader,
            released,
            pending_release: None,
        },
        Scheduler {
            events: vec![None; capacity],
            pending: Vec::with_capacity(capacity),
            free: (0..capacity).rev().collect(),
            payload,
            payload_stride,
            completed,
            released: recycle,
            pending_result: None,
            highest_sequence: Sequence(0),
            recycle_limit: capacity,
        },
    ))
}

impl Scheduler {
    pub fn capacity(&self) -> usize {
        self.events.len()
    }
    pub fn pending(&self) -> usize {
        self.pending.len()
    }
    pub fn available(&self) -> usize {
        self.free.len()
    }
    pub fn payload_limit(&self) -> usize {
        self.payload_stride
    }

    /// Bounded credit recycling at the callback boundary. No producer/consumer
    /// synchronization waits; newly released credits may wait for the next call.
    pub fn begin_callback(&mut self) -> usize {
        if let Some(record) = self.pending_result.take() {
            if let Err(PushError::Full(record)) = self.completed.push(record) {
                self.pending_result = Some(record);
            }
        }
        let count = self.released.slots().min(self.recycle_limit);
        for _ in 0..count {
            let Ok(slot) = self.released.pop() else {
                break;
            };
            // Tokens are private and each terminal result yields exactly one.
            self.events[slot] = None;
            self.free.push(slot);
        }
        count
    }

    /// Caller validates live target IDs/ports and debits the payload copy budget
    /// before invoking this method. This method validates payload format, slot and
    /// split credits, and monotonic request identities before mutating any state.
    pub fn admit(
        &mut self,
        sequence: Sequence,
        requested_frame: Frame,
        now: Frame,
        action: Action,
        payload: &[u8],
    ) -> Result<Admission, Error> {
        if sequence <= self.highest_sequence {
            return Err(Error::InvalidId);
        }
        if self.free.is_empty() {
            return Err(Error::Full);
        }
        match action {
            Action::Parameter { value, .. } => {
                if !payload.is_empty() || !value.is_finite() {
                    return Err(Error::InvalidEvent);
                }
            }
            Action::Midi { .. } => {
                Message::parse(payload, self.payload_stride)?;
            }
        }
        let effective_frame = requested_frame.max(now);
        let event = Event {
            sequence,
            requested_frame,
            effective_frame,
            action,
            payload_len: payload.len(),
        };
        let slot = self.free.pop().ok_or(Error::Full)?;
        let start = slot * self.payload_stride;
        self.payload[start..start + payload.len()].copy_from_slice(payload);
        let index = self.pending.partition_point(|&index| {
            self.events[index].is_some_and(|previous| previous.order() <= event.order())
        });
        self.events[slot] = Some(event);
        // One pending entry per admitted slot; removal does not shrink capacity.
        self.pending.insert(index, slot);
        self.highest_sequence = sequence;
        Ok(Admission {
            sequence,
            effective_frame,
            late_by: now.0.saturating_sub(requested_frame.0),
        })
    }

    pub fn next_frame(&self) -> Option<Frame> {
        let slot = *self.pending.first()?;
        self.events[slot].map(|event| event.effective_frame)
    }

    pub fn due(&self, frame: Frame) -> Option<Due<'_>> {
        if self.pending_result.is_some() {
            return None;
        }
        let slot = *self.pending.first()?;
        let event = self.events[slot]?;
        if event.effective_frame > frame {
            return None;
        }
        let start = slot * self.payload_stride;
        Some(Due {
            sequence: event.sequence,
            frame: event.effective_frame,
            action: event.action,
            payload: &self.payload[start..start + event.payload_len],
        })
    }

    /// Acknowledge exactly the oldest due operation after applying it. A rejected
    /// operation also produces a reliable execution result. The slot remains
    /// reserved until control receives that result and returns its credit.
    pub fn finish(
        &mut self,
        sequence: Sequence,
        actual_frame: Frame,
        result: Result<(), Error>,
    ) -> Result<(), Error> {
        if self.pending_result.is_some() {
            return Err(Error::Full);
        }
        let slot = *self.pending.first().ok_or(Error::InvalidState)?;
        let event = self.events[slot].ok_or(Error::InvalidState)?;
        if event.sequence != sequence {
            return Err(Error::InvalidId);
        }
        if actual_frame < event.effective_frame {
            return Err(Error::InvalidFrame);
        }
        let record = Record {
            slot,
            execution: Execution {
                sequence,
                action: event.action,
                requested_frame: event.requested_frame,
                actual_frame,
                result,
            },
        };
        self.pending.remove(0);
        if let Err(PushError::Full(record)) = self.completed.push(record) {
            // No retry/re-execution or result loss if an invariant ever fails.
            self.pending_result = Some(record);
        }
        Ok(())
    }
}
