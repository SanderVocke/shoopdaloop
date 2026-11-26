//! ARM-owned reservations and address binding for two executable regions.
use crate::*;
use alloc::vec;

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
    pub fn has_composites(&self) -> bool {
        !self.composites.is_empty()
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
                Operation::Monitor { ramp, recorder } => {
                    *ramp = address(*ramp);
                    *recorder = recorder.map(id).transpose()?;
                }
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
        if let Some(composite) = old.and_then(|layout| {
            layout
                .composites
                .first()
                .or_else(|| program.composites.first())
        }) {
            return Err(CompileError::Unsupported {
                object: shoop_graph::Owner::Loop(composite.id),
                feature: Feature::Composite,
            }
            .into());
        }
        let mut loops: Vec<LoopSlot> = Vec::new();
        for l in &program.loops {
            if let Some(existing) = old.and_then(|o| o.loops.iter().find(|v| v.logical == l.id)) {
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
