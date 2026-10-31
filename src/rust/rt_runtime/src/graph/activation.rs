//! Validated graph owners and borrow-scoped, infallible activation after preflight.
use super::PreparedPlan;
use crate::{Epoch, Error};
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_SCOPE: AtomicU32 = AtomicU32::new(1);
fn reserve_scope(counter: &AtomicU32) -> Result<u32, Error> {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| Error::Exhausted)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StageId {
    scope: u32,
    slot: u32,
    generation: u32,
}
impl StageId {
    pub fn from_parts(scope: u32, slot: u32, generation: u32) -> Result<Self, Error> {
        if scope == 0 || generation == 0 {
            return Err(Error::InvalidId);
        }
        Ok(Self {
            scope,
            slot,
            generation,
        })
    }
    pub fn parts(self) -> [u32; 3] {
        [self.scope, self.slot, self.generation]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StageStatus {
    Ready { validated_epoch: Epoch },
    Taken,
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    Idle,
    Ready,
    Taken,
}
struct Slot {
    generation: u32,
    phase: Phase,
    validated_epoch: Epoch,
    owner: Option<PreparedPlan>,
}
pub(crate) struct GraphStages {
    scope: u32,
    plan_bytes: usize,
    slots: Box<[Slot]>,
}
impl GraphStages {
    pub(crate) fn disabled() -> Self {
        Self {
            scope: 0,
            plan_bytes: 0,
            slots: Box::new([]),
        }
    }
    pub(crate) fn enabled(&self) -> bool {
        self.scope != 0
    }
    pub(crate) fn prepare(count: usize, config: crate::PreparedConfig) -> Result<Self, Error> {
        if count == 0 {
            return Err(Error::InvalidConfig);
        }
        if count > u32::MAX as usize
            || count
                .checked_mul(size_of::<Slot>())
                .is_none_or(|n| n > i32::MAX as usize)
        {
            return Err(Error::Capacity);
        }
        let limits = config.limits();
        let mut plan_bytes = size_of::<PreparedPlan>();
        for (count, size) in [
            (limits.max_nodes, size_of::<super::NodeStep>()),
            (limits.max_edges, size_of::<super::Connection>()),
            (limits.max_nodes, size_of::<super::Membership>()),
            (
                limits.max_nodes,
                size_of::<crate::planner::DecisionBinding>(),
            ),
        ] {
            plan_bytes = count
                .checked_mul(size)
                .and_then(|n| plan_bytes.checked_add(n))
                .ok_or(Error::Capacity)?;
        }
        let total = count
            .checked_add(1)
            .and_then(|n| n.checked_mul(plan_bytes))
            .and_then(|n| {
                count
                    .checked_mul(size_of::<Slot>())
                    .and_then(|slots| n.checked_add(slots))
            })
            .ok_or(Error::Capacity)?;
        if total > i32::MAX as usize {
            return Err(Error::Capacity);
        }
        let scope = reserve_scope(&NEXT_SCOPE)?;
        let mut slots: Box<[_]> = (0..count)
            .map(|_| Slot {
                generation: 0,
                phase: Phase::Idle,
                validated_epoch: Epoch(0),
                owner: None,
            })
            .collect();
        for slot in &mut slots {
            std::hint::black_box(slot).validated_epoch = Epoch(0);
        }
        Ok(Self {
            scope,
            plan_bytes,
            slots,
        })
    }
    pub(crate) fn accepts(&self, plan: &PreparedPlan) -> Result<(), Error> {
        if plan.storage_bytes()? > self.plan_bytes {
            Err(Error::Capacity)
        } else {
            Ok(())
        }
    }
    pub(crate) fn can_stage(&self, index: usize) -> Result<(), Error> {
        let slot = self.slots.get(index).ok_or(Error::InvalidId)?;
        if slot.phase != Phase::Idle {
            return Err(Error::Busy);
        }
        slot.generation.checked_add(1).ok_or(Error::Exhausted)?;
        Ok(())
    }
    /// Called only after complete validation against the current catalog/epoch.
    pub(crate) fn install(&mut self, index: usize, epoch: Epoch, owner: PreparedPlan) -> StageId {
        let slot = &mut self.slots[index];
        debug_assert!(slot.phase == Phase::Idle && slot.owner.is_none());
        slot.generation = slot
            .generation
            .checked_add(1)
            .expect("checked destination generation");
        slot.phase = Phase::Ready;
        slot.validated_epoch = epoch;
        slot.owner = Some(owner);
        StageId {
            scope: self.scope,
            slot: index as u32,
            generation: slot.generation,
        }
    }
    fn slot(&self, id: StageId) -> Result<&Slot, Error> {
        self.slots
            .get(id.slot as usize)
            .filter(|slot| {
                id.scope == self.scope
                    && slot.generation == id.generation
                    && slot.phase != Phase::Idle
            })
            .ok_or(Error::InvalidId)
    }
    fn slot_mut(&mut self, id: StageId) -> Result<&mut Slot, Error> {
        self.slot(id)?;
        Ok(&mut self.slots[id.slot as usize])
    }
    pub(crate) fn release(&mut self, id: StageId) -> Result<Option<PreparedPlan>, Error> {
        let slot = self.slot_mut(id)?;
        slot.phase = Phase::Idle;
        Ok(slot.owner.take())
    }
}

struct AccessState<'a> {
    stages: &'a mut GraphStages,
    active: &'a mut Option<PreparedPlan>,
    epoch: &'a mut Epoch,
    activation_allowed: bool,
    check: &'a dyn Fn(&PreparedPlan) -> Result<(), Error>,
}
/// Callback-scoped graph authority. It cannot create plans or validate untrusted descriptors.
/// Preparing a permit does not publish anything; dropping it leaves both owners unchanged.
pub struct GraphAccess<'a> {
    inner: Option<AccessState<'a>>,
}
impl<'a> GraphAccess<'a> {
    /// An inert context for standalone planner implementations/prototypes.
    pub fn disabled() -> Self {
        Self { inner: None }
    }
    pub(crate) fn new(
        stages: &'a mut GraphStages,
        active: &'a mut Option<PreparedPlan>,
        epoch: &'a mut Epoch,
        activation_allowed: bool,
        check: &'a dyn Fn(&PreparedPlan) -> Result<(), Error>,
    ) -> Self {
        Self {
            inner: Some(AccessState {
                stages,
                active,
                epoch,
                activation_allowed,
                check,
            }),
        }
    }
    pub fn active(&self) -> Option<&PreparedPlan> {
        self.inner.as_ref().and_then(|inner| inner.active.as_ref())
    }
    pub fn status(&self, id: StageId) -> Result<StageStatus, Error> {
        let inner = self.inner.as_ref().ok_or(Error::InvalidState)?;
        let slot = inner.stages.slot(id)?;
        Ok(match slot.phase {
            Phase::Ready => StageStatus::Ready {
                validated_epoch: slot.validated_epoch,
            },
            Phase::Taken => StageStatus::Taken,
            Phase::Idle => unreachable!("live stage"),
        })
    }
    pub fn candidate(&self, id: StageId) -> Result<&PreparedPlan, Error> {
        let inner = self.inner.as_ref().ok_or(Error::InvalidState)?;
        let slot = inner.stages.slot(id)?;
        if slot.phase != Phase::Ready {
            return Err(Error::InvalidState);
        }
        slot.owner.as_ref().ok_or(Error::InvalidState)
    }
    /// Recheck scoped stage/catalog identity, validated/current epochs, retained
    /// capacities and pending MIDI compatibility, then reserve an infallible swap.
    /// The caller must finish all fallible domain work before committing the permit.
    /// Unused authority leaves graph owners unchanged, but cannot undo arbitrary
    /// application mutations performed before dropping it.
    pub fn prepare_activation(
        &mut self,
        id: StageId,
        expected: Epoch,
    ) -> Result<GraphActivation<'_>, Error> {
        let inner = self.inner.as_mut().ok_or(Error::InvalidState)?;
        if !inner.activation_allowed {
            return Err(Error::InvalidState);
        }
        if *inner.epoch != expected {
            return Err(Error::StaleEpoch);
        }
        let slot = inner.stages.slot(id)?;
        if slot.phase != Phase::Ready {
            return Err(Error::InvalidState);
        }
        if slot.validated_epoch != expected {
            return Err(Error::StaleEpoch);
        }
        (inner.check)(slot.owner.as_ref().expect("ready owner"))?;
        if let Some(active) = inner.active.as_ref() {
            inner.stages.accepts(active)?;
        }
        let next = expected.checked_next()?;
        Ok(GraphActivation {
            slot: inner.stages.slot_mut(id).expect("checked live stage"),
            active: inner.active,
            epoch: inner.epoch,
            next,
        })
    }
}

