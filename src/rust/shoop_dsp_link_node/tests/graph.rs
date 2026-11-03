use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::executor::{Control, GraphRuntime, PreparedNode};
use rt_runtime::graph::{Connection, Endpoint as Port, GraphDescription, PortType};
use rt_runtime::planner::NoPlanner;
use rt_runtime::resource::ResourceStore;
use rt_runtime::{Config, NodeId};
use shoop_dsp::DelayCopy;
use shoop_dsp_link::{memory, service::ReferenceService, Buffer, Descriptor, Endpoint, Header};
use shoop_dsp_link_node::RemoteNode;
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn descriptor() -> Descriptor {
    Descriptor {
        channels: 2,
        quantum: 3,
        dsp_frames: 5,
        max_callback: 7,
        service_allowance: 11,
        latency: 36,
        slots: 16,
        result_budget: 16,
        midi_capacity: 8,
        ..Descriptor::default()
    }
}
fn config(d: Descriptor) -> rt_runtime::PreparedConfig {
    Config {
        sample_rate: d.sample_rate,
        max_frames: d.max_callback as usize,
        audio_inputs: d.channels as usize,
        audio_outputs: d.channels as usize,
        midi_inputs: usize::from(d.midi_capacity != 0),
        midi_outputs: usize::from(d.midi_capacity != 0),
        max_nodes: 1,
        max_ports: 16,
        max_edges: 8,
        resource_slots: 1,
        resource_pages: 1,
        page_bytes: 64,
        ..Config::default()
    }
    .prepare()
    .unwrap()
}
fn description(d: Descriptor, node: NodeId) -> GraphDescription {
    let mut edges = Vec::new();
    for kind in [PortType::Audio, PortType::Midi] {
        let count = if kind == PortType::Audio {
            d.channels as u32
        } else {
            u32::from(d.midi_capacity != 0)
        };
        for p in 0..count {
            edges.push(Connection {
                source: Port::host(kind, p),
                destination: Port::node(node, kind, p),
            });
            edges.push(Connection {
                source: Port::node(node, kind, p),
                destination: Port::host(kind, p),
            });
        }
    }
    GraphDescription {
        nodes: vec![node],
        connections: edges,
    }
}
fn graph<E: Endpoint>(endpoint: E) -> (Control<RemoteNode<E>>, GraphRuntime<RemoteNode<E>>) {
    let d = endpoint.descriptor();
    let config = config(d);
    let node = NodeId::from_parts(0, 1).unwrap();
    let processor = RemoteNode::prepare(endpoint, config).unwrap();
    GraphRuntime::prepare_with_initial_graph(
        config,
        vec![PreparedNode::new(node, processor)],
        NoPlanner,
        ResourceStore::new(config).unwrap(),
        &description(d, node),
        &[],
    )
    .unwrap()
}
#[shoop_wasm_test_support::shoop_test]
fn actual_graph_preserves_l_plus_a_across_variable_callbacks_without_heap_work() {
    let d = descriptor();
    let (endpoint, peer) = memory::pair(d).unwrap();
    let (mut control, mut graph) = graph(endpoint);
    let mut service = ReferenceService::prepare(peer).unwrap();
    let mut dsp = DelayCopy::prepare(d.dsp_config(), 7).unwrap();
    let mut incoming = [MidiBuffer::new(16, 256).unwrap()];
    let mut outgoing = [MidiBuffer::new(64, 512).unwrap()];
    let mut checked = 0;
    let mut midi_count = 0;
    let initial_epoch = graph.epoch();
    for iteration in 0..200 {
        if iteration == 51 {
            let plan = rt_runtime::graph::compile(
                graph.catalog(),
                &description(d, NodeId::from_parts(0, 1).unwrap()),
            )
            .unwrap();
            assert!(control
                .submit(
                    rt_runtime::transport::Lane::Normal,
                    rt_runtime::executor::Command::commit(graph.epoch(), plan)
                )
                .is_ok());
        }
        let n = [1, 7, 2, 3, 5][iteration % 5];
        let frame = graph.next_frame().0;
        let mut left = [0.; 7];
        let mut right = [0.; 7];
        for i in 0..n {
            left[i] = (frame + i as u64 + 1) as f32;
            right[i] = -left[i];
        }
        incoming[0].clear();
        outgoing[0].clear();
        for i in 0..n {
            if (frame + i as u64) % 23 == 0 {
                incoming[0].push(i as u32, n, &[0x90, 60, 100]).unwrap();
            }
        }
        let mut out_left = [0.; 7];
        let mut out_right = [0.; 7];
        assert_no_heap(|| {
            graph.process(
                AudioBlock {
                    frame: graph.next_frame(),
                    frames: n,
                    inputs: &[&left[..n], &right[..n]],
                    outputs: &mut [&mut out_left[..n], &mut out_right[..n]],
                },
                MidiBlock {
                    inputs: &incoming,
                    outputs: &mut outgoing,
                },
            )
        })
        .unwrap();
        for i in 0..n {
            let f = frame + i as u64;
            let expected = if f < 43 { 0. } else { (f - 43 + 1) as f32 };
            assert_eq!(out_left[i], expected, "source frame {f}");
            assert_eq!(out_right[i], -expected);
            if expected != 0. {
                checked += 1;
            }
        }
        assert!(!outgoing[0].is_recovery());
        for i in 0..outgoing[0].len() {
            let (offset, bytes) = outgoing[0].get(i).unwrap();
            assert_eq!((frame + u64::from(offset) - 36) % 23, 0);
            assert_eq!(bytes, [0x90, 60, 100]);
            midi_count += 1;
        }
        assert_no_heap(|| service.poll(&mut dsp)).unwrap();
        while let Some(completion) = control.try_completion() {
            assert_eq!(completion.outcome, rt_runtime::transport::Outcome::Applied);
        }
    }
    assert!(checked > 500 && midi_count > 20);
    assert_ne!(graph.epoch(), initial_epoch);
}

