use super::*;
use crate::program::ProgramKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Relation {
    source: usize,
    target: usize,
    action: bool,
}
#[derive(Clone, Copy)]
enum Phase {
    Nodes(usize),
    Links(usize),
    Programs(usize),
    Target {
        program: usize,
        target: usize,
    },
    Segment {
        program: usize,
        target: usize,
        segment: usize,
    },
    ProgramDone(usize),
    Components(usize),
    Members {
        group: usize,
        member: usize,
    },
    RecursionSeed(usize),
    RecursionPop,
    RecursionEdges {
        program: usize,
        target: usize,
    },
    Edges {
        section: u8,
        index: usize,
    },
    Search {
        edge: Relation,
        cursor: usize,
        section: u8,
        next: usize,
    },
    CheckEdges(usize),
    Strong {
        group: usize,
        visited: usize,
        current: usize,
    },
    Choose {
        group: usize,
        scan: usize,
    },
    RetireEdges {
        group: usize,
        edge: usize,
    },
    Done,
}
pub(super) struct Validator {
    phase: Phase,
    slots: Box<[usize]>,
    programs: Box<[usize]>,
    owners: Box<[usize]>,
    groups: Box<[usize]>,
    marks: Box<[usize]>,
    degrees: Box<[usize]>,
    queue: Box<[usize]>,
    ordinal_program: Box<[usize]>,
    ordinal_owner: Box<[usize]>,
    ordinal_mode: Box<[Mode]>,
    edges: Vec<Relation>,
    head: usize,
    tail: usize,
    processed: usize,
    composites: usize,
    target_end: usize,
    segment_end: usize,
    member_end: usize,
    covered: usize,
    max_end: u64,
    max_ordinal: usize,
    entry_minimum: usize,
    previous: Option<Segment>,
}
impl Validator {
    pub fn heap_bytes(limits: Limits) -> Result<usize, Error> {
        let mut bytes = 0usize;
        for (count, element) in [
            (limits.loop_slots, 6 * size_of::<usize>()),
            (
                limits.program_entries,
                3 * size_of::<usize>() + size_of::<Mode>(),
            ),
            (limits.relations, size_of::<Relation>()),
        ] {
            bytes = bytes
                .checked_add(crate::config::checked_storage(count, element)?)
                .ok_or(Error::Capacity)?;
        }
        crate::config::checked_storage(bytes, 1)
    }
    pub fn prepare(limits: Limits) -> Self {
        let nodes = || initialized(limits.loop_slots, usize::MAX).into_boxed_slice();
        Self {
            phase: Phase::Nodes(0),
            slots: nodes(),
            programs: nodes(),
            owners: initialized(limits.program_entries, usize::MAX).into_boxed_slice(),
            groups: nodes(),
            marks: nodes(),
            degrees: nodes(),
            queue: nodes(),
            ordinal_program: initialized(limits.program_entries, usize::MAX).into_boxed_slice(),
            ordinal_owner: initialized(limits.program_entries, 0).into_boxed_slice(),
            ordinal_mode: initialized(limits.program_entries, Mode::Stopped).into_boxed_slice(),
            edges: pretouched(
                limits.relations,
                Relation {
                    source: 0,
                    target: 0,
                    action: false,
                },
            ),
            head: 0,
            tail: 0,
            processed: 0,
            composites: 0,
            target_end: 0,
            segment_end: 0,
            member_end: 0,
            covered: 0,
            max_end: 0,
            max_ordinal: 0,
            entry_minimum: 0,
            previous: None,
        }
    }
    pub fn reset(&mut self) {
        self.phase = Phase::Nodes(0);
        self.slots.fill(usize::MAX);
        self.programs.fill(usize::MAX);
        self.groups.fill(usize::MAX);
        self.marks.fill(usize::MAX);
        self.degrees.fill(0);
        self.ordinal_program.fill(usize::MAX);
        self.edges.clear();
        self.head = 0;
        self.tail = 0;
        self.processed = 0;
        self.composites = 0;
        self.target_end = 0;
        self.segment_end = 0;
        self.member_end = 0;
        self.covered = 0;
        self.max_end = 0;
        self.max_ordinal = 0;
        self.entry_minimum = 0;
        self.previous = None;
    }
    fn node(&self, topology: &PreparedTopology, id: LoopId) -> Result<usize, Error> {
        let index = *self.slots.get(id.slot() as usize).ok_or(Error::InvalidId)?;
        if topology.nodes.get(index).is_none_or(|node| node.id != id) {
            Err(Error::InvalidId)
        } else {
            Ok(index)
        }
    }
    fn push(&mut self, node: usize) {
        self.queue[self.tail] = node;
        self.tail += 1;
    }
    fn end(range: TableRange, limit: usize) -> Result<usize, Error> {
        range
            .start
            .checked_add(range.len)
            .filter(|end| *end <= limit)
            .ok_or(Error::InvalidProgram)
    }
    pub fn step(&mut self, t: &PreparedTopology, relations: usize) -> Result<bool, Error> {
        self.phase = match self.phase {
            Phase::Nodes(index) => {
                if index == t.nodes.len() {
                    Phase::Links(0)
                } else {
                    let node = t.nodes[index];
                    let slot = node.id.slot() as usize;
                    if slot >= self.slots.len() {
                        return Err(Error::Capacity);
                    }
                    if index > 0 && t.nodes[index - 1].id.slot() >= node.id.slot() {
                        return Err(Error::DuplicateId);
                    }
                    self.slots[slot] = index;
                    self.composites += usize::from(node.kind == LoopKind::Composite);
                    Phase::Nodes(index + 1)
                }
            }
            Phase::Links(index) => {
                if index == t.nodes.len() {
                    Phase::Programs(0)
                } else {
                    if let Some(source) = t.nodes[index].sync_source {
                        self.node(t, source)?;
                    }
                    Phase::Links(index + 1)
                }
            }
            Phase::Programs(index) => {
                if index == t.programs.len() {
                    if self.composites != t.programs.len()
                        || self.target_end != t.targets.len()
                        || self.segment_end != t.segments.len()
                    {
                        return Err(Error::InvalidProgram);
                    }
                    Phase::Target {
                        program: 0,
                        target: 0,
                    }
                } else {
                    let program = t.programs[index];
                    let node = self.node(t, program.source)?;
                    if t.nodes[node].kind != LoopKind::Composite
                        || index > 0 && t.programs[index - 1].source >= program.source
                        || program.nominal_unit_frames == 0
                    {
                        return Err(Error::InvalidProgram);
                    }
                    let frames = program
                        .iterations
                        .checked_mul(program.nominal_unit_frames)
                        .ok_or(Error::Exhausted)?;
                    if frames != t.nodes[node].length
                        || program.targets.start != self.target_end
                        || program.segments.start != self.segment_end
                    {
                        return Err(Error::InvalidProgram);
                    }
                    self.target_end = Self::end(program.targets, t.targets.len())?;
                    self.segment_end = Self::end(program.segments, t.segments.len())?;
                    if program.targets.len == 0
                        && (program.iterations != 0
                            || program.segments.len != 0
                            || program.kind != ProgramKind::Regular)
                    {
                        return Err(Error::InvalidProgram);
                    }
                    self.programs[node] = index;
                    Phase::Programs(index + 1)
                }
            }
            Phase::Target { program, target } => {
                if program == t.programs.len() {
                    Phase::Components(0)
                } else {
                    let p = t.programs[program];
                    if target == 0 {
                        self.covered = 0;
                        self.max_end = 0;
                        self.max_ordinal = 0;
                    }
                    if target == p.targets.len {
                        Phase::ProgramDone(program)
                    } else {
                        let at = p.targets.start + target;
                        let row = t.targets[at];
                        let node = self.node(t, row.target)?;
                        if target > 0 && t.targets[at - 1].target >= row.target
                            || row.segments.start != self.covered
                            || row.segments.len == 0
                        {
                            return Err(Error::InvalidProgram);
                        }
                        self.covered = Self::end(row.segments, p.segments.len)?;
                        self.owners[at] = self.node(t, p.source)?;
                        if t.nodes[node].kind == LoopKind::Composite {
                            self.degrees[node] += 1;
                        }
                        self.previous = None;
                        Phase::Segment {
                            program,
                            target,
                            segment: 0,
                        }
                    }
                }
            }
            Phase::Segment {
                program,
                target,
                segment,
            } => {
                let p = t.programs[program];
                let at = p.targets.start + target;
                let row = t.targets[at];
                if segment == row.segments.len {
                    Phase::Target {
                        program,
                        target: target + 1,
                    }
                } else {
                    let value = t.segments[p.segments.start + row.segments.start + segment];
                    let ordinal = value.ordinal as usize;
                    if value.start >= value.end
                        || value.end > p.iterations
                        || value.occurrence_start != value.start
                        || ordinal >= t.limits.program_entries
                    {
                        return Err(Error::InvalidProgram);
                    }
                    if self.previous.is_some_and(|old| {
                        old.end > value.start || old.end == value.start && old.mode == value.mode
                    }) {
                        return Err(Error::InvalidProgram);
                    }
                    if p.kind == ProgramKind::Regular && value.mode != Mode::Playing
                        || value.mode == Mode::Recording
                            && t.nodes[self.node(t, row.target)?].kind == LoopKind::Composite
                    {
                        return Err(Error::InvalidProgram);
                    }
                    if self.ordinal_program[ordinal] == program {
                        if self.ordinal_owner[ordinal] != at
                            || self.ordinal_mode[ordinal] != value.mode
                        {
                            return Err(Error::InvalidProgram);
                        }
                    } else {
                        self.ordinal_program[ordinal] = program;
                        self.ordinal_owner[ordinal] = at;
                        self.ordinal_mode[ordinal] = value.mode;
                    }
                    self.previous = Some(value);
                    self.max_end = self.max_end.max(value.end);
                    self.max_ordinal = self.max_ordinal.max(ordinal);
                    Phase::Segment {
                        program,
                        target,
                        segment: segment + 1,
                    }
                }
            }
            Phase::ProgramDone(program) => {
                let p = t.programs[program];
                if self.covered != p.segments.len || self.max_end != p.iterations {
                    return Err(Error::InvalidProgram);
                }
                self.entry_minimum = self
                    .entry_minimum
                    .checked_add(if p.targets.len == 0 {
                        0
                    } else {
                        self.max_ordinal + 1
                    })
                    .ok_or(Error::Capacity)?;
                if self.entry_minimum > t.limits.program_entries {
                    return Err(Error::Capacity);
                }
                Phase::Target {
                    program: program + 1,
                    target: 0,
                }
            }
            Phase::Components(group) => {
                if group == t.components.len() {
                    if self.member_end != t.nodes.len() {
                        return Err(Error::InvalidProgram);
                    }
                    Phase::RecursionSeed(0)
                } else {
                    let range = t.components[group];
                    if range.start != self.member_end || range.len == 0 {
                        return Err(Error::InvalidProgram);
                    }
                    self.member_end = Self::end(range, t.members.len())?;
                    Phase::Members { group, member: 0 }
                }
            }
            Phase::Members { group, member } => {
                let range = t.components[group];
                if member == range.len {
                    Phase::Components(group + 1)
                } else {
                    let at = range.start + member;
                    let id = t.members[at];
                    let node = self.node(t, id)?;
                    if self.groups[node] != usize::MAX || member > 0 && t.members[at - 1] >= id {
                        return Err(Error::InvalidProgram);
                    }
                    self.groups[node] = group;
                    Phase::Members {
                        group,
                        member: member + 1,
                    }
                }
            }
            Phase::RecursionSeed(index) => {
                if index == t.nodes.len() {
                    Phase::RecursionPop
                } else {
                    if t.nodes[index].kind == LoopKind::Composite && self.degrees[index] == 0 {
                        self.push(index);
                    }
                    Phase::RecursionSeed(index + 1)
                }
            }
            Phase::RecursionPop => {
                if self.head == self.tail {
                    if self.processed != t.programs.len() {
                        return Err(Error::RecursiveProgram);
                    }
                    Phase::Edges {
                        section: 0,
                        index: 0,
                    }
                } else {
                    let node = self.queue[self.head];
                    self.head += 1;
                    self.processed += 1;
                    Phase::RecursionEdges {
                        program: self.programs[node],
                        target: 0,
                    }
                }
            }
            Phase::RecursionEdges { program, target } => {
                let p = t.programs[program];
                if target == p.targets.len {
                    Phase::RecursionPop
                } else {
                    let node = self.node(t, t.targets[p.targets.start + target].target)?;
                    if t.nodes[node].kind == LoopKind::Composite {
                        self.degrees[node] -= 1;
                        if self.degrees[node] == 0 {
                            self.push(node);
                        }
                    }
                    Phase::RecursionEdges {
                        program,
                        target: target + 1,
                    }
                }
            }
            Phase::Edges { section, index } => {
                let count = match section {
                    0 => t.nodes.len(),
                    1 => t.triggers.len(),
                    _ => t.targets.len(),
                };
                if index == count {
                    if section == 2 {
                        if self.edges.len() != relations {
                            return Err(Error::InvalidProgram);
                        }
                        Phase::CheckEdges(0)
                    } else {
                        Phase::Edges {
                            section: section + 1,
                            index: 0,
                        }
                    }
                } else {
                    let edge = match section {
                        0 => t.nodes[index]
                            .sync_source
                            .map(|source| {
                                self.node(t, source).map(|source| Relation {
                                    source,
                                    target: index,
                                    action: false,
                                })
                            })
                            .transpose()?,
                        1 => {
                            let row = t.triggers[index];
                            if index > 0 && t.triggers[index - 1] >= row {
                                return Err(Error::DuplicateId);
                            }
                            Some(Relation {
                                source: self.node(t, row.source)?,
                                target: self.node(t, row.target)?,
                                action: true,
                            })
                        }
                        _ => Some(Relation {
                            source: self.owners[index],
                            target: self.node(t, t.targets[index].target)?,
                            action: true,
                        }),
                    };
                    if let Some(edge) = edge {
                        Phase::Search {
                            edge,
                            cursor: 0,
                            section,
                            next: index + 1,
                        }
                    } else {
                        Phase::Edges {
                            section,
                            index: index + 1,
                        }
                    }
                }
            }
            Phase::Search {
                edge,
                cursor,
                section,
                next,
            } => {
                if cursor == self.edges.len() {
                    if self.edges.len() == self.edges.capacity() {
                        return Err(Error::Capacity);
                    }
                    self.edges.push(edge);
                    Phase::Edges {
                        section,
                        index: next,
                    }
                } else if self.edges[cursor] == edge {
                    Phase::Edges {
                        section,
                        index: next,
                    }
                } else {
                    Phase::Search {
                        edge,
                        cursor: cursor + 1,
                        section,
                        next,
                    }
                }
            }
            Phase::CheckEdges(index) => {
                if index == self.edges.len() {
                    Phase::Strong {
                        group: 0,
                        visited: 0,
                        current: 0,
                    }
                } else {
                    let edge = self.edges[index];
                    let source = self.groups[edge.source];
                    let target = self.groups[edge.target];
                    if source == target && edge.action {
                        return Err(Error::InstantaneousDependencyCycle);
                    }
                    if source > target {
                        return Err(Error::InvalidProgram);
                    }
                    if source < target {
                        self.degrees[target] += 1;
                    }
                    Phase::CheckEdges(index + 1)
                }
            }
            Phase::Strong {
                group,
                visited,
                current,
            } => {
                if group == t.components.len() {
                    Phase::Choose { group: 0, scan: 0 }
                } else {
                    let range = t.components[group];
                    if range.len == 1 {
                        Phase::Strong {
                            group: group + 1,
                            visited: 0,
                            current: 0,
                        }
                    } else {
                        let first = self.node(t, t.members[range.start])?;
                        let current = if visited == 0 { first } else { current };
                        if self.marks[current] == group {
                            return Err(Error::InvalidProgram);
                        }
                        self.marks[current] = group;
                        let source = self.node(
                            t,
                            t.nodes[current].sync_source.ok_or(Error::InvalidProgram)?,
                        )?;
                        if self.groups[source] != group {
                            return Err(Error::InvalidProgram);
                        }
                        if visited + 1 == range.len {
                            if source != first {
                                return Err(Error::InvalidProgram);
                            }
                            Phase::Strong {
                                group: group + 1,
                                visited: 0,
                                current: 0,
                            }
                        } else {
                            Phase::Strong {
                                group,
                                visited: visited + 1,
                                current: source,
                            }
                        }
                    }
                }
            }
            Phase::Choose { group, scan } => {
                if group == t.components.len() {
                    Phase::Done
                } else {
                    if self.degrees[group] != 0 {
                        return Err(Error::InvalidProgram);
                    }
                    if scan == t.components.len() {
                        Phase::RetireEdges { group, edge: 0 }
                    } else {
                        if self.degrees[scan] == 0
                            && t.members[t.components[scan].start]
                                < t.members[t.components[group].start]
                        {
                            return Err(Error::InvalidProgram);
                        }
                        Phase::Choose {
                            group,
                            scan: scan + 1,
                        }
                    }
                }
            }
            Phase::RetireEdges { group, edge } => {
                if edge == self.edges.len() {
                    Phase::Choose {
                        group: group + 1,
                        scan: group + 1,
                    }
                } else {
                    let row = self.edges[edge];
                    let source = self.groups[row.source];
                    let target = self.groups[row.target];
                    if source == group && source != target {
                        self.degrees[target] -= 1;
                    }
                    Phase::RetireEdges {
                        group,
                        edge: edge + 1,
                    }
                }
            }
            Phase::Done => Phase::Done,
        };
        Ok(matches!(self.phase, Phase::Done))
    }
}
