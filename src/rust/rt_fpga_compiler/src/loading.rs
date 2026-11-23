//! ARM-owned reservations and address binding for two executable regions.
use crate::*;
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LoadError {
    Compile(CompileError),
    Busy,
    StaleRevision,
    Exhausted,
    Geometry,
    IncompatibleState,
}
impl From<CompileError> for LoadError {
    fn from(value: CompileError) -> Self {
        Self::Compile(value)
    }
}
impl core::fmt::Display for LoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl core::error::Error for LoadError {}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct ValueSlot {
    key: StateKey,
    address: u32,
}
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LoopSlot {
    pub logical: Identity,
    pub physical: Identity,
    pub fresh: bool,
    pub sync_source: Option<Identity>,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CompositeCut {
    pub old_slot: Identity,
    pub require_empty: bool,
    pub allow_restart: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub restart_source: Option<Identity>,
    pub future_before: Option<u32>,
}

impl CompositeCut {
    pub fn allows_playing(self, position: u64, length: u64, restart_ready: bool) -> bool {
        (self.allow_restart && restart_ready && length != 0 && position == length)
            || (position < length
                && self
                    .future_before
                    .is_some_and(|frame| position < u64::from(frame)))
    }

    pub fn waits_for_restart(self, position: u64, length: u64, restart_ready: bool) -> bool {
        self.allow_restart
            && length != 0
            && position <= length
            && !self.allows_playing(position, length, restart_ready)
    }
}

impl CompositeSchedule {
    fn future_edit_before(&self, candidate: &Self) -> Option<u32> {
        let mut schedules = BTreeMap::<Identity, [&[FrameSpan]; 2]>::new();
        for (version, children) in [&self.children, &candidate.children]
            .into_iter()
            .enumerate()
        {
            for child in children {
                schedules.entry(child.id).or_default()[version] = &child.spans;
            }
        }
        let mut earliest = self.length;
        for [before, after] in schedules.into_values() {
            // Removing the last relationship changes how the child handles
            // its own pulse plan. A stopped handoff handles that case.
            // A new recorder can also change the storage credit calculation.
            if after.is_empty()
                || (before.is_empty() && after.iter().any(|span| span.mode == SpanMode::Record))
            {
                return None;
            }
            let mut old = before.iter();
            let mut new = after.iter();
            loop {
                let frame = match (old.next(), new.next()) {
                    (Some(a), Some(b)) if a == b => continue,
                    (Some(a), Some(b)) if a.start == b.start && a.mode == b.mode => {
                        a.end.min(b.end)
                    }
                    (Some(a), Some(b)) => a.start.min(b.start),
                    (Some(a), None) | (None, Some(a)) => a.start,
                    (None, None) => break,
                };
                earliest = earliest.min(frame);
                break;
            }
        }
        earliest.checked_sub(self.unit_frames)
    }
}

/// A bound image owns no mutable access to continuing allocations.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Layout {
    pub expected: u64,
    pub revision: u64,
    pub region: u8,
    pub loops: Vec<LoopSlot>,
    values: Vec<ValueSlot>,
    fresh_values: Vec<bool>,
    histories: Vec<(StateKey, u32)>,
    fresh_histories: Vec<u32>,
    changed_sources: Vec<Identity>,
    quantum: u32,
    chunk_frames: u32,
    #[cfg_attr(feature = "serde", serde(default))]
    composites: Vec<CompositeSchedule>,
    #[cfg_attr(feature = "serde", serde(default))]
    composite_cuts: Vec<CompositeCut>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Allocations {
    pub state: usize,
    pub loops: usize,
    pub history: usize,
}

impl Layout {
    pub fn allocations(&self) -> Allocations {
        Allocations {
            state: self.values.len(),
            loops: self.loops.len(),
            history: self.histories.len(),
        }
    }

    pub fn fresh_allocations(&self) -> Allocations {
        Allocations {
            state: self.new_values().count(),
            loops: self.new_loops().count(),
            history: self.new_histories().len(),
        }
    }

    pub fn loop_id(&self, logical: Identity) -> Result<Identity, CompileError> {
        self.loops
            .iter()
            .find(|l| l.logical == logical)
            .map(|l| l.physical)
            .ok_or(CompileError::InvalidProgram)
    }
    pub fn state_address(&self, logical: u32) -> u32 {
        self.values[logical as usize].address
    }
    pub fn new_values(&self) -> impl Iterator<Item = (usize, u32)> + '_ {
        self.values
            .iter()
            .enumerate()
            .filter(|(i, _)| self.fresh_values[*i])
            .map(|(i, v)| (i, v.address))
    }
    pub fn media_views(&self, program: &LogicalStateProgram) -> Vec<u64> {
        let count = program
            .initial_state
            .iter()
            .filter_map(|v| match v {
                InitialValue::MediaView(index) => Some(*index as usize + 1),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let mut views = vec![0; count];
        for (address, value) in program.initial_state.iter().enumerate() {
            if let InitialValue::MediaView(index) = *value {
                views[index as usize] = u64::from(self.state_address(address as u32));
            }
        }
        views
    }
    pub fn new_media_views(&self) -> impl Iterator<Item = u32> + '_ {
        self.values
            .iter()
            .enumerate()
            .filter(|(i, v)| self.fresh_values[*i] && v.key.role == StateRole::View)
            .map(|(_, v)| v.address)
    }
    pub fn new_histories(&self) -> &[u32] {
        &self.fresh_histories
    }
    pub fn changed_sources(&self) -> &[Identity] {
        &self.changed_sources
    }
    pub fn composite_cuts(&self) -> &[CompositeCut] {
        &self.composite_cuts
    }
    pub fn composite_schedule(&self, logical: Identity) -> Option<&CompositeSchedule> {
        self.composites
            .iter()
            .find(|schedule| schedule.id == logical)
    }
    pub fn new_loops(&self) -> impl Iterator<Item = Identity> + '_ {
        self.loops.iter().filter(|l| l.fresh).map(|l| l.physical)
    }
    pub fn relocate(
        &self,
        logical: &LogicalStateProgram,
    ) -> Result<LogicalStateProgram, CompileError> {
        let mut bound = logical.clone();
        let address = |v| self.state_address(v);
        let id = |v| self.loop_id(v);
        let count = self.values.iter().map(|v| v.address + 1).max().unwrap_or(0);
        bound.initial_state = vec![
            InitialValue::Ramp(Ramp {
                start: 0,
                target: 0,
                duration: 0,
                elapsed: 0
            });
            count as usize
        ];
        for (i, &value) in logical.initial_state.iter().enumerate() {
            let value = match value {
                InitialValue::Smoothing {
                    destination,
                    duration,
                    ..
                } => InitialValue::Smoothing {
                    destination,
                    duration,
                    history: self
                        .histories
                        .iter()
                        .find(|(owner, _)| *owner == self.values[i].key)
                        .ok_or(CompileError::InvalidProgram)?
                        .1,
                },
                value => value,
            };
            bound.initial_state[address(i as u32) as usize] = value;
        }
        for binding in &mut bound.state_bindings {
            binding.address = address(binding.address);
        }
        for l in &mut bound.loops {
            l.id = id(l.id)?;
            l.sync_source = l.sync_source.map(id).transpose()?;
        }
        for l in &mut bound.sync_pass {
            *l = id(*l)?;
        }
        for schedule in &mut bound.composites {
            schedule.id = id(schedule.id)?;
            for child in &mut schedule.children {
                child.id = id(child.id)?;
            }
        }
        for b in &mut bound.bindings {
            b.id = id(b.id)?;
            b.gain = b.gain.map(address);
            b.view = b.view.map(address);
        }
        for op in &mut bound.work_pass {
            match op {
                Operation::MidiData(
                    crate::MidiOperation::State { loop_id, .. }
                    | crate::MidiOperation::Capture { loop_id, .. }
                    | crate::MidiOperation::Playback { loop_id, .. },
                ) => *loop_id = id(*loop_id)?,
                Operation::Midi {
                    loop_id: Some(loop_id),
                    ..
                } => *loop_id = id(*loop_id)?,
                Operation::Playback { loop_id, view, .. }
                | Operation::Capture { loop_id, view, .. } => {
                    *loop_id = id(*loop_id)?;
                    *view = address(*view);
                }
                Operation::Gain { ramp, .. } => *ramp = address(*ramp),
                Operation::Smoothing {
                    loop_id, control, ..
                } => {
                    *loop_id = id(*loop_id)?;
                    *control = address(*control);
                }
                _ => {}
            }
        }
        bound.resources.history_slots = bound
            .resources
            .history_slots
            .max(self.histories.iter().map(|(_, h)| h + 1).max().unwrap_or(0));
        Ok(bound)
    }
}

