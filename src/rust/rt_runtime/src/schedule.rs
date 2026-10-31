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

/// `O` is the planner's operation family; `Q` is the processors' family.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action<O = (), Q = ()> {
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
        operation: Q,
    },
}
impl<O: Copy, Q: Copy> Action<O, Q> {
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
    fn accepts<R, S>(self, value: &Value<R, S>) -> bool {
        matches!(
            (self, value),
            (Self::Parameter { .. } | Self::Midi { .. }, Value::Ordinary)
                | (Self::Planner { .. }, Value::Planner(_))
                | (Self::Processor { .. }, Value::Processor(_))
        )
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
pub struct Execution<O = (), R = (), Q = (), S = ()> {
    pub sequence: Sequence,
    pub lane: Lane,
    pub action: Action<O, Q>,
    pub requested_frame: Frame,
    /// Actual action delivery, absent when work terminates before delivery.
    pub delivered_frame: Option<Frame>,
    /// Terminal receipt frame, which may be later than delivery for Pending work.
    pub actual_frame: Frame,
    /// Runtime delivery/receipt error. A domain outcome is carried separately.
    pub result: Result<(), Error>,
    /// Planner operation outcome.
    pub operation_result: Option<R>,
    /// Processor operation outcome.
    pub processor_result: Option<S>,
}
impl<O, R, Q, S> Execution<O, R, Q, S> {
    /// Delivery lateness relative to the request, not time spent Pending.
    /// Returns zero when no delivery occurred.
    pub fn late_by(&self) -> u64 {
        self.delivered_frame
            .map_or(0, |frame| frame.0.saturating_sub(self.requested_frame.0))
    }
}

#[derive(Clone, Copy)]
struct Record<O, R, Q, S> {
    slot: usize,
    execution: Execution<O, R, Q, S>,
}

pub struct Results<O = (), R = (), Q = (), S = ()> {
    completed: Consumer<Record<O, R, Q, S>>,
    released: Producer<usize>,
    pending_release: Option<usize>,
}
impl<O: Copy, R: Copy, Q: Copy, S: Copy> Results<O, R, Q, S> {
    /// Consuming a terminal record releases its credit at a later callback boundary.
    pub fn try_execution(&mut self) -> Option<Execution<O, R, Q, S>> {
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
struct Event<O, Q> {
    sequence: Sequence,
    lane: Lane,
    requested_frame: Frame,
    effective_frame: Frame,
    admitted_frame: Frame,
    delivered_frame: Option<Frame>,
    terminal: bool,
    action: Action<O, Q>,
    payload_len: usize,
}
impl<O: Copy, Q: Copy> Event<O, Q> {
    fn order(self) -> (Frame, (Target, u8, u32), Sequence) {
        (self.effective_frame, self.action.order(), self.sequence)
    }
}

pub struct Due<'a, O = (), Q = ()> {
    pub sequence: Sequence,
    pub frame: Frame,
    pub action: Action<O, Q>,
    pub payload: &'a [u8],
}

/// Removing a due event does not release its terminal reservation. Payload and
/// receipt fields borrow disjoint storage, allowing completion during delivery.
pub struct Delivery<'a, O, R, Q = (), S = ()> {
    pub token: ReceiptToken,
    pub lane: Lane,
    pub requested_frame: Frame,
    pub frame: Frame,
    pub action: Action<O, Q>,
    pub payload: &'a [u8],
    pub receipts: Receipts<'a, O, R, Q, S>,
}

enum Value<R, S> {
    Ordinary,
    Planner(R),
    Processor(S),
}

struct Store<O, R, Q, S> {
    scope: u32,
    events: Vec<Option<Event<O, Q>>>,
    pending: Vec<usize>,
    completed: Producer<Record<O, R, Q, S>>,
    pending_result: Option<Record<O, R, Q, S>>,
}
impl<O: Copy, R: Copy, Q: Copy, S: Copy> Store<O, R, Q, S> {
    /// Slot of a normal-lane typed action that has not been delivered yet.
    fn queued(&self, sequence: Sequence, planner: bool) -> Option<usize> {
        self.events.iter().position(|event| {
            event.is_some_and(|event| {
                event.sequence == sequence
                    && event.lane == Lane::Normal
                    && matches!(event.action, Action::Planner { .. }) == planner
            })
        })
    }
    fn resolve(
        &mut self,
        frame: Frame,
        token: ReceiptToken,
        result: Result<Value<R, S>, Error>,
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
            if frame < event.admitted_frame {
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
            if frame < delivered {
                return Err(Error::InvalidFrame);
            }
            None
        };
        if result
            .as_ref()
            .is_ok_and(|value| !event.action.accepts(value))
        {
            return Err(Error::InvalidEvent);
        }
        if self.pending_result.is_some() {
            return Err(Error::Full);
        }
        let (result, operation_result, processor_result) = match result {
            Ok(Value::Ordinary) => (Ok(()), None, None),
            Ok(Value::Planner(value)) => (Ok(()), Some(value), None),
            Ok(Value::Processor(value)) => (Ok(()), None, Some(value)),
            Err(error) => (Err(error), None, None),
        };
        let record = Record {
            slot: token.slot,
            execution: Execution {
                sequence: event.sequence,
                lane: event.lane,
                action: event.action,
                requested_frame: event.requested_frame,
                delivered_frame,
                actual_frame: frame,
                result,
                operation_result,
                processor_result,
            },
        };
        event.terminal = true;
        if let Some(index) = queued_index {
            self.pending.remove(index);
        }
        if let Err(PushError::Full(record)) = self.completed.push(record) {
            self.pending_result = Some(record);
        }
        Ok(())
    }
}

trait PlannerReceipts<O, R> {
    fn scope(&self) -> u32;
    fn queued(&self, sequence: Sequence) -> Option<usize>;
    fn resolve(
        &mut self,
        frame: Frame,
        token: ReceiptToken,
        result: Result<R, Error>,
        queued: bool,
    ) -> Result<(), Error>;
}
impl<O: Copy, R: Copy, Q: Copy, S: Copy> PlannerReceipts<O, R> for Store<O, R, Q, S> {
    fn scope(&self) -> u32 {
        self.scope
    }
    fn queued(&self, sequence: Sequence) -> Option<usize> {
        Store::queued(self, sequence, true)
    }
    fn resolve(
        &mut self,
        frame: Frame,
        token: ReceiptToken,
        result: Result<R, Error>,
        queued: bool,
    ) -> Result<(), Error> {
        Store::resolve(self, frame, token, result.map(Value::Planner), queued)
    }
}

/// Planner receipt authority at one frame, independent of processor operation types.
pub struct ReceiptWriter<'a, O, R> {
    frame: Frame,
    store: &'a mut dyn PlannerReceipts<O, R>,
}
impl<O, R> ReceiptWriter<'_, O, R> {
    pub fn reborrow(&mut self) -> ReceiptWriter<'_, O, R> {
        ReceiptWriter {
            frame: self.frame,
            store: &mut *self.store,
        }
    }
    /// Resolve one retained token at this writer's frame. A still-retained terminal
    /// token gives InvalidState; stale/reused identities give InvalidId. External
    /// result consumption recycles its credit at a later callback boundary.
    pub fn complete(&mut self, token: ReceiptToken, result: Result<R, Error>) -> Result<(), Error> {
        self.store.resolve(self.frame, token, result, false)
    }
    /// Cancel a normal typed planner event before delivery. Its result retains the
    /// slot; delivered pending work is cancelled by completing its retained token.
    pub fn cancel_queued(&mut self, sequence: Sequence, result: R) -> Result<(), Error> {
        let slot = self.store.queued(sequence).ok_or(Error::InvalidId)?;
        let token = ReceiptToken {
            scope: self.store.scope(),
            slot,
            sequence,
        };
        self.store.resolve(self.frame, token, Ok(result), true)
    }
}

