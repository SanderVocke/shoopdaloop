#![cfg(feature = "rt-interface")]
use rt_fpga_reference::{
    fixed::{Fixed, Ramp},
    jobs::{BufferId, JobProcessor},
    pipeline::Pipeline,
    rt_interface::{Configuration, Endpoint, LoopBinding, Processor, Settings},
    state::{Address, Instruction, Program, Value},
    storage::MediaStore,
    transitions::{self as model, LoopId},
};
use shoop_rt_engine::{testing::OfflineHost, EngineDescription, HostPorts, Segment};
use shoop_rt_protocol::{
    self as protocol,
    immediate::{
        Batch, ClearPlanned, Code, Command, PlannedTransition, TransitionMode, TransitionPlan,
    },
    snapshot::{Header, Record, Records, HEADER},
    *,
};
use shoop_rt_transport::{
    carrier::{InProcess, Outbox},
    client::{ClientError, ProtocolClient},
    rt,
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

struct Controller(
    ProtocolClient<InProcess<Endpoint>>,
    std::collections::VecDeque<protocol::immediate::Response>,
);
impl Controller {
    fn submit(&mut self, batch: Batch) -> Result<(), ClientError> {
        self.0.immediate(batch)
    }
    fn response(&mut self) -> Option<protocol::immediate::Response> {
        if let Some(response) = self.1.pop_front() {
            return Some(response);
        }
        while let Some(message) = self.0.poll() {
            if let EngineMessage::ImmediateResponse(response) = message {
                return Some(response);
            }
        }
        None
    }
    fn snapshot_error(&mut self) -> Option<rt::Error> {
        self.0.carrier_mut().endpoint().snapshot_error()
    }
}

fn id(slot: u32) -> LoopId {
    LoopId {
        slot,
        generation: 1,
    }
}
fn pid(slot: u32) -> LoopSlotId {
    LoopSlotId(SlotId::new(slot, 1))
}
fn prepare(
    q: u32,
    config: Configuration,
    n: usize,
) -> (Controller, Processor, JobProcessor, MediaStore) {
    let plan = model::TransitionPlan::compile(
        model::PlanDescription {
            quantum: q,
            loops: (0..n)
                .map(|i| model::LoopSpec {
                    id: id(i as u32),
                    kind: model::LoopKind::Primitive,
                    length: 0,
                    sync_source: (i > 0).then(|| id(0)),
                    repeat_sync: false,
                    timeline: None,
                })
                .collect(),
        },
        model::TransitionLimits {
            loop_slots: n as u32,
            events: 4 * n as u32,
        },
    )
    .unwrap();
    let mut memory = vec![Value::Ramp(Ramp::constant(Fixed::ONE)); n];
    let mut instructions = vec![Instruction::ProcessInput {
        port: 0,
        destination: BufferId(0),
    }];
    let mut media = MediaStore::new(1).unwrap();
    for i in 0..n {
        let view = media.create_recording().unwrap();
        memory.push(Value::Word(view.0 as u64));
        instructions.extend([
            Instruction::ProcessLogicalPlayback {
                loop_id: id(i as u32),
                view: Address(n + i),
                destination: BufferId(1 + 2 * i),
            },
            Instruction::ProcessGain {
                ramp: Address(i),
                source: BufferId(1 + 2 * i),
                destination: BufferId(2 + 2 * i),
            },
            Instruction::ProcessOutput {
                source: BufferId(2 + 2 * i),
                port: i,
            },
            Instruction::ProcessLogicalCapture {
                loop_id: id(i as u32),
                view: Address(n + i),
                source: BufferId(0),
            },
        ]);
    }
    let program = Program {
        sync_instructions: plan.sync_instructions(),
        instructions,
        quantum: q,
        chunk_frames: 1,
        jobs_per_quantum: 1 + 5 * n,
    };
    let pipeline = Pipeline::prepare(plan, program, memory, 1 + 5 * n).unwrap();
    let (control, ports) =
        rt::channel(256, 3, protocol::snapshot::capacity(0, n, 0).unwrap()).unwrap();
    let bindings = (0..n)
        .map(|i| LoopBinding {
            gain: Some(Address(i)),
            meter: Some(BufferId(2 + 2 * i)),
            ..LoopBinding::new(id(i as u32))
        })
        .collect();
    let limits = Limits {
        composite_entries: 0,
        composite_segments: 0,
        timing_relations: 0,
        content_chunk_bytes: 0,
        strips: 0,
        loops: n as u32,
        strip_audio_channels: 1,
        master_channels: 1,
        buses: 0,
        bus_channels: 0,
        host_audio_inputs: 1,
        host_audio_outputs: n as u16,
        host_midi_inputs: 0,
        host_midi_outputs: 0,
        max_frames: 2 * q,
        quantum: q,
        processing_latency_frames: 0,
        smoothing_limit_frames: 0,
        reliable_credits: 0,
        bulk_credits: 0,
    };
    let endpoint = Endpoint::new(
        control,
        Settings {
            graph: config.graph,
            topology: config.topology,
            limits,
            driver: DriverStatus {
                kind: DriverKind::Simulated,
                state: DriverRunState::Running,
                sample_rate: 1000,
                buffer_frames: q,
            },
        },
    )
    .unwrap();
    let mut client = ProtocolClient::new(InProcess::new(endpoint, Outbox::new(256, 8, 0)));
    client.connect().unwrap();
    assert!(matches!(client.poll(), Some(EngineMessage::Welcome { .. })));
    client.observe_snapshots(true).unwrap();
    client
        .configure_snapshots(protocol::snapshot::SnapshotDelivery::Pull)
        .unwrap();
    (
        Controller(client, Default::default()),
        Processor::prepare(pipeline, ports, config, bindings).unwrap(),
        JobProcessor::new(q as usize, 1 + 2 * n),
        media,
    )
}
fn done(host: &mut OfflineHost, q: usize, request: Request) {
    let correlation = host.request(request).unwrap();
    for _ in 0..2000 {
        while let Some(message) = host.receive() {
            if let EngineMessage::Result {
                correlation: c,
                outcome,
            } = message
            {
                if c == correlation {
                    assert_eq!(outcome, Outcome::Done);
                    return;
                }
            }
        }
        host.process(q).unwrap();
    }
    panic!("native setup timeout {request:?}");
}
fn native(q: u32, n: usize) -> OfflineHost {
    let description = EngineDescription {
        sample_rate: 1000,
        max_frames: 2 * q as usize,
        quantum: q as usize,
        host: HostPorts::numbered(1, 1, 0),
        segments: vec![Segment {
            strips: 1,
            loops: n as u32,
        }],
        strip_channels: 1,
        master_channels: 1,
        buses: 1,
        bus_channels: 1,
        master_connections: vec![(0, 0)],
        resource_pages: 512,
        page_bytes: 256,
        ramp_frames: 0,
        smoothing_frames: 0,
        control_lead_frames: 0,
        processors: ProcessorSettings::disabled(),
        ..EngineDescription::default()
    };
    let mut host = OfflineHost::new(&description).unwrap();
    host.connect().unwrap();
    while host.receive().is_some() {}
    done(
        &mut host,
        q as usize,
        Request::CreateStrip {
            strip: StripId(SlotId::new(0, 1)),
            shape: StripShape {
                audio_inputs: 1,
                audio_outputs: 1,
                midi_input: false,
                midi_output: false,
            },
        },
    );
    for i in 0..n {
        done(
            &mut host,
            q as usize,
            Request::CreateLoop {
                loop_id: pid(i as u32),
                strip: StripId(SlotId::new(0, 1)),
            },
        );
        if i > 0 {
            done(
                &mut host,
                q as usize,
                Request::LoopSync {
                    loop_id: pid(i as u32),
                    source: Some(pid(0)),
                },
            );
        }
    }
    host.client.observe_snapshots(true).unwrap();
    host.client
        .configure_snapshots(protocol::snapshot::SnapshotDelivery::Pull)
        .unwrap();
    host.process(q as usize).unwrap();
    while host.receive().is_some() {}
    host
}
fn native_snapshot(host: &mut OfflineHost) -> (Header, Vec<Record>) {
    let mut bytes = vec![0; host.endpoint().snapshot_capacity()];
    let (_, len) = host.endpoint().read_snapshot(&mut bytes).unwrap();
    (
        Header::read(&bytes[..len]).unwrap(),
        Records(&bytes[HEADER..len]).map(Result::unwrap).collect(),
    )
}
fn reference_snapshot(control: &mut Controller) -> (Header, Vec<Record>) {
    control.0.request_snapshot(0).unwrap();
    let mut bytes = Vec::new();
    loop {
        match control.0.poll().expect("complete snapshot publication") {
            EngineMessage::SnapshotReply(chunk) => {
                assert_eq!(chunk.offset as usize, bytes.len());
                bytes.extend_from_slice(&chunk.bytes[..chunk.len as usize]);
            }
            EngineMessage::SnapshotReady { .. } => break,
            EngineMessage::SnapshotConfiguration { .. } => {}
            EngineMessage::ImmediateResponse(response) => control.1.push_back(response),
            message => panic!("{message:?}"),
        }
    }
    assert!(!bytes.is_empty());
    (
        Header::read(&bytes).unwrap(),
        Records(&bytes[HEADER..]).map(Result::unwrap).collect(),
    )
}
fn run(
    endpoint: &mut Processor,
    jobs: &mut JobProcessor,
    media: &mut MediaStore,
    q: usize,
    n: usize,
) {
    let mut output = vec![vec![Fixed::ZERO; q]; n];
    let mut slices: Vec<_> = output.iter_mut().map(Vec::as_mut_slice).collect();
    endpoint
        .process_block(jobs, media, &[&vec![Fixed::ZERO; q]], &mut slices, q)
        .unwrap();
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 16
    }
    fn mode(&mut self) -> TransitionMode {
        match self.next() % 3 {
            0 => TransitionMode::Stopped,
            1 => TransitionMode::Playing,
            _ => TransitionMode::Recording,
        }
    }
    fn step(&mut self) -> Option<PlannedTransition> {
        (self.next() % 3 != 0).then(|| PlannedTransition {
            mode: self.mode(),
            skip: (self.next() % 4) as u32,
        })
    }
    fn command(&mut self) -> Command {
        let target = (self.next() % 2) as u32;
        match self.next() % 5 {
            0 => Command::LoopGain(target, (self.next() % 5) as f32 / 4.),
            1 => Command::Transition(target, self.mode().into()),
            2 => Command::Transition(
                target,
                TransitionPlan {
                    now: (self.next() % 2 == 0).then(|| self.mode()),
                    first: self.step(),
                    second: self.step(),
                },
            ),
            3 => Command::ClearPlannedTransitions(
                target,
                match self.next() % 3 {
                    0 => ClearPlanned::First,
                    1 => ClearPlanned::Second,
                    _ => ClearPlanned::Both,
                },
            ),
            _ => Command::LoopGain(99, 0.5),
        }
    }
}

