#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::Command,
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    midi_state::MidiState,
    processor::{ProcessBlock, Processor},
    resource::{self, CaptureGroupConfig, Format, MidiIndexConfig, ResourceStore},
    schedule::Execution,
    transport::{Lane, Outcome as TransportOutcome},
    Budgets, Config, Epoch, Error, Frame, NodeId, PlannerId, Revision, Sequence,
};
use shoop_loops::{
    audio::Smoothing,
    media,
    runtime::*,
    topology::staging::{StageId, Status},
    *,
};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[path = "support/configuration_scenarios.rs"]
mod configuration_scenarios;
#[path = "support/program_graph_scenarios.rs"]
mod program_graph_scenarios;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
fn limits(journal: usize) -> Limits {
    Limits {
        loop_slots: 3,
        relations: 8,
        program_entries: 8,
        program_segments: 16,
        pending_requests: 17,
        journal_events: journal,
    }
}
fn description(changed: bool, reverse: bool) -> TopologyDescription {
    let mut loops = vec![
        LoopSpec {
            id: id(0),
            kind: LoopKind::Primitive,
            length: 1,
            sync_source: None,
        },
        LoopSpec {
            id: id(1),
            kind: LoopKind::Primitive,
            length: 4,
            sync_source: Some(id(0)),
        },
        LoopSpec {
            id: id(2),
            kind: LoopKind::Composite,
            length: 0,
            sync_source: Some(id(0)),
        },
    ];
    if reverse {
        loops.reverse();
    }
    TopologyDescription {
        loops,
        triggers: vec![],
        programs: vec![ProgramDescription {
            source: id(2),
            nominal_unit_frames: 1,
            sections: vec![Section {
                entries: vec![Entry {
                    target: id(1),
                    delay: u64::from(changed),
                    duration: Some(if changed { 2 } else { 4 }),
                    repetitions: 1,
                    mode: None,
                }],
            }],
        }],
    }
}
struct Fx {
    fault: Option<Frame>,
    stimulus: u32,
    source: f32,
}
impl Drop for Fx {
    fn drop(&mut self) {
        assert!(!support::is_rendering());
    }
}
impl Processor<LoopPlanner> for Fx {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 1,
            midi_outputs: 1,
        }
    }
    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        if parameter == u32::MAX - 1 && value.is_finite() {
            self.source = value;
            return Ok(());
        }
        if parameter != u32::MAX || ![1., 2.].contains(&value) {
            return Err(Error::InvalidParameter);
        }
        self.stimulus = value as u32;
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        if self.fault == Some(block.frame) {
            return Err(Error::InvalidState);
        }
        for channel in 0..2 {
            for frame in 0..block.frames {
                let value = block.audio.input(channel).unwrap()[frame] * 0.5
                    + self.source * if channel == 0 { 1. } else { 10. };
                block.audio.output(channel).unwrap()[frame] = value;
            }
        }
        for index in 0..block.midi_inputs[0].len() {
            let (offset, bytes) = block.midi_inputs[0].get(index).unwrap();
            block.midi_outputs[0].push(offset, block.frames, bytes)?;
        }
        if self.stimulus == 1 {
            for packet in [
                [0xb2, 1, 37],
                [0x92, 61, 91],
                [0xb2, 64, 127],
                [0x82, 61, 0],
                [0x92, 62, 72],
            ] {
                block.midi_outputs[0].push(0, block.frames, &packet)?;
            }
        } else if self.stimulus == 2 {
            block.midi_outputs[0].push(0, block.frames, &[0x92, 63, 55])?;
        }
        self.stimulus = 0;
        Ok(())
    }
}
struct Log {
    events: Vec<Event>,
    states: Box<[[Option<State>; 3]; 128]>,
    dropped: u64,
}
impl Log {
    fn new() -> Self {
        let mut events = vec![
            Event {
                frame: Frame(0),
                target: id(0),
                kind: decision::EventKind::Pulse
            };
            256
        ];
        events.clear();
        Self {
            events,
            states: Box::new([[None; 3]; 128]),
            dropped: 0,
        }
    }
}
impl BoundaryObserver for Log {
    fn boundary(&mut self, frame: Frame, decisions: &[Decision], events: &[Event], dropped: u64) {
        assert!(self.events.len() + events.len() <= self.events.capacity());
        self.events.extend_from_slice(events);
        self.dropped = dropped;
        for decision in decisions {
            self.states[frame.0 as usize][decision.state.id.slot() as usize] = Some(decision.state);
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
fn encoded_midi() -> Vec<u8> {
    [(0u64, [0x90, 60, 100]), (2, [0x80, 60, 45])]
        .into_iter()
        .flat_map(|(frame, bytes)| {
            frame
                .to_le_bytes()
                .into_iter()
                .chain(3u32.to_le_bytes())
                .chain(bytes)
        })
        .collect()
}
fn upload(store: &mut ResourceStore, format: Format, bytes: &[u8]) -> media::Source {
    let resource = store.create().unwrap();
    let description = resource::Description {
        format,
        frames: 4,
        bytes: bytes.len(),
    };
    let token = if format == Format::Midi {
        store
            .begin_indexed_midi_upload(resource, Revision(0), description, &MidiState::default())
            .unwrap()
    } else {
        store
            .begin_upload(resource, Revision(0), description)
            .unwrap()
    };
    store.begin_callback();
    store.write_upload(token, 0, bytes).unwrap();
    store.finish_upload(token).unwrap();
    store.commit(token).unwrap();
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    media::Source {
        snapshot,
        index: store.read_index(snapshot.id).unwrap(),
    }
}
struct Harness {
    control: Controller<Fx>,
    engine: Engine<Fx>,
    media: bool,
    output: [[f32; 32]; 2],
    midi: [MidiBuffer; 1],
    samples: Vec<[f32; 2]>,
    packets: Vec<(u64, Vec<u8>)>,
    replies: Vec<Execution<Operation, Reply>>,
    returned: Vec<(Sequence, Box<[u8]>)>,
    graphs: Vec<
        rt_runtime::transport::Completion<
            Command<Node<Fx>, LoopPlanner>,
            rt_runtime::executor::Retired<Node<Fx>>,
        >,
    >,
    imports: Vec<rt_runtime::ResourceId>,
}
impl Harness {
    fn new(media: bool, journal: usize, reverse: bool, staging: TopologySettings) -> Self {
        Self::options(media, journal, reverse, staging, 8, None)
    }
    fn options(
        media: bool,
        journal: usize,
        reverse: bool,
        staging: TopologySettings,
        media_slots: usize,
        fault: Option<Frame>,
    ) -> Self {
        Self::with_node_pairs(
            media,
            journal,
            reverse,
            staging,
            media_slots,
            fault,
            NodePairSettings::default(),
            false,
        )
    }
    fn with_node_pairs(
        media: bool,
        journal: usize,
        reverse: bool,
        staging: TopologySettings,
        media_slots: usize,
        fault: Option<Frame>,
        node_pairs: NodePairSettings,
        spare: bool,
    ) -> Self {
        let config = Config {
            max_frames: 32,
            max_nodes: if spare { 7 } else { 5 },
            max_ports: 24,
            max_edges: 16,
            audio_inputs: 0,
            audio_outputs: if media { 2 } else { 0 },
            midi_inputs: 0,
            midi_outputs: usize::from(media),
            midi_events_per_port: 1024,
            midi_bytes_per_port: 4096,
            resource_slots: 16,
            resource_pages: 128,
            page_bytes: 64,
            command_slots: 32,
            scheduled_events: 32,
            budgets: Budgets {
                commands: 32,
                events: 32,
                publications: 32,
                subranges: 0,
                transfer_bytes: (staging.records_per_callback * 64).max(64),
                validation_steps: 128,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()
        .unwrap();
        let mut resources = ResourceStore::with_capture_groups(
            config,
            Some(MidiIndexConfig {
                versions: 4,
                checkpoints_per_version: 4,
                replay_events: 128,
            }),
            CaptureGroupConfig {
                groups: 2,
                members_per_group: 2,
                staging_bytes_per_group: 4096,
            },
        )
        .unwrap();
        let mut imports = Vec::new();
        let mut owned = Vec::new();
        if media {
            let bytes: Vec<_> = [4f32, 8., 12., 16.]
                .into_iter()
                .flat_map(|v| [v, v * 10.])
                .flat_map(f32::to_le_bytes)
                .collect();
            let audio = upload(&mut resources, Format::Audio { channels: 2 }, &bytes);
            let midi = upload(&mut resources, Format::Midi, &encoded_midi());
            imports.extend([audio.snapshot.resource, midi.snapshot.resource]);
            owned.push(media::Input {
                description: media::Description {
                    owner: id(1),
                    revision: Revision(0),
                    frames: 4,
                    channels: 2,
                    midi: true,
                },
                audio: Some(audio),
                midi: Some(midi),
            });
        }
        let mut bindings: Vec<_> = (0..2)
            .map(|slot| Binding {
                target: id(slot),
                playback: node(slot * 2),
                capture: node(slot * 2 + 1),
                channels: if media && slot == 1 { 2 } else { 0 },
                midi: media && slot == 1,
                gain: 1.,
                smoothing: Smoothing::None,
                midi_events: 256,
                midi_bytes: 512,
                packet_bytes: 512,
            })
            .collect();
        if spare {
            bindings.push(Binding {
                target: LoopId::from_parts(2, 2).unwrap(),
                playback: node(5),
                capture: node(6),
                channels: 0,
                midi: false,
                gain: 1.,
                smoothing: Smoothing::None,
                midi_events: 0,
                midi_bytes: 0,
                packet_bytes: 0,
            });
        }
        let mut graph = GraphDescription {
            nodes: (0..if media { 5 } else { 4 }).map(node).collect(),
            connections: vec![],
        };
        if media {
            for (kind, ports) in [(PortType::Audio, 2), (PortType::Midi, 1)] {
                for port in 0..ports {
                    graph.connections.push(Connection {
                        source: Endpoint::node(node(2), kind, port),
                        destination: Endpoint::node(node(4), kind, port),
                    });
                    for destination in [
                        Endpoint::node(node(3), kind, port),
                        Endpoint::host(kind, port),
                    ] {
                        graph.connections.push(Connection {
                            source: Endpoint::node(node(4), kind, port),
                            destination,
                        });
                    }
                }
            }
        }
        if reverse {
            bindings.reverse();
            graph.nodes.reverse();
            graph.connections.reverse();
        }
        let (control, engine) = Engine::prepare_with_configuration_staging(
            config,
            compile(limits(journal), &description(false, reverse)).unwrap(),
            resources,
            Settings {
                planner: PlannerId::from_parts(0, 1).unwrap(),
                normal_requests: 16,
                media_slots,
                export_slots: 4,
            },
            bindings,
            owned,
            if media {
                vec![(
                    node(4),
                    Fx {
                        fault,
                        stimulus: 0,
                        source: 0.,
                    },
                )]
            } else {
                vec![]
            },
            graph,
            staging,
            node_pairs,
            media.then(|| Box::new(Log::new()) as Box<dyn BoundaryObserver>),
        )
        .unwrap();
        Self {
            control,
            engine,
            media,
            output: [[0.; 32]; 2],
            midi: [MidiBuffer::new(1024, 4096).unwrap()],
            samples: Vec::new(),
            packets: Vec::new(),
            replies: Vec::new(),
            returned: Vec::new(),
            graphs: Vec::new(),
            imports,
        }
    }
    fn send(&mut self, lane: Lane, at: u64, operation: Operation, payload: Box<[u8]>) -> Sequence {
        assert_no_heap(|| {
            self.control
                .submit_payload(lane, Frame(at), operation, payload)
        })
        .unwrap_or_else(|e| panic!("{:?}", e.error))
        .sequence()
    }
    fn op(&mut self, at: u64, operation: Operation) -> Sequence {
        self.send(Lane::Normal, at, operation, Box::new([]))
    }
    fn step(&mut self, frames: usize) {
        let frame = self.engine.next_frame();
        self.midi[0].clear();
        let [left, right] = &mut self.output;
        if self.media {
            assert_no_heap(|| {
                self.engine.process(
                    AudioBlock {
                        frame,
                        frames,
                        inputs: &[],
                        outputs: &mut [&mut left[..frames], &mut right[..frames]],
                    },
                    MidiBlock {
                        inputs: &[],
                        outputs: &mut self.midi,
                    },
                )
            })
            .unwrap();
        } else {
            assert_no_heap(|| {
                self.engine.process(
                    AudioBlock {
                        frame,
                        frames,
                        inputs: &[],
                        outputs: &mut [],
                    },
                    MidiBlock {
                        inputs: &[],
                        outputs: &mut [],
                    },
                )
            })
            .unwrap();
        }
        for i in 0..frames {
            self.samples.push([left[i], right[i]]);
        }
        for i in 0..self.midi[0].len() {
            let (offset, bytes) = self.midi[0].get(i).unwrap();
            self.packets.push((frame.0 + offset as u64, bytes.to_vec()));
        }
    }
    fn drain(&mut self) {
        while let Some(completion) = assert_no_heap(|| self.control.try_completion()) {
            if matches!(
                completion.request,
                Command::StageGraph { .. } | Command::ReleaseGraph { .. }
            ) {
                self.graphs.push(completion);
                continue;
            }
            assert!(
                matches!(
                    completion.outcome,
                    TransportOutcome::Scheduled { late_by: 0, .. }
                ),
                "sequence {:?}: {:?}",
                completion.sequence,
                completion.outcome
            );
            if let Command::Schedule { payload, .. } = completion.request {
                self.returned.push((completion.sequence, payload));
            } else {
                panic!("unexpected owning completion");
            }
        }
        while let Some(reply) = assert_no_heap(|| self.control.try_execution()) {
            assert_eq!(reply.result, Ok(()));
            self.replies.push(reply);
        }
    }
    fn reply(&self, sequence: Sequence) -> Reply {
        self.replies
            .iter()
            .find(|r| r.sequence == sequence)
            .unwrap()
            .operation_result
            .unwrap()
    }
    fn begin(&mut self, slot: usize, topology: &PreparedTopology) -> StageId {
        let mut header = [0; 64];
        topology.encode_header(&mut header);
        let sequence = self.send(
            Lane::Normal,
            self.engine.next_frame().0,
            Operation::TopologyBegin { slot },
            Box::new(header),
        );
        self.step(1);
        self.drain();
        assert_eq!(
            &*self.returned.iter().find(|r| r.0 == sequence).unwrap().1,
            &header
        );
        match self.reply(sequence) {
            Reply::TopologyStatus {
                id,
                status: Status::Receiving { received: 0, .. },
            } => id,
            other => panic!("{other:?}"),
        }
    }
    fn ready(&mut self, slot: usize, topology: &PreparedTopology) -> StageId {
        let id = self.begin(slot, topology);
        let bytes = image(topology, 0, topology.encoded_records());
        let original = bytes.clone();
        let at = self.engine.next_frame().0;
        let append = self.send(
            Lane::Normal,
            at,
            Operation::TopologyAppend {
                id,
                first_record: 0,
            },
            bytes,
        );
        let validate = self.op(at, Operation::TopologyValidate { id });
        self.step(1);
        self.drain();
        assert_eq!(
            &*self.returned.iter().find(|r| r.0 == append).unwrap().1,
            &*original
        );
        assert!(
            matches!(self.reply(validate),Reply::TopologyProgress {progress,..} if progress.ready)
        );
        id
    }
    fn graph_plan(
        &mut self,
        slot: usize,
        epoch: u64,
        plan: rt_runtime::graph::PreparedPlan,
    ) -> GraphStageId {
        let sequence = assert_no_heap(|| self.control.stage_graph(slot, Epoch(epoch), plan))
            .unwrap_or_else(|e| panic!("{:?}", e.error))
            .sequence();
        self.step(1);
        self.drain();
        let result = self.graphs.iter().find(|r| r.sequence == sequence).unwrap();
        assert_eq!(result.outcome, TransportOutcome::Applied);
        match &result.request {
            Command::StageGraph {
                id: Some(id),
                plan: None,
                ..
            } => *id,
            _ => panic!("ready graph owner"),
        }
    }
    fn graph(&mut self, slot: usize, epoch: u64, bypass: bool, reverse: bool) -> GraphStageId {
        let plan = self
            .control
            .compile_graph(&routes(self.media, bypass, reverse))
            .unwrap();
        self.graph_plan(slot, epoch, plan)
    }
    fn read_export(
        &mut self,
        export: media::ExportId,
        midi: bool,
        expected_bytes: usize,
    ) -> Vec<u8> {
        let sequence = self.op(
            self.engine.next_frame().0,
            Operation::Cursor { export, midi },
        );
        self.step(1);
        self.drain();
        let Reply::Cursor(mut cursor) = self.reply(sequence) else {
            panic!("cursor")
        };
        let mut output = Vec::new();
        for chunk in 0..expected_bytes.div_ceil(64).max(1) {
            let bytes = Box::new([0; 64]);
            let ticket = assert_no_heap(|| self.control.read(cursor, bytes))
                .unwrap_or_else(|e| panic!("{:?}", e.error));
            self.step(1);
            let completed = assert_no_heap(|| self.control.try_completion()).unwrap();
            assert_eq!(completed.sequence, ticket.sequence());
            assert_eq!(completed.outcome, TransportOutcome::Applied);
            let Command::Resource(command) = completed.request else {
                panic!("read")
            };
            let Some(resource::Reply::Read { written, eof }) = command.reply() else {
                panic!("read result")
            };
            let resource::Request::Read {
                cursor: next,
                bytes,
            } = command.into_request()
            else {
                panic!("read owner")
            };
            output.extend_from_slice(&bytes[..written]);
            cursor = next;
            assert_eq!(eof, chunk + 1 == expected_bytes.div_ceil(64).max(1));
        }
        assert_eq!(output.len(), expected_bytes);
        output
    }
    fn finish(mut self) -> Option<Log> {
        self.drain();
        drop(self.control);
        let mut stopped = self.engine.into_stopped();
        assert_no_heap(|| {
            stopped.cleanup().unwrap();
            stopped.cleanup().unwrap();
            for id in &self.imports {
                stopped.resources().remove(*id).unwrap();
            }
            for _ in 0..128 {
                stopped.resources().begin_callback();
                stopped.resources().reclaim();
            }
            assert_eq!(stopped.resource_usage().free_pages, 128);
            assert_eq!(stopped.resource_usage().free_versions, 16);
            assert_eq!(stopped.resource_usage().free_snapshots, 16);
            assert_eq!(stopped.media_usage().references, 0);
        });
        stopped
            .observer()
            .as_any()
            .downcast_ref::<Log>()
            .map(|log| Log {
                events: log.events.clone(),
                states: log.states.clone(),
                dropped: log.dropped,
            })
    }
}
fn image(topology: &PreparedTopology, start: usize, count: usize) -> Box<[u8]> {
    let mut bytes = vec![0; count * 64];
    for (index, chunk) in bytes.chunks_exact_mut(64).enumerate() {
        topology
            .encode_record(start + index, chunk.try_into().unwrap())
            .unwrap();
    }
    bytes.into_boxed_slice()
}
fn staging() -> TopologySettings {
    TopologySettings {
        candidates: 2,
        records_per_callback: 64,
        validation_steps_per_callback: 4096,
    }
}
fn play(target: u32) -> Operation {
    Operation::Transition {
        target: id(target),
        timing: Timing::At,
        transition: Transition::Play {
            position: 0,
            retrigger: true,
        },
    }
}
fn stop(target: u32) -> Operation {
    Operation::Transition {
        target: id(target),
        timing: Timing::At,
        transition: Transition::Stop,
    }
}
fn activate(id: StageId, epoch: u64) -> Operation {
    Operation::TopologyActivate {
        id,
        graph_epoch: Epoch(1),
        topology_epoch: Epoch(epoch),
    }
}
fn program_activate(id: StageId) -> Operation {
    Operation::ProgramActivate {
        id,
        target: self::id(2),
        graph_epoch: Epoch(1),
        topology_epoch: Epoch(0),
    }
}
fn restore(frame: u64, output: &mut Vec<(u64, Vec<u8>)>) {
    for channel in 0..16 {
        for packet in [
            [0xe0 | channel, 0, 64],
            [0xb0 | channel, 64, 0],
            [0xb0 | channel, 69, 0],
            [0xb0 | channel, 66, 0],
        ] {
            output.push((frame, packet.to_vec()));
        }
    }
    output.push((frame, vec![0x90, 60, 100]));
}
fn expected_events() -> Vec<Event> {
    use decision::EventKind::{Iteration, Pulse, Transition as Changed, Wrap};
    let play = Transition::Play {
        position: 0,
        retrigger: true,
    };
    let mut events = Vec::new();
    for frame in 2..=12 {
        let mut push = |target, kind| {
            events.push(Event {
                frame: Frame(frame),
                target: id(target),
                kind,
            })
        };
        if frame == 12 {
            push(0, Changed(Transition::Stop));
        } else {
            push(0, if frame == 2 { Changed(play) } else { Wrap });
            push(0, Pulse);
        }
        if [5, 12].contains(&frame) {
            push(2, Changed(Transition::Stop));
        } else {
            if [2, 6].contains(&frame) {
                push(2, Changed(play));
            }
            push(2, Pulse);
            push(
                2,
                Iteration {
                    iteration: if frame < 5 {
                        frame - 2
                    } else {
                        (frame - 6) % 3
                    },
                    passes: u64::from(frame >= 9),
                },
            );
        }
        if [2, 7, 10].contains(&frame) {
            push(1, Changed(play));
        }
        if [5, 9, 12].contains(&frame) {
            push(1, Changed(Transition::Stop));
        }
        if ![5, 6, 9, 12].contains(&frame) {
            push(1, Pulse);
        }
    }
    events
}
#[shoop_test]
fn staging_work_shares_the_callback_budget_and_recovery_ignores_normal_receipt_stalls() {
    let settings = TopologySettings {
        candidates: 2,
        records_per_callback: 2,
        validation_steps_per_callback: 3,
    };
    let mut h = Harness::new(false, 0, false, settings);
    let topology = compile(limits(0), &description(true, false)).unwrap();
    let staged = h.begin(0, &topology);
    h.op(1, play(0));
    let first = h.send(
        Lane::Normal,
        4,
        Operation::TopologyAppend {
            id: staged,
            first_record: 0,
        },
        image(&topology, 0, 2),
    );
    h.step(1);
    let refused = h.send(
        Lane::Normal,
        5,
        Operation::TopologyAppend {
            id: staged,
            first_record: 2,
        },
        image(&topology, 2, 1),
    );
    let status = h.op(6, Operation::TopologyStatus { id: staged });
    h.step(1);
    h.step(4);
    h.drain();
    assert_eq!(
        h.reply(first),
        Reply::TopologyStatus {
            id: staged,
            status: Status::Receiving {
                received: 2,
                total: topology.encoded_records()
            }
        }
    );
    assert_eq!(
        h.reply(refused),
        Reply::Failed(Failure::Timeline(shoop_loops::Error::Capacity))
    );
    assert_eq!(h.reply(status), h.reply(first));
    let mut received = 2;
    while received < topology.encoded_records() {
        let count = 2.min(topology.encoded_records() - received);
        let at = h.engine.next_frame().0;
        h.send(
            Lane::Normal,
            at,
            Operation::TopologyAppend {
                id: staged,
                first_record: received,
            },
            image(&topology, received, count),
        );
        h.step(1);
        h.drain();
        received += count;
    }
    let at = h.engine.next_frame().0;
    let a = h.op(at, Operation::TopologyValidate { id: staged });
    let b = h.op(at + 1, Operation::TopologyValidate { id: staged });
    h.step(4);
    h.drain();
    let Reply::TopologyProgress {
        progress: first, ..
    } = h.reply(a)
    else {
        panic!("progress")
    };
    let Reply::TopologyProgress {
        progress: second, ..
    } = h.reply(b)
    else {
        panic!("progress")
    };
    assert_eq!(first.used, 3);
    assert!(!first.ready);
    assert_eq!(second.used, 0);
    assert_eq!(first.total_steps, second.total_steps);
    assert!(!second.ready);
    let mut previous = second;
    let mut calls = 0;
    while !previous.ready {
        let at = h.engine.next_frame().0;
        let seq = h.op(at, Operation::TopologyValidate { id: staged });
        h.step(1);
        h.drain();
        let Reply::TopologyProgress { progress, .. } = h.reply(seq) else {
            panic!("progress")
        };
        assert!(progress.used > 0 && progress.used <= 3);
        assert_eq!(
            progress.total_steps,
            previous.total_steps + progress.used as u64
        );
        previous = progress;
        calls += 1;
        assert!(calls < 4096);
    }
    let at = h.engine.next_frame().0;
    for _ in 0..32 {
        h.op(at, Operation::State { target: id(0) });
    }
    h.step(1);
    let cancellation = h.send(
        Lane::Recovery,
        h.engine.next_frame().0,
        Operation::TopologyCancel { id: staged },
        Box::new([]),
    );
    for _ in 0..4 {
        h.step(32);
    }
    h.drain();
    assert_eq!(
        h.reply(cancellation),
        Reply::TopologyStatus {
            id: staged,
            status: Status::Idle
        }
    );
    let at = h.engine.next_frame().0;
    let cut = h.op(at, Operation::State { target: id(0) });
    h.step(1);
    h.drain();
    let Reply::State(cut) = h.reply(cut) else {
        panic!("cut")
    };
    assert_eq!(cut.state.mode, Mode::Playing);
    assert_eq!(cut.state.position, 0);
    assert_eq!(cut.state.cycles, at - 1);
    assert_eq!(cut.state.last_pulse, Some(Frame(at)));
    assert_eq!(cut.topology_epoch, Epoch(0));
    h.finish();
}

#[shoop_test]
fn stale_scope_generation_node_pair_and_malformed_candidates_preserve_the_working_timeline() {
    let mut h = Harness::new(false, 0, false, staging());
    h.op(0, play(0));
    let source = compile(limits(0), &description(false, false)).unwrap();
    let first = h.ready(0, &source);
    let at = h.engine.next_frame().0;
    let applied = h.op(at, activate(first, 0));
    let cannot_cancel = h.op(at, Operation::TopologyCancel { id: first });
    let recycle = h.op(at, Operation::TopologyRecycle { id: first });
    h.step(1);
    h.drain();
    assert!(matches!(
        h.reply(applied),
        Reply::TopologyActivated {
            topology_epoch: Epoch(1),
            ..
        }
    ));
    assert_eq!(
        h.reply(cannot_cancel),
        Reply::Failed(Failure::Timeline(shoop_loops::Error::InvalidState))
    );
    assert_eq!(
        h.reply(recycle),
        Reply::TopologyStatus {
            id: first,
            status: Status::Idle
        }
    );
    let mut changed = description(true, false);
    let new_id = LoopId::from_parts(1, 2).unwrap();
    changed.loops[1].id = new_id;
    changed.programs[0].sections[0].entries[0].target = new_id;
    let incompatible = compile(limits(0), &changed).unwrap();
    let second = h.ready(0, &incompatible);
    assert_eq!(first.parts()[0], second.parts()[0]);
    assert_eq!(first.parts()[1] + 1, second.parts()[1]);
    let at = h.engine.next_frame().0;
    let stale = h.op(at, Operation::TopologyCancel { id: first });
    let foreign = h.op(
        at,
        Operation::TopologyValidate {
            id: StageId::from_parts(u32::MAX, 1).unwrap(),
        },
    );
    let refused = h.op(at, activate(second, 1));
    let still_ready = h.op(at, Operation::TopologyStatus { id: second });
    h.step(1);
    h.drain();
    for seq in [stale, foreign] {
        assert_eq!(
            h.reply(seq),
            Reply::Failed(Failure::Timeline(shoop_loops::Error::InvalidId))
        );
    }
    assert_eq!(
        h.reply(refused),
        Reply::Failed(Failure::Runtime(Error::InvalidId))
    );
    assert_eq!(
        h.reply(still_ready),
        Reply::TopologyStatus {
            id: second,
            status: Status::Ready
        }
    );
    let at = h.engine.next_frame().0;
    let cancelled = h.send(
        Lane::Recovery,
        at,
        Operation::TopologyCancel { id: second },
        Box::new([]),
    );
    h.step(1);
    h.drain();
    assert_eq!(
        h.reply(cancelled),
        Reply::TopologyStatus {
            id: second,
            status: Status::Idle
        }
    );
    let malformed = h.begin(0, &source);
    let mut bytes = image(&source, 0, source.encoded_records());
    bytes[10] = 1;
    let at = h.engine.next_frame().0;
    let bad = h.send(
        Lane::Normal,
        at,
        Operation::TopologyAppend {
            id: malformed,
            first_record: 0,
        },
        bytes,
    );
    let status = h.op(at, Operation::TopologyStatus { id: malformed });
    let payload = h.send(
        Lane::Normal,
        at,
        Operation::State { target: id(0) },
        Box::new([7; 64]),
    );
    h.step(1);
    h.drain();
    assert_eq!(
        h.reply(bad),
        Reply::Failed(Failure::Timeline(shoop_loops::Error::InvalidProgram))
    );
    assert_eq!(
        h.reply(status),
        Reply::TopologyStatus {
            id: malformed,
            status: Status::Receiving {
                received: 0,
                total: source.encoded_records()
            }
        }
    );
    assert_eq!(
        h.reply(payload),
        Reply::Failed(Failure::Runtime(Error::InvalidShape))
    );
    let mut bytes = image(&source, 0, source.encoded_records());
    bytes[4 * 64 + 12..4 * 64 + 16].fill(0);
    let at = h.engine.next_frame().0;
    h.send(
        Lane::Normal,
        at,
        Operation::TopologyAppend {
            id: malformed,
            first_record: 0,
        },
        bytes,
    );
    let failed = h.op(at, Operation::TopologyValidate { id: malformed });
    let status = h.op(at, Operation::TopologyStatus { id: malformed });
    let not_active = h.op(at, activate(malformed, 1));
    let cut = h.op(at, Operation::State { target: id(0) });
    h.step(1);
    h.drain();
    assert_eq!(
        h.reply(failed),
        Reply::Failed(Failure::Timeline(shoop_loops::Error::InvalidProgram))
    );
    assert_eq!(
        h.reply(status),
        Reply::TopologyStatus {
            id: malformed,
            status: Status::Failed(shoop_loops::Error::InvalidProgram)
        }
    );
    assert_eq!(
        h.reply(not_active),
        Reply::Failed(Failure::Runtime(Error::InvalidState))
    );
    let Reply::State(cut) = h.reply(cut) else {
        panic!("cut")
    };
    assert_eq!(cut.topology_epoch, Epoch(1));
    assert_eq!(cut.state.mode, Mode::Playing);
    assert_eq!(cut.state.cycles, at);
    assert_eq!(cut.state.last_pulse, Some(Frame(at)));
    h.finish();
}

#[shoop_test]
fn topology_revalidation_completes_countdown_receipts_and_cancelled_delivery_keeps_the_candidate() {
    let mut h = Harness::new(false, 0, false, staging());
    h.op(0, play(0));
    let pending = h.op(
        0,
        Operation::Transition {
            target: id(1),
            timing: Timing::AfterTicks {
                source: id(0),
                skip: u64::MAX,
            },
            transition: Transition::Play {
                position: 0,
                retrigger: true,
            },
        },
    );
    let mut changed = description(true, false);
    changed.loops[1].sync_source = None;
    let candidate = compile(limits(0), &changed).unwrap();
    let staged = h.ready(0, &candidate);
    assert!(h.replies.iter().all(|r| r.sequence != pending));
    let at = h.engine.next_frame().0;
    let ticket = assert_no_heap(|| {
        h.control
            .submit(Lane::Normal, Frame(at), activate(staged, 0))
    })
    .unwrap_or_else(|e| panic!("{:?}", e.error));
    ticket.cancel();
    h.step(1);
    let completion = assert_no_heap(|| h.control.try_completion()).unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    assert_eq!(completion.outcome, TransportOutcome::Cancelled);
    assert!(assert_no_heap(|| h.control.try_execution()).is_none());
    let at = h.engine.next_frame().0;
    let status = h.op(at, Operation::TopologyStatus { id: staged });
    let applied = h.op(at, activate(staged, 0));
    h.step(1);
    h.drain();
    assert_eq!(
        h.reply(status),
        Reply::TopologyStatus {
            id: staged,
            status: Status::Ready
        }
    );
    assert!(matches!(
        h.reply(applied),
        Reply::TopologyActivated {
            topology_epoch: Epoch(1),
            ..
        }
    ));
    assert!(
        matches!(h.reply(pending),Reply::Transition(Completion {outcome:Outcome::Failed {frame,reason:shoop_loops::Error::MissingSync},..}) if frame==Frame(at))
    );
    let terminal = h.replies.iter().find(|r| r.sequence == pending).unwrap();
    assert_eq!(terminal.requested_frame, Frame(0));
    assert_eq!(terminal.delivered_frame, Some(Frame(0)));
    assert_eq!(terminal.actual_frame, Frame(at));
    h.finish();
}

#[shoop_test]
fn stopped_extraction_recovers_receiving_validating_ready_failed_and_retired_banks() {
    for phase in 0..5 {
        let mut h = Harness::new(false, 0, false, staging());
        let source = compile(limits(0), &description(true, false)).unwrap();
        let staged = h.begin(0, &source);
        if phase != 0 {
            let mut bytes = image(&source, 0, source.encoded_records());
            if phase == 3 {
                bytes[4 * 64 + 12..4 * 64 + 16].fill(0);
            }
            let at = h.engine.next_frame().0;
            h.send(
                Lane::Normal,
                at,
                Operation::TopologyAppend {
                    id: staged,
                    first_record: 0,
                },
                bytes,
            );
            if phase != 1 {
                h.op(at, Operation::TopologyValidate { id: staged });
            }
            h.step(1);
            h.drain();
            if phase == 4 {
                let at = h.engine.next_frame().0;
                h.op(at, activate(staged, 0));
                h.step(1);
                h.drain();
            }
        }
        let at = h.engine.next_frame().0;
        let status = h.op(at, Operation::TopologyStatus { id: staged });
        h.step(1);
        h.drain();
        let expected = match phase {
            0 => Status::Receiving {
                received: 0,
                total: source.encoded_records(),
            },
            1 => Status::Validating,
            2 => Status::Ready,
            3 => Status::Failed(shoop_loops::Error::InvalidProgram),
            4 => Status::Taken,
            _ => unreachable!(),
        };
        assert_eq!(
            h.reply(status),
            Reply::TopologyStatus {
                id: staged,
                status: expected
            }
        );
        h.finish();
    }
}

fn read_export(h: &mut Harness, export: media::ExportId, midi: bool) -> Vec<u8> {
    let at = h.engine.next_frame().0;
    let seq = h.op(at, Operation::Cursor { export, midi });
    h.step(1);
    h.drain();
    let Reply::Cursor(mut cursor) = h.reply(seq) else {
        panic!("cursor")
    };
    let mut output = Vec::new();
    loop {
        let bytes = vec![0; 64].into_boxed_slice();
        let ticket = assert_no_heap(|| h.control.read(cursor, bytes))
            .unwrap_or_else(|e| panic!("{:?}", e.error));
        h.step(1);
        let completion = assert_no_heap(|| h.control.try_completion()).unwrap();
        assert_eq!(completion.sequence, ticket.sequence());
        assert_eq!(completion.outcome, TransportOutcome::Applied);
        let Command::Resource(command) = completion.request else {
            panic!("owner")
        };
        let resource::Reply::Read { written, eof } = command.reply().unwrap() else {
            panic!("read")
        };
        let resource::Request::Read {
            cursor: next,
            bytes,
        } = command.into_request()
        else {
            panic!("buffer")
        };
        assert_eq!(next.position(), cursor.position() + written);
        cursor = next;
        output.extend_from_slice(&bytes[..written]);
        if eof {
            break;
        }
        assert!(written > 0);
    }
    output
}
#[shoop_test]
fn timing_activation_during_capture_keeps_the_take_prefix_and_recorded_fx_media_coherent() {
    for parts in [vec![9], vec![1; 9], vec![2, 3, 4]] {
        let mut h = Harness::new(true, 0, false, staging());
        let mut changed = description(false, false);
        changed.loops[1].sync_source = None;
        let candidate = compile(limits(0), &changed).unwrap();
        let staged = h.ready(0, &candidate);
        h.op(2, play(0));
        h.op(
            2,
            Operation::Transition {
                target: id(1),
                timing: Timing::At,
                transition: Transition::Record {
                    monitor_previous: true,
                },
            },
        );
        let prefix = h.op(
            4,
            Operation::Export {
                target: id(1),
                selection: Selection::Recording,
            },
        );
        let applied = h.op(4, activate(staged, 0));
        let during = h.op(4, Operation::State { target: id(1) });
        h.op(6, play(1));
        let frozen = h.op(
            6,
            Operation::Export {
                target: id(1),
                selection: Selection::Latest,
            },
        );
        h.op(10, stop(0));
        h.op(10, stop(1));
        for frames in parts {
            h.step(frames);
        }
        h.drain();
        assert!(matches!(
            h.reply(applied),
            Reply::TopologyActivated {
                topology_epoch: Epoch(1),
                ..
            }
        ));
        let Reply::State(cut) = h.reply(during) else {
            panic!("cut")
        };
        assert_eq!(cut.frame, Frame(4));
        assert_eq!(cut.topology_epoch, Epoch(1));
        assert_eq!(cut.state.mode, Mode::Recording);
        assert_eq!(cut.state.capture_frames, 2);
        assert_eq!(cut.state.capture_revision, Revision(1));
        assert_eq!(cut.state.content_revision, Revision(0));
        assert_eq!(cut.state.position, 2);
        assert!(cut.state.monitor_previous);
        assert_eq!(cut.capture_failure, None);
        let expected: Vec<_> = [0f32, 0., 2., 4., 6., 8., 1., 2., 3., 4., 0.]
            .into_iter()
            .map(|v| [v, 10. * v])
            .collect();
        assert_eq!(h.samples, expected);
        let mut originals = Vec::new();
        restore(0, &mut originals);
        originals.push((2, vec![0x80, 60, 45]));
        let mut packets: Vec<_> = originals
            .iter()
            .map(|(at, p)| (*at + 2, p.clone()))
            .collect();
        let mut initial_restore = Vec::new();
        restore(6, &mut initial_restore);
        initial_restore.pop();
        packets.extend(initial_restore);
        packets.extend(originals.iter().map(|(at, p)| (*at + 6, p.clone())));
        assert_eq!(h.packets, packets);
        let mut ids = Vec::new();
        for (seq, frames, at) in [(prefix, 2, 4), (frozen, 4, 6)] {
            let Reply::Export { id, cut, media } = h.reply(seq) else {
                panic!("export")
            };
            assert_eq!(cut.frame, Frame(at));
            assert_eq!(cut.topology_epoch, Epoch(1));
            assert_eq!(media.view.description.frames, frames);
            assert_eq!(media.view.description.revision, Revision(1));
            for source in [media.view.audio.unwrap(), media.view.midi.unwrap()] {
                assert_eq!(source.snapshot.cut_frame, Frame(at));
                assert_eq!(source.snapshot.description.frames, frames);
            }
            let audio: Vec<_> = [2f32, 4., 6., 8.]
                .into_iter()
                .take(frames as usize)
                .flat_map(|v| [v, v * 10.])
                .flat_map(f32::to_le_bytes)
                .collect();
            assert_eq!(read_export(&mut h, id, false), audio);
            let midi: Vec<_> = originals
                .iter()
                .filter(|(at, _)| *at < frames)
                .flat_map(|(at, bytes)| {
                    at.to_le_bytes()
                        .into_iter()
                        .chain((bytes.len() as u32).to_le_bytes())
                        .chain(bytes.iter().copied())
                })
                .collect();
            assert_eq!(read_export(&mut h, id, true), midi);
            let now = h.engine.next_frame().0;
            let mut queries = Vec::new();
            for channel in 0..16 {
                queries.push((
                    channel,
                    h.op(
                        now,
                        Operation::MidiInitial {
                            export: id,
                            channel,
                        },
                    ),
                ));
            }
            h.step(1);
            h.drain();
            let initial = MidiState::default();
            for (channel, sequence) in queries {
                assert_eq!(
                    h.reply(sequence),
                    Reply::MidiInitial {
                        channel,
                        state: *initial.channel(channel).unwrap(),
                        system_reset_seen: false
                    }
                );
            }
            ids.push(id);
        }
        let at = h.engine.next_frame().0;
        for export in ids {
            h.op(at, Operation::Release { export });
        }
        h.step(1);
        h.drain();
        h.finish();
    }
}

#[shoop_test]
fn staged_fixed_graph_activation_preserves_media_and_matches_independent_timeline() {
    for (parts, journal, reverse) in [
        (vec![11], 1024, false),
        (vec![1; 11], 0, true),
        (vec![3, 2, 6], 1, true),
    ] {
        let mut h = Harness::new(true, journal, reverse, staging());
        let candidate = compile(limits(journal), &description(true, reverse)).unwrap();
        let candidate_id = h.ready(0, &candidate);
        assert_eq!(h.engine.next_frame(), Frame(2));
        let mut transitions = Vec::new();
        for (at, op) in [
            (2, play(0)),
            (2, play(2)),
            (5, stop(2)),
            (6, play(2)),
            (12, stop(0)),
            (12, stop(2)),
        ] {
            transitions.push((h.op(at, op), at));
        }
        let running = h.op(3, activate(candidate_id, 0));
        let retained = h.op(4, Operation::TopologyStatus { id: candidate_id });
        let mut cuts = Vec::new();
        for frame in 2..=12 {
            cuts.push((frame, h.op(frame, Operation::State { target: id(2) })));
        }
        let before = h.op(
            4,
            Operation::Export {
                target: id(1),
                selection: Selection::Playback,
            },
        );
        let bad_graph = h.op(
            6,
            Operation::TopologyActivate {
                id: candidate_id,
                graph_epoch: Epoch(2),
                topology_epoch: Epoch(0),
            },
        );
        let bad_topology = h.op(6, activate(candidate_id, 1));
        let applied = h.op(6, activate(candidate_id, 0));
        let retired = h.op(7, Operation::TopologyStatus { id: candidate_id });
        let after = h.op(
            8,
            Operation::Export {
                target: id(1),
                selection: Selection::Playback,
            },
        );
        let recycled = h.op(9, Operation::TopologyRecycle { id: candidate_id });
        for frames in parts {
            h.step(frames);
        }
        h.drain();
        assert!(matches!(
            h.reply(running),
            Reply::Failed(Failure::Timeline(shoop_loops::Error::InvalidState))
        ));
        assert_eq!(
            h.reply(retained),
            Reply::TopologyStatus {
                id: candidate_id,
                status: Status::Ready
            }
        );
        assert_eq!(
            h.reply(bad_graph),
            Reply::Failed(Failure::Runtime(Error::InvalidState))
        );
        assert_eq!(
            h.reply(bad_topology),
            Reply::Failed(Failure::Timeline(shoop_loops::Error::InvalidState))
        );
        assert_eq!(
            h.reply(applied),
            Reply::TopologyActivated {
                id: candidate_id,
                graph_epoch: Epoch(1),
                topology_epoch: Epoch(1)
            }
        );
        assert_eq!(
            h.reply(retired),
            Reply::TopologyStatus {
                id: candidate_id,
                status: Status::Taken
            }
        );
        assert_eq!(
            h.reply(recycled),
            Reply::TopologyStatus {
                id: candidate_id,
                status: Status::Idle
            }
        );
        for (sequence, at) in transitions {
            assert!(
                matches!(h.reply(sequence),Reply::Transition(Completion {outcome:Outcome::Applied {frame},..}) if frame==Frame(at))
            );
        }
        for (frame, sequence) in cuts {
            let Reply::State(cut) = h.reply(sequence) else {
                panic!("cut")
            };
            assert_eq!(cut.frame, Frame(frame));
            assert_eq!(cut.graph_epoch, Epoch(1));
            assert_eq!(cut.topology_epoch, Epoch(u64::from(frame >= 6)));
            assert_eq!(cut.state.id, id(2));
            assert_eq!(cut.state.content_length, if frame < 6 { 4 } else { 3 });
            assert_eq!(
                cut.state.mode,
                if [5, 12].contains(&frame) {
                    Mode::Stopped
                } else {
                    Mode::Playing
                }
            );
            let cursor = cut.state.composite.unwrap();
            assert_eq!(
                cursor.iteration,
                if [5, 12].contains(&frame) {
                    0
                } else if frame < 5 {
                    frame - 2
                } else {
                    (frame - 6) % 3
                }
            );
            assert_eq!(cursor.passes, u64::from(frame >= 9));
            assert_eq!(cursor.entered_at, Some(Frame(frame)));
            assert_eq!(cut.state.position, 0);
            assert_eq!(cut.state.cycles, 0);
            assert_eq!(cut.state.content_revision, Revision(0));
            assert_eq!(cut.state.capture_frames, 0);
            assert!(!cut.state.capture_incomplete);
            assert!(!cut.state.monitor_previous);
            assert_eq!(cut.playback, None);
            assert_eq!(cut.latest, None);
            assert_eq!(cut.capture_failure, None);
            assert_eq!(cut.midi_overflows, 0);
        }
        let expected: Vec<_> = [0f32, 0., 2., 4., 6., 0., 0., 2., 4., 0., 2., 4., 0.]
            .into_iter()
            .map(|v| [v, v * 10.])
            .collect();
        assert_eq!(h.samples, expected);
        let mut midi = Vec::new();
        restore(2, &mut midi);
        midi.push((4, vec![0x80, 60, 45]));
        restore(7, &mut midi);
        midi.push((9, vec![0x80, 60, 0]));
        restore(10, &mut midi);
        midi.push((12, vec![0x80, 60, 0]));
        assert_eq!(h.packets, midi);
        let mut exports = Vec::new();
        for (seq, epoch, frame) in [(before, 0, 4), (after, 1, 8)] {
            let Reply::Export { id, cut, media } = h.reply(seq) else {
                panic!("export")
            };
            assert_eq!(cut.topology_epoch, Epoch(epoch));
            assert_eq!(cut.frame, Frame(frame));
            assert_eq!(media.view.description.frames, 4);
            assert_eq!(media.requested_cut, Frame(frame));
            exports.push((id, media.media));
        }
        assert_eq!(exports[0].1, exports[1].1);
        for (export, _) in exports {
            let cursor_seq = h.op(
                h.engine.next_frame().0,
                Operation::Cursor {
                    export,
                    midi: false,
                },
            );
            h.step(1);
            h.drain();
            let Reply::Cursor(cursor) = h.reply(cursor_seq) else {
                panic!("cursor")
            };
            let bytes = vec![0; 32].into_boxed_slice();
            let ticket = assert_no_heap(|| h.control.read(cursor, bytes))
                .unwrap_or_else(|e| panic!("{:?}", e.error));
            h.step(1);
            let completion = assert_no_heap(|| h.control.try_completion()).unwrap();
            assert_eq!(completion.sequence, ticket.sequence());
            let Command::Resource(command) = completion.request else {
                panic!("read owner")
            };
            assert_eq!(completion.outcome, TransportOutcome::Applied);
            assert!(matches!(
                command.reply(),
                Some(resource::Reply::Read {
                    written: 32,
                    eof: true
                })
            ));
            let resource::Request::Read { bytes, .. } = command.into_request() else {
                panic!("read bytes")
            };
            let expected: Vec<_> = [4f32, 8., 12., 16.]
                .into_iter()
                .flat_map(|v| [v, v * 10.])
                .flat_map(f32::to_le_bytes)
                .collect();
            assert_eq!(&*bytes, &expected);
            let at = h.engine.next_frame().0;
            h.op(at, Operation::Release { export });
            h.step(1);
            h.drain();
        }
        let log = h.finish().unwrap();
        let complete = expected_events();
        let mut expected = Vec::new();
        for frame in 2..=12 {
            expected.extend(
                complete
                    .iter()
                    .filter(|e| e.frame == Frame(frame))
                    .take(journal)
                    .copied(),
            );
        }
        assert_eq!(log.events, expected);
        assert_eq!(log.dropped, (complete.len() - expected.len()) as u64);
        #[derive(Debug, PartialEq)]
        struct Musical {
            mode: Mode,
            position: u64,
            cycles: u64,
            last_pulse: Option<Frame>,
            composite: Option<state::CompositeState>,
        }
        for (frame, states) in log.states.iter().enumerate() {
            let frame = frame as u64;
            for (slot, actual) in states.iter().enumerate() {
                let Some(actual) = actual else { continue };
                assert_eq!(actual.id, id(slot as u32));
                assert_eq!(
                    actual.content_length,
                    if slot == 0 {
                        1
                    } else if slot == 1 || frame < 6 {
                        4
                    } else {
                        3
                    }
                );
                assert_eq!(actual.content_revision, Revision(0));
                assert_eq!(actual.capture_frames, 0);
                assert_eq!(actual.capture_revision, Revision(0));
                assert!(!actual.capture_incomplete);
                assert!(!actual.monitor_previous);
                let mut expected = Musical {
                    mode: Mode::Stopped,
                    position: 0,
                    cycles: 0,
                    last_pulse: None,
                    composite: None,
                };
                match slot {
                    0 => {
                        expected.mode = if (2..12).contains(&frame) {
                            Mode::Playing
                        } else {
                            Mode::Stopped
                        };
                        expected.cycles = frame.saturating_sub(2).min(9);
                        expected.last_pulse = (frame >= 2).then_some(Frame(frame.min(11)));
                    }
                    1 => {
                        let start = if (2..5).contains(&frame) {
                            Some(2)
                        } else if (7..9).contains(&frame) {
                            Some(7)
                        } else if (10..12).contains(&frame) {
                            Some(10)
                        } else {
                            None
                        };
                        if let Some(start) = start {
                            expected.mode = Mode::Playing;
                            expected.position = frame - start;
                        }
                        expected.last_pulse = match frame {
                            0..=1 => None,
                            2..=4 => Some(Frame(frame)),
                            5..=6 => Some(Frame(4)),
                            7..=8 => Some(Frame(frame)),
                            9 => Some(Frame(8)),
                            _ => Some(Frame(frame.min(11))),
                        };
                    }
                    2 => {
                        expected.mode = if (2..5).contains(&frame) || (6..12).contains(&frame) {
                            Mode::Playing
                        } else {
                            Mode::Stopped
                        };
                        expected.last_pulse = if frame < 2 {
                            None
                        } else if frame == 5 {
                            Some(Frame(4))
                        } else {
                            Some(Frame(frame.min(11)))
                        };
                        expected.composite = Some(state::CompositeState {
                            iteration: if frame < 2 || frame == 5 || frame >= 12 {
                                0
                            } else if frame < 5 {
                                frame - 2
                            } else {
                                (frame - 6) % 3
                            },
                            passes: u64::from(frame >= 9),
                            entered_at: (frame >= 2).then_some(Frame(frame.min(12))),
                        });
                    }
                    _ => unreachable!(),
                }
                assert_eq!(
                    Musical {
                        mode: actual.mode,
                        position: actual.position,
                        cycles: actual.cycles,
                        last_pulse: actual.last_pulse,
                        composite: actual.composite
                    },
                    expected,
                    "slot {slot} frame {frame}"
                );
            }
        }
    }
}

#[shoop_test]
fn running_program_bank_activation_matches_independent_media_states_and_ordered_journals() {
    for (parts, journal, reverse) in [
        (vec![11], 1024, false),
        (vec![1; 11], 0, true),
        (vec![2, 5, 1, 3], 1, false),
        (vec![1; 11], 1024, true),
    ] {
        let mut h = Harness::new(true, journal, reverse, staging());
        let candidate = compile(limits(journal), &description(true, reverse)).unwrap();
        let staged = h.ready(0, &candidate);
        h.op(2, play(0));
        h.op(2, play(2));
        let change = h.op(3, program_activate(staged));
        let exported = h.op(
            6,
            Operation::Export {
                target: id(1),
                selection: Selection::Playback,
            },
        );
        let state = h.op(6, Operation::State { target: id(2) });
        h.op(12, stop(0));
        h.op(12, stop(2));
        for frames in parts {
            h.step(frames);
        }
        h.drain();
        let Reply::ProgramChanged {
            id: actual,
            graph_epoch,
            topology_epoch,
            completion,
        } = h.reply(change)
        else {
            panic!("program receipt")
        };
        assert_eq!(
            (actual, graph_epoch, topology_epoch),
            (staged, Epoch(1), Epoch(1))
        );
        assert_eq!(
            (
                completion.sequence,
                completion.requested,
                completion.effective,
                completion.outcome
            ),
            (
                change,
                Frame(3),
                Frame(3),
                Outcome::Applied { frame: Frame(6) }
            )
        );
        let execution = h.replies.iter().find(|r| r.sequence == change).unwrap();
        assert_eq!(execution.delivered_frame, Some(Frame(3)));
        assert_eq!(execution.actual_frame, Frame(6));
        let Reply::State(cut) = h.reply(state) else {
            panic!("state")
        };
        assert_eq!(
            (
                cut.frame,
                cut.graph_epoch,
                cut.topology_epoch,
                cut.state.content_length
            ),
            (Frame(6), Epoch(1), Epoch(1), 3)
        );
        assert_eq!(cut.state.composite.unwrap().iteration, 0);
        assert_eq!(cut.state.composite.unwrap().passes, 1);
        let expected: Vec<_> = [0f32, 0., 2., 4., 6., 8., 0., 2., 4., 0., 2., 4., 0.]
            .into_iter()
            .map(|v| [v, v * 10.])
            .collect();
        assert_eq!(h.samples, expected);
        let mut packets = Vec::new();
        restore(2, &mut packets);
        packets.push((4, vec![0x80, 60, 45]));
        restore(7, &mut packets);
        packets.push((9, vec![0x80, 60, 0]));
        restore(10, &mut packets);
        packets.push((12, vec![0x80, 60, 0]));
        assert_eq!(h.packets, packets);
        let Reply::Export {
            id: export,
            cut,
            media,
        } = h.reply(exported)
        else {
            panic!("export")
        };
        assert_eq!(
            (cut.frame, cut.topology_epoch, cut.state.mode),
            (Frame(6), Epoch(1), Mode::Stopped)
        );
        assert_eq!(
            (
                media.view.description.frames,
                media.view.description.revision
            ),
            (4, Revision(0))
        );
        let audio: Vec<_> = [4f32, 8., 12., 16.]
            .into_iter()
            .flat_map(|v| [v, v * 10.])
            .flat_map(f32::to_le_bytes)
            .collect();
        assert_eq!(read_export(&mut h, export, false), audio);
        assert_eq!(read_export(&mut h, export, true), encoded_midi());
        let at = h.engine.next_frame().0;
        h.op(at, Operation::Release { export });
        let recycle = h.op(at, Operation::TopologyRecycle { id: staged });
        h.step(1);
        h.drain();
        assert_eq!(
            h.reply(recycle),
            Reply::TopologyStatus {
                id: staged,
                status: Status::Idle
            }
        );
        let log = h.finish().unwrap();
        let mut complete = Vec::new();
        for f in 2..=12 {
            let mut push = |target, kind| {
                complete.push(Event {
                    frame: Frame(f),
                    target: id(target),
                    kind,
                })
            };
            if f == 6 {
                push(2, EventKind::Transition(Transition::Program));
            }
            if f == 12 {
                push(0, EventKind::Transition(Transition::Stop));
                push(2, EventKind::Transition(Transition::Stop));
            } else {
                push(
                    0,
                    if f == 2 {
                        EventKind::Transition(Transition::Play {
                            position: 0,
                            retrigger: true,
                        })
                    } else {
                        EventKind::Wrap
                    },
                );
                push(0, EventKind::Pulse);
                if f == 2 {
                    push(
                        2,
                        EventKind::Transition(Transition::Play {
                            position: 0,
                            retrigger: true,
                        }),
                    );
                }
                push(2, EventKind::Pulse);
                push(
                    2,
                    EventKind::Iteration {
                        iteration: if f < 6 { f - 2 } else { (f - 6) % 3 },
                        passes: if f < 6 { 0 } else { 1 + (f - 6) / 3 },
                    },
                );
            }
            if [2, 7, 10].contains(&f) {
                push(
                    1,
                    EventKind::Transition(Transition::Play {
                        position: 0,
                        retrigger: true,
                    }),
                );
            }
            if [6, 9, 12].contains(&f) {
                push(1, EventKind::Transition(Transition::Stop));
            } else {
                push(1, EventKind::Pulse);
            }
        }
        let mut expected = Vec::new();
        for f in 2..=12 {
            expected.extend(
                complete
                    .iter()
                    .filter(|e| e.frame == Frame(f))
                    .take(journal)
                    .copied(),
            );
        }
        assert_eq!(log.events, expected);
        assert_eq!(log.dropped, (complete.len() - expected.len()) as u64);
        for f in 0usize..=12 {
            for slot in 0..3 {
                let mut expected = log.states[0][slot].unwrap();
                match slot {
                    0 => {
                        expected.mode = if (2..12).contains(&f) {
                            Mode::Playing
                        } else {
                            Mode::Stopped
                        };
                        expected.cycles = f.saturating_sub(2).min(9) as u64;
                        expected.last_pulse = (f >= 2).then_some(Frame(f.min(11) as u64));
                    }
                    1 => {
                        let start = if (2..6).contains(&f) {
                            Some(2)
                        } else if (7..9).contains(&f) {
                            Some(7)
                        } else if (10..12).contains(&f) {
                            Some(10)
                        } else {
                            None
                        };
                        if let Some(start) = start {
                            expected.mode = Mode::Playing;
                            expected.position = (f - start) as u64;
                        }
                        expected.last_pulse = match f {
                            0..=1 => None,
                            2..=5 => Some(Frame(f as u64)),
                            6 => Some(Frame(5)),
                            7..=8 => Some(Frame(f as u64)),
                            9 => Some(Frame(8)),
                            _ => Some(Frame(f.min(11) as u64)),
                        };
                    }
                    2 => {
                        expected.mode = if (2..12).contains(&f) {
                            Mode::Playing
                        } else {
                            Mode::Stopped
                        };
                        expected.content_length = if f < 6 { 4 } else { 3 };
                        expected.last_pulse = (f >= 2).then_some(Frame(f.min(11) as u64));
                        expected.composite = Some(state::CompositeState {
                            iteration: if f < 2 || f == 12 {
                                0
                            } else if f < 6 {
                                (f - 2) as u64
                            } else {
                                ((f - 6) % 3) as u64
                            },
                            passes: if f < 6 {
                                0
                            } else {
                                (1 + (f.min(11) - 6) / 3) as u64
                            },
                            entered_at: (f >= 2).then_some(Frame(f as u64)),
                        });
                    }
                    _ => unreachable!(),
                }
                assert_eq!(
                    log.states[f][slot].unwrap(),
                    expected,
                    "frame {f} slot {slot}"
                );
            }
        }
    }
}

#[shoop_test]
fn program_capture_preflight_uses_real_banks_and_preserves_old_media_on_family_refusal() {
    for refused in [false, true] {
        for parts in [vec![11], vec![1; 11], vec![3, 2, 6]] {
            let mut h = Harness::options(
                true,
                1024,
                false,
                staging(),
                if refused { 2 } else { 8 },
                None,
            );
            let mut changed = description(false, false);
            changed.programs[0].sections[0].entries = vec![
                Entry {
                    target: id(1),
                    delay: 0,
                    duration: Some(2),
                    repetitions: 1,
                    mode: Some(Mode::Recording),
                },
                Entry {
                    target: id(1),
                    delay: 2,
                    duration: Some(2),
                    repetitions: 1,
                    mode: Some(Mode::Playing),
                },
            ];
            let candidate = compile(limits(1024), &changed).unwrap();
            let staged = h.ready(0, &candidate);
            h.op(2, play(0));
            h.op(2, play(2));
            let change = h.op(3, program_activate(staged));
            let state = h.op(6, Operation::State { target: id(1) });
            let prefix = h.op(
                7,
                Operation::Export {
                    target: id(1),
                    selection: Selection::Recording,
                },
            );
            let frozen = h.op(
                8,
                Operation::Export {
                    target: id(1),
                    selection: if refused {
                        Selection::Playback
                    } else {
                        Selection::Latest
                    },
                },
            );
            h.op(12, stop(0));
            h.op(12, stop(2));
            for frames in parts {
                h.step(frames);
            }
            h.drain();
            let Reply::ProgramChanged {
                topology_epoch,
                completion,
                ..
            } = h.reply(change)
            else {
                panic!("change")
            };
            assert_eq!(topology_epoch, Epoch(u64::from(!refused)));
            assert_eq!(
                completion.outcome,
                if refused {
                    Outcome::Failed {
                        frame: Frame(6),
                        reason: shoop_loops::Error::Capacity,
                    }
                } else {
                    Outcome::Applied { frame: Frame(6) }
                }
            );
            let Reply::State(cut) = h.reply(state) else {
                panic!("cut")
            };
            assert_eq!(cut.topology_epoch, topology_epoch);
            assert_eq!(
                cut.state.mode,
                if refused {
                    Mode::Playing
                } else {
                    Mode::Recording
                }
            );
            assert_eq!(cut.state.capture_revision, Revision(u64::from(!refused)));
            assert_eq!(cut.state.capture_frames, 0);
            assert_eq!(cut.state.content_revision, Revision(0));
            assert_eq!(cut.capture_failure, refused.then_some(Error::Capacity));
            let values = if refused {
                [0f32, 0., 2., 4., 6., 8., 2., 4., 6., 8., 2., 4., 0.]
            } else {
                [0f32, 0., 2., 4., 6., 8., 0., 0., 0., 0., 0., 0., 0.]
            };
            assert_eq!(
                h.samples,
                values.into_iter().map(|v| [v, v * 10.]).collect::<Vec<_>>()
            );
            let mut packets = Vec::new();
            restore(2, &mut packets);
            packets.push((4, vec![0x80, 60, 45]));
            if refused {
                restore(6, &mut packets);
                packets.push((8, vec![0x80, 60, 45]));
                restore(10, &mut packets);
                packets.push((12, vec![0x80, 60, 0]));
            } else {
                restore(8, &mut packets);
                packets.pop();
            }
            assert_eq!(h.packets, packets);
            let mut exports = Vec::new();
            if refused {
                assert_eq!(
                    h.reply(prefix),
                    Reply::Failed(Failure::Runtime(Error::InvalidState))
                );
            } else {
                let Reply::Export { id, cut, media } = h.reply(prefix) else {
                    panic!("prefix")
                };
                assert_eq!(
                    (
                        cut.frame,
                        media.view.description.frames,
                        media.view.description.revision
                    ),
                    (Frame(7), 1, Revision(1))
                );
                assert_eq!(read_export(&mut h, id, false), vec![0; 8]);
                assert_eq!(read_export(&mut h, id, true), Vec::<u8>::new());
                exports.push(id);
            }
            let Reply::Export { id, cut, media } = h.reply(frozen) else {
                panic!("frozen")
            };
            assert_eq!(cut.frame, Frame(8));
            assert_eq!(
                (
                    media.view.description.frames,
                    media.view.description.revision
                ),
                if refused {
                    (4, Revision(0))
                } else {
                    (2, Revision(1))
                }
            );
            if !refused {
                assert_eq!(read_export(&mut h, id, false), vec![0; 16]);
                assert_eq!(read_export(&mut h, id, true), Vec::<u8>::new());
            }
            exports.push(id);
            let at = h.engine.next_frame().0;
            for export in exports {
                h.op(at, Operation::Release { export });
            }
            let status = h.op(at, Operation::TopologyStatus { id: staged });
            h.step(1);
            h.drain();
            assert_eq!(
                h.reply(status),
                Reply::TopologyStatus {
                    id: staged,
                    status: if refused {
                        Status::Ready
                    } else {
                        Status::Taken
                    }
                }
            );
            let log = h.finish().unwrap();
            assert_eq!(
                log.events
                    .iter()
                    .filter(|e| e.kind == EventKind::Transition(Transition::Program))
                    .count(),
                usize::from(!refused)
            );
            if refused {
                assert!(!log
                    .events
                    .iter()
                    .any(|e| matches!(e.kind, EventKind::Transition(Transition::Record { .. }))));
            }
        }
    }
}

#[shoop_test]
fn pending_program_cancellation_disconnect_and_fault_extraction_return_every_bank_owner() {
    for phase in 0..5 {
        let fault = (phase == 4).then_some(Frame(6));
        let mut h = Harness::options(phase == 4, 0, false, staging(), 8, fault);
        let candidate = compile(limits(0), &description(true, false)).unwrap();
        let staged = h.ready(0, &candidate);
        h.op(2, play(0));
        h.op(2, play(2));
        let change = h.op(3, program_activate(staged));
        if phase != 4 {
            h.op(4, stop(0));
        }
        h.step(3);
        h.drain();
        assert!(!h.replies.iter().any(|r| r.sequence == change));
        if phase < 2 {
            let op = if phase == 0 {
                Operation::Cancel { sequence: change }
            } else {
                Operation::TopologyCancel { id: staged }
            };
            h.send(Lane::Recovery, 5, op, Box::new([]));
            h.step(1);
            h.drain();
            let Reply::ProgramChanged {
                completion,
                topology_epoch,
                ..
            } = h.reply(change)
            else {
                panic!("cancelled")
            };
            assert_eq!(completion.outcome, Outcome::Cancelled { frame: Frame(5) });
            assert_eq!(topology_epoch, Epoch(0));
            let status = h.op(6, Operation::TopologyStatus { id: staged });
            h.step(1);
            h.drain();
            assert_eq!(
                h.reply(status),
                Reply::TopologyStatus {
                    id: staged,
                    status: if phase == 0 {
                        Status::Ready
                    } else {
                        Status::Idle
                    }
                }
            );
        } else if phase == 3 {
            // A second candidate changes the epoch while the first waits on a stopped clock.
            let replacement = compile(limits(0), &description(false, false)).unwrap();
            let other = h.ready(1, &replacement);
            let at = h.engine.next_frame().0;
            h.op(at, activate(other, 0));
            h.step(1);
            h.drain();
            let Reply::ProgramChanged {
                completion,
                topology_epoch,
                ..
            } = h.reply(change)
            else {
                panic!("stale")
            };
            assert_eq!(topology_epoch, Epoch(1));
            assert_eq!(
                completion.outcome,
                Outcome::Failed {
                    frame: Frame(at),
                    reason: shoop_loops::Error::InvalidState
                }
            );
        } else if phase == 4 {
            let [left, right] = &mut h.output;
            assert_eq!(
                assert_no_heap(|| h.engine.process(
                    AudioBlock {
                        frame: Frame(5),
                        frames: 3,
                        inputs: &[],
                        outputs: &mut [&mut left[..3], &mut right[..3]]
                    },
                    MidiBlock {
                        inputs: &[],
                        outputs: &mut h.midi
                    },
                )),
                Err(Error::InvalidState)
            );
            h.drain();
            let Reply::ProgramChanged {
                completion,
                topology_epoch,
                ..
            } = h.reply(change)
            else {
                panic!("published before render fault")
            };
            assert_eq!(topology_epoch, Epoch(1));
            assert_eq!(completion.outcome, Outcome::Applied { frame: Frame(6) });
        }
        // Includes a still-pending owner and an activated owner in an unfinished interval.
        h.finish();
    }
}

#[shoop_test]
fn program_delivery_validation_and_queued_cancellation_preserve_ready_bank_generations() {
    let mut h = Harness::new(false, 0, false, staging());
    let candidate = compile(limits(0), &description(true, false)).unwrap();
    let staged = h.ready(0, &candidate);
    let wrong_stage = StageId::from_parts(staged.parts()[0], staged.parts()[1] + 1).unwrap();
    for (operation, error) in [
        (
            Operation::ProgramActivate {
                id: staged,
                target: id(2),
                graph_epoch: Epoch(0),
                topology_epoch: Epoch(0),
            },
            Failure::Runtime(Error::InvalidState),
        ),
        (
            Operation::ProgramActivate {
                id: staged,
                target: id(2),
                graph_epoch: Epoch(1),
                topology_epoch: Epoch(1),
            },
            Failure::Timeline(shoop_loops::Error::InvalidState),
        ),
        (
            Operation::ProgramActivate {
                id: staged,
                target: id(1),
                graph_epoch: Epoch(1),
                topology_epoch: Epoch(0),
            },
            Failure::Timeline(shoop_loops::Error::InvalidProgram),
        ),
        (
            program_activate(wrong_stage),
            Failure::Timeline(shoop_loops::Error::InvalidId),
        ),
        (
            Operation::Transition {
                target: id(2),
                timing: Timing::At,
                transition: Transition::Program,
            },
            Failure::Runtime(Error::InvalidEvent),
        ),
    ] {
        let at = h.engine.next_frame().0;
        let rejected = h.op(at, operation);
        let status = h.op(at, Operation::TopologyStatus { id: staged });
        h.step(1);
        h.drain();
        assert_eq!(h.reply(rejected), Reply::Failed(error));
        assert_eq!(
            h.reply(status),
            Reply::TopologyStatus {
                id: staged,
                status: Status::Ready
            }
        );
    }
    let at = h.engine.next_frame();
    let ticket = assert_no_heap(|| h.control.submit(Lane::Normal, at, program_activate(staged)))
        .unwrap_or_else(|e| panic!("{:?}", e.error));
    ticket.cancel();
    h.step(1);
    let completion = assert_no_heap(|| h.control.try_completion()).unwrap();
    assert_eq!(
        (completion.sequence, completion.outcome),
        (ticket.sequence(), TransportOutcome::Cancelled)
    );
    assert!(assert_no_heap(|| h.control.try_execution()).is_none());
    let at = h.engine.next_frame().0;
    let applied = h.op(at, program_activate(staged));
    let state = h.op(at, Operation::State { target: id(2) });
    h.step(1);
    h.drain();
    assert!(
        matches!(h.reply(applied), Reply::ProgramChanged { topology_epoch: Epoch(1), completion: Completion { outcome: Outcome::Applied { frame }, .. }, .. } if frame == Frame(at))
    );
    let Reply::State(cut) = h.reply(state) else {
        panic!("cut")
    };
    assert_eq!(
        (
            cut.state.mode,
            cut.state.last_pulse,
            cut.state.content_length
        ),
        (Mode::Stopped, None, 3)
    );
    let at = h.engine.next_frame().0;
    h.op(at, Operation::TopologyRecycle { id: staged });
    h.step(1);
    h.drain();
    let replacement = h.ready(0, &candidate);
    assert_ne!(staged, replacement);
    let at = h.engine.next_frame().0;
    let stale = h.op(at, program_activate(staged));
    let status = h.op(at, Operation::TopologyStatus { id: replacement });
    h.step(1);
    h.drain();
    assert_eq!(
        h.reply(stale),
        Reply::Failed(Failure::Timeline(shoop_loops::Error::InvalidId))
    );
    assert_eq!(
        h.reply(status),
        Reply::TopologyStatus {
            id: replacement,
            status: Status::Ready
        }
    );
    h.finish();
}

#[shoop_test]
fn full_normal_domain_slots_and_unread_program_results_allow_clock_progress_and_recovery() {
    let mut h = Harness::new(false, 0, false, staging());
    let candidate = compile(limits(0), &description(true, false)).unwrap();
    let first = h.ready(0, &candidate);
    let second = h.ready(1, &candidate);
    h.op(4, play(0));
    h.op(4, play(2));
    h.step(1);
    let change = h.op(5, program_activate(first));
    let occupied = h.op(5, program_activate(second));
    let wait = Operation::Transition {
        target: id(1),
        timing: Timing::AfterTicks {
            source: id(0),
            skip: 10_000,
        },
        transition: Transition::Stop,
    };
    let mut waiting = Vec::new();
    for _ in 0..15 {
        waiting.push(h.op(5, wait));
    }
    h.step(4);
    waiting.push(h.op(9, wait));
    let full = h.op(10, program_activate(second));
    for frames in [32, 32, 32, 28] {
        h.step(frames);
    }
    assert_eq!(h.engine.next_frame(), Frame(133));
    h.send(
        Lane::Recovery,
        133,
        Operation::Cancel {
            sequence: waiting[0],
        },
        Box::new([]),
    );
    h.step(1);
    h.drain();
    assert_eq!(
        h.reply(occupied),
        Reply::Failed(Failure::Timeline(shoop_loops::Error::Capacity))
    );
    assert_eq!(
        h.reply(full),
        Reply::Failed(Failure::Runtime(Error::Capacity))
    );
    assert!(matches!(
        h.reply(change),
        Reply::ProgramChanged {
            topology_epoch: Epoch(1),
            completion: Completion {
                outcome: Outcome::Applied { frame: Frame(8) },
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        h.reply(waiting[0]),
        Reply::Transition(Completion {
            outcome: Outcome::Cancelled { frame: Frame(133) },
            ..
        })
    ));
    assert!(waiting[1..]
        .iter()
        .all(|seq| !h.replies.iter().any(|r| r.sequence == *seq)));
    let state = h.op(134, Operation::State { target: id(0) });
    h.step(1);
    h.drain();
    let Reply::State(cut) = h.reply(state) else {
        panic!("clock")
    };
    assert_eq!(
        (
            cut.frame,
            cut.state.mode,
            cut.state.cycles,
            cut.state.last_pulse
        ),
        (Frame(134), Mode::Playing, 130, Some(Frame(134)))
    );
    let retry = h.op(
        135,
        Operation::ProgramActivate {
            id: second,
            target: id(2),
            graph_epoch: Epoch(1),
            topology_epoch: Epoch(1),
        },
    );
    h.step(3);
    h.drain();
    assert!(matches!(
        h.reply(retry),
        Reply::ProgramChanged {
            topology_epoch: Epoch(2),
            completion: Completion {
                outcome: Outcome::Applied { frame: Frame(137) },
                ..
            },
            ..
        }
    ));
    // Both displaced banks and fifteen never-delivered musical requests remain retained.
    h.finish();
}

fn routes(media: bool, bypass: bool, reverse: bool) -> GraphDescription {
    let mut graph = GraphDescription {
        nodes: (0..if media && !bypass { 5 } else { 4 })
            .map(node)
            .collect(),
        connections: Vec::new(),
    };
    if media {
        for (kind, ports) in [(PortType::Audio, 2), (PortType::Midi, 1)] {
            for port in 0..ports {
                let source = if bypass {
                    node(2)
                } else {
                    graph.connections.push(Connection {
                        source: Endpoint::node(node(2), kind, port),
                        destination: Endpoint::node(node(4), kind, port),
                    });
                    node(4)
                };
                for destination in [
                    Endpoint::node(node(3), kind, port),
                    Endpoint::host(kind, port),
                ] {
                    graph.connections.push(Connection {
                        source: Endpoint::node(source, kind, port),
                        destination,
                    });
                }
            }
        }
    }
    if reverse {
        graph.nodes.reverse();
        graph.connections.reverse();
    }
    graph
}
fn live(media: bool, journal: usize, reverse: bool) -> Harness {
    let mut h = Harness::new(media, journal, reverse, staging());
    h.engine.prepare_graph_staging(2).unwrap();
    h
}
fn switch(graph: GraphStageId, epoch: u64, topology: u64) -> Operation {
    Operation::GraphActivate {
        id: graph,
        graph_epoch: Epoch(epoch),
        topology_epoch: Epoch(topology),
    }
}
fn pair(id: StageId, graph: GraphStageId, graph_epoch: u64, topology_epoch: u64) -> Operation {
    Operation::TopologyGraphActivate {
        id,
        graph,
        graph_epoch: Epoch(graph_epoch),
        topology_epoch: Epoch(topology_epoch),
    }
}
fn encoded_packets(packets: &[(u64, Vec<u8>)]) -> Vec<u8> {
    packets
        .iter()
        .flat_map(|(frame, bytes)| {
            frame
                .to_le_bytes()
                .into_iter()
                .chain((bytes.len() as u32).to_le_bytes())
                .chain(bytes.iter().copied())
        })
        .collect()
}
fn assert_pair_states(log: &Log) {
    for (frame, states) in log.states.iter().enumerate() {
        let f = (frame as u64).saturating_sub(1);
        for (slot, actual) in states.iter().enumerate() {
            let Some(s) = actual else { continue };
            assert_eq!(s.id, id(slot as u32));
            assert_eq!(
                s.content_length,
                if slot == 0 {
                    1
                } else if slot == 1 || f < 6 {
                    4
                } else {
                    3
                }
            );
            assert_eq!(
                (s.content_revision, s.capture_revision, s.capture_frames),
                (Revision(0), Revision(0), 0)
            );
            assert!(!s.capture_incomplete && !s.monitor_previous);
            let (mut mode, mut position, mut cycles, mut pulse, mut composite) =
                (Mode::Stopped, 0, 0, None, None);
            match slot {
                0 => {
                    if (2..12).contains(&f) {
                        mode = Mode::Playing;
                    }
                    cycles = f.saturating_sub(2).min(9);
                    pulse = (f >= 2).then_some(Frame(f.min(11) + 1));
                }
                1 => {
                    let start = if (2..5).contains(&f) {
                        Some(2)
                    } else if (7..9).contains(&f) {
                        Some(7)
                    } else if (10..12).contains(&f) {
                        Some(10)
                    } else {
                        None
                    };
                    if let Some(start) = start {
                        mode = Mode::Playing;
                        position = f - start;
                    }
                    pulse = match f {
                        0..=1 => None,
                        2..=4 | 7..=8 => Some(Frame(f + 1)),
                        5..=6 => Some(Frame(5)),
                        9 => Some(Frame(9)),
                        _ => Some(Frame(f.min(11) + 1)),
                    };
                }
                2 => {
                    if (2..5).contains(&f) || (6..12).contains(&f) {
                        mode = Mode::Playing;
                    }
                    if f >= 2 {
                        pulse = Some(Frame(if f == 5 { 5 } else { f.min(11) + 1 }));
                    }
                    composite = Some(state::CompositeState {
                        iteration: if f < 2 || f == 5 || f >= 12 {
                            0
                        } else if f < 5 {
                            f - 2
                        } else {
                            (f - 6) % 3
                        },
                        passes: u64::from(f >= 9),
                        entered_at: (f >= 2).then_some(Frame(f.min(12) + 1)),
                    });
                }
                _ => unreachable!(),
            }
            assert_eq!(
                (s.mode, s.position, s.cycles, s.last_pulse, s.composite),
                (mode, position, cycles, pulse, composite),
                "frame {frame} slot {slot}"
            );
        }
    }
}
#[shoop_test]
fn graph_and_stopped_topology_publish_together_with_independent_media_and_journals() {
    for (parts, journal, reverse) in [
        (vec![11], 1024, false),
        (vec![1; 11], 0, true),
        (vec![2, 4, 1, 4], 1, false),
        (vec![4, 7], 1024, true),
    ] {
        let mut h = live(true, journal, reverse);
        let candidate = compile(limits(journal), &description(true, reverse)).unwrap();
        let topology = h.ready(0, &candidate);
        let graph = h.graph(0, 1, true, reverse);
        assert_eq!(h.engine.next_frame(), Frame(3));
        h.op(3, play(0));
        h.op(3, play(2));
        let refused = h.op(5, pair(topology, graph, 1, 0));
        let still_ready = h.op(5, Operation::GraphStatus { id: graph });
        h.op(6, stop(2));
        let query = if reverse {
            Some(h.op(7, Operation::State { target: id(2) }))
        } else {
            None
        };
        let applied = h.op(7, pair(topology, graph, 1, 0));
        h.op(7, play(2));
        let query = query.unwrap_or_else(|| h.op(7, Operation::State { target: id(2) }));
        let exported = h.op(
            7,
            Operation::Export {
                target: id(1),
                selection: Selection::Playback,
            },
        );
        h.op(13, stop(0));
        h.op(13, stop(2));
        for frames in parts {
            h.step(frames);
            if h.engine.next_frame() == Frame(7) {
                assert_eq!(h.engine.diagnostics().epoch, Epoch(1));
                h.step(0);
                assert_eq!(h.engine.diagnostics().epoch, Epoch(1));
            }
        }
        h.drain();
        assert_eq!(
            h.reply(refused),
            Reply::Failed(Failure::Timeline(shoop_loops::Error::InvalidState))
        );
        assert_eq!(
            h.reply(still_ready),
            Reply::GraphStatus {
                id: graph,
                status: GraphStageStatus::Ready {
                    validated_epoch: Epoch(1)
                }
            }
        );
        assert_eq!(
            h.reply(applied),
            Reply::TopologyGraphActivated {
                id: topology,
                graph,
                graph_epoch: Epoch(2),
                topology_epoch: Epoch(1)
            }
        );
        let execution = h.replies.iter().find(|r| r.sequence == applied).unwrap();
        assert_eq!(
            (
                execution.requested_frame,
                execution.delivered_frame,
                execution.actual_frame
            ),
            (Frame(7), Some(Frame(7)), Frame(7))
        );
        let Reply::State(cut) = h.reply(query) else {
            panic!("state")
        };
        assert_eq!(
            (
                cut.frame,
                cut.graph_epoch,
                cut.topology_epoch,
                cut.state.mode,
                cut.state.content_length
            ),
            (Frame(7), Epoch(2), Epoch(1), Mode::Playing, 3)
        );
        let expected: Vec<_> = [0., 0., 0., 2., 4., 6., 0., 0., 4., 8., 0., 4., 8., 0.]
            .into_iter()
            .map(|v| [v, v * 10.])
            .collect();
        assert_eq!(h.samples, expected);
        let mut packets = Vec::new();
        restore(3, &mut packets);
        packets.push((5, vec![0x80, 60, 45]));
        restore(8, &mut packets);
        packets.push((10, vec![0x80, 60, 0]));
        restore(11, &mut packets);
        packets.push((13, vec![0x80, 60, 0]));
        assert_eq!(h.packets, packets);
        let Reply::Export {
            id: export,
            cut,
            media,
        } = h.reply(exported)
        else {
            panic!("export")
        };
        assert_eq!(
            (
                cut.graph_epoch,
                cut.topology_epoch,
                media.view.description.frames
            ),
            (Epoch(2), Epoch(1), 4)
        );
        let release = assert_no_heap(|| h.control.release_graph(graph))
            .unwrap_or_else(|e| panic!("{:?}", e.error))
            .sequence();
        let recycled = h.op(14, Operation::TopologyRecycle { id: topology });
        let bytes = h.read_export(export, false, 32);
        let original: Vec<_> = [4f32, 8., 12., 16.]
            .into_iter()
            .flat_map(|v| [v, v * 10.])
            .flat_map(f32::to_le_bytes)
            .collect();
        assert_eq!(bytes, original);
        assert_eq!(
            h.reply(recycled),
            Reply::TopologyStatus {
                id: topology,
                status: Status::Idle
            }
        );
        let returned = h.graphs.iter().find(|r| r.sequence == release).unwrap();
        assert_eq!(returned.outcome, TransportOutcome::Applied);
        let Some(rt_runtime::executor::Retired::Plan(old)) = &returned.retired else {
            panic!("old graph")
        };
        assert!(old.descriptor().steps.iter().any(|s| s.node == node(4)));
        h.op(h.engine.next_frame().0, Operation::Release { export });
        h.step(1);
        h.drain();
        let log = h.finish().unwrap();
        let complete: Vec<_> = expected_events()
            .into_iter()
            .map(|mut e| {
                e.frame.0 += 1;
                e
            })
            .collect();
        let expected: Vec<_> = (3..=13)
            .flat_map(|frame| {
                complete
                    .iter()
                    .filter(move |e| e.frame == Frame(frame))
                    .take(journal)
                    .copied()
            })
            .collect();
        assert_eq!(log.events, expected);
        assert_eq!(log.dropped, (complete.len() - expected.len()) as u64);
        assert_pair_states(&log);
    }
}
#[shoop_test]
fn intervening_live_graph_change_refuses_pending_program_without_publishing_its_zero_actions() {
    for parts in [vec![8], vec![1; 8], vec![2, 3, 3]] {
        let mut h = live(true, 1024, false);
        let candidate = compile(limits(1024), &description(true, false)).unwrap();
        let topology = h.ready(0, &candidate);
        let graph = h.graph(0, 1, true, false);
        h.op(3, play(0));
        h.op(3, play(2));
        let pending = h.op(4, program_activate(topology));
        let changed = h.op(5, switch(graph, 1, 0));
        let ready = h.op(6, Operation::TopologyStatus { id: topology });
        let state = h.op(7, Operation::State { target: id(2) });
        h.op(10, stop(0));
        h.op(10, stop(2));
        for n in parts {
            h.step(n);
        }
        h.drain();
        assert_eq!(
            h.reply(changed),
            Reply::GraphActivated {
                id: graph,
                graph_epoch: Epoch(2),
                topology_epoch: Epoch(0)
            }
        );
        let Reply::ProgramChanged {
            graph_epoch,
            topology_epoch,
            completion,
            ..
        } = h.reply(pending)
        else {
            panic!("program refusal")
        };
        assert_eq!((graph_epoch, topology_epoch), (Epoch(2), Epoch(0)));
        assert_eq!(
            (completion.requested, completion.effective),
            (Frame(4), Frame(4))
        );
        assert_eq!(
            completion.outcome,
            Outcome::Failed {
                frame: Frame(5),
                reason: shoop_loops::Error::InvalidState
            }
        );
        let execution = h.replies.iter().find(|e| e.sequence == pending).unwrap();
        assert_eq!(
            (execution.delivered_frame, execution.actual_frame),
            (Some(Frame(4)), Frame(5))
        );
        assert_eq!(
            h.reply(ready),
            Reply::TopologyStatus {
                id: topology,
                status: Status::Ready
            }
        );
        let Reply::State(cut) = h.reply(state) else {
            panic!("state")
        };
        assert_eq!(
            (
                cut.graph_epoch,
                cut.topology_epoch,
                cut.state.content_length,
                cut.state.composite.unwrap().passes
            ),
            (Epoch(2), Epoch(0), 4, 1)
        );
        let expected: Vec<_> = [0f32, 0., 0., 2., 4., 12., 16., 4., 8., 12., 0.]
            .into_iter()
            .map(|v| [v, v * 10.])
            .collect();
        assert_eq!(h.samples, expected);
        let mut midi = Vec::new();
        restore(3, &mut midi);
        midi.push((5, vec![0x80, 60, 45]));
        restore(7, &mut midi);
        midi.push((9, vec![0x80, 60, 45]));
        assert_eq!(h.packets, midi);
        let log = h.finish().unwrap();
        assert!(log
            .events
            .iter()
            .all(|e| e.kind != decision::EventKind::Transition(Transition::Program)));
    }
}
#[shoop_test]
fn graph_pair_preflight_rejects_node_pair_scope_epoch_and_cancelled_owners_without_partial_publication(
) {
    let mut disabled = Harness::new(false, 0, false, staging());
    assert_eq!(
        disabled.engine.prepare_graph_staging(0),
        Err(Error::InvalidConfig)
    );
    let plan = disabled
        .control
        .compile_graph(&routes(false, true, false))
        .unwrap();
    let ticket = assert_no_heap(|| disabled.control.stage_graph(0, Epoch(1), plan))
        .unwrap_or_else(|e| panic!("{:?}", e.error));
    disabled.step(1);
    disabled.drain();
    let result = disabled
        .graphs
        .iter()
        .find(|r| r.sequence == ticket.sequence())
        .unwrap();
    assert_eq!(result.outcome, TransportOutcome::Rejected(Error::InvalidId));
    assert!(matches!(
        result.request,
        Command::StageGraph {
            plan: Some(_),
            id: None,
            ..
        }
    ));
    assert_eq!(
        disabled.engine.prepare_graph_staging(2),
        Err(Error::InvalidState)
    );
    disabled.finish();
    let mut h = live(false, 0, false);
    let candidate = compile(limits(0), &description(true, false)).unwrap();
    let topology = h.ready(0, &candidate);
    let graph = h.graph(0, 1, true, false);
    let parts = graph.parts();
    let foreign = GraphStageId::from_parts(parts[0] + 1, parts[1], parts[2]).unwrap();
    let parts = topology.parts();
    let stale_topology = StageId::from_parts(parts[0], parts[1] + 1).unwrap();
    for (operation, error) in [
        (
            pair(topology, graph, 99, 0),
            Failure::Runtime(Error::StaleEpoch),
        ),
        (
            pair(topology, graph, 1, 99),
            Failure::Timeline(shoop_loops::Error::InvalidState),
        ),
        (
            pair(topology, foreign, 1, 0),
            Failure::Runtime(Error::InvalidId),
        ),
        (
            pair(stale_topology, graph, 1, 0),
            Failure::Timeline(shoop_loops::Error::InvalidId),
        ),
    ] {
        let at = h.engine.next_frame().0;
        let rejected = h.op(at, operation);
        let gs = h.op(at, Operation::GraphStatus { id: graph });
        let ts = h.op(at, Operation::TopologyStatus { id: topology });
        let state = h.op(at, Operation::State { target: id(2) });
        h.step(1);
        h.drain();
        assert_eq!(h.reply(rejected), Reply::Failed(error));
        assert_eq!(
            h.reply(gs),
            Reply::GraphStatus {
                id: graph,
                status: GraphStageStatus::Ready {
                    validated_epoch: Epoch(1)
                }
            }
        );
        assert_eq!(
            h.reply(ts),
            Reply::TopologyStatus {
                id: topology,
                status: Status::Ready
            }
        );
        let Reply::State(cut) = h.reply(state) else {
            panic!("cut")
        };
        assert_eq!(
            (
                cut.graph_epoch,
                cut.topology_epoch,
                cut.state.content_length
            ),
            (Epoch(1), Epoch(0), 4)
        );
    }
    for missing in [false, true] {
        let plan = h
            .control
            .compile_graph(&routes(false, true, false))
            .unwrap();
        let mut descriptor = plan.descriptor().clone();
        let mut planning = plan.planning().clone();
        if missing {
            descriptor.steps.retain(|s| s.node != node(1));
            planning.bindings.retain(|b| b.consumer != node(1));
        } else {
            planning.bindings[0].decision_slot = 2;
        }
        let bad = rt_runtime::graph::PreparedPlan::from_planned_descriptor(
            plan.config(),
            descriptor.clone(),
            planning.clone(),
        )
        .unwrap();
        let staged = h.graph_plan(1, 1, bad);
        let at = h.engine.next_frame().0;
        let rejected = h.op(at, pair(topology, staged, 1, 0));
        let ts = h.op(at, Operation::TopologyStatus { id: topology });
        h.step(1);
        h.drain();
        assert_eq!(
            h.reply(rejected),
            Reply::Failed(Failure::Runtime(Error::InvalidShape))
        );
        assert_eq!(
            h.reply(ts),
            Reply::TopologyStatus {
                id: topology,
                status: Status::Ready
            }
        );
        assert_eq!(h.engine.diagnostics().epoch, Epoch(1));
        let released = assert_no_heap(|| h.control.release_graph(staged))
            .unwrap_or_else(|e| panic!("{:?}", e.error))
            .sequence();
        h.step(1);
        h.drain();
        let returned = h.graphs.iter().find(|r| r.sequence == released).unwrap();
        assert_eq!(returned.outcome, TransportOutcome::Applied);
        let Some(rt_runtime::executor::Retired::Plan(owner)) = &returned.retired else {
            panic!("candidate owner")
        };
        assert_eq!(owner.descriptor(), &descriptor);
        assert_eq!(owner.planning(), &planning);
    }
    let at = h.engine.next_frame();
    let ticket = assert_no_heap(|| {
        h.control
            .submit(Lane::Normal, at, pair(topology, graph, 1, 0))
    })
    .unwrap_or_else(|e| panic!("{:?}", e.error));
    ticket.cancel();
    h.step(1);
    let completed = assert_no_heap(|| h.control.try_completion()).unwrap();
    assert_eq!(completed.sequence, ticket.sequence());
    assert_eq!(completed.outcome, TransportOutcome::Cancelled);
    assert!(assert_no_heap(|| h.control.try_execution()).is_none());
    let at = h.engine.next_frame();
    let recovery = assert_no_heap(|| h.control.submit(Lane::Recovery, at, switch(graph, 1, 0)))
        .unwrap_or_else(|e| panic!("{:?}", e.error));
    h.step(1);
    let completed = assert_no_heap(|| h.control.try_completion()).unwrap();
    assert_eq!(completed.sequence, recovery.sequence());
    assert_eq!(
        completed.outcome,
        TransportOutcome::Rejected(Error::InvalidState)
    );
    assert!(assert_no_heap(|| h.control.try_execution()).is_none());
    let rejected = h.send(
        Lane::Normal,
        h.engine.next_frame().0,
        pair(topology, graph, 1, 0),
        Box::new([1]),
    );
    h.step(1);
    h.drain();
    assert_eq!(
        h.reply(rejected),
        Reply::Failed(Failure::Runtime(Error::InvalidShape))
    );
    let release = assert_no_heap(|| h.control.release_graph(graph))
        .unwrap_or_else(|e| panic!("{:?}", e.error))
        .sequence();
    h.step(1);
    h.drain();
    assert_eq!(
        h.graphs
            .iter()
            .find(|r| r.sequence == release)
            .unwrap()
            .outcome,
        TransportOutcome::Applied
    );
    let plan = h
        .control
        .compile_graph(&routes(false, true, false))
        .unwrap();
    let ticket = assert_no_heap(|| h.control.stage_graph(0, Epoch(1), plan))
        .unwrap_or_else(|e| panic!("{:?}", e.error));
    ticket.cancel();
    h.step(1);
    h.drain();
    let cancelled = h
        .graphs
        .iter()
        .find(|r| r.sequence == ticket.sequence())
        .unwrap();
    assert_eq!(cancelled.outcome, TransportOutcome::Cancelled);
    assert!(matches!(
        cancelled.request,
        Command::StageGraph {
            plan: Some(_),
            id: None,
            ..
        }
    ));
    let next = h.graph(0, 1, true, false);
    assert_eq!(next.parts()[2], graph.parts()[2] + 1);
    let stale = h.op(h.engine.next_frame().0, switch(graph, 1, 0));
    h.step(1);
    h.drain();
    assert_eq!(
        h.reply(stale),
        Reply::Failed(Failure::Runtime(Error::InvalidId))
    );
    let applied = h.op(h.engine.next_frame().0, pair(topology, next, 1, 0));
    h.step(1);
    h.drain();
    assert_eq!(
        h.reply(applied),
        Reply::TopologyGraphActivated {
            id: topology,
            graph: next,
            graph_epoch: Epoch(2),
            topology_epoch: Epoch(1)
        }
    );
    h.finish();
}
#[shoop_test]
fn full_domain_slots_unread_graph_results_and_retirement_do_not_gate_clock_or_recovery() {
    let mut h = live(false, 0, false);
    let graph = h.graph(0, 1, true, false);
    let stale = h.graph(1, 1, true, true);
    h.op(2, play(0));
    let mut requests = Vec::new();
    for _ in 0..16 {
        requests.push(h.op(
            2,
            Operation::Transition {
                target: id(1),
                timing: Timing::AfterTicks {
                    source: id(0),
                    skip: 1000,
                },
                transition: Transition::Stop,
            },
        ));
    }
    let activated = h.op(3, switch(graph, 1, 0));
    for frames in [32, 32, 32, 32, 2] {
        h.step(frames);
    }
    assert_eq!(h.engine.next_frame(), Frame(132));
    let cancelled = h.send(
        Lane::Recovery,
        132,
        Operation::Cancel {
            sequence: requests[0],
        },
        Box::new([]),
    );
    h.step(1);
    let state = h.op(133, Operation::State { target: id(0) });
    h.step(1);
    h.drain();
    assert_eq!(
        h.reply(activated),
        Reply::GraphActivated {
            id: graph,
            graph_epoch: Epoch(2),
            topology_epoch: Epoch(0)
        }
    );
    assert_eq!(
        h.reply(cancelled),
        Reply::Cancelled {
            sequence: requests[0],
            delivered: true
        }
    );
    let Reply::State(cut) = h.reply(state) else {
        panic!("state")
    };
    assert_eq!(
        (
            cut.state.mode,
            cut.state.cycles,
            cut.state.last_pulse,
            cut.graph_epoch
        ),
        (Mode::Playing, 131, Some(Frame(133)), Epoch(2))
    );
    let refused = h.op(134, switch(stale, 2, 0));
    h.step(1);
    h.drain();
    assert_eq!(
        h.reply(refused),
        Reply::Failed(Failure::Runtime(Error::StaleEpoch))
    );
    let release = assert_no_heap(|| h.control.release_graph(stale))
        .unwrap_or_else(|e| panic!("{:?}", e.error))
        .sequence();
    h.step(1);
    h.drain();
    let index = h.graphs.iter().position(|r| r.sequence == release).unwrap();
    let returned = h.graphs.remove(index);
    let Some(rt_runtime::executor::Retired::Plan(owner)) = returned.retired else {
        panic!("stale owner")
    };
    let restaged = h.graph_plan(1, 2, owner);
    assert_eq!(restaged.parts()[2], stale.parts()[2] + 1);
    let at = h.engine.next_frame().0;
    h.op(
        at,
        Operation::Transition {
            target: id(1),
            timing: Timing::AfterTicks {
                source: id(0),
                skip: 1000,
            },
            transition: Transition::Stop,
        },
    );
    let applied = h.op(at, switch(restaged, 2, 0));
    h.step(1);
    h.drain();
    assert_eq!(
        h.reply(applied),
        Reply::GraphActivated {
            id: restaged,
            graph_epoch: Epoch(3),
            topology_epoch: Epoch(0)
        }
    );
    // Both retired graph owners and sixteen unresolved musical requests are retained.
    h.finish();
}
#[shoop_test]
fn paired_graph_topology_owners_survive_disconnect_and_post_publication_render_fault() {
    for before_delivery in [false, true] {
        let mut h = live(false, 0, false);
        let candidate = compile(limits(0), &description(true, false)).unwrap();
        let topology = h.ready(0, &candidate);
        let graph = h.graph(0, 1, true, false);
        let applied = h.op(4, pair(topology, graph, 1, 0));
        if !before_delivery {
            h.step(2);
            h.drain();
            assert_eq!(
                h.reply(applied),
                Reply::TopologyGraphActivated {
                    id: topology,
                    graph,
                    graph_epoch: Epoch(2),
                    topology_epoch: Epoch(1)
                }
            );
        }
        let Harness {
            control,
            mut engine,
            ..
        } = h;
        drop(control);
        if before_delivery {
            assert_no_heap(|| {
                engine.process(
                    AudioBlock {
                        frame: Frame(3),
                        frames: 2,
                        inputs: &[],
                        outputs: &mut [],
                    },
                    MidiBlock {
                        inputs: &[],
                        outputs: &mut [],
                    },
                )
            })
            .unwrap();
        }
        assert_eq!(engine.diagnostics().epoch, Epoch(2));
        let mut stopped = engine.into_stopped();
        assert_no_heap(|| {
            stopped.cleanup().unwrap();
            stopped.cleanup().unwrap();
            assert_eq!(stopped.media_usage().references, 0);
            assert_eq!(stopped.resource_usage().free_pages, 128);
            assert_eq!(stopped.resource_usage().free_versions, 16);
            assert_eq!(stopped.resource_usage().free_snapshots, 16);
        });
    }
    let mut h = Harness::options(true, 1024, false, staging(), 8, Some(Frame(4)));
    h.engine.prepare_graph_staging(2).unwrap();
    let candidate = compile(limits(1024), &description(true, false)).unwrap();
    let topology = h.ready(0, &candidate);
    let mut graph = routes(true, false, false);
    graph.connections.push(Connection {
        source: Endpoint::node(node(2), PortType::Audio, 0),
        destination: Endpoint::host(PortType::Audio, 0),
    });
    let plan = h.control.compile_graph(&graph).unwrap();
    let graph = h.graph_plan(0, 1, plan);
    let applied = h.op(4, pair(topology, graph, 1, 0));
    h.op(4, play(0));
    h.op(4, play(2));
    let query = h.op(4, Operation::State { target: id(2) });
    h.step(1);
    let [left, right] = &mut h.output;
    h.midi[0].clear();
    let error = assert_no_heap(|| {
        h.engine.process(
            AudioBlock {
                frame: Frame(4),
                frames: 1,
                inputs: &[],
                outputs: &mut [&mut left[..1], &mut right[..1]],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut h.midi,
            },
        )
    })
    .unwrap_err();
    assert_eq!(error, Error::InvalidState);
    assert_eq!(h.engine.next_frame(), Frame(5));
    assert_eq!(h.engine.diagnostics().epoch, Epoch(2));
    h.drain();
    assert_eq!(
        h.reply(applied),
        Reply::TopologyGraphActivated {
            id: topology,
            graph,
            graph_epoch: Epoch(2),
            topology_epoch: Epoch(1)
        }
    );
    let Reply::State(cut) = h.reply(query) else {
        panic!("cut")
    };
    assert_eq!(
        (
            cut.frame,
            cut.graph_epoch,
            cut.topology_epoch,
            cut.state.mode
        ),
        (Frame(4), Epoch(2), Epoch(1), Mode::Playing)
    );
    let log = h.finish().unwrap();
    assert_eq!(log.states[4][2].unwrap().content_length, 3);
}

#[shoop_test]
fn live_route_cut_keeps_recording_prefix_and_midi_continuity_without_restarting_the_take() {
    for (parts, reverse) in [
        (vec![10], false),
        (vec![1; 10], true),
        (vec![3, 2, 5], false),
    ] {
        let mut h = live(true, 1024, reverse);
        let graph = h.graph(0, 1, true, reverse);
        h.op(2, play(0));
        h.op(
            2,
            Operation::Transition {
                target: id(1),
                timing: Timing::At,
                transition: Transition::Record {
                    monitor_previous: true,
                },
            },
        );
        let switched = h.op(4, switch(graph, 1, 0));
        let prefix = h.op(
            4,
            Operation::Export {
                target: id(1),
                selection: Selection::Recording,
            },
        );
        h.op(8, play(1));
        let full = h.op(
            8,
            Operation::Export {
                target: id(1),
                selection: Selection::Playback,
            },
        );
        h.op(10, stop(0));
        h.op(10, stop(1));
        for n in parts {
            h.step(n);
        }
        h.drain();
        assert_eq!(
            h.reply(switched),
            Reply::GraphActivated {
                id: graph,
                graph_epoch: Epoch(2),
                topology_epoch: Epoch(0)
            }
        );
        let expected: Vec<_> = [0., 0., 2., 4., 12., 16., 4., 8., 2., 4., 0.]
            .into_iter()
            .map(|v| [v, v * 10.])
            .collect();
        assert_eq!(h.samples, expected);
        let mut originals = Vec::new();
        restore(0, &mut originals);
        originals.push((2, vec![0x80, 60, 45]));
        restore(4, &mut originals);
        let mut packets: Vec<_> = originals.iter().map(|(f, b)| (*f + 2, b.clone())).collect();
        packets.push((8, vec![0x80, 60, 0]));
        let mut initial = Vec::new();
        restore(8, &mut initial);
        initial.pop();
        packets.extend(initial);
        packets.extend(
            originals
                .iter()
                .filter(|(f, _)| *f < 2)
                .map(|(f, b)| (*f + 8, b.clone())),
        );
        packets.push((10, vec![0x80, 60, 0]));
        assert_eq!(h.packets, packets);
        let mut exports = Vec::new();
        for (seq, frames, samples) in [
            (prefix, 2, vec![2f32, 4.]),
            (full, 6, vec![2f32, 4., 12., 16., 4., 8.]),
        ] {
            let Reply::Export {
                id: export,
                cut,
                media,
            } = h.reply(seq)
            else {
                panic!("export")
            };
            assert_eq!(
                (
                    cut.graph_epoch,
                    cut.topology_epoch,
                    media.view.description.frames,
                    media.view.description.revision
                ),
                (Epoch(2), Epoch(0), frames, Revision(1))
            );
            assert_eq!(cut.state.capture_revision, Revision(1));
            assert!(!cut.state.capture_incomplete);
            let expected: Vec<_> = samples
                .into_iter()
                .flat_map(|v| [v, v * 10.])
                .flat_map(f32::to_le_bytes)
                .collect();
            assert_eq!(h.read_export(export, false, expected.len()), expected);
            let expected = encoded_packets(
                &originals
                    .iter()
                    .filter(|(f, _)| *f < frames)
                    .cloned()
                    .collect::<Vec<_>>(),
            );
            assert_eq!(h.read_export(export, true, expected.len()), expected);
            exports.push(export);
        }
        for export in exports {
            h.op(h.engine.next_frame().0, Operation::Release { export });
        }
        h.step(1);
        h.drain();
        h.finish();
    }
}
