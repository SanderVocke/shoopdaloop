use shoop_latency::{
    ComponentApplication, LatencyCertainty, LatencyComponentKind, LatencyDomainError,
    LatencyOperationKind, LatencyRangeFrames, ResolvedLatencyRecipe, MAX_RECIPE_COMPONENTS,
};
use std::array;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};

/// Bounded, allocation-free observation used by callback-facing engine surfaces.
///
/// Source and interval identities remain in control-path policy snapshots; this value is the
/// coherently published numeric portion that realtime code needs to detect revisions and latch
/// recipes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeLatencyObservation {
    pub range: Option<LatencyRangeFrames>,
    pub certainty: LatencyCertainty,
    pub sample_rate: u32,
    pub revision: u64,
}

impl RuntimeLatencyObservation {
    pub fn new(
        range: Option<LatencyRangeFrames>,
        certainty: LatencyCertainty,
        sample_rate: u32,
        revision: u64,
    ) -> Result<Self, LatencyDomainError> {
        match (certainty, range) {
            (LatencyCertainty::Exact, Some(range)) if range.min() == range.max() => {}
            (LatencyCertainty::Range, Some(range)) if range.min() < range.max() => {}
            (LatencyCertainty::Estimated, Some(_)) => {}
            (LatencyCertainty::ManualOnly | LatencyCertainty::Unknown, None) => {}
            _ => return Err(LatencyDomainError::CertaintyRangeMismatch),
        }
        if range.is_some() && sample_rate == 0 {
            return Err(LatencyDomainError::ZeroSampleRate);
        }
        Ok(Self {
            range,
            certainty,
            sample_rate,
            revision,
        })
    }

    pub fn exact(frames: u32, sample_rate: u32, revision: u64) -> Result<Self, LatencyDomainError> {
        Self::new(
            Some(LatencyRangeFrames::new(frames, frames)?),
            LatencyCertainty::Exact,
            sample_rate,
            revision,
        )
    }

    pub const fn unknown(sample_rate: u32, revision: u64) -> Self {
        Self {
            range: None,
            certainty: LatencyCertainty::Unknown,
            sample_rate,
            revision,
        }
    }
}

impl Default for RuntimeLatencyObservation {
    fn default() -> Self {
        Self::unknown(0, 0)
    }
}

/// Result of resolving the latency revisions that overlap retained media.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetainedLatencySelection {
    /// The bounded history does not cover the entire requested media interval.
    Unavailable,
    /// Every retained frame was captured under one coherent observation.
    Stable(RuntimeLatencyObservation),
    /// Several observations overlap; callers use the newest deterministically and
    /// retain the revision count as visible variable-history provenance.
    Variable {
        newest: RuntimeLatencyObservation,
        revisions: u32,
    },
}

/// Seqlock publication for a complete runtime latency observation.
///
/// There is one writer per provider/control surface. Callback readers retry if they overlap the
/// short publication window and never take a lock or allocate.
#[derive(Debug)]
pub struct AtomicLatencyObservation {
    generation: AtomicU64,
    minimum: AtomicU32,
    maximum: AtomicU32,
    certainty: AtomicU8,
    sample_rate: AtomicU32,
    revision: AtomicU64,
}

impl AtomicLatencyObservation {
    pub fn new(observation: RuntimeLatencyObservation) -> Self {
        let range = observation.range;
        Self {
            generation: AtomicU64::new(0),
            minimum: AtomicU32::new(range.map(LatencyRangeFrames::min).unwrap_or(0)),
            maximum: AtomicU32::new(range.map(LatencyRangeFrames::max).unwrap_or(0)),
            certainty: AtomicU8::new(certainty_to_u8(observation.certainty)),
            sample_rate: AtomicU32::new(observation.sample_rate),
            revision: AtomicU64::new(observation.revision),
        }
    }

