#![cfg(feature = "rt-interface")]
use rt_fpga_compiler::{
    graph::*,
    loading::{Allocator, LoadError},
    Identity, ProgramLimits,
};
use rt_fpga_reference::{
    compiler::RustTarget,
    fixed::Fixed,
    jobs::{BufferId, JobProcessor, Memory},
    pipeline::Pipeline,
    rt_interface::*,
    state::{Instruction, Program},
    storage::{MediaStore, ViewId},
    transitions::{PlanDescription, TransitionLimits, TransitionPlan},
    Error,
};
use shoop_rt_protocol::{
    immediate::{Batch, Code, Command, Response, SwitchProgram, TransitionMode},
    snapshot::{Header, Record, Records, HEADER},
    *,
};
use shoop_rt_transport::{
    carrier::{InProcess, Outbox},
    client::ProtocolClient,
    rt,
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
fn limits() -> ProgramLimits {
    ProgramLimits {
        loop_slots: 8,
        state_values: 32,
        jobs: 128,
        buffers: 32,
        history_slots: 32,
        instructions: 128,
        ..Default::default()
    }
}
struct DelayedMemory {
    media: MediaStore,
    delay: usize,
    pending: usize,
}
impl Memory for DelayedMemory {
    fn begin_capture(&mut self, view: ViewId) -> Result<(), Error> {
        self.media.begin_capture(view)
    }
    fn read(
        &mut self,
        view: ViewId,
        position: u64,
        length: u64,
        out: &mut [Fixed],
    ) -> Result<(), Error> {
        Memory::read(&mut self.media, view, position, length, out)
    }
    fn write(&mut self, view: ViewId, input: &[Fixed]) -> Result<(), Error> {
        Memory::write(&mut self.media, view, input)
    }
    fn prepare_unused_view(&mut self, view: ViewId) -> Result<bool, Error> {
        self.media.prepare_unused_view(view)
    }
    fn begin_completion_fence(&mut self) -> Result<(), Error> {
        self.pending = self.delay;
        self.media.begin_completion_fence()
    }
    fn completion_fence_ready(&mut self) -> Result<bool, Error> {
        if self.pending != 0 {
            self.pending -= 1;
            Ok(false)
        } else {
            Ok(true)
        }
    }
}
struct Fixture {
    client: ProtocolClient<InProcess<Endpoint>>,
    programs: ProgramControl,
    processor: Processor,
    jobs: JobProcessor,
    memory: DelayedMemory,
    allocator: Allocator,
}
impl Fixture {
    fn new() -> Self {
        let plan = TransitionPlan::compile(
            PlanDescription {
                quantum: 16,
                loops: vec![],
            },
            TransitionLimits {
                loop_slots: 8,
                events: 32,
            },
        )
        .unwrap();
        let program = Program {
            quantum: 16,
            chunk_frames: 1,
            sync_instructions: vec![],
            jobs_per_quantum: 3,
            instructions: vec![
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
            ],
        };
        let pipeline = Pipeline::prepare(plan, program, vec![], 128).unwrap();
        let (control, execution) = rt::channel(
            64,
            4,
            shoop_rt_protocol::snapshot::capacity(0, 8, 0).unwrap(),
        )
        .unwrap();
        let endpoint = Endpoint::new(
            control,
            Settings {
                graph: 0,
                topology: 0,
                limits: Limits {
                    composite_entries: 0,
                    composite_segments: 0,
                    timing_relations: 0,
                    content_chunk_bytes: 0,
                    strips: 0,
                    loops: 8,
                    strip_audio_channels: 1,
                    master_channels: 2,
                    buses: 0,
                    bus_channels: 0,
                    host_audio_inputs: 1,
                    host_audio_outputs: 2,
                    host_midi_inputs: 0,
                    host_midi_outputs: 0,
                    max_frames: 128,
                    quantum: 16,
                    processing_latency_frames: 0,
                    smoothing_limit_frames: 0,
                    reliable_credits: 0,
                    bulk_credits: 0,
                },
                driver: DriverStatus {
                    kind: DriverKind::Simulated,
                    state: DriverRunState::Running,
                    sample_rate: 8000,
                    buffer_frames: 16,
                },
            },
        )
        .unwrap();
        let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(8, 4, 0)));
        client.connect().unwrap();
        assert!(matches!(client.poll(), Some(EngineMessage::Welcome { .. })));
        let (programs, execution_programs) = program_memory();
        let processor = Processor::prepare(
            pipeline,
            execution,
            Configuration {
                graph: 0,
                topology: 0,
                frame: 0,
            },
            vec![],
        )
        .unwrap()
        .with_program_memory(execution_programs, limits())
        .unwrap();
        let mut media = MediaStore::new(1).unwrap();
        media.create_recordings(8).unwrap();
        media.reserve_chunk_capacity(4096).unwrap();
        for view in 0..8 {
            media.reserve_view_capacity(ViewId(view), 4096).unwrap();
        }
        Self {
            client,
            programs,
            processor,
            jobs: JobProcessor::with_history_slots(16, 32, 32),
            memory: DelayedMemory {
                media,
                delay: 0,
                pending: 0,
            },
            allocator: Allocator::new(limits()),
        }
    }
    fn step(&mut self) {
        self.processor
            .process(
                &mut self.jobs,
                &mut self.memory,
                &[&[Fixed::from_f32(0.25); 16]],
                &mut [&mut [Fixed::ZERO; 16], &mut [Fixed::ZERO; 16]],
            )
            .unwrap();
    }
    fn stage(&mut self, ids: &[u32]) -> SwitchProgram {
        let logical = GraphDescription {
            quantum: 16,
            loops: ids
                .iter()
                .map(|&slot| LoopDescription {
                    id: Identity {
                        slot,
                        generation: 1,
                    },
                    source: None,
                    input: true,
                    output: true,
                    gain: 1.0,
                    repeat_sync: false,
                })
                .collect(),
        }
        .compile()
        .unwrap();
        let layout = self.allocator.stage(&logical).unwrap();
        let views = layout
            .loops
            .iter()
            .map(|l| u64::from(l.physical.slot))
            .collect::<Vec<_>>();
        let fresh = layout
            .new_loops()
            .map(|l| ViewId(l.slot as usize))
            .collect();
        let mut target = RustTarget::new(&views);
        target.limits = limits();
        let image =
            PreparedProgram::new(target.build_image(&logical, layout).unwrap(), fresh).unwrap();
        let descriptor = image.descriptor();
        assert!(self.programs.stage(image).is_ok());
        for _ in 0..100 {
            self.step();
            if let Some(ready) = self.programs.ready() {
                assert_eq!(ready.revision, descriptor.revision);
                ready.result.unwrap();
                return descriptor;
            }
        }
        panic!("staging did not become ready");
    }
    fn send(&mut self, revision: u64, commands: &[Command]) {
        self.client
            .immediate(Batch::new(revision, revision, commands).unwrap())
            .unwrap();
    }
    fn publish(&mut self, s: SwitchProgram) {
        let old = self.allocator.revision();
        self.allocator.publish(s.revision).unwrap();
        self.client
            .carrier_mut()
            .endpoint_mut()
            .set_configuration(s.revision, s.revision);
        self.send(old, &[Command::SwitchProgram(s)]);
    }
    fn response(&mut self) -> Option<Response> {
        while let Some(message) = self.client.poll() {
            if let EngineMessage::ImmediateResponse(r) = message {
                return Some(r);
            }
        }
        None
    }
    fn install(&mut self, ids: &[u32]) {
        let s = self.stage(ids);
        self.publish(s);
        self.step();
        let response = self.response().unwrap();
        assert_eq!(response.records()[0].code(), Code::Applied);
        self.allocator.complete(s.revision).unwrap();
        self.step();
        while self.programs.reclaim().is_some() {}
    }
    fn snapshot(&mut self) -> (Header, Vec<Record>) {
        let endpoint = self.client.carrier_mut().endpoint();
        let mut bytes = vec![0; endpoint.snapshot_capacity()];
        let (_, len) = endpoint.read_snapshot(&mut bytes).unwrap();
        (
            Header::read(&bytes[..len]).unwrap(),
            Records(&bytes[HEADER..len])
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
        )
    }
}

