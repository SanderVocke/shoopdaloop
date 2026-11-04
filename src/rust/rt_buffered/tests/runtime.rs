use rt_buffered::{BufferedProcessor, MidiLimits, Timing};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortType},
    planner::NoPlanner,
    resource::ResourceStore,
    Config, Error, Frame, NodeId, Quantum,
};
use shoop_wasm_test_support::shoop_test;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

struct Host(GraphRuntime<BufferedProcessor>);

impl Host {
    fn new(q: usize, timing: Timing, buffered: bool, limits: MidiLimits) -> Self {
        let quantum = Quantum::new(q).unwrap();
        let node = NodeId::from_parts(0, 1).unwrap();
        let processor = BufferedProcessor::prepare(quantum, timing, limits).unwrap();
        let capacities = Config {
            max_frames: 128,
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 1,
            midi_outputs: 1,
            max_nodes: 1,
            max_ports: 12,
            max_edges: 6,
            midi_events_per_port: 64,
            midi_bytes_per_port: 4096,
            ..Config::default()
        };
        let mut graph = GraphDescription {
            nodes: vec![node],
            connections: Vec::new(),
        };
        for (kind, count) in [(PortType::Audio, 2), (PortType::Midi, 1)] {
            for c in 0..count {
                for (source, destination) in [
                    (Endpoint::host(kind, c), Endpoint::node(node, kind, c)),
                    (Endpoint::node(node, kind, c), Endpoint::host(kind, c)),
                ] {
                    graph.connections.push(Connection {
                        source,
                        destination,
                    });
                }
            }
        }
        let config = capacities.prepare().unwrap();
        let (_, mut runtime) = GraphRuntime::prepare_with_planner_and_resources(
            config,
            quantum,
            vec![PreparedNode::new(node, processor)],
            NoPlanner,
            ResourceStore::new(config, quantum).unwrap(),
        )
        .unwrap();
        runtime.prepare_initial_graph(&graph).unwrap();
        if buffered {
            runtime.prepare_host_buffering().unwrap();
        }
        Self(runtime)
    }
    fn process(&mut self, audio: AudioBlock<'_, '_>, midi: MidiBlock<'_>) -> Result<(), Error> {
        self.0.process(audio, midi)
    }
    fn recovery(&mut self, sink: &mut MidiBuffer) -> usize {
        self.0.drain_midi_recovery(0, sink, 16).unwrap()
    }
}

type Journal = (Vec<f32>, Vec<f32>, Vec<(u64, Vec<u8>)>);

fn run(q: usize, m: usize, s: usize, partition: usize, buffered: bool) -> Journal {
    let timing = Timing {
        batch_quanta: m,
        slack_quanta: s,
    };
    let mut host = Host::new(q, timing, buffered, MidiLimits::default());
    let total = 96 + (m + s) * q + if buffered { q } else { 0 };
    let source: &[(usize, &[u8])] = &[
        (0, &[0x9f, 60, 100]),
        (15, &[0x90, 61, 0]),
        (16, &[0x8f, 60, 0]),
        (47, &[0xf0, 1, 2, 0xf7]),
        (47, &[0xf8]),
    ];
    let mut result = (Vec::new(), Vec::new(), Vec::new());
    let mut frame = 0;
    let mut input = MidiBuffer::new(64, 4096).unwrap();
    let mut output = MidiBuffer::new(64, 4096).unwrap();
    host.process(
        AudioBlock {
            frame: Frame(0),
            frames: 0,
            inputs: &[&[], &[]],
            outputs: &mut [&mut [], &mut []],
        },
        MidiBlock {
            inputs: core::slice::from_ref(&input),
            outputs: core::slice::from_mut(&mut output),
        },
    )
    .unwrap();
    while frame < total {
        let count = partition.min(total - frame);
        let left: Vec<_> = (frame..frame + count)
            .map(|n| if n < 96 { n as f32 * 0.25 } else { 0. })
            .collect();
        let right: Vec<_> = (frame..frame + count)
            .map(|n| if n < 96 { -(n as f32) - 1. } else { 0. })
            .collect();
        let mut out_left = vec![99.; count];
        let mut out_right = vec![99.; count];
        input.clear();
        output.clear();
        for &(at, packet) in source {
            if at >= frame && at < frame + count {
                input.push((at - frame) as u32, count, packet).unwrap();
            }
        }
        host.process(
            AudioBlock {
                frame: Frame(frame as u64),
                frames: count,
                inputs: &[&left, &right],
                outputs: &mut [&mut out_left, &mut out_right],
            },
            MidiBlock {
                inputs: core::slice::from_ref(&input),
                outputs: core::slice::from_mut(&mut output),
            },
        )
        .unwrap();
        result.0.extend(out_left);
        result.1.extend(out_right);
        for i in 0..output.len() {
            let (at, bytes) = output.get(i).unwrap();
            result
                .2
                .push(((frame + at as usize) as u64, bytes.to_vec()));
        }
        frame += count;
    }
    let delay = (m + s) * q + if buffered { q } else { 0 };
    for n in 0..total {
        let active = n >= delay && n < delay + 96;
        assert_eq!(
            result.0[n],
            if active { (n - delay) as f32 * 0.5 } else { 0. }
        );
        assert_eq!(
            result.1[n],
            if active {
                -2. * (n - delay) as f32 - 2.
            } else {
                0.
            }
        );
    }
    assert_eq!(
        result.2,
        vec![
            (delay as u64, vec![0x90, 60, 100]),
            ((delay + 15) as u64, vec![0x91, 61, 0]),
            ((delay + 16) as u64, vec![0x80, 60, 0]),
            ((delay + 47) as u64, vec![0xf0, 1, 2, 0xf7]),
            ((delay + 47) as u64, vec![0xf8]),
        ]
    );
    result
}

