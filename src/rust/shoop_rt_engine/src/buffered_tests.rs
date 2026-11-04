use crate::{
    cells::Cells,
    node::{NodeSettings, StripOutput},
    EngineOp,
};
use alloc::{boxed::Box, vec, vec::Vec};
use rt_buffered::{BufferedProcessor, MidiLimits, Timing};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{Command, GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    processor::{OperationContext, ProcessBlock, Processor},
    resource::ResourceStore,
    schedule::Action,
    transport::Lane,
    Config, Error, Frame, NodeId, PlannerId, Quantum,
};
use shoop_loops::{
    compile,
    runtime::{LoopPlanner, Options, Settings},
    Limits, TopologyDescription,
};
use shoop_rt_protocol::{DriverKind, DriverRunState, DriverStatus};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

enum Node {
    Engine(StripOutput),
    Buffered(BufferedProcessor),
}
impl Processor<LoopPlanner, EngineOp> for Node {
    fn ports(&self) -> PortCounts {
        match self {
            Self::Engine(n) => n.ports(),
            Self::Buffered(n) => n.ports(),
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        match self {
            Self::Engine(n) => n.process(block),
            Self::Buffered(n) => {
                <BufferedProcessor as Processor<LoopPlanner, EngineOp>>::process(n, block)
            }
        }
    }
    fn operation(&mut self, value: EngineOp, context: OperationContext<'_>) -> Result<(), Error> {
        match self {
            Self::Engine(n) => n.operation(value, context),
            Self::Buffered(_) => Err(Error::InvalidEvent),
        }
    }
}

#[shoop_test]
fn buffered_composes_with_prepared_engine_dsp_and_typed_controls() {
    let quantum = Quantum::new(16).unwrap();
    let config = Config {
        max_frames: 128,
        audio_inputs: 2,
        audio_outputs: 2,
        midi_inputs: 1,
        midi_outputs: 1,
        max_nodes: 2,
        max_ports: 32,
        max_edges: 16,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let cells = Cells::new(
        1,
        0,
        0,
        1,
        DriverStatus {
            kind: DriverKind::Dummy,
            state: DriverRunState::Running,
            sample_rate: 48000,
            buffer_frames: 128,
        },
    );
    let settings = NodeSettings {
        cells,
        channels: 2,
        master_channels: 2,
        frames: quantum.frames(),
        ramp: 0,
    };
    let mut resources = ResourceStore::new(config, quantum).unwrap();
    let topology = compile(
        Limits {
            loop_slots: 1,
            relations: 1,
            program_entries: 1,
            program_segments: 1,
            pending_requests: 2,
            journal_events: 0,
        },
        &TopologyDescription::default(),
    )
    .unwrap();
    let prepared = LoopPlanner::prepare(
        config,
        quantum,
        topology,
        &mut resources,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 1,
            media_slots: 8,
            export_slots: 2,
        },
        Vec::new(),
        Vec::new(),
        Options::default(),
    )
    .unwrap();
    let buffered_id = NodeId::from_parts(0, 1).unwrap();
    let downstream = NodeId::from_parts(1, 1).unwrap();
    let nodes = vec![
        PreparedNode::new(
            buffered_id,
            Node::Buffered(
                BufferedProcessor::prepare(quantum, Timing::default(), MidiLimits::default())
                    .unwrap(),
            ),
        ),
        PreparedNode::new(downstream, Node::Engine(StripOutput::new(0, &settings))),
    ];
    let mut graph = GraphDescription {
        nodes: vec![downstream, buffered_id],
        connections: Vec::new(),
    };
    for (kind, count) in [(PortType::Audio, 2), (PortType::Midi, 1)] {
        for c in 0..count {
            for (source, destination) in [
                (
                    Endpoint::host(kind, c),
                    Endpoint::node(buffered_id, kind, c),
                ),
                (
                    Endpoint::node(buffered_id, kind, c),
                    Endpoint::node(downstream, kind, c),
                ),
                (Endpoint::node(downstream, kind, c), Endpoint::host(kind, c)),
            ] {
                graph.connections.push(Connection {
                    source,
                    destination,
                });
            }
        }
    }
    let (mut control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        quantum,
        nodes,
        prepared.planner,
        resources,
        &graph,
    )
    .unwrap();
    for operation in [
        EngineOp::Reset {
            inputs: 2,
            outputs: 2,
        },
        EngineOp::Gain(0.5),
    ] {
        control
            .submit(
                Lane::Normal,
                Command::Schedule {
                    at: Frame(0),
                    action: Action::Processor {
                        node: downstream,
                        operation,
                    },
                    payload: Box::new([]),
                },
            )
            .unwrap_or_else(|refusal| panic!("{:?}", refusal.error));
    }
    let mut input = MidiBuffer::new(16, 128).unwrap();
    let mut output = MidiBuffer::new(16, 128).unwrap();
    let mut midi = Vec::new();
    let source: &[(usize, &[u8], &[u8])] = &[
        (15, &[0x9f, 60, 100], &[0x90, 60, 100]),
        (31, &[0x8f, 60, 0], &[0x80, 60, 0]),
        (31, &[0x90, 61, 0], &[0x91, 61, 0]),
        (32, &[0xf0, 1, 2, 0xf7], &[0xf0, 1, 2, 0xf7]),
    ];
    for frame in (0..160).step_by(16) {
        let left: Vec<_> = (frame..frame + 16)
            .map(|n| if n < 64 { n as f32 * 0.25 } else { 0. })
            .collect();
        let right: Vec<_> = (frame..frame + 16)
            .map(|n| if n < 64 { -(n as f32) - 1. } else { 0. })
            .collect();
        let mut out_left = [0.; 16];
        let mut out_right = [0.; 16];
        input.clear();
        output.clear();
        for &(at, bytes, _) in source {
            if at >= frame && at < frame + 16 {
                input.push((at - frame) as u32, 16, bytes).unwrap();
            }
        }
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame: Frame(frame as u64),
                    frames: 16,
                    inputs: &[&left, &right],
                    outputs: &mut [&mut out_left, &mut out_right],
                },
                MidiBlock {
                    inputs: core::slice::from_ref(&input),
                    outputs: core::slice::from_mut(&mut output),
                },
            )
        })
        .unwrap();
        for i in 0..16 {
            let n = frame + i;
            let active = (48..112).contains(&n);
            assert_eq!(
                out_left[i],
                if active { (n - 48) as f32 * 0.25 } else { 0. }
            );
            assert_eq!(
                out_right[i],
                if active { -((n - 48) as f32) - 1. } else { 0. }
            );
        }
        for i in 0..output.len() {
            let (at, bytes) = output.get(i).unwrap();
            midi.push((frame + at as usize, bytes.to_vec()));
        }
    }
    assert_eq!(
        midi,
        source
            .iter()
            .map(|&(at, _, bytes)| (at + 48, bytes.to_vec()))
            .collect::<Vec<_>>()
    );
    for _ in 0..2 {
        let execution = control.try_execution().unwrap();
        assert_eq!(execution.actual_frame, Frame(0));
        assert_eq!(execution.result, Ok(()));
        assert_eq!(execution.processor_result, Some(()));
        assert!(control.try_completion().is_some());
    }
    assert!(control.try_execution().is_none());
}
