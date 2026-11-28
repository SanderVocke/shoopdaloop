//! Lower logical programs to packed RTL instructions and configuration RAM images.
use crate::{bits::Bits, component};
use alloc::{collections::BTreeMap, vec, vec::Vec};
use rt_fpga_compiler::{
    CompileError, Identity, InitialValue, LogicalStateProgram, Operation, ProgramLimits, Resource,
    Resources, StateProgramTarget,
};
#[path = "target_midi.rs"]
mod midi;

pub(crate) const COMPOSITE_LOOP: u64 = 1 << 45;

pub struct RtlTarget<'a> {
    pub capacities: ProgramLimits,
    pub media_views: &'a [u64],
    pub state_base: u16,
}
impl<'a> RtlTarget<'a> {
    pub fn build_image(
        &self,
        logical: &LogicalStateProgram,
        layout: rt_fpga_compiler::loading::Layout,
    ) -> Result<ProgramImage, CompileError> {
        self.image(logical, layout, false)
    }
    #[cfg(feature = "rt-interface")]
    pub fn build_semantic_image(
        &self,
        logical: &LogicalStateProgram,
        layout: rt_fpga_compiler::loading::Layout,
    ) -> Result<ProgramImage, CompileError> {
        if self.state_base != 0 {
            return Err(CompileError::InvalidProgram);
        }
        self.image(logical, layout, true)
    }
    fn image(
        &self,
        logical: &LogicalStateProgram,
        layout: rt_fpga_compiler::loading::Layout,
        whole_sync_prefix: bool,
    ) -> Result<ProgramImage, CompileError> {
        if self.capacities.instructions > 32768 {
            return Err(CompileError::Capacity(Resource::Instructions));
        }
        let compiled = self.lower(&layout.relocate(logical)?, true, true, whole_sync_prefix)?;
        let initializers = layout
            .new_values()
            .map(|(_, address)| {
                let address = self.address(address)?;
                Ok((address, compiled.state[usize::from(address)].clone()))
            })
            .collect::<Result<_, CompileError>>()?;
        let new_loops = compiled
            .loops
            .iter()
            .filter(|row| layout.new_loops().any(|id| id == row.id))
            .map(|row| LoopImage {
                id: row.id,
                words: row.words,
            })
            .collect();
        Ok(ProgramImage {
            layout,
            compiled,
            initializers,
            new_loops,
            #[cfg(feature = "rt-interface")]
            whole_sync_prefix,
        })
    }
    pub fn new(capacities: [u64; 7], loop_slots: u32, media_views: &'a [u64]) -> Self {
        let count = |n: u64| n.min(65535) as u32;
        Self {
            capacities: ProgramLimits {
                instructions: count(capacities[0]),
                state_values: count(capacities[1]),
                jobs: count(capacities[2]),
                quantum: capacities[3].min(128) as u32,
                buffers: count(capacities[4]),
                history_slots: count(capacities[4]),
                input_ports: count(capacities[5]),
                output_ports: count(capacities[6]),
                loop_slots: loop_slots.min(4096),
            },
            media_views,
            state_base: 0,
        }
    }
    fn address(&self, logical: u32) -> Result<u16, CompileError> {
        u16::try_from(
            u32::from(self.state_base)
                .checked_add(logical)
                .ok_or(CompileError::Capacity(Resource::State))?,
        )
        .map_err(|_| CompileError::Capacity(Resource::State))
    }
}
pub struct Binding {
    pub id: Identity,
    pub initial_gain_bits: Option<u32>,
    pub gain: Option<u16>,
    pub view: Option<u16>,
    pub meter: Option<u16>,
}
pub struct LoopImage {
    pub id: Identity,
    pub words: [u64; 16],
}
pub struct ProgramImage {
    pub layout: rt_fpga_compiler::loading::Layout,
    pub compiled: CompiledProgram,
    pub initializers: Vec<(u16, Bits)>,
    pub new_loops: Vec<LoopImage>,
    #[cfg(feature = "rt-interface")]
    pub(crate) whole_sync_prefix: bool,
}
pub struct CompiledProgram {
    pub instructions: Vec<Bits>,
    pub state: Vec<Bits>,
    pub loops: Vec<LoopImage>,
    pub bindings: Vec<Binding>,
    pub quantum: u32,
    pub chunk_frames: u32,
    /// Number of loop visits in the stream, excluding inline schedule rows.
    pub sync_count: usize,
    pub jobs_per_quantum: u32,
    pub resources: Resources,
}
fn id(value: Identity) -> component::Identity {
    component::Identity {
        slot: value.slot,
        generation: value.generation,
    }
}
fn operand(value: u32) -> Result<u16, CompileError> {
    u16::try_from(value).map_err(|_| CompileError::InvalidProgram)
}
fn encode(instruction: component::Instruction) -> Result<Bits, CompileError> {
    component::instruction_words(instruction)
        .map(|words| Bits::from(words).resized(96))
        .map_err(|_| CompileError::InvalidProgram)
}

