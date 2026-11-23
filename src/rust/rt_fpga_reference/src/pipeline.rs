//! One quantum of state, job and media execution.
use crate::{
    fixed::Fixed,
    jobs::{Job, JobProcessor, JobQueue, Memory},
    state::{Address, ModePlan, PlannedTransition, PrepareWork, Program, StateProcessor, Value},
    transitions::*,
    Error,
};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "stage",
        content = "error",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum PipelineError {
    Transition(TransitionError),
    State(Error),
    Jobs(Error),
}

/// The first settled tick and each completed sample quantum of one callback.
pub trait Observer {
    fn settled(&mut self, _state: &StateProcessor) {}
    fn processed(&mut self, _jobs: &JobProcessor) {}
    fn quantum(&mut self, _state: &StateProcessor, _base: usize) {}
}
impl Observer for () {}

pub struct Pipeline {
    revision: u64,
    region: u8,
    state: StateProcessor,
    program: Program,
    queue: JobQueue,
    fault: Option<PipelineError>,
    trace_jobs: bool,
    emitted: Vec<Job>,
}
impl Pipeline {
    pub fn from_image(
        image: crate::target::ProgramImage,
        limits: rt_fpga_compiler::ProgramLimits,
    ) -> Result<Self, PipelineError> {
        let revision = image.layout.revision;
        let region = image.layout.region;
        let compiled = image.compiled;
        let mut pipeline = Self::prepare(
            compiled.plan,
            compiled.program,
            compiled.memory,
            limits.jobs as usize,
        )?;
        pipeline.reserve_loading(limits)?;
        pipeline.revision = revision;
        pipeline.region = region;
        Ok(pipeline)
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn region(&self) -> u8 {
        self.region
    }
    pub fn reserve_loading(
        &mut self,
        limits: rt_fpga_compiler::ProgramLimits,
    ) -> Result<(), PipelineError> {
        self.state.reserve_loading(limits);
        self.queue = JobQueue::new(limits.jobs as usize);
        self.emitted = Vec::with_capacity(limits.jobs as usize);
        Ok(())
    }
    /// Called at a completed quantum boundary; the caller retains the old image until its fence completes.
    pub fn switch_image(
        &mut self,
        image: &mut crate::target::ProgramImage,
    ) -> Result<(), PipelineError> {
        if image.layout.expected != self.revision
            || image.layout.revision <= self.revision
            || (self.revision != 0 && image.layout.region == self.region)
            || self.fault.is_some()
            || !self.queue.is_empty()
            || image.compiled.program.quantum != self.program.quantum
            || image.compiled.program.chunk_frames != self.program.chunk_frames
            || image.compiled.program.jobs_per_quantum > self.queue.remaining()
        {
            return Err(PipelineError::State(Error::InvalidState));
        }
        if image.compiled.program.instructions.iter().any(|i| {
            matches!(i, crate::state::Instruction::ProcessMidiState { .. })
                && !self.program.instructions.contains(i)
        }) {
            return Err(PipelineError::State(Error::InvalidState));
        }
        for cut in image.layout.composite_cuts() {
            let slot = self
                .state
                .loop_state(LoopId {
                    slot: cut.old_slot.slot,
                    generation: cut.old_slot.generation,
                })
                .map_err(PipelineError::Transition)?;
            let ready = (slot.state.mode == Mode::Stopped && !slot.render.plays())
                || (slot.state.mode == Mode::Playing
                    && cut.allows_playing(
                        slot.state.position,
                        slot.state.content_length,
                        self.state.composite_restart_ready(*cut, slot),
                    ));
            if !ready
                || slot.planned.iter().any(Option::is_some)
                || (cut.require_empty && slot.state.content_length != 0)
            {
                return Err(PipelineError::State(Error::InvalidState));
            }
        }
        for instruction in &self.program.instructions {
            if let crate::state::Instruction::ProcessMidiState { loop_id, .. } = instruction {
                if !image.compiled.program.instructions.contains(instruction) {
                    return Err(PipelineError::State(Error::InvalidState));
                }
                self.state
                    .loop_state(*loop_id)
                    .map_err(|_| PipelineError::State(Error::InvalidState))?;
            }
            if let crate::state::Instruction::ProcessMidiCapture {
                loop_id, sequence, ..
            } = instruction
            {
                if self.state.loop_state(*loop_id).is_ok_and(|slot| slot.midi_capture_open)
                    && !image.compiled.program.instructions.iter().any(|next| matches!(next,
                        crate::state::Instruction::ProcessMidiCapture { loop_id: id, sequence: seq, .. }
                        if id == loop_id && seq == sequence)) {
                    return Err(PipelineError::State(Error::InvalidState));
                }
            }
        }
        image
            .compiled
            .program
            .check_job_budget()
            .map_err(PipelineError::State)?;
        self.state
            .switch_image(image)
            .map_err(PipelineError::State)?;
        core::mem::swap(&mut self.program, &mut image.compiled.program);
        self.revision = image.layout.revision;
        self.region = image.layout.region;
        Ok(())
    }
    pub fn prepare(
        plan: TransitionPlan,
        mut program: Program,
        initial_state: Vec<Value>,
        capacity: usize,
    ) -> Result<Self, PipelineError> {
        if plan.image().description.quantum != program.quantum {
            return Err(PipelineError::Transition(TransitionError::InvalidConfig));
        }
        program.check_job_budget().map_err(PipelineError::State)?;
        if capacity < program.jobs_per_quantum {
            return Err(PipelineError::State(Error::QueueFull));
        }
        if program.sync_instructions.is_empty() {
            program.sync_instructions = plan.sync_instructions();
        }
        Ok(Self {
            revision: 0,
            region: 0,
            state: StateProcessor::with_loops(plan, initial_state),
            program,
            queue: JobQueue::new(capacity),
            fault: None,
            trace_jobs: false,
            emitted: Vec::with_capacity(capacity),
        })
    }
    pub fn state(&self) -> &StateProcessor {
        &self.state
    }
    pub fn uses_media_view(&self, view: crate::storage::ViewId) -> bool {
        self.program.instructions.iter().any(|instruction| {
            let address = match instruction {
                crate::state::Instruction::ProcessLogicalPlayback { view, .. }
                | crate::state::Instruction::ProcessLogicalCapture { view, .. }
                | crate::state::Instruction::ProcessLoopPlayback { view, .. }
                | crate::state::Instruction::ProcessLoopCapture { view, .. } => *view,
                _ => return false,
            };
            self.state.read(address).ok().and_then(|v| v.word().ok()) == Some(view.0 as u64)
        })
    }
    pub fn program(&self) -> &Program {
        &self.program
    }
    pub fn fault(&self) -> Option<PipelineError> {
        self.fault
    }
    pub fn trace_jobs(&mut self, enabled: bool) {
        self.trace_jobs = enabled;
    }
    pub fn emitted_jobs(&self) -> &[Job] {
        &self.emitted
    }
    pub fn queued_jobs(&self) -> impl Iterator<Item = &Job> {
        self.queue.iter()
    }
    fn ready(&self) -> Result<(), TransitionError> {
        if self.fault.is_some() {
            Err(TransitionError::InvalidState)
        } else {
            Ok(())
        }
    }
    pub fn write_plans(
        &mut self,
        id: LoopId,
        plans: [Option<PlannedTransition>; 2],
    ) -> Result<(), TransitionError> {
        self.ready()?;
        self.state.write_plans(id, plans)
    }
    pub fn write_mode_plan(&mut self, id: LoopId, plan: ModePlan) -> Result<(), TransitionError> {
        self.ready()?;
        self.state.write_mode_plan(id, plan)
    }
    #[cfg(feature = "rt-interface")]
    pub(crate) fn set_repeat_sync(
        &mut self,
        id: LoopId,
        enabled: bool,
    ) -> Result<(), TransitionError> {
        self.ready()?;
        self.state.set_repeat_sync(id, enabled)
    }
    pub fn clear_mode_plan(&mut self, id: LoopId, mask: u8) -> Result<(), TransitionError> {
        self.ready()?;
        self.state.clear_mode_plan(id, mask)
    }
    #[cfg(feature = "rt-interface")]
    pub(crate) fn initialize_frame(&mut self, frame: u64) -> Result<(), Error> {
        self.state.initialize_frame(frame)
    }
    pub fn write_state(&mut self, address: Address, value: Value) -> Result<(), Error> {
        if self.fault.is_some() {
            return Err(Error::InvalidState);
        }
        self.state.write(address, value)
    }
    /// Failed output is silent. Recreate the engine to resume after a fault.
    pub fn process(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
    ) -> Result<(), PipelineError> {
        self.process_block(jobs, memory, inputs, outputs, self.program.quantum as usize)
    }
    pub fn process_block(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
        frames: usize,
    ) -> Result<(), PipelineError> {
        self.process_observed(jobs, memory, inputs, outputs, frames, &mut ())
    }
    pub fn process_observed(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
        frames: usize,
        observer: &mut impl Observer,
    ) -> Result<(), PipelineError> {
        self.process_mixed(jobs, memory, inputs, outputs, frames, observer, None)
    }
    fn process_mixed(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
        frames: usize,
        observer: &mut impl Observer,
        mut midi: Option<&mut crate::midi::Io<'_>>,
    ) -> Result<(), PipelineError> {
        for output in outputs.iter_mut() {
            output.fill(Fixed::ZERO);
        }
        self.emitted.clear();
        if let Some(error) = self.fault {
            return Err(error);
        }
        let q = self.program.quantum as usize;
        if q == 0
            || frames == 0
            || frames % q != 0
            || jobs.quantum() != q
            || memory.chunk_frames() != self.program.chunk_frames as usize
            || inputs.iter().any(|input| input.len() != frames)
            || outputs.iter().any(|output| output.len() != frames)
        {
            return Err(PipelineError::Jobs(Error::InvalidShape));
        }
        let result = (0..frames).step_by(q).try_for_each(|base| {
            self.process_quantum_mixed(
                jobs,
                memory,
                inputs,
                outputs,
                base,
                observer,
                base == 0,
                midi.as_deref_mut(),
            )
        });
        if let Err(error) = result {
            self.fault = Some(error);
            for output in outputs.iter_mut() {
                output.fill(Fixed::ZERO);
            }
        }
        result
    }
    /// Publish a completed sequence while its loop is stopped and its capture has finished.
    pub fn replace_midi_view(
        &mut self,
        memory: &mut crate::midi::MidiMemory,
        loop_id: LoopId,
        sequence: crate::midi::SequenceId,
        view: Option<crate::midi::ViewId>,
    ) -> Result<(), crate::midi::Error> {
        use crate::midi::Error as M;
        if self.fault.is_some() {
            return Err(M::InvalidState);
        }
        if !self.program.instructions.iter().any(|instruction| {
            matches!(instruction,
            crate::state::Instruction::ProcessMidiPlayback { loop_id: id, sequence: seq, .. }
            | crate::state::Instruction::ProcessMidiCapture { loop_id: id, sequence: seq, .. }
            if *id == loop_id && *seq == sequence)
        }) {
            return Err(M::InvalidState);
        }
        let slot = self
            .state
            .loop_state(loop_id)
            .map_err(|_| M::InvalidState)?;
        if slot.state.mode != Mode::Stopped || slot.midi_capture_open {
            return Err(M::Busy);
        }
        let frames = match view {
            Some(v) => {
                let d = memory.store.describe(v)?;
                if d.status != crate::midi::TakeStatus::Complete {
                    return Err(M::Incomplete);
                }
                d.frames
            }
            None => 0,
        };
        let updates = [(loop_id, frames)];
        self.state
            .validate_midi_contents(&updates)
            .map_err(|_| M::InvalidState)?;
        memory.replace(sequence, view)?;
        self.state
            .publish_midi_contents(&updates)
            .expect("validated content publication");
        Ok(())
    }
    /// Prepare compiled state bindings outside callback processing.
    pub fn prepare_midi_state(
        &self,
        memory: &mut crate::midi::MidiMemory,
    ) -> Result<(), crate::midi::Error> {
        let limits = memory
            .expression
            .as_ref()
            .ok_or(crate::midi::Error::Unsupported)?
            .limits;
        for instruction in &self.program.instructions {
            match instruction {
                crate::state::Instruction::ProcessMidiInput { port, .. }
                    if *port >= limits.inputs =>
                {
                    return Err(crate::midi::Error::InvalidState)
                }
                crate::state::Instruction::ProcessMidiOutput { port, .. }
                    if *port >= limits.outputs =>
                {
                    return Err(crate::midi::Error::InvalidState)
                }
                _ => {}
            }
            if let crate::state::Instruction::ProcessMidiState {
                loop_id,
                sequence,
                input,
                outputs,
                channels,
            } = *instruction
            {
                memory.bind_state(
                    sequence,
                    crate::midi::StateBinding {
                        owner: loop_id,
                        input,
                        outputs,
                        channels,
                    },
                )?;
            }
        }
        Ok(())
    }
    pub fn process_with_midi(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
        frames: usize,
        midi: &mut crate::midi::MidiMemory,
        events: &[crate::midi::InputEvent],
        sink: &mut impl FnMut(crate::midi::OutputEvent) -> Result<(), crate::midi::Error>,
    ) -> Result<crate::midi::Work, PipelineError> {
        if let Some(error) = self.fault {
            for output in outputs.iter_mut() {
                output.fill(Fixed::ZERO);
            }
            return Err(error);
        }
        if let Err(error) = jobs.begin_midi_callback(events, frames) {
            for output in outputs.iter_mut() {
                output.fill(Fixed::ZERO);
            }
            return Err(PipelineError::Jobs(Error::Midi(error)));
        }
        if let Some(state) = &mut midi.expression {
            state.work = crate::midi::StateWork::default();
        }
        let mut io = crate::midi::Io {
            memory: midi,
            inputs: events,
            sink,
        };
        self.process_mixed(
            jobs,
            memory,
            inputs,
            outputs,
            frames,
            &mut (),
            Some(&mut io),
        )?;
        Ok(jobs.midi_work())
    }
    #[cfg(feature = "rt-interface")]
    pub(crate) fn process_quantum(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
        base: usize,
        observer: &mut impl Observer,
        publish: bool,
    ) -> Result<(), PipelineError> {
        self.process_quantum_mixed(jobs, memory, inputs, outputs, base, observer, publish, None)
    }
    pub(crate) fn process_quantum_mixed(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
        base: usize,
        observer: &mut impl Observer,
        publish: bool,
        mut midi: Option<&mut crate::midi::Io<'_>>,
    ) -> Result<(), PipelineError> {
        self.emitted.clear();
        if let Some(error) = memory.fault() {
            return Err(PipelineError::Jobs(error));
        }
        let state_result =
            self.state
                .run_with_preparation(&self.program, &mut self.queue, |work| match work {
                    PrepareWork::Read(intent) => {
                        intent.prefetch(memory);
                        Ok(())
                    }
                    PrepareWork::Captures { requests, frames } => {
                        memory.prepare_capture_writes(requests, frames)
                    }
                });
        if self.trace_jobs {
            self.emitted.extend(self.queue.iter().copied());
        }
        state_result.map_err(|error| match error {
            Error::Transition(error) => PipelineError::Transition(error),
            error => PipelineError::State(error),
        })?;
        if publish {
            observer.settled(&self.state);
        }
        jobs.run_at_mixed(
            &mut self.queue,
            memory,
            inputs,
            outputs,
            base,
            midi.as_deref_mut(),
        )
        .map_err(PipelineError::Jobs)?;
        if let Some(error) = memory.fault() {
            return Err(PipelineError::Jobs(error));
        }
        if let Some(io) = midi {
            io.memory.store.maintenance(64);
        }
        observer.quantum(&self.state, base);
        observer.processed(jobs);
        Ok(())
    }
    #[cfg(feature = "rt-interface")]
    pub(crate) fn latch_fault(&mut self, error: PipelineError) {
        self.fault = Some(error);
    }
}