#[shoop_wasm_test_support::shoop_test]
fn late_result_cannot_patch_the_remaining_samples_of_a_committed_block() {
    let d = Descriptor {
        quantum: 4,
        dsp_frames: 4,
        max_callback: 3,
        service_allowance: 0,
        latency: 20,
        ..descriptor()
    };
    let (endpoint, mut peer) = memory::pair(d).unwrap();
    let (_control, mut graph) = graph(endpoint);
    let mut midi = [MidiBuffer::new(1, 3).unwrap()];
    let incoming = [MidiBuffer::new(1, 3).unwrap()];
    let mut out = [0.; 3];
    for _ in 0..7 {
        assert_no_heap(|| {
            graph.process(
                AudioBlock {
                    frame: graph.next_frame(),
                    frames: 3,
                    inputs: &[&[1.; 3], &[1.; 3]],
                    outputs: &mut [&mut out, &mut [0.; 3]],
                },
                MidiBlock {
                    inputs: &incoming,
                    outputs: &mut midi,
                },
            )
        })
        .unwrap();
    }
    assert_eq!(graph.next_frame().0, 21);
    for (frame, value) in [(0, 42.), (4, 99.)] {
        let mut result = peer.acquire_output().unwrap();
        result.set_header(Header {
            generation: d.generation,
            frame,
            ..Header::default()
        });
        result.audio_mut().fill(value);
        assert!(peer.publish(result).is_ok());
    }
    assert_no_heap(|| {
        graph.process(
            AudioBlock {
                frame: graph.next_frame(),
                frames: 3,
                inputs: &[&[1.; 3], &[1.; 3]],
                outputs: &mut [&mut out, &mut [0.; 3]],
            },
            MidiBlock {
                inputs: &incoming,
                outputs: &mut midi,
            },
        )
    })
    .unwrap();
    assert_eq!(out, [0.; 3]);
    assert_no_heap(|| {
        graph.process(
            AudioBlock {
                frame: graph.next_frame(),
                frames: 3,
                inputs: &[&[1.; 3], &[1.; 3]],
                outputs: &mut [&mut out, &mut [0.; 3]],
            },
            MidiBlock {
                inputs: &incoming,
                outputs: &mut midi,
            },
        )
    })
    .unwrap();
    assert_eq!(out, [99.; 3]);
    drop(peer);
    assert_no_heap(|| {
        graph.process(
            AudioBlock {
                frame: graph.next_frame(),
                frames: 3,
                inputs: &[&[1.; 3], &[1.; 3]],
                outputs: &mut [&mut out, &mut [0.; 3]],
            },
            MidiBlock {
                inputs: &incoming,
                outputs: &mut midi,
            },
        )
    })
    .unwrap();
    assert_eq!(out[1..], [0., 0.]);
}

