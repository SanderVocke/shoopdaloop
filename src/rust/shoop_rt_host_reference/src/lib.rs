//! Portable ARM-side graph binding and driver-owned persistent reference rendering.
#![no_std]
extern crate alloc;

#[cfg(feature = "rtl")]
pub mod fpga;
mod usage;

use alloc::{sync::Arc, vec, vec::Vec};
use anyhow::{ensure, Result};
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use rt_fpga_compiler::{
    compile_composites, compile_with_chunk_frames, loading::Allocator, CompileContext,
    CompileError, Feature, ProgramLimits, Resource,
};
use rt_fpga_reference::{
    fixed::Fixed,
    jobs::{BufferId, JobProcessor},
    pipeline::Pipeline,
    rt_interface::{program_memory, Configuration, PreparedProgram, Processor, ProgramControl},
    state::{Instruction, Program},
    storage::{MediaStore, ViewId},
    target::RustTarget,
    transitions::{PlanDescription, TransitionLimits, TransitionPlan},
};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    host::{Phase, Renderer, Report},
    host_adapter::QuantumAdapter,
    Error, Frame,
};
use shoop_rt_protocol::{
    immediate::{Batch, Code, Command, CommandRecord, Header, Response, SwitchProgram},
    Capabilities, ClientMessage, DriverKind, DriverRunState, DriverStatus, EngineMessage, Lane,
    Limits, MemoryReport, MemoryResource, MemoryUnit,
};
use shoop_rt_transport::{
    carrier::{CarrierError, Endpoint as CarrierEndpoint, Outbox},
    rt,
};
use usage::{ProgramUsage, StorageCounter, UsageTelemetry};

pub use usage::StorageUsage;

const BOOTSTRAP_INSTRUCTIONS: [Instruction; 3] = [
    Instruction::ProcessSilence {
        destination: BufferId(0),
    },
    Instruction::ProcessOutput {
        source: BufferId(0),
        port: 0,
    },
    Instruction::ProcessOutput {
        source: BufferId(0),
        port: 1,
    },
];
const BOOTSTRAP_USAGE: ProgramUsage = ProgramUsage {
    instructions: BOOTSTRAP_INSTRUCTIONS.len() as u32,
    jobs: 3,
    buffers: 1,
};

// Keep per-view block tables affordable at normal desktop and browser media sizes.
pub const MEDIA_BLOCK_CHUNKS: usize = 1024;

pub fn program_limits() -> ProgramLimits {
    ProgramLimits {
        instructions: 1024,
        state_values: 512,
        loop_slots: 128,
        jobs: 1024,
        buffers: 512,
        history_slots: 512,
        input_ports: 1,
        output_ports: 2,
        quantum: 128,
    }
}
#[derive(Debug)]
struct GraphRefusal(Code);
impl core::fmt::Display for GraphRefusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "graph refused: {:?}", self.0)
    }
}
impl core::error::Error for GraphRefusal {}
fn refusal_code(error: &anyhow::Error) -> Code {
    if let Some(GraphRefusal(code)) = error.downcast_ref::<GraphRefusal>() {
        return *code;
    }
    let compile = error.downcast_ref::<CompileError>().copied().or_else(|| {
        match error.downcast_ref::<rt_fpga_compiler::loading::LoadError>() {
            Some(rt_fpga_compiler::loading::LoadError::Compile(e)) => Some(*e),
            _ => None,
        }
    });
    match compile {
        Some(CompileError::Unsupported { feature, .. }) => match feature {
            Feature::Midi => Code::GraphMidiUnsupported,
            Feature::Channels => Code::GraphChannelsUnsupported,
            Feature::Processor => Code::GraphProcessorUnsupported,
            Feature::Composite => Code::GraphCompositeUnsupported,
            Feature::StripControls => Code::GraphStripControlsUnsupported,
            Feature::LoopControls => Code::GraphLoopControlsUnsupported,
            Feature::BusControls => Code::GraphBusControlsUnsupported,
            Feature::Buses => Code::GraphBusesUnsupported,
            Feature::Routing => Code::GraphRoutingUnsupported,
        },
        Some(CompileError::Capacity(resource)) => match resource {
            Resource::Instructions => Code::InstructionCapacity,
            Resource::State => Code::StateCapacity,
            Resource::Loops => Code::LoopCapacity,
            Resource::Jobs => Code::JobCapacity,
            Resource::Buffers => Code::BufferCapacity,
            Resource::History => Code::HistoryCapacity,
            Resource::Inputs => Code::InputCapacity,
            Resource::Outputs => Code::OutputCapacity,
            Resource::Quantum => Code::QuantumCapacity,
        },
        _ => Code::CannotApply,
    }
}
struct Pending {
    batch: Batch,
    descriptor: SwitchProgram,
    ready: Option<bool>,
    published: bool,
    cancel_key: Option<u64>,
    cancel_requested: bool,
    notify: bool,
    usage: ProgramUsage,
}
struct ImageUpload {
    graph: u64,
    quantum: u32,
    expected: usize,
    bytes: Vec<u8>,
}
struct Upload {
    graph: u64,
    quantum: u32,
    expected: u32,
    loops: Vec<shoop_rt_protocol::immediate::GraphLoop>,
    failure: Option<Code>,
}
pub struct Endpoint<L = ProgramControl> {
    control: rt::Endpoint,
    programs: L,
    allocator: Allocator,
    pending: Option<Pending>,
    upload: Option<Upload>,
    image_upload: Option<ImageUpload>,
    limits: ProgramLimits,
    replies: Outbox,
    held: Option<EngineMessage>,
    quantum: u32,
    status: Arc<Status>,
    failed: bool,
    sent_frame: u64,
    program_usage: ProgramUsage,
    storage: [Option<MemoryResource>; 2],
    usage: UsageTelemetry,
}