#[shoop_test]
fn fifo_commands_responses_and_loop_snapshots_match_native() {
    for q in [1, 16, 32, 64, 128] {
        let mut native = native(q, 2);
        let (header, _) = native_snapshot(&mut native);
        let config = Configuration {
            graph: header.graph,
            topology: header.topology,
            frame: native.endpoint().command_clock().frame().0,
        };
        let (mut control, mut endpoint, mut jobs, mut media) = prepare(q, config, 2);
        let mut random = Rng(u64::from(q) + 91);
        for tick in 0..128 {
            let commands = match tick {
                0 => vec![
                    Command::Transition(0, TransitionMode::Recording.into()),
                    Command::LoopGain(1, 0.25),
                ],
                3 => vec![Command::Transition(0, TransitionMode::Playing.into())],
                4 => vec![Command::Transition(
                    1,
                    TransitionPlan {
                        now: None,
                        first: Some(PlannedTransition {
                            mode: TransitionMode::Recording,
                            skip: 0,
                        }),
                        second: Some(PlannedTransition {
                            mode: TransitionMode::Stopped,
                            skip: 2,
                        }),
                    },
                )],
                5 => vec![Command::Transition(1, TransitionMode::Playing.into())],
                14 => vec![
                    Command::Transition(1, TransitionMode::Playing.into()),
                    Command::LoopGain(0, 0.5),
                ],
                17 => vec![Command::Transition(0, TransitionMode::Playing.into())],
                20 => vec![Command::ClearPlannedTransitions(1, ClearPlanned::Both)],
                23 => vec![
                    Command::Transition(0, TransitionMode::Stopped.into()),
                    Command::Transition(1, TransitionMode::Stopped.into()),
                ],
                24 => vec![Command::Transition(0, TransitionMode::Playing.into())],
                25 => vec![Command::Transition(
                    1,
                    TransitionPlan {
                        now: None,
                        first: Some(PlannedTransition {
                            mode: TransitionMode::Recording,
                            skip: 0,
                        }),
                        second: Some(PlannedTransition {
                            mode: TransitionMode::Playing,
                            skip: 0,
                        }),
                    },
                )],
                32.. => (0..random.next() % 4).map(|_| random.command()).collect(),
                _ => vec![],
            };
            if !commands.is_empty() {
                let batch = Batch::new(config.graph, config.topology, &commands).unwrap();
                native.client.immediate(batch).unwrap();
                control.submit(batch).unwrap();
            }
            let frames = q as usize * if tick % 7 == 6 { 2 } else { 1 };
            native.process(frames).unwrap();
            run(&mut endpoint, &mut jobs, &mut media, frames, 2);
            let mut responses = vec![];
            while let Some(message) = native.receive() {
                if let EngineMessage::ImmediateResponse(r) = message {
                    responses.push(r);
                }
            }
            let mut actual = vec![];
            while let Some(r) = control.response() {
                actual.push(r);
            }
            assert_eq!(actual, responses, "responses Q={q} tick={tick}");
            let (expected_header, expected) = native_snapshot(&mut native);
            let (actual_header, actual) = reference_snapshot(&mut control);
            assert_eq!(
                (
                    actual_header.frame,
                    actual_header.graph,
                    actual_header.topology
                ),
                (
                    expected_header.frame,
                    expected_header.graph,
                    expected_header.topology
                ),
                "header Q={q} tick={tick}"
            );
            let expected: Vec<_> = expected
                .into_iter()
                .filter(|r| {
                    matches!(
                        r,
                        Record::LoopPlan { .. }
                            | Record::LoopLevels { .. }
                            | Record::Telemetry(Telemetry::Loop { .. })
                            | Record::State(StateRecord::LoopTiming { .. })
                    )
                })
                .collect();
            assert_eq!(actual.len(), expected.len());
            for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
                assert_eq!(
                    actual, expected,
                    "snapshot Q={q} tick={tick} record={index}"
                );
            }
        }
    }
}

