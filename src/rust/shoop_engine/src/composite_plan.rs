//! Off-thread compilation of immutable composite-loop plans.

use crate::LoopMode;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_COMPOSITE_TARGETS: usize = 64;
pub const MAX_COMPOSITE_BOUNDARY_OUTPUTS: usize = MAX_COMPOSITE_TARGETS * 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LoopTargetKind {
    Basic,
    Composite,
}

/// Stable engine identity. A reused slot receives a new generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopIdentity {
    pub slot: u32,
    pub generation: u32,
    pub kind: LoopTargetKind,
}

impl Ord for LoopIdentity {
    fn cmp(&self, other: &Self) -> Ordering {
        self.slot
            .cmp(&other.slot)
            .then_with(|| self.generation.cmp(&other.generation))
            .then_with(|| self.kind.cmp(&other.kind))
    }
}

impl PartialOrd for LoopIdentity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopTargetMetadata {
    pub identity: LoopIdentity,
    pub length_samples: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoopTargetCatalog {
    targets: Vec<LoopTargetMetadata>,
}

impl LoopTargetCatalog {
    pub fn new(mut targets: Vec<LoopTargetMetadata>) -> Result<Self, CompositeCompileError> {
        targets.sort_unstable_by_key(|target| target.identity);
        if targets
            .windows(2)
            .any(|pair| pair[0].identity.slot == pair[1].identity.slot)
        {
            return Err(CompositeCompileError::DuplicateIdentity);
        }
        Ok(Self { targets })
    }

    pub fn get(&self, identity: LoopIdentity) -> Option<&LoopTargetMetadata> {
        self.targets
            .binary_search_by_key(&identity, |target| target.identity)
            .ok()
            .map(|index| &self.targets[index])
    }

