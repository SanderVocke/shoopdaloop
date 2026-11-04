use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{Control, GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortType},
    resource::{self, CaptureGroupConfig, MidiIndexConfig, ResourceStore},
    transport::Lane,
    Config, NodeId, PlannerId, Quantum, Sequence,
};
use shoop_loops::{media, runtime::*, *};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

type Runtime = GraphRuntime<LoopNode, LoopPlanner>;
struct Harness {
    runtime: Runtime,
    control: Control<LoopNode, LoopPlanner>,
    client: LoopClient,
    inputs: Vec<Vec<f32>>,
    outputs: Vec<Vec<f32>>,
    midi_in: [MidiBuffer; 1],
    midi_out: [MidiBuffer; 1],
    q: usize,
}
fn id() -> LoopId {
    LoopId::from_parts(0, 1).unwrap()
}
fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
impl Harness {
    fn new(channels: usize, q: usize) -> Self {
        Self::with_pages(channels, q, 1024)
    }
    fn with_pages(channels: usize, q: usize, shared_pages: usize) -> Self {
        Self::with_shape(channels, channels, q, shared_pages)
    }
    fn with_shape(channels: usize, dry: usize, q: usize, shared_pages: usize) -> Self {
        let total = channels + dry;
        let config = Config {
            max_frames: q,
            audio_inputs: total,
            audio_outputs: total,
            midi_inputs: 1,
            midi_outputs: 1,
            max_nodes: 2,
            max_ports: total * 4 + 4,
            max_edges: total * 2 + 2,
            resource_slots: 64,
            resource_pages: 2048,
            page_bytes: 256,
            midi_events_per_port: 64,
            midi_bytes_per_port: 512,
            ..Config::default()
        }
        .prepare()
        .unwrap();
        let quantum = Quantum::new(q).unwrap();
        let mut resources = ResourceStore::with_capture_groups(
            config,
            quantum,
            Some(MidiIndexConfig {
                versions: 8,
                checkpoints_per_version: 8,
                replay_events: 128,
            }),
            CaptureGroupConfig {
                groups: 4,
                members_per_group: 3,
                staging_bytes_per_group: q * total * 4 + 8192,
            },
        )
        .unwrap();
        let topology = compile(
            Limits::default(),
            &TopologyDescription {
                loops: vec![LoopSpec {
                    id: id(),
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: Some(id()),
                }],
                ..TopologyDescription::default()
            },
        )
        .unwrap();
        let mut prepared = LoopPlanner::prepare(
            config,
            quantum,
            topology,
            &mut resources,
            Settings {
                planner: PlannerId::from_parts(0, 1).unwrap(),
                normal_requests: 8,
                media_slots: 16,
                export_slots: 8,
            },
            vec![Binding {
                target: id(),
                playback: node(0),
                capture: node(1),
                channels,
                auxiliary_channels: dry,
                dry_wet: true,
                midi: true,
                gain: 1.,
                smoothing: audio::Smoothing::None,
                midi_events: 64,
                midi_bytes: 512,
                packet_bytes: 512,
            }],
            vec![],
            Options::default(),
        )
        .unwrap();
        prepared
            .planner
            .prepare_shared(
                &mut resources,
                SharedSettings {
                    histories: vec![HistorySettings {
                        source: 0,
                        audio: Some(resource::shared_audio::Config {
                            channels: channels as u32,
                            history_frames: q * 4,
                            views: 32,
                            pages: shared_pages,
                        }),
                        midi: None,
                    }],
                    ..SharedSettings::default()
                },
            )
            .unwrap();
        let mut graph = GraphDescription {
            nodes: vec![node(0), node(1)],
            connections: vec![],
        };
        for port in 0..total {
            graph.connections.push(Connection {
                source: Endpoint::host(PortType::Audio, port as u32),
                destination: Endpoint::node(node(1), PortType::Audio, port as u32),
            });
            graph.connections.push(Connection {
                source: Endpoint::node(node(0), PortType::Audio, port as u32),
                destination: Endpoint::host(PortType::Audio, port as u32),
            });
        }
        graph.connections.extend([
            Connection {
                source: Endpoint::host(PortType::Midi, 0),
                destination: Endpoint::node(node(1), PortType::Midi, 0),
            },
            Connection {
                source: Endpoint::node(node(0), PortType::Midi, 0),
                destination: Endpoint::host(PortType::Midi, 0),
            },
        ]);
        let nodes = prepared
            .nodes
            .into_iter()
            .map(|(id, processor)| PreparedNode::new(id, processor))
            .collect();
        let (mut control, runtime) = GraphRuntime::prepare_with_initial_graph(
            config,
            quantum,
            nodes,
            prepared.planner,
            resources,
            &graph,
        )
        .unwrap();
        control.prepare_submissions().unwrap();
        Self {
            runtime,
            control,
            client: prepared.client,
            inputs: vec![vec![0.; q]; total],
            outputs: vec![vec![0.; q]; total],
            midi_in: [MidiBuffer::new(64, 512).unwrap()],
            midi_out: [MidiBuffer::new(64, 512).unwrap()],
            q,
        }
    }
    fn send(&mut self, op: Operation) -> Sequence {
        let at = self.runtime.next_frame();
        assert_no_heap(|| {
            self.control
                .submit_prepared(Lane::Normal, self.client.command(at, op))
                .unwrap_or_else(|e| panic!("{:?}", e.error))
                .sequence()
        })
    }
    fn step(&mut self) {
        let inputs: Vec<_> = self.inputs.iter().map(Vec::as_slice).collect();
        let mut outputs: Vec<_> = self.outputs.iter_mut().map(Vec::as_mut_slice).collect();
        assert_no_heap(|| {
            self.runtime
                .process(
                    AudioBlock {
                        frame: self.runtime.next_frame(),
                        frames: self.q,
                        inputs: &inputs,
                        outputs: &mut outputs,
                    },
                    MidiBlock {
                        inputs: &self.midi_in,
                        outputs: &mut self.midi_out,
                    },
                )
                .unwrap()
        });
        self.midi_in[0].clear();
        while let Some(completion) = self.control.try_completion() {
            assert!(matches!(
                completion.outcome,
                rt_runtime::transport::Outcome::Applied
                    | rt_runtime::transport::Outcome::Scheduled { .. }
            ));
        }
    }
    fn transition(&mut self, transition: Transition) {
        self.send(Operation::Transition {
            target: id(),
            timing: Timing::At,
            transition,
        });
    }
    fn export(&mut self) -> media::Export {
        let seq = self.send(Operation::Export {
            target: id(),
            selection: Selection::Playback,
        });
        self.step();
        while let Some(execution) = self.control.try_execution() {
            assert_eq!(execution.result, Ok(()));
            assert!(!matches!(
                execution.operation_result,
                Some(Reply::Failed(_))
                    | Some(Reply::Transition(Completion {
                        outcome: Outcome::Failed { .. },
                        ..
                    }))
            ));
            if execution.sequence == seq {
                let Some(Reply::Export { media, .. }) = execution.operation_result else {
                    panic!("export failed: {:?}", execution.result);
                };
                return media;
            }
        }
        panic!("missing export");
    }
}
#[shoop_test]
fn independent_maximum_dry_and_wet_sources_survive_wrapping_wet_rerecord() {
    for channels in [1, 2, 8] {
        for q in [1, 16, 32, 64, 128] {
            let mut h = Harness::new(channels, q);
            h.transition(Transition::Record {
                monitor_previous: false,
            });
            for tick in 0..4 {
                for channel in 0..channels {
                    h.inputs[channel].fill((10 + channel * 4 + tick) as f32);
                    h.inputs[channels + channel].fill((100 + channel * 4 + tick) as f32);
                }
                if tick == 0 {
                    h.midi_in[0].push(0, q, &[0x90, 60, 90]).unwrap();
                }
                if tick == 3 {
                    h.midi_in[0].push(0, q, &[0x80, 60, 0]).unwrap();
                }
                h.step();
            }
            h.transition(Transition::Stop);
            let original = h.export();
            assert_eq!(original.view.description.frames, (q * 4) as u64);
            assert_eq!(
                original.view.description.auxiliary_channels,
                channels as u32
            );
            h.transition(Transition::Play {
                position: 0,
                retrigger: false,
            });
            h.step();
            for channel in 0..channels {
                assert_eq!(h.outputs[channel], vec![(10 + channel * 4) as f32; q]);
                assert_eq!(h.outputs[channels + channel], vec![0.; q]);
            }
            assert!(h.midi_out[0].is_empty());
            h.transition(Transition::PlayDry {
                position: 0,
                retrigger: true,
            });
            h.step();
            for channel in 0..channels {
                assert_eq!(h.outputs[channel], vec![0.; q]);
                assert_eq!(
                    h.outputs[channels + channel],
                    vec![(100 + channel * 4) as f32; q]
                );
            }
            h.transition(Transition::Rerecord);
            for tick in 0..6 {
                for channel in 0..channels {
                    h.inputs[channel].fill((1000 + tick * 10 + channel) as f32);
                }
                h.step();
            }
            h.transition(Transition::Stop);
            let edited = h.export();
            assert_eq!(
                edited.view.description.frames,
                original.view.description.frames
            );
            let old_dry = original.view.auxiliary.unwrap().snapshot;
            let new_dry = edited.view.auxiliary.unwrap().snapshot;
            assert_eq!(
                (old_dry.resource, old_dry.revision, old_dry.description),
                (new_dry.resource, new_dry.revision, new_dry.description)
            );
            assert_eq!(
                original.view.midi.unwrap().snapshot.resource,
                edited.view.midi.unwrap().snapshot.resource
            );
            let (_, mut resources) = h.runtime.into_planner_and_resources();
            for (source, role) in [
                (original.view.audio.unwrap(), 0),
                (edited.view.audio.unwrap(), 1),
                (edited.view.auxiliary.unwrap(), 2),
            ] {
                let mut cursor = resources.cursor(source.snapshot.id).unwrap();
                let mut bytes = vec![0; q * 4 * channels * 4];
                let mut read = 0;
                while read < bytes.len() {
                    resources.begin_callback();
                    let end = (read + 256).min(bytes.len());
                    read += resources.read(&mut cursor, &mut bytes[read..end]).unwrap();
                }
                for frame in 0..q * 4 {
                    for channel in 0..channels {
                        let offset = (frame * channels + channel) * 4;
                        let expected = match role {
                            0 => (10 + channel * 4 + frame / q) as f32,
                            1 => (1000 + [30, 40, 50, 20][frame / q] + channel) as f32,
                            _ => (100 + channel * 4 + frame / q) as f32,
                        };
                        assert_eq!(
                            f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()),
                            expected,
                            "channels={channels} q={q} role={role} frame={frame}"
                        );
                    }
                }
            }
        }
    }
}