fn append_work(instructions: &mut Vec<Bits>, work: Vec<Bits>, interleave: bool) {
    let is_midi = |word: &Bits| matches!(word.0[0] & 0xff, 0x0e..=0x15);
    if !interleave || !work.iter().any(is_midi) || work.iter().all(is_midi) {
        instructions.extend(work);
        return;
    }
    let (midi, audio): (Vec<_>, Vec<_>) = work.into_iter().partition(is_midi);
    let mut midi = midi.into_iter();
    for job in audio {
        instructions.push(job);
        instructions.extend(midi.next());
    }
    instructions.extend(midi);
}

#[derive(Clone, Copy, Default)]
struct Access {
    read_slot: usize,
    write_slot: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Dependency {
    AudioScratch(u16),
    AudioView(u64),
    Ramp(u16),
    History(u16),
    AudioOutput(u16),
    MidiList(u16),
    MidiSequence(u16),
    MidiInputPort(u16),
    MidiPackets,
}

struct WorkSchedule {
    sync_slot_by_loop: Vec<usize>,
    scratch: Vec<Access>,
    other: BTreeMap<Dependency, Access>,
    midi_prepare_slot: usize,
}

impl WorkSchedule {
    fn access(&mut self, dependency: Dependency) -> &mut Access {
        match dependency {
            Dependency::AudioScratch(id) => &mut self.scratch[usize::from(id)],
            other => self.other.entry(other).or_default(),
        }
    }

    fn slot(&mut self, instruction: component::Instruction, state: &[Bits]) -> usize {
        use Dependency::*;
        use component::Instruction::*;
        if let MidiMergeOutput { sources, count, .. } = instruction {
            let reads = sources.map(|source| Some(MidiList(u16::from(source))));
            return self.reserve_slot(0, &reads[..usize::from(count)], &[Some(MidiPackets)]);
        }
        let synced = |id: component::Identity| self.sync_slot_by_loop[id.slot as usize];
        let media_view = |address: u16| AudioView(state[usize::from(address)].get(0, 64));
        let (slot, reads, writes) = match instruction {
            ProcessInput { destination, .. } | ProcessSilence { destination } => {
                (0, [None, None], [Some(AudioScratch(destination)), None])
            }
            ProcessLogicalPlayback {
                loop_id,
                view,
                destination,
            } => (
                synced(loop_id),
                [Some(media_view(view)), None],
                [Some(AudioScratch(destination)), None],
            ),
            ProcessMix {
                left,
                right,
                destination,
            } => (
                0,
                [Some(AudioScratch(left)), Some(AudioScratch(right))],
                [Some(AudioScratch(destination)), None],
            ),
            ProcessGain {
                ramp,
                source,
                destination,
            } => (
                0,
                [Some(AudioScratch(source)), None],
                [Some(AudioScratch(destination)), Some(Ramp(ramp))],
            ),
            ProcessInputMonitor { ramp, recorder } => (
                recorder.map_or(0, synced),
                [Some(Ramp(ramp)), None],
                [Some(Ramp(ramp)), None],
            ),
            ProcessHistorySmoothing {
                loop_id,
                history,
                source,
                destination,
                ..
            } => (
                synced(loop_id),
                [Some(AudioScratch(source)), None],
                [Some(AudioScratch(destination)), Some(History(history))],
            ),
            ProcessLogicalCapture {
                loop_id,
                view,
                source,
            } => (
                synced(loop_id),
                [Some(AudioScratch(source)), None],
                [Some(media_view(view)), None],
            ),
            ProcessOutput { source, port } => (
                0,
                [Some(AudioScratch(source)), None],
                [Some(AudioOutput(port)), None],
            ),
            MidiPrepareCapture { loop_id, sequence } => (
                synced(loop_id),
                // Opening a take refreshes its tracker against the current output state.
                [Some(MidiPackets), None],
                [Some(MidiSequence(sequence)), None],
            ),
            MidiState {
                loop_id, sequence, ..
            } => (
                synced(loop_id),
                [Some(MidiSequence(u16::from(sequence))), None],
                [Some(MidiPackets), None],
            ),
            MidiInput { port, destination } => (
                // A new take snapshots its runtime-bound input before observing this quantum.
                self.midi_prepare_slot,
                [None, None],
                [Some(MidiList(destination)), Some(MidiInputPort(port))],
            ),
            MidiPlayback {
                loop_id,
                sequence,
                destination,
                ..
            } => (
                synced(loop_id),
                [Some(MidiSequence(u16::from(sequence))), None],
                [Some(MidiList(u16::from(destination))), None],
            ),
            MidiAppend {
                loop_id,
                sequence,
                source,
            } => (
                synced(loop_id),
                [Some(MidiList(source)), None],
                [Some(MidiSequence(sequence)), None],
            ),
            MidiMerge {
                left,
                right,
                destination,
            } => (
                0,
                [Some(MidiList(left)), Some(MidiList(right))],
                [Some(MidiList(destination)), None],
            ),
            MidiOutput { source, .. } => {
                (0, [Some(MidiList(source)), None], [Some(MidiPackets), None])
            }
            _ => unreachable!("only instructions lowered from a logical program are scheduled"),
        };
        let slot = self.reserve_slot(slot, &reads, &writes);
        if matches!(instruction, MidiPrepareCapture { .. }) {
            self.midi_prepare_slot = self.midi_prepare_slot.max(slot);
        }
        slot
    }