/// Exclusive validated activation capability; owns no allocation and performs no work on drop.
/// The displaced plan stays in the stage's reserved storage until owning release.
pub struct GraphActivation<'a> {
    slot: &'a mut Slot,
    active: &'a mut Option<PreparedPlan>,
    epoch: &'a mut Epoch,
    next: Epoch,
}
impl GraphActivation<'_> {
    pub fn candidate(&self) -> &PreparedPlan {
        self.slot.owner.as_ref().expect("ready owner")
    }
    pub fn current(&self) -> Option<&PreparedPlan> {
        self.active.as_ref()
    }
    pub fn next_epoch(&self) -> Epoch {
        self.next
    }
    pub fn commit(self) -> Epoch {
        let candidate = self.slot.owner.take().expect("ready owner");
        self.slot.owner = self.active.replace(candidate);
        self.slot.phase = Phase::Taken;
        *self.epoch = self.next;
        self.next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[shoop_wasm_test_support::shoop_test]
    fn stage_scope_generation_and_epoch_exhaustion_preserve_unpublished_owners() {
        let counter = AtomicU32::new(u32::MAX - 1);
        assert_eq!(reserve_scope(&counter), Ok(u32::MAX - 1));
        assert_eq!(reserve_scope(&counter), Err(Error::Exhausted));
        assert_eq!(counter.load(Ordering::Relaxed), u32::MAX);
        assert_eq!(StageId::from_parts(0, 0, 1), Err(Error::InvalidId));
        assert_eq!(StageId::from_parts(1, 0, 0), Err(Error::InvalidId));
        let config = crate::Config {
            max_nodes: 1,
            max_edges: 1,
            ..crate::Config::default()
        }
        .prepare()
        .unwrap();
        let owner =
            PreparedPlan::from_descriptor(config, super::super::PlanDescriptor::default()).unwrap();
        let mut stages = GraphStages::prepare(1, config).unwrap();
        let excessive = i32::MAX as usize / (stages.plan_bytes + size_of::<Slot>()) + 1;
        assert!(excessive * size_of::<Slot>() < i32::MAX as usize);
        assert!(matches!(
            GraphStages::prepare(excessive, config),
            Err(Error::Capacity)
        ));
        stages.slots[0].generation = u32::MAX - 1;
        stages.can_stage(0).unwrap();
        let id = stages.install(0, Epoch(0), owner);
        assert_eq!(id.parts()[2], u32::MAX);
        let mut active = None;
        let mut epoch = Epoch(0);
        {
            let mut access =
                GraphAccess::new(&mut stages, &mut active, &mut epoch, true, &|_| Ok(()));
            let permit = access.prepare_activation(id, Epoch(0)).unwrap();
            assert!(permit.current().is_none());
            assert_eq!(permit.commit(), Epoch(1));
            assert_eq!(access.status(id), Ok(StageStatus::Taken));
        }
        assert!(stages.release(id).unwrap().is_none());
        assert_eq!(stages.can_stage(0), Err(Error::Exhausted));
        assert_eq!(stages.release(id).err(), Some(Error::InvalidId));
        let mut stages = GraphStages::prepare(1, config).unwrap();
        let id = stages.install(0, Epoch(u64::MAX), active.take().unwrap());
        epoch = Epoch(u64::MAX);
        {
            let mut access =
                GraphAccess::new(&mut stages, &mut active, &mut epoch, true, &|_| Ok(()));
            assert_eq!(
                access.prepare_activation(id, Epoch(u64::MAX)).err(),
                Some(Error::Exhausted)
            );
            assert!(access.active().is_none());
            assert!(access.candidate(id).unwrap().descriptor().steps.is_empty());
            assert_eq!(
                access.status(id),
                Ok(StageStatus::Ready {
                    validated_epoch: Epoch(u64::MAX)
                })
            );
        }
        assert_eq!(epoch, Epoch(u64::MAX));
        assert!(stages.release(id).unwrap().is_some());
        let mut disabled = GraphAccess::disabled();
        assert_eq!(disabled.status(id), Err(Error::InvalidState));
        assert_eq!(
            disabled.prepare_activation(id, Epoch(0)).err(),
            Some(Error::InvalidState)
        );
    }
}