/// Receipt authority for one delivered event, including processor results.
pub struct Receipts<'a, O, R, Q, S> {
    frame: Frame,
    store: &'a mut Store<O, R, Q, S>,
}
impl<O: Copy, R: Copy, Q: Copy, S: Copy> Receipts<'_, O, R, Q, S> {
    /// Planner view of this receipt authority.
    pub fn planner(&mut self) -> ReceiptWriter<'_, O, R> {
        ReceiptWriter {
            frame: self.frame,
            store: &mut *self.store,
        }
    }
    pub fn complete(&mut self, token: ReceiptToken, result: Result<R, Error>) -> Result<(), Error> {
        self.store
            .resolve(self.frame, token, result.map(Value::Planner), false)
    }
    pub fn complete_processor(
        &mut self,
        token: ReceiptToken,
        result: Result<S, Error>,
    ) -> Result<(), Error> {
        self.store
            .resolve(self.frame, token, result.map(Value::Processor), false)
    }
    pub(crate) fn ordinary(
        &mut self,
        token: ReceiptToken,
        result: Result<(), Error>,
    ) -> Result<(), Error> {
        self.store
            .resolve(self.frame, token, result.map(|()| Value::Ordinary), false)
    }
}

pub struct Scheduler<O = (), R = (), Q = (), S = ()> {
    normal_capacity: usize,
    recovery_available: bool,
    store: Store<O, R, Q, S>,
    free: Vec<usize>,
    payload: Box<[u8]>,
    payload_stride: usize,
    midi_payload_limit: usize,
    released: Consumer<usize>,
    highest_sequence: [Sequence; 2],
}

pub fn scheduler(config: PreparedConfig) -> Result<(Results, Scheduler), Error> {
    typed_scheduler(config, 0, false)
}

