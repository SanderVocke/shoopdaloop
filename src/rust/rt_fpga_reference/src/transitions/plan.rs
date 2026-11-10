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
    pub pending_requests: u32,
    pub events: u32,
}
impl Default for TransitionLimits {
    fn default() -> Self {
        Self {
            loop_slots: 64,
            pending_requests: 64,
            events: 192,
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

pub struct TransitionPlan {
    pub(crate) image: TransitionPlanImage,
    pub(crate) specs: Vec<Option<LoopSpec>>,
    pub(crate) followers: Vec<LoopId>,
    pub(crate) follower_ranges: Vec<Range<usize>>,
    pub(crate) component_of: Vec<usize>,
}
impl TransitionPlan {
    pub fn compile(
        mut description: PlanDescription,
        limits: TransitionLimits,
    ) -> Result<Self, Error> {
        if !matches!(description.quantum, 1 | 16 | 32 | 64 | 128)
            || limits.loop_slots == 0
            || limits.loop_slots > 65536
            || limits.pending_requests == 0
            || limits.pending_requests > 65536
            || limits.events > 196608
            || limits.events
                < limits
                    .loop_slots
                    .checked_mul(2)
                    .and_then(|v| v.checked_add(limits.pending_requests))
                    .ok_or(Error::Capacity)?
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
        let mut reverse = vec![Vec::new(); count];
        for (target, spec) in description.loops.iter().enumerate() {
            if let Some(source) = spec.sync_source {
                let source = description
                    .loops
                    .binary_search_by_key(&source, |s| s.id)
                    .map_err(|_| Error::InvalidId)?;
                forward[source].push(target);
                reverse[target].push(source);
            }
        }
        let mut seen = vec![false; count];
        let mut finish = Vec::with_capacity(count);
        let mut stack = Vec::with_capacity(count);
        for start in 0..count {
            if seen[start] {
                continue;
            }
            seen[start] = true;
            stack.push((start, 0usize));
            while let Some((node, edge)) = stack.last_mut() {
                if let Some(&target) = forward[*node].get(*edge) {
                    *edge += 1;
                    if !seen[target] {
                        seen[target] = true;
                        stack.push((target, 0));
                    }
                } else {
                    finish.push(*node);
                    stack.pop();
                }
            }
        }
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut membership = vec![usize::MAX; count];
        let mut pending = Vec::with_capacity(count);
        for start in finish.into_iter().rev() {
            if membership[start] != usize::MAX {
                continue;
            }
            let group = groups.len();
            let mut members = Vec::new();
            pending.push(start);
            membership[start] = group;
            while let Some(node) = pending.pop() {
                members.push(node);
                for &target in &reverse[node] {
                    if membership[target] == usize::MAX {
                        membership[target] = group;
                        pending.push(target);
                    }
                }
            }
            members.sort_unstable();
            groups.push(members);
        }
        let mut edges = BTreeSet::new();
        let mut indegree = vec![0; groups.len()];
        for (source, targets) in forward.iter().enumerate() {
            for &target in targets {
                let edge = (membership[source], membership[target]);
                if edge.0 != edge.1 && edges.insert(edge) {
                    indegree[edge.1] += 1;
                }
            }
        }
        let mut ready = BTreeSet::new();
        for (group, members) in groups.iter().enumerate() {
            if indegree[group] == 0 {
                ready.insert((description.loops[members[0]].id, group));
            }
        }
        let mut components: Vec<Vec<LoopId>> = Vec::with_capacity(groups.len());
        while let Some((_, group)) = ready.pop_first() {
            components.push(
                groups[group]
                    .iter()
                    .map(|&i| description.loops[i].id)
                    .collect(),
            );
            for &(_, target) in edges.range((group, 0)..=(group, usize::MAX)) {
                indegree[target] -= 1;
                if indegree[target] == 0 {
                    ready.insert((description.loops[groups[target][0]].id, target));
                }
            }
        }
        let mut component_of = vec![usize::MAX; limits.loop_slots as usize];
        for (component, members) in components.iter().enumerate() {
            for id in members {
                component_of[id.slot as usize] = component;
            }
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
            component_of,
        })
    }
    pub fn image(&self) -> &TransitionPlanImage {
        &self.image
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
