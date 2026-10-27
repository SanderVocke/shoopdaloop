//! Generic planning metadata, independent of signal ports and domain semantics.

use crate::{Error, Frame, NodeId, PlannerId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlannerSpec {
    pub id: PlannerId,
    pub decision_slots: usize,
    pub observation_capacity: usize,
    pub operation_bytes: usize,
}

impl PlannerSpec {
    pub(crate) fn validate(self) -> Result<(), Error> {
        for count in [
            self.decision_slots,
            self.observation_capacity,
            self.operation_bytes,
        ] {
            if count > i32::MAX as usize {
                return Err(Error::Capacity);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConsumerAccess {
    pub decision_required: bool,
    pub observations: usize,
    pub operation_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConsumerSpec {
    pub node: NodeId,
    pub access: ConsumerAccess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanningStep {
    pub planner: PlannerSpec,
    /// The serial lifecycle requires a planning prefix, before signal step zero.
    pub before_step: usize,
}

/// Validated planning dependency, separate from signal edges. A binding selects
/// one immutable decision slot for the exact consumer generation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DecisionBinding {
    pub planner: PlannerId,
    pub consumer: NodeId,
    pub decision_slot: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlanningDescriptor {
    pub step: Option<PlanningStep>,
    pub bindings: Vec<DecisionBinding>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Interval {
    pub start: Frame,
    pub end: Frame,
}

/// Authority for one boundary. Internal intervals do not renew callback-wide
/// ingress, result recycling, validation, background-copy or recovery allowances.
pub struct PlanContext<'a, O = (), R = ()> {
    /// Epoch on context entry; a successful permit may publish a newer one.
    pub graph_epoch: crate::Epoch,
    pub graphs: crate::graph::activation::GraphAccess<'a>,
    /// Start of the enclosing nonempty host callback, not the internal interval.
    pub callback_start: Frame,
    pub frame: Frame,
    pub ceiling: Frame,
    pub resources: crate::resource::PlannerResourceIo<'a>,
    pub receipts: crate::schedule::ReceiptWriter<'a, O, R>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Disposition<R> {
    Complete(R),
    /// Retain the context's receipt token and resolve it exactly once later.
    /// The token reserves a result, not graph/candidate ownership or a lock.
    Pending,
}

pub struct OperationContext<'a, O, R> {
    /// Epoch on entry, not the outcome of any publication within this operation.
    pub graph_epoch: crate::Epoch,
    pub graphs: crate::graph::activation::GraphAccess<'a>,
    /// Stable across every operation and planning interval in this callback.
    pub callback_start: Frame,
    pub token: crate::schedule::ReceiptToken,
    pub requested_frame: Frame,
    pub frame: Frame,
    pub recovery: bool,
    pub payload: &'a [u8],
    pub resources: crate::resource::PlannerResourceIo<'a>,
    pub receipts: crate::schedule::ReceiptWriter<'a, O, R>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Observation<O> {
    pub producer: NodeId,
    pub value: O,
}

pub struct FinishContext<'a, V, O = (), R = ()> {
    pub interval: Interval,
    pub observations: &'a [Observation<V>],
    pub resources: crate::resource::PlannerResourceIo<'a>,
    pub receipts: crate::schedule::ReceiptWriter<'a, O, R>,
}

/// Implementations own initialized, pre-touched decision storage before runtime
/// preparation. Its length and signature remain fixed. Callback methods obey the
/// processor nonblocking/no-heap contract; same-frame settlement must terminate.
pub trait Planner: Send {
    type Decision: Copy + Send + 'static;
    type Observation: Copy + Default + Send + 'static;
    type Operation: Copy + Send + 'static;
    type Result: Copy + Send + 'static;

    /// Queried only during preparation. None selects the signal-only lifecycle.
    fn spec(&self) -> Option<PlannerSpec>;
    fn decisions(&self) -> &[Self::Decision];
    /// Settle the current boundary and return frame < end <= ceiling.
    fn plan(
        &mut self,
        context: PlanContext<'_, Self::Operation, Self::Result>,
    ) -> Result<Frame, Error>;
    /// Consume this interval's observations without settling its end boundary.
    fn finish(
        &mut self,
        context: FinishContext<'_, Self::Observation, Self::Operation, Self::Result>,
    ) -> Result<(), Error>;
    /// Recovery is payload-free and must complete no later than planning at the
    /// delivery frame. Active structural edits and arbitrary deferred work are not safe.
    fn recovery_safe(_operation: Self::Operation) -> bool {
        false
    }
    fn operation(
        &mut self,
        _operation: Self::Operation,
        _context: OperationContext<'_, Self::Operation, Self::Result>,
    ) -> Result<Disposition<Self::Result>, Error> {
        Err(Error::InvalidEvent)
    }
}

#[derive(Default)]
pub struct NoPlanner;
impl Planner for NoPlanner {
    type Decision = ();
    type Observation = ();
    type Operation = ();
    type Result = ();
    fn spec(&self) -> Option<PlannerSpec> {
        None
    }
    fn decisions(&self) -> &[()] {
        &[]
    }
    fn plan(&mut self, context: PlanContext<'_>) -> Result<Frame, Error> {
        Ok(context.ceiling)
    }
    fn finish(&mut self, _context: FinishContext<'_, ()>) -> Result<(), Error> {
        Ok(())
    }
}

/// Borrowed decision for this interval only. Copying the view does not extend its
/// lifetime or permit retaining it across planner mutation/publication.
#[derive(Clone, Copy)]
pub struct DecisionView<'a, D> {
    pub planner: PlannerId,
    pub slot: usize,
    pub interval: Interval,
    pub value: &'a D,
}

/// A consumer cannot spend another consumer's observation reservation. Values are
/// copied into initialized storage; refusal leaves earlier observations intact.
pub struct ObservationWriter<'a, O> {
    buffer: &'a mut [Observation<O>],
    written: &'a mut usize,
    remaining: usize,
    producer: NodeId,
}
impl<'a, O: Copy> ObservationWriter<'a, O> {
    pub fn push(&mut self, value: O) -> Result<(), Error> {
        if self.remaining == 0 {
            return Err(Error::Capacity);
        }
        let target = self.buffer.get_mut(*self.written).ok_or(Error::Capacity)?;
        *target = Observation {
            producer: self.producer,
            value,
        };
        *self.written += 1;
        self.remaining -= 1;
        Ok(())
    }
    pub fn remaining(&self) -> usize {
        self.remaining
    }
    pub fn disabled(written: &'a mut usize) -> Self {
        Self {
            buffer: &mut [],
            written,
            remaining: 0,
            producer: NodeId::from_parts(0, 1).expect("valid fixed identity"),
        }
    }
}

pub(crate) struct Observations<O> {
    buffer: Box<[Observation<O>]>,
    written: usize,
}
impl<O: Copy + Default> Observations<O> {
    pub fn new(capacity: usize) -> Result<Self, Error> {
        if capacity
            .checked_mul(size_of::<Observation<O>>())
            .is_none_or(|bytes| bytes > i32::MAX as usize)
        {
            return Err(Error::Capacity);
        }
        let initial = Observation {
            producer: NodeId::from_parts(0, 1)?,
            value: O::default(),
        };
        let mut buffer = vec![initial; capacity].into_boxed_slice();
        for value in &mut buffer {
            *std::hint::black_box(value) = initial;
        }
        Ok(Self { buffer, written: 0 })
    }
    pub fn clear(&mut self) {
        self.written = 0;
    }
    pub fn values(&self) -> &[Observation<O>] {
        &self.buffer[..self.written]
    }
    pub fn writer(&mut self, producer: NodeId, allowance: usize) -> ObservationWriter<'_, O> {
        ObservationWriter {
            buffer: &mut self.buffer,
            written: &mut self.written,
            remaining: allowance,
            producer,
        }
    }
}
