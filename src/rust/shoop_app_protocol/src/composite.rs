//! Authored arrangements and immutable effective timing shared by service clients.
use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};
use shoop_app_api::{CompositeKind, LoopId, LoopMode};

pub const MAX_EVENTS: usize = 16_384;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TimingBasis {
    pub sample_rate: u32,
    pub unit_frames: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Instance {
    pub instance_id: u64,
    pub loop_id: LoopId,
    pub start_cycle: u64,
    pub n_cycles: Option<u32>,
    pub mode: Option<LoopMode>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Document {
    pub kind: CompositeKind,
    pub basis: TimingBasis,
    pub instances: Vec<Instance>,
}

impl Document {
    pub fn valid(&self) -> bool {
        let mut ids = alloc::collections::BTreeSet::new();
        self.kind != CompositeKind::None
            && self.basis.sample_rate != 0
            && self.basis.unit_frames != 0
            && self.instances.len() <= MAX_EVENTS
            && self.instances.iter().all(|event| {
                event.instance_id != 0
                    && ids.insert(event.instance_id)
                    && event.loop_id.is_valid()
                    && event.n_cycles != Some(0)
                    && match self.kind {
                        CompositeKind::Regular => event.mode.is_none(),
                        CompositeKind::Script => {
                            event.mode.is_some_and(|mode| mode != LoopMode::Unknown)
                        }
                        CompositeKind::None => false,
                    }
            })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Span {
    pub target: LoopId,
    pub start: u64,
    pub end: u64,
    pub occurrence_start: u64,
    pub ordinal: u32,
    pub mode: LoopMode,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EffectiveInstance {
    pub instance_id: u64,
    pub start_frame: u64,
    pub end_frame: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Prepared {
    pub sample_rate: u32,
    pub quantum: u32,
    pub unit_frames: u64,
    pub iterations: u64,
    pub spans: Vec<Span>,
    pub instances: Vec<EffectiveInstance>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Content {
    pub document: Document,
    pub prepared: Prepared,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Versioned {
    pub version: u64,
    pub document: Document,
    pub prepared: Prepared,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PendingStage {
    Queued,
    Staging,
    AwaitingRestart,
    Confirming,
    Cancelling,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct View {
    pub mode: LoopMode,
    pub position_frames: u64,
    pub length_frames: u64,
    pub iteration: u64,
    pub passes: u64,
    pub next_mode: Option<LoopMode>,
    pub next_transition_delay: Option<u32>,
    pub observed_frame: Option<u64>,
    pub repeat_sync: bool,
    pub error: Option<String>,
    pub working: Option<Content>,
    pub loop_id: LoopId,
    pub active: Option<Versioned>,
    pub pending: Option<Versioned>,
    pub pending_stage: Option<PendingStage>,
    pub waiting_repeat: bool,
    pub active_applied_frame: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Edit {
    Convert {
        target: LoopId,
    },
    Append {
        target: LoopId,
        add: Vec<LoopId>,
        parallel: bool,
        convert_if_empty: bool,
    },
    Insert {
        target: LoopId,
        source: LoopId,
        start_cycle: u64,
    },
    Delete {
        target: LoopId,
        events: Vec<u64>,
    },
    Relocate {
        target: LoopId,
        events: Vec<u64>,
        start_cycle: u64,
        duplicate: bool,
    },
    Duration {
        target: LoopId,
        event: u64,
        n_cycles: Option<u32>,
    },
    Kind {
        target: LoopId,
        kind: CompositeKind,
    },
    Mode {
        target: LoopId,
        event: u64,
        mode: LoopMode,
    },
    Timing {
        target: LoopId,
        basis: TimingBasis,
    },
    Cancel {
        target: LoopId,
        version: u64,
    },
}

impl Edit {
    pub fn target(&self) -> LoopId {
        match self {
            Self::Convert { target }
            | Self::Append { target, .. }
            | Self::Insert { target, .. }
            | Self::Delete { target, .. }
            | Self::Relocate { target, .. }
            | Self::Duration { target, .. }
            | Self::Kind { target, .. }
            | Self::Mode { target, .. }
            | Self::Timing { target, .. }
            | Self::Cancel { target, .. } => *target,
        }
    }
    pub fn bounded(&self) -> bool {
        self.target().is_valid()
            && match self {
                Self::Append { add, .. } => add.len() <= 128 && add.iter().all(|id| id.is_valid()),
                Self::Insert { source, .. } => source.is_valid(),
                Self::Delete { events, .. } | Self::Relocate { events, .. } => {
                    events.len() <= MAX_EVENTS && events.iter().all(|id| *id != 0)
                }
                Self::Duration {
                    event, n_cycles, ..
                } => *event != 0 && *n_cycles != Some(0),
                Self::Kind { kind, .. } => *kind != CompositeKind::None,
                Self::Mode { event, mode, .. } => *event != 0 && *mode != LoopMode::Unknown,
                Self::Timing { basis, .. } => basis.sample_rate != 0 && basis.unit_frames != 0,
                Self::Cancel { version, .. } => *version != 0,
                Self::Convert { .. } => true,
            }
    }
}

impl Versioned {
    pub fn valid(&self) -> bool {
        self.version != 0 && valid_content(&self.document, &self.prepared)
    }
}
impl Content {
    pub fn valid(&self) -> bool {
        valid_content(&self.document, &self.prepared)
    }
}
fn valid_content(document: &Document, p: &Prepared) -> bool {
    if !document.valid()
        || !(shoop_app_api::ConversionTarget {
            sample_rate: p.sample_rate,
            quantum_frames: p.quantum,
        })
        .is_valid()
        || p.unit_frames == 0
        || p.unit_frames % u64::from(p.quantum) != 0
        || p.unit_frames.checked_mul(p.iterations).is_none()
        || p.spans.len() > MAX_EVENTS * 2
        || p.instances.len() != document.instances.len()
        || (p.iterations == 0) != p.spans.is_empty()
    {
        return false;
    }
    let total = p.unit_frames * p.iterations;
    if !p
        .instances
        .iter()
        .zip(&document.instances)
        .all(|(effective, authored)| {
            effective.instance_id == authored.instance_id
                && effective.start_frame < effective.end_frame
                && effective.end_frame <= total
                && effective.start_frame % p.unit_frames == 0
                && effective.end_frame % p.unit_frames == 0
        })
    {
        return false;
    }
    let mut previous = None;
    p.spans.iter().all(|span| {
        let event = document.instances.get(span.ordinal as usize);
        let valid = span.start < span.end
            && span.end <= p.iterations
            && span.occurrence_start == span.start
            && event.is_some_and(|event| {
                event.loop_id == span.target && event.mode.unwrap_or(LoopMode::Playing) == span.mode
            })
            && previous.is_none_or(|(target, end, mode)| {
                target < span.target
                    || target == span.target
                        && end <= span.start
                        && !(end == span.start && mode == span.mode)
            });
        previous = Some((span.target, span.end, span.mode));
        valid
    })
}

impl View {
    pub fn valid(&self) -> bool {
        self.loop_id.is_valid()
            && self.position_frames <= self.length_frames
            && (!self.waiting_repeat
                || self.mode == LoopMode::Playing
                    && self.position_frames == self.length_frames
                    && self.length_frames > 0)
            && self.working.as_ref().is_none_or(Content::valid)
            && (self.active.is_some() || self.pending.is_some())
            && self
                .active
                .iter()
                .chain(self.pending.iter())
                .all(|version| {
                    version.valid()
                        && version
                            .document
                            .instances
                            .iter()
                            .all(|event| event.loop_id != self.loop_id)
                })
            && self.pending.as_ref().is_none_or(|pending| {
                self.active
                    .as_ref()
                    .is_none_or(|active| pending.version > active.version)
            })
            && self.pending_stage.is_some() == self.pending.is_some()
            && self
                .error
                .as_ref()
                .is_none_or(|message| message.len() <= 512)
    }
}
