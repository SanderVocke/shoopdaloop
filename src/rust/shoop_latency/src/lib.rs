use thiserror::Error;

pub const MAX_COMPENSATION_FRAMES: u32 = 768_000;
pub const MAX_RETAINED_MARGIN_FRAMES: u32 = 768_000;
pub const MAX_LATENCY_PATHS: usize = 256;
pub const MAX_RECIPE_COMPONENTS: usize = 16;
pub const MAX_OBSERVATION_HISTORY: usize = 4_096;
pub const MAX_SOURCE_IDENTITY_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LatencyRangeFrames {
    min: u32,
    max: u32,
}

impl LatencyRangeFrames {
    pub fn new(min: u32, max: u32) -> Result<Self, LatencyDomainError> {
        if min > max {
            return Err(LatencyDomainError::InvertedRange { min, max });
        }
        if max > MAX_COMPENSATION_FRAMES {
            return Err(LatencyDomainError::ValueExceedsMaximum(max));
        }
        Ok(Self { min, max })
    }

    pub const fn min(self) -> u32 {
        self.min
    }

    pub const fn max(self) -> u32 {
        self.max
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LatencyCertainty {
    Exact,
    Range,
    Estimated,
    ManualOnly,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LatencyComponentKind {
    ExternalCapture,
    Processor,
    CuePlayback,
    BackendBuffering,
    Manual,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceIdentity(String);

impl SourceIdentity {
    pub fn new(value: impl Into<String>) -> Result<Self, LatencyDomainError> {
        let value = value.into();
        if value.is_empty() {
            return Err(LatencyDomainError::EmptySourceIdentity);
        }
        if value.len() > MAX_SOURCE_IDENTITY_BYTES {
            return Err(LatencyDomainError::SourceIdentityTooLong(value.len()));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LatencyIntervalIdentity(SourceIdentity);

impl LatencyIntervalIdentity {
    pub fn new(value: impl Into<String>) -> Result<Self, LatencyDomainError> {
        Ok(Self(SourceIdentity::new(value)?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LatencyObservation {
    range: Option<LatencyRangeFrames>,
    certainty: LatencyCertainty,
    sample_rate: u32,
    revision: u64,
    source_identity: SourceIdentity,
    interval: Option<LatencyIntervalIdentity>,
}

impl LatencyObservation {
    pub fn new(
        range: Option<LatencyRangeFrames>,
        certainty: LatencyCertainty,
        sample_rate: u32,
        revision: u64,
        source_identity: SourceIdentity,
        interval: Option<LatencyIntervalIdentity>,
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
        if matches!(
            certainty,
            LatencyCertainty::Exact | LatencyCertainty::Range | LatencyCertainty::Estimated
        ) && interval.is_none()
        {
            return Err(LatencyDomainError::MissingAutomaticInterval);
        }
        Ok(Self {
            range,
            certainty,
            sample_rate,
            revision,
            source_identity,
            interval,
        })
    }

    pub fn exact(
        frames: u32,
        sample_rate: u32,
        revision: u64,
        source_identity: SourceIdentity,
        interval: LatencyIntervalIdentity,
    ) -> Result<Self, LatencyDomainError> {
        Self::new(
            Some(LatencyRangeFrames::new(frames, frames)?),
            LatencyCertainty::Exact,
            sample_rate,
            revision,
            source_identity,
            Some(interval),
        )
    }

    pub fn unknown(
        sample_rate: u32,
        revision: u64,
        source_identity: SourceIdentity,
    ) -> Result<Self, LatencyDomainError> {
        Self::new(
            None,
            LatencyCertainty::Unknown,
            sample_rate,
            revision,
            source_identity,
            None,
        )
    }

    pub fn manual_only(
        sample_rate: u32,
        revision: u64,
        source_identity: SourceIdentity,
    ) -> Result<Self, LatencyDomainError> {
        Self::new(
            None,
            LatencyCertainty::ManualOnly,
            sample_rate,
            revision,
            source_identity,
            None,
        )
    }

    pub const fn range(&self) -> Option<LatencyRangeFrames> {
        self.range
    }

    pub const fn certainty(&self) -> LatencyCertainty {
        self.certainty
    }

    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn source_identity(&self) -> &SourceIdentity {
        &self.source_identity
    }

    pub fn interval(&self) -> Option<&LatencyIntervalIdentity> {
        self.interval.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LatencyRangeSelection {
    Minimum,
    Maximum,
    Midpoint,
}

impl LatencyRangeSelection {
    pub fn select(self, range: LatencyRangeFrames) -> u32 {
        match self {
            Self::Minimum => range.min(),
            Self::Maximum => range.max(),
            Self::Midpoint => range.min() + (range.max() - range.min()) / 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LatencyValueMode {
    Automatic,
    Manual(u32),
    AutomaticPlusTrim(i32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LatencyComponentPolicy {
    pub enabled: bool,
    pub value_mode: LatencyValueMode,
    pub range_selection: LatencyRangeSelection,
}

impl Default for LatencyComponentPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            value_mode: LatencyValueMode::Automatic,
            range_selection: LatencyRangeSelection::Maximum,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LatencyChannelRole {
    Direct,
    Dry,
    Wet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingReference {
    ExternalWorld,
    ShoopCue,
    ExternalPhysicalClock,
}

impl RecordingReference {
    const fn uses_cue_playback(self) -> bool {
        matches!(self, Self::ShoopCue | Self::ExternalPhysicalClock)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LatencyOperationKind {
    RecordDirect,
    RecordDry,
    RecordWet,
    DryThroughWet,
    RecordDryIntoWet,
    Grab(LatencyChannelRole),
    Replacement(LatencyChannelRole),
}

impl LatencyOperationKind {
    fn role(self) -> LatencyChannelRole {
        match self {
            Self::RecordDirect => LatencyChannelRole::Direct,
            Self::RecordDry | Self::DryThroughWet | Self::RecordDryIntoWet => {
                LatencyChannelRole::Dry
            }
            Self::RecordWet => LatencyChannelRole::Wet,
            Self::Grab(role) | Self::Replacement(role) => role,
        }
    }

    fn is_render(self) -> bool {
        matches!(self, Self::DryThroughWet | Self::RecordDryIntoWet)
    }

    fn component_is_applicable(self, kind: LatencyComponentKind) -> bool {
        if self.is_render() {
            return matches!(
                kind,
                LatencyComponentKind::Processor
                    | LatencyComponentKind::BackendBuffering
                    | LatencyComponentKind::Manual
            );
        }
        match self.role() {
            LatencyChannelRole::Direct | LatencyChannelRole::Dry => {
                !matches!(kind, LatencyComponentKind::Processor)
            }
            LatencyChannelRole::Wet => true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LatencyComponentInput {
    pub kind: LatencyComponentKind,
    pub observation: LatencyObservation,
    pub policy: LatencyComponentPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentApplication {
    Applied,
    Disabled,
    NotApplicable,
    Unresolved,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedLatencyComponent {
    pub kind: LatencyComponentKind,
    pub observation: LatencyObservation,
    pub policy: LatencyComponentPolicy,
    pub selected_frames: Option<u32>,
    pub contribution_frames: u32,
    pub application: ComponentApplication,
    pub applied_during_render: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnresolvedReason {
    AutomaticValueUnavailable,
    NegativeEffectiveValue,
    ValueExceedsMaximum,
    OverlappingAutomaticInterval,
    TotalOverflow,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LatencyResolutionWarning {
    pub component: Option<LatencyComponentKind>,
    pub reason: UnresolvedReason,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedLatencyRecipe {
    pub operation: LatencyOperationKind,
    pub recording_reference: RecordingReference,
    pub components: Vec<ResolvedLatencyComponent>,
    pub total_frames: Option<u32>,
    pub warnings: Vec<LatencyResolutionWarning>,
}

impl ResolvedLatencyRecipe {
    pub fn is_resolved(&self) -> bool {
        self.total_frames.is_some()
    }
}

pub fn resolve_latency_recipe(
    operation: LatencyOperationKind,
    recording_reference: RecordingReference,
    inputs: &[LatencyComponentInput],
) -> Result<ResolvedLatencyRecipe, LatencyDomainError> {
    if inputs.len() > MAX_RECIPE_COMPONENTS {
        return Err(LatencyDomainError::TooManyRecipeComponents(inputs.len()));
    }
    Ok(ResolvedLatencyRecipe {
        operation,
        recording_reference,
        components: inputs
            .iter()
            .map(|input| {
                let cue_applicable = input.kind != LatencyComponentKind::CuePlayback
                    || recording_reference.uses_cue_playback();
                let applicable = operation.component_is_applicable(input.kind) && cue_applicable;
                if applicable && input.policy.enabled {
                    let _ = select_component_value(&input.observation, input.policy);
                }
                ResolvedLatencyComponent {
                    kind: input.kind,
                    observation: input.observation.clone(),
                    policy: input.policy,
                    selected_frames: None,
                    contribution_frames: 0,
                    application: if !applicable {
                        ComponentApplication::NotApplicable
                    } else if !input.policy.enabled {
                        ComponentApplication::Disabled
                    } else {
                        ComponentApplication::Unresolved
                    },
                    applied_during_render: false,
                }
            })
            .collect(),
        total_frames: None,
        warnings: vec![LatencyResolutionWarning {
            component: None,
            reason: UnresolvedReason::Unsupported,
        }],
    })
}

fn select_component_value(
    observation: &LatencyObservation,
    policy: LatencyComponentPolicy,
) -> Result<u32, UnresolvedReason> {
    match policy.value_mode {
        LatencyValueMode::Manual(value) => {
            if value > MAX_COMPENSATION_FRAMES {
                Err(UnresolvedReason::ValueExceedsMaximum)
            } else {
                Ok(value)
            }
        }
        LatencyValueMode::Automatic | LatencyValueMode::AutomaticPlusTrim(_) => {
            let range = observation
                .range()
                .ok_or(UnresolvedReason::AutomaticValueUnavailable)?;
            let automatic = policy.range_selection.select(range);
            let trim = match policy.value_mode {
                LatencyValueMode::AutomaticPlusTrim(trim) => trim,
                _ => 0,
            };
            let value = i64::from(automatic) + i64::from(trim);
            if value < 0 {
                return Err(UnresolvedReason::NegativeEffectiveValue);
            }
            if value > i64::from(MAX_COMPENSATION_FRAMES) {
                return Err(UnresolvedReason::ValueExceedsMaximum);
            }
            Ok(value as u32)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PathAggregation {
    Equivalent(LatencyObservation),
    Ranged(LatencyObservation),
    Unknown,
    Ambiguous(PathAmbiguity),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathAmbiguity {
    NoPaths,
    TooManyPaths,
    MixedSampleRates,
    MissingRange,
}

pub fn aggregate_latency_paths(
    _paths: &[LatencyObservation],
    _aggregate_source: SourceIdentity,
    _aggregate_interval: LatencyIntervalIdentity,
) -> Result<PathAggregation, LatencyDomainError> {
    Ok(PathAggregation::Unknown)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TakeLatencyStatus {
    pub changed: bool,
    pub incomplete: bool,
    pub variable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TakeLatencySnapshot {
    pub operation: LatencyOperationKind,
    pub sample_rate: u32,
    pub operation_frame: u64,
    pub operation_revision: u64,
    pub resolved_total_frames: u32,
    pub retained_before_frames: u32,
    pub retained_after_frames: u32,
    pub components: Vec<ResolvedLatencyComponent>,
    pub status: TakeLatencyStatus,
}

impl TakeLatencySnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        recipe: &ResolvedLatencyRecipe,
        sample_rate: u32,
        operation_frame: u64,
        operation_revision: u64,
        retained_before_frames: u32,
        retained_after_frames: u32,
        status: TakeLatencyStatus,
    ) -> Result<Self, LatencyDomainError> {
        let resolved_total_frames = recipe.total_frames.unwrap_or_default();
        if sample_rate == 0 {
            return Err(LatencyDomainError::ZeroSampleRate);
        }
        if retained_before_frames > MAX_RETAINED_MARGIN_FRAMES
            || retained_after_frames > MAX_RETAINED_MARGIN_FRAMES
        {
            return Err(LatencyDomainError::RetainedMarginExceedsMaximum);
        }
        Ok(Self {
            operation: recipe.operation,
            sample_rate,
            operation_frame,
            operation_revision,
            resolved_total_frames,
            retained_before_frames,
            retained_after_frames,
            components: recipe.components.clone(),
            status: TakeLatencyStatus {
                incomplete: recipe.total_frames.is_none() || status.incomplete,
                ..status
            },
        })
    }

    pub fn detect_revision_change(&mut self, current: &[LatencyComponentInput]) {
        self.status.changed |= self.components.iter().any(|latched| {
            current.iter().any(|candidate| {
                candidate.kind == latched.kind
                    && candidate.observation.source_identity()
                        == latched.observation.source_identity()
                    && candidate.observation.revision() != latched.observation.revision()
            })
        });
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum LatencyDomainError {
    #[error("latency range is inverted: {min}..{max}")]
    InvertedRange { min: u32, max: u32 },
    #[error("latency value {0} exceeds the supported maximum")]
    ValueExceedsMaximum(u32),
    #[error("latency certainty and range do not agree")]
    CertaintyRangeMismatch,
    #[error("a meaningful latency observation has a zero sample rate")]
    ZeroSampleRate,
    #[error("an automatic latency observation has no interval identity")]
    MissingAutomaticInterval,
    #[error("latency source identity is empty")]
    EmptySourceIdentity,
    #[error("latency source identity has {0} bytes")]
    SourceIdentityTooLong(usize),
    #[error("recipe has {0} components")]
    TooManyRecipeComponents(usize),
    #[error("latency recipe is unresolved")]
    UnresolvedRecipe,
    #[error("retained latency margin exceeds the supported maximum")]
    RetainedMarginExceedsMaximum,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(value: &str) -> SourceIdentity {
        SourceIdentity::new(value).unwrap()
    }

    fn interval(value: &str) -> LatencyIntervalIdentity {
        LatencyIntervalIdentity::new(value).unwrap()
    }

    fn exact(kind: LatencyComponentKind, frames: u32, scope: &str) -> LatencyComponentInput {
        LatencyComponentInput {
            kind,
            observation: LatencyObservation::exact(
                frames,
                48_000,
                1,
                source(&format!("source:{scope}")),
                interval(scope),
            )
            .unwrap(),
            policy: LatencyComponentPolicy::default(),
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn checked_observations_preserve_truthful_certainty() {
        assert_eq!(
            LatencyRangeFrames::new(2, 1),
            Err(LatencyDomainError::InvertedRange { min: 2, max: 1 })
        );
        assert!(LatencyObservation::new(
            Some(LatencyRangeFrames::new(1, 2).unwrap()),
            LatencyCertainty::Exact,
            48_000,
            1,
            source("input"),
            Some(interval("capture")),
        )
        .is_err());
        assert!(LatencyObservation::unknown(48_000, 1, source("unknown"))
            .unwrap()
            .range()
            .is_none());
        assert!(LatencyObservation::manual_only(48_000, 1, source("manual"))
            .unwrap()
            .range()
            .is_none());
        assert!(LatencyObservation::exact(1, 0, 1, source("input"), interval("capture")).is_err());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn every_range_selection_stays_inside_the_range() {
        for min in 0..=32 {
            for max in min..=32 {
                let range = LatencyRangeFrames::new(min, max).unwrap();
                for selection in [
                    LatencyRangeSelection::Minimum,
                    LatencyRangeSelection::Maximum,
                    LatencyRangeSelection::Midpoint,
                ] {
                    let selected = selection.select(range);
                    assert!(selected >= min && selected <= max);
                }
            }
        }
        assert_eq!(
            LatencyRangeSelection::Midpoint.select(LatencyRangeFrames::new(2, 5).unwrap()),
            3
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn component_modes_cover_disable_manual_trim_unknown_and_bounds() {
        let ranged = LatencyObservation::new(
            Some(LatencyRangeFrames::new(10, 20).unwrap()),
            LatencyCertainty::Range,
            48_000,
            1,
            source("processor"),
            Some(interval("processor-path")),
        )
        .unwrap();
        let cases = [
            (
                LatencyComponentPolicy {
                    enabled: true,
                    value_mode: LatencyValueMode::Automatic,
                    range_selection: LatencyRangeSelection::Maximum,
                },
                Some(20),
            ),
            (
                LatencyComponentPolicy {
                    enabled: true,
                    value_mode: LatencyValueMode::AutomaticPlusTrim(-3),
                    range_selection: LatencyRangeSelection::Midpoint,
                },
                Some(12),
            ),
            (
                LatencyComponentPolicy {
                    enabled: true,
                    value_mode: LatencyValueMode::Manual(7),
                    range_selection: LatencyRangeSelection::Minimum,
                },
                Some(7),
            ),
        ];
        for (policy, expected) in cases {
            assert_eq!(select_component_value(&ranged, policy).ok(), expected);
        }
        assert_eq!(
            select_component_value(
                &ranged,
                LatencyComponentPolicy {
                    enabled: true,
                    value_mode: LatencyValueMode::AutomaticPlusTrim(-21),
                    range_selection: LatencyRangeSelection::Maximum,
                }
            ),
            Err(UnresolvedReason::NegativeEffectiveValue)
        );
        let unknown = LatencyObservation::unknown(48_000, 1, source("unknown")).unwrap();
        assert_eq!(
            select_component_value(&unknown, LatencyComponentPolicy::default()),
            Err(UnresolvedReason::AutomaticValueUnavailable)
        );
        assert_eq!(
            select_component_value(
                &unknown,
                LatencyComponentPolicy {
                    enabled: true,
                    value_mode: LatencyValueMode::Manual(9),
                    range_selection: LatencyRangeSelection::Maximum,
                }
            ),
            Ok(9)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn operation_recipes_enforce_component_and_cue_semantics() {
        let inputs = vec![
            exact(LatencyComponentKind::ExternalCapture, 3, "capture"),
            exact(LatencyComponentKind::Processor, 5, "processor"),
            exact(LatencyComponentKind::CuePlayback, 7, "cue"),
            exact(LatencyComponentKind::BackendBuffering, 2, "backend"),
            exact(LatencyComponentKind::Manual, 1, "manual"),
        ];
        let direct_world = resolve_latency_recipe(
            LatencyOperationKind::RecordDirect,
            RecordingReference::ExternalWorld,
            &inputs,
        )
        .unwrap();
        assert_eq!(direct_world.total_frames, Some(6));
        let direct_cue = resolve_latency_recipe(
            LatencyOperationKind::RecordDirect,
            RecordingReference::ShoopCue,
            &inputs,
        )
        .unwrap();
        assert_eq!(direct_cue.total_frames, Some(13));
        let wet_cue = resolve_latency_recipe(
            LatencyOperationKind::RecordWet,
            RecordingReference::ShoopCue,
            &inputs,
        )
        .unwrap();
        assert_eq!(wet_cue.total_frames, Some(18));
        let render = resolve_latency_recipe(
            LatencyOperationKind::DryThroughWet,
            RecordingReference::ShoopCue,
            &inputs,
        )
        .unwrap();
        assert_eq!(render.total_frames, Some(8));
        let rerecord = resolve_latency_recipe(
            LatencyOperationKind::RecordDryIntoWet,
            RecordingReference::ExternalWorld,
            &inputs,
        )
        .unwrap();
        assert_eq!(rerecord.total_frames, Some(8));
        assert!(rerecord.components.iter().any(|component| {
            component.kind == LatencyComponentKind::Processor && component.applied_during_render
        }));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn disabled_unknown_is_zero_but_enabled_unknown_is_unresolved() {
        let mut input = LatencyComponentInput {
            kind: LatencyComponentKind::ExternalCapture,
            observation: LatencyObservation::unknown(48_000, 1, source("input")).unwrap(),
            policy: LatencyComponentPolicy::default(),
        };
        let unresolved = resolve_latency_recipe(
            LatencyOperationKind::RecordDirect,
            RecordingReference::ExternalWorld,
            &[input.clone()],
        )
        .unwrap();
        assert_eq!(unresolved.total_frames, None);
        assert_eq!(
            unresolved.components[0].application,
            ComponentApplication::Unresolved
        );

        input.policy.enabled = false;
        let disabled = resolve_latency_recipe(
            LatencyOperationKind::RecordDirect,
            RecordingReference::ExternalWorld,
            &[input],
        )
        .unwrap();
        assert_eq!(disabled.total_frames, Some(0));
        assert_eq!(
            disabled.components[0].application,
            ComponentApplication::Disabled
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn overlapping_automatic_intervals_are_not_double_counted() {
        let inputs = vec![
            exact(
                LatencyComponentKind::ExternalCapture,
                4,
                "physical-to-input",
            ),
            exact(
                LatencyComponentKind::BackendBuffering,
                2,
                "physical-to-input",
            ),
        ];
        let recipe = resolve_latency_recipe(
            LatencyOperationKind::RecordDirect,
            RecordingReference::ExternalWorld,
            &inputs,
        )
        .unwrap();
        assert_eq!(recipe.total_frames, None);
        assert!(recipe
            .warnings
            .iter()
            .any(|warning| { warning.reason == UnresolvedReason::OverlappingAutomaticInterval }));
        assert!(recipe
            .components
            .iter()
            .all(|component| component.contribution_frames == 0));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn every_component_toggle_and_mode_resolves_independently() {
        for (index, kind) in [
            LatencyComponentKind::ExternalCapture,
            LatencyComponentKind::Processor,
            LatencyComponentKind::CuePlayback,
            LatencyComponentKind::BackendBuffering,
            LatencyComponentKind::Manual,
        ]
        .into_iter()
        .enumerate()
        {
            let observation = LatencyObservation::new(
                Some(LatencyRangeFrames::new(2, 6).unwrap()),
                LatencyCertainty::Range,
                48_000,
                1,
                source(&format!("component-{index}")),
                Some(interval(&format!("interval-{index}"))),
            )
            .unwrap();
            for (mode, selection, expected) in [
                (
                    LatencyValueMode::Automatic,
                    LatencyRangeSelection::Minimum,
                    2,
                ),
                (
                    LatencyValueMode::Automatic,
                    LatencyRangeSelection::Midpoint,
                    4,
                ),
                (
                    LatencyValueMode::Automatic,
                    LatencyRangeSelection::Maximum,
                    6,
                ),
                (
                    LatencyValueMode::AutomaticPlusTrim(-1),
                    LatencyRangeSelection::Maximum,
                    5,
                ),
                (
                    LatencyValueMode::AutomaticPlusTrim(3),
                    LatencyRangeSelection::Minimum,
                    5,
                ),
                (
                    LatencyValueMode::Manual(9),
                    LatencyRangeSelection::Minimum,
                    9,
                ),
            ] {
                let input = LatencyComponentInput {
                    kind,
                    observation: observation.clone(),
                    policy: LatencyComponentPolicy {
                        enabled: true,
                        value_mode: mode,
                        range_selection: selection,
                    },
                };
                let recipe = resolve_latency_recipe(
                    LatencyOperationKind::RecordWet,
                    RecordingReference::ShoopCue,
                    &[input],
                )
                .unwrap();
                assert_eq!(recipe.total_frames, Some(expected), "kind={kind:?}");
            }

            let disabled = LatencyComponentInput {
                kind,
                observation,
                policy: LatencyComponentPolicy {
                    enabled: false,
                    ..LatencyComponentPolicy::default()
                },
            };
            let recipe = resolve_latency_recipe(
                LatencyOperationKind::RecordWet,
                RecordingReference::ShoopCue,
                &[disabled],
            )
            .unwrap();
            assert_eq!(recipe.total_frames, Some(0), "kind={kind:?}");
            assert_eq!(
                recipe.components[0].application,
                ComponentApplication::Disabled,
                "kind={kind:?}"
            );
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn grab_and_replacement_follow_their_channel_roles() {
        let inputs = vec![
            exact(LatencyComponentKind::ExternalCapture, 3, "capture"),
            exact(LatencyComponentKind::Processor, 5, "processor"),
            exact(LatencyComponentKind::CuePlayback, 7, "cue"),
            exact(LatencyComponentKind::BackendBuffering, 2, "backend"),
            exact(LatencyComponentKind::Manual, 1, "manual"),
        ];
        for operation in [
            LatencyOperationKind::Grab(LatencyChannelRole::Direct),
            LatencyOperationKind::Grab(LatencyChannelRole::Dry),
            LatencyOperationKind::Replacement(LatencyChannelRole::Direct),
            LatencyOperationKind::Replacement(LatencyChannelRole::Dry),
        ] {
            let recipe =
                resolve_latency_recipe(operation, RecordingReference::ExternalWorld, &inputs)
                    .unwrap();
            assert_eq!(recipe.total_frames, Some(6), "operation={operation:?}");
        }
        for operation in [
            LatencyOperationKind::Grab(LatencyChannelRole::Wet),
            LatencyOperationKind::Replacement(LatencyChannelRole::Wet),
        ] {
            let recipe =
                resolve_latency_recipe(operation, RecordingReference::ExternalWorld, &inputs)
                    .unwrap();
            assert_eq!(recipe.total_frames, Some(11), "operation={operation:?}");
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn checked_recipe_summation_rejects_limits_and_capacity_overflow() {
        let inputs = vec![
            exact(
                LatencyComponentKind::ExternalCapture,
                500_000,
                "capture-large",
            ),
            exact(LatencyComponentKind::Processor, 500_000, "processor-large"),
        ];
        let recipe = resolve_latency_recipe(
            LatencyOperationKind::RecordWet,
            RecordingReference::ExternalWorld,
            &inputs,
        )
        .unwrap();
        assert_eq!(recipe.total_frames, None);
        assert!(recipe
            .warnings
            .iter()
            .any(|warning| warning.reason == UnresolvedReason::TotalOverflow));
        assert_eq!(
            LatencyRangeFrames::new(0, MAX_COMPENSATION_FRAMES + 1),
            Err(LatencyDomainError::ValueExceedsMaximum(
                MAX_COMPENSATION_FRAMES + 1
            ))
        );

        let component = exact(LatencyComponentKind::Manual, 0, "manual-capacity");
        let too_many = vec![component; MAX_RECIPE_COMPONENTS + 1];
        assert_eq!(
            resolve_latency_recipe(
                LatencyOperationKind::RecordWet,
                RecordingReference::ShoopCue,
                &too_many
            ),
            Err(LatencyDomainError::TooManyRecipeComponents(
                MAX_RECIPE_COMPONENTS + 1
            ))
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn path_aggregation_distinguishes_equivalent_ranged_unknown_and_ambiguous() {
        let a = LatencyObservation::exact(3, 48_000, 1, source("a"), interval("a")).unwrap();
        let b = LatencyObservation::exact(3, 48_000, 2, source("b"), interval("b")).unwrap();
        assert!(matches!(
            aggregate_latency_paths(&[a.clone(), b], source("aggregate"), interval("aggregate"))
                .unwrap(),
            PathAggregation::Equivalent(_)
        ));
        let c = LatencyObservation::exact(7, 48_000, 3, source("c"), interval("c")).unwrap();
        let ranged =
            aggregate_latency_paths(&[a.clone(), c], source("aggregate"), interval("aggregate"))
                .unwrap();
        assert!(matches!(ranged, PathAggregation::Ranged(_)));
        let unknown = LatencyObservation::unknown(48_000, 1, source("unknown")).unwrap();
        assert_eq!(
            aggregate_latency_paths(
                &[a.clone(), unknown],
                source("aggregate"),
                interval("aggregate")
            )
            .unwrap(),
            PathAggregation::Unknown
        );
        let other_rate =
            LatencyObservation::exact(3, 44_100, 1, source("other"), interval("other")).unwrap();
        assert_eq!(
            aggregate_latency_paths(&[a, other_rate], source("aggregate"), interval("aggregate"))
                .unwrap(),
            PathAggregation::Ambiguous(PathAmbiguity::MixedSampleRates)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn take_snapshot_is_frozen_and_detects_later_revision_changes() {
        let input = exact(LatencyComponentKind::ExternalCapture, 4, "capture");
        let recipe = resolve_latency_recipe(
            LatencyOperationKind::RecordDirect,
            RecordingReference::ExternalWorld,
            &[input.clone()],
        )
        .unwrap();
        let mut snapshot =
            TakeLatencySnapshot::new(&recipe, 48_000, 100, 7, 4, 4, TakeLatencyStatus::default())
                .unwrap();
        let mut changed = input;
        changed.observation =
            LatencyObservation::exact(9, 48_000, 2, source("source:capture"), interval("capture"))
                .unwrap();
        snapshot.detect_revision_change(&[changed]);
        assert!(snapshot.status.changed);
        assert_eq!(snapshot.resolved_total_frames, 4);
    }

    #[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
    shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
}