#[shoop_wasm_test_support::shoop_test]
fn midi_reset_uses_reserved_capacity_and_rejects_invalid_offset() {
    let mut midi = MidiBuffer::new(0, 0).unwrap();
    assert_no_heap(|| {
        assert!(midi.request_reset(3, 3).is_err());
        assert!(midi.is_empty());
        midi.request_reset(0, 1).unwrap();
        assert_eq!(midi.len(), 32);
        assert!(midi.is_recovery());
        assert!(midi.push(0, 1, &[0x90, 60, 100]).is_err());
    });
}

#[shoop_wasm_test_support::shoop_test]
fn service_failure_leaves_an_unrelated_graph_path_running() {
    let d = descriptor();
    let (endpoint, peer) = memory::pair(d).unwrap();
    let config = Config {
        audio_inputs: 3,
        audio_outputs: 3,
        max_ports: 32,
        ..config(d).limits()
    }
    .prepare()
    .unwrap();
    let id = NodeId::from_parts(0, 1).unwrap();
    let node = RemoteNode::prepare(endpoint, config).unwrap();
    let mut desc = description(d, id);
    desc.connections.push(Connection {
        source: Port::host(PortType::Audio, 2),
        destination: Port::host(PortType::Audio, 2),
    });
    let (_control, mut runtime) = GraphRuntime::<_, NoPlanner, ()>::prepare_with_initial_graph(
        config,
        vec![PreparedNode::new(id, node)],
        NoPlanner,
        ResourceStore::new(config).unwrap(),
        &desc,
        &[],
    )
    .unwrap();
    let incoming = [MidiBuffer::new(8, 256).unwrap()];
    let mut midi = [MidiBuffer::new(8, 256).unwrap()];
    drop(peer);
    for _ in 0..20 {
        let mut l = [9.; 7];
        let mut r = [9.; 7];
        let mut dry = [9.; 7];
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame: runtime.next_frame(),
                    frames: 7,
                    inputs: &[&[1.; 7], &[1.; 7], &[0.75; 7]],
                    outputs: &mut [&mut l, &mut r, &mut dry],
                },
                MidiBlock {
                    inputs: &incoming,
                    outputs: &mut midi,
                },
            )
        })
        .unwrap();
        assert_eq!(l, [0.; 7]);
        assert_eq!(r, [0.; 7]);
        assert_eq!(dry, [0.75; 7]);
    }
}

#[shoop_wasm_test_support::shoop_test]
fn impulses_survive_variable_ranges_and_both_sides_of_a_packet_boundary() {
    let d = descriptor();
    let (endpoint, peer) = memory::pair(d).unwrap();
    let (_control, mut runtime) = graph(endpoint);
    let mut service = ReferenceService::prepare(peer).unwrap();
    let mut dsp = DelayCopy::prepare(d.dsp_config(), 5).unwrap();
    let incoming = [MidiBuffer::new(1, 3).unwrap()];
    let mut midi = [MidiBuffer::new(1, 3).unwrap()];
    let mut impulses = 0;
    for k in 0..80 {
        let frames = [2, 7, 1, 3][k % 4];
        let start = runtime.next_frame().0;
        let mut input = [0.; 7];
        for (i, v) in input[..frames].iter_mut().enumerate() {
            if [0, 2, 3, 4].contains(&(start + i as u64)) {
                *v = 1.;
            }
        }
        let mut output = [0.; 7];
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame: runtime.next_frame(),
                    frames,
                    inputs: &[&input[..frames], &input[..frames]],
                    outputs: &mut [&mut output[..frames], &mut [0.; 7][..frames]],
                },
                MidiBlock {
                    inputs: &incoming,
                    outputs: &mut midi,
                },
            )
        })
        .unwrap();
        for (i, v) in output[..frames].iter().enumerate() {
            let f = start + i as u64;
            let expected = if [41, 43, 44, 45].contains(&f) {
                1.
            } else {
                0.
            };
            assert_eq!(*v, expected);
            if *v != 0. {
                impulses += 1;
            }
        }
        service.poll(&mut dsp).unwrap();
    }
    assert_eq!(impulses, 4);
}

