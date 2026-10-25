//! Prepared timing dependencies are independent of audio/MIDI signal routes.

use crate::program::{compile_program, expanded_entries, CompiledProgram};
use crate::{Error, Limits, LoopId, Mode, ProgramDescription};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoopKind {
    Primitive,
    Composite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoopSpec {
    pub id: LoopId,
    pub kind: LoopKind,
    /// Primitive content length; compiled nominal duration for a composite.
    pub length: u64,
    pub sync_source: Option<LoopId>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Trigger {
    pub source: LoopId,
    pub target: LoopId,
    pub mode: Mode,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TopologyDescription {
    pub loops: Vec<LoopSpec>,
    pub triggers: Vec<Trigger>,
    pub programs: Vec<ProgramDescription>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct PreparedTopology {
    limits: Limits,
    nodes: Box<[LoopSpec]>,
    triggers: Box<[Trigger]>,
    programs: Box<[CompiledProgram]>,
    components: Box<[Box<[LoopId]>]>,
    relation_count: usize,
}

impl PreparedTopology {
    pub fn limits(&self) -> Limits {
        self.limits
    }
    pub fn nodes(&self) -> &[LoopSpec] {
        &self.nodes
    }
    pub fn node(&self, id: LoopId) -> Result<LoopSpec, Error> {
        self.nodes
            .binary_search_by_key(&id, |node| node.id)
            .ok()
            .map(|index| self.nodes[index])
            .ok_or(Error::InvalidId)
    }
    pub fn triggers(&self) -> &[Trigger] {
        &self.triggers
    }
    pub fn program(&self, id: LoopId) -> Option<&CompiledProgram> {
        self.programs
            .binary_search_by_key(&id, |program| program.source)
            .ok()
            .map(|index| &self.programs[index])
    }
    pub fn components(&self) -> &[Box<[LoopId]>] {
        &self.components
    }
    pub fn relation_count(&self) -> usize {
        self.relation_count
    }
}

fn index(nodes: &[LoopSpec], id: LoopId) -> Result<usize, Error> {
    nodes
        .binary_search_by_key(&id, |node| node.id)
        .map_err(|_| Error::InvalidId)
}

/// Allocating preparation only. Output is canonical under declaration reordering.
pub fn compile(
    limits: Limits,
    description: &TopologyDescription,
) -> Result<PreparedTopology, Error> {
    let limits = limits.validate()?;
    if description.loops.len() > limits.loop_slots
        || description.programs.len() > description.loops.len()
        || description.triggers.len() > limits.relations
    {
        return Err(Error::Capacity);
    }
    let mut nodes = description.loops.clone();
    nodes.sort_unstable_by_key(|node| node.id);
    for (position, node) in nodes.iter().enumerate() {
        if node.id.slot() as usize >= limits.loop_slots {
            return Err(Error::Capacity);
        }
        if position > 0 && nodes[position - 1].id.slot() == node.id.slot() {
            return Err(Error::DuplicateId);
        }
        if let Some(source) = node.sync_source {
            index(&nodes, source)?;
        }
    }
    let mut descriptions = BTreeMap::new();
    let mut expanded = 0usize;
    for program in &description.programs {
        let source = index(&nodes, program.source)?;
        if nodes[source].kind != LoopKind::Composite {
            return Err(Error::InvalidProgram);
        }
        if descriptions.insert(program.source, program).is_some() {
            return Err(Error::DuplicateId);
        }
        expanded = expanded
            .checked_add(expanded_entries(program, limits)?)
            .ok_or(Error::Capacity)?;
        if expanded > limits.program_entries {
            return Err(Error::Capacity);
        }
        for entry in program.sections.iter().flat_map(|section| &section.entries) {
            let target = index(&nodes, entry.target)?;
            if nodes[target].kind == LoopKind::Composite && entry.mode == Some(Mode::Recording) {
                return Err(Error::InvalidProgram);
            }
        }
    }
    for node in &nodes {
        if node.kind == LoopKind::Composite && !descriptions.contains_key(&node.id) {
            return Err(Error::InvalidProgram);
        }
    }

    // Compile child programs first so implicit durations have complete metadata.
    let mut compiled = BTreeMap::new();
    let mut segment_count = 0usize;
    while compiled.len() < descriptions.len() {
        let candidate = descriptions.iter().find(|(source, program)| {
            !compiled.contains_key(*source)
                && program
                    .sections
                    .iter()
                    .flat_map(|section| &section.entries)
                    .all(|entry| {
                        nodes[index(&nodes, entry.target).expect("validated target")].kind
                            == LoopKind::Primitive
                            || compiled.contains_key(&entry.target)
                    })
        });
        let Some((&source, description)) = candidate else {
            return Err(Error::RecursiveProgram);
        };
        let program = compile_program(description, limits, |target| {
            Ok(nodes[index(&nodes, target)?].length)
        })?;
        segment_count = segment_count
            .checked_add(program.segment_count())
            .ok_or(Error::Capacity)?;
        if segment_count > limits.program_segments {
            return Err(Error::Capacity);
        }
        let source_index = index(&nodes, source)?;
        nodes[source_index].length = program.nominal_frames()?;
        compiled.insert(source, program);
    }

    let mut triggers = description.triggers.clone();
    triggers.sort_unstable();
    if triggers.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(Error::DuplicateId);
    }
    // false = synchronization; true = state-changing action dependency.
    let mut relations = BTreeSet::new();
    for (target, node) in nodes.iter().enumerate() {
        if let Some(source) = node.sync_source {
            relations.insert((index(&nodes, source)?, target, false));
        }
    }
    for trigger in &triggers {
        let source = index(&nodes, trigger.source)?;
        let target = index(&nodes, trigger.target)?;
        if nodes[target].kind == LoopKind::Composite && trigger.mode == Mode::Recording {
            return Err(Error::InvalidProgram);
        }
        relations.insert((source, target, true));
    }
    for program in compiled.values() {
        for target in program.targets() {
            relations.insert((
                index(&nodes, program.source)?,
                index(&nodes, target.target)?,
                true,
            ));
        }
    }
    if relations.len() > limits.relations {
        return Err(Error::Capacity);
    }
    let mut forward = vec![Vec::new(); nodes.len()];
    let mut reverse = vec![Vec::new(); nodes.len()];
    for &(source, target, _) in &relations {
        forward[source].push(target);
        reverse[target].push(source);
    }
    let (groups, membership) = strongly_connected(&forward, &reverse);
    if relations
        .iter()
        .any(|&(source, target, action)| action && membership[source] == membership[target])
    {
        return Err(Error::InstantaneousDependencyCycle);
    }
    let mut group_edges = BTreeSet::new();
    let mut indegree = vec![0; groups.len()];
    for &(source, target, _) in &relations {
        let edge = (membership[source], membership[target]);
        if edge.0 != edge.1 && group_edges.insert(edge) {
            indegree[edge.1] += 1;
        }
    }
    let mut ready = BTreeSet::new();
    for (group, members) in groups.iter().enumerate() {
        if indegree[group] == 0 {
            ready.insert((nodes[members[0]].id, group));
        }
    }
    let mut components = Vec::with_capacity(groups.len());
    while let Some((_, group)) = ready.pop_first() {
        components.push(
            groups[group]
                .iter()
                .map(|&node| nodes[node].id)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        for &(_, target) in group_edges.range((group, 0)..=(group, usize::MAX)) {
            indegree[target] -= 1;
            if indegree[target] == 0 {
                ready.insert((nodes[groups[target][0]].id, target));
            }
        }
    }
    if components.len() != groups.len() {
        return Err(Error::InstantaneousDependencyCycle);
    }
    Ok(PreparedTopology {
        limits,
        nodes: nodes.into_boxed_slice(),
        triggers: triggers.into_boxed_slice(),
        programs: compiled
            .into_values()
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        components: components.into_boxed_slice(),
        relation_count: relations.len(),
    })
}

// Iterative passes avoid consuming the native/Wasm call stack on deep chains.
fn strongly_connected(
    forward: &[Vec<usize>],
    reverse: &[Vec<usize>],
) -> (Vec<Vec<usize>>, Vec<usize>) {
    let mut seen = vec![false; forward.len()];
    let mut finish = Vec::with_capacity(forward.len());
    let mut stack = Vec::with_capacity(forward.len());
    for start in 0..forward.len() {
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
    let mut membership = vec![usize::MAX; forward.len()];
    let mut groups = Vec::new();
    let mut pending = Vec::with_capacity(forward.len());
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
    (groups, membership)
}