#[shoop_test]
fn ordered_failures_configuration_deadlines_and_backpressure_preserve_state() {
    let config = Configuration {
        graph: 7,
        topology: 9,
        frame: 0,
    };
    let (mut control, mut endpoint, mut jobs, mut media) = prepare(1, config, 1);
    for _ in 0..128 {
        control
            .submit(Batch::new(7, 9, &[Command::LoopGain(0, 0.25)]).unwrap())
            .unwrap();
    }
    assert!(control
        .submit(Batch::new(7, 9, &[Command::LoopGain(0, 0.5)]).unwrap())
        .is_err());
    run(&mut endpoint, &mut jobs, &mut media, 1, 1);
    assert!(
        control
            .submit(Batch::new(7, 9, &[Command::LoopGain(0, 0.5)]).unwrap())
            .is_err(),
        "responses retain credits"
    );
    for _ in 0..128 {
        assert_eq!(
            control.response().unwrap().records()[0].code(),
            Code::Applied
        );
    }
    let batches = [
        Batch::new(6, 9, &[Command::LoopGain(0, 0.5)]).unwrap(),
        Batch::new(7, 9, &[Command::LoopGain(0, 0.5)])
            .unwrap()
            .with_deadline(0),
        Batch::new(
            7,
            9,
            &[
                Command::LoopGain(99, 0.5),
                Command::LoopGain(0, f32::NAN),
                Command::BusMute(0, true),
                Command::LoopGain(0, 0.75),
            ],
        )
        .unwrap(),
    ];
    for batch in batches {
        control.submit(batch).unwrap();
    }
    run(&mut endpoint, &mut jobs, &mut media, 1, 1);
    assert_eq!(
        control.response().unwrap().header.code(),
        Code::StaleConfiguration
    );
    assert_eq!(
        control.response().unwrap().header.code(),
        Code::DeadlineMissed
    );
    let response = control.response().unwrap();
    assert_eq!(
        response
            .records()
            .iter()
            .map(|r| r.code())
            .collect::<Vec<_>>(),
        vec![
            Code::TargetUnavailable,
            Code::InvalidValue,
            Code::Unsupported,
            Code::Applied
        ]
    );
    assert!(reference_snapshot(&mut control)
        .1
        .contains(&Record::LoopLevels {
            id: pid(0),
            gain: 0.75,
            balance: 0.,
            repeat_sync: false
        }));
}

