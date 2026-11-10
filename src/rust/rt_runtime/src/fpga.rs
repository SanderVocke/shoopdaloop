//! Host bridge for the FPGA reference. Compilation owns graph/port knowledge;
//! the interpreters receive only instructions, values, and ordered jobs.

use crate::{
    audio_refs::GainEnvelope,
    graph::NodeSpec,
    planner::Planner,
    resource::{CaptureTickId, ReadIndex, ResourceStore},
    Error, Frame, NodeId, PreparedConfig, Quantum,
};
use alloc::{boxed::Box, vec, vec::Vec};
use rt_fpga_reference::{
    compiler::Program,
    jobs::{JobProcessor, JobQueue, Memory},
    state::{Address, Instruction, JobTemplate, Register, StateProcessor, Value},
    storage::ViewId,
};
pub use rt_fpga_reference::{
    fixed::{Fixed, Ramp},
    jobs::{BufferId, Job},
};

#[derive(Clone, Copy)]
enum View {
    Read {
        index: ReadIndex,
        channel: usize,
        channels: usize,
    },
    Capture {
        interval: CaptureTickId,
        channel: usize,
        channels: usize,
    },
}

pub struct Compiler {
    program: Program,
    values: Vec<Value>,
    views: Vec<View>,
    buffers: usize,
    capacity: usize,
}

impl Compiler {
    fn new(quantum: Quantum, capacity: usize) -> Self {
        Self {
            program: Program {
                instructions: Vec::with_capacity(capacity * 5),
                quantum: quantum.frames() as u32,
                jobs_per_quantum: 0,
            },
            values: Vec::with_capacity(capacity),
            views: Vec::with_capacity(capacity),
            buffers: 0,
            capacity,
        }
    }
    fn reset(&mut self) {
        self.program.instructions.clear();
        self.program.jobs_per_quantum = 0;
        self.values.clear();
        self.views.clear();
        self.buffers = 0;
    }
    pub fn emit(&mut self, job: Job) -> Result<(), Error> {
        if self.program.jobs_per_quantum == self.capacity {
            return Err(Error::Capacity);
        }
        self.program
            .instructions
            .push(Instruction::Emit(JobTemplate::Literal(job)));
        self.program.jobs_per_quantum += 1;
        Ok(())
    }
    fn buffer(&mut self) -> Result<BufferId, Error> {
        if self.buffers == self.capacity {
            return Err(Error::Capacity);
        }
        let id = BufferId(self.buffers);
        self.buffers += 1;
        Ok(id)
    }
    pub fn silence(&mut self) -> Result<BufferId, Error> {
        let destination = self.buffer()?;
        self.emit(Job::Silence { destination })?;
        Ok(destination)
    }
    pub fn input(&mut self, port: usize) -> Result<BufferId, Error> {
        let destination = self.buffer()?;
        self.emit(Job::Input { port, destination })?;
        Ok(destination)
    }
    pub fn mix(&mut self, left: BufferId, right: BufferId) -> Result<BufferId, Error> {
        let destination = self.buffer()?;
        self.emit(Job::Mix {
            left,
            right,
            destination,
        })?;
        Ok(destination)
    }
    pub fn gain(&mut self, source: BufferId, gain: GainEnvelope) -> Result<BufferId, Error> {
        let destination = self.buffer()?;
        if self.values.len() == self.capacity || self.program.jobs_per_quantum == self.capacity {
            return Err(Error::Capacity);
        }
        let address = Address(self.values.len());
        self.values.push(Value::Ramp(Ramp::new(
            Fixed::from_f32(gain.start),
            Fixed::from_f32(gain.end),
            self.program.quantum - 1,
        )));
        self.program.instructions.extend_from_slice(&[
            Instruction::Load {
                address,
                destination: Register(0),
            },
            Instruction::Emit(JobTemplate::Gain {
                ramp: Register(0),
                source,
                destination,
            }),
            Instruction::AdvanceRamp { ramp: Register(0) },
            Instruction::Store {
                source: Register(0),
                address,
            },
        ]);
        self.program.jobs_per_quantum += 1;
        Ok(destination)
    }
    pub fn read(
        &mut self,
        index: ReadIndex,
        channel: usize,
        channels: usize,
        position: u64,
        length: u64,
    ) -> Result<BufferId, Error> {
        let view = self.view(View::Read {
            index,
            channel,
            channels,
        })?;
        let destination = self.buffer()?;
        self.emit(Job::ReadView {
            view,
            position,
            length,
            destination,
        })?;
        Ok(destination)
    }
    pub fn capture(&mut self, interval: CaptureTickId, sources: &[BufferId]) -> Result<(), Error> {
        for (channel, &source) in sources.iter().enumerate() {
            let view = self.view(View::Capture {
                interval,
                channel,
                channels: sources.len(),
            })?;
            self.emit(Job::Capture { source, view })?;
        }
        Ok(())
    }
    fn view(&mut self, view: View) -> Result<ViewId, Error> {
        if self.views.len() == self.capacity {
            return Err(Error::Capacity);
        }
        let id = ViewId(self.views.len());
        self.views.push(view);
        Ok(id)
    }
}