#[shoop_test]
fn ordered_switch_preserves_recording_and_gain_and_rebinds_recycled_physical_slots() {
    let mut f = Fixture::new();
    f.install(&[0, 1]);
    f.send(
        1,
        &[
            Command::Transition(1, TransitionMode::Recording.into()),
            Command::LoopGain(1, 0.5),
        ],
    );
    f.step();
    assert!(f.response().is_some());
    let b = f.stage(&[1, 2]);
    let before = f.processor.pipeline().state().frame();
    let physical = f
        .allocator
        .current()
        .unwrap()
        .loop_id(Identity {
            slot: 1,
            generation: 1,
        })
        .unwrap();
    let id = rt_fpga_reference::transitions::LoopId {
        slot: physical.slot,
        generation: physical.generation,
    };
    let captured = f
        .processor
        .pipeline()
        .state()
        .loop_state(id)
        .unwrap()
        .state
        .capture_frames;
    f.publish(b);
    f.send(2, &[Command::LoopGain(1, 0.25)]);
    f.send(1, &[Command::LoopGain(1, 0.9)]);
    assert_no_heap(|| f.step());
    assert_eq!(f.response().unwrap().header.frame(), before);
    assert_eq!(f.response().unwrap().records()[0].code(), Code::Applied);
    assert_eq!(
        f.response().unwrap().header.code(),
        Code::StaleConfiguration
    );
    assert_eq!(
        f.processor
            .pipeline()
            .state()
            .loop_state(id)
            .unwrap()
            .state
            .capture_frames,
        captured + 16
    );
    assert!(f
        .snapshot()
        .1
        .iter()
        .any(|r| matches!(r,Record::LoopLevels {id,gain,..} if id.0.index==1 && *gain==0.25)));
    f.allocator.complete(b.revision).unwrap();
    f.step();
    while f.programs.reclaim().is_some() {}
    f.install(&[1, 3]);
    let current = f.allocator.current().unwrap();
    assert_eq!(
        current
            .loop_id(Identity {
                slot: 3,
                generation: 1
            })
            .unwrap()
            .slot,
        0
    );
    f.send(3, &[Command::LoopGain(3, 0.125)]);
    f.step();
    assert_eq!(f.response().unwrap().records()[0].code(), Code::Applied);
    assert!(f
        .snapshot()
        .1
        .iter()
        .any(|r| matches!(r,Record::LoopLevels {id,gain,..} if id.0.index==3 && *gain==0.125)));
}

