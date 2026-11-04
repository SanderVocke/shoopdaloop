mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{Control, GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    planner::NoPlanner,
    processor::{ProcessBlock, Processor},
    profiling::{self, Clock, Collector, Reader},
    resource::ResourceStore,
    Config, Error, NodeId, Quantum,
};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

std::thread_local! { static TIME: core::cell::Cell<u64> = const { core::cell::Cell::new(0) }; }
fn now() -> u64 {
    TIME.with(|time| {
        let at = time.get();
        time.set(at + 100);
        at
    })
}
fn id(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
struct Gain {
    fail: bool,
    cost: u64,
}
impl Processor for Gain {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            midi_inputs: 1,
            midi_outputs: 1,
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        if self.fail {
            return Err(Error::InvalidState);
        }
        TIME.with(|time| time.set(time.get() + self.cost));
        let (input, output) = block.audio.pair(0, 0).unwrap();
        for (input, output) in input.iter().zip(output) {
            *output = input * 0.5;
        }
        for i in 0..block.midi_inputs[0].len() {
            let (offset, bytes) = block.midi_inputs[0].get(i).unwrap();
            block.midi_outputs[0].push(offset, block.frames, bytes)?;
        }
        Ok(())
    }
}
fn prepare(
    q: usize,
    enabled: bool,
    fail: bool,
    work: bool,
) -> (
    GraphRuntime<Gain>,
    Control<Gain>,
    profiling::Control,
    Reader,
) {
    let config = Config {
        sample_rate: 48000,
        max_frames: q,
        max_nodes: 2,
        max_ports: 16,
        max_edges: 8,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 1,
        midi_outputs: 1,
        midi_events_per_port: 32,
        midi_bytes_per_port: 256,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let nodes = vec![
        PreparedNode::new(
            id(0),
            Gain {
                fail,
                cost: if work { 500 } else { 0 },
            },
        ),
        PreparedNode::new(
            id(1),
            Gain {
                fail: false,
                cost: if work { 300 } else { 0 },
            },
        ),
    ];
    let (control, mut runtime) = GraphRuntime::prepare_for_host(
        config,
        256,
        Quantum::new(q).unwrap(),
        nodes,
        NoPlanner,
        ResourceStore::new(config, Quantum::new(q).unwrap()).unwrap(),
    )
    .unwrap();
    let mut edges = Vec::new();
    for kind in [PortType::Audio, PortType::Midi] {
        edges.extend([
            Connection {
                source: Endpoint::host(kind, 0),
                destination: Endpoint::node(id(1), kind, 0),
            },
            Connection {
                source: Endpoint::node(id(1), kind, 0),
                destination: Endpoint::node(id(0), kind, 0),
            },
            Connection {
                source: Endpoint::node(id(0), kind, 0),
                destination: Endpoint::host(kind, 0),
            },
        ]);
    }
    runtime
        .prepare_initial_graph(&GraphDescription {
            nodes: vec![id(0), id(1)],
            connections: edges,
        })
        .unwrap();
    runtime.prepare_host_buffering().unwrap();
    let (profile, mut recorder, reader) = profiling::prepare(2).unwrap();
    recorder.set_clock(Clock {
        now,
        resolution_ns: 100,
    });
    profile.set_enabled(enabled);
    runtime.set_profiling(recorder);
    (runtime, control, profile, reader)
}

#[shoop_test]
fn profiling_preserves_audio_and_midi_through_arbitrary_callback_partitions() {
    for q in [1, 16, 32, 64, 128] {
        for enabled in [false, true] {
            let (mut runtime, _control, profile, mut reader) = prepare(q, enabled, false, false);
            let mut sent = Vec::new();
            let mut received = Vec::new();
            let mut midi_in = MidiBuffer::new(32, 256).unwrap();
            let mut midi_out = MidiBuffer::new(32, 256).unwrap();
            let before = TIME.with(core::cell::Cell::get);
            for frames in [1, 7, 17, 33, 129, 256, 23] {
                let frame = runtime.next_frame();
                let input: Vec<_> = (0..frames)
                    .map(|i| ((frame.0 + i as u64) % 32) as f32)
                    .collect();
                let mut output = vec![99.; frames];
                midi_in.clear();
                let note = [0x90, (frame.0 % 100) as u8, 100];
                midi_in.push(0, frames, &note).unwrap();
                sent.push((frame.0 + q as u64, note.to_vec()));
                assert_no_heap(|| {
                    runtime.process(
                        AudioBlock {
                            frame,
                            frames,
                            inputs: &[&input],
                            outputs: &mut [&mut output],
                        },
                        MidiBlock {
                            inputs: core::slice::from_ref(&midi_in),
                            outputs: core::slice::from_mut(&mut midi_out),
                        },
                    )
                })
                .unwrap();
                for (i, sample) in output.iter().enumerate() {
                    let at = frame.0 + i as u64;
                    let expected = if at < q as u64 {
                        0.
                    } else {
                        ((at - q as u64) % 32) as f32 * 0.25
                    };
                    assert_eq!(*sample, expected, "q={q}, enabled={enabled}, frame={at}");
                }
                for i in 0..midi_out.len() {
                    let (offset, bytes) = midi_out.get(i).unwrap();
                    received.push((frame.0 + u64::from(offset), bytes.to_vec()));
                }
            }
            let expected: Vec<_> = sent
                .into_iter()
                .filter(|(frame, _)| *frame < runtime.next_frame().0)
                .collect();
            assert_eq!(received, expected);
            if !enabled {
                assert_eq!(TIME.with(core::cell::Cell::get), before);
            }
            let mut collector = Collector::default();
            collector.status(profile.status());
            for _ in 0..1024 {
                if let Some(packet) = reader.next_packet() {
                    collector.receive(packet);
                }
            }
            profile.set_enabled(false);
            let mut status = profile.status();
            status.active = false;
            collector.status(status);
            let report = collector.view();
            if enabled {
                assert_eq!(
                    report.live.rows.iter().map(|r| r.key).collect::<Vec<_>>(),
                    [1, 0]
                );
                assert!(report.live.rows.iter().all(|row| row.avg_ns == 100.));
            } else {
                assert!(report.live.rows.is_empty());
            }
        }
    }
}

#[shoop_test]
fn partial_fault_does_not_report_an_unfinished_node_as_zero_duration() {
    let (mut runtime, _control, profile, mut reader) = prepare(32, true, true, false);
    let mut output = [99.; 64];
    let input = [1.; 64];
    let midi_in = MidiBuffer::new(32, 256).unwrap();
    let mut midi_out = MidiBuffer::new(32, 256).unwrap();
    assert_eq!(
        assert_no_heap(|| runtime.process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 64,
                inputs: &[&input],
                outputs: &mut [&mut output]
            },
            MidiBlock {
                inputs: core::slice::from_ref(&midi_in),
                outputs: core::slice::from_mut(&mut midi_out)
            }
        )),
        Err(Error::InvalidState)
    );
    assert!(output.iter().all(|v| *v == 0.));
    let mut collector = Collector::default();
    collector.status(profile.status());
    for _ in 0..32 {
        if let Some(packet) = reader.next_packet() {
            collector.receive(packet);
        }
    }
    let report = collector.view();
    assert!(report.live.incomplete);
    assert_eq!(report.live.rows.len(), 2);
    assert_eq!(report.live.rows[0].key, 1);
    assert_eq!(report.live.rows[0].samples, 1);
    assert_eq!(report.live.rows[1].key, 0);
    assert_eq!(report.live.rows[1].samples, 0);
}