#[shoop_test]
fn buffered_runtime_host_partitions_match_independent_reference() {
    for (q, m, s) in [(16, 2, 1), (32, 3, 7)] {
        let reference = run(q, m, s, q, false);
        for partition in [q, 128] {
            assert_eq!(run(q, m, s, partition, false), reference);
        }
        let buffered_reference = run(q, m, s, 128, true);
        for partition in [1, 7, q, 128] {
            assert_eq!(run(q, m, s, partition, true), buffered_reference);
        }
    }
}

#[shoop_test]
fn buffered_capacity_fault_silences_audio_and_recovers_rotated_notes() {
    {
        let mut host = Host::new(
            16,
            Timing {
                batch_quanta: 1,
                slack_quanta: 0,
            },
            false,
            MidiLimits {
                events_per_quantum: 1,
                bytes_per_quantum: 3,
            },
        );
        let mut input = MidiBuffer::new(2, 6).unwrap();
        let mut output = MidiBuffer::new(64, 4096).unwrap();
        let mut left = [0.; 16];
        let mut right = [0.; 16];
        for tick in 0..3 {
            input.clear();
            output.clear();
            if tick == 0 {
                input.push(0, 16, &[0x90, 60, 100]).unwrap();
            }
            if tick == 2 {
                input.push(0, 16, &[0x80, 60, 0]).unwrap();
                input.push(1, 16, &[0x90, 61, 100]).unwrap();
            }
            let result = host.process(
                AudioBlock {
                    frame: Frame(tick * 16),
                    frames: 16,
                    inputs: &[&[1.; 16], &[2.; 16]],
                    outputs: &mut [&mut left, &mut right],
                },
                MidiBlock {
                    inputs: core::slice::from_ref(&input),
                    outputs: core::slice::from_mut(&mut output),
                },
            );
            if tick == 1 {
                assert_eq!(output.get(0), Some((0, &[0x91, 60, 100][..])));
            }
            if tick < 2 {
                result.unwrap();
            } else {
                assert_eq!(result, Err(Error::Capacity));
            }
        }
        assert_eq!(left, [0.; 16]);
        assert_eq!(right, [0.; 16]);
        output.clear();
        assert_eq!(host.recovery(&mut output), 32);
        assert_eq!(output.get(2), Some((0, &[0xb1, 120, 0][..])));
        assert_eq!(output.get(3), Some((0, &[0xb1, 123, 0][..])));
        output.clear();
        assert_eq!(host.recovery(&mut output), 0);
        left.fill(9.);
        right.fill(9.);
        assert_eq!(
            host.process(
                AudioBlock {
                    frame: Frame(32),
                    frames: 16,
                    inputs: &[&[1.; 16], &[2.; 16]],
                    outputs: &mut [&mut left, &mut right],
                },
                MidiBlock {
                    inputs: core::slice::from_ref(&input),
                    outputs: core::slice::from_mut(&mut output)
                }
            ),
            Err(Error::Capacity)
        );
        assert_eq!(left, [0.; 16]);
        assert_eq!(right, [0.; 16]);
        assert!(output.is_empty());
    }
}