    pub fn publish(&self, observation: RuntimeLatencyObservation) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        let range = observation.range;
        self.minimum.store(
            range.map(LatencyRangeFrames::min).unwrap_or(0),
            Ordering::Relaxed,
        );
        self.maximum.store(
            range.map(LatencyRangeFrames::max).unwrap_or(0),
            Ordering::Relaxed,
        );
        self.sample_rate
            .store(observation.sample_rate, Ordering::Relaxed);
        self.certainty
            .store(certainty_to_u8(observation.certainty), Ordering::Relaxed);
        self.revision.store(observation.revision, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Release);
    }

    pub fn read(&self) -> RuntimeLatencyObservation {
        loop {
            let before = self.generation.load(Ordering::Acquire);
            if before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let certainty = certainty_from_u8(self.certainty.load(Ordering::Relaxed));
            let sample_rate = self.sample_rate.load(Ordering::Relaxed);
            let minimum = self.minimum.load(Ordering::Relaxed);
            let maximum = self.maximum.load(Ordering::Relaxed);
            let revision = self.revision.load(Ordering::Relaxed);
            let after = self.generation.load(Ordering::Acquire);
            if before != after {
                std::hint::spin_loop();
                continue;
            }
            let range = matches!(
                certainty,
                LatencyCertainty::Exact | LatencyCertainty::Range | LatencyCertainty::Estimated
            )
            .then(|| {
                LatencyRangeFrames::new(minimum, maximum)
                    .expect("published runtime latency is validated")
            });
            return RuntimeLatencyObservation {
                range,
                certainty,
                sample_rate,
                revision,
            };
        }
    }
}