#[shoop_test]
fn refused_wet_edit_returns_all_reservations_and_keeps_dry_playback_running() {
    let mut h = Harness::with_pages(8, 64, 128);
    for samples in &mut h.inputs {
        samples.fill(7.);
    }
    h.transition(Transition::Record {
        monitor_previous: false,
    });
    for _ in 0..4 {
        h.step();
    }
    h.transition(Transition::Stop);
    let before = h.export();
    h.transition(Transition::PlayDry {
        position: 0,
        retrigger: false,
    });
    h.step();
    while h.control.try_execution().is_some() {}
    let snapshots = h.runtime.resource_usage().free_snapshots;
    for _ in 0..20 {
        let sequence = h.send(Operation::Transition {
            target: id(),
            timing: Timing::At,
            transition: Transition::Rerecord,
        });
        h.step();
        let mut outcomes = 0;
        while let Some(execution) = h.control.try_execution() {
            if execution.sequence == sequence {
                assert!(matches!(
                    execution.operation_result,
                    Some(Reply::Transition(Completion {
                        outcome: Outcome::Failed {
                            reason: Error::Capacity,
                            ..
                        },
                        ..
                    }))
                ));
                outcomes += 1;
            }
        }
        assert_eq!(outcomes, 1);
        assert_eq!(h.runtime.resource_usage().free_snapshots, snapshots);
        for output in &h.outputs[8..] {
            assert!(output.iter().all(|sample| *sample == 7.));
        }
        // Query state without acquiring another export owner.
        let sequence = h.send(Operation::State { target: id() });
        h.step();
        let mut seen = false;
        while let Some(execution) = h.control.try_execution() {
            if execution.sequence == sequence {
                let Some(Reply::State(cut)) = execution.operation_result else {
                    panic!("state missing");
                };
                assert_eq!(cut.playback, Some(before.media));
                assert_eq!(cut.state.mode, Mode::PlayingDryThroughWet);
                seen = true;
            }
        }
        assert!(seen);
    }
}

