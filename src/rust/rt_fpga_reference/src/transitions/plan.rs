//! Control-side timing compilation. The execution side consumes fixed components.
use super::{LoopId, LoopState, Mode, TransitionError as Error};
use alloc::{collections::BTreeSet, vec, vec::Vec};
use core::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum LoopKind {
    Primitive,
    Composite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LoopSpec {
    pub id: LoopId,
    pub kind: LoopKind,
    pub length: u64,
    pub sync_source: Option<LoopId>,
    pub repeat_sync: bool,
    pub timeline: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanDescription {
    pub quantum: u32,
    pub loops: Vec<LoopSpec>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TransitionLimits {
    pub loop_slots: u32,
    pub events: u32,
}
impl Default for TransitionLimits {
    fn default() -> Self {
        Self {
            loop_slots: 64,
            events: 256,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TransitionPlanImage {
    pub description: PlanDescription,
    pub limits: TransitionLimits,
    pub components: Vec<Vec<LoopId>>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TransitionPlan {
    pub(crate) image: TransitionPlanImage,
    pub(crate) specs: Vec<Option<LoopSpec>>,
    pub(crate) followers: Vec<LoopId>,
    pub(crate) follower_ranges: Vec<Range<usize>>,
}
impl TransitionPlan {
    pub fn compile(
        mut description: PlanDescription,
        limits: TransitionLimits,
    ) -> Result<Self, Error> {
        if !matches!(description.quantum, 1 | 16 | 32 | 64 | 128)
            || limits.loop_slots == 0
            || limits.loop_slots > 65536
            || limits.events > 262144
            || u64::from(limits.events) < 4 * description.loops.len() as u64
        {
            return Err(Error::InvalidConfig);
        }
        if description.loops.len() > limits.loop_slots as usize {
            return Err(Error::Capacity);
        }
        let mut specs = vec![None; limits.loop_slots as usize];
        description.loops.sort_unstable_by_key(|spec| spec.id);
        for spec in &description.loops {
            if spec.kind != LoopKind::Primitive || spec.timeline.is_some() {
                return Err(Error::Unsupported);
            }
            if spec.id.generation == 0 || spec.id.slot >= limits.loop_slots {
                return Err(Error::InvalidId);
            }
            if spec.length % u64::from(description.quantum) != 0 {
                return Err(Error::InvalidFrame);
            }
            if specs[spec.id.slot as usize].replace(*spec).is_some() {
                return Err(Error::DuplicateId);
            }
        }
        for spec in &description.loops {
            if let Some(source) = spec.sync_source {
                if specs
                    .get(source.slot as usize)
                    .copied()
                    .flatten()
                    .map(|s| s.id)
                    != Some(source)
                {
                    return Err(Error::InvalidId);
                }
            }
        }
        let count = description.loops.len();
        let mut forward = vec![Vec::new(); count];
        for (target, spec) in description.loops.iter().enumerate() {
            if let Some(source) = spec.sync_source {
                let source = description
                    .loops
                    .binary_search_by_key(&source, |s| s.id)
                    .map_err(|_| Error::InvalidId)?;
                forward[source].push(target);
            }
        }
        let mut indegree = vec![0; count];
        for (source, targets) in forward.iter().enumerate() {
            for &target in targets {
                if source != target {
                    indegree[target] += 1;
                }
            }
        }
        let mut ready = BTreeSet::new();
        for (node, &degree) in indegree.iter().enumerate() {
            if degree == 0 {
                ready.insert(node);
            }
        }
        let mut components = Vec::with_capacity(count);
        while let Some(node) = ready.pop_first() {
            components.push(vec![description.loops[node].id]);
            for &target in &forward[node] {
                if target != node {
                    indegree[target] -= 1;
                    if indegree[target] == 0 {
                        ready.insert(target);
                    }
                }
            }
        }
        if components.len() != count {
            return Err(Error::InstantaneousDependencyCycle);
        }
        let mut followers = Vec::with_capacity(count);
        let mut follower_ranges = vec![0..0; limits.loop_slots as usize];
        for (source, targets) in forward.iter().enumerate() {
            let start = followers.len();
            followers.extend(targets.iter().map(|&target| description.loops[target].id));
            follower_ranges[description.loops[source].id.slot as usize] = start..followers.len();
        }
        Ok(Self {
            image: TransitionPlanImage {
                description,
                limits,
                components,
            },
            specs,
            followers,
            follower_ranges,
        })
    }
    pub fn image(&self) -> &TransitionPlanImage {
        &self.image
    }
    pub fn sync_instructions(&self) -> Vec<crate::state::SyncInstruction> {
        self.image
            .components
            .iter()
            .flatten()
            .copied()
            .map(|loop_id| crate::state::SyncInstruction::ProcessLoop {
                loop_id,
                sync_source: self.specs[loop_id.slot as usize].unwrap().sync_source,
            })
            .collect()
    }
    pub fn followers(&self, id: LoopId) -> Result<&[LoopId], Error> {
        if self
            .specs
            .get(id.slot as usize)
            .copied()
            .flatten()
            .map(|s| s.id)
            != Some(id)
        {
            return Err(Error::InvalidId);
        }
        Ok(&self.followers[self.follower_ranges[id.slot as usize].clone()])
    }
    pub(crate) fn initial_states(&self) -> Vec<Option<LoopState>> {
        self.specs
            .iter()
            .map(|spec| {
                spec.map(|s| LoopState {
                    id: s.id,
                    mode: Mode::Stopped,
                    content_length: s.length,
                    position: 0,
                    capture_frames: 0,
                    cycles: 0,
                    content_revision: 0,
                    capture_revision: 0,
                    revision_watermark: 0,
                    last_pulse: None,
                    repeat_sync: s.repeat_sync,
                })
            })
            .collect()
    }
}
