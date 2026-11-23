//! Carrier endpoint and independently driven fixed-graph processor.
use crate::{
    fixed::{Fixed, Ramp},
    jobs::{BufferId, JobProcessor, Memory},
    pipeline::{Observer, Pipeline, PipelineError},
    state::{prepare_composite_iteration, Address, ModePlan, ModeStep, StateProcessor, Value},
    transitions::{LoopId, Mode, TransitionError},
    Error,
};
use alloc::{boxed::Box, vec, vec::Vec};
use shoop_rt_protocol::{
    codec,
    immediate::{Code, Command, TransitionMode, TransitionPlan},
    snapshot::{self, Record},
    LoopMode, LoopSlotId, Peaks, SlotId, StateRecord, Telemetry, TimingState,
};
use shoop_rt_transport::rt::Execution;
pub use shoop_rt_transport::rt::{Endpoint, Settings};
mod loading;
pub use loading::{
    program_memory, PreparedProgram, ProgramControl, ProgramExecution, ProgramReady,
};

#[derive(Clone, Copy, Debug)]
pub struct Configuration {
    pub graph: u64,
    pub topology: u64,
    pub frame: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct LoopBinding {
    pub id: LoopId,
    pub public_id: LoopId,
    pub gain: Option<Address>,
    pub meter: Option<BufferId>,
    pub initial_gain: f32,
    pub timing: TimingState,
}
impl LoopBinding {
    pub fn new(id: LoopId) -> Self {
        Self {
            id,
            public_id: id,
            gain: None,
            meter: None,
            initial_gain: 1.,
            timing: TimingState::default(),
        }
    }
}
struct Loop {
    binding: LoopBinding,
    gain: f32,
    peak: f32,
}
struct Engine {
    pipeline: Pipeline,
    configuration: Configuration,
    loops: Vec<Loop>,
}
/// Driver-owned execution; carrier delivery and polling never run this pipeline.
pub struct Processor {
    engine: Engine,
    ports: Execution,
    snapshot_bytes: Box<[u8]>,
    programs: Option<ProgramExecution>,
    next_program: Option<Box<PreparedProgram>>,
    retired_program: Option<Box<PreparedProgram>>,
    pending_switch: Option<loading::PendingSwitch>,
}
impl Processor {
    pub fn prepare(
        mut pipeline: Pipeline,
        ports: Execution,
        configuration: Configuration,
        mut bindings: Vec<LoopBinding>,
    ) -> Result<Self, Error> {
        pipeline.initialize_frame(configuration.frame)?;
        bindings.sort_unstable_by_key(|b| b.public_id);
        if bindings.len() != pipeline.state().loop_states().count()
            || bindings.windows(2).any(|w| w[0].id.slot == w[1].id.slot)
        {
            return Err(Error::InvalidState);
        }
        for (i, binding) in bindings.iter().enumerate() {
            if binding.gain.is_some() && bindings[..i].iter().any(|b| b.gain == binding.gain) {
                return Err(Error::InvalidState);
            }
            let value = pipeline
                .state()
                .loop_state(binding.id)
                .map_err(Error::Transition)?
                .state;
            if binding.timing.windows != shoop_rt_protocol::PlaybackWindows::default()
                || ![0, value.content_length].contains(&binding.timing.length)
                || ![0, value.content_length].contains(&binding.timing.raw_audio_frames)
                || ![0, value.content_length].contains(&binding.timing.raw_midi_frames)
                || binding.timing.content_revision != value.content_revision
            {
                return Err(Error::InvalidState);
            }
            if !binding.initial_gain.is_finite() {
                return Err(Error::InvalidState);
            }
            if let Some(address) = binding.gain {
                pipeline
                    .state()
                    .values()
                    .get(address.0)
                    .ok_or(Error::InvalidState)?
                    .ramp()?;
                pipeline.write_state(
                    address,
                    Value::Ramp(Ramp::constant(Fixed::from_f32(binding.initial_gain))),
                )?;
            }
            pipeline
                .write_mode_plan(binding.id, ModePlan::default())
                .map_err(Error::Transition)?;
        }
        let bytes = snapshot::capacity(0, bindings.len(), 0).ok_or(Error::InvalidShape)?;
        if ports.snapshots.capacity() < bytes {
            return Err(Error::InvalidShape);
        }
        let loops = bindings
            .into_iter()
            .map(|binding| Loop {
                gain: binding.initial_gain,
                binding,
                peak: 0.,
            })
            .collect();
        Ok(Self {
            engine: Engine {
                pipeline,
                configuration,
                loops,
            },
            ports,
            snapshot_bytes: vec![0; bytes].into_boxed_slice(),
            programs: None,
            next_program: None,
            retired_program: None,
            pending_switch: None,
        })
    }
    pub fn pipeline(&self) -> &Pipeline {
        &self.engine.pipeline
    }
    pub fn publish_initial_snapshot(&mut self) {
        let mut publication = Publication {
            loops: &mut self.engine.loops,
            configuration: self.engine.configuration,
            bytes: &mut self.snapshot_bytes,
            publisher: &mut self.ports.snapshots,
            staged: self.next_program.as_deref(),
        };
        publication.settled(self.engine.pipeline.state());
    }
    pub fn handoff_pending(&self) -> bool {
        self.pending_switch.is_some()
    }
    pub fn process(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
    ) -> Result<(), PipelineError> {
        self.process_block(
            jobs,
            memory,
            inputs,
            outputs,
            self.engine.pipeline.program().quantum as usize,
        )
    }
    pub fn process_block(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
        frames: usize,
    ) -> Result<(), PipelineError> {
        self.process_block_mixed(jobs, memory, inputs, outputs, frames, None)
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
        if let Some(error) = self.engine.pipeline.fault() {
            for output in outputs {
                output.fill(Fixed::ZERO);
            }
            return Err(error);
        }
        if let Err(error) = jobs.begin_midi_callback(events, frames) {
            for output in outputs {
                output.fill(Fixed::ZERO);
            }
            return Err(PipelineError::Jobs(Error::Midi(error)));
        }
        let mut io = crate::midi::Io {
            memory: midi,
            inputs: events,
            sink,
        };
        self.process_block_mixed(jobs, memory, inputs, outputs, frames, Some(&mut io))?;
        Ok(jobs.midi_work())
    }
    fn process_block_mixed(
        &mut self,
        jobs: &mut JobProcessor,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
        frames: usize,
        mut midi: Option<&mut crate::midi::Io<'_>>,
    ) -> Result<(), PipelineError> {
        let quantum = self.engine.pipeline.program().quantum as usize;
        if frames == 0
            || frames % quantum != 0
            || jobs.quantum() != quantum
            || memory.chunk_frames() != self.engine.pipeline.program().chunk_frames as usize
            || inputs.iter().any(|v| v.len() != frames)
            || outputs.iter().any(|v| v.len() != frames)
        {
            for output in outputs {
                output.fill(Fixed::ZERO);
            }
            return Err(PipelineError::Jobs(Error::InvalidShape));
        }
        for output in outputs.iter_mut() {
            output.fill(Fixed::ZERO);
        }
        let result = (0..frames).step_by(quantum).try_for_each(|base| {
            let fault = self.engine.pipeline.fault();
            if let Some(error) = fault {
                if self.pending_switch.is_some() {
                    return Err(error);
                }
            } else {
                self.prepare_staging(memory)?;
            }
            let frame = self.engine.pipeline.state().frame();
            let mut switched = false;
            for (lane, commands) in core::iter::once(&mut self.ports.commands)
                .chain(self.ports.switches.iter_mut())
                .enumerate()
            {
                let program_lane = lane != 0;
                if base == 0 || self.programs.is_some() {
                    commands.begin_callback();
                }
                if self
                    .pending_switch
                    .as_ref()
                    .is_some_and(|pending| pending.program_lane != program_lane)
                {
                    continue;
                }
                while let Some(group) = commands.pop() {
                    if self.pending_switch.is_some() {
                        if !memory
                            .completion_fence_ready()
                            .map_err(PipelineError::Jobs)?
                        {
                            group.leave_unread();
                            break;
                        }
                        let pending = self.pending_switch.take().unwrap();
                        assert_eq!(pending.header, group.header, "pending FIFO barrier");
                        let mut index = 0;
                        group.respond(
                            pending.header.complete(pending.frame, Code::Applied),
                            |_| {
                                let r = pending.records[index];
                                index += 1;
                                r
                            },
                        );
                        assert_eq!(index, pending.count);
                        assert!(
                            self.retired_program.is_none(),
                            "bounded reclamation storage"
                        );
                        self.retired_program = Some(pending.retired);
                        continue;
                    }
                    let header = group.header;
                    if fault.is_none()
                        && self.retired_program.is_some()
                        && group
                            .commands()
                            .any(|r| matches!(r.command(), Command::SwitchProgram(_)))
                    {
                        group.leave_unread();
                        break;
                    }
                    let configuration = self.engine.configuration;
                    let count = group.commands().count();
                    let malformed = group.commands().enumerate().any(|(i, r)| {
                        matches!(r.command(), Command::SwitchProgram(_)) && i + 1 != count
                    });
                    let code = if (header.graph(), header.topology())
                        != (configuration.graph, configuration.topology)
                    {
                        Code::StaleConfiguration
                    } else if header.deadline().is_some_and(|limit| frame > limit) {
                        Code::DeadlineMissed
                    } else if fault.is_some() || malformed {
                        Code::CannotApply
                    } else {
                        Code::Applied
                    };
                    if code == Code::Applied && count == 1 {
                        let command = group.commands().next().unwrap().command();
                        if let Command::SwitchProgram(descriptor) = command {
                            if self.next_program.as_ref().is_some_and(|stage| {
                                stage.descriptor() == descriptor
                                    && stage.ready()
                                    && self.engine.waits_for_composite_cut(stage)
                            }) {
                                group.leave_unread();
                                break;
                            }
                        }
                    }
                    let mut records = [shoop_rt_protocol::immediate::CommandRecord::new(
                        Command::BusMute(0, false),
                    );
                        shoop_rt_protocol::immediate::COMMANDS];
                    let mut retired = None;
                    for (i, record) in group.commands().enumerate() {
                        let applied = if code != Code::Applied {
                            code
                        } else if let Command::SwitchProgram(descriptor) = record.command() {
                            if let Some(mut image) = self.next_program.take() {
                                let code = if image.descriptor() == descriptor && image.cancelled {
                                    Code::Cancelled
                                } else {
                                    if let Err(error) = memory.begin_completion_fence() {
                                        self.next_program = Some(image);
                                        return Err(PipelineError::Jobs(error));
                                    }
                                    self.engine.switch(&mut image, descriptor, jobs)
                                };
                                if code == Code::Applied {
                                    retired = Some(image);
                                    switched = true;
                                } else {
                                    self.next_program = Some(image);
                                }
                                code
                            } else {
                                Code::CannotApply
                            }
                        } else {
                            self.engine.execute(record.command())
                        };
                        if applied != Code::Applied {
                            if let Command::SwitchProgram(descriptor) = record.command() {
                                if self.retired_program.is_none()
                                    && self
                                        .next_program
                                        .as_ref()
                                        .is_some_and(|image| image.descriptor() == descriptor)
                                {
                                    self.retired_program = self.next_program.take();
                                }
                            }
                        }
                        records[i] = record.complete(applied);
                    }
                    if let Some(retired) = retired {
                        self.pending_switch = Some(loading::PendingSwitch {
                            program_lane,
                            header,
                            frame,
                            records,
                            count,
                            retired,
                        });
                        if !memory
                            .completion_fence_ready()
                            .map_err(PipelineError::Jobs)?
                        {
                            group.leave_unread();
                            break;
                        }
                        assert!(
                            self.retired_program.is_none(),
                            "bounded reclamation storage"
                        );
                        self.retired_program = Some(self.pending_switch.take().unwrap().retired);
                    }
                    let mut index = 0;
                    group.respond(header.complete(frame, code), |_| {
                        let r = records[index];
                        index += 1;
                        r
                    });
                }
            }
            if let Some(error) = fault {
                return Err(error);
            }
            let mut observer = Publication {
                loops: &mut self.engine.loops,
                configuration: self.engine.configuration,
                bytes: &mut self.snapshot_bytes,
                publisher: &mut self.ports.snapshots,
                staged: self.next_program.as_deref(),
            };
            if let Some(io) = midi.as_deref_mut() {
                self.engine.pipeline.process_quantum_mixed(
                    jobs,
                    memory,
                    inputs,
                    outputs,
                    base,
                    &mut observer,
                    base == 0 || switched,
                    Some(io),
                )
            } else {
                self.engine.pipeline.process_quantum(
                    jobs,
                    memory,
                    inputs,
                    outputs,
                    base,
                    &mut observer,
                    base == 0 || switched,
                )
            }
        });
        if let Err(error) = result {
            self.engine.pipeline.latch_fault(error);
            self.ports.snapshots.fail();
            for output in outputs.iter_mut() {
                output.fill(Fixed::ZERO);
            }
        }
        result
    }
}
fn mode(mode: TransitionMode) -> Mode {
    match mode {
        TransitionMode::Stopped => Mode::Stopped,
        TransitionMode::Playing => Mode::Playing,
        TransitionMode::Recording => Mode::Recording,
    }
}
fn protocol_mode(mode: Mode) -> LoopMode {
    match mode {
        Mode::Stopped => LoopMode::Stopped,
        Mode::Playing => LoopMode::Playing,
        Mode::Recording => LoopMode::Recording,
    }
}
fn plan(plan: ModePlan) -> TransitionPlan {
    let step = |step: ModeStep| shoop_rt_protocol::immediate::PlannedTransition {
        mode: match step.mode {
            Mode::Stopped => TransitionMode::Stopped,
            Mode::Playing => TransitionMode::Playing,
            Mode::Recording => TransitionMode::Recording,
        },
        skip: step.skip,
    };
    TransitionPlan {
        now: None,
        first: plan.steps[0].map(step),
        second: plan.steps[1].map(step),
    }
}
impl Engine {
    fn execute(&mut self, command: Command) -> Code {
        if !command.valid() {
            return Code::InvalidValue;
        }
        let target = match command {
            Command::Transition(id, _)
            | Command::ClearPlannedTransitions(id, _)
            | Command::LoopGain(id, _) => id,
            Command::LoopIteration(id, _) | Command::LoopRepeatSync(id, _) => id.0.index,
            _ => return Code::Unsupported,
        };
        let Ok(index) = self
            .loops
            .binary_search_by_key(&target, |row| row.binding.public_id.slot)
        else {
            return Code::TargetUnavailable;
        };
        let binding = self.loops[index].binding;
        let result = match command {
            Command::Transition(_, value) => {
                let step = |s: shoop_rt_protocol::immediate::PlannedTransition| ModeStep {
                    mode: mode(s.mode),
                    skip: s.skip,
                };
                self.pipeline.write_mode_plan(
                    binding.id,
                    ModePlan {
                        now: value.now.map(mode),
                        steps: [value.first.map(step), value.second.map(step)],
                    },
                )
            }
            Command::ClearPlannedTransitions(_, mask) => {
                self.pipeline.clear_mode_plan(binding.id, mask as u8)
            }
            Command::LoopIteration(id, request) => (|| {
                if id.0.generation != binding.public_id.generation {
                    return Err(TransitionError::InvalidId);
                }
                let sp = self.pipeline.state();
                let slot = sp.loop_state(binding.id)?;
                let composite = sp
                    .composite_status(binding.id)
                    .ok_or(TransitionError::InvalidId)?;
                let plan = prepare_composite_iteration(
                    slot.state,
                    composite.unit_frames,
                    sp.frame(),
                    request.iteration,
                    request.retrigger,
                    request.seek,
                )?;
                self.pipeline.write_plans(binding.id, [Some(plan), None])
            })(),
            Command::LoopRepeatSync(id, enabled)
                if id.0.generation == binding.public_id.generation =>
            {
                self.pipeline.set_repeat_sync(binding.id, enabled)
            }
            Command::LoopRepeatSync(..) => Err(TransitionError::InvalidId),
            Command::LoopGain(_, gain) => {
                let Some(address) = binding.gain else {
                    return Code::Unsupported;
                };
                if self
                    .pipeline
                    .write_state(address, Value::Ramp(Ramp::constant(Fixed::from_f32(gain))))
                    .is_err()
                {
                    return Code::CannotApply;
                }
                self.loops[index].gain = gain;
                Ok(())
            }
            _ => unreachable!(),
        };
        match result {
            Err(TransitionError::InvalidId) => Code::TargetUnavailable,
            Err(TransitionError::InvalidFrame) if matches!(command, Command::LoopIteration(..)) => {
                Code::InvalidValue
            }
            Ok(())
                if matches!(command, Command::Transition(..))
                    && self
                        .pipeline
                        .state()
                        .mode_plan(binding.id)
                        .unwrap()
                        .0
                        .steps
                        .iter()
                        .any(Option::is_some) =>
            {
                Code::Scheduled
            }
            Ok(()) => Code::Applied,
            Err(TransitionError::Exhausted | TransitionError::Capacity) => Code::CaptureUnavailable,
            Err(TransitionError::Unsupported) => Code::Unsupported,
            Err(_) => Code::CannotApply,
        }
    }
}
struct Publication<'a> {
    loops: &'a mut [Loop],
    configuration: Configuration,
    bytes: &'a mut [u8],
    publisher: &'a mut shoop_rt_transport::rt::SnapshotPublisher,
    staged: Option<&'a PreparedProgram>,
}
impl Observer for Publication<'_> {
    fn settled(&mut self, state: &StateProcessor) {
        let mut position = snapshot::HEADER;
        let mut records = 0;
        let frame = state
            .frame()
            .saturating_sub(u64::from(state.quantum().unwrap_or(0)));
        for row in self.loops.iter_mut() {
            let Ok(value) = state.render_state(row.binding.id) else {
                self.publisher.fail();
                return;
            };
            let (pending, failures) = state.mode_plan(row.binding.id).unwrap();
            let pending = plan(pending);
            let id = LoopSlotId(SlotId::new(
                row.binding.public_id.slot,
                row.binding.public_id.generation,
            ));
            let next = [pending.first, pending.second]
                .into_iter()
                .flatten()
                .min_by_key(|s| s.skip);
            let armed = [pending.first, pending.second]
                .into_iter()
                .flatten()
                .any(|s| s.mode == TransitionMode::Recording);
            let mut timing = row.binding.timing;
            timing.content_revision = value.content_revision;
            timing.length = value.content_length;
            timing.raw_audio_frames = value.content_length;
            timing.raw_midi_frames = value.content_length;
            let peak = row.peak;
            row.peak = 0.;
            let values = [
                Record::LoopPlan {
                    id,
                    plan: pending,
                    failures,
                },
                Record::LoopLevels {
                    id,
                    gain: row.gain,
                    balance: 0.,
                    repeat_sync: value.repeat_sync,
                },
                Record::State(StateRecord::LoopTiming {
                    loop_id: id,
                    timing,
                }),
                Record::Telemetry(Telemetry::Loop {
                    loop_id: id,
                    mode: protocol_mode(value.mode),
                    next_mode: next.map(|s| protocol_mode(mode(s.mode))),
                    next_delay: next.map(|s| s.skip),
                    content_revision: value.content_revision,
                    recording_armed: armed,
                    capture_incomplete: false,
                    timing_revision: timing.revision,
                    length: if value.mode == Mode::Recording {
                        value.capture_frames
                    } else {
                        value.content_length
                    },
                    position: value.position,
                    peaks: if row.binding.meter.is_some() {
                        Peaks::new(&[peak]).unwrap()
                    } else {
                        Peaks::default()
                    },
                }),
            ];
            for record in values {
                let Ok(len) = codec::encode_snapshot(&record, &mut self.bytes[position..]) else {
                    self.publisher.fail();
                    return;
                };
                position += len;
                records += 1;
            }
            if let Some(composite) = state.composite_status(row.binding.id) {
                let pending = self.staged.and_then(|staged| {
                    if staged.cancelled {
                        return None;
                    }
                    let id = row.binding.public_id;
                    staged
                        .image
                        .layout
                        .composite_schedule(rt_fpga_compiler::Identity {
                            slot: id.slot,
                            generation: id.generation,
                        })
                        .filter(|plan| plan.version != composite.version)
                });
                let record = Record::Telemetry(Telemetry::Composite {
                    loop_id: id,
                    frame,
                    mode: protocol_mode(value.mode),
                    position: value.position,
                    length: value.content_length,
                    active_version: composite.version,
                    pending_version: pending.map(|plan| plan.version),
                    pending_admitted: pending.is_some()
                        && self.staged.is_some_and(PreparedProgram::ready),
                    iteration: composite.iteration,
                    passes: composite.passes,
                    waiting_repeat: composite.waiting_repeat,
                });
                let Ok(len) = codec::encode_snapshot(&record, &mut self.bytes[position..]) else {
                    self.publisher.fail();
                    return;
                };
                position += len;
                records += 1;
            }
        }
        snapshot::Header {
            frame,
            graph: self.configuration.graph,
            topology: self.configuration.topology,
            records,
            loops: self.loops.len() as u32,
            strips: 0,
            buses: 0,
        }
        .write(self.bytes);
        let _ = self.publisher.publish(&self.bytes[..position]);
    }
    fn processed(&mut self, jobs: &JobProcessor) {
        for row in self.loops.iter_mut() {
            if let Some(buffer) = row.binding.meter.and_then(|id| jobs.buffer(id).ok()) {
                for sample in buffer {
                    row.peak = row.peak.max(sample.to_f32().abs());
                }
            }
        }
    }
}
