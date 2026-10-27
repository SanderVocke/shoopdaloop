//! Fixed external admission and reliable terminal receipts, independent of internal intervals.

use crate::midi::Message;
use crate::transport::Lane;
use crate::{Error, Frame, NodeId, PlannerId, PreparedConfig, Sequence};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_SCOPE: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Target {
    Planner(PlannerId),
    Processor(NodeId),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action<O = ()> {
    Parameter {
        node: NodeId,
        parameter: u32,
        value: f32,
    },
    Midi {
        node: NodeId,
        port: u32,
    },
    Planner {
        planner: PlannerId,
        operation: O,
    },
    Processor {
        node: NodeId,
        operation: O,
    },
}
impl<O: Copy> Action<O> {
    pub fn target(self) -> Target {
        match self {
            Self::Planner { planner, .. } => Target::Planner(planner),
            Self::Parameter { node, .. }
            | Self::Midi { node, .. }
            | Self::Processor { node, .. } => Target::Processor(node),
        }
    }
    pub fn node(self) -> Option<NodeId> {
        match self.target() {
            Target::Planner(_) => None,
            Target::Processor(node) => Some(node),
        }
    }
    fn order(self) -> (Target, u8, u32) {
        let (kind, port) = match self {
            Self::Parameter { parameter, .. } => (0, parameter),
            Self::Midi { port, .. } => (1, port),
            Self::Processor { .. } => (2, 0),
            Self::Planner { .. } => (0, 0),
        };
        (self.target(), kind, port)
    }
    fn typed(self) -> bool {
        matches!(self, Self::Planner { .. } | Self::Processor { .. })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Admission {
    pub sequence: Sequence,
    pub effective_frame: Frame,
    pub late_by: u64,
}

/// Runtime-instance-scoped receipt. The per-lane monotonic sequence is its generation.
/// Tokens are not portable capabilities across independent Wasm instances.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReceiptToken {
    scope: u32,
    slot: usize,
    sequence: Sequence,
}
impl ReceiptToken {
    pub fn sequence(self) -> Sequence {
        self.sequence
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Execution<O = (), R = ()> {
    pub sequence: Sequence,
    pub lane: Lane,
    pub action: Action<O>,
    pub requested_frame: Frame,
    /// Actual action delivery, absent when work terminates before delivery.
    pub delivered_frame: Option<Frame>,
    /// Terminal receipt frame, which may be later than delivery for Pending work.
    pub actual_frame: Frame,
    /// Runtime delivery/receipt error. A domain outcome is carried separately.
    pub result: Result<(), Error>,
    pub operation_result: Option<R>,
}
impl<O, R> Execution<O, R> {
    /// Delivery lateness relative to the request, not time spent Pending.
    /// Returns zero when no delivery occurred.
    pub fn late_by(&self) -> u64 {
        self.delivered_frame
            .map_or(0, |frame| frame.0.saturating_sub(self.requested_frame.0))
    }
}

#[derive(Clone, Copy)]
struct Record<O, R> {
    slot: usize,
    execution: Execution<O, R>,
}

pub struct Results<O = (), R = ()> {
    completed: Consumer<Record<O, R>>,
    released: Producer<usize>,
    pending_release: Option<usize>,
}
impl<O: Copy, R: Copy> Results<O, R> {
    /// Consuming a terminal record releases its credit at a later callback boundary.
    pub fn try_execution(&mut self) -> Option<Execution<O, R>> {
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
struct Event<O> {
    sequence: Sequence,
    lane: Lane,
    requested_frame: Frame,
    effective_frame: Frame,
    admitted_frame: Frame,
    delivered_frame: Option<Frame>,
    terminal: bool,
    action: Action<O>,
    payload_len: usize,
}
impl<O: Copy> Event<O> {
    fn order(self) -> (Frame, (Target, u8, u32), Sequence) {
        (self.effective_frame, self.action.order(), self.sequence)
    }
}

pub struct Due<'a, O = ()> {
    pub sequence: Sequence,
    pub frame: Frame,
    pub action: Action<O>,
    pub payload: &'a [u8],
}

/// Removing a due event does not release its terminal reservation. Payload and
/// receipt fields borrow disjoint storage, allowing completion during delivery.
pub struct Delivery<'a, O, R> {
    pub token: ReceiptToken,
    pub lane: Lane,
    pub requested_frame: Frame,
    pub frame: Frame,
    pub action: Action<O>,
    pub payload: &'a [u8],
    pub receipts: ReceiptWriter<'a, O, R>,
}

pub struct ReceiptWriter<'a, O, R> {
    scope: u32,
    frame: Frame,
    events: &'a mut [Option<Event<O>>],
    pending: &'a mut Vec<usize>,
    completed: &'a mut Producer<Record<O, R>>,
    pending_result: &'a mut Option<Record<O, R>>,
}
impl<O: Copy, R: Copy> ReceiptWriter<'_, O, R> {
    pub fn reborrow(&mut self) -> ReceiptWriter<'_, O, R> {
        ReceiptWriter {
            scope: self.scope,
            frame: self.frame,
            events: self.events,
            pending: self.pending,
            completed: self.completed,
            pending_result: self.pending_result,
        }
    }
    /// Resolve one retained token at this writer's frame. A still-retained terminal
    /// token gives InvalidState; stale/reused identities give InvalidId. External
    /// result consumption recycles its credit at a later callback boundary.
    pub fn complete(&mut self, token: ReceiptToken, result: Result<R, Error>) -> Result<(), Error> {
        self.resolve(token, result.map(Some), false)
    }
    pub(crate) fn ordinary(
        &mut self,
        token: ReceiptToken,
        result: Result<(), Error>,
    ) -> Result<(), Error> {
        self.resolve(token, result.map(|()| None), false)
    }
    /// Cancel a normal typed event before delivery. Its result retains the slot;
    /// delivered pending work is cancelled by completing its retained token.
    pub fn cancel_queued(&mut self, sequence: Sequence, result: R) -> Result<(), Error> {
        let slot = self
            .events
            .iter()
            .position(|event| {
                event.is_some_and(|event| {
                    event.sequence == sequence && event.lane == Lane::Normal && event.action.typed()
                })
            })
            .ok_or(Error::InvalidId)?;
        self.resolve(
            ReceiptToken {
                scope: self.scope,
                slot,
                sequence,
            },
            Ok(Some(result)),
            true,
        )
    }
    fn resolve(
        &mut self,
        token: ReceiptToken,
        result: Result<Option<R>, Error>,
        queued: bool,
    ) -> Result<(), Error> {
        if token.scope != self.scope {
            return Err(Error::InvalidId);
        }
        let event = self
            .events
            .get_mut(token.slot)
            .and_then(Option::as_mut)
            .ok_or(Error::InvalidId)?;
        if token.sequence != event.sequence {
            return Err(Error::InvalidId);
        }
        if event.terminal {
            return Err(Error::InvalidState);
        }
        let delivered_frame = event.delivered_frame;
        let queued_index = if queued {
            if self.frame < event.admitted_frame {
                return Err(Error::InvalidFrame);
            }
            if delivered_frame.is_some() {
                return Err(Error::InvalidState);
            }
            Some(
                self.pending
                    .iter()
                    .position(|&slot| slot == token.slot)
                    .ok_or(Error::InvalidState)?,
            )
        } else {
            let delivered = delivered_frame.ok_or(Error::InvalidState)?;
            if self.frame < delivered {
                return Err(Error::InvalidFrame);
            }
            None
        };
        if result
            .as_ref()
            .is_ok_and(|value| value.is_some() != event.action.typed())
        {
            return Err(Error::InvalidEvent);
        }
        if self.pending_result.is_some() {
            return Err(Error::Full);
        }
        let record = Record {
            slot: token.slot,
            execution: Execution {
                sequence: event.sequence,
                lane: event.lane,
                action: event.action,
                requested_frame: event.requested_frame,
                delivered_frame,
                actual_frame: self.frame,
                result: result.map(|_| ()),
                operation_result: result.ok().flatten(),
            },
        };
        event.terminal = true;
        if let Some(index) = queued_index {
            self.pending.remove(index);
        }
        if let Err(PushError::Full(record)) = self.completed.push(record) {
            *self.pending_result = Some(record);
        }
        Ok(())
    }
}

pub struct Scheduler<O = (), R = ()> {
    scope: u32,
    normal_capacity: usize,
    recovery_available: bool,
    events: Vec<Option<Event<O>>>,
    pending: Vec<usize>,
    free: Vec<usize>,
    payload: Box<[u8]>,
    payload_stride: usize,
    midi_payload_limit: usize,
    completed: Producer<Record<O, R>>,
    released: Consumer<usize>,
    pending_result: Option<Record<O, R>>,
    highest_sequence: [Sequence; 2],
}

pub fn scheduler(config: PreparedConfig) -> Result<(Results, Scheduler), Error> {
    typed_scheduler(config, 0, false)
}

/// Extra payload stride and the independent recovery receipt are prepared once.
/// The caller validates target signatures, operation semantics, and recovery safety.
pub fn typed_scheduler<O: Copy, R: Copy>(
    config: PreparedConfig,
    operation_bytes: usize,
    recovery: bool,
) -> Result<(Results<O, R>, Scheduler<O, R>), Error> {
    let limits = config.limits();
    if operation_bytes > i32::MAX as usize {
        return Err(Error::Capacity);
    }
    let capacity = limits.scheduled_events.min(limits.budgets.events);
    let total = capacity
        .checked_add(usize::from(recovery))
        .ok_or(Error::Capacity)?;
    if capacity == 0 {
        return Err(Error::InvalidConfig);
    }
    let payload_stride = limits
        .midi_bytes_per_port
        .max(operation_bytes)
        .min(limits.budgets.transfer_bytes);
    for size in [
        payload_stride,
        size_of::<Option<Event<O>>>(),
        size_of::<Record<O, R>>(),
        size_of::<usize>(),
    ] {
        if total
            .checked_mul(size)
            .is_none_or(|bytes| bytes > i32::MAX as usize)
        {
            return Err(Error::Capacity);
        }
    }
    let scope = NEXT_SCOPE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| Error::Exhausted)?;
    let mut payload = vec![0; total * payload_stride].into_boxed_slice();
    for byte in &mut payload {
        *std::hint::black_box(byte) = 0;
    }
    let mut events = vec![None; total];
    for event in &mut events {
        *std::hint::black_box(event) = None;
    }
    let mut pending = vec![0; total];
    for index in &mut pending {
        *std::hint::black_box(index) = 0;
    }
    pending.clear();
    let (completed, reader) = RingBuffer::new(total);
    let (released, recycle) = RingBuffer::new(total);
    Ok((
        Results {
            completed: reader,
            released,
            pending_release: None,
        },
        Scheduler {
            scope,
            normal_capacity: capacity,
            recovery_available: recovery,
            events,
            pending,
            free: (0..capacity).rev().collect(),
            payload,
            payload_stride,
            midi_payload_limit: limits
                .midi_bytes_per_port
                .min(limits.budgets.transfer_bytes),
            completed,
            released: recycle,
            pending_result: None,
            highest_sequence: [Sequence(0); 2],
        },
    ))
}

