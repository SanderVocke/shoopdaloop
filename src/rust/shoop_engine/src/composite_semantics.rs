//! Executable parts of the composite-loop semantic contract.
//!
//! These definitions do not implement the composite state machine. They make
//! shared validation and runtime decisions independently testable.

use crate::LoopMode;
use std::cmp::Ordering;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryPhase {
    AcceptCommands,
    ApplySourceStops,
    SeedNaturalEvents,
    ResolveCompositeIntents,
    ApplyTargetStops,
    ApplyTargetStartsAndModes,
    Commit,
}

pub const BOUNDARY_PHASE_ORDER: [BoundaryPhase; 7] = [
    BoundaryPhase::AcceptCommands,
    BoundaryPhase::ApplySourceStops,
    BoundaryPhase::SeedNaturalEvents,
    BoundaryPhase::ResolveCompositeIntents,
    BoundaryPhase::ApplyTargetStops,
    BoundaryPhase::ApplyTargetStartsAndModes,
    BoundaryPhase::Commit,
];

pub const fn half_open_interval_contains(start: u64, end: u64, sample: u64) -> bool {
    start <= sample && sample < end
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompiledBoundaryAction {
    None,
    Stop,
    SetMode(LoopMode),
}

pub const fn normalize_coincident_schedule_actions(
    outgoing_mode: Option<LoopMode>,
    incoming_mode: Option<LoopMode>,
) -> CompiledBoundaryAction {
    match (outgoing_mode, incoming_mode) {
        (None, None) => CompiledBoundaryAction::None,
        (Some(_), None) => CompiledBoundaryAction::Stop,
        (None, Some(mode)) => CompiledBoundaryAction::SetMode(mode),
        (Some(old), Some(new)) if old as i32 == new as i32 => CompiledBoundaryAction::None,
        (Some(_), Some(new)) => CompiledBoundaryAction::SetMode(new),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentOrigin {
    Natural,
    InheritedRegular,
    ExplicitScript,
    DirectControl,
}

impl IntentOrigin {
    const fn rank(self) -> u8 {
        match self {
            Self::Natural => 0,
            Self::InheritedRegular => 1,
            Self::ExplicitScript => 2,
            Self::DirectControl => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntentPriority {
    pub origin: IntentOrigin,
    pub source_id: u64,
    pub action_ordinal: u32,
    pub acceptance_sequence: u64,
}

impl IntentPriority {
    pub fn precedence_over(self, other: Self) -> Ordering {
        match self.origin.rank().cmp(&other.origin.rank()) {
            Ordering::Equal => {}
            ordering => return ordering,
        }

        if self.origin == IntentOrigin::DirectControl {
            return self
                .acceptance_sequence
                .cmp(&other.acceptance_sequence)
                .then_with(|| self.action_ordinal.cmp(&other.action_ordinal));
        }

        other
            .source_id
            .cmp(&self.source_id)
            .then_with(|| self.action_ordinal.cmp(&other.action_ordinal))
    }
}

pub const fn source_emits_due_action(
    was_running_before_boundary: bool,
    stopped_before_delivery: bool,
) -> bool {
    was_running_before_boundary && !stopped_before_delivery
}

pub const fn nested_iteration_zero_is_same_sample() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeStatus {
    Stopped,
    Pending,
    Running,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanActivation {
    CurrentCommandBoundary,
    NextIterationZeroBoundary,
}

pub const fn plan_activation(status: RuntimeStatus) -> PlanActivation {
    match status {
        RuntimeStatus::Stopped | RuntimeStatus::Pending => PlanActivation::CurrentCommandBoundary,
        RuntimeStatus::Running => PlanActivation::NextIterationZeroBoundary,
    }
}

pub const fn plan_can_enter_running(n_iterations: u32) -> bool {
    n_iterations > 0
}

pub const fn countdown_execution_boundary(delay: u32) -> u64 {
    delay as u64 + 1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationError {
    ZeroSyncLength,
    NonPositiveExplicit,
    TooLong,
}

pub fn entry_duration(
    explicit_cycles: Option<i64>,
    child_length: u64,
    sync_length: u64,
) -> Result<u32, DurationError> {
    if let Some(explicit) = explicit_cycles {
        if explicit <= 0 {
            return Err(DurationError::NonPositiveExplicit);
        }
        return u32::try_from(explicit).map_err(|_| DurationError::TooLong);
    }
    if sync_length == 0 {
        return Err(DurationError::ZeroSyncLength);
    }

    let duration = child_length
        .checked_add(sync_length - 1)
        .ok_or(DurationError::TooLong)?
        / sync_length;
    u32::try_from(duration.max(1)).map_err(|_| DurationError::TooLong)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositeKind {
    Regular,
    Script,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModePlanError {
    MixedImplicitAndExplicit,
    UnknownExplicitMode,
}

pub fn classify_plan_modes(modes: &[Option<LoopMode>]) -> Result<CompositeKind, ModePlanError> {
    let implicit = modes.iter().filter(|mode| mode.is_none()).count();
    let explicit = modes.len() - implicit;
    if modes
        .iter()
        .flatten()
        .any(|mode| *mode == LoopMode::Unknown)
    {
        return Err(ModePlanError::UnknownExplicitMode);
    }
    if implicit > 0 && explicit > 0 {
        Err(ModePlanError::MixedImplicitAndExplicit)
    } else if explicit > 0 {
        Ok(CompositeKind::Script)
    } else {
        Ok(CompositeKind::Regular)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyChildAction {
    Apply,
    ReserveDurationOnly,
}

pub const fn empty_child_action(mode: LoopMode) -> EmptyChildAction {
    match mode {
        LoopMode::Playing | LoopMode::PlayingDryThroughWet => EmptyChildAction::ReserveDurationOnly,
        _ => EmptyChildAction::Apply,
    }
}

pub const fn records_this_occurrence(previous_occurrences: u32) -> bool {
    previous_occurrences == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassEnd {
    CycleToIterationZero,
    BeginPlaybackAtIterationZero,
    Stop,
}

pub const fn pass_end(kind: CompositeKind, mode: LoopMode, play_after_record: bool) -> PassEnd {
    if matches!(kind, CompositeKind::Script) {
        PassEnd::Stop
    } else if matches!(mode, LoopMode::Recording | LoopMode::RecordingDryIntoWet) {
        if play_after_record {
            PassEnd::BeginPlaybackAtIterationZero
        } else {
            PassEnd::Stop
        }
    } else {
        PassEnd::CycleToIterationZero
    }
}

pub const fn valid_seek_iteration(iteration: i64, n_iterations: u32) -> bool {
    iteration >= 0 && iteration < n_iterations as i64
}

pub const fn seek_cycle_offset(
    iteration: u32,
    action_start: u32,
    duration: u32,
    recording: bool,
) -> Option<u32> {
    if iteration < action_start || duration == 0 {
        return None;
    }
    let elapsed = iteration - action_start;
    if recording {
        Some(elapsed)
    } else {
        Some(elapsed % duration)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetIdentity {
    pub slot: u32,
    pub generation: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetResolution {
    Apply,
    ReportStaleAndSkip,
}

pub const fn resolve_target(
    expected: TargetIdentity,
    current: Option<TargetIdentity>,
) -> TargetResolution {
    match current {
        Some(actual)
            if actual.slot == expected.slot && actual.generation == expected.generation =>
        {
            TargetResolution::Apply
        }
        _ => TargetResolution::ReportStaleAndSkip,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowSite {
    CommandQueue,
    PlanQueue,
    PlanCapacity,
    BoundaryEventQueue,
    EventWaves,
    SubBlocks,
    SnapshotQueue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowDisposition {
    RejectBeforeAcceptance,
    EnterRtFaultAtBoundary,
    DropStaleObservation,
}

pub const fn overflow_disposition(site: OverflowSite) -> OverflowDisposition {
    match site {
        OverflowSite::CommandQueue | OverflowSite::PlanQueue | OverflowSite::PlanCapacity => {
            OverflowDisposition::RejectBeforeAcceptance
        }
        OverflowSite::BoundaryEventQueue | OverflowSite::EventWaves | OverflowSite::SubBlocks => {
            OverflowDisposition::EnterRtFaultAtBoundary
        }
        OverflowSite::SnapshotQueue => OverflowDisposition::DropStaleObservation,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampRelation {
    Past,
    InCurrentBuffer(u32),
    Future,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandTiming {
    Untimestamped,
    Timestamped(TimestampRelation),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandDisposition {
    AcceptAtCallbackStart,
    AcceptAtSampleOffset(u32),
    Defer,
    RejectLateTimestamp,
}

pub const fn command_disposition(
    offered_before_callback_cutoff: bool,
    timing: CommandTiming,
) -> CommandDisposition {
    if !offered_before_callback_cutoff {
        return CommandDisposition::Defer;
    }

    match timing {
        CommandTiming::Untimestamped => CommandDisposition::AcceptAtCallbackStart,
        CommandTiming::Timestamped(TimestampRelation::Past) => {
            CommandDisposition::RejectLateTimestamp
        }
        CommandTiming::Timestamped(TimestampRelation::InCurrentBuffer(offset)) => {
            CommandDisposition::AcceptAtSampleOffset(offset)
        }
        CommandTiming::Timestamped(TimestampRelation::Future) => CommandDisposition::Defer,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyError {
    UnknownNode { from: usize, to: usize },
    Cycle { nodes: Vec<usize> },
}

/// Produces a deterministic parent-before-child order for off-thread plan compilation.
pub fn dependency_order(
    node_count: usize,
    parent_to_child_edges: &[(usize, usize)],
) -> Result<Vec<usize>, DependencyError> {
    let mut outgoing = vec![BTreeSet::new(); node_count];
    let mut incoming_count = vec![0usize; node_count];

    for &(from, to) in parent_to_child_edges {
        if from >= node_count || to >= node_count {
            return Err(DependencyError::UnknownNode { from, to });
        }
        if outgoing[from].insert(to) {
            incoming_count[to] += 1;
        }
    }

    let mut ready: BTreeSet<usize> = incoming_count
        .iter()
        .enumerate()
        .filter_map(|(node, count)| (*count == 0).then_some(node))
        .collect();
    let mut order = Vec::with_capacity(node_count);

    while let Some(node) = ready.pop_first() {
        order.push(node);
        for child in &outgoing[node] {
            incoming_count[*child] -= 1;
            if incoming_count[*child] == 0 {
                ready.insert(*child);
            }
        }
    }

    if order.len() == node_count {
        Ok(order)
    } else {
        Err(DependencyError::Cycle {
            nodes: incoming_count
                .iter()
                .enumerate()
                .filter_map(|(node, count)| (*count > 0).then_some(node))
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn priority(
        origin: IntentOrigin,
        source_id: u64,
        ordinal: u32,
        sequence: u64,
    ) -> IntentPriority {
        IntentPriority {
            origin,
            source_id,
            action_ordinal: ordinal,
            acceptance_sequence: sequence,
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn sample_intervals_are_half_open_at_a_boundary() {
        assert!(half_open_interval_contains(10, 20, 10));
        assert!(half_open_interval_contains(10, 20, 19));
        assert!(!half_open_interval_contains(10, 20, 20));
        assert!(half_open_interval_contains(20, 30, 20));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn boundary_phase_order_is_fixed() {
        assert_eq!(
            BOUNDARY_PHASE_ORDER,
            [
                BoundaryPhase::AcceptCommands,
                BoundaryPhase::ApplySourceStops,
                BoundaryPhase::SeedNaturalEvents,
                BoundaryPhase::ResolveCompositeIntents,
                BoundaryPhase::ApplyTargetStops,
                BoundaryPhase::ApplyTargetStartsAndModes,
                BoundaryPhase::Commit,
            ]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn coincident_end_and_start_continue_or_change_mode() {
        assert_eq!(
            normalize_coincident_schedule_actions(Some(LoopMode::Playing), Some(LoopMode::Playing)),
            CompiledBoundaryAction::None
        );
        assert_eq!(
            normalize_coincident_schedule_actions(
                Some(LoopMode::Playing),
                Some(LoopMode::Recording)
            ),
            CompiledBoundaryAction::SetMode(LoopMode::Recording)
        );
        assert_eq!(
            normalize_coincident_schedule_actions(Some(LoopMode::Playing), None),
            CompiledBoundaryAction::Stop
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn incompatible_intents_have_total_precedence() {
        let natural = priority(IntentOrigin::Natural, 1, 0, 0);
        let regular = priority(IntentOrigin::InheritedRegular, 9, 0, 0);
        let script = priority(IntentOrigin::ExplicitScript, 10, 0, 0);
        let direct = priority(IntentOrigin::DirectControl, 0, 0, 12);
        assert_eq!(regular.precedence_over(natural), Ordering::Greater);
        assert_eq!(script.precedence_over(regular), Ordering::Greater);
        assert_eq!(direct.precedence_over(script), Ordering::Greater);

        let lower_source = priority(IntentOrigin::ExplicitScript, 2, 0, 0);
        let higher_source = priority(IntentOrigin::ExplicitScript, 3, 99, 0);
        assert_eq!(
            lower_source.precedence_over(higher_source),
            Ordering::Greater
        );

        let earlier_action = priority(IntentOrigin::ExplicitScript, 2, 3, 0);
        let later_action = priority(IntentOrigin::ExplicitScript, 2, 4, 0);
        assert_eq!(
            later_action.precedence_over(earlier_action),
            Ordering::Greater
        );

        let earlier_command = priority(IntentOrigin::DirectControl, 0, 0, 12);
        let later_command = priority(IntentOrigin::DirectControl, 0, 0, 13);
        assert_eq!(
            later_command.precedence_over(earlier_command),
            Ordering::Greater
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn stop_before_delivery_suppresses_a_due_action() {
        assert!(source_emits_due_action(true, false));
        assert!(!source_emits_due_action(true, true));
        assert!(!source_emits_due_action(false, false));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn nested_iteration_zero_occurs_at_the_parent_start_sample() {
        assert!(nested_iteration_zero_is_same_sample());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dependency_order_is_stable_and_parent_before_child() {
        assert_eq!(
            dependency_order(5, &[(0, 3), (1, 3), (3, 4)]),
            Ok(vec![0, 1, 2, 3, 4])
        );
        assert_eq!(dependency_order(3, &[(0, 2), (0, 2)]), Ok(vec![0, 1, 2]));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn direct_and_transitive_dependency_cycles_are_rejected() {
        assert_eq!(
            dependency_order(2, &[(0, 1), (1, 0)]),
            Err(DependencyError::Cycle { nodes: vec![0, 1] })
        );
        assert_eq!(
            dependency_order(4, &[(0, 1), (1, 2), (2, 0), (2, 3)]),
            Err(DependencyError::Cycle {
                nodes: vec![0, 1, 2, 3]
            })
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn invalid_dependency_identity_is_rejected() {
        assert_eq!(
            dependency_order(2, &[(0, 2)]),
            Err(DependencyError::UnknownNode { from: 0, to: 2 })
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn plan_activation_depends_only_on_runtime_status() {
        assert_eq!(
            plan_activation(RuntimeStatus::Stopped),
            PlanActivation::CurrentCommandBoundary
        );
        assert_eq!(
            plan_activation(RuntimeStatus::Pending),
            PlanActivation::CurrentCommandBoundary
        );
        assert_eq!(
            plan_activation(RuntimeStatus::Running),
            PlanActivation::NextIterationZeroBoundary
        );
        assert!(!plan_can_enter_running(0));
        assert!(plan_can_enter_running(1));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn countdown_delay_counts_boundaries_to_skip() {
        assert_eq!(countdown_execution_boundary(0), 1);
        assert_eq!(countdown_execution_boundary(3), 4);
        assert_eq!(countdown_execution_boundary(u32::MAX), 1u64 << 32);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn schedule_duration_is_explicit_or_length_derived() {
        assert_eq!(entry_duration(Some(3), 999, 100), Ok(3));
        assert_eq!(entry_duration(None, 0, 100), Ok(1));
        assert_eq!(entry_duration(None, 200, 100), Ok(2));
        assert_eq!(entry_duration(None, 201, 100), Ok(3));
        assert_eq!(
            entry_duration(Some(0), 100, 100),
            Err(DurationError::NonPositiveExplicit)
        );
        assert_eq!(
            entry_duration(None, 100, 0),
            Err(DurationError::ZeroSyncLength)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn regular_and_script_modes_are_all_or_nothing() {
        assert_eq!(classify_plan_modes(&[]), Ok(CompositeKind::Regular));
        assert_eq!(
            classify_plan_modes(&[None, None]),
            Ok(CompositeKind::Regular)
        );
        assert_eq!(
            classify_plan_modes(&[Some(LoopMode::Recording), Some(LoopMode::Playing)]),
            Ok(CompositeKind::Script)
        );
        assert_eq!(
            classify_plan_modes(&[None, Some(LoopMode::Playing)]),
            Err(ModePlanError::MixedImplicitAndExplicit)
        );
        assert_eq!(
            classify_plan_modes(&[Some(LoopMode::Unknown)]),
            Err(ModePlanError::UnknownExplicitMode)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn empty_child_and_recording_pass_rules_are_explicit() {
        assert_eq!(
            empty_child_action(LoopMode::Playing),
            EmptyChildAction::ReserveDurationOnly
        );
        assert_eq!(
            empty_child_action(LoopMode::Recording),
            EmptyChildAction::Apply
        );
        assert!(records_this_occurrence(0));
        assert!(!records_this_occurrence(1));
        assert_eq!(
            pass_end(CompositeKind::Regular, LoopMode::Recording, true),
            PassEnd::BeginPlaybackAtIterationZero
        );
        assert_eq!(
            pass_end(CompositeKind::Regular, LoopMode::Recording, false),
            PassEnd::Stop
        );
        assert_eq!(
            pass_end(CompositeKind::Regular, LoopMode::Playing, true),
            PassEnd::CycleToIterationZero
        );
        assert_eq!(
            pass_end(CompositeKind::Script, LoopMode::Playing, true),
            PassEnd::Stop
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn immediate_seek_is_bounded_and_derives_cycle_offsets() {
        assert!(valid_seek_iteration(0, 4));
        assert!(valid_seek_iteration(3, 4));
        assert!(!valid_seek_iteration(-1, 4));
        assert!(!valid_seek_iteration(4, 4));
        assert!(!valid_seek_iteration(0, 0));
        assert_eq!(seek_cycle_offset(5, 2, 2, false), Some(1));
        assert_eq!(seek_cycle_offset(5, 2, 2, true), Some(3));
        assert_eq!(seek_cycle_offset(1, 2, 2, false), None);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn stale_or_missing_targets_are_never_retargeted() {
        let expected = TargetIdentity {
            slot: 7,
            generation: 2,
        };
        assert_eq!(
            resolve_target(expected, Some(expected)),
            TargetResolution::Apply
        );
        assert_eq!(
            resolve_target(
                expected,
                Some(TargetIdentity {
                    slot: 7,
                    generation: 3,
                })
            ),
            TargetResolution::ReportStaleAndSkip
        );
        assert_eq!(
            resolve_target(expected, None),
            TargetResolution::ReportStaleAndSkip
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn overflow_never_turns_into_a_late_event() {
        for site in [
            OverflowSite::CommandQueue,
            OverflowSite::PlanQueue,
            OverflowSite::PlanCapacity,
        ] {
            assert_eq!(
                overflow_disposition(site),
                OverflowDisposition::RejectBeforeAcceptance
            );
        }
        for site in [
            OverflowSite::BoundaryEventQueue,
            OverflowSite::EventWaves,
            OverflowSite::SubBlocks,
        ] {
            assert_eq!(
                overflow_disposition(site),
                OverflowDisposition::EnterRtFaultAtBoundary
            );
        }
        assert_eq!(
            overflow_disposition(OverflowSite::SnapshotQueue),
            OverflowDisposition::DropStaleObservation
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn callback_cutoff_defers_commands_that_missed_the_drain() {
        assert_eq!(
            command_disposition(true, CommandTiming::Untimestamped),
            CommandDisposition::AcceptAtCallbackStart
        );
        assert_eq!(
            command_disposition(false, CommandTiming::Untimestamped),
            CommandDisposition::Defer
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn timestamped_commands_keep_exact_in_buffer_timing() {
        assert_eq!(
            command_disposition(
                true,
                CommandTiming::Timestamped(TimestampRelation::InCurrentBuffer(17))
            ),
            CommandDisposition::AcceptAtSampleOffset(17)
        );
        assert_eq!(
            command_disposition(true, CommandTiming::Timestamped(TimestampRelation::Past)),
            CommandDisposition::RejectLateTimestamp
        );
        assert_eq!(
            command_disposition(true, CommandTiming::Timestamped(TimestampRelation::Future)),
            CommandDisposition::Defer
        );
    }
}