#[shoop_test]
fn node_boundaries_include_independently_prescribed_work() {
    let (mut runtime, _control, profile, mut reader) = prepare(32, true, false, true);
    let input = [1.; 64];
    let mut output = [0.; 64];
    let midi_in = MidiBuffer::new(32, 256).unwrap();
    let mut midi_out = MidiBuffer::new(32, 256).unwrap();
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 64,
                inputs: &[&input],
                outputs: &mut [&mut output],
            },
            MidiBlock {
                inputs: core::slice::from_ref(&midi_in),
                outputs: core::slice::from_mut(&mut midi_out),
            },
        )
    })
    .unwrap();
    let mut collector = Collector::default();
    collector.status(profile.status());
    let mut phases = Vec::new();
    let mut nodes = 0;
    for _ in 0..64 {
        if let Some(packet) = reader.next_packet() {
            let mut previous = None;
            for sample in &packet.samples[..usize::from(packet.len)] {
                if sample.key & profiling::PHASE != 0 {
                    phases.push(sample.key);
                    if sample.key == profiling::PHASE | profiling::Phase::GraphStart as u64 {
                        previous = Some(sample.at);
                    }
                } else if sample.key & profiling::LAYOUT == 0 {
                    assert_eq!(
                        sample.at - previous.unwrap(),
                        if sample.key == 1 { 400 } else { 600 }
                    );
                    previous = Some(sample.at);
                    nodes += 1;
                }
            }
            collector.receive(packet);
        }
    }
    assert_eq!(nodes, 4);
    for phase in [
        profiling::Phase::Commands,
        profiling::Phase::StateSettled,
        profiling::Phase::OutputRouting,
        profiling::Phase::StateAdvanced,
        profiling::Phase::CallbackEnd,
    ] {
        assert!(phases.contains(&(profiling::PHASE | phase as u64)));
    }
    let view = collector.view();
    assert_eq!(view.live.rows[0].avg_ns, 400.);
    assert_eq!(view.live.rows[1].avg_ns, 600.);
}