impl<O: Copy, R: Copy> Scheduler<O, R> {
    pub fn capacity(&self) -> usize {
        self.normal_capacity
    }
    pub fn delivery_limit(&self) -> usize {
        self.events.len()
    }
    pub fn pending(&self) -> usize {
        self.pending.len()
    }
    pub fn available(&self) -> usize {
        self.free.len()
    }
    pub fn recovery_available(&self) -> bool {
        self.recovery_available
    }
    pub fn payload_limit(&self) -> usize {
        self.payload_stride
    }
    pub fn recovery_pending(&self) -> bool {
        self.events
            .get(self.normal_capacity)
            .and_then(Option::as_ref)
            .is_some_and(|event| !event.terminal)
    }

    pub fn begin_callback(&mut self) -> usize {
        if let Some(record) = self.pending_result.take() {
            if let Err(PushError::Full(record)) = self.completed.push(record) {
                self.pending_result = Some(record);
            }
        }
        let count = self.released.slots().min(self.events.len());
        for _ in 0..count {
            let Ok(slot) = self.released.pop() else {
                break;
            };
            self.events[slot] = None;
            if slot == self.normal_capacity {
                self.recovery_available = true;
            } else {
                self.free.push(slot);
            }
        }
        count
    }

    pub fn admit(
        &mut self,
        sequence: Sequence,
        requested_frame: Frame,
        now: Frame,
        action: Action<O>,
        payload: &[u8],
    ) -> Result<Admission, Error> {
        self.admit_on(
            Lane::Normal,
            sequence,
            requested_frame,
            now,
            action,
            payload,
        )
    }
    pub fn admit_recovery(
        &mut self,
        sequence: Sequence,
        requested_frame: Frame,
        now: Frame,
        action: Action<O>,
        payload: &[u8],
    ) -> Result<Admission, Error> {
        if requested_frame > now || !payload.is_empty() || !matches!(action, Action::Planner { .. })
        {
            return Err(Error::InvalidEvent);
        }
        self.admit_on(
            Lane::Recovery,
            sequence,
            requested_frame,
            now,
            action,
            payload,
        )
    }
    fn admit_on(
        &mut self,
        lane: Lane,
        sequence: Sequence,
        requested_frame: Frame,
        now: Frame,
        action: Action<O>,
        payload: &[u8],
    ) -> Result<Admission, Error> {
        let lane_index = usize::from(lane == Lane::Recovery);
        if sequence <= self.highest_sequence[lane_index] {
            return Err(Error::InvalidId);
        }
        if (lane == Lane::Normal && self.free.is_empty())
            || (lane == Lane::Recovery && !self.recovery_available)
        {
            return Err(Error::Full);
        }
        if payload.len() > self.payload_stride {
            return Err(Error::Capacity);
        }
        match action {
            Action::Parameter { value, .. } if !payload.is_empty() || !value.is_finite() => {
                return Err(Error::InvalidEvent)
            }
            Action::Midi { .. } => {
                Message::parse(payload, self.midi_payload_limit)?;
            }
            _ => {}
        }
        let effective_frame = requested_frame.max(now);
        let event = Event {
            sequence,
            lane,
            requested_frame,
            effective_frame,
            admitted_frame: now,
            delivered_frame: None,
            terminal: false,
            action,
            payload_len: payload.len(),
        };
        let slot = if lane == Lane::Recovery {
            self.recovery_available = false;
            self.normal_capacity
        } else {
            self.free.pop().ok_or(Error::Full)?
        };
        let start = slot * self.payload_stride;
        self.payload[start..start + payload.len()].copy_from_slice(payload);
        let index = self.pending.partition_point(|&index| {
            self.events[index].is_some_and(|previous| previous.order() <= event.order())
        });
        self.events[slot] = Some(event);
        self.pending.insert(index, slot);
        self.highest_sequence[lane_index] = sequence;
        Ok(Admission {
            sequence,
            effective_frame,
            late_by: now.0.saturating_sub(requested_frame.0),
        })
    }
    pub fn next_frame(&self) -> Option<Frame> {
        self.events[*self.pending.first()?].map(|event| event.effective_frame)
    }
    pub fn due(&self, frame: Frame) -> Option<Due<'_, O>> {
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
    pub fn deliver(&mut self, frame: Frame) -> Option<Delivery<'_, O, R>> {
        if self.pending_result.is_some() {
            return None;
        }
        let slot = *self.pending.first()?;
        let event = self.events[slot].as_mut()?;
        if event.effective_frame > frame {
            return None;
        }
        event.delivered_frame = Some(frame);
        let event = *event;
        self.pending.remove(0);
        let start = slot * self.payload_stride;
        Some(Delivery {
            token: ReceiptToken {
                scope: self.scope,
                slot,
                sequence: event.sequence,
            },
            lane: event.lane,
            requested_frame: event.requested_frame,
            frame,
            action: event.action,
            payload: &self.payload[start..start + event.payload_len],
            receipts: ReceiptWriter {
                scope: self.scope,
                frame,
                events: &mut self.events,
                pending: &mut self.pending,
                completed: &mut self.completed,
                pending_result: &mut self.pending_result,
            },
        })
    }
    pub fn receipts(&mut self, frame: Frame) -> ReceiptWriter<'_, O, R> {
        ReceiptWriter {
            scope: self.scope,
            frame,
            events: &mut self.events,
            pending: &mut self.pending,
            completed: &mut self.completed,
            pending_result: &mut self.pending_result,
        }
    }
    /// Immediate ordinary parameter/MIDI completion. Typed operations use delivery receipts.
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
        if event.action.typed() {
            return Err(Error::InvalidState);
        }
        let mut delivery = self.deliver(actual_frame).ok_or(Error::InvalidState)?;
        delivery.receipts.ordinary(delivery.token, result)
    }
}