#[derive(Debug)]
pub struct Status {
    kind: DriverKind,
    sample_rate: u32,
    buffer: AtomicU32,
    state: AtomicU8,
    frame: AtomicU64,
    callbacks: AtomicU64,
    xruns: AtomicU32,
    dsp: shoop_rt_protocol::dsp::Peak,
    storage: StorageCounter,
}
impl Status {
    pub fn new(driver: DriverStatus) -> Self {
        Self {
            kind: driver.kind,
            sample_rate: driver.sample_rate,
            buffer: AtomicU32::new(driver.buffer_frames),
            state: AtomicU8::new(match driver.state {
                DriverRunState::Starting => 0,
                DriverRunState::Running => 1,
                DriverRunState::Stopped => 2,
                DriverRunState::Failed => 3,
            }),
            frame: AtomicU64::new(0),
            callbacks: AtomicU64::new(0),
            xruns: AtomicU32::new(0),
            dsp: Default::default(),
            storage: StorageCounter::default(),
        }
    }
    pub fn update_driver(&self, driver: DriverStatus) {
        self.buffer.store(driver.buffer_frames, Ordering::Relaxed);
        match driver.state {
            DriverRunState::Starting => {}
            DriverRunState::Running => {
                let _ = self
                    .state
                    .compare_exchange(0, 1, Ordering::Release, Ordering::Relaxed);
            }
            DriverRunState::Stopped => self.state.store(2, Ordering::Release),
            DriverRunState::Failed => self.state.store(3, Ordering::Release),
        }
    }
    pub fn driver(&self) -> DriverStatus {
        DriverStatus {
            kind: self.kind,
            sample_rate: self.sample_rate,
            buffer_frames: self.buffer.load(Ordering::Relaxed),
            state: match self.state.load(Ordering::Acquire) {
                0 => DriverRunState::Starting,
                1 => DriverRunState::Running,
                2 => DriverRunState::Stopped,
                _ => DriverRunState::Failed,
            },
        }
    }
    pub fn set_frame(&self, frame: u64) {
        self.frame.store(frame, Ordering::Release);
    }
    pub fn publish_storage(&self, usage: StorageUsage) {
        self.storage.publish(usage);
    }
    pub fn usage_interval(&self) -> u64 {
        u64::from((self.sample_rate / 5).max(1))
    }
    pub fn xrun(&self) {
        self.xruns.fetch_add(1, Ordering::Relaxed);
    }
    #[cfg(feature = "rtl")]
    pub(crate) fn observe_fpga(
        &self,
        quanta: u32,
        xruns: u32,
        dsp: shoop_rt_protocol::DspMeasurement,
    ) {
        self.callbacks.store(quanta.into(), Ordering::Relaxed);
        self.xruns.store(xruns, Ordering::Relaxed);
        self.record_dsp(dsp);
    }
    pub fn callback_complete(&self) {
        self.callbacks.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_dsp(&self, sample: shoop_rt_protocol::DspMeasurement) {
        self.dsp.record(sample);
    }
    pub fn observe_worklet_engine(
        &self,
        driver: DriverStatus,
        callbacks: u64,
        xruns: u32,
        dsp: Option<shoop_rt_protocol::DspMeasurement>,
    ) {
        self.update_driver(driver);
        self.callbacks.store(callbacks, Ordering::Relaxed);
        self.xruns.store(xruns, Ordering::Relaxed);
        if let Some(sample) = dsp {
            self.record_dsp(sample);
        }
    }
    fn publish_engine(&self, frame: u64, sent_frame: &mut u64, out: &mut Outbox) {
        if frame.saturating_sub(*sent_frame) >= u64::from((self.sample_rate / 30).max(1)) {
            *sent_frame = frame;
            let _ = out.push(EngineMessage::Telemetry(self.telemetry()));
        }
    }
    pub fn telemetry(&self) -> shoop_rt_protocol::Telemetry {
        shoop_rt_protocol::Telemetry::Engine {
            frame: self.frame.load(Ordering::Acquire),
            callbacks: self.callbacks.load(Ordering::Relaxed),
            xruns: self.xruns.load(Ordering::Relaxed),
            dsp: self.dsp.take(),
            input_peak: 0.,
            output_peak: 0.,
            driver: self.driver(),
        }
    }
}
/// ARM-side staging. Implementations lower the same logical graph into their own target.
pub trait ProgramLoader {
    fn limits(&self) -> ProgramLimits;
    fn chunk_frames(&self) -> u32 {
        1
    }
    fn stage_program(
        &mut self,
        logical: &rt_fpga_compiler::LogicalStateProgram,
        layout: rt_fpga_compiler::loading::Layout,
    ) -> Result<SwitchProgram>;
    fn cancel_program(&mut self, revision: u64) -> Result<(), CarrierError>;
    fn program_ready(&mut self) -> Option<(u64, bool)>;
    fn reclaim_programs(&mut self);
}
impl ProgramLoader for ProgramControl {
    fn limits(&self) -> ProgramLimits {
        program_limits()
    }
    fn stage_program(
        &mut self,
        logical: &rt_fpga_compiler::LogicalStateProgram,
        layout: rt_fpga_compiler::loading::Layout,
    ) -> Result<SwitchProgram> {
        let views = layout.media_views(logical);
        let unused = layout
            .new_media_views()
            .map(|v| ViewId(v as usize))
            .collect();
        let mut target = RustTarget::new(&views);
        target.limits = self.limits();
        let image = PreparedProgram::new(target.build_image(logical, layout)?, unused)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let descriptor = image.descriptor();
        self.stage(image)
            .map_err(|_| anyhow::anyhow!("program memory handoff is full"))?;
        Ok(descriptor)
    }
    fn program_ready(&mut self) -> Option<(u64, bool)> {
        self.ready().map(|r| (r.revision, r.result.is_ok()))
    }
    fn cancel_program(&mut self, revision: u64) -> Result<(), CarrierError> {
        self.cancel(revision)
            .then_some(())
            .ok_or(CarrierError::Full)
    }
    fn reclaim_programs(&mut self) {
        while self.reclaim().is_some() {}
    }
}
pub fn prepare_control(
    quantum: u32,
    driver: DriverStatus,
    latency: u32,
) -> Result<(rt::Endpoint, rt::Execution)> {
    let pool = program_limits();
    let (control, execution) = rt::prepared_channel(
        256,
        4,
        shoop_rt_protocol::snapshot::capacity(0, pool.loop_slots as usize, 1).unwrap(),
    )?;
    let limits = Limits {
        composite_entries: pool.instructions,
        composite_segments: pool.instructions,
        timing_relations: pool.instructions,
        content_chunk_bytes: 0,
        strips: 0,
        loops: pool.loop_slots,
        strip_audio_channels: 1,
        master_channels: 2,
        buses: 1,
        bus_channels: 2,
        host_audio_inputs: 1,
        host_audio_outputs: 2,
        host_midi_inputs: 0,
        host_midi_outputs: 0,
        max_frames: quantum,
        quantum,
        processing_latency_frames: latency,
        smoothing_limit_frames: 0,
        reliable_credits: 0,
        bulk_credits: 0,
    };
    let control = rt::Endpoint::new(
        control,
        rt::Settings {
            graph: 0,
            topology: 0,
            limits,
            driver,
        },
    )?
    .with_capabilities(
        Capabilities::PREPARED_GRAPH
            .union(Capabilities::LIVE_PROGRAMS)
            .union(Capabilities::CANCEL_PREPARED_COMPOSITES)
            .union(Capabilities::PREPARED_AUTOMATIC_MONITORING),
    );
    Ok((control, execution))
}
pub struct Prepared {
    pub endpoint: Endpoint,
    pub renderer: QuantumAdapter<ReferenceRenderer>,
    pub status: Arc<Status>,
}
pub fn prepare(quantum: u32, driver: DriverStatus, memory_mb: u32) -> Result<Prepared> {
    ensure!(
        matches!(quantum, 1 | 16 | 32 | 64 | 128) && driver.sample_rate > 0,
        "invalid reference driver geometry"
    );
    let pool = program_limits();
    let (control, execution) = prepare_control(quantum, driver, quantum)?;
    let plan = TransitionPlan::compile(
        PlanDescription {
            quantum,
            loops: vec![],
        },
        TransitionLimits {
            loop_slots: pool.loop_slots,
            events: 4 * pool.loop_slots,
        },
    )
    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let program = Program {
        quantum,
        chunk_frames: 1,
        jobs_per_quantum: BOOTSTRAP_USAGE.jobs as usize,
        sync_instructions: vec![],
        instructions: BOOTSTRAP_INSTRUCTIONS.into(),
    };
    let pipeline = Pipeline::prepare(plan, program, vec![], pool.jobs as usize)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let (programs, program_execution) = program_memory();
    let mut processor = Processor::prepare(
        pipeline,
        execution,
        Configuration {
            graph: 0,
            topology: 0,
            frame: 0,
        },
        vec![],
    )
    .and_then(|p| p.with_program_memory(program_execution, pool))
    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let chunks = (memory_mb as usize)
        .checked_mul(1024 * 1024 / 4)
        .ok_or_else(|| anyhow::anyhow!("reference media capacity overflow"))?;
    let mut media = MediaStore::with_block_chunk_limit(1, MEDIA_BLOCK_CHUNKS, chunks)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    media
        .create_recordings(pool.state_values as usize)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    media
        .reserve_chunk_capacity(chunks)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    for view in 0..pool.state_values {
        media
            .reserve_view_capacity(ViewId(view as usize), chunks)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    let jobs = JobProcessor::with_history_slots(
        quantum as usize,
        pool.buffers as usize,
        pool.history_slots as usize,
    );

    processor.publish_initial_snapshot();
    let status = Arc::new(Status::new(driver));
    if let Some(audio_blocks) = media.block_usage() {
        status.publish_storage(StorageUsage {
            audio_blocks,
            metadata_pages: None,
        });
    }
    let endpoint =
        Endpoint::new(control, programs, quantum, status.clone()).with_reference_bootstrap();

    let renderer = ReferenceRenderer {
        processor,
        jobs,
        media,
        reclaim: 0,
        slots: pool.state_values as usize,
        input: vec![Fixed::ZERO; quantum as usize],
        left: vec![Fixed::ZERO; quantum as usize],
        right: vec![Fixed::ZERO; quantum as usize],
        status: status.clone(),
        storage_frame: 0,
    };
    Ok(Prepared {
        endpoint,
        renderer: QuantumAdapter::new(renderer, quantum as usize, 1, 2)?,
        status,
    })
}
pub struct ReferenceRenderer {
    processor: Processor,
    jobs: JobProcessor,
    media: MediaStore,
    reclaim: usize,
    slots: usize,
    input: Vec<Fixed>,
    left: Vec<Fixed>,
    right: Vec<Fixed>,
    status: Arc<Status>,
    storage_frame: u64,
}
impl Renderer for ReferenceRenderer {
    fn next_frame(&self) -> Frame {
        Frame(self.processor.pipeline().state().frame())
    }
    fn process(&mut self, audio: AudioBlock<'_, '_>, midi: MidiBlock<'_>) -> Result<Report, Error> {
        if audio.frames != self.input.len() {
            return Err(Error::InvalidShape);
        }
        for (index, v) in self.input.iter_mut().enumerate() {
            *v = Fixed::from_f32(audio.inputs.first().map_or(0., |p| p[index]));
        }
        self.processor
            .process(
                &mut self.jobs,
                &mut self.media,
                &[&self.input],
                &mut [&mut self.left, &mut self.right],
            )
            .map_err(|_| Error::InvalidState)?;
        if !self.processor.handoff_pending()
            && !self
                .processor
                .pipeline()
                .uses_media_view(ViewId(self.reclaim))
        {
            rt_fpga_reference::jobs::Memory::prepare_unused_view(
                &mut self.media,
                ViewId(self.reclaim),
            )
            .map_err(|_| Error::InvalidState)?;
        }
        self.reclaim = (self.reclaim + 1) % self.slots;
        for (port, out) in audio.outputs.iter_mut().enumerate() {
            for (index, value) in out[..audio.frames].iter_mut().enumerate() {
                *value = match port {
                    0 => self.left[index].to_f32(),
                    1 => self.right[index].to_f32(),
                    _ => 0.,
                };
            }
        }
        for out in midi.outputs {
            out.clear();
        }
        let next_frame = self.next_frame();
        if next_frame.0.saturating_sub(self.storage_frame) >= self.status.usage_interval() {
            self.storage_frame = next_frame.0;
            if let Some(audio_blocks) = self.media.block_usage() {
                self.status.publish_storage(StorageUsage {
                    audio_blocks,
                    metadata_pages: None,
                });
            }
        }
        self.status.set_frame(next_frame.0);
        Ok(Report {
            next_frame,
            phase: Phase::Running,
            recovery_packets: 0,
        })
    }
}
impl<L: ProgramLoader> Endpoint<L> {
    pub fn into_control(self) -> (StorageEndpoint, L) {
        (
            StorageEndpoint {
                control: self.control,
                status: self.status,
                sent_frame: self.sent_frame,
                usage: UsageTelemetry::default(),
            },
            self.programs,
        )
    }
    pub fn new(control: rt::Endpoint, programs: L, quantum: u32, status: Arc<Status>) -> Self {
        let limits = programs.limits();
        Self {
            control,
            programs,
            allocator: Allocator::new(limits),
            pending: None,
            upload: None,
            image_upload: None,
            limits,
            replies: Outbox::new(8, 8, 0),
            held: None,
            quantum,
            status,
            failed: false,
            sent_frame: 0,
            program_usage: ProgramUsage::default(),
            storage: [None; 2],
            usage: UsageTelemetry::default(),
        }
    }
    pub fn with_reference_bootstrap(mut self) -> Self {
        self.program_usage = BOOTSTRAP_USAGE;
        self
    }
    pub fn observe_worklet_storage(&mut self, report: &MemoryReport) {
        self.storage[0] = report.resources.iter().flatten().copied().find(|resource| {
            resource.name.as_str() == usage::AUDIO_STORAGE && resource.unit == MemoryUnit::Blocks
        });
    }
    fn publish_usage(&mut self, out: &mut Outbox) {
        if !self.control.is_connected() {
            return;
        }
        let frame = self.status.frame.load(Ordering::Acquire);
        if let Some(storage) = self.status.storage.report(frame) {
            self.storage.copy_from_slice(&storage.resources[..2]);
        }
        let mut report = MemoryReport {
            frame,
            ..Default::default()
        };
        report.resources[..2].copy_from_slice(&self.storage);
        usage::add_program_usage(
            &mut report,
            &self.allocator,
            self.limits,
            self.program_usage,
            self.pending.as_ref().map(|pending| pending.usage),
        );
        self.usage
            .publish(report, self.status.usage_interval(), out);
    }
    fn accept_graph(&mut self, batch: Batch) -> Result<bool> {
        ensure!(self.control.is_connected(), "session is not connected");
        ensure!(self.pending.is_none(), "a program is already pending");
        let revision = self.allocator.revision();
        ensure!(
            batch.graph == revision && batch.topology == revision && batch.deadline.is_none(),
            "stale graph preparation"
        );
        for (index, command) in batch.commands().iter().enumerate() {
            match *command {
                Command::BeginGraphImage { quantum, bytes } if index == 0 => {
                    self.upload = None;
                    self.image_upload = None;
                    ensure!(
                        quantum == self.quantum
                            && bytes > 0
                            && bytes as usize <= shoop_graph::MAX_TRANSFER_BYTES,
                        "invalid graph transfer header"
                    );
                    self.image_upload = Some(ImageUpload {
                        graph: revision,
                        quantum,
                        expected: bytes as usize,
                        bytes: Vec::new(),
                    });
                }
                Command::GraphImageData { len, bytes } => {
                    let upload = self
                        .image_upload
                        .as_mut()
                        .ok_or_else(|| anyhow::anyhow!("missing graph image header"))?;
                    ensure!(
                        command.valid()
                            && upload.graph == revision
                            && upload.bytes.len() + len as usize <= upload.expected,
                        "invalid graph transfer data"
                    );
                    upload.bytes.extend_from_slice(&bytes[..len as usize]);
                }
                Command::BeginGraph { quantum, loops } if index == 0 => {
                    self.image_upload = None;
                    ensure!(quantum == self.quantum && loops > 0, "invalid graph header");
                    self.upload = Some(Upload {
                        graph: revision,
                        quantum,
                        expected: loops,
                        loops: Vec::new(),
                        // The physical loop pool also bounds ARM-side assembly memory.
                        failure: (loops > self.limits.loop_slots).then_some(Code::LoopCapacity),
                    });
                }
                Command::ConfigureLoop(l) => {
                    let upload = self
                        .upload
                        .as_mut()
                        .ok_or_else(|| anyhow::anyhow!("missing graph header"))?;
                    ensure!(upload.graph == revision, "stale graph upload");
                    if upload.failure.is_none() {
                        if !command.valid() || upload.loops.len() >= upload.expected as usize {
                            upload.failure = Some(Code::InvalidValue);
                        } else {
                            upload.loops.push(l);
                        }
                    }
                }
                Command::ActivateGraph | Command::ActivateCompositeGraph(_)
                    if index + 1 == batch.commands().len() =>
                {
                    ensure!(command.valid(), "invalid graph activation");
                    if let Some(upload) = self.image_upload.take() {
                        ensure!(
                            upload.graph == revision && upload.bytes.len() == upload.expected,
                            "incomplete graph image"
                        );
                        let graph = shoop_graph::Graph::decode(&upload.bytes)?;
                        if graph.loops.iter().any(|entry| {
                            entry
                                .composite
                                .as_ref()
                                .is_some_and(|plan| plan.sample_rate != self.status.sample_rate)
                        }) {
                            anyhow::bail!(GraphRefusal(Code::InvalidValue));
                        }
                        let logical = compile_composites(
                            &graph,
                            CompileContext {
                                quantum: upload.quantum,
                                limits: self.limits,
                            },
                            self.programs.chunk_frames(),
                        )?;
                        self.stage_graph(batch, logical)?;
                        return Ok(true);
                    }
                    let upload = self
                        .upload
                        .take()
                        .ok_or_else(|| anyhow::anyhow!("missing graph header"))?;
                    if let Some(code) = upload.failure {
                        anyhow::bail!(GraphRefusal(code));
                    }
                    ensure!(
                        upload.graph == revision && upload.loops.len() == upload.expected as usize,
                        "incomplete graph upload"
                    );
                    let graph = shoop_rt_protocol::graph::from_loops(&upload.loops);
                    let logical = compile_with_chunk_frames(
                        &graph,
                        CompileContext {
                            quantum: upload.quantum,
                            limits: self.limits,
                        },
                        self.programs.chunk_frames(),
                    )?;
                    self.stage_graph(batch, logical)?;
                    return Ok(true);
                }
                _ => anyhow::bail!("invalid graph record"),
            }
        }
        Ok(false)
    }
    fn stage_graph(
        &mut self,
        batch: Batch,
        logical: rt_fpga_compiler::LogicalStateProgram,
    ) -> Result<()> {
        let cancel_key = match batch.commands().last() {
            Some(Command::ActivateCompositeGraph(key)) => Some(*key),
            _ => None,
        };
        let layout = self
            .allocator
            .stage_with_composite_retirement(&logical, cancel_key.is_none())?;
        let revision = layout.revision;
        let staged = self.programs.stage_program(&logical, layout);
        match staged {
            Ok(descriptor) => {
                self.pending = Some(Pending {
                    batch,
                    descriptor,
                    ready: None,
                    published: false,
                    cancel_key,
                    cancel_requested: false,
                    notify: true,
                    usage: ProgramUsage {
                        instructions: u32::from(descriptor.instructions),
                        jobs: logical.resources.queue_capacity,
                        buffers: u32::from(descriptor.buffers),
                    },
                });
                Ok(())
            }
            Err(error) => {
                self.allocator.cancel(revision)?;
                Err(error)
            }
        }
    }
    fn response(batch: Batch, frame: u64, code: Code, committed: Option<u64>) -> EngineMessage {
        let records = batch
            .commands()
            .iter()
            .map(|&c| CommandRecord::new(c).complete(code))
            .collect::<Vec<_>>();
        let header = committed.map_or_else(|| batch.header(), |r| Header::new(r, r, None));
        EngineMessage::ImmediateResponse(
            Response::new(header.complete(frame, code), &records).unwrap(),
        )
    }
    fn translate(&mut self, message: EngineMessage) -> Option<EngineMessage> {
        if let EngineMessage::ImmediateResponse(response) = &message {
            if let Some(record) = self.pending.as_ref().filter(|p| p.published).and_then(|p| {
                response
                    .records()
                    .iter()
                    .find(|r| r.command() == Command::SwitchProgram(p.descriptor))
            }) {
                let pending = self.pending.take().unwrap();
                let code = match record.code() {
                    Code::Applied => Code::Applied,
                    Code::Cancelled => Code::Cancelled,
                    _ => Code::CannotApply,
                };
                let success = code == Code::Applied;
                if success {
                    self.allocator
                        .complete(pending.descriptor.revision)
                        .expect("fenced switch completion");
                    self.program_usage = pending.usage;
                } else {
                    self.allocator
                        .refused(pending.descriptor.revision)
                        .expect("refused pending image");
                }
                return pending.notify.then(|| {
                    Self::response(
                        pending.batch,
                        response.header.frame(),
                        code,
                        success.then_some(pending.descriptor.revision),
                    )
                });
            }
        }
        Some(message)
    }

    fn cancel_graph(&mut self, batch: Batch) -> Result<Code, CarrierError> {
        if !self.control.is_connected() {
            return Err(CarrierError::Disconnected);
        }
        let revision = self.allocator.revision();
        if (batch.graph, batch.topology) != (revision, revision) {
            return Ok(Code::StaleConfiguration);
        }
        let [Command::CancelCompositeGraph(key)] = batch.commands() else {
            return Ok(Code::InvalidValue);
        };
        if *key == 0 || batch.deadline.is_some() {
            return Ok(Code::InvalidValue);
        }
        let Some(pending) = self.pending.as_mut().filter(|p| p.cancel_key == Some(*key)) else {
            return Ok(Code::TargetUnavailable);
        };
        if !pending.cancel_requested {
            self.programs.cancel_program(pending.descriptor.revision)?;
            pending.cancel_requested = true;
        }
        Ok(Code::Applied)
    }
    fn flush(&mut self, out: &mut Outbox) -> bool {
        loop {
            let Some(message) = self.held.take().or_else(|| self.replies.pop()) else {
                return true;
            };
            let Some(message) = self.translate(message) else {
                continue;
            };
            if out.available(message.lane()) == 0 {
                self.held = Some(message);
                return false;
            }
            out.push(message).unwrap();
        }
    }
}
impl<L: ProgramLoader> CarrierEndpoint for Endpoint<L> {
    fn try_deliver(
        &mut self,
        message: ClientMessage,
        out: &mut Outbox,
    ) -> core::result::Result<(), CarrierError> {
        if self.status.driver().state == DriverRunState::Failed {
            return Err(CarrierError::Disconnected);
        }
        if let ClientMessage::Immediate(batch) = message {
            if batch
                .commands()
                .iter()
                .any(|c| matches!(c, Command::CancelCompositeGraph(_)))
            {
                if out.available(Lane::Reliable) == 0 {
                    return Err(CarrierError::Full);
                }
                let code = self.cancel_graph(batch)?;
                return out.push(Self::response(
                    batch,
                    self.status.frame.load(Ordering::Acquire),
                    code,
                    None,
                ));
            }
            if batch.commands().iter().any(|c| {
                matches!(
                    c,
                    Command::BeginGraph { .. }
                        | Command::ConfigureLoop(_)
                        | Command::BeginGraphImage { .. }
                        | Command::GraphImageData { .. }
                        | Command::ActivateGraph
                        | Command::ActivateCompositeGraph(_)
                )
            }) {
                if out.available(Lane::Reliable) == 0 {
                    return Err(CarrierError::Full);
                }
                match self.accept_graph(batch) {
                    Ok(true) => {}
                    Ok(false) => out.push(Self::response(batch, 0, Code::Applied, None))?,
                    Err(error) => {
                        let code = refusal_code(&error);
                        self.upload = None;
                        self.image_upload = None;
                        out.push(Self::response(batch, 0, code, None))?;
                    }
                }
                return Ok(());
            }
            if self.allocator.revision() == 0 {
                return out.push(Self::response(batch, 0, Code::CannotApply, None));
            }
        }
        let new_session = matches!(
            message,
            ClientMessage::Hello { .. } | ClientMessage::Goodbye
        );
        self.control.try_deliver(message, out)?;
        if new_session {
            self.upload = None;
            self.image_upload = None;
            if let Some(pending) = &mut self.pending {
                pending.cancel_key = None;
                pending.notify = false;
            }
            self.usage = UsageTelemetry::default();
        }
        Ok(())
    }
    fn deliver(&mut self, message: ClientMessage, out: &mut Outbox) {
        let _ = self.try_deliver(message, out);
    }
    fn poll(&mut self, out: &mut Outbox) {
        self.control.set_driver(self.status.driver());
        if self.status.driver().state == DriverRunState::Failed {
            if self.failed || !self.flush(out) {
                return;
            }
            while !self.control.drain_responses(&mut self.replies) {
                if !self.flush(out) {
                    return;
                }
            }
            if !self.flush(out) || out.available(Lane::Reliable) == 0 {
                return;
            }
            if self
                .pending
                .as_ref()
                .is_some_and(|pending| !pending.published)
            {
                let pending = self.pending.take().unwrap();
                let frame = self.status.frame.load(Ordering::Acquire);
                if pending.notify {
                    out.push(Self::response(
                        pending.batch,
                        frame,
                        Code::CannotApply,
                        None,
                    ))
                    .expect("available failure response slot");
                }
            }
            self.failed = out
                .push(EngineMessage::Disconnected(
                    shoop_rt_protocol::DisconnectReason::Fault,
                ))
                .is_ok();
            return;
        }
        let frame = self.status.frame.load(Ordering::Acquire);
        if self.control.is_connected() {
            self.status.publish_engine(frame, &mut self.sent_frame, out);
        }
        self.programs.reclaim_programs();
        if !self.flush(out) {
            return;
        }
        while let Some((revision, success)) = self.programs.program_ready() {
            if let Some(p) = &mut self.pending {
                if p.descriptor.revision == revision {
                    p.ready = Some(success);
                }
            }
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.ready == Some(false))
        {
            let pending = self.pending.take().unwrap();
            self.allocator.cancel(pending.descriptor.revision).unwrap();
            self.held = pending
                .notify
                .then(|| Self::response(pending.batch, 0, Code::CannotApply, None));
            if !self.flush(out) {
                return;
            }
        }
        if let Some(pending) = &mut self.pending {
            if pending.ready == Some(true) && !pending.published {
                let revision = self.allocator.revision();
                if self
                    .control
                    .submit_switch(Header::new(revision, revision, None), pending.descriptor)
                    .is_ok()
                {
                    self.allocator.publish(pending.descriptor.revision).unwrap();
                    pending.published = true;
                }
            }
        }
        self.control.poll(&mut self.replies);
        self.flush(out);
        self.publish_usage(out);
    }
}

/// Render-side endpoint for a worklet whose compiler and graph allocator run remotely.
pub struct StorageEndpoint {
    control: rt::Endpoint,
    status: Arc<Status>,
    sent_frame: u64,
    usage: UsageTelemetry,
}

impl StorageEndpoint {
    pub fn set_driver(&mut self, driver: DriverStatus) {
        self.control.set_driver(driver);
    }
}

impl CarrierEndpoint for StorageEndpoint {
    fn try_deliver(
        &mut self,
        message: ClientMessage,
        out: &mut Outbox,
    ) -> Result<(), CarrierError> {
        if matches!(
            message,
            ClientMessage::Hello { .. } | ClientMessage::Goodbye
        ) {
            self.usage = UsageTelemetry::default();
        }
        self.control.try_deliver(message, out)
    }

    fn deliver(&mut self, message: ClientMessage, out: &mut Outbox) {
        let _ = self.try_deliver(message, out);
    }

    fn poll(&mut self, out: &mut Outbox) {
        self.control.poll(out);
        if self.control.is_connected() {
            let frame = self.status.frame.load(Ordering::Acquire);
            self.status.publish_engine(frame, &mut self.sent_frame, out);
            if let Some(report) = self.status.storage.report(frame) {
                self.usage
                    .publish(report, self.status.usage_interval(), out);
            }
        }
    }
}

impl<L: ProgramLoader> Endpoint<L> {
    pub fn snapshot_capacity(&self) -> usize {
        self.control.snapshot_capacity()
    }
    pub fn read_snapshot(&self, bytes: &mut [u8]) -> Option<(u32, usize)> {
        self.control.read_snapshot(bytes)
    }
}