impl Default for AtomicLatencyObservation {
    fn default() -> Self {
        Self::new(RuntimeLatencyObservation::default())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeLatencyComponent {
    pub kind: LatencyComponentKind,
    pub observation: RuntimeLatencyObservation,
    pub selected_frames: Option<u32>,
    pub contribution_frames: u32,
    pub application: ComponentApplication,
    pub applied_during_render: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeLatencyRecipe {
    pub operation: LatencyOperationKind,
    pub total_frames: Option<u32>,
    pub revision: u64,
    components: [Option<RuntimeLatencyComponent>; MAX_RECIPE_COMPONENTS],
    n_components: usize,
}

impl RuntimeLatencyRecipe {
    pub fn from_resolved(recipe: &ResolvedLatencyRecipe, revision: u64) -> Self {
        debug_assert!(recipe.components.len() <= MAX_RECIPE_COMPONENTS);
        let mut components = [None; MAX_RECIPE_COMPONENTS];
        for (destination, component) in components.iter_mut().zip(&recipe.components) {
            *destination = Some(RuntimeLatencyComponent {
                kind: component.kind,
                observation: RuntimeLatencyObservation {
                    range: component.observation.range(),
                    certainty: component.observation.certainty(),
                    sample_rate: component.observation.sample_rate(),
                    revision: component.observation.revision(),
                },
                selected_frames: component.selected_frames,
                contribution_frames: component.contribution_frames,
                application: component.application,
                applied_during_render: component.applied_during_render,
            });
        }
        Self {
            operation: recipe.operation,
            total_frames: recipe.total_frames,
            revision,
            components,
            n_components: recipe.components.len().min(MAX_RECIPE_COMPONENTS),
        }
    }

    pub fn components(&self) -> impl Iterator<Item = RuntimeLatencyComponent> + '_ {
        self.components[..self.n_components]
            .iter()
            .flatten()
            .copied()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LatchedLatencyRecipe {
    pub recipe: RuntimeLatencyRecipe,
    pub operation_frame: u64,
    pub changed: bool,
}

impl LatchedLatencyRecipe {
    pub const fn new(recipe: RuntimeLatencyRecipe, operation_frame: u64) -> Self {
        Self {
            recipe,
            operation_frame,
            changed: false,
        }
    }

    pub fn observe(&mut self, kind: LatencyComponentKind, current: RuntimeLatencyObservation) {
        if self
            .recipe
            .components()
            .any(|component| component.kind == kind && component.observation != current)
        {
            self.changed = true;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperationLatencyLatch {
    pending: Option<RuntimeLatencyRecipe>,
    latched: Option<LatchedLatencyRecipe>,
}

impl OperationLatencyLatch {
    pub fn prepare(&mut self, recipe: Option<RuntimeLatencyRecipe>) {
        self.pending = recipe;
    }

    pub const fn pending(&self) -> Option<RuntimeLatencyRecipe> {
        self.pending
    }

    pub const fn latched(&self) -> Option<LatchedLatencyRecipe> {
        self.latched
    }

    pub fn latch(&mut self, operation: LatencyOperationKind, operation_frame: u64) -> bool {
        let Some(recipe) = self.pending.filter(|recipe| recipe.operation == operation) else {
            return false;
        };
        self.latched = Some(LatchedLatencyRecipe::new(recipe, operation_frame));
        true
    }

    pub fn observe(&mut self, kind: LatencyComponentKind, current: RuntimeLatencyObservation) {
        if let Some(latched) = self.latched.as_mut() {
            latched.observe(kind, current);
        }
    }
}

const NO_SELECTED_FRAMES: u64 = u64::MAX;
const NO_TOTAL_FRAMES: u64 = u64::MAX;
const NO_OPERATION_FRAME: u64 = u64::MAX;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PublishedLatencyRecipe {
    pub recipe: Option<RuntimeLatencyRecipe>,
    pub operation_frame: Option<u64>,
    pub changed: bool,
}

#[derive(Debug)]
struct AtomicRuntimeLatencyComponent {
    kind: AtomicU8,
    minimum: AtomicU32,
    maximum: AtomicU32,
    certainty: AtomicU8,
    sample_rate: AtomicU32,
    observation_revision: AtomicU64,
    selected_frames: AtomicU64,
    contribution_frames: AtomicU32,
    application: AtomicU8,
    applied_during_render: AtomicBool,
}

impl Default for AtomicRuntimeLatencyComponent {
    fn default() -> Self {
        Self {
            kind: AtomicU8::new(0),
            minimum: AtomicU32::new(0),
            maximum: AtomicU32::new(0),
            certainty: AtomicU8::new(certainty_to_u8(LatencyCertainty::Unknown)),
            sample_rate: AtomicU32::new(0),
            observation_revision: AtomicU64::new(0),
            selected_frames: AtomicU64::new(NO_SELECTED_FRAMES),
            contribution_frames: AtomicU32::new(0),
            application: AtomicU8::new(0),
            applied_during_render: AtomicBool::new(false),
        }
    }
}

impl AtomicRuntimeLatencyComponent {
    fn publish(&self, component: RuntimeLatencyComponent) {
        let range = component.observation.range;
        self.kind
            .store(component_kind_to_u8(component.kind), Ordering::Relaxed);
        self.minimum.store(
            range.map(LatencyRangeFrames::min).unwrap_or(0),
            Ordering::Relaxed,
        );
        self.maximum.store(
            range.map(LatencyRangeFrames::max).unwrap_or(0),
            Ordering::Relaxed,
        );
        self.certainty.store(
            certainty_to_u8(component.observation.certainty),
            Ordering::Relaxed,
        );
        self.sample_rate
            .store(component.observation.sample_rate, Ordering::Relaxed);
        self.observation_revision
            .store(component.observation.revision, Ordering::Relaxed);
        self.selected_frames.store(
            component
                .selected_frames
                .map(u64::from)
                .unwrap_or(NO_SELECTED_FRAMES),
            Ordering::Relaxed,
        );
        self.contribution_frames
            .store(component.contribution_frames, Ordering::Relaxed);
        self.application
            .store(application_to_u8(component.application), Ordering::Relaxed);
        self.applied_during_render
            .store(component.applied_during_render, Ordering::Relaxed);
    }

    fn read(&self) -> RuntimeLatencyComponent {
        let certainty = certainty_from_u8(self.certainty.load(Ordering::Relaxed));
        let range = matches!(
            certainty,
            LatencyCertainty::Exact | LatencyCertainty::Range | LatencyCertainty::Estimated
        )
        .then(|| {
            LatencyRangeFrames::new(
                self.minimum.load(Ordering::Relaxed),
                self.maximum.load(Ordering::Relaxed),
            )
            .expect("published recipe range is validated")
        });
        let selected = self.selected_frames.load(Ordering::Relaxed);
        RuntimeLatencyComponent {
            kind: component_kind_from_u8(self.kind.load(Ordering::Relaxed)),
            observation: RuntimeLatencyObservation {
                range,
                certainty,
                sample_rate: self.sample_rate.load(Ordering::Relaxed),
                revision: self.observation_revision.load(Ordering::Relaxed),
            },
            selected_frames: (selected != NO_SELECTED_FRAMES).then_some(selected as u32),
            contribution_frames: self.contribution_frames.load(Ordering::Relaxed),
            application: application_from_u8(self.application.load(Ordering::Relaxed)),
            applied_during_render: self.applied_during_render.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug)]
pub struct AtomicLatencyRecipePublication {
    generation: AtomicU64,
    present: AtomicBool,
    operation: AtomicU8,
    total_frames: AtomicU64,
    revision: AtomicU64,
    components: [AtomicRuntimeLatencyComponent; MAX_RECIPE_COMPONENTS],
    n_components: AtomicUsize,
    operation_frame: AtomicU64,
    changed: AtomicBool,
}

impl Default for AtomicLatencyRecipePublication {
    fn default() -> Self {
        Self {
            generation: AtomicU64::new(0),
            present: AtomicBool::new(false),
            operation: AtomicU8::new(0),
            total_frames: AtomicU64::new(NO_TOTAL_FRAMES),
            revision: AtomicU64::new(0),
            components: array::from_fn(|_| AtomicRuntimeLatencyComponent::default()),
            n_components: AtomicUsize::new(0),
            operation_frame: AtomicU64::new(NO_OPERATION_FRAME),
            changed: AtomicBool::new(false),
        }
    }
}

impl AtomicLatencyRecipePublication {
    pub fn publish_pending(&self, recipe: Option<RuntimeLatencyRecipe>) {
        self.publish(recipe, None, false);
    }

    pub fn publish_latched(&self, recipe: Option<LatchedLatencyRecipe>) {
        self.publish(
            recipe.map(|latched| latched.recipe),
            recipe.map(|latched| latched.operation_frame),
            recipe.is_some_and(|latched| latched.changed),
        );
    }

    fn publish(
        &self,
        recipe: Option<RuntimeLatencyRecipe>,
        operation_frame: Option<u64>,
        changed: bool,
    ) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.present.store(recipe.is_some(), Ordering::Relaxed);
        if let Some(recipe) = recipe {
            self.operation
                .store(operation_to_u8(recipe.operation), Ordering::Relaxed);
            self.total_frames.store(
                recipe
                    .total_frames
                    .map(u64::from)
                    .unwrap_or(NO_TOTAL_FRAMES),
                Ordering::Relaxed,
            );
            self.revision.store(recipe.revision, Ordering::Relaxed);
            for (destination, component) in self.components.iter().zip(recipe.components()) {
                destination.publish(component);
            }
            self.n_components
                .store(recipe.n_components, Ordering::Relaxed);
        } else {
            self.n_components.store(0, Ordering::Relaxed);
        }
        self.operation_frame.store(
            operation_frame.unwrap_or(NO_OPERATION_FRAME),
            Ordering::Relaxed,
        );
        self.changed.store(changed, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Release);
    }

    pub fn read(&self) -> PublishedLatencyRecipe {
        loop {
            let before = self.generation.load(Ordering::Acquire);
            if before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let present = self.present.load(Ordering::Relaxed);
            let operation = self.operation.load(Ordering::Relaxed);
            let total = self.total_frames.load(Ordering::Relaxed);
            let revision = self.revision.load(Ordering::Relaxed);
            let n_components = self
                .n_components
                .load(Ordering::Relaxed)
                .min(MAX_RECIPE_COMPONENTS);
            let mut components = [None; MAX_RECIPE_COMPONENTS];
            for (destination, source) in components
                .iter_mut()
                .zip(&self.components)
                .take(n_components)
            {
                *destination = Some(source.read());
            }
            let operation_frame = self.operation_frame.load(Ordering::Relaxed);
            let changed = self.changed.load(Ordering::Relaxed);
            let after = self.generation.load(Ordering::Acquire);
            if before == after {
                let recipe = present.then(|| RuntimeLatencyRecipe {
                    operation: operation_from_u8(operation),
                    total_frames: (total != NO_TOTAL_FRAMES).then_some(total as u32),
                    revision,
                    components,
                    n_components,
                });
                return PublishedLatencyRecipe {
                    recipe,
                    operation_frame: (operation_frame != NO_OPERATION_FRAME)
                        .then_some(operation_frame),
                    changed,
                };
            }
            std::hint::spin_loop();
        }
    }
}

const fn component_kind_to_u8(kind: LatencyComponentKind) -> u8 {
    match kind {
        LatencyComponentKind::ExternalCapture => 0,
        LatencyComponentKind::Processor => 1,
        LatencyComponentKind::CuePlayback => 2,
        LatencyComponentKind::BackendBuffering => 3,
        LatencyComponentKind::Manual => 4,
    }
}

const fn component_kind_from_u8(kind: u8) -> LatencyComponentKind {
    match kind {
        1 => LatencyComponentKind::Processor,
        2 => LatencyComponentKind::CuePlayback,
        3 => LatencyComponentKind::BackendBuffering,
        4 => LatencyComponentKind::Manual,
        _ => LatencyComponentKind::ExternalCapture,
    }
}

const fn application_to_u8(application: ComponentApplication) -> u8 {
    match application {
        ComponentApplication::Applied => 0,
        ComponentApplication::Disabled => 1,
        ComponentApplication::NotApplicable => 2,
        ComponentApplication::Unresolved => 3,
    }
}

const fn application_from_u8(application: u8) -> ComponentApplication {
    match application {
        1 => ComponentApplication::Disabled,
        2 => ComponentApplication::NotApplicable,
        3 => ComponentApplication::Unresolved,
        _ => ComponentApplication::Applied,
    }
}

const fn operation_to_u8(operation: LatencyOperationKind) -> u8 {
    match operation {
        LatencyOperationKind::RecordDirect => 0,
        LatencyOperationKind::RecordDry => 1,
        LatencyOperationKind::RecordWet => 2,
        LatencyOperationKind::DryThroughWet => 3,
        LatencyOperationKind::RecordDryIntoWet => 4,
        LatencyOperationKind::Grab(shoop_latency::LatencyChannelRole::Direct) => 5,
        LatencyOperationKind::Grab(shoop_latency::LatencyChannelRole::Dry) => 6,
        LatencyOperationKind::Grab(shoop_latency::LatencyChannelRole::Wet) => 7,
        LatencyOperationKind::Replacement(shoop_latency::LatencyChannelRole::Direct) => 8,
        LatencyOperationKind::Replacement(shoop_latency::LatencyChannelRole::Dry) => 9,
        LatencyOperationKind::Replacement(shoop_latency::LatencyChannelRole::Wet) => 10,
    }
}

const fn operation_from_u8(operation: u8) -> LatencyOperationKind {
    use shoop_latency::LatencyChannelRole::{Direct, Dry, Wet};
    match operation {
        1 => LatencyOperationKind::RecordDry,
        2 => LatencyOperationKind::RecordWet,
        3 => LatencyOperationKind::DryThroughWet,
        4 => LatencyOperationKind::RecordDryIntoWet,
        5 => LatencyOperationKind::Grab(Direct),
        6 => LatencyOperationKind::Grab(Dry),
        7 => LatencyOperationKind::Grab(Wet),
        8 => LatencyOperationKind::Replacement(Direct),
        9 => LatencyOperationKind::Replacement(Dry),
        10 => LatencyOperationKind::Replacement(Wet),
        _ => LatencyOperationKind::RecordDirect,
    }
}

const fn certainty_to_u8(certainty: LatencyCertainty) -> u8 {
    match certainty {
        LatencyCertainty::Exact => 0,
        LatencyCertainty::Range => 1,
        LatencyCertainty::Estimated => 2,
        LatencyCertainty::ManualOnly => 3,
        LatencyCertainty::Unknown => 4,
    }
}

const fn certainty_from_u8(certainty: u8) -> LatencyCertainty {
    match certainty {
        0 => LatencyCertainty::Exact,
        1 => LatencyCertainty::Range,
        2 => LatencyCertainty::Estimated,
        3 => LatencyCertainty::ManualOnly,
        _ => LatencyCertainty::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn publication_preserves_complete_observations_without_allocation() {
        let observation = RuntimeLatencyObservation::new(
            Some(LatencyRangeFrames::new(7, 11).unwrap()),
            LatencyCertainty::Range,
            48_000,
            9,
        )
        .unwrap();
        let atomic = AtomicLatencyObservation::default();
        assert_no_alloc::assert_no_alloc(|| {
            atomic.publish(observation);
            assert_eq!(atomic.read(), observation);
        });
    }

    #[shoop_wasm_test_support::shoop_test]
    fn operation_latch_changes_only_at_matching_boundaries_and_marks_revisions() {
        let observation = RuntimeLatencyObservation::exact(4, 48_000, 1).unwrap();
        let component = RuntimeLatencyComponent {
            kind: LatencyComponentKind::ExternalCapture,
            observation,
            selected_frames: Some(4),
            contribution_frames: 4,
            application: ComponentApplication::Applied,
            applied_during_render: false,
        };
        let mut components = [None; MAX_RECIPE_COMPONENTS];
        components[0] = Some(component);
        let first = RuntimeLatencyRecipe {
            operation: LatencyOperationKind::RecordDirect,
            total_frames: Some(4),
            revision: 1,
            components,
            n_components: 1,
        };
        let mut latch = OperationLatencyLatch::default();
        latch.prepare(Some(first));
        assert!(!latch.latch(LatencyOperationKind::RecordWet, 10));
        assert!(latch.latched().is_none());
        assert!(latch.latch(LatencyOperationKind::RecordDirect, 11));
        assert_eq!(latch.latched().unwrap().operation_frame, 11);

        let mut second = first;
        second.revision = 2;
        second.total_frames = Some(9);
        latch.prepare(Some(second));
        assert_eq!(latch.latched().unwrap().recipe, first);
        latch.observe(
            LatencyComponentKind::ExternalCapture,
            RuntimeLatencyObservation::exact(9, 48_000, 2).unwrap(),
        );
        assert!(latch.latched().unwrap().changed);
        assert!(latch.latch(LatencyOperationKind::RecordDirect, 20));
        assert_eq!(latch.latched().unwrap().recipe, second);
        assert!(!latch.latched().unwrap().changed);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn recipe_publication_is_complete_and_allocation_free() {
        let observation = RuntimeLatencyObservation::exact(11, 48_000, 9).unwrap();
        let component = RuntimeLatencyComponent {
            kind: LatencyComponentKind::ExternalCapture,
            observation,
            selected_frames: Some(11),
            contribution_frames: 11,
            application: ComponentApplication::Applied,
            applied_during_render: false,
        };
        let mut components = [None; MAX_RECIPE_COMPONENTS];
        components[0] = Some(component);
        let recipe = RuntimeLatencyRecipe {
            operation: LatencyOperationKind::RecordDirect,
            total_frames: Some(11),
            revision: 13,
            components,
            n_components: 1,
        };
        let latched = LatchedLatencyRecipe {
            recipe,
            operation_frame: 101,
            changed: true,
        };
        let publication = AtomicLatencyRecipePublication::default();
        assert_no_alloc::assert_no_alloc(|| {
            publication.publish_latched(Some(latched));
            assert_eq!(
                publication.read(),
                PublishedLatencyRecipe {
                    recipe: Some(recipe),
                    operation_frame: Some(101),
                    changed: true,
                }
            );
        });
    }
}