    fn contains_slot(&self, identity: LoopIdentity) -> bool {
        self.targets
            .iter()
            .any(|target| target.identity.slot == identity.slot)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositeEntry {
    pub target: LoopIdentity,
    pub delay: i64,
    pub n_cycles: Option<i64>,
    pub mode: Option<LoopMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompositeSection {
    pub entries: Vec<CompositeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompositeTimeline {
    pub sections: Vec<CompositeSection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositePlanDescriptor {
    pub source: LoopIdentity,
    pub sync_length: u64,
    pub timelines: Vec<CompositeTimeline>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeDependency {
    pub source: LoopIdentity,
    pub composite_children: Vec<LoopIdentity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositePlanLimits {
    pub max_entries: usize,
    pub max_actions: usize,
    pub max_targets: usize,
    pub max_iterations: u32,
    pub max_seek_entries: usize,
    pub max_dependency_nodes: usize,
    pub max_dependency_edges: usize,
    pub max_nesting_depth: usize,
}

impl Default for CompositePlanLimits {
    fn default() -> Self {
        Self {
            max_entries: 256,
            max_actions: 512,
            max_targets: MAX_COMPOSITE_TARGETS,
            max_iterations: 16_384,
            max_seek_entries: 65_536,
            max_dependency_nodes: 256,
            max_dependency_edges: 1_024,
            max_nesting_depth: 32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompiledCompositeKind {
    Regular,
    Script,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompiledChildMode {
    Inherit,
    Explicit(LoopMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompiledDesiredState {
    pub mode: CompiledChildMode,
    pub occurrence: u32,
    pub start_iteration: u32,
    pub duration: u32,
    pub child_is_empty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompiledPlanActionKind {
    Stop,
    SetDesired(CompiledDesiredState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompiledPlanAction {
    pub iteration: u32,
    pub target_index: u16,
    pub action_ordinal: u32,
    pub kind: CompiledPlanActionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompiledActionRange {
    pub iteration: u32,
    pub start: u32,
    pub len: u32,
}

/// Allocation-complete plan consumed through immutable slices by the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledCompositePlan {
    source: LoopIdentity,
    kind: CompiledCompositeKind,
    sync_length: u64,
    n_iterations: u32,
    targets: Box<[LoopIdentity]>,
    desired_by_iteration: Box<[Option<CompiledDesiredState>]>,
    first_recording_by_iteration: Box<[Option<CompiledDesiredState>]>,
    actions: Box<[CompiledPlanAction]>,
    action_ranges: Box<[CompiledActionRange]>,
    dependency_order: Box<[LoopIdentity]>,
}

impl CompiledCompositePlan {
    pub fn source(&self) -> LoopIdentity {
        self.source
    }

    pub fn kind(&self) -> CompiledCompositeKind {
        self.kind
    }

    pub fn sync_length(&self) -> u64 {
        self.sync_length
    }

    pub fn n_iterations(&self) -> u32 {
        self.n_iterations
    }

    pub fn targets(&self) -> &[LoopIdentity] {
        &self.targets
    }

    pub fn actions(&self) -> &[CompiledPlanAction] {
        &self.actions
    }

    pub fn action_ranges(&self) -> &[CompiledActionRange] {
        &self.action_ranges
    }

    pub fn dependency_order(&self) -> &[LoopIdentity] {
        &self.dependency_order
    }

    pub fn actions_at(&self, iteration: u32) -> &[CompiledPlanAction] {
        match self
            .action_ranges
            .binary_search_by_key(&iteration, |range| range.iteration)
        {
            Ok(index) => {
                let range = self.action_ranges[index];
                &self.actions[range.start as usize..(range.start + range.len) as usize]
            }
            Err(_) => &[],
        }
    }

    pub fn desired(
        &self,
        iteration: u32,
        target_index: usize,
        first_recording_only: bool,
    ) -> Option<CompiledDesiredState> {
        if iteration >= self.n_iterations || target_index >= self.targets.len() {
            return None;
        }
        let index = iteration as usize * self.targets.len() + target_index;
        if first_recording_only {
            self.first_recording_by_iteration[index]
        } else {
            self.desired_by_iteration[index]
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CompositeCompileError {
    #[error("the target catalog contains a duplicate identity")]
    DuplicateIdentity,
    #[error("the source identity is missing")]
    MissingSource,
    #[error("the source identity is stale")]
    StaleSource,
    #[error("the source identity does not identify a composite")]
    SourceIsNotComposite,
    #[error("a target identity is missing")]
    MissingTarget,
    #[error("a target identity is stale")]
    StaleTarget,
    #[error("an entry delay is negative")]
    NegativeDelay,
    #[error("an explicit cycle count is not positive")]
    NonPositiveCycleCount,
    #[error("a duration requires a nonzero synchronization length")]
    ZeroSyncLength,
    #[error("an explicit mode is unknown")]
    UnknownMode,
    #[error("implicit and explicit entry modes are mixed")]
    MixedModes,
    #[error("schedule arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("a configured plan capacity is invalid")]
    InvalidCapacity,
    #[error("the plan has too many entries")]
    EntryCapacity,
    #[error("the plan has too many targets")]
    TargetCapacity,
    #[error("the plan has too many iterations")]
    IterationCapacity,
    #[error("the plan has too many precomputed seek entries")]
    SeekCapacity,
    #[error("the plan has too many compiled actions")]
    ActionCapacity,
    #[error("the dependency graph has too many nodes")]
    DependencyNodeCapacity,
    #[error("the dependency graph has too many edges")]
    DependencyEdgeCapacity,
    #[error("the dependency graph exceeds its nesting-depth limit")]
    NestingDepthCapacity,
    #[error("the dependency graph contains a cycle")]
    DependencyCycle,
    #[error("the dependency graph contains a basic-loop child")]
    NonCompositeDependency,
    #[error("the dependency graph references an unknown identity")]
    UnknownDependency,
}

#[derive(Debug, Clone, Copy)]
struct ScheduledOccurrence {
    target: LoopIdentity,
    start: u32,
    end: u32,
    mode: CompiledChildMode,
    child_is_empty: bool,
    occurrence: u32,
}

impl ScheduledOccurrence {
    fn desired(self) -> CompiledDesiredState {
        CompiledDesiredState {
            mode: self.mode,
            occurrence: self.occurrence,
            start_iteration: self.start,
            duration: self.end - self.start,
            child_is_empty: self.child_is_empty,
        }
    }
}

pub fn compile_composite_plan(
    descriptor: &CompositePlanDescriptor,
    catalog: &LoopTargetCatalog,
    installed_topology: &[CompositeDependency],
    limits: CompositePlanLimits,
) -> Result<CompiledCompositePlan, CompositeCompileError> {
    validate_limits(limits)?;
    let source = resolve_source(descriptor.source, catalog)?;
    let kind = classify_kind(descriptor)?;

    let mut occurrences = Vec::new();
    let mut entry_count = 0usize;
    let mut n_iterations = 0u32;

    for timeline in &descriptor.timelines {
        let mut section_origin = 0u32;
        for section in &timeline.sections {
            let mut section_duration = 0u32;
            for entry in &section.entries {
                entry_count = entry_count
                    .checked_add(1)
                    .ok_or(CompositeCompileError::EntryCapacity)?;
                if entry_count > limits.max_entries {
                    return Err(CompositeCompileError::EntryCapacity);
                }
                if entry.delay < 0 {
                    return Err(CompositeCompileError::NegativeDelay);
                }
                let metadata = resolve_target(entry.target, catalog)?;
                let duration = compile_duration(
                    entry.n_cycles,
                    metadata.length_samples,
                    descriptor.sync_length,
                )?;
                let delay = u32::try_from(entry.delay)
                    .map_err(|_| CompositeCompileError::ArithmeticOverflow)?;
                let start = section_origin
                    .checked_add(delay)
                    .ok_or(CompositeCompileError::ArithmeticOverflow)?;
                let end = start
                    .checked_add(duration)
                    .ok_or(CompositeCompileError::ArithmeticOverflow)?;
                section_duration = section_duration.max(
                    delay
                        .checked_add(duration)
                        .ok_or(CompositeCompileError::ArithmeticOverflow)?,
                );
                occurrences.push(ScheduledOccurrence {
                    target: metadata.identity,
                    start,
                    end,
                    mode: match entry.mode {
                        Some(mode) => CompiledChildMode::Explicit(mode),
                        None => CompiledChildMode::Inherit,
                    },
                    child_is_empty: metadata.length_samples == 0,
                    occurrence: 0,
                });
            }
            section_origin = section_origin
                .checked_add(section_duration)
                .ok_or(CompositeCompileError::ArithmeticOverflow)?;
        }
        n_iterations = n_iterations.max(section_origin);
    }

    if n_iterations > limits.max_iterations {
        return Err(CompositeCompileError::IterationCapacity);
    }
    u64::from(n_iterations)
        .checked_mul(descriptor.sync_length)
        .ok_or(CompositeCompileError::ArithmeticOverflow)?;

    occurrences.sort_unstable_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.target.cmp(&right.target))
            .then_with(|| left.end.cmp(&right.end))
            .then_with(|| mode_sort_key(left.mode).cmp(&mode_sort_key(right.mode)))
    });

    let mut occurrence_counts = BTreeMap::<LoopIdentity, u32>::new();
    let mut explicitly_recorded_targets = BTreeSet::<LoopIdentity>::new();
    for occurrence in &mut occurrences {
        if explicitly_recorded_targets.contains(&occurrence.target) {
            occurrence.child_is_empty = false;
        }
        let count = occurrence_counts.entry(occurrence.target).or_default();
        occurrence.occurrence = *count;
        *count = count
            .checked_add(1)
            .ok_or(CompositeCompileError::ArithmeticOverflow)?;
        if matches!(
            occurrence.mode,
            CompiledChildMode::Explicit(LoopMode::Recording | LoopMode::RecordingDryIntoWet)
        ) {
            explicitly_recorded_targets.insert(occurrence.target);
        }
    }

    let targets: Vec<_> = occurrence_counts.keys().copied().collect();
    if targets.len() > limits.max_targets || targets.len() > MAX_COMPOSITE_TARGETS {
        return Err(CompositeCompileError::TargetCapacity);
    }

    let seek_entries = (n_iterations as usize)
        .checked_mul(targets.len())
        .ok_or(CompositeCompileError::SeekCapacity)?;
    let total_seek_entries = seek_entries
        .checked_mul(if kind == CompiledCompositeKind::Regular {
            2
        } else {
            1
        })
        .ok_or(CompositeCompileError::SeekCapacity)?;
    if total_seek_entries > limits.max_seek_entries {
        return Err(CompositeCompileError::SeekCapacity);
    }

    let mut desired_by_iteration = vec![None; seek_entries];
    let mut first_recording_by_iteration = if kind == CompiledCompositeKind::Regular {
        vec![None; seek_entries]
    } else {
        Vec::new()
    };
    for occurrence in occurrences {
        let target_index = targets
            .binary_search(&occurrence.target)
            .map_err(|_| CompositeCompileError::MissingTarget)?;
        for iteration in occurrence.start..occurrence.end {
            let index = iteration as usize * targets.len() + target_index;
            desired_by_iteration[index] = Some(occurrence.desired());
            if kind == CompiledCompositeKind::Regular && occurrence.occurrence == 0 {
                first_recording_by_iteration[index] = Some(occurrence.desired());
            }
        }
    }

    let (actions, action_ranges) = compile_actions(
        n_iterations,
        &targets,
        &desired_by_iteration,
        limits.max_actions,
    )?;
    let dependency_order = compile_dependency_order(
        source.identity,
        descriptor,
        catalog,
        installed_topology,
        limits,
    )?;

    Ok(CompiledCompositePlan {
        source: source.identity,
        kind,
        sync_length: descriptor.sync_length,
        n_iterations,
        targets: targets.into_boxed_slice(),
        desired_by_iteration: desired_by_iteration.into_boxed_slice(),
        first_recording_by_iteration: first_recording_by_iteration.into_boxed_slice(),
        actions: actions.into_boxed_slice(),
        action_ranges: action_ranges.into_boxed_slice(),
        dependency_order: dependency_order.into_boxed_slice(),
    })
}

fn validate_limits(limits: CompositePlanLimits) -> Result<(), CompositeCompileError> {
    if limits.max_targets > MAX_COMPOSITE_TARGETS
        || limits.max_actions == 0
        || limits.max_seek_entries == 0
        || limits.max_dependency_nodes == 0
        || limits.max_dependency_edges == 0
        || limits.max_nesting_depth == 0
    {
        Err(CompositeCompileError::InvalidCapacity)
    } else {
        Ok(())
    }
}

fn resolve_source<'a>(
    identity: LoopIdentity,
    catalog: &'a LoopTargetCatalog,
) -> Result<&'a LoopTargetMetadata, CompositeCompileError> {
    match catalog.get(identity) {
        Some(metadata) if metadata.identity.kind == LoopTargetKind::Composite => Ok(metadata),
        Some(_) => Err(CompositeCompileError::SourceIsNotComposite),
        None if catalog.contains_slot(identity) => Err(CompositeCompileError::StaleSource),
        None => Err(CompositeCompileError::MissingSource),
    }
}

fn resolve_target(
    identity: LoopIdentity,
    catalog: &LoopTargetCatalog,
) -> Result<&LoopTargetMetadata, CompositeCompileError> {
    match catalog.get(identity) {
        Some(metadata) => Ok(metadata),
        None if catalog.contains_slot(identity) => Err(CompositeCompileError::StaleTarget),
        None => Err(CompositeCompileError::MissingTarget),
    }
}

fn classify_kind(
    descriptor: &CompositePlanDescriptor,
) -> Result<CompiledCompositeKind, CompositeCompileError> {
    let mut implicit = 0usize;
    let mut explicit = 0usize;
    for entry in descriptor
        .timelines
        .iter()
        .flat_map(|timeline| &timeline.sections)
        .flat_map(|section| &section.entries)
    {
        match entry.mode {
            Some(LoopMode::Unknown) => return Err(CompositeCompileError::UnknownMode),
            Some(_) => explicit += 1,
            None => implicit += 1,
        }
    }
    if implicit > 0 && explicit > 0 {
        Err(CompositeCompileError::MixedModes)
    } else if explicit > 0 {
        Ok(CompiledCompositeKind::Script)
    } else {
        Ok(CompiledCompositeKind::Regular)
    }
}

fn compile_duration(
    explicit: Option<i64>,
    child_length: u64,
    sync_length: u64,
) -> Result<u32, CompositeCompileError> {
    if let Some(cycles) = explicit {
        if cycles <= 0 {
            return Err(CompositeCompileError::NonPositiveCycleCount);
        }
        return u32::try_from(cycles).map_err(|_| CompositeCompileError::ArithmeticOverflow);
    }
    if sync_length == 0 {
        return Err(CompositeCompileError::ZeroSyncLength);
    }
    let cycles = child_length
        .checked_add(sync_length - 1)
        .ok_or(CompositeCompileError::ArithmeticOverflow)?
        / sync_length;
    u32::try_from(cycles.max(1)).map_err(|_| CompositeCompileError::ArithmeticOverflow)
}

fn mode_sort_key(mode: CompiledChildMode) -> i32 {
    match mode {
        CompiledChildMode::Inherit => -1,
        CompiledChildMode::Explicit(mode) => mode as i32,
    }
}

fn compile_actions(
    n_iterations: u32,
    targets: &[LoopIdentity],
    desired: &[Option<CompiledDesiredState>],
    max_actions: usize,
) -> Result<(Vec<CompiledPlanAction>, Vec<CompiledActionRange>), CompositeCompileError> {
    let mut actions = Vec::new();
    let mut ranges = Vec::new();
    let mut ordinal = 0u32;

    for iteration in 0..=n_iterations {
        let range_start = actions.len();
        let mut boundary_actions = Vec::new();
        for target_index in 0..targets.len() {
            let before = if iteration == 0 {
                None
            } else {
                desired[(iteration as usize - 1) * targets.len() + target_index]
            };
            let after = if iteration == n_iterations {
                None
            } else {
                desired[iteration as usize * targets.len() + target_index]
            };
            if before == after {
                continue;
            }
            let kind = match after {
                Some(state) => CompiledPlanActionKind::SetDesired(state),
                None => CompiledPlanActionKind::Stop,
            };
            boundary_actions.push((target_index as u16, kind));
        }
        boundary_actions.sort_unstable_by_key(|(target_index, kind)| {
            let phase = match kind {
                CompiledPlanActionKind::Stop => 0,
                CompiledPlanActionKind::SetDesired(_) => 1,
            };
            (phase, *target_index)
        });
        for (target_index, kind) in boundary_actions {
            actions.push(CompiledPlanAction {
                iteration,
                target_index,
                action_ordinal: ordinal,
                kind,
            });
            ordinal = ordinal
                .checked_add(1)
                .ok_or(CompositeCompileError::ActionCapacity)?;
            if actions.len() > max_actions {
                return Err(CompositeCompileError::ActionCapacity);
            }
        }
        if actions.len() != range_start {
            ranges.push(CompiledActionRange {
                iteration,
                start: u32::try_from(range_start)
                    .map_err(|_| CompositeCompileError::ActionCapacity)?,
                len: u32::try_from(actions.len() - range_start)
                    .map_err(|_| CompositeCompileError::ActionCapacity)?,
            });
        }
    }
    Ok((actions, ranges))
}

fn compile_dependency_order(
    candidate_source: LoopIdentity,
    descriptor: &CompositePlanDescriptor,
    catalog: &LoopTargetCatalog,
    installed: &[CompositeDependency],
    limits: CompositePlanLimits,
) -> Result<Vec<LoopIdentity>, CompositeCompileError> {
    let mut edges = BTreeSet::new();
    for dependency in installed {
        if dependency.source == candidate_source {
            continue;
        }
        validate_dependency_identity(dependency.source, catalog)?;
        for &child in &dependency.composite_children {
            validate_dependency_identity(child, catalog)?;
            edges.insert((dependency.source, child));
        }
    }
    for entry in descriptor
        .timelines
        .iter()
        .flat_map(|timeline| &timeline.sections)
        .flat_map(|section| &section.entries)
        .filter(|entry| entry.target.kind == LoopTargetKind::Composite)
    {
        validate_dependency_identity(entry.target, catalog)?;
        edges.insert((candidate_source, entry.target));
    }
    if edges.len() > limits.max_dependency_edges {
        return Err(CompositeCompileError::DependencyEdgeCapacity);
    }

    let mut nodes = BTreeSet::from([candidate_source]);
    for &(parent, child) in &edges {
        nodes.insert(parent);
        nodes.insert(child);
    }
    if nodes.len() > limits.max_dependency_nodes {
        return Err(CompositeCompileError::DependencyNodeCapacity);
    }
    let nodes: Vec<_> = nodes.into_iter().collect();
    let indices: BTreeMap<_, _> = nodes
        .iter()
        .copied()
        .enumerate()
        .map(|(index, identity)| (identity, index))
        .collect();
    let mut outgoing = vec![Vec::<usize>::new(); nodes.len()];
    let mut incoming = vec![0usize; nodes.len()];
    for (parent, child) in edges {
        let parent_index = indices[&parent];
        let child_index = indices[&child];
        outgoing[parent_index].push(child_index);
        incoming[child_index] += 1;
    }

    let mut ready: BTreeSet<_> = incoming
        .iter()
        .enumerate()
        .filter_map(|(index, count)| (*count == 0).then_some(index))
        .collect();
    let mut order = Vec::with_capacity(nodes.len());
    let mut depth = vec![1usize; nodes.len()];
    while let Some(index) = ready.pop_first() {
        order.push(nodes[index]);
        for &child in &outgoing[index] {
            depth[child] = depth[child].max(depth[index] + 1);
            if depth[child] > limits.max_nesting_depth {
                return Err(CompositeCompileError::NestingDepthCapacity);
            }
            incoming[child] -= 1;
            if incoming[child] == 0 {
                ready.insert(child);
            }
        }
    }
    if order.len() != nodes.len() {
        return Err(CompositeCompileError::DependencyCycle);
    }
    Ok(order)
}

fn validate_dependency_identity(
    identity: LoopIdentity,
    catalog: &LoopTargetCatalog,
) -> Result<(), CompositeCompileError> {
    if identity.kind != LoopTargetKind::Composite {
        return Err(CompositeCompileError::NonCompositeDependency);
    }
    match catalog.get(identity) {
        Some(_) => Ok(()),
        None => Err(CompositeCompileError::UnknownDependency),
    }
}
