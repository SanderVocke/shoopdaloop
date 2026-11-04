mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    executor::{Command, GraphRuntime},
    graph::{self, Connection, Endpoint, GraphDescription, PortCounts, PortType},
    planner::NoPlanner,
    processor::{ProcessBlock, Processor},
    resource::ResourceStore,
    transport::Lane,
    Config, Epoch, Error, Frame, Quantum,
};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

struct Unused;
impl Processor for Unused {
    fn ports(&self) -> PortCounts {
        PortCounts::default()
    }
    fn process(&mut self, _: &mut ProcessBlock<'_>) -> Result<(), Error> {
        Err(Error::InvalidState)
    }
}
fn graph(input: u32) -> GraphDescription {
    GraphDescription {
        nodes: vec![],
        connections: vec![Connection {
            source: Endpoint::host(PortType::Audio, input),
            destination: Endpoint::host(PortType::Audio, 0),
        }],
    }
}
fn step(runtime: &mut GraphRuntime<Unused>, frames: usize) -> Vec<f32> {
    let first = vec![1.; frames];
    let second = vec![2.; frames];
    let mut output = vec![99.; frames];
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames,
                inputs: &[&first, &second],
                outputs: &mut [&mut output],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
    .unwrap();
    output
}

#[shoop_test]
fn buffered_generic_publication_cannot_change_an_already_collected_tick() {
    for q in [16, 32, 64, 128] {
        let config = Config {
            max_nodes: 1,
            max_ports: 3,
            max_edges: 1,
            max_frames: q,
            audio_inputs: 2,
            audio_outputs: 1,
            midi_inputs: 0,
            midi_outputs: 0,
            ..Config::default()
        }
        .prepare()
        .unwrap();
        let (mut control, mut runtime) = GraphRuntime::<Unused>::prepare_for_host(
            config,
            128,
            Quantum::new(q).unwrap(),
            vec![],
            NoPlanner,
            ResourceStore::new(config, Quantum::new(q).unwrap()).unwrap(),
        )
        .unwrap();
        runtime.prepare_host_buffering().unwrap();
        let plan = graph::compile(runtime.catalog(), &graph(0)).unwrap();
        control
            .submit(Lane::Normal, Command::commit(Epoch(0), plan))
            .unwrap_or_else(|e| panic!("{:?}", e.error));
        assert_eq!(step(&mut runtime, 1), [0.]);
        assert_eq!(runtime.epoch(), Epoch(1));
        let plan = graph::compile(runtime.catalog(), &graph(1)).unwrap();
        control
            .submit(Lane::Normal, Command::commit(Epoch(1), plan))
            .unwrap_or_else(|e| panic!("{:?}", e.error));
        assert_eq!(step(&mut runtime, q - 1), vec![0.; q - 1]);
        assert_eq!(runtime.epoch(), Epoch(1));
        assert_eq!(step(&mut runtime, q), vec![1.; q]);
        assert_eq!(runtime.epoch(), Epoch(2));
        assert_eq!(step(&mut runtime, q), vec![2.; q]);
        assert_eq!(runtime.next_frame(), Frame(3 * q as u64));
    }
}

#[shoop_test]
fn buffered_capture_begins_at_an_eligible_tick_boundary() {
    for q in [16, 32, 64, 128] {
        let config = Config {
            max_frames: q,
            audio_inputs: 0,
            audio_outputs: 0,
            midi_inputs: 0,
            midi_outputs: 0,
            ..Config::default()
        }
        .prepare()
        .unwrap();
        let mut resources = ResourceStore::new(config, Quantum::new(q).unwrap()).unwrap();
        let resource = resources.create().unwrap();
        let (mut control, mut runtime) = GraphRuntime::<Unused>::prepare_for_host(
            config,
            128,
            Quantum::new(q).unwrap(),
            vec![],
            NoPlanner,
            resources,
        )
        .unwrap();
        runtime.prepare_host_buffering().unwrap();
        let step = |runtime: &mut GraphRuntime<Unused>, frames| {
            assert_no_heap(|| {
                runtime.process(
                    AudioBlock {
                        frame: runtime.next_frame(),
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
        };
        step(&mut runtime, 1);
        control
            .submit(
                Lane::Normal,
                Command::resource(rt_runtime::resource::Request::BeginCapture {
                    resource,
                    expected_revision: rt_runtime::Revision(0),
                    format: rt_runtime::resource::Format::Audio { channels: 1 },
                }),
            )
            .unwrap_or_else(|e| panic!("{:?}", e.error));
        step(&mut runtime, q - 1);
        assert!(control.try_completion().is_none());
        step(&mut runtime, 1);
        assert!(control.try_completion().is_some());
        let resources = runtime.into_resources();
        assert_eq!(
            resources
                .info(resource)
                .unwrap()
                .capture
                .unwrap()
                .next_frame,
            Frame(q as u64)
        );
    }
}