/// Extra payload stride and the independent recovery receipt are prepared once.
/// The caller validates target signatures, operation semantics, and recovery safety.
pub fn typed_scheduler<O: Copy, R: Copy, Q: Copy, S: Copy>(
    config: PreparedConfig,
    operation_bytes: usize,
    recovery: bool,
) -> Result<(Results<O, R, Q, S>, Scheduler<O, R, Q, S>), Error> {
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
        size_of::<Option<Event<O, Q>>>(),
        size_of::<Record<O, R, Q, S>>(),
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
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
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
            normal_capacity: capacity,
            recovery_available: recovery,
            store: Store {
                scope,
                events,
                pending,
                completed,
                pending_result: None,
            },
            free: (0..capacity).rev().collect(),
            payload,
            payload_stride,
            midi_payload_limit: limits
                .midi_bytes_per_port
                .min(limits.budgets.transfer_bytes),
            released: recycle,
            highest_sequence: [Sequence(0); 2],
        },
    ))
}

impl<O: Copy, R: Copy, Q: Copy, S: Copy> Scheduler<O, R, Q, S> {
    pub fn capacity(&self) -> usize {
        self.normal_capacity
    }
    pub fn delivery_limit(&self) -> usize {
        self.store.events.len()
    }
    pub fn pending(&self) -> usize {
        self.store.pending.len()
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
        self.store
            .events
            .get(self.normal_capacity)
            .and_then(Option::as_ref)
            .is_some_and(|event| !event.terminal)
    }

    pub fn begin_callback(&mut self) -> usize {
        if let Some(record) = self.store.pending_result.take() {
            if let Err(PushError::Full(record)) = self.store.completed.push(record) {
                self.store.pending_result = Some(record);
            }
        }
        let count = self.released.slots().min(self.store.events.len());
        for _ in 0..count {
            let Ok(slot) = self.released.pop() else {
                break;
            };
            self.store.events[slot] = None;
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
        action: Action<O, Q>,
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
        action: Action<O, Q>,
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
        action: Action<O, Q>,
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
        let index = self.store.pending.partition_point(|&index| {
            self.store.events[index].is_some_and(|previous| previous.order() <= event.order())
        });
        self.store.events[slot] = Some(event);
        self.store.pending.insert(index, slot);
        self.highest_sequence[lane_index] = sequence;
        Ok(Admission {
            sequence,
            effective_frame,
            late_by: now.0.saturating_sub(requested_frame.0),
        })
    }
    pub fn next_frame(&self) -> Option<Frame> {
        self.store.events[*self.store.pending.first()?].map(|event| event.effective_frame)
    }
    pub fn due(&self, frame: Frame) -> Option<Due<'_, O, Q>> {
        if self.store.pending_result.is_some() {
            return None;
        }
        let slot = *self.store.pending.first()?;
        let event = self.store.events[slot]?;
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
    pub fn deliver(&mut self, frame: Frame) -> Option<Delivery<'_, O, R, Q, S>> {
        if self.store.pending_result.is_some() {
            return None;
        }
        let slot = *self.store.pending.first()?;
        let event = self.store.events[slot].as_mut()?;
        if event.effective_frame > frame {
            return None;
        }
        event.delivered_frame = Some(frame);
        let event = *event;
        self.store.pending.remove(0);
        let start = slot * self.payload_stride;
        Some(Delivery {
            token: ReceiptToken {
                scope: self.store.scope,
                slot,
                sequence: event.sequence,
            },
            lane: event.lane,
            requested_frame: event.requested_frame,
            frame,
            action: event.action,
            payload: &self.payload[start..start + event.payload_len],
            receipts: Receipts {
                frame,
                store: &mut self.store,
            },
        })
    }
    pub fn receipts(&mut self, frame: Frame) -> ReceiptWriter<'_, O, R> {
        ReceiptWriter {
            frame,
            store: &mut self.store,
        }
    }
    /// Immediate ordinary parameter/MIDI completion. Typed operations use delivery receipts.
    pub fn finish(
        &mut self,
        sequence: Sequence,
        actual_frame: Frame,
        result: Result<(), Error>,
    ) -> Result<(), Error> {
        if self.store.pending_result.is_some() {
            return Err(Error::Full);
        }
        let slot = *self.store.pending.first().ok_or(Error::InvalidState)?;
        let event = self.store.events[slot].ok_or(Error::InvalidState)?;
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
    /// Cancel a queued normal-lane parameter, MIDI or processor action before
    /// delivery. Planner operations are cancelled by their planner.
    pub fn cancel(&mut self, frame: Frame, sequence: Sequence) -> Result<(), Error> {
        let slot = self.store.queued(sequence, false).ok_or(Error::InvalidId)?;
        let token = ReceiptToken {
            scope: self.store.scope,
            slot,
            sequence,
        };
        self.store
            .resolve(frame, token, Err(Error::Cancelled), true)
    }
}