/// Only handoff completion releases the previous program's reservations.
pub struct Allocator {
    limits: ProgramLimits,
    current: Option<Layout>,
    pending: Option<Layout>,
    generations: Vec<u32>,
    serial: u64,
    published: bool,
}
impl Allocator {
    pub fn new(limits: ProgramLimits) -> Self {
        Self {
            limits,
            current: None,
            pending: None,
            generations: vec![0; limits.loop_slots as usize],
            serial: 0,
            published: false,
        }
    }
    pub fn current(&self) -> Option<&Layout> {
        self.current.as_ref()
    }
    pub fn pending(&self) -> Option<&Layout> {
        self.pending.as_ref()
    }
    pub fn revision(&self) -> u64 {
        self.current.as_ref().map_or(0, |l| l.revision)
    }
    pub fn stage(&mut self, program: &LogicalStateProgram) -> Result<Layout, LoadError> {
        if self.pending.is_some() {
            return Err(LoadError::Busy);
        }
        program.validate_logical(self.limits)?;
        if self
            .current
            .as_ref()
            .is_some_and(|l| (l.quantum, l.chunk_frames) != (program.quantum, program.chunk_frames))
        {
            return Err(LoadError::Geometry);
        }
        let old = self.current.as_ref();
        let mut changed_composites = BTreeSet::new();
        let mut replaced_slots = BTreeSet::new();
        let mut composite_cuts = Vec::new();
        if let Some(old) = old {
            for schedule in &old.composites {
                if !program.composites.contains(schedule) {
                    changed_composites.insert(schedule.id);
                }
            }
            for schedule in &program.composites {
                if old
                    .composite_schedule(schedule.id)
                    .is_some_and(|before| before != schedule && before.version >= schedule.version)
                {
                    return Err(LoadError::StaleRevision);
                }
                if !old.composites.contains(schedule) {
                    changed_composites.insert(schedule.id);
                }
            }
            // A primitive-to-composite conversion gives the public slot a new
            // generation. Fence the departing primitive before its take can be
            // retired, just as for a conversion that keeps the logical ID.
            let converting = old
                .loops
                .iter()
                .filter(|row| {
                    old.composite_schedule(row.logical).is_none()
                        && !program.loops.iter().any(|l| l.id == row.logical)
                        && program
                            .composites
                            .iter()
                            .any(|plan| plan.id.slot == row.logical.slot)
                })
                .map(|row| row.logical);
            let mut stopped = changed_composites.clone();
            stopped.extend(converting);
            loop {
                let count = stopped.len();
                for schedule in &old.composites {
                    if schedule
                        .children
                        .iter()
                        .any(|child| stopped.contains(&child.id))
                    {
                        stopped.insert(schedule.id);
                    }
                }
                if stopped.len() == count {
                    break;
                }
            }
            for row in &old.loops {
                if stopped.contains(&row.logical) {
                    let before = old.composites.iter().find(|s| s.id == row.logical);
                    let after = program.composites.iter().find(|s| s.id == row.logical);
                    let continuing = before.zip(after).filter(|(before, after)| {
                        (before.unit_frames, before.length, before.explicit)
                            == (after.unit_frames, after.length, after.explicit)
                    });
                    if continuing.is_none() && changed_composites.contains(&row.logical) {
                        replaced_slots.insert(row.logical);
                    }
                    let independent = !old
                        .composites
                        .iter()
                        .chain(&program.composites)
                        .any(|s| s.children.iter().any(|child| child.id == row.logical));
                    let external_source = row.sync_source.filter(|source| *source != row.logical);
                    let restart_source = external_source.and_then(|source| {
                        let old_source = old.loops.iter().find(|loop_| loop_.logical == source)?;
                        let new_source = program.loops.iter().find(|loop_| loop_.id == source)?;
                        let independent_clock = old_source
                            .sync_source
                            .is_none_or(|id| id == source)
                            && new_source.sync_source == old_source.sync_source
                            && !old
                                .composites
                                .iter()
                                .chain(&program.composites)
                                .any(|schedule| {
                                    schedule.id == source
                                        || schedule.children.iter().any(|child| child.id == source)
                                });
                        independent_clock.then_some(old_source.physical)
                    });
                    let same_source = program
                        .loops
                        .iter()
                        .any(|l| l.id == row.logical && l.sync_source == row.sync_source);
                    let allow_restart = changed_composites.contains(&row.logical)
                        && independent
                        && same_source
                        && (external_source.is_none() || restart_source.is_some())
                        && continuing.is_some_and(|(before, after)| {
                            let tail = |s: &CompositeSchedule| -> Vec<Identity> {
                                s.children
                                    .iter()
                                    .filter(|child| {
                                        child.spans.last().is_some_and(|span| span.end == s.length)
                                    })
                                    .map(|child| child.id)
                                    .collect()
                            };
                            !before.explicit && tail(before) == tail(after)
                        });
                    let future_before = if changed_composites.contains(&row.logical)
                        && independent
                        && same_source
                    {
                        continuing.and_then(|(before, after)| before.future_edit_before(after))
                    } else {
                        None
                    };
                    composite_cuts.push(CompositeCut {
                        old_slot: row.physical,
                        require_empty: before.is_none(),
                        allow_restart,
                        restart_source,
                        future_before,
                    });
                }
            }
        }
        let mut loops: Vec<LoopSlot> = Vec::new();
        for l in &program.loops {
            if let Some(existing) = old.and_then(|o| {
                o.loops
                    .iter()
                    .find(|v| v.logical == l.id && !replaced_slots.contains(&l.id))
            }) {
                loops.push(LoopSlot {
                    fresh: false,
                    sync_source: l.sync_source,
                    ..existing.clone()
                });
            } else {
                let slot = (0..self.limits.loop_slots)
                    .find(|&s| {
                        old.is_none_or(|o| o.loops.iter().all(|l| l.physical.slot != s))
                            && loops.iter().all(|l| l.physical.slot != s)
                    })
                    .ok_or(CompileError::Capacity(Resource::Loops))?;
                let generation = self.generations[slot as usize]
                    .checked_add(1)
                    .ok_or(LoadError::Exhausted)?;
                self.generations[slot as usize] = generation;
                loops.push(LoopSlot {
                    logical: l.id,
                    physical: Identity { slot, generation },
                    fresh: true,
                    sync_source: l.sync_source,
                });
            }
        }
        let mut keys = vec![None; program.initial_state.len()];
        let mut assign = |index: u32, key: StateKey| -> Result<(), CompileError> {
            let entry = keys
                .get_mut(index as usize)
                .ok_or(CompileError::InvalidProgram)?;
            if entry.is_some_and(|old| old != key) {
                return Err(CompileError::InvalidProgram);
            }
            *entry = Some(key);
            Ok(())
        };
        for binding in &program.state_bindings {
            assign(binding.address, binding.key)?;
        }
        if let Some(old) = old {
            for entry in loops.iter().filter(|l| !l.fresh) {
                let previous = old
                    .values
                    .iter()
                    .filter(|v| {
                        v.key.owner == shoop_graph::Owner::Loop(entry.logical)
                            && v.key.role == StateRole::View
                    })
                    .map(|v| v.key);
                let next = keys.iter().flatten().filter(|k| {
                    k.owner == shoop_graph::Owner::Loop(entry.logical) && k.role == StateRole::View
                });
                if previous.clone().any(|k| !next.clone().any(|n| *n == k))
                    || next.clone().any(|k| !previous.clone().any(|p| p == *k))
                {
                    return Err(LoadError::IncompatibleState);
                }
            }
        }
        let changed_sources = loops
            .iter()
            .filter(|l| {
                !l.fresh
                    && old
                        .and_then(|o| o.loops.iter().find(|p| p.logical == l.logical))
                        .is_some_and(|p| p.sync_source != l.sync_source)
            })
            .map(|l| l.logical)
            .collect();
        let mut values: Vec<ValueSlot> = Vec::new();
        let mut fresh_values = Vec::new();
        let mut histories = Vec::new();
        let mut fresh_histories = Vec::new();
        for key in keys {
            let key = key.ok_or(CompileError::InvalidProgram)?;
            if values.iter().any(|v| v.key == key) {
                return Err(CompileError::InvalidProgram.into());
            }
            let existing = old.and_then(|l| l.values.iter().find(|v| v.key == key));
            let address = if let Some(v) = existing {
                v.address
            } else {
                (0..self.limits.state_values)
                    .find(|&a| {
                        old.is_none_or(|o| o.values.iter().all(|v| v.address != a))
                            && values.iter().all(|v| v.address != a)
                    })
                    .ok_or(CompileError::Capacity(Resource::State))?
            };
            values.push(ValueSlot { key, address });
            fresh_values.push(existing.is_none());
            if matches!(key.role, StateRole::Smoothing(_)) {
                let previous =
                    old.and_then(|l| l.histories.iter().find(|(owner, _)| *owner == key));
                let history = if let Some((_, h)) = previous {
                    *h
                } else {
                    let h = (0..self.limits.history_slots)
                        .find(|&h| {
                            old.is_none_or(|o| o.histories.iter().all(|(_, v)| *v != h))
                                && histories.iter().all(|(_, v)| *v != h)
                        })
                        .ok_or(CompileError::Capacity(Resource::History))?;
                    fresh_histories.push(h);
                    h
                };
                histories.push((key, history));
            }
        }
        self.serial = self.serial.checked_add(1).ok_or(LoadError::Exhausted)?;
        let layout = Layout {
            expected: self.revision(),
            revision: self.serial,
            region: old.map_or(0, |l| 1 - l.region),
            loops,
            values,
            fresh_values,
            histories,
            fresh_histories,
            changed_sources,
            quantum: program.quantum,
            chunk_frames: program.chunk_frames,
            composites: program.composites.clone(),
            composite_cuts,
        };
        layout.relocate(program)?.validate(self.limits)?;
        self.pending = Some(layout.clone());
        Ok(layout)
    }
    pub fn cancel(&mut self, revision: u64) -> Result<(), LoadError> {
        if self.published {
            return Err(LoadError::Busy);
        }
        if !self
            .pending
            .as_ref()
            .is_some_and(|l| l.revision == revision)
        {
            return Err(LoadError::StaleRevision);
        }
        self.pending = None;
        Ok(())
    }
    pub fn publish(&mut self, revision: u64) -> Result<(), LoadError> {
        if !self
            .pending
            .as_ref()
            .is_some_and(|l| l.revision == revision)
        {
            return Err(LoadError::StaleRevision);
        }
        if self.published {
            return Err(LoadError::Busy);
        }
        self.published = true;
        Ok(())
    }
    pub fn refused(&mut self, revision: u64) -> Result<(), LoadError> {
        if !self
            .pending
            .as_ref()
            .is_some_and(|l| l.revision == revision)
        {
            return Err(LoadError::StaleRevision);
        }
        self.pending = None;
        self.published = false;
        Ok(())
    }
    pub fn complete(&mut self, revision: u64) -> Result<(), LoadError> {
        if !self.published {
            return Err(LoadError::StaleRevision);
        }
        if !self
            .pending
            .as_ref()
            .is_some_and(|l| l.revision == revision && l.expected == self.revision())
        {
            return Err(LoadError::StaleRevision);
        }
        self.current = self.pending.take();
        self.published = false;
        Ok(())
    }
}