    fn reserve_slot(
        &mut self,
        mut slot: usize,
        reads: &[Option<Dependency>],
        writes: &[Option<Dependency>],
    ) -> usize {
        for &resource in reads.iter().flatten() {
            slot = slot.max(self.access(resource).write_slot);
        }
        for &resource in writes.iter().flatten() {
            let access = self.access(resource);
            slot = slot.max(access.read_slot).max(access.write_slot);
        }
        for &resource in reads.iter().flatten() {
            let access = self.access(resource);
            access.read_slot = access.read_slot.max(slot);
        }
        for &resource in writes.iter().flatten() {
            self.access(resource).write_slot = slot;
        }
        slot
    }
}

impl StateProgramTarget for RtlTarget<'_> {
    type CompiledProgram = CompiledProgram;
    fn limits(&self) -> ProgramLimits {
        ProgramLimits {
            instructions: self.capacities.instructions.min(65535),
            state_values: self.capacities.state_values.min(65535),
            jobs: u32::MAX,
            buffers: self.capacities.buffers.min(65535),
            history_slots: self.capacities.history_slots.min(65535),
            input_ports: self.capacities.input_ports.min(65535),
            output_ports: self.capacities.output_ports.min(65535),
            loop_slots: self.capacities.loop_slots.min(4096),
            quantum: self.capacities.quantum.min(128),
        }
    }
    fn build(&self, logical: &LogicalStateProgram) -> Result<CompiledProgram, CompileError> {
        self.build_with_midi_merge_output(logical, true)
    }
}