/// App parameters and loop decisions enter here as metadata. There is no sample access.
pub struct CompileBlock<'a, L: Planner> {
    pub node: NodeId,
    pub frame: Frame,
    pub graph_epoch: crate::Epoch,
    pub quantum: Quantum,
    pub inputs: &'a [BufferId],
    pub outputs: &'a mut [BufferId],
    pub compiler: &'a mut Compiler,
    pub domain: &'a mut L,
}

pub(crate) struct Machine {
    pub(crate) compiler: Compiler,
    pub(crate) state: StateProcessor,
    pub(crate) jobs: JobProcessor,
    pub(crate) queue: JobQueue,
    pub(crate) inputs: Box<[Box<[BufferId]>]>,
    pub(crate) outputs: Box<[Box<[BufferId]>]>,
    pub(crate) host_inputs: Box<[Box<[Fixed]>]>,
    pub(crate) host_outputs: Box<[Box<[Fixed]>]>,
    pub(crate) host_ids: Box<[Option<BufferId>]>,
    staging: Box<[f32]>,
}
impl Machine {
    pub(crate) fn prepare(
        config: PreparedConfig,
        quantum: Quantum,
        specs: &[NodeSpec],
    ) -> Result<Self, Error> {
        let ports: usize = specs
            .iter()
            .map(|spec| spec.ports.audio_inputs + spec.ports.audio_outputs)
            .sum();
        let limits = config.limits();
        if limits.audio_inputs > 1024 || limits.audio_outputs > 1024 {
            return Err(Error::Capacity);
        }
        let capacity = ports
            .checked_mul(32)
            .and_then(|n| {
                n.checked_add(limits.max_edges * 2 + limits.audio_inputs + limits.audio_outputs + 1)
            })
            .ok_or(Error::Capacity)?;
        let mappings = |input: bool| {
            let mut result: Vec<Box<[BufferId]>> =
                (0..limits.max_nodes).map(|_| Box::default()).collect();
            for spec in specs {
                let count = if input {
                    spec.ports.audio_inputs
                } else {
                    spec.ports.audio_outputs
                };
                result[spec.id.slot() as usize] = vec![BufferId(0); count].into_boxed_slice();
            }
            result.into_boxed_slice()
        };
        let buffers = |count| {
            (0..count)
                .map(|_| vec![Fixed::ZERO; quantum.frames()].into_boxed_slice())
                .collect()
        };
        let channels = specs
            .iter()
            .map(|spec| spec.ports.audio_inputs.max(spec.ports.audio_outputs))
            .max()
            .unwrap_or(1);
        Ok(Self {
            compiler: Compiler::new(quantum, capacity),
            state: StateProcessor::prepare(capacity),
            jobs: JobProcessor::new(quantum.frames(), capacity),
            queue: JobQueue::new(capacity),
            inputs: mappings(true),
            outputs: mappings(false),
            host_inputs: buffers(limits.audio_inputs),
            host_outputs: buffers(limits.audio_outputs),
            host_ids: vec![None; limits.audio_inputs].into_boxed_slice(),
            staging: vec![0.; channels * quantum.frames()].into_boxed_slice(),
        })
    }
    pub(crate) fn reset(&mut self) {
        self.compiler.reset();
        self.host_ids.fill(None);
    }
    pub(crate) fn execute(&mut self, resources: &mut ResourceStore) -> Result<(), Error> {
        self.state.reset(&self.compiler.values).map_err(map_error)?;
        self.state
            .run(&self.compiler.program, &mut self.queue)
            .map_err(map_error)?;
        let mut memory = ExternalMemory {
            resources,
            views: &self.compiler.views,
            staging: &mut self.staging,
        };
        self.jobs.begin();
        // Port limits are enforced by runtime preparation.
        let mut inputs: [&[Fixed]; 1024] = [&[]; 1024];
        for (input, samples) in inputs.iter_mut().zip(&self.host_inputs) {
            *input = samples;
        }
        let output_count = self.host_outputs.len();
        let mut outputs: [&mut [Fixed]; 1024] = core::array::from_fn(|_| &mut [] as &mut [Fixed]);
        for (output, samples) in outputs.iter_mut().zip(&mut self.host_outputs) {
            samples.fill(Fixed::ZERO);
            *output = samples;
        }
        while let Some(job) = self.queue.pop() {
            self.jobs
                .execute(
                    job,
                    &mut memory,
                    &inputs[..self.host_inputs.len()],
                    &mut outputs[..output_count],
                )
                .map_err(map_error)?;
        }
        Ok(())
    }
}
fn map_error(error: rt_fpga_reference::Error) -> Error {
    match error {
        rt_fpga_reference::Error::QueueFull => Error::Capacity,
        _ => Error::InvalidState,
    }
}
struct ExternalMemory<'a> {
    resources: &'a mut ResourceStore,
    views: &'a [View],
    staging: &'a mut [f32],
}
impl Memory for ExternalMemory<'_> {
    fn read(
        &mut self,
        view: ViewId,
        position: u64,
        length: u64,
        output: &mut [Fixed],
    ) -> Result<(), rt_fpga_reference::Error> {
        let fail = || rt_fpga_reference::Error::InvalidView;
        let Some(View::Read {
            index,
            channel,
            channels,
        }) = self.views.get(view.0).copied()
        else {
            return Err(fail());
        };
        if length == 0 || position >= length || channel >= channels {
            return Err(fail());
        }
        let mut io = self.resources.render_io();
        let snapshot = io.snapshot(index).map_err(|_| fail())?;
        if snapshot.description.frames < length {
            return Err(fail());
        }
        let mut frame = position;
        for sample in output {
            let offset = usize::try_from(frame)
                .ok()
                .and_then(|n| n.checked_mul(channels))
                .and_then(|n| n.checked_add(channel))
                .and_then(|n| n.checked_mul(4))
                .ok_or_else(|| fail())?;
            let mut cursor = io.cursor_at(index, offset).map_err(|_| fail())?;
            let mut bytes = [0; 4];
            let mut copied = 0;
            while copied < 4 {
                let count = io
                    .read(&mut cursor, &mut bytes[copied..])
                    .map_err(|_| fail())?;
                if count == 0 {
                    return Err(fail());
                }
                copied += count;
            }
            *sample = Fixed::from_f32(f32::from_le_bytes(bytes));
            frame = if frame + 1 == length { 0 } else { frame + 1 };
        }
        Ok(())
    }
    fn write(&mut self, view: ViewId, samples: &[Fixed]) -> Result<(), rt_fpga_reference::Error> {
        let fail = || rt_fpga_reference::Error::InvalidView;
        let Some(View::Capture {
            interval,
            channel,
            channels,
        }) = self.views.get(view.0).copied()
        else {
            return Err(fail());
        };
        if samples.len() * channels > self.staging.len() {
            return Err(fail());
        }
        for (frame, sample) in samples.iter().enumerate() {
            self.staging[frame * channels + channel] = sample.to_f32();
        }
        if channel + 1 == channels {
            self.resources
                .render_io()
                .stage_audio(interval, 0, &self.staging[..samples.len() * channels])
                .map_err(|_| fail())?;
        }
        Ok(())
    }
}
