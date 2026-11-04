use rt_external::{External, MidiLimits, Timing};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    fixed::{self, Processor, Quantum, Runtime},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    Error, Frame, NodeId,
};
use shoop_rt_engine::{fixed as engine, EngineDescription, EngineOp, HostPorts, Segment};
use shoop_rt_protocol::{DriverKind, DriverRunState, DriverStatus};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

enum Node {
    Engine(engine::Node),
    External(External),
}

impl Processor<engine::State, EngineOp> for Node {
    fn ports(&self) -> PortCounts {
        match self {
            Self::Engine(n) => <engine::Node as Processor<engine::State, EngineOp>>::ports(n),
            Self::External(n) => n.ports(),
        }
    }
    fn validate_state(&self, id: NodeId, state: &engine::State) -> Result<(), Error> {
        match self {
            Self::Engine(n) => n.validate_state(id, state),
            Self::External(_) => Ok(()),
        }
    }
    fn process(
        &mut self,
        block: &mut fixed::Block<'_>,
        state: &mut engine::State,
    ) -> Result<(), Error> {
        match self {
            Self::Engine(n) => n.process(block, state),
            Self::External(n) => {
                <External as Processor<engine::State, EngineOp>>::process(n, block, state)
            }
        }
    }
    fn operation(
        &mut self,
        value: EngineOp,
        context: fixed::OperationContext<'_>,
        state: &mut engine::State,
    ) -> Result<(), Error> {
        match self {
            Self::Engine(n) => n.operation(value, context, state),
            Self::External(_) => Err(Error::InvalidEvent),
        }
    }
}

#[shoop_test]
fn external_composes_with_prepared_engine_dsp_and_typed_controls() {
    let quantum = Quantum::new(16).unwrap();
    let description = EngineDescription {
        quantum: 16,
        max_frames: 128,
        host: HostPorts::numbered(2, 2, 1),
        segments: vec![Segment {
            strips: 1,
            loops: 0,
        }],
        buses: 1,
        bus_channels: 2,
        resource_pages: 16,
        page_bytes: 256,
        ramp_frames: 0,
        ..EngineDescription::default()
    };
    let prepared = engine::prepare(
        &description,
        DriverStatus {
            kind: DriverKind::Dummy,
            state: DriverRunState::Running,
            sample_rate: 48000,
            buffer_frames: 128,
        },
        quantum,
    )
    .unwrap();
    let external_id = NodeId::from_parts(1, 1).unwrap();
    let downstream = NodeId::from_parts(3, 1).unwrap();
    let nodes = prepared
        .nodes
        .into_iter()
        .filter_map(|(id, node)| {
            if id == external_id {
                Some((
                    id,
                    Node::External(
                        External::prepare(quantum, Timing::default(), MidiLimits::default())
                            .unwrap(),
                    ),
                ))
            } else if id == downstream {
                Some((id, Node::Engine(node)))
            } else {
                None
            }
        })
        .collect();
    let mut graph = GraphDescription {
        nodes: vec![downstream, external_id],
        connections: Vec::new(),
    };
    for (kind, count) in [(PortType::Audio, 2), (PortType::Midi, 1)] {
        for c in 0..count {
            for (source, destination) in [
                (
                    Endpoint::host(kind, c),
                    Endpoint::node(external_id, kind, c),
                ),
                (
                    Endpoint::node(external_id, kind, c),
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
    let mut runtime = Runtime::<Node, engine::State, EngineOp>::prepare(
        prepared.config,
        nodes,
        prepared.state,
        prepared.resources,
        &graph,
    )
    .unwrap();
    let mut operations = runtime.prepare_operations(4, 4).unwrap();
    operations
        .submit(
            Frame(0),
            downstream,
            EngineOp::Reset {
                inputs: 2,
                outputs: 2,
            },
        )
        .unwrap();
    operations
        .submit(Frame(0), downstream, EngineOp::Gain(0.5))
        .unwrap();
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
    assert!(operations.try_receipt().unwrap().result.is_ok());
    assert!(operations.try_receipt().unwrap().result.is_ok());
    runtime.shutdown_operations(&mut operations).unwrap();
}