#[shoop_wasm_test_support::shoop_test]
fn oversized_sysex_requests_recovery_instead_of_truncation() {
    let d = descriptor();
    let (endpoint, peer) = memory::pair(d).unwrap();
    let (_control, mut runtime) = graph(endpoint);
    let mut service = ReferenceService::prepare(peer).unwrap();
    let mut dsp = DelayCopy::prepare(d.dsp_config(), 0).unwrap();
    let mut incoming = [MidiBuffer::new(4, 256).unwrap()];
    let mut midi = [MidiBuffer::new(4, 256).unwrap()];
    let mut sysex = [0; 70];
    sysex[0] = 0xf0;
    sysex[69] = 0xf7;
    let mut resets = 0;
    let mut recovery = MidiBuffer::new(4, 64).unwrap();
    let mut ordinary = false;
    for i in 0..20 {
        incoming[0].clear();
        midi[0].clear();
        if i == 0 {
            incoming[0].push(0, 7, &sysex).unwrap();
        }
        if i == 12 {
            incoming[0].push(0, 7, &[0x90, 60, 100]).unwrap();
        }
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame: runtime.next_frame(),
                    frames: 7,
                    inputs: &[&[1.; 7], &[1.; 7]],
                    outputs: &mut [&mut [0.; 7], &mut [0.; 7]],
                },
                MidiBlock {
                    inputs: &incoming,
                    outputs: &mut midi,
                },
            )
        })
        .unwrap();
        for n in 0..midi[0].len() {
            assert_eq!(midi[0].get(n).unwrap().1.len(), 3);
            ordinary |= midi[0].get(n).unwrap().1 == [0x90, 60, 100];
        }
        recovery.clear();
        resets += assert_no_heap(|| runtime.drain_midi_recovery(0, &mut recovery, 7)).unwrap();
        service.poll(&mut dsp).unwrap();
    }
    assert!(resets >= 32 && ordinary);
    assert!(!runtime.midi_recovery_pending(0).unwrap());
}

#[shoop_wasm_test_support::shoop_test]
fn continuously_ready_peer_cannot_extend_the_render_drain_budget() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Endless {
        d: Descriptor,
        block: Option<shoop_dsp_link::Block>,
        reads: Arc<AtomicUsize>,
    }
    impl Endpoint for Endless {
        type Lease = shoop_dsp_link::Block;
        fn descriptor(&self) -> Descriptor {
            self.d
        }
        fn available(&self) -> bool {
            true
        }
        fn acquire_input(&mut self) -> Option<Self::Lease> {
            None
        }
        fn submit(&mut self, b: Self::Lease) -> Result<(), Self::Lease> {
            Err(b)
        }
        fn release_input(&mut self, b: Self::Lease) {
            self.block = Some(b);
        }
        fn receive(&mut self) -> Option<Self::Lease> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            self.block.take()
        }
        fn release_output(&mut self, b: Self::Lease) {
            self.block = Some(b);
        }
    }
    let d = descriptor();
    let reads = Arc::new(AtomicUsize::new(0));
    let endpoint = Endless {
        d,
        block: Some(shoop_dsp_link::Block::prepare(d).unwrap()),
        reads: reads.clone(),
    };
    let (_control, mut runtime) = graph(endpoint);
    let incoming = [MidiBuffer::new(1, 3).unwrap()];
    let mut midi = [MidiBuffer::new(1, 3).unwrap()];
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 7,
                inputs: &[&[1.; 7], &[1.; 7]],
                outputs: &mut [&mut [0.; 7], &mut [0.; 7]],
            },
            MidiBlock {
                inputs: &incoming,
                outputs: &mut midi,
            },
        )
    })
    .unwrap();
    assert_eq!(reads.load(Ordering::Relaxed), d.result_budget as usize);
}