#[shoop_test]
fn cancelled_rerecord_does_not_claim_media_or_double_count_wraps() {
    let mut h = Harness::new(2, 16);
    for samples in &mut h.inputs {
        samples.fill(3.);
    }
    h.transition(Transition::Record {
        monitor_previous: false,
    });
    for _ in 0..4 {
        h.step();
    }
    h.transition(Transition::Stop);
    let original = h.export();
    h.transition(Transition::PlayDry {
        position: 0,
        retrigger: true,
    });
    h.step();
    while h.control.try_execution().is_some() {}
    let snapshots = h.runtime.resource_usage().free_snapshots;
    let sequence = h.send(Operation::Transition {
        target: id(),
        timing: Timing::AfterTicks {
            source: id(),
            skip: 1,
        },
        transition: Transition::Rerecord,
    });
    h.send(Operation::Cancel { sequence });
    h.step();
    for _ in 0..8 {
        h.step();
    }
    let query = h.send(Operation::State { target: id() });
    h.step();
    let mut cancelled = 0;
    let mut state = None;
    while let Some(execution) = h.control.try_execution() {
        if execution.sequence == sequence {
            assert!(matches!(
                execution.operation_result,
                Some(Reply::Transition(Completion {
                    outcome: Outcome::Cancelled { .. },
                    ..
                }))
            ));
            cancelled += 1;
        }
        if execution.sequence == query {
            let Some(Reply::State(cut)) = execution.operation_result else {
                panic!("state missing");
            };
            state = Some(cut);
        }
    }
    let cut = state.unwrap();
    assert_eq!(cut.state.mode, Mode::PlayingDryThroughWet);
    assert_eq!(cut.playback, Some(original.media));
    assert_eq!(cut.state.cycles, 2);
    assert_eq!(h.runtime.resource_usage().free_snapshots, snapshots);
    assert_eq!(cancelled, 1);
}