#[shoop_test]
fn reference_fifo_drives_exact_audio_and_publishes_the_first_settled_quantum() {
    let config = Configuration {
        graph: 1,
        topology: 2,
        frame: 0,
    };
    let (mut control, mut endpoint, mut jobs, mut media) = prepare(16, config, 1);
    control
        .submit(
            Batch::new(
                1,
                2,
                &[Command::Transition(0, TransitionMode::Recording.into())],
            )
            .unwrap(),
        )
        .unwrap();
    let input: Vec<_> = (0..32)
        .map(|i| Fixed::from_f32(if i < 16 { 0.25 } else { 0.5 }))
        .collect();
    let mut output = vec![Fixed(99); 32];
    endpoint
        .process_block(&mut jobs, &mut media, &[&input], &mut [&mut output], 32)
        .unwrap();
    assert_eq!(output, vec![Fixed::ZERO; 32]);
    assert_eq!(endpoint.pipeline().state().frame(), 32);
    assert_eq!(reference_snapshot(&mut control).0.frame, 0);
    assert_eq!(
        endpoint
            .pipeline()
            .state()
            .loop_state(id(0))
            .unwrap()
            .state
            .capture_frames,
        32
    );
    control.response().unwrap();
    control
        .submit(
            Batch::new(
                1,
                2,
                &[
                    Command::Transition(0, TransitionMode::Playing.into()),
                    Command::LoopGain(0, 0.5),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    for expected in [0.125, 0.25, 0.125] {
        let mut output = [Fixed(99); 16];
        endpoint
            .process(
                &mut jobs,
                &mut media,
                &[&[Fixed::ZERO; 16]],
                &mut [&mut output],
            )
            .unwrap();
        assert_eq!(output, [Fixed::from_f32(expected); 16]);
    }
    let (_, records) = reference_snapshot(&mut control);
    assert!(records.iter().any(
        |r| matches!(r,Record::Telemetry(Telemetry::Loop{peaks,..}) if peaks.as_slice()==[0.25])
    ));
    // Unread publications are superseded without blocking execution.
    for _ in 0..20 {
        endpoint
            .process(
                &mut jobs,
                &mut media,
                &[&[Fixed::ZERO; 16]],
                &mut [&mut [Fixed::ZERO; 16]],
            )
            .unwrap();
    }
    assert_eq!(
        reference_snapshot(&mut control).0.frame,
        endpoint.pipeline().state().frame() - 16
    );
}

#[shoop_test]
fn endpoint_scalar_execution_and_snapshot_publication_do_not_allocate() {
    let config = Configuration {
        graph: 1,
        topology: 2,
        frame: 0,
    };
    let (mut control, mut endpoint, mut jobs, mut media) = prepare(16, config, 1);
    let mut output = [Fixed::ZERO; 16];
    for _ in 0..20 {
        control
            .submit(Batch::new(1, 2, &[Command::LoopGain(0, 0.25)]).unwrap())
            .unwrap();
        assert_no_heap(|| {
            endpoint.process(
                &mut jobs,
                &mut media,
                &[&[Fixed::ZERO; 16]],
                &mut [&mut output],
            )
        })
        .unwrap();
        assert_eq!(
            control.response().unwrap().records()[0].code(),
            Code::Applied
        );
    }
}

struct Inject {
    media: MediaStore,
    control: Controller,
    publish: Option<Batch>,
    fail: bool,
}

#[shoop_test]
fn carrier_polling_and_client_lifetime_do_not_drive_processing() {
    use shoop_rt_transport::carrier::Carrier;
    let config = Configuration {
        graph: 1,
        topology: 2,
        frame: 0,
    };
    let (mut control, mut processor, mut jobs, mut media) = prepare(16, config, 1);
    for message in [
        ClientMessage::Request {
            correlation: 1,
            request: Request::RemoveLoop { loop_id: pid(0) },
        },
        ClientMessage::Resync,
        ClientMessage::Bulk(BulkRead {
            id: 1,
            loop_id: pid(0),
            selector: BulkSelector::AudioFrames,
            offset: 0,
        }),
    ] {
        control.0.carrier_mut().send(message).unwrap();
    }
    control
        .submit(
            Batch::new(
                1,
                2,
                &[Command::Transition(0, TransitionMode::Recording.into())],
            )
            .unwrap(),
        )
        .unwrap();
    for _ in 0..100 {
        assert!(control.0.poll().is_none());
        assert_eq!(processor.pipeline().state().frame(), 0);
        assert_eq!(
            processor
                .pipeline()
                .state()
                .loop_state(id(0))
                .unwrap()
                .state
                .mode,
            model::Mode::Stopped
        );
    }
    run(&mut processor, &mut jobs, &mut media, 16, 1);
    assert_eq!(control.response().unwrap().header.frame(), 0);
    assert_eq!(reference_snapshot(&mut control).0.frame, 0);
    drop(control);
    for _ in 0..8 {
        run(&mut processor, &mut jobs, &mut media, 16, 1);
    }
    let state = processor.pipeline().state();
    assert_eq!(state.frame(), 144);
    assert_eq!(state.loop_state(id(0)).unwrap().state.capture_frames, 144);
}

impl rt_fpga_reference::jobs::Memory for Inject {
    fn begin_capture(
        &mut self,
        view: rt_fpga_reference::storage::ViewId,
    ) -> Result<(), rt_fpga_reference::Error> {
        rt_fpga_reference::jobs::Memory::begin_capture(&mut self.media, view)
    }
    fn read(
        &mut self,
        view: rt_fpga_reference::storage::ViewId,
        position: u64,
        length: u64,
        output: &mut [Fixed],
    ) -> Result<(), rt_fpga_reference::Error> {
        rt_fpga_reference::jobs::Memory::read(&mut self.media, view, position, length, output)
    }
    fn write(
        &mut self,
        view: rt_fpga_reference::storage::ViewId,
        input: &[Fixed],
    ) -> Result<(), rt_fpga_reference::Error> {
        if let Some(batch) = self.publish.take() {
            self.control.submit(batch).unwrap();
        }
        if self.fail {
            return Err(rt_fpga_reference::Error::MemoryFull);
        }
        rt_fpga_reference::jobs::Memory::write(&mut self.media, view, input)
    }
}
#[shoop_test]
fn commands_published_during_a_callback_wait_for_the_next_callback() {
    let config = Configuration {
        graph: 1,
        topology: 2,
        frame: 0,
    };
    let (mut control, mut endpoint, mut jobs, media) = prepare(1, config, 1);
    control
        .submit(
            Batch::new(
                1,
                2,
                &[Command::Transition(0, TransitionMode::Recording.into())],
            )
            .unwrap(),
        )
        .unwrap();
    let mut memory = Inject {
        media,
        control,
        publish: Some(Batch::new(1, 2, &[Command::LoopGain(0, 0.25)]).unwrap()),
        fail: false,
    };
    endpoint
        .process_block(
            &mut jobs,
            &mut memory,
            &[&[Fixed::ZERO; 2]],
            &mut [&mut [Fixed::ZERO; 2]],
            2,
        )
        .unwrap();
    assert!(reference_snapshot(&mut memory.control)
        .1
        .contains(&Record::LoopLevels {
            id: pid(0),
            gain: 1.,
            balance: 0.,
            repeat_sync: false
        }));
    assert_eq!(memory.control.response().unwrap().header.frame(), 0);
    assert!(memory.control.response().is_none());
    endpoint
        .process(
            &mut jobs,
            &mut memory,
            &[&[Fixed::ZERO; 1]],
            &mut [&mut [Fixed::ZERO; 1]],
        )
        .unwrap();
    assert_eq!(memory.control.response().unwrap().header.frame(), 2);
    assert!(reference_snapshot(&mut memory.control)
        .1
        .contains(&Record::LoopLevels {
            id: pid(0),
            gain: 0.25,
            balance: 0.,
            repeat_sync: false
        }));
}
#[shoop_test]
fn invalid_blocks_preserve_commands_and_processing_faults_reject_future_groups() {
    use rt_fpga_reference::{pipeline::PipelineError, Error};
    let config = Configuration {
        graph: 1,
        topology: 2,
        frame: 0,
    };
    let (mut control, mut endpoint, mut jobs, media) = prepare(16, config, 1);
    control
        .submit(
            Batch::new(
                1,
                2,
                &[Command::Transition(0, TransitionMode::Recording.into())],
            )
            .unwrap(),
        )
        .unwrap();
    let mut memory = Inject {
        media,
        control,
        publish: None,
        fail: true,
    };
    let mut output = [Fixed(99); 15];
    assert_eq!(
        endpoint.process_block(
            &mut jobs,
            &mut memory,
            &[&[Fixed::ZERO; 15]],
            &mut [&mut output],
            15
        ),
        Err(PipelineError::Jobs(Error::InvalidShape))
    );
    assert!(memory.control.response().is_none());
    assert_eq!(endpoint.pipeline().state().frame(), 0);
    assert_eq!(output, [Fixed::ZERO; 15]);
    assert_eq!(
        endpoint.process(
            &mut jobs,
            &mut memory,
            &[&[Fixed::ZERO; 16]],
            &mut [&mut [Fixed::ZERO; 16]]
        ),
        Err(PipelineError::Jobs(Error::MemoryFull))
    );
    assert_eq!(
        memory.control.response().unwrap().records()[0].code(),
        Code::Applied
    );
    assert!(memory.control.snapshot_error().is_some());
    memory
        .control
        .submit(Batch::new(1, 2, &[Command::LoopGain(0, 0.5)]).unwrap())
        .unwrap();
    assert_eq!(
        endpoint.process(
            &mut jobs,
            &mut memory,
            &[&[Fixed::ZERO; 16]],
            &mut [&mut [Fixed::ZERO; 16]]
        ),
        Err(PipelineError::Jobs(Error::MemoryFull))
    );
    let response = memory.control.response().unwrap();
    assert_eq!(response.header.code(), Code::CannotApply);
    assert_eq!(response.records()[0].code(), Code::CannotApply);
}