#[shoop_wasm_test_support::shoop_test]
fn graph_retirement_reclaims_off_render_and_fresh_incarnation_resumes_at_a_new_origin() {
    use rt_runtime::{
        executor::{Command, Retired},
        transport::{Lane, Outcome},
    };
    fn step(runtime: &mut GraphRuntime<RemoteNode<memory::RenderEndpoint>>) -> f32 {
        let mut out = [0.];
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame: runtime.next_frame(),
                    frames: 1,
                    inputs: &[&[2.], &[2.]],
                    outputs: &mut [&mut out, &mut [0.]],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
        })
        .unwrap();
        out[0]
    }
    let d = Descriptor {
        midi_capacity: 0,
        ..descriptor()
    };
    let (endpoint, peer) = memory::pair(d).unwrap();
    let (mut control, mut runtime) = graph(endpoint);
    let mut service = ReferenceService::prepare(peer).unwrap();
    let mut dsp = DelayCopy::prepare(d.dsp_config(), 0).unwrap();
    for _ in 0..60 {
        step(&mut runtime);
        service.poll(&mut dsp).unwrap();
    }
    let empty =
        rt_runtime::graph::compile(runtime.catalog(), &GraphDescription::default()).unwrap();
    assert!(control
        .submit(Lane::Normal, Command::commit(runtime.epoch(), empty))
        .is_ok());
    step(&mut runtime);
    assert_eq!(control.try_completion().unwrap().outcome, Outcome::Applied);
    let old = NodeId::from_parts(0, 1).unwrap();
    assert!(control
        .submit(Lane::Normal, Command::Remove { node: old })
        .is_ok());
    step(&mut runtime);
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.outcome, Outcome::Applied);
    let Some(Retired::Processor(retired)) = completion.retired else {
        panic!("expected retired remote node")
    };
    assert!(retired.stats().processed > 0);
    drop(retired);
    drop(service);
    let fresh = old.checked_next_generation().unwrap();
    let d2 = Descriptor {
        generation: 777,
        start_frame: runtime.next_frame().0 + 1,
        ..d
    };
    let (endpoint, mut peer) = memory::pair(d2).unwrap();
    let node = RemoteNode::prepare(endpoint, config(d2)).unwrap();
    assert!(control
        .submit(
            Lane::Normal,
            Command::Install {
                prepared: Some(PreparedNode::new(fresh, node))
            }
        )
        .is_ok());
    step(&mut runtime);
    assert_eq!(control.try_completion().unwrap().outcome, Outcome::Applied);
    let plan = rt_runtime::graph::compile(runtime.catalog(), &description(d2, fresh)).unwrap();
    assert!(control
        .submit(Lane::Normal, Command::commit(runtime.epoch(), plan))
        .is_ok());
    step(&mut runtime);
    assert_eq!(control.try_completion().unwrap().outcome, Outcome::Applied);
    let mut stale = peer.acquire_output().unwrap();
    stale.set_header(Header {
        generation: 1,
        frame: d2.start_frame,
        ..Header::default()
    });
    stale.audio_mut().fill(99.);
    assert!(peer.publish(stale).is_ok());
    let mut service = ReferenceService::prepare(peer).unwrap();
    let mut dsp = DelayCopy::prepare(d2.dsp_config(), 0).unwrap();
    let mut audible = 0;
    for _ in 0..80 {
        let f = runtime.next_frame().0;
        let out = step(&mut runtime);
        assert_eq!(
            out,
            if f < d2.start_frame + u64::from(d2.latency) {
                0.
            } else {
                2.
            }
        );
        if out == 2. {
            audible += 1;
        }
        service.poll(&mut dsp).unwrap();
    }
    assert!(audible > 20);
}