impl RtlTarget<'_> {
    pub fn build_with_midi_merge_output(
        &self,
        logical: &LogicalStateProgram,
        merge_output: bool,
    ) -> Result<CompiledProgram, CompileError> {
        self.build_with_midi_options(logical, merge_output, true)
    }

    pub fn build_with_midi_options(
        &self,
        logical: &LogicalStateProgram,
        merge_output: bool,
        interleave: bool,
    ) -> Result<CompiledProgram, CompileError> {
        self.lower(logical, merge_output, interleave, false)
    }
    fn lower(
        &self,
        logical: &LogicalStateProgram,
        merge_output: bool,
        interleave: bool,
        whole_sync_prefix: bool,
    ) -> Result<CompiledProgram, CompileError> {
        logical.validate(self.limits())?;
        if usize::from(self.state_base) + logical.initial_state.len()
            > self.limits().state_values as usize
        {
            return Err(CompileError::Capacity(Resource::State));
        }
        let mut state = vec![Bits::zero(129); usize::from(self.state_base)];
        for value in &logical.initial_state {
            state.push(
                Bits::from(component::value_words(match *value {
                    InitialValue::MediaView(view) => component::Value::Word(
                        *self
                            .media_views
                            .get(view as usize)
                            .ok_or(CompileError::MissingView(view))?,
                    ),
                    InitialValue::Ramp(r) => component::Value::Ramp(component::Ramp {
                        start: r.start,
                        target: r.target,
                        duration: r.duration,
                        elapsed: r.elapsed,
                    }),
                    InitialValue::Smoothing { duration, .. } => {
                        component::Value::Word(u64::from(duration) << 32)
                    }
                }))
                .resized(129),
            );
        }
        let mut recording_views = alloc::collections::BTreeSet::new();
        if logical.has_authored_recording() {
            for operation in &logical.work_pass {
                if let Operation::Capture { view, .. } = *operation {
                    let InitialValue::MediaView(index) = logical.initial_state[view as usize]
                    else {
                        return Err(CompileError::InvalidProgram);
                    };
                    if !recording_views.insert(self.media_views[index as usize]) {
                        return Err(CompileError::InvalidProgram);
                    }
                }
            }
        }
        let capture_views = u16::try_from(recording_views.len())
            .map_err(|_| CompileError::Capacity(Resource::Jobs))?;
        let sync_count = logical.sync_pass.len();
        let loop_table_size = logical
            .sync_pass
            .iter()
            .map(|id| id.slot as usize + 1)
            .max()
            .unwrap_or(0);
        let mut schedule = WorkSchedule {
            sync_slot_by_loop: vec![0; loop_table_size],
            scratch: vec![Access::default(); logical.resources.buffers as usize],
            other: BTreeMap::new(),
            midi_prepare_slot: 0,
        };
        let mut sync_instructions = Vec::with_capacity(sync_count);
        let mut work = vec![Vec::new(); sync_count + 1];
        for (index, loop_id) in logical.sync_pass.iter().enumerate() {
            schedule.sync_slot_by_loop[loop_id.slot as usize] = index + 1;
            let mut rows = Vec::new();
            let mut bits = Bits::zero(96);
            bits.set(8, 16, u64::from(operand(loop_id.slot)?)).set(
                24,
                32,
                u64::from(loop_id.generation),
            );
            let source = logical
                .loops
                .iter()
                .find(|l| l.id == *loop_id)
                .unwrap()
                .sync_source
                .map_or(u16::MAX, |s| s.slot as u16);
            let words = u16::try_from(
                logical
                    .composite_parents(*loop_id)
                    .map(|(_, child)| 1 + child.spans.len())
                    .sum::<usize>(),
            )
            .map_err(|_| CompileError::Capacity(Resource::Instructions))?;
            if words != 0 {
                bits.set(0, 8, 0x16)
                    .set(56, 16, u64::from(source))
                    .set(72, 16, u64::from(words));
                rows.push(bits);
                for (parent, child) in logical.composite_parents(*loop_id) {
                    let span_count = child.spans.len() as u64;
                    let mut header = Bits::zero(96);
                    header
                        .set(0, 16, u64::from(operand(parent.id.slot)?))
                        .set(16, 32, u64::from(parent.id.generation))
                        .set(48, 6, span_count & 63)
                        .set(54, 16, u64::from(capture_views))
                        .set(70, 10, span_count >> 6);
                    rows.push(header);
                    for span in &child.spans {
                        let mut row = Bits::zero(96);
                        row.set(0, 32, u64::from(span.start))
                            .set(32, 32, u64::from(span.end))
                            .set(
                                64,
                                2,
                                match span.mode {
                                    rt_fpga_compiler::SpanMode::Play => 1,
                                    rt_fpga_compiler::SpanMode::Stop => 2,
                                    rt_fpga_compiler::SpanMode::Record => 3,
                                },
                            );
                        rows.push(row);
                    }
                }
                sync_instructions.push(rows);
                continue;
            }
            bits.set(0, 8, 12)
                .set(56, 16, u64::from(source))
                .set(72, 1, 1);
            rows.push(bits);
            sync_instructions.push(rows);
        }
        use rt_fpga_compiler::MidiOperation as M;
        let fusion = merge_output.then(|| midi::merge_outputs(&logical.work_pass));
        let byte = |value| u8::try_from(value).map_err(|_| CompileError::InvalidProgram);
        for op in &logical.work_pass {
            let setup = match *op {
                Operation::MidiData(M::Capture {
                    loop_id, sequence, ..
                }) => component::Instruction::MidiPrepareCapture {
                    loop_id: id(loop_id),
                    sequence,
                },
                Operation::MidiData(M::State {
                    loop_id,
                    sequence,
                    input,
                    outputs,
                    channels,
                }) => component::Instruction::MidiState {
                    loop_id: id(loop_id),
                    sequence: byte(sequence)?,
                    input: byte(input)?,
                    outputs,
                    channels,
                },
                _ => continue,
            };
            let slot = schedule.slot(setup, &state);
            work[slot].push(encode(setup)?);
        }
        for (index, op) in logical.work_pass.iter().enumerate() {
            if fusion
                .as_ref()
                .is_some_and(|plan| plan[index] == midi::Lowering::Remove)
            {
                continue;
            }
            let instruction = match *op {
                Operation::Midi { .. } => return Err(CompileError::InvalidProgram),
                Operation::MidiData(operation) => match operation {
                    M::State { .. } => continue,
                    M::Input { port, destination } => {
                        component::Instruction::MidiInput { port, destination }
                    }
                    M::Capture {
                        loop_id,
                        sequence,
                        source,
                    } => component::Instruction::MidiAppend {
                        loop_id: id(loop_id),
                        sequence,
                        source,
                    },
                    M::Playback {
                        loop_id,
                        sequence,
                        cursor,
                        channels,
                        destination,
                    } => component::Instruction::MidiPlayback {
                        loop_id: id(loop_id),
                        sequence: byte(sequence)?,
                        cursor: byte(cursor)?,
                        channels,
                        destination: byte(destination)?,
                    },
                    M::Merge {
                        left,
                        right,
                        destination,
                    } => component::Instruction::MidiMerge {
                        left,
                        right,
                        destination,
                    },
                    M::Output { source, port } => match fusion.as_ref().map(|plan| plan[index]) {
                        Some(midi::Lowering::Output { sources, count }) => {
                            component::Instruction::MidiMergeOutput {
                                sources,
                                count,
                                port,
                            }
                        }
                        _ => component::Instruction::MidiOutput { source, port },
                    },
                },
                Operation::Silence { destination } => component::Instruction::ProcessSilence {
                    destination: operand(destination)?,
                },
                Operation::Mix {
                    left,
                    right,
                    destination,
                } => component::Instruction::ProcessMix {
                    left: operand(left)?,
                    right: operand(right)?,
                    destination: operand(destination)?,
                },
                Operation::Input { port, destination } => component::Instruction::ProcessInput {
                    port: operand(port)?,
                    destination: operand(destination)?,
                },
                Operation::Playback {
                    loop_id,
                    view,
                    destination,
                } => component::Instruction::ProcessLogicalPlayback {
                    loop_id: id(loop_id),
                    view: self.address(view)?,
                    destination: operand(destination)?,
                },
                Operation::Capture {
                    loop_id,
                    view,
                    source,
                } => component::Instruction::ProcessLogicalCapture {
                    loop_id: id(loop_id),
                    view: self.address(view)?,
                    source: operand(source)?,
                },
                Operation::Gain {
                    ramp,
                    source,
                    destination,
                } => component::Instruction::ProcessGain {
                    ramp: self.address(ramp)?,
                    source: operand(source)?,
                    destination: operand(destination)?,
                },
                Operation::Monitor { ramp, recorder } => {
                    component::Instruction::ProcessInputMonitor {
                        ramp: self.address(ramp)?,
                        recorder: recorder.map(id),
                    }
                }
                Operation::Smoothing {
                    loop_id,
                    control,
                    source,
                } => {
                    let InitialValue::Smoothing {
                        destination,
                        history,
                        ..
                    } = logical.initial_state[control as usize]
                    else {
                        unreachable!()
                    };
                    component::Instruction::ProcessHistorySmoothing {
                        loop_id: id(loop_id),
                        control: self.address(control)?,
                        source: operand(source)?,
                        destination: operand(destination)?,
                        history: operand(history)?,
                    }
                }
                Operation::Output { source, port } => component::Instruction::ProcessOutput {
                    source: operand(source)?,
                    port: operand(port)?,
                },
            };
            let slot = schedule.slot(instruction, &state);
            work[slot].push(encode(instruction)?);
        }
        let mut instructions = Vec::with_capacity(sync_count + logical.work_pass.len());
        if whole_sync_prefix {
            instructions.extend(sync_instructions.into_iter().flatten());
            for slot in work {
                append_work(&mut instructions, slot, interleave);
            }
        } else {
            let mut work = work.into_iter();
            append_work(&mut instructions, work.next().unwrap(), interleave);
            for (sync, work) in sync_instructions.into_iter().zip(work) {
                instructions.extend(sync);
                append_work(&mut instructions, work, interleave);
            }
        }
        if instructions.len() > self.limits().instructions as usize {
            return Err(CompileError::Capacity(Resource::Instructions));
        }
        let loops = logical
            .loops
            .iter()
            .map(|l| {
                let mut words = [0; 16];
                words[0] = 1 | (u64::from(l.id.generation) << 1) | (u64::from(l.repeat_sync) << 35);
                if let Some(schedule) = logical
                    .composites
                    .iter()
                    .find(|schedule| schedule.id == l.id)
                {
                    words[0] |= u64::from(schedule.explicit) << 62;
                    words[1] |= COMPOSITE_LOOP;
                    words[6] = u64::from(schedule.unit_frames);
                }
                if let Some(source) = l.sync_source {
                    words[0] |= (1 << 36) | (u64::from(source.slot) << 37);
                    words[1] |= u64::from(source.generation);
                }
                if logical
                    .composite_parents(l.id)
                    .any(|(_, child)| !child.spans.is_empty())
                {
                    words[1] |= 1 << 44;
                }
                words[3] = l.length;
                LoopImage { id: l.id, words }
            })
            .collect();
        let bindings = logical
            .bindings
            .iter()
            .map(|b| {
                Ok(Binding {
                    id: b.id,
                    initial_gain_bits: b.initial_gain_bits,
                    gain: b.gain.map(|a| self.address(a)).transpose()?,
                    view: b.view.map(|a| self.address(a)).transpose()?,
                    meter: b.meter.map(operand).transpose()?,
                })
            })
            .collect::<Result<Vec<_>, CompileError>>()?;
        Ok(CompiledProgram {
            instructions,
            state,
            loops,
            bindings,
            quantum: logical.quantum,
            chunk_frames: logical.chunk_frames,
            sync_count,
            jobs_per_quantum: logical.jobs_per_quantum,
            resources: logical.resources,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Program,
    State,
    Loop,
    History,
}
pub struct ConfigurationWrite {
    pub space: Space,
    pub address: u64,
    pub value: Bits,
}
impl ProgramImage {
    /// Contains no initializers for state retained from the current program.
    pub fn writes(&self, region_depth: u64) -> Vec<ConfigurationWrite> {
        let mut writes = Vec::new();
        for (i, value) in self.compiled.instructions.iter().enumerate() {
            writes.push(ConfigurationWrite {
                space: Space::Program,
                address: u64::from(self.layout.region) * region_depth + i as u64,
                value: value.clone(),
            });
        }
        for (address, value) in &self.initializers {
            writes.push(ConfigurationWrite {
                space: Space::State,
                address: u64::from(*address),
                value: value.clone(),
            });
        }
        for row in &self.new_loops {
            for (word, value) in row.words.iter().enumerate() {
                writes.push(ConfigurationWrite {
                    space: Space::Loop,
                    address: u64::from(row.id.slot) * 16 + word as u64,
                    value: Bits::from_u64(*value),
                });
            }
        }
        for &address in self.layout.new_histories() {
            writes.push(ConfigurationWrite {
                space: Space::History,
                address: u64::from(address),
                value: Bits::zero(32),
            });
        }
        writes
    }
}
