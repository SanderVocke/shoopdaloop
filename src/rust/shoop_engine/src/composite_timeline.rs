//! Bounded same-sample resolution for compiled composite-loop plans.

use crate::composite_plan::{
    CompiledChildMode, CompiledCompositeKind, CompiledCompositePlan, LoopIdentity, LoopTargetKind,
    MAX_COMPOSITE_BOUNDARY_OUTPUTS,
};
use crate::composite_runtime::{
    CompositeRuntime, CompositeRuntimeError, CompositeTargetAction, CompositeTransitionBatch,
};
use crate::state_mirror::CompositeStateMirror;
use crate::{DefaultPlaybackMode, LoopMode};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

pub const MAX_COMPOSITE_CONTROLS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositeTimelineLimits {
    pub max_composites: usize,
    pub max_primitive_events: usize,
    pub max_intents: usize,
    pub max_event_waves: usize,
    pub max_controls: usize,
    pub max_trace_entries: usize,
}

impl Default for CompositeTimelineLimits {
    fn default() -> Self {
        Self {
            max_composites: 64,
            max_primitive_events: 256,
            max_intents: 16_384,
            max_event_waves: 32,
            max_controls: MAX_COMPOSITE_CONTROLS,
            max_trace_entries: 16_384,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompositeTimelineNode {
    pub plan: CompiledCompositePlan,
    pub sync_source: LoopIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryTargetAction {
    Stop,
    SetMode {
        mode: LoopMode,
        offset_samples: u64,
        retrigger: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryIntentOrigin {
    Direct {
        acceptance_sequence: u64,
    },
    ScriptComposite {
        source: LoopIdentity,
        ordinal: u32,
        authoritative: bool,
    },
    RegularComposite {
        source: LoopIdentity,
        ordinal: u32,
        authoritative: bool,
    },
    Natural {
        source: LoopIdentity,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundaryIntent {
    pub target: LoopIdentity,
    pub action: BoundaryTargetAction,
    pub origin: BoundaryIntentOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundaryTraceEntry {
    pub at_sample: u64,
    pub target: LoopIdentity,
    pub action: BoundaryTargetAction,
    pub winner: BoundaryIntentOrigin,
    pub n_losing_conflicts: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptedTimelineControl {
    pub at_sample: u64,
    pub target: LoopIdentity,
    pub action: BoundaryTargetAction,
    pub acceptance_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompositeTimelineFault {
    #[default]
    None,
    PrimitiveEventCapacity,
    IntentCapacity,
    EventWaveCapacity,
    Runtime,
    SubBlockCapacity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompositeTimelineFaultRecord {
    pub fault: CompositeTimelineFault,
    pub at_sample: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompositeTimelineCounters {
    pub rejected_controls: u64,
    pub late_controls: u64,
    pub conflicts: u64,
    pub primitive_event_overflows: u64,
    pub intent_overflows: u64,
    pub wave_overflows: u64,
    pub trace_overflows: u64,
    pub runtime_errors: u64,
    pub sub_block_overflows: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CompositeTimelineBuildError {
    #[error("a composite timeline capacity is zero or above its hard maximum")]
    InvalidCapacity,
    #[error("the composite timeline exceeds its composite capacity")]
    CompositeCapacity,
    #[error("a composite source identity is duplicated")]
    DuplicateSource,
    #[error("a timeline node source is not a composite identity")]
    SourceIsNotComposite,
    #[error("a composite target is not installed in this timeline")]
    MissingCompositeTarget,
    #[error("the installed composite topology contains a cycle")]
    DependencyCycle,
    #[error("the installed composite topology exceeds the event-wave capacity")]
    EventWaveCapacity,
    #[error("the configured intent capacity cannot hold one maximal boundary")]
    IntentCapacity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CompositeTimelineControlError {
    #[error("the accepted-control storage is full")]
    QueueFull,
    #[error("the composite source is not installed")]
    NoSuchComposite,
    #[error("the control timestamp is already in the past")]
    Late,
    #[error("unknown is not a valid target mode")]
    UnknownMode,
    #[error("the requested composite iteration is outside the installed plan")]
    InvalidSeek,
    #[error("the composite boundary resolver rejected the immediate control")]
    BoundaryFault,
}

#[derive(Debug, Clone)]
struct InstalledComposite {
    plan: Option<CompiledCompositePlan>,
    pending_plan: Option<CompiledCompositePlan>,
    active_version: u64,
    pending_version: Option<u64>,
    sync_source: LoopIdentity,
    runtime: CompositeRuntime,
    state: Arc<CompositeStateMirror>,
}

impl InstalledComposite {
    fn plan(&self) -> &CompiledCompositePlan {
        self.plan
            .as_ref()
            .expect("installed timelines always have an active plan")
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CompositeTimelineNodeState<'a> {
    pub plan: &'a CompiledCompositePlan,
    pub active_version: u64,
    pub pending_version: Option<u64>,
    pub sync_source: LoopIdentity,
    pub runtime: &'a CompositeRuntime,
}

#[derive(Debug, Clone, Copy)]
struct IntentRecord {
    intent: BoundaryIntent,
    resolved: bool,
}

#[derive(Debug, Clone)]
pub struct CompositeBoundaryTimeline {
    nodes: Vec<InstalledComposite>,
    current_identities: Vec<LoopIdentity>,
    working_runtimes: Vec<CompositeRuntime>,
    activated_replacements: Vec<bool>,
    retired_plans: Vec<CompiledCompositePlan>,
    delivered_composites: Vec<bool>,
    delivered_sample: Option<u64>,
    triggered_sources: Vec<LoopIdentity>,
    intents: Vec<IntentRecord>,
    targets: Vec<LoopIdentity>,
    trace: Vec<BoundaryTraceEntry>,
    history_trace: VecDeque<BoundaryTraceEntry>,
    boundary_trace: Vec<BoundaryTraceEntry>,
    controls: [Option<AcceptedTimelineControl>; MAX_COMPOSITE_CONTROLS],
    prepared_primitive_sync_sources: Option<Box<[Option<usize>]>>,
    prepared_version: Option<u64>,
    limits: CompositeTimelineLimits,
    sample_clock: u64,
    fault: CompositeTimelineFaultRecord,
    counters: CompositeTimelineCounters,
}

impl Default for CompositeBoundaryTimeline {
    fn default() -> Self {
        Self::new(Vec::new(), CompositeTimelineLimits::default())
            .expect("the default composite timeline limits are valid")
    }
}

impl CompositeBoundaryTimeline {
    pub fn new(
        nodes: Vec<CompositeTimelineNode>,
        limits: CompositeTimelineLimits,
    ) -> Result<Self, CompositeTimelineBuildError> {
        validate_limits(limits)?;
        if nodes.len() > limits.max_composites {
            return Err(CompositeTimelineBuildError::CompositeCapacity);
        }

        let mut by_source = BTreeMap::new();
        for node in nodes {
            if node.plan.source().kind != LoopTargetKind::Composite {
                return Err(CompositeTimelineBuildError::SourceIsNotComposite);
            }
            let source = node.plan.source();
            let runtime = CompositeRuntime::new(&node.plan);
            if by_source
                .insert(
                    source,
                    InstalledComposite {
                        plan: Some(node.plan),
                        pending_plan: None,
                        active_version: 0,
                        pending_version: None,
                        sync_source: node.sync_source,
                        runtime,
                        state: Arc::new(CompositeStateMirror::new(source)),
                    },
                )
                .is_some()
            {
                return Err(CompositeTimelineBuildError::DuplicateSource);
            }
        }

        let source_set: BTreeSet<_> = by_source.keys().copied().collect();
        for node in by_source.values() {
            if node.sync_source.kind == LoopTargetKind::Composite
                && !source_set.contains(&node.sync_source)
            {
                return Err(CompositeTimelineBuildError::MissingCompositeTarget);
            }
            for target in node
                .plan()
                .targets()
                .iter()
                .filter(|target| target.kind == LoopTargetKind::Composite)
            {
                if !source_set.contains(target) {
                    return Err(CompositeTimelineBuildError::MissingCompositeTarget);
                }
            }
        }

        let (order, depth) = topology_order(&by_source)?;
        if depth > limits.max_event_waves {
            return Err(CompositeTimelineBuildError::EventWaveCapacity);
        }
        let maximal_intents = order
            .len()
            .checked_mul(MAX_COMPOSITE_BOUNDARY_OUTPUTS)
            .and_then(|n| n.checked_add(limits.max_controls))
            .and_then(|n| n.checked_add(limits.max_primitive_events))
            .ok_or(CompositeTimelineBuildError::IntentCapacity)?;
        if maximal_intents > limits.max_intents {
            return Err(CompositeTimelineBuildError::IntentCapacity);
        }

        let mut nodes = Vec::with_capacity(order.len());
        for source in order {
            nodes.push(
                by_source
                    .remove(&source)
                    .ok_or(CompositeTimelineBuildError::DependencyCycle)?,
            );
        }
        let mut current_identities: Vec<_> =
            nodes.iter().map(|node| node.plan().source()).collect();
        current_identities.sort_unstable();
        let working_runtimes = nodes.iter().map(|node| node.runtime.clone()).collect();
        let n_nodes = nodes.len();

        Ok(Self {
            nodes,
            current_identities,
            working_runtimes,
            activated_replacements: vec![false; n_nodes],
            retired_plans: Vec::with_capacity(limits.max_composites),
            delivered_composites: vec![false; n_nodes],
            delivered_sample: None,
            triggered_sources: Vec::with_capacity(
                limits.max_primitive_events + limits.max_composites + limits.max_controls,
            ),
            intents: Vec::with_capacity(limits.max_intents),
            targets: Vec::with_capacity(limits.max_intents),
            trace: Vec::with_capacity(limits.max_trace_entries),
            history_trace: VecDeque::with_capacity(limits.max_trace_entries),
            boundary_trace: Vec::with_capacity(limits.max_trace_entries),
            controls: [None; MAX_COMPOSITE_CONTROLS],
            prepared_primitive_sync_sources: None,
            prepared_version: None,
            limits,
            sample_clock: 0,
            fault: CompositeTimelineFaultRecord {
                fault: CompositeTimelineFault::None,
                at_sample: 0,
            },
            counters: CompositeTimelineCounters::default(),
        })
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn sample_clock(&self) -> u64 {
        self.sample_clock
    }

    pub fn fault(&self) -> CompositeTimelineFaultRecord {
        self.fault
    }

    pub fn counters(&self) -> CompositeTimelineCounters {
        self.counters
    }

    pub fn trace(&self) -> &[BoundaryTraceEntry] {
        &self.trace
    }

    pub fn history_trace(&self) -> impl Iterator<Item = BoundaryTraceEntry> + '_ {
        self.history_trace.iter().copied()
    }

    pub fn n_history_trace_entries(&self) -> usize {
        self.history_trace.len()
    }

    pub fn begin_callback(&mut self) {
        self.trace.clear();
    }

    pub fn max_event_waves(&self) -> usize {
        self.limits.max_event_waves
    }

    pub fn runtime(&self, source: LoopIdentity) -> Option<&CompositeRuntime> {
        self.nodes
            .iter()
            .find(|node| node.plan().source() == source)
            .map(|node| &node.runtime)
    }

    pub fn n_composites(&self) -> usize {
        self.nodes.len()
    }

    /// Replaces the default mirror while the timeline is still prepared off-thread.
    pub fn set_state_mirror(
        &mut self,
        source: LoopIdentity,
        state: Arc<CompositeStateMirror>,
    ) -> bool {
        let Some(node) = self
            .nodes
            .iter_mut()
            .find(|node| node.plan().source() == source)
        else {
            return false;
        };
        node.state = state;
        true
    }

    pub fn state_mirror(&self, source: LoopIdentity) -> Option<&Arc<CompositeStateMirror>> {
        self.nodes
            .iter()
            .find(|node| node.plan().source() == source)
            .map(|node| &node.state)
    }

    /// Marks mirrors whose identities disappeared from a replacement as uninstalled.
    pub fn mark_mirrors_removed_by(&self, replacement: &Self) {
        for node in &self.nodes {
            if !replacement.is_current_composite(node.plan().source()) {
                node.state.mark_uninstalled();
            }
        }
    }

    /// Publishes all frontend-visible composite runtime state without locking or allocating.
    pub fn publish_state_mirrors(&self, mut is_current_target: impl FnMut(LoopIdentity) -> bool) {
        for node in &self.nodes {
            let source = node.plan().source();
            let pending = node.runtime.pending();
            let anticipated = pending
                .map(|pending| (pending.mode, pending.boundaries_to_skip))
                .or_else(|| self.anticipated_transition(source));
            node.state.publish(
                node.sync_source,
                node.active_version,
                node.pending_version,
                node.runtime.mode(),
                anticipated,
                node.runtime.iteration(),
                node.runtime.cycle_count(),
                node.runtime.length_samples(node.plan()).unwrap_or(0),
                node.runtime.position_samples(node.plan()).unwrap_or(0),
                node.runtime.play_after_record(),
                node.runtime.counters(),
                node.runtime.fault(),
                node.runtime
                    .active_children()
                    .filter(|child| is_current_target(child.identity)),
            );
        }
    }

    pub fn n_retired_plans(&self) -> usize {
        self.retired_plans.len()
    }

    pub(crate) fn reclaim_retired_plans(
        &mut self,
        mut storage: Vec<CompiledCompositePlan>,
    ) -> Vec<CompiledCompositePlan> {
        if storage.is_empty() && storage.capacity() >= self.retired_plans.len() {
            std::mem::swap(&mut storage, &mut self.retired_plans);
        }
        storage
    }

    pub fn replacement_requires_runtime_transfer(&self) -> bool {
        self.nodes.iter().any(|node| {
            node.runtime.mode() != LoopMode::Stopped || node.runtime.pending().is_some()
        })
    }

    pub fn active_primitive_children(&self) -> impl Iterator<Item = LoopIdentity> + '_ {
        self.nodes.iter().flat_map(|node| {
            node.runtime
                .active_children()
                .map(|child| child.identity)
                .filter(|identity| identity.kind == LoopTargetKind::Basic)
        })
    }

    fn n_controls(&self) -> usize {
        self.controls
            .iter()
            .filter(|control| control.is_some())
            .count()
    }

    pub fn can_restart_with_changed_topology(&self, candidate: &Self) -> bool {
        let retained_running = self
            .nodes
            .iter()
            .filter(|current| {
                current.runtime.mode() != LoopMode::Stopped
                    && candidate.nodes.iter().any(|next| {
                        next.plan().source() == current.plan().source()
                            && next.plan().n_iterations() > 0
                    })
            })
            .count();
        self.n_controls().saturating_add(retained_running) <= candidate.limits.max_controls
    }

    fn inherit_timeline_state(&mut self, previous: &mut Self) {
        self.sample_clock = previous.sample_clock;
        self.fault = previous.fault;
        self.counters = previous.counters;
        self.controls = previous.controls;
        previous.controls = [None; MAX_COMPOSITE_CONTROLS];
        std::mem::swap(&mut self.history_trace, &mut previous.history_trace);
    }

    pub fn prepare_changed_topology_restart(
        &mut self,
        previous: &mut Self,
        acceptance_sequence: &mut u64,
    ) {
        debug_assert!(previous.can_restart_with_changed_topology(self));
        self.inherit_timeline_state(previous);
        for current in &previous.nodes {
            let source = current.plan().source();
            let mode = current.runtime.mode();
            let should_restart = {
                let Some(next) = self
                    .nodes
                    .iter_mut()
                    .find(|candidate| candidate.plan().source() == source)
                else {
                    continue;
                };
                next.runtime
                    .set_play_after_record(current.runtime.play_after_record());
                mode != LoopMode::Stopped && next.plan().n_iterations() > 0
            };
            if should_restart {
                self.queue_control(AcceptedTimelineControl {
                    at_sample: self.sample_clock,
                    target: source,
                    action: BoundaryTargetAction::SetMode {
                        mode,
                        offset_samples: 0,
                        retrigger: true,
                    },
                    acceptance_sequence: *acceptance_sequence,
                })
                .expect("changed-topology restart capacity was checked");
                *acceptance_sequence = acceptance_sequence.saturating_add(1);
            }
        }
    }

    pub fn prepare_stopped_replacement(&mut self, previous: &mut Self) {
        debug_assert!(!previous.replacement_requires_runtime_transfer());
        self.inherit_timeline_state(previous);
    }

    pub fn can_queue_runtime_preserving_replacement(&self, candidate: &Self) -> bool {
        if self.nodes.len() != candidate.nodes.len() {
            return false;
        }
        if !self
            .nodes
            .iter()
            .zip(&candidate.nodes)
            .all(|(current, next)| {
                current.plan().source() == next.plan().source()
                    && current.sync_source == next.sync_source
                    && composite_targets(current.plan()).eq(composite_targets(next.plan()))
            })
        {
            return false;
        }
        let additional_retirements = self
            .nodes
            .iter()
            .zip(&candidate.nodes)
            .filter(|(current, next)| {
                current.plan() != next.plan()
                    && current.runtime.mode() != LoopMode::Stopped
                    && current.pending_plan.is_none()
                    && !current
                        .runtime
                        .can_adopt_plan_before_future_change(current.plan(), next.plan())
            })
            .count();
        self.retired_plans.len() + additional_retirements <= self.retired_plans.capacity()
    }

    pub fn queue_runtime_preserving_replacement(&mut self, mut candidate: Self) -> Self {
        for (current, next) in self.nodes.iter_mut().zip(candidate.nodes.iter_mut()) {
            if current.plan() == next.plan() {
                current.active_version = next.active_version;
                continue;
            }
            if current.runtime.mode() == LoopMode::Stopped {
                let current_plan = current
                    .plan
                    .as_ref()
                    .expect("installed timelines always have an active plan");
                let next_plan = next
                    .plan
                    .as_ref()
                    .expect("prepared replacement nodes have a plan");
                let _ = current
                    .runtime
                    .activate_plan(current_plan, next_plan, |_| true);
                std::mem::swap(&mut current.plan, &mut next.plan);
                std::mem::swap(&mut current.active_version, &mut next.active_version);
            } else if current.pending_plan.is_none()
                && current
                    .runtime
                    .adopt_plan_before_future_change(
                        current
                            .plan
                            .as_ref()
                            .expect("installed timelines always have an active plan"),
                        next.plan
                            .as_ref()
                            .expect("prepared replacement nodes have a plan"),
                    )
                    .expect("replacement plans have matching composite identities")
            {
                std::mem::swap(&mut current.plan, &mut next.plan);
                std::mem::swap(&mut current.active_version, &mut next.active_version);
            } else {
                let next_plan = next
                    .plan
                    .take()
                    .expect("prepared replacement nodes have a plan");
                next.plan = current.pending_plan.replace(next_plan);
                let next_version = next.active_version;
                next.active_version = current.pending_version.replace(next_version).unwrap_or(0);
            }
        }
        std::mem::swap(
            &mut self.prepared_primitive_sync_sources,
            &mut candidate.prepared_primitive_sync_sources,
        );
        self.prepared_version = candidate.prepared_version;
        candidate
    }

    pub fn node_state(&self, index: usize) -> Option<CompositeTimelineNodeState<'_>> {
        self.nodes
            .get(index)
            .map(|node| CompositeTimelineNodeState {
                plan: node.plan(),
                active_version: node.active_version,
                pending_version: node.pending_version,
                sync_source: node.sync_source,
                runtime: &node.runtime,
            })
    }

    pub fn anticipated_transition(&self, target: LoopIdentity) -> Option<(LoopMode, u32)> {
        self.anticipated_transition_with_default_playback(target, |_| DefaultPlaybackMode::Regular)
    }

    pub fn anticipated_transition_with_default_playback<D>(
        &self,
        target: LoopIdentity,
        mut primitive_default_playback: D,
    ) -> Option<(LoopMode, u32)>
    where
        D: FnMut(LoopIdentity) -> DefaultPlaybackMode,
    {
        let mut node_modes = [None; MAX_COMPOSITE_CONTROLS];
        let mut anticipated = None;
        for (node_index, node) in self.nodes.iter().enumerate() {
            let inherited = node
                .runtime
                .pending()
                .map(|pending| (pending.mode, pending.boundaries_to_skip));
            let Some((composite_mode, delay)) =
                inherited.or_else(|| node_modes.get(node_index).copied().flatten())
            else {
                continue;
            };
            let first_recording_only = node.plan().kind() == CompiledCompositeKind::Regular
                && matches!(
                    composite_mode,
                    LoopMode::Recording | LoopMode::RecordingDryIntoWet
                );
            for (target_index, child) in node.plan().targets().iter().copied().enumerate() {
                let Some(desired) = node.plan().desired(0, target_index, first_recording_only)
                else {
                    continue;
                };
                let mode = match desired.mode {
                    CompiledChildMode::DefaultPlayback => {
                        if child.kind == LoopTargetKind::Composite {
                            LoopMode::Playing
                        } else {
                            primitive_default_playback(child).loop_mode()
                        }
                    }
                    CompiledChildMode::Explicit(mode) => mode,
                };
                if desired.child_is_empty
                    && !matches!(mode, LoopMode::Recording | LoopMode::RecordingDryIntoWet)
                {
                    continue;
                }
                if child == target {
                    anticipated = Some((mode, delay));
                }
                if child.kind == LoopTargetKind::Composite {
                    if let Some(child_index) = self
                        .nodes
                        .iter()
                        .position(|candidate| candidate.plan().source() == child)
                    {
                        if let Some(slot) = node_modes.get_mut(child_index) {
                            *slot = Some((mode, delay));
                        }
                    }
                }
            }
        }
        if anticipated.is_none() {
            for node in &self.nodes {
                let mode = node.runtime.mode();
                if mode == LoopMode::Stopped || node.plan().n_iterations() == 0 {
                    continue;
                }
                let next_iteration = if node.runtime.iteration() + 1 < node.plan().n_iterations() {
                    Some(node.runtime.iteration() + 1)
                } else if node.plan().kind() == CompiledCompositeKind::Regular {
                    Some(0)
                } else {
                    None
                };
                let current = node
                    .runtime
                    .active_children()
                    .find(|child| child.identity == target);
                let desired = node
                    .plan()
                    .targets()
                    .binary_search(&target)
                    .ok()
                    .and_then(|target_index| {
                        next_iteration.and_then(|iteration| {
                            node.plan().desired(iteration, target_index, false)
                        })
                    })
                    .and_then(|desired| {
                        let mode = match desired.mode {
                            CompiledChildMode::DefaultPlayback => {
                                if target.kind == LoopTargetKind::Composite {
                                    LoopMode::Playing
                                } else {
                                    primitive_default_playback(target).loop_mode()
                                }
                            }
                            CompiledChildMode::Explicit(mode) => mode,
                        };
                        (!desired.child_is_empty
                            || matches!(mode, LoopMode::Recording | LoopMode::RecordingDryIntoWet))
                        .then_some(mode)
                    });
                anticipated = match (current, desired) {
                    (Some(_), None) => Some((LoopMode::Stopped, 0)),
                    (None, Some(mode)) => Some((mode, 0)),
                    (Some(current), Some(mode)) if current.mode != mode => Some((mode, 0)),
                    _ => anticipated,
                };
            }
        }
        anticipated
    }

    pub fn is_current_composite(&self, identity: LoopIdentity) -> bool {
        self.current_identities.binary_search(&identity).is_ok()
    }

    pub fn first_invalid_primitive<F>(&self, mut is_current: F) -> Option<LoopIdentity>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        self.nodes.iter().find_map(|node| {
            if node.sync_source.kind == LoopTargetKind::Basic && !is_current(node.sync_source) {
                return Some(node.sync_source);
            }
            node.plan()
                .targets()
                .iter()
                .copied()
                .find(|target| target.kind == LoopTargetKind::Basic && !is_current(*target))
        })
    }

    pub fn prepare_primitive_sync_sources(
        &mut self,
        sync_sources: &[Option<usize>],
    ) -> Result<(), CompositeTimelineBuildError> {
        self.validate_primitive_sync_sources(sync_sources)?;
        self.prepared_primitive_sync_sources = Some(sync_sources.to_vec().into_boxed_slice());
        Ok(())
    }

    pub fn prepare_install(
        &mut self,
        version: u64,
        sync_sources: &[Option<usize>],
    ) -> Result<(), CompositeTimelineBuildError> {
        self.prepare_primitive_sync_sources(sync_sources)?;
        self.prepared_version = Some(version);
        for node in &mut self.nodes {
            node.active_version = version;
        }
        Ok(())
    }

    pub fn prepared_version(&self) -> Option<u64> {
        self.prepared_version
    }

    pub fn matches_prepared_primitive_sync_sources(&self, sync_sources: &[Option<usize>]) -> bool {
        self.prepared_primitive_sync_sources.as_deref() == Some(sync_sources)
    }

    pub fn validate_primitive_sync_sources(
        &self,
        sync_sources: &[Option<usize>],
    ) -> Result<(), CompositeTimelineBuildError> {
        let mut identities = BTreeSet::new();
        let mut edges: BTreeMap<LoopIdentity, BTreeSet<LoopIdentity>> = BTreeMap::new();
        let basic_identity = |slot: usize| LoopIdentity {
            slot: slot as u32,
            generation: 1,
            kind: LoopTargetKind::Basic,
        };
        for (follower, source) in sync_sources.iter().copied().enumerate() {
            let follower = basic_identity(follower);
            identities.insert(follower);
            if let Some(source) = source {
                let source = basic_identity(source);
                identities.insert(source);
                edges.entry(source).or_default().insert(follower);
            }
        }
        for node in &self.nodes {
            let source = node.plan().source();
            identities.insert(source);
            identities.insert(node.sync_source);
            edges.entry(node.sync_source).or_default().insert(source);
            for &target in node.plan().targets() {
                identities.insert(target);
                edges.entry(source).or_default().insert(target);
            }
        }
        let depth = stable_graph_depth(&identities, &edges)?;
        if depth > self.limits.max_event_waves {
            Err(CompositeTimelineBuildError::EventWaveCapacity)
        } else {
            Ok(())
        }
    }

    pub fn request_transition(
        &mut self,
        source: LoopIdentity,
        mode: LoopMode,
        delay: u32,
    ) -> Result<(), CompositeTimelineControlError> {
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.plan().source() == source)
            .ok_or(CompositeTimelineControlError::NoSuchComposite)?;
        node.runtime
            .request_transition(mode, delay)
            .map_err(|_| CompositeTimelineControlError::UnknownMode)
    }

    pub fn queue_immediate_transition(
        &mut self,
        source: LoopIdentity,
        mode: LoopMode,
        iteration: i64,
        acceptance_sequence: u64,
    ) -> Result<(), CompositeTimelineControlError> {
        let node = self
            .nodes
            .iter()
            .find(|node| node.plan().source() == source)
            .ok_or(CompositeTimelineControlError::NoSuchComposite)?;
        if mode == LoopMode::Unknown {
            return Err(CompositeTimelineControlError::UnknownMode);
        }
        if mode != LoopMode::Stopped
            && (iteration < 0 || iteration >= i64::from(node.plan().n_iterations()))
        {
            return Err(CompositeTimelineControlError::InvalidSeek);
        }
        let offset_samples = if mode == LoopMode::Stopped {
            0
        } else {
            (iteration as u64)
                .checked_mul(node.plan().sync_length())
                .ok_or(CompositeTimelineControlError::InvalidSeek)?
        };
        self.queue_control(AcceptedTimelineControl {
            at_sample: self.sample_clock,
            target: source,
            action: BoundaryTargetAction::SetMode {
                mode,
                offset_samples,
                retrigger: true,
            },
            acceptance_sequence,
        })
    }

    pub fn set_play_after_record(
        &mut self,
        source: LoopIdentity,
        enabled: bool,
    ) -> Result<(), CompositeTimelineControlError> {
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.plan().source() == source)
            .ok_or(CompositeTimelineControlError::NoSuchComposite)?;
        node.runtime.set_play_after_record(enabled);
        Ok(())
    }

    pub fn queue_control(
        &mut self,
        control: AcceptedTimelineControl,
    ) -> Result<(), CompositeTimelineControlError> {
        if matches!(
            control.action,
            BoundaryTargetAction::SetMode {
                mode: LoopMode::Unknown,
                ..
            }
        ) {
            self.counters.rejected_controls = self.counters.rejected_controls.saturating_add(1);
            return Err(CompositeTimelineControlError::UnknownMode);
        }
        if control.at_sample < self.sample_clock {
            self.counters.late_controls = self.counters.late_controls.saturating_add(1);
            return Err(CompositeTimelineControlError::Late);
        }
        let Some(slot) = self.controls[..self.limits.max_controls]
            .iter_mut()
            .find(|slot| slot.is_none())
        else {
            self.counters.rejected_controls = self.counters.rejected_controls.saturating_add(1);
            return Err(CompositeTimelineControlError::QueueFull);
        };
        *slot = Some(control);
        Ok(())
    }

    pub fn next_control_poi(&self, max_samples: usize) -> Option<usize> {
        self.controls[..self.limits.max_controls]
            .iter()
            .flatten()
            .filter_map(|control| {
                control
                    .at_sample
                    .checked_sub(self.sample_clock)
                    .and_then(|distance| usize::try_from(distance).ok())
            })
            .filter(|distance| *distance < max_samples)
            .min()
    }

    pub fn advance_clock(&mut self, n_samples: usize) {
        self.sample_clock = self.sample_clock.saturating_add(n_samples as u64);
    }

    pub fn align_sync_positions<F>(&mut self, mut primitive_position: F)
    where
        F: FnMut(LoopIdentity) -> Option<u64>,
    {
        if self.fault.fault != CompositeTimelineFault::None {
            return;
        }
        for index in 0..self.nodes.len() {
            let sync_source = self.nodes[index].sync_source;
            let position = if sync_source.kind == LoopTargetKind::Basic {
                primitive_position(sync_source)
            } else {
                self.nodes
                    .iter()
                    .find(|node| node.plan().source() == sync_source)
                    .map(|node| node.runtime.sync_position())
            };
            let sync_length = self.nodes[index].plan().sync_length();
            if sync_length > 0 {
                if let Some(position) = position {
                    let node = &mut self.nodes[index];
                    let plan = node
                        .plan
                        .as_ref()
                        .expect("installed timelines always have an active plan");
                    let _ = node.runtime.set_sync_position(plan, position % sync_length);
                }
            }
        }
    }

    pub fn resolve_boundary<F>(
        &mut self,
        primitive_triggers: &[LoopIdentity],
        natural_intents: &[BoundaryIntent],
        primitive_is_current: F,
    ) -> Result<&[BoundaryTraceEntry], CompositeTimelineFaultRecord>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        self.resolve_boundary_with_default_playback(
            primitive_triggers,
            natural_intents,
            primitive_is_current,
            |_| DefaultPlaybackMode::Regular,
        )
    }

    pub fn resolve_boundary_with_default_playback<F, D>(
        &mut self,
        primitive_triggers: &[LoopIdentity],
        natural_intents: &[BoundaryIntent],
        mut primitive_is_current: F,
        mut primitive_default_playback: D,
    ) -> Result<&[BoundaryTraceEntry], CompositeTimelineFaultRecord>
    where
        F: FnMut(LoopIdentity) -> bool,
        D: FnMut(LoopIdentity) -> DefaultPlaybackMode,
    {
        if self.fault.fault != CompositeTimelineFault::None {
            return Err(self.fault);
        }
        if self.delivered_sample != Some(self.sample_clock) {
            self.delivered_composites.fill(false);
            self.delivered_sample = Some(self.sample_clock);
        }
        let trace_start = self.trace.len();
        self.boundary_trace.clear();
        self.activated_replacements.fill(false);
        self.intents.clear();
        self.targets.clear();
        self.triggered_sources.clear();

        if primitive_triggers.len() > self.limits.max_primitive_events {
            self.counters.primitive_event_overflows =
                self.counters.primitive_event_overflows.saturating_add(1);
            return Err(self.latch(CompositeTimelineFault::PrimitiveEventCapacity));
        }
        self.triggered_sources.extend_from_slice(primitive_triggers);
        self.triggered_sources.sort_unstable();
        self.triggered_sources.dedup();

        for runtime_index in 0..self.nodes.len() {
            self.working_runtimes[runtime_index].clone_from(&self.nodes[runtime_index].runtime);
        }

        for &intent in natural_intents {
            self.push_intent(intent)?;
        }
        for index in 0..self.limits.max_controls {
            let Some(control) = self.controls[index] else {
                continue;
            };
            if control.at_sample < self.sample_clock {
                self.controls[index] = None;
                self.counters.late_controls = self.counters.late_controls.saturating_add(1);
                continue;
            }
            if control.at_sample == self.sample_clock {
                self.controls[index] = None;
                self.push_intent(BoundaryIntent {
                    target: control.target,
                    action: control.action,
                    origin: BoundaryIntentOrigin::Direct {
                        acceptance_sequence: control.acceptance_sequence,
                    },
                })?;
            }
        }

        for node_index in 0..self.nodes.len() {
            let sync_source = self.nodes[node_index].sync_source;
            if sync_source.kind == LoopTargetKind::Basic {
                if let Some(intent) = self.resolve_target(sync_source)? {
                    match intent.action {
                        BoundaryTargetAction::Stop => {
                            if let Ok(index) = self.triggered_sources.binary_search(&sync_source) {
                                self.triggered_sources.remove(index);
                            }
                        }
                        BoundaryTargetAction::SetMode {
                            mode: LoopMode::Stopped | LoopMode::Unknown,
                            ..
                        } if !matches!(intent.origin, BoundaryIntentOrigin::Natural { .. }) => {
                            if let Ok(index) = self.triggered_sources.binary_search(&sync_source) {
                                self.triggered_sources.remove(index);
                            }
                        }
                        BoundaryTargetAction::SetMode {
                            mode: LoopMode::Stopped | LoopMode::Unknown,
                            ..
                        } => {}
                        BoundaryTargetAction::SetMode { .. } => self.mark_triggered(sync_source)?,
                    }
                }
            }

            let source = self.nodes[node_index].plan().source();
            let incoming = self.resolve_target(source)?;
            let mut controlled = false;
            if let Some(intent) = incoming {
                controlled = true;
                self.delivered_composites[node_index] = true;
                let batch = self.apply_to_composite(
                    node_index,
                    intent.action,
                    intent_is_authoritative(intent.origin),
                    &mut primitive_is_current,
                )?;
                self.append_batch(node_index, &batch, &mut primitive_default_playback)?;
                if matches!(
                    intent.action,
                    BoundaryTargetAction::Stop
                        | BoundaryTargetAction::SetMode {
                            mode: LoopMode::Stopped,
                            ..
                        }
                ) && self.nodes[node_index].pending_plan.is_some()
                {
                    let activation =
                        self.activate_stopped_replacement(node_index, &mut primitive_is_current)?;
                    self.activated_replacements[node_index] = true;
                    self.append_batch(node_index, &activation, &mut primitive_default_playback)?;
                }
                if matches!(intent.action, BoundaryTargetAction::SetMode { .. })
                    && self.working_runtimes[node_index].mode() != LoopMode::Stopped
                {
                    self.mark_triggered(source)?;
                }
            }

            if !controlled
                && !self.delivered_composites[node_index]
                && self.source_triggered(self.nodes[node_index].sync_source)
            {
                self.delivered_composites[node_index] = true;
                let was_eligible = self.working_runtimes[node_index].mode() != LoopMode::Stopped
                    || self.working_runtimes[node_index].pending().is_some();
                let replacement_due = self.nodes[node_index].pending_plan.is_some()
                    && self.working_runtimes[node_index].mode() != LoopMode::Stopped
                    && self.working_runtimes[node_index]
                        .iteration()
                        .saturating_add(1)
                        == self.nodes[node_index].plan().n_iterations();
                let batch = if replacement_due {
                    let batch =
                        self.activate_running_replacement(node_index, &mut primitive_is_current)?;
                    self.activated_replacements[node_index] = true;
                    batch
                } else {
                    let current_ids = &self.current_identities;
                    let plan = self.nodes[node_index].plan();
                    self.working_runtimes[node_index]
                        .sync_boundary(plan, |identity| {
                            if identity.kind == LoopTargetKind::Composite {
                                current_ids.binary_search(&identity).is_ok()
                            } else {
                                primitive_is_current(identity)
                            }
                        })
                        .map_err(|_| self.runtime_fault())?
                };
                self.append_batch(node_index, &batch, &mut primitive_default_playback)?;
                if was_eligible {
                    self.mark_triggered(source)?;
                }
            }
        }

        for record in &self.intents {
            if !record.resolved && record.intent.target.kind == LoopTargetKind::Basic {
                self.targets.push(record.intent.target);
            }
        }
        self.targets.sort_unstable();
        self.targets.dedup();
        for target_index in 0..self.targets.len() {
            let target = self.targets[target_index];
            let _ = self.resolve_target(target)?;
        }

        let available = self
            .limits
            .max_trace_entries
            .saturating_sub(self.trace.len());
        let retained = available.min(self.boundary_trace.len());
        self.trace
            .extend_from_slice(&self.boundary_trace[..retained]);
        for entry in self.boundary_trace.iter().copied() {
            if self.history_trace.len() == self.limits.max_trace_entries {
                self.history_trace.pop_front();
            }
            self.history_trace.push_back(entry);
        }
        self.counters.trace_overflows = self
            .counters
            .trace_overflows
            .saturating_add((self.boundary_trace.len() - retained) as u64);
        for index in 0..self.nodes.len() {
            if self.activated_replacements[index] {
                let node = &mut self.nodes[index];
                std::mem::swap(&mut node.plan, &mut node.pending_plan);
                let old_plan = node
                    .pending_plan
                    .take()
                    .expect("an activated replacement retains its old plan");
                node.active_version = node
                    .pending_version
                    .take()
                    .expect("an activated replacement retains its candidate version");
                self.retired_plans.push(old_plan);
            }
            std::mem::swap(
                &mut self.nodes[index].runtime,
                &mut self.working_runtimes[index],
            );
        }
        Ok(&self.trace[trace_start..])
    }

    pub fn latch_event_wave_overflow(&mut self) -> CompositeTimelineFaultRecord {
        self.counters.wave_overflows = self.counters.wave_overflows.saturating_add(1);
        self.latch(CompositeTimelineFault::EventWaveCapacity)
    }

    pub fn latch_sub_block_overflow(&mut self) -> CompositeTimelineFaultRecord {
        self.counters.sub_block_overflows = self.counters.sub_block_overflows.saturating_add(1);
        self.latch(CompositeTimelineFault::SubBlockCapacity)
    }

    pub fn reset_fault(&mut self) {
        self.fault = CompositeTimelineFaultRecord {
            fault: CompositeTimelineFault::None,
            at_sample: self.sample_clock,
        };
    }

    fn activate_running_replacement<F>(
        &mut self,
        node_index: usize,
        primitive_is_current: &mut F,
    ) -> Result<CompositeTransitionBatch, CompositeTimelineFaultRecord>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        let current_ids = &self.current_identities;
        let node = &self.nodes[node_index];
        let current_plan = node.plan();
        let candidate = node
            .pending_plan
            .as_ref()
            .expect("replacement activation requires a pending plan");
        let result = self.working_runtimes[node_index].activate_deferred_at_iteration_zero(
            current_plan,
            candidate,
            |identity| {
                if identity.kind == LoopTargetKind::Composite {
                    current_ids.binary_search(&identity).is_ok()
                } else {
                    primitive_is_current(identity)
                }
            },
        );
        result.map_err(|_| self.runtime_fault())
    }

    fn activate_stopped_replacement<F>(
        &mut self,
        node_index: usize,
        primitive_is_current: &mut F,
    ) -> Result<CompositeTransitionBatch, CompositeTimelineFaultRecord>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        let current_ids = &self.current_identities;
        let node = &self.nodes[node_index];
        let current_plan = node.plan();
        let candidate = node
            .pending_plan
            .as_ref()
            .expect("replacement activation requires a pending plan");
        let result =
            self.working_runtimes[node_index].activate_plan(current_plan, candidate, |identity| {
                if identity.kind == LoopTargetKind::Composite {
                    current_ids.binary_search(&identity).is_ok()
                } else {
                    primitive_is_current(identity)
                }
            });
        match result {
            Ok((_, batch)) => Ok(batch),
            Err(_) => Err(self.runtime_fault()),
        }
    }

    fn apply_to_composite<F>(
        &mut self,
        node_index: usize,
        action: BoundaryTargetAction,
        authoritative: bool,
        primitive_is_current: &mut F,
    ) -> Result<CompositeTransitionBatch, CompositeTimelineFaultRecord>
    where
        F: FnMut(LoopIdentity) -> bool,
    {
        let current_ids = &self.current_identities;
        let is_current = |identity: LoopIdentity| {
            if identity.kind == LoopTargetKind::Composite {
                current_ids.binary_search(&identity).is_ok()
            } else {
                primitive_is_current(identity)
            }
        };
        let plan = self.nodes[node_index].plan();
        let result: Result<_, CompositeRuntimeError> = match action {
            BoundaryTargetAction::Stop => self.working_runtimes[node_index].stop(plan, is_current),
            BoundaryTargetAction::SetMode {
                mode,
                offset_samples,
                ..
            } => {
                let sync_length = plan.sync_length();
                let n_iterations = u64::from(plan.n_iterations());
                let iteration = if sync_length == 0 || n_iterations == 0 {
                    None
                } else {
                    Some((offset_samples / sync_length) as i64)
                };
                if authoritative {
                    self.working_runtimes[node_index]
                        .transition_immediate(plan, mode, iteration, is_current)
                } else {
                    self.working_runtimes[node_index]
                        .transition_immediate_delta(plan, mode, iteration, is_current)
                }
            }
        };
        result.map_err(|_| self.runtime_fault())
    }

    fn append_batch<D>(
        &mut self,
        node_index: usize,
        batch: &CompositeTransitionBatch,
        primitive_default_playback: &mut D,
    ) -> Result<(), CompositeTimelineFaultRecord>
    where
        D: FnMut(LoopIdentity) -> DefaultPlaybackMode,
    {
        let plan = if self.activated_replacements[node_index] {
            self.nodes[node_index]
                .pending_plan
                .as_ref()
                .expect("activated replacement has a candidate plan")
        } else {
            self.nodes[node_index].plan()
        };
        let source = plan.source();
        let kind = plan.kind();
        let sync_length = plan.sync_length();
        let sync_position = self.working_runtimes[node_index].sync_position();
        for (ordinal, transition) in batch.as_slice().iter().enumerate() {
            let action = match transition.action {
                CompositeTargetAction::Stop => BoundaryTargetAction::Stop,
                CompositeTargetAction::DefaultPlayback {
                    cycle_offset,
                    retrigger,
                } => {
                    let mode = if transition.target.kind == LoopTargetKind::Composite {
                        LoopMode::Playing
                    } else {
                        primitive_default_playback(transition.target).loop_mode()
                    };
                    self.working_runtimes[node_index]
                        .latch_default_playback_mode(transition.target, mode);
                    BoundaryTargetAction::SetMode {
                        mode,
                        offset_samples: u64::from(cycle_offset)
                            .saturating_mul(sync_length)
                            .saturating_add(sync_position),
                        retrigger,
                    }
                }
                CompositeTargetAction::SetMode {
                    mode,
                    cycle_offset,
                    retrigger,
                } => BoundaryTargetAction::SetMode {
                    mode,
                    offset_samples: u64::from(cycle_offset)
                        .saturating_mul(sync_length)
                        .saturating_add(sync_position),
                    retrigger,
                },
            };
            let origin = match kind {
                CompiledCompositeKind::Script => BoundaryIntentOrigin::ScriptComposite {
                    source,
                    ordinal: ordinal as u32,
                    authoritative: transition.authoritative,
                },
                CompiledCompositeKind::Regular => BoundaryIntentOrigin::RegularComposite {
                    source,
                    ordinal: ordinal as u32,
                    authoritative: transition.authoritative,
                },
            };
            self.push_intent(BoundaryIntent {
                target: transition.target,
                action,
                origin,
            })?;
        }
        Ok(())
    }

    fn push_intent(&mut self, intent: BoundaryIntent) -> Result<(), CompositeTimelineFaultRecord> {
        if self.intents.len() >= self.limits.max_intents {
            self.counters.intent_overflows = self.counters.intent_overflows.saturating_add(1);
            return Err(self.latch(CompositeTimelineFault::IntentCapacity));
        }
        self.intents.push(IntentRecord {
            intent,
            resolved: false,
        });
        Ok(())
    }

    fn resolve_target(
        &mut self,
        target: LoopIdentity,
    ) -> Result<Option<BoundaryIntent>, CompositeTimelineFaultRecord> {
        let mut winner_index: Option<usize> = None;
        for index in 0..self.intents.len() {
            if self.intents[index].resolved || self.intents[index].intent.target != target {
                continue;
            }
            winner_index = Some(match winner_index {
                Some(winner)
                    if !intent_wins(self.intents[index].intent, self.intents[winner].intent) =>
                {
                    winner
                }
                _ => index,
            });
        }
        let Some(winner_index) = winner_index else {
            return Ok(None);
        };
        let winner = self.intents[winner_index].intent;
        let mut losing = 0u16;
        for record in self
            .intents
            .iter_mut()
            .filter(|record| !record.resolved && record.intent.target == target)
        {
            record.resolved = true;
            if record.intent != winner && record.intent.action != winner.action {
                losing = losing.saturating_add(1);
            }
        }
        self.counters.conflicts = self.counters.conflicts.saturating_add(u64::from(losing));
        if self.boundary_trace.len() < self.limits.max_trace_entries {
            self.boundary_trace.push(BoundaryTraceEntry {
                at_sample: self.sample_clock,
                target,
                action: winner.action,
                winner: winner.origin,
                n_losing_conflicts: losing,
            });
        } else {
            self.counters.trace_overflows = self.counters.trace_overflows.saturating_add(1);
        }
        Ok(Some(winner))
    }

    fn source_triggered(&self, source: LoopIdentity) -> bool {
        self.triggered_sources.binary_search(&source).is_ok()
    }

    fn mark_triggered(&mut self, source: LoopIdentity) -> Result<(), CompositeTimelineFaultRecord> {
        match self.triggered_sources.binary_search(&source) {
            Ok(_) => Ok(()),
            Err(index) => {
                if self.triggered_sources.len()
                    >= self.limits.max_primitive_events
                        + self.limits.max_composites
                        + self.limits.max_controls
                {
                    self.counters.wave_overflows = self.counters.wave_overflows.saturating_add(1);
                    return Err(self.latch(CompositeTimelineFault::EventWaveCapacity));
                }
                self.triggered_sources.insert(index, source);
                Ok(())
            }
        }
    }

    fn runtime_fault(&mut self) -> CompositeTimelineFaultRecord {
        self.counters.runtime_errors = self.counters.runtime_errors.saturating_add(1);
        self.latch(CompositeTimelineFault::Runtime)
    }

    fn latch(&mut self, fault: CompositeTimelineFault) -> CompositeTimelineFaultRecord {
        if self.fault.fault == CompositeTimelineFault::None {
            self.fault = CompositeTimelineFaultRecord {
                fault,
                at_sample: self.sample_clock,
            };
        }
        self.fault
    }
}

fn validate_limits(limits: CompositeTimelineLimits) -> Result<(), CompositeTimelineBuildError> {
    if limits.max_composites == 0
        || limits.max_primitive_events == 0
        || limits.max_intents == 0
        || limits.max_event_waves == 0
        || limits.max_controls == 0
        || limits.max_controls > MAX_COMPOSITE_CONTROLS
        || limits.max_trace_entries == 0
    {
        Err(CompositeTimelineBuildError::InvalidCapacity)
    } else {
        Ok(())
    }
}

fn stable_graph_depth(
    identities: &BTreeSet<LoopIdentity>,
    edges: &BTreeMap<LoopIdentity, BTreeSet<LoopIdentity>>,
) -> Result<usize, CompositeTimelineBuildError> {
    let mut indegree: BTreeMap<_, usize> = identities
        .iter()
        .copied()
        .map(|identity| (identity, 0))
        .collect();
    for children in edges.values() {
        for child in children {
            *indegree
                .get_mut(child)
                .ok_or(CompositeTimelineBuildError::MissingCompositeTarget)? += 1;
        }
    }
    let mut ready: BTreeSet<_> = indegree
        .iter()
        .filter_map(|(&identity, &degree)| (degree == 0).then_some(identity))
        .collect();
    let mut depth: BTreeMap<_, usize> = identities
        .iter()
        .copied()
        .map(|identity| (identity, 1))
        .collect();
    let mut visited = 0usize;
    while let Some(source) = ready.pop_first() {
        visited += 1;
        if let Some(children) = edges.get(&source) {
            for &child in children {
                let next_depth = depth[&source].saturating_add(1);
                depth
                    .entry(child)
                    .and_modify(|value| *value = (*value).max(next_depth));
                let degree = indegree
                    .get_mut(&child)
                    .ok_or(CompositeTimelineBuildError::MissingCompositeTarget)?;
                *degree -= 1;
                if *degree == 0 {
                    ready.insert(child);
                }
            }
        }
    }
    if visited != identities.len() {
        Err(CompositeTimelineBuildError::DependencyCycle)
    } else {
        Ok(depth.values().copied().max().unwrap_or(0))
    }
}

fn composite_targets(plan: &CompiledCompositePlan) -> impl Iterator<Item = LoopIdentity> + '_ {
    plan.targets()
        .iter()
        .copied()
        .filter(|target| target.kind == LoopTargetKind::Composite)
}

fn topology_order(
    nodes: &BTreeMap<LoopIdentity, InstalledComposite>,
) -> Result<(Vec<LoopIdentity>, usize), CompositeTimelineBuildError> {
    let mut indegree: BTreeMap<LoopIdentity, usize> = nodes
        .keys()
        .copied()
        .map(|identity| (identity, 0))
        .collect();
    let mut edges: BTreeMap<LoopIdentity, BTreeSet<LoopIdentity>> = BTreeMap::new();
    for (&source, node) in nodes {
        if node.sync_source.kind == LoopTargetKind::Composite
            && edges.entry(node.sync_source).or_default().insert(source)
        {
            *indegree
                .get_mut(&source)
                .ok_or(CompositeTimelineBuildError::MissingCompositeTarget)? += 1;
        }
        for &target in node
            .plan()
            .targets()
            .iter()
            .filter(|target| target.kind == LoopTargetKind::Composite)
        {
            if edges.entry(source).or_default().insert(target) {
                *indegree
                    .get_mut(&target)
                    .ok_or(CompositeTimelineBuildError::MissingCompositeTarget)? += 1;
            }
        }
    }
    for (&producer, node) in nodes {
        for &basic_target in node
            .plan()
            .targets()
            .iter()
            .filter(|target| target.kind == LoopTargetKind::Basic)
        {
            for (&follower, follower_node) in nodes {
                if producer != follower
                    && follower_node.sync_source == basic_target
                    && edges.entry(producer).or_default().insert(follower)
                {
                    *indegree
                        .get_mut(&follower)
                        .ok_or(CompositeTimelineBuildError::MissingCompositeTarget)? += 1;
                }
            }
        }
    }

    let mut ready: BTreeSet<_> = indegree
        .iter()
        .filter_map(|(&identity, &degree)| (degree == 0).then_some(identity))
        .collect();
    let mut depth: BTreeMap<_, usize> = nodes
        .keys()
        .copied()
        .map(|identity| (identity, 1))
        .collect();
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(source) = ready.pop_first() {
        order.push(source);
        if let Some(children) = edges.get(&source) {
            for &child in children {
                let next_depth = depth[&source].saturating_add(1);
                depth
                    .entry(child)
                    .and_modify(|value| *value = (*value).max(next_depth));
                let degree = indegree
                    .get_mut(&child)
                    .ok_or(CompositeTimelineBuildError::MissingCompositeTarget)?;
                *degree -= 1;
                if *degree == 0 {
                    ready.insert(child);
                }
            }
        }
    }
    if order.len() != nodes.len() {
        return Err(CompositeTimelineBuildError::DependencyCycle);
    }
    Ok((order, depth.values().copied().max().unwrap_or(0)))
}

fn intent_wins(candidate: BoundaryIntent, incumbent: BoundaryIntent) -> bool {
    intent_priority(candidate.origin)
        .cmp(&intent_priority(incumbent.origin))
        .then_with(|| tie_break(candidate.origin, incumbent.origin))
        == Ordering::Greater
}

fn intent_priority(origin: BoundaryIntentOrigin) -> u8 {
    match origin {
        BoundaryIntentOrigin::Direct { .. } => 4,
        BoundaryIntentOrigin::ScriptComposite { .. } => 3,
        BoundaryIntentOrigin::RegularComposite { .. } => 2,
        BoundaryIntentOrigin::Natural { .. } => 1,
    }
}

fn intent_is_authoritative(origin: BoundaryIntentOrigin) -> bool {
    match origin {
        BoundaryIntentOrigin::Direct { .. } => true,
        BoundaryIntentOrigin::ScriptComposite { authoritative, .. }
        | BoundaryIntentOrigin::RegularComposite { authoritative, .. } => authoritative,
        BoundaryIntentOrigin::Natural { .. } => false,
    }
}

fn tie_break(candidate: BoundaryIntentOrigin, incumbent: BoundaryIntentOrigin) -> Ordering {
    match (candidate, incumbent) {
        (
            BoundaryIntentOrigin::Direct {
                acceptance_sequence: candidate,
            },
            BoundaryIntentOrigin::Direct {
                acceptance_sequence: incumbent,
            },
        ) => candidate.cmp(&incumbent),
        (
            BoundaryIntentOrigin::ScriptComposite {
                source: candidate_source,
                ordinal: candidate_ordinal,
                ..
            }
            | BoundaryIntentOrigin::RegularComposite {
                source: candidate_source,
                ordinal: candidate_ordinal,
                ..
            },
            BoundaryIntentOrigin::ScriptComposite {
                source: incumbent_source,
                ordinal: incumbent_ordinal,
                ..
            }
            | BoundaryIntentOrigin::RegularComposite {
                source: incumbent_source,
                ordinal: incumbent_ordinal,
                ..
            },
        ) => incumbent_source
            .cmp(&candidate_source)
            .then_with(|| candidate_ordinal.cmp(&incumbent_ordinal)),
        (
            BoundaryIntentOrigin::Natural { source: candidate },
            BoundaryIntentOrigin::Natural { source: incumbent },
        ) => incumbent.cmp(&candidate),
        _ => Ordering::Equal,
    }
}