#[shoop_test]
fn dry_and_wet_widths_are_independent_and_each_uses_its_full_capacity() {
    for (wet, dry) in [(1, 8), (8, 1), (2, 5), (5, 2)] {
        let mut h = Harness::with_shape(wet, dry, 32, 1024);
        for (index, input) in h.inputs.iter_mut().enumerate() {
            input.fill((index + 1) as f32);
        }
        h.transition(Transition::Record {
            monitor_previous: false,
        });
        for _ in 0..4 {
            h.step();
        }
        h.transition(Transition::Stop);
        let original = h.export();
        assert_eq!(
            original.view.audio.unwrap().snapshot.description.format,
            resource::Format::Audio {
                channels: wet as u32
            }
        );
        assert_eq!(
            original.view.auxiliary.unwrap().snapshot.description.format,
            resource::Format::Audio {
                channels: dry as u32
            }
        );
        h.transition(Transition::PlayDry {
            position: 0,
            retrigger: true,
        });
        h.step();
        for channel in 0..wet {
            assert_eq!(h.outputs[channel], vec![0.; 32]);
        }
        for channel in 0..dry {
            assert_eq!(
                h.outputs[wet + channel],
                vec![(wet + channel + 1) as f32; 32]
            );
        }
        h.transition(Transition::Rerecord);
        for channel in 0..wet {
            h.inputs[channel].fill(-1.);
        }
        for _ in 0..8 {
            h.step();
        }
        h.transition(Transition::Stop);
        let edited = h.export();
        assert_eq!(
            edited.view.auxiliary.unwrap().snapshot.resource,
            original.view.auxiliary.unwrap().snapshot.resource
        );
        h.transition(Transition::Play {
            position: 0,
            retrigger: true,
        });
        h.step();
        for channel in 0..wet {
            assert_eq!(h.outputs[channel], vec![-1.; 32]);
        }
        for channel in 0..dry {
            assert_eq!(h.outputs[wet + channel], vec![0.; 32]);
        }
    }
}

#[shoop_test]
fn unaligned_dry_seek_is_refused_before_changing_the_usable_take() {
    let mut h = Harness::new(2, 16);
    for input in &mut h.inputs {
        input.fill(0.5);
    }
    h.transition(Transition::Record {
        monitor_previous: false,
    });
    for _ in 0..4 {
        h.step();
    }
    h.transition(Transition::Stop);
    let original = h.export();
    h.transition(Transition::Play {
        position: 0,
        retrigger: true,
    });
    h.step();
    while h.control.try_execution().is_some() {}
    let sequence = h.send(Operation::Transition {
        target: id(),
        timing: Timing::At,
        transition: Transition::PlayDry {
            position: 1,
            retrigger: true,
        },
    });
    h.step();
    let mut refused = 0;
    while let Some(execution) = h.control.try_execution() {
        if execution.sequence == sequence {
            assert!(matches!(
                execution.operation_result,
                Some(Reply::Failed(Failure::Timeline(Error::InvalidFrame)))
            ));
            refused += 1;
        }
    }
    assert_eq!(refused, 1);
    assert_eq!(h.export().media, original.media);
    for output in &h.outputs[..2] {
        assert_eq!(output, &vec![0.5; 16]);
    }
}

#[shoop_test]
fn source_demand_enters_prerouting_only_in_the_last_cycle_and_cancellation_clears_it() {
    for (steps, cancel, pre) in [(0, false, false), (2, false, true), (2, true, false)] {
        let mut h = Harness::new(2, 16);
        h.transition(Transition::Record {
            monitor_previous: false,
        });
        for _ in 0..4 {
            h.step();
        }
        h.transition(Transition::Play {
            position: 0,
            retrigger: true,
        });
        h.step();
        let sequence = h.send(Operation::Transition {
            target: id(),
            timing: Timing::AfterTicks {
                source: id(),
                skip: 1,
            },
            transition: Transition::Rerecord,
        });
        h.step();
        for _ in 0..steps {
            h.step();
        }
        if cancel {
            h.send(Operation::Cancel { sequence });
        }
        h.step();
        let (planner, _) = h.runtime.into_planner_and_resources();
        let activity = planner.source_activity(0);
        assert_eq!(
            activity.pre_rerecording, pre,
            "steps={steps} cancel={cancel}"
        );
        assert!(!activity.rerecording);
        assert!(!activity.recording);
        assert_eq!(planner.source_activity(1), SourceActivity::default());
    }
}