#[shoop_test]
fn completion_waits_for_memory_fence_while_new_program_and_snapshots_advance() {
    let mut f = Fixture::new();
    f.install(&[0]);
    let b = f.stage(&[1]);
    f.memory.delay = 3;
    let frame = f.processor.pipeline().state().frame();
    f.publish(b);
    assert_no_heap(|| f.step());
    assert_eq!(f.processor.pipeline().revision(), b.revision);
    assert_eq!(f.snapshot().0.graph, b.revision);
    assert!(f.response().is_none());
    assert!(f.programs.reclaim().is_none());
    assert_eq!(f.allocator.revision(), 1);
    assert_eq!(f.allocator.cancel(b.revision), Err(LoadError::Busy));
    for _ in 0..2 {
        assert_no_heap(|| f.step());
        assert!(f.response().is_none());
        assert!(f.programs.reclaim().is_none());
    }
    assert_no_heap(|| f.step());
    let response = f.response().unwrap();
    assert_eq!(response.header.frame(), frame);
    assert_eq!(response.records()[0].code(), Code::Applied);
    f.allocator.complete(b.revision).unwrap();
    assert_no_heap(|| f.step());
    assert!(f.programs.reclaim().is_some());
    assert_eq!(f.processor.pipeline().state().frame(), frame + 5 * 16);
}

#[shoop_test]
fn cancelled_staging_and_wrong_descriptor_cannot_activate_or_modify_current_state() {
    let mut f = Fixture::new();
    f.install(&[0]);
    let b = f.stage(&[1]);
    assert!(f.programs.discard(b.revision));
    f.step();
    f.allocator.cancel(b.revision).unwrap();
    f.step();
    assert!(f.programs.reclaim().is_some());
    let c = f.stage(&[1]);
    f.send(1, &[Command::SwitchProgram(b)]);
    f.step();
    assert_eq!(f.response().unwrap().records()[0].code(), Code::CannotApply);
    assert_eq!(f.processor.pipeline().revision(), 1);
    f.publish(c);
    f.step();
    assert_eq!(f.response().unwrap().records()[0].code(), Code::Applied);
    f.allocator.complete(c.revision).unwrap();
}
