//! Prepared timing dependencies are independent of audio/MIDI signal routes.

use crate::program::{
    compile_program, expanded_entries, CompiledProgram, ProgramRecord, Segment, TableRange,
    TargetRecord,
};
use crate::{Error, Limits, LoopId, Mode, ProgramDescription};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::{vec, vec::Vec};

pub mod staging;

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
    nodes: Vec<LoopSpec>,
    triggers: Vec<Trigger>,
    programs: Vec<ProgramRecord>,
    targets: Vec<TargetRecord>,
    segments: Vec<Segment>,
    order: Vec<LoopId>,
    relation_count: usize,
}

impl PreparedTopology {
    pub fn validate_quantum(&self, quantum: rt_runtime::Quantum) -> Result<(), Error> {
        let q = quantum.frames() as u64;
        if self.nodes.iter().any(|n| n.length % q != 0)
            || self.programs.iter().any(|p| p.nominal_unit_frames % q != 0)
        {
            return Err(Error::InvalidFrame);
        }
        Ok(())
    }
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
    pub fn program(&self, id: LoopId) -> Option<CompiledProgram<'_>> {
        self.programs
            .binary_search_by_key(&id, |program| program.source)
            .ok()
            .map(|index| {
                CompiledProgram::from_record(&self.programs[index], &self.targets, &self.segments)
            })
    }
    /// Loop IDs in canonical source-before-dependent order. Self-sync adds no ordering edge.
    pub fn dependency_order(&self) -> &[LoopId] {
        &self.order
    }
    pub fn relation_count(&self) -> usize {
        self.relation_count
    }
    /// Header plus owned flat table capacity, excluding compiler scratch and
    /// allocator overhead. Receiver storage retains capacity for later reuse.
    pub fn storage_bytes(&self) -> usize {
        storage_bytes(
            self.nodes.capacity(),
            self.triggers.capacity(),
            self.programs.capacity(),
            self.targets.capacity(),
            self.segments.capacity(),
            self.order.capacity(),
        )
        .expect("validated topology storage")
    }
}

pub(crate) fn storage_bytes(
    nodes: usize,
    triggers: usize,
    programs: usize,
    targets: usize,
    segments: usize,
    order: usize,
) -> Result<usize, Error> {
    let mut bytes = size_of::<PreparedTopology>();
    for (count, element) in [
        (nodes, size_of::<LoopSpec>()),
        (triggers, size_of::<Trigger>()),
        (programs, size_of::<ProgramRecord>()),
        (targets, size_of::<TargetRecord>()),
        (segments, size_of::<Segment>()),
        (order, size_of::<LoopId>()),
    ] {
        bytes = bytes
            .checked_add(crate::config::checked_storage(count, element)?)
            .ok_or(Error::Capacity)?;
    }
    crate::config::checked_storage(bytes, 1)
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
            if nodes[target].kind == LoopKind::Composite
                && entry
                    .mode
                    .is_some_and(|mode| !matches!(mode, Mode::Playing | Mode::Stopped))
            {
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
        nodes[source_index].length = program.nominal_frames().map_err(|_| Error::Exhausted)?;
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
        if nodes[target].kind == LoopKind::Composite
            && !matches!(trigger.mode, Mode::Playing | Mode::Stopped)
        {
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
    let mut indegree = vec![0; nodes.len()];
    for &(source, target, action) in &relations {
        if source == target {
            if action {
                return Err(Error::InstantaneousDependencyCycle);
            }
        } else {
            forward[source].push(target);
            indegree[target] += 1;
        }
    }
    let mut ready = BTreeSet::new();
    for (node, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            ready.insert(node);
        }
    }
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(node) = ready.pop_first() {
        order.push(nodes[node].id);
        for &target in &forward[node] {
            indegree[target] -= 1;
            if indegree[target] == 0 {
                ready.insert(target);
            }
        }
    }
    if order.len() != nodes.len() {
        return Err(Error::InstantaneousDependencyCycle);
    }
    let mut programs = Vec::with_capacity(compiled.len());
    let mut targets = Vec::new();
    let mut segments = Vec::new();
    for program in compiled.into_values() {
        let target_start = targets.len();
        let segment_start = segments.len();
        for target in program.targets {
            // Relative ranges make a program's semantic equality independent of
            // unrelated programs inserted earlier in the global tables.
            targets.push(TargetRecord {
                target: target.target,
                segments: TableRange {
                    start: segments.len() - segment_start,
                    len: target.segments.len(),
                },
            });
            segments.extend_from_slice(&target.segments);
        }
        programs.push(ProgramRecord {
            source: program.source,
            kind: program.kind,
            nominal_unit_frames: program.nominal_unit_frames,
            iterations: program.iterations,
            targets: TableRange {
                start: target_start,
                len: targets.len() - target_start,
            },
            segments: TableRange {
                start: segment_start,
                len: segments.len() - segment_start,
            },
        });
    }
    Ok(PreparedTopology {
        limits,
        nodes: nodes.into_boxed_slice().into_vec(),
        triggers: triggers.into_boxed_slice().into_vec(),
        programs: programs.into_boxed_slice().into_vec(),
        targets: targets.into_boxed_slice().into_vec(),
        segments: segments.into_boxed_slice().into_vec(),
        order: order.into_boxed_slice().into_vec(),
        relation_count: relations.len(),
    })
}
