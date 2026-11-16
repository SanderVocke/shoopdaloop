use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlannedTransition {
    pub mode: Mode,
    pub skip: u32,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransitionPlan {
    pub now: Option<Mode>,
    pub first: Option<PlannedTransition>,
    pub second: Option<PlannedTransition>,
}
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct MusicalState {
    owner: Option<LoopId>,
    source: Option<LoopId>,
    configured_source: Option<LoopId>,
    pub(super) entries: [Option<PlannedTransition>; 2],
    pub failures: u32,
}
impl<S: Sample> LoopPlanner<S> {
    pub fn transition_plan(&self, target: LoopId) -> (TransitionPlan, u32) {
        let Some(state) = self
            .musical
            .get(target.slot() as usize)
            .filter(|state| state.owner == Some(target))
        else {
            return (TransitionPlan::default(), 0);
        };
        (
            TransitionPlan {
                now: None,
                first: state.entries[0],
                second: state.entries[1],
            },
            state.failures,
        )
    }
    pub(super) fn pending_musical_delay(&self, target: LoopId) -> Option<u32> {
        let (plan, _) = self.transition_plan(target);
        [
            plan.first.map(|s| s.skip),
            plan.second.map(|s| s.skip),
            self.resolver
                .pending_delay(target)
                .and_then(|n| u32::try_from(n).ok()),
        ]
        .into_iter()
        .flatten()
        .min()
    }
    pub fn clear_planned(&mut self, target: LoopId, mask: u8) -> Result<(), Error> {
        self.row(target)?;
        self.resolver.state(target).map_err(runtime_error)?;
        let state = &mut self.musical[target.slot() as usize];
        for index in 0..2 {
            if mask & (1 << index) != 0 {
                state.entries[index] = None;
            }
        }
        if mask == 3 {
            self.resolver.cancel_target(target).map_err(runtime_error)?;
        }
        Ok(())
    }
    fn install_plan(&mut self, target: LoopId, source: Option<LoopId>, plan: TransitionPlan) {
        let configured_source = self
            .resolver
            .topology()
            .node(target)
            .ok()
            .and_then(|n| n.sync_source);
        let state = &mut self.musical[target.slot() as usize];
        if state.owner != Some(target) {
            *state = MusicalState::default();
        }
        state.configured_source = configured_source;
        state.owner = Some(target);
        state.source = source;
        state.entries = [plan.first, plan.second];
    }
    pub(super) fn plan_source(
        &self,
        target: LoopId,
        plan: TransitionPlan,
    ) -> Result<Option<LoopId>, Error> {
        self.row(target)?;
        let node = self
            .resolver
            .topology()
            .node(target)
            .map_err(runtime_error)?;
        if node.kind != LoopKind::Primitive {
            return Err(Error::InvalidState);
        }
        if plan.first.is_none() && plan.second.is_none() {
            return Ok(None);
        }
        let source = node.sync_source.unwrap_or(target);
        let state = self.resolver.state(source).map_err(runtime_error)?;
        if self
            .resolver
            .topology()
            .node(source)
            .map_err(runtime_error)?
            .kind
            != LoopKind::Primitive
            || (source == target && !state.mode.playback_mode() && plan.now != Some(Mode::Playing))
        {
            return Err(Error::InvalidState);
        }
        Ok(Some(source))
    }
    pub fn apply_transition_plan(
        &mut self,
        target: LoopId,
        mut plan: TransitionPlan,
        frame: Frame,
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<(), Error> {
        let node = self
            .resolver
            .topology()
            .node(target)
            .map_err(runtime_error)?;
        if node.sync_source.is_none_or(|source| source == target)
            && !self
                .resolver
                .state(target)
                .map_err(runtime_error)?
                .mode
                .playback_mode()
            && plan.now.is_none()
        {
            if plan.first.is_some() && plan.second.is_some() {
                return Err(Error::InvalidState);
            }
            if let Some(step) = plan.first.or(plan.second) {
                plan.now = Some(step.mode);
                plan.first = None;
                plan.second = None;
            }
        }
        let source = self.plan_source(target, plan)?;
        if let Some(mode) = plan.now {
            self.apply_plan_mode(target, mode, false, frame, io)?;
        }
        self.resolver.cancel_target(target).map_err(runtime_error)?;
        self.install_plan(target, source, plan);
        Ok(())
    }
    fn apply_plan_mode(
        &mut self,
        target: LoopId,
        mode: Mode,
        scheduled: bool,
        frame: Frame,
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<(), Error> {
        if scheduled
            && mode == Mode::Playing
            && self.resolver.state(target).map_err(runtime_error)?.mode == Mode::Playing
        {
            return Ok(());
        }
        let op = match mode {
            Mode::Playing => Immediate::Playback(true),
            Mode::Stopped => Immediate::Playback(false),
            Mode::Recording => Immediate::Record,
            _ => return Err(Error::InvalidParameter),
        };
        self.apply_immediate_capture(target, op, frame, io)
    }
    pub(super) fn install_grab_plan(
        &mut self,
        target: LoopId,
        source: Option<LoopId>,
        finish: Option<crate::grab::Finish>,
    ) -> Result<(), Error> {
        self.resolver.cancel_target(target).map_err(runtime_error)?;
        self.install_plan(
            target,
            source,
            TransitionPlan {
                now: None,
                first: finish.map(|f| PlannedTransition {
                    mode: f.mode,
                    skip: f.delay,
                }),
                second: None,
            },
        );
        Ok(())
    }
    pub(super) fn advance_musical_plans(
        &mut self,
        frame: Frame,
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<(), Error> {
        for slot in 0..self.musical.len() {
            self.musical_pulses[slot] = self
                .resolver
                .id_at_slot(slot as u32)
                .is_some_and(|id| self.resolver.immediate_pulse(id).unwrap_or(false));
        }
        for slot in 0..self.musical.len() {
            let mut state = self.musical[slot];
            let Some(target) = state.owner else { continue };
            if self.resolver.state(target).is_err()
                || self.resolver.topology().node(target).is_err()
            {
                self.musical[slot] = MusicalState::default();
                continue;
            }
            let Some(source) = state.source else { continue };
            if self.rows[slot]
                .is_some_and(|row| row.active.is_some() && (row.failure.is_some() || row.aborted))
                && state.entries.iter().any(Option::is_some)
            {
                self.musical[slot].entries = [None; 2];
                self.musical[slot].failures = state.failures.saturating_add(1);
                continue;
            }
            if self.resolver.state(source).is_err()
                || self
                    .resolver
                    .topology()
                    .node(target)
                    .map_err(runtime_error)?
                    .sync_source
                    != state.configured_source
            {
                self.musical[slot].entries = [None; 2];
                continue;
            }
            if !self.musical_pulses[source.slot() as usize] {
                continue;
            }
            for entry in &mut state.entries {
                let Some(mut step) = *entry else { continue };
                if step.skip > 0 {
                    step.skip -= 1;
                    *entry = Some(step);
                    continue;
                }
                *entry = None;
                if self
                    .apply_plan_mode(target, step.mode, true, frame, io)
                    .is_err()
                {
                    state.entries = [None; 2];
                    state.failures = state.failures.saturating_add(1);
                    break;
                }
            }
            self.musical[slot] = state;
        }
        Ok(())
    }
}
