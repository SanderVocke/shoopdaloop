//! Allocation-free composite-loop state transitions over a compiled plan.

use crate::composite_plan::{
    CompiledChildMode, CompiledCompositeKind, CompiledCompositePlan, CompiledDesiredState,
    LoopIdentity, LoopTargetKind, MAX_COMPOSITE_BOUNDARY_OUTPUTS, MAX_COMPOSITE_TARGETS,
};
use crate::LoopMode;

const EMPTY_IDENTITY: LoopIdentity = LoopIdentity {
    slot: 0,
    generation: 0,
    kind: LoopTargetKind::Basic,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositeTargetAction {
    Stop,
    SetMode {
        mode: LoopMode,
        cycle_offset: u32,
        retrigger: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositeTargetTransition {
    pub target: LoopIdentity,
    pub action: CompositeTargetAction,
}

const EMPTY_TRANSITION: CompositeTargetTransition = CompositeTargetTransition {
    target: EMPTY_IDENTITY,
    action: CompositeTargetAction::Stop,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositeTransitionBatch {
    transitions: [CompositeTargetTransition; MAX_COMPOSITE_BOUNDARY_OUTPUTS],
    len: usize,
}

impl Default for CompositeTransitionBatch {
    fn default() -> Self {
        Self {
            transitions: [EMPTY_TRANSITION; MAX_COMPOSITE_BOUNDARY_OUTPUTS],
            len: 0,
        }
    }
}

impl CompositeTransitionBatch {
    pub fn as_slice(&self) -> &[CompositeTargetTransition] {
        &self.transitions[..self.len]
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn push(&mut self, transition: CompositeTargetTransition) -> Result<(), CompositeRuntimeError> {
        let destination = self
            .transitions
            .get_mut(self.len)
            .ok_or(CompositeRuntimeError::OutputCapacity)?;
        *destination = transition;
        self.len += 1;
        Ok(())
    }

    fn append(&mut self, other: &Self) -> Result<(), CompositeRuntimeError> {
        for &transition in other.as_slice() {
            self.push(transition)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompositeRuntimeCounters {
    pub stale_targets: u64,
    pub invalid_seeks: u64,
    pub rejected_modes: u64,
    pub plan_mismatches: u64,
    pub output_overflows: u64,
    pub arithmetic_overflows: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositeRuntimeFault {
    None,
    OutputCapacity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CompositeRuntimeError {
    #[error("the plan does not match this runtime")]
    PlanMismatch,
    #[error("unknown is not a runnable composite mode")]
    UnknownMode,
    #[error("the requested seek iteration is outside the plan")]
    InvalidSeek,
    #[error("the transition output capacity was exceeded")]
    OutputCapacity,
    #[error("a replacement plan contains a stale target")]
    StalePlanTarget,
    #[error("a deferred replacement was offered away from iteration zero")]
    NotAtIterationZero,
    #[error("the synchronization position is outside the current iteration")]
    InvalidSyncPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositePlanReplacement {
    Activated,
    DeferredUntilIterationZero,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingCompositeTransition {
    pub mode: LoopMode,
    pub boundaries_to_skip: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActiveTarget {
    active: bool,
    mode: LoopMode,
    cycle_offset: u32,
}

const INACTIVE_TARGET: ActiveTarget = ActiveTarget {
    active: false,
    mode: LoopMode::Stopped,
    cycle_offset: 0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveCompositeChild {
    pub identity: LoopIdentity,
    pub mode: LoopMode,
    pub cycle_offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeRuntime {
    source: LoopIdentity,
    installed_targets: [LoopIdentity; MAX_COMPOSITE_TARGETS],
    target_count: usize,
    active: [ActiveTarget; MAX_COMPOSITE_TARGETS],
    mode: LoopMode,
    pending: Option<PendingCompositeTransition>,
    iteration: u32,
    sync_position: u64,
    cycle_count: u64,
    play_after_record: bool,
    counters: CompositeRuntimeCounters,
    fault: CompositeRuntimeFault,
}

impl CompositeRuntime {
    pub fn new(plan: &CompiledCompositePlan) -> Self {
        let mut runtime = Self {
            source: plan.source(),
            installed_targets: [EMPTY_IDENTITY; MAX_COMPOSITE_TARGETS],
            target_count: 0,
            active: [INACTIVE_TARGET; MAX_COMPOSITE_TARGETS],
            mode: LoopMode::Stopped,
            pending: None,
            iteration: 0,
            sync_position: 0,
            cycle_count: 0,
            play_after_record: false,
            counters: CompositeRuntimeCounters::default(),
            fault: CompositeRuntimeFault::None,
        };
        runtime.install_target_table(plan);
        runtime
    }

    pub fn mode(&self) -> LoopMode {
        self.mode
    }

    pub fn pending(&self) -> Option<PendingCompositeTransition> {
        self.pending
    }

    pub fn iteration(&self) -> u32 {
        self.iteration
    }

    pub fn sync_position(&self) -> u64 {
        self.sync_position
    }

    pub fn cycle_count(&self) -> u64 {
        self.cycle_count
    }

    pub fn play_after_record(&self) -> bool {
        self.play_after_record
    }

    pub fn counters(&self) -> CompositeRuntimeCounters {
        self.counters
    }

    pub fn fault(&self) -> CompositeRuntimeFault {
        self.fault
    }

    pub fn length_samples(
        &self,
        plan: &CompiledCompositePlan,
    ) -> Result<u64, CompositeRuntimeError> {
        self.ensure_plan(plan)?;
        Ok(u64::from(plan.n_iterations()).saturating_mul(plan.sync_length()))
    }

    pub fn position_samples(
        &self,
        plan: &CompiledCompositePlan,
    ) -> Result<u64, CompositeRuntimeError> {
        self.ensure_plan(plan)?;
        if self.mode == LoopMode::Stopped {
            return Ok(0);
        }
        Ok(u64::from(self.iteration)
            .saturating_mul(plan.sync_length())
            .saturating_add(self.sync_position))
    }

    pub fn set_play_after_record(&mut self, enabled: bool) {
        self.play_after_record = enabled;
    }

    pub fn set_sync_position(
        &mut self,
        plan: &CompiledCompositePlan,
        position: u64,
    ) -> Result<(), CompositeRuntimeError> {
        self.ensure_plan_mut(plan)?;
        if plan.sync_length() == 0 || position >= plan.sync_length() {
            self.bump_arithmetic_overflow();
            return Err(CompositeRuntimeError::InvalidSyncPosition);
        }
        self.sync_position = position;
        Ok(())
    }

    pub fn request_transition(
        &mut self,
        mode: LoopMode,
        delay: u32,
    ) -> Result<(), CompositeRuntimeError> {
        if mode == LoopMode::Unknown {
            self.counters.rejected_modes = self.counters.rejected_modes.saturating_add(1);
            return Err(CompositeRuntimeError::UnknownMode);
        }
        self.pending = Some(PendingCompositeTransition {
            mode,
            boundaries_to_skip: delay,
        });
        Ok(())
    }

    pub fn transition_immediate<F>(
        &mut self,
        plan: &CompiledCompositePlan,
        mode: LoopMode,
        seek_iteration: Option<i64>,
        mut target_is_current: F,
    ) -> Result<CompositeTransitionBatch, CompositeRuntimeError>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        self.ensure_plan_mut(plan)?;
        if mode == LoopMode::Unknown {
            self.counters.rejected_modes = self.counters.rejected_modes.saturating_add(1);
            return Err(CompositeRuntimeError::UnknownMode);
        }
        if mode == LoopMode::Stopped {
            return self.stop_inner(plan, &mut target_is_current);
        }
        if plan.n_iterations() == 0 {
            self.mode = LoopMode::Stopped;
            self.pending = None;
            self.iteration = 0;
            self.sync_position = 0;
            return Ok(CompositeTransitionBatch::default());
        }
        let iteration = seek_iteration.unwrap_or(0);
        if iteration < 0 || iteration >= i64::from(plan.n_iterations()) {
            self.counters.invalid_seeks = self.counters.invalid_seeks.saturating_add(1);
            return Err(CompositeRuntimeError::InvalidSeek);
        }

        self.mode = mode;
        self.pending = None;
        self.iteration = iteration as u32;
        self.reconcile(
            plan,
            Some(self.iteration),
            mode,
            true,
            false,
            &mut target_is_current,
        )
    }

    pub fn seek<F>(
        &mut self,
        plan: &CompiledCompositePlan,
        iteration: i64,
        mut target_is_current: F,
    ) -> Result<CompositeTransitionBatch, CompositeRuntimeError>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        self.ensure_plan_mut(plan)?;
        if self.mode == LoopMode::Stopped
            || iteration < 0
            || iteration >= i64::from(plan.n_iterations())
        {
            self.counters.invalid_seeks = self.counters.invalid_seeks.saturating_add(1);
            return Err(CompositeRuntimeError::InvalidSeek);
        }
        self.iteration = iteration as u32;
        self.reconcile(
            plan,
            Some(self.iteration),
            self.mode,
            true,
            false,
            &mut target_is_current,
        )
    }

    pub fn stop<F>(
        &mut self,
        plan: &CompiledCompositePlan,
        mut target_is_current: F,
    ) -> Result<CompositeTransitionBatch, CompositeRuntimeError>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        self.ensure_plan_mut(plan)?;
        self.stop_inner(plan, &mut target_is_current)
    }

    pub fn sync_boundary<F>(
        &mut self,
        plan: &CompiledCompositePlan,
        mut target_is_current: F,
    ) -> Result<CompositeTransitionBatch, CompositeRuntimeError>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        self.ensure_plan_mut(plan)?;
        self.sync_position = 0;
        if let Some(mut pending) = self.pending {
            if pending.boundaries_to_skip == 0 {
                self.pending = None;
                return self.transition_immediate(plan, pending.mode, None, target_is_current);
            }
            pending.boundaries_to_skip -= 1;
            self.pending = Some(pending);
        }
        self.advance(plan, &mut target_is_current)
    }

    pub fn activate_plan<F>(
        &mut self,
        current_plan: &CompiledCompositePlan,
        candidate: &CompiledCompositePlan,
        mut target_is_current: F,
    ) -> Result<(CompositePlanReplacement, CompositeTransitionBatch), CompositeRuntimeError>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        self.ensure_plan_mut(current_plan)?;
        if candidate.source() != self.source {
            self.bump_plan_mismatch();
            return Err(CompositeRuntimeError::PlanMismatch);
        }
        if self.mode != LoopMode::Stopped {
            return Ok((
                CompositePlanReplacement::DeferredUntilIterationZero,
                CompositeTransitionBatch::default(),
            ));
        }
        for &target in candidate.targets() {
            if !target_is_current(target) {
                self.counters.stale_targets = self.counters.stale_targets.saturating_add(1);
                return Err(CompositeRuntimeError::StalePlanTarget);
            }
        }

        let pending = self.pending;
        let batch = self.reconcile(
            current_plan,
            None,
            LoopMode::Stopped,
            false,
            false,
            &mut target_is_current,
        )?;
        self.install_target_table(candidate);
        self.active.fill(INACTIVE_TARGET);
        self.pending = pending;
        self.iteration = 0;
        self.sync_position = 0;
        self.cycle_count = 0;
        Ok((CompositePlanReplacement::Activated, batch))
    }

    pub fn activate_deferred_at_iteration_zero<F>(
        &mut self,
        current_plan: &CompiledCompositePlan,
        candidate: &CompiledCompositePlan,
        mut target_is_current: F,
    ) -> Result<CompositeTransitionBatch, CompositeRuntimeError>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        self.ensure_plan_mut(current_plan)?;
        if candidate.source() != self.source {
            self.bump_plan_mismatch();
            return Err(CompositeRuntimeError::PlanMismatch);
        }
        if self.mode == LoopMode::Stopped
            || current_plan.n_iterations() == 0
            || self.iteration + 1 != current_plan.n_iterations()
        {
            return Err(CompositeRuntimeError::NotAtIterationZero);
        }
        for &target in candidate.targets() {
            if !target_is_current(target) {
                self.counters.stale_targets = self.counters.stale_targets.saturating_add(1);
                return Err(CompositeRuntimeError::StalePlanTarget);
            }
        }

        let old_kind = current_plan.kind();
        let old_mode = self.mode;
        let mut next_mode = match old_kind {
            CompiledCompositeKind::Script => LoopMode::Stopped,
            CompiledCompositeKind::Regular if is_recording_mode(old_mode) => {
                if self.play_after_record {
                    playback_mode_after_record(old_mode)
                } else {
                    LoopMode::Stopped
                }
            }
            CompiledCompositeKind::Regular => old_mode,
        };
        if candidate.n_iterations() == 0 {
            next_mode = LoopMode::Stopped;
        }

        let mut output = CompositeTransitionBatch::default();
        for old_index in 0..self.target_count {
            if !self.active[old_index].active {
                continue;
            }
            let old_identity = self.installed_targets[old_index];
            let desired = candidate
                .targets()
                .binary_search(&old_identity)
                .ok()
                .and_then(|new_index| {
                    candidate.desired(
                        0,
                        new_index,
                        uses_first_recording_table(candidate.kind(), next_mode),
                    )
                })
                .and_then(|state| effective_target(state, next_mode, 0));
            if desired.is_none() {
                self.emit(
                    &mut output,
                    old_index,
                    CompositeTargetAction::Stop,
                    &mut target_is_current,
                )?;
                self.active[old_index] = INACTIVE_TARGET;
            }
        }

        let old_targets = self.installed_targets;
        let old_active = self.active;
        let old_target_count = self.target_count;
        self.install_target_table(candidate);
        self.active.fill(INACTIVE_TARGET);
        for new_index in 0..self.target_count {
            if let Ok(old_index) =
                old_targets[..old_target_count].binary_search(&self.installed_targets[new_index])
            {
                self.active[new_index] = old_active[old_index];
            }
        }

        self.mode = next_mode;
        self.iteration = 0;
        self.sync_position = 0;
        if old_kind == CompiledCompositeKind::Regular && !is_recording_mode(old_mode) {
            if self.cycle_count == u64::MAX {
                self.bump_arithmetic_overflow();
            } else {
                self.cycle_count += 1;
            }
        }
        if next_mode != LoopMode::Stopped {
            let recorded_children = old_kind == CompiledCompositeKind::Regular
                && is_recording_mode(old_mode)
                && self.play_after_record;
            let starts = self.reconcile(
                candidate,
                Some(0),
                next_mode,
                false,
                recorded_children,
                &mut target_is_current,
            )?;
            if let Err(error) = output.append(&starts) {
                self.counters.output_overflows = self.counters.output_overflows.saturating_add(1);
                self.fault = CompositeRuntimeFault::OutputCapacity;
                return Err(error);
            }
        }
        Ok(output)
    }

    pub fn clear<F>(
        &mut self,
        plan: &CompiledCompositePlan,
        mut target_is_current: F,
    ) -> Result<CompositeTransitionBatch, CompositeRuntimeError>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        self.ensure_plan_mut(plan)?;
        let batch = self.stop_inner(plan, &mut target_is_current)?;
        self.target_count = 0;
        self.installed_targets.fill(EMPTY_IDENTITY);
        self.cycle_count = 0;
        Ok(batch)
    }

    pub fn active_children(&self) -> impl Iterator<Item = ActiveCompositeChild> + '_ {
        self.active[..self.target_count]
            .iter()
            .enumerate()
            .filter_map(|(index, state)| {
                state.active.then_some(ActiveCompositeChild {
                    identity: self.installed_targets[index],
                    mode: state.mode,
                    cycle_offset: state.cycle_offset,
                })
            })
    }

    fn advance<F>(
        &mut self,
        plan: &CompiledCompositePlan,
        target_is_current: &mut F,
    ) -> Result<CompositeTransitionBatch, CompositeRuntimeError>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        if self.mode == LoopMode::Stopped || plan.n_iterations() == 0 {
            return Ok(CompositeTransitionBatch::default());
        }
        let next = self.iteration + 1;
        if next < plan.n_iterations() {
            self.iteration = next;
            return self.reconcile(plan, Some(next), self.mode, false, false, target_is_current);
        }

        match plan.kind() {
            CompiledCompositeKind::Script => {
                let batch = self.reconcile(
                    plan,
                    None,
                    LoopMode::Stopped,
                    false,
                    false,
                    target_is_current,
                )?;
                self.mode = LoopMode::Stopped;
                self.pending = None;
                self.iteration = 0;
                Ok(batch)
            }
            CompiledCompositeKind::Regular if is_recording_mode(self.mode) => {
                self.iteration = 0;
                if self.play_after_record {
                    self.mode = playback_mode_after_record(self.mode);
                    self.reconcile(plan, Some(0), self.mode, false, true, target_is_current)
                } else {
                    let batch = self.reconcile(
                        plan,
                        None,
                        LoopMode::Stopped,
                        false,
                        false,
                        target_is_current,
                    )?;
                    self.mode = LoopMode::Stopped;
                    Ok(batch)
                }
            }
            CompiledCompositeKind::Regular => {
                self.iteration = 0;
                if self.cycle_count == u64::MAX {
                    self.bump_arithmetic_overflow();
                } else {
                    self.cycle_count += 1;
                }
                self.reconcile(plan, Some(0), self.mode, false, false, target_is_current)
            }
        }
    }

    fn stop_inner<F>(
        &mut self,
        plan: &CompiledCompositePlan,
        target_is_current: &mut F,
    ) -> Result<CompositeTransitionBatch, CompositeRuntimeError>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        self.pending = None;
        let batch = self.reconcile(
            plan,
            None,
            LoopMode::Stopped,
            false,
            false,
            target_is_current,
        )?;
        self.mode = LoopMode::Stopped;
        self.iteration = 0;
        self.sync_position = 0;
        Ok(batch)
    }

    fn reconcile<F>(
        &mut self,
        plan: &CompiledCompositePlan,
        desired_iteration: Option<u32>,
        composite_mode: LoopMode,
        force_seek: bool,
        assume_recorded_children_nonempty: bool,
        target_is_current: &mut F,
    ) -> Result<CompositeTransitionBatch, CompositeRuntimeError>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        let mut batch = CompositeTransitionBatch::default();
        let first_recording_only = uses_first_recording_table(plan.kind(), composite_mode);

        for index in 0..self.target_count {
            let desired = desired_iteration
                .and_then(|iteration| plan.desired(iteration, index, first_recording_only))
                .map(|mut state| {
                    if assume_recorded_children_nonempty {
                        state.child_is_empty = false;
                    }
                    state
                })
                .and_then(|state| effective_target(state, composite_mode, self.iteration));
            if self.active[index].active && desired.is_none() {
                self.emit(
                    &mut batch,
                    index,
                    CompositeTargetAction::Stop,
                    target_is_current,
                )?;
                self.active[index] = INACTIVE_TARGET;
            }
        }

        for index in 0..self.target_count {
            let Some(desired) = desired_iteration
                .and_then(|iteration| plan.desired(iteration, index, first_recording_only))
                .map(|mut state| {
                    if assume_recorded_children_nonempty {
                        state.child_is_empty = false;
                    }
                    state
                })
                .and_then(|state| effective_target(state, composite_mode, self.iteration))
            else {
                continue;
            };
            let current = self.active[index];
            let retrigger_composite = current.active
                && self.iteration == 0
                && self.installed_targets[index].kind == LoopTargetKind::Composite;
            let must_emit = !current.active
                || current.mode != desired.mode
                || force_seek
                || retrigger_composite;
            let applied = if must_emit {
                self.emit(
                    &mut batch,
                    index,
                    CompositeTargetAction::SetMode {
                        mode: desired.mode,
                        cycle_offset: desired.cycle_offset,
                        retrigger: force_seek || !current.active || retrigger_composite,
                    },
                    target_is_current,
                )?
            } else {
                true
            };
            self.active[index] = if applied {
                ActiveTarget {
                    active: true,
                    mode: desired.mode,
                    cycle_offset: desired.cycle_offset,
                }
            } else {
                INACTIVE_TARGET
            };
        }

        Ok(batch)
    }

    fn emit<F>(
        &mut self,
        batch: &mut CompositeTransitionBatch,
        target_index: usize,
        action: CompositeTargetAction,
        target_is_current: &mut F,
    ) -> Result<bool, CompositeRuntimeError>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        let target = self.installed_targets[target_index];
        if !target_is_current(target) {
            self.counters.stale_targets = self.counters.stale_targets.saturating_add(1);
            self.active[target_index] = INACTIVE_TARGET;
            return Ok(false);
        }
        if let Err(error) = batch.push(CompositeTargetTransition { target, action }) {
            self.counters.output_overflows = self.counters.output_overflows.saturating_add(1);
            self.fault = CompositeRuntimeFault::OutputCapacity;
            return Err(error);
        }
        Ok(true)
    }

    fn ensure_plan(&self, plan: &CompiledCompositePlan) -> Result<(), CompositeRuntimeError> {
        if plan.source() != self.source
            || plan.targets().len() != self.target_count
            || plan.targets() != &self.installed_targets[..self.target_count]
        {
            Err(CompositeRuntimeError::PlanMismatch)
        } else {
            Ok(())
        }
    }

    fn ensure_plan_mut(
        &mut self,
        plan: &CompiledCompositePlan,
    ) -> Result<(), CompositeRuntimeError> {
        if let Err(error) = self.ensure_plan(plan) {
            self.bump_plan_mismatch();
            Err(error)
        } else {
            Ok(())
        }
    }

    fn install_target_table(&mut self, plan: &CompiledCompositePlan) {
        self.target_count = plan.targets().len();
        self.installed_targets.fill(EMPTY_IDENTITY);
        self.installed_targets[..self.target_count].copy_from_slice(plan.targets());
    }

    fn bump_plan_mismatch(&mut self) {
        self.counters.plan_mismatches = self.counters.plan_mismatches.saturating_add(1);
    }

    fn bump_arithmetic_overflow(&mut self) {
        self.counters.arithmetic_overflows = self.counters.arithmetic_overflows.saturating_add(1);
    }
}

#[derive(Debug, Clone, Copy)]
struct EffectiveTarget {
    mode: LoopMode,
    cycle_offset: u32,
}

fn effective_target(
    desired: CompiledDesiredState,
    composite_mode: LoopMode,
    iteration: u32,
) -> Option<EffectiveTarget> {
    let mode = match desired.mode {
        CompiledChildMode::Inherit => composite_mode,
        CompiledChildMode::Explicit(mode) => mode,
    };
    if mode == LoopMode::Stopped || mode == LoopMode::Unknown {
        return None;
    }
    if desired.child_is_empty && matches!(mode, LoopMode::Playing | LoopMode::PlayingDryThroughWet)
    {
        return None;
    }
    let elapsed = iteration.saturating_sub(desired.start_iteration);
    let cycle_offset = if is_recording_mode(mode) {
        elapsed
    } else {
        elapsed % desired.duration.max(1)
    };
    Some(EffectiveTarget { mode, cycle_offset })
}

fn is_recording_mode(mode: LoopMode) -> bool {
    matches!(mode, LoopMode::Recording | LoopMode::RecordingDryIntoWet)
}

fn uses_first_recording_table(kind: CompiledCompositeKind, mode: LoopMode) -> bool {
    kind == CompiledCompositeKind::Regular && is_recording_mode(mode)
}

fn playback_mode_after_record(mode: LoopMode) -> LoopMode {
    match mode {
        LoopMode::RecordingDryIntoWet => LoopMode::PlayingDryThroughWet,
        _ => LoopMode::Playing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composite_plan::{
        compile_composite_plan, CompositeEntry, CompositePlanDescriptor, CompositePlanLimits,
        CompositeSection, CompositeTimeline, LoopTargetCatalog, LoopTargetMetadata,
    };

    #[tracy_nextest_capture::tracy_capture_test]
    fn cycle_counter_saturates_and_reports_integer_overflow() {
        let source = LoopIdentity {
            slot: 1,
            generation: 1,
            kind: LoopTargetKind::Composite,
        };
        let child = LoopIdentity {
            slot: 2,
            generation: 1,
            kind: LoopTargetKind::Basic,
        };
        let catalog = LoopTargetCatalog::new(vec![
            LoopTargetMetadata {
                identity: source,
                length_samples: 0,
            },
            LoopTargetMetadata {
                identity: child,
                length_samples: 1,
            },
        ])
        .unwrap();
        let descriptor = CompositePlanDescriptor {
            source,
            sync_length: 1,
            timelines: vec![CompositeTimeline {
                sections: vec![CompositeSection {
                    entries: vec![CompositeEntry {
                        target: child,
                        delay: 0,
                        n_cycles: Some(1),
                        mode: None,
                    }],
                }],
            }],
        };
        let plan =
            compile_composite_plan(&descriptor, &catalog, &[], CompositePlanLimits::default())
                .unwrap();
        let mut runtime = CompositeRuntime::new(&plan);
        runtime
            .transition_immediate(&plan, LoopMode::Playing, None, |_| true)
            .unwrap();
        runtime.cycle_count = u64::MAX;

        runtime.sync_boundary(&plan, |_| true).unwrap();
        assert_eq!(runtime.cycle_count(), u64::MAX);
        assert_eq!(runtime.counters().arithmetic_overflows, 1);
    }
}
