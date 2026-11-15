use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    executor::GraphRuntime,
    graph::{Connection, Endpoint, GraphDescription, PortType},
    planner::NoPlanner,
    processor::Processor,
    resource::ResourceStore,
    Config, Frame, Quantum, Q824,
};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn production_fan_in_preserves_connection_order_and_each_fixed_saturation() {
    for values in [[100, 100, -100], [100, -100, 100]] {
        let config = Config {
            max_frames: 32,
            audio_inputs: 3,
            audio_outputs: 1,
            midi_inputs: 0,
            midi_outputs: 0,
            ..Default::default()
        }
        .prepare()
        .unwrap();
        let q = Quantum::new(32).unwrap();
        let resources = ResourceStore::<Q824>::new_numeric(config, q).unwrap();
        type Node = Box<dyn Processor<NoPlanner, (), Q824>>;
        let (_, mut runtime) = GraphRuntime::<Node, NoPlanner, (), Q824>::prepare_with_resources(
            config,
            q,
            vec![],
            resources,
        )
        .unwrap();
        runtime
            .prepare_initial_graph(&GraphDescription {
                nodes: vec![],
                connections: [2, 1, 0]
                    .map(|port| Connection {
                        source: Endpoint::host(PortType::Audio, port),
                        destination: Endpoint::host(PortType::Audio, 0),
                    })
                    .to_vec(),
            })
            .unwrap();
        let a = [Q824(values[0] << 24); 32];
        let b = [Q824(values[1] << 24); 32];
        let c = [Q824(values[2] << 24); 32];
        let mut output = [Q824::ZERO; 32];
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame: Frame(0),
                    frames: 32,
                    inputs: &[&a, &b, &c],
                    outputs: &mut [&mut output],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
        })
        .unwrap();
        let expected = if values == [100, 100, -100] {
            i32::MAX - (100 << 24)
        } else {
            100 << 24
        };
        assert_eq!(output, [Q824(expected); 32]);
    }
}

#[shoop_test]
fn fixed_host_buffering_preserves_raw_samples_and_declared_quantum_latency_across_partitions() {
    for partitions in [vec![128, 32], vec![1; 160], vec![3, 1, 27, 34, 17, 78]] {
        let config = Config {
            max_frames: 128,
            audio_inputs: 1,
            audio_outputs: 1,
            midi_inputs: 0,
            midi_outputs: 0,
            ..Default::default()
        }
        .prepare()
        .unwrap();
        let q = Quantum::new(32).unwrap();
        let resources = ResourceStore::<Q824>::new_numeric(config, q).unwrap();
        type Node = Box<dyn Processor<NoPlanner, (), Q824>>;
        let (_, mut runtime) = GraphRuntime::<Node, NoPlanner, (), Q824>::prepare_with_resources(
            config,
            q,
            vec![],
            resources,
        )
        .unwrap();
        runtime
            .prepare_initial_graph(&GraphDescription {
                nodes: vec![],
                connections: vec![Connection {
                    source: Endpoint::host(PortType::Audio, 0),
                    destination: Endpoint::host(PortType::Audio, 0),
                }],
            })
            .unwrap();
        runtime.prepare_host_buffering().unwrap();
        assert_eq!(runtime.buffering_latency(), 32);
        let input: Vec<_> = (0..160).map(|i| Q824(16_777_217 + i * 13)).collect();
        let mut output = vec![Q824::ZERO; 160];
        let mut at = 0;
        for frames in partitions {
            assert_no_heap(|| {
                runtime.process(
                    AudioBlock {
                        frame: Frame(at as u64),
                        frames,
                        inputs: &[&input[at..at + frames]],
                        outputs: &mut [&mut output[at..at + frames]],
                    },
                    MidiBlock {
                        inputs: &[],
                        outputs: &mut [],
                    },
                )
            })
            .unwrap();
            at += frames;
        }
        assert_eq!(&output[..32], &[Q824::ZERO; 32]);
        assert_eq!(&output[32..], &input[..128]);
        assert_eq!(runtime.next_frame(), Frame(160));
    }
}
