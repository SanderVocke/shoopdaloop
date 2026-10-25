#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{Command, GraphRuntime, PreparedNode},
    graph::{self, Connection, Endpoint, GraphDescription, PortCounts, PortType},
    midi_state::MidiState,
    processor::{ProcessBlock, Processor},
    resource::*,
    transport::Lane,
    Budgets, Config, Error, Frame, NodeId, Revision,
};
use shoop_loop_engine::midi::{MidiCapture, MidiRenderer, Playback};
use shoop_wasm_test_support::shoop_test;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn config() -> rt_runtime::PreparedConfig {
    Config {
        max_frames: 64,
        resource_slots: 8,
        resource_pages: 256,
        page_bytes: 16,
        midi_events_per_port: 512,
        midi_bytes_per_port: 2048,
        budgets: Budgets {
            transfer_bytes: 16,
            validation_steps: 1024,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap()
}
fn store_with(cfg: rt_runtime::PreparedConfig) -> ResourceStore {
    ResourceStore::with_capture_groups(
        cfg,
        Some(MidiIndexConfig {
            versions: 4,
            checkpoints_per_version: 16,
            replay_events: 4,
        }),
        CaptureGroupConfig {
            groups: 1,
            members_per_group: 1,
            staging_bytes_per_group: 1024,
        },
    )
    .unwrap()
}
fn store() -> ResourceStore {
    store_with(config())
}
fn upload(
    store: &mut ResourceStore,
    events: &[(u64, Vec<u8>)],
    frames: u64,
    initial: &MidiState,
) -> Snapshot {
    let resource = store.create().unwrap();
    let mut data = Vec::new();
    for (frame, packet) in events {
        let mut record = vec![0; 12 + packet.len()];
        encode_midi_record(*frame, packet, &mut record).unwrap();
        data.extend(record);
    }
    let token = store
        .begin_indexed_midi_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Midi,
                frames,
                bytes: data.len(),
            },
            initial,
        )
        .unwrap();
    for (page, chunk) in data.chunks(16).enumerate() {
        store.begin_callback();
        store.write_upload(token, page * 16, chunk).unwrap();
    }
    store.finish_upload(token).unwrap();
    store.commit(token).unwrap();
    store.snapshot(resource, Frame(0)).unwrap()
}
fn cleanup(store: &mut ResourceStore, snapshots: &[Snapshot]) {
    assert_no_heap(|| {
        for snapshot in snapshots {
            store.release_snapshot(snapshot.id).unwrap();
            store.remove(snapshot.resource).unwrap();
        }
        for _ in 0..272 {
            store.begin_callback();
            store.reclaim();
        }
        assert_eq!(store.usage().free_pages, 256);
        assert_eq!(store.usage().free_versions, 8);
        assert_eq!(store.usage().free_snapshots, 8);
        assert_eq!(store.midi_index_usage().free_versions, 4);
    });
}
fn collect(output: &MidiBuffer, at: u64, target: &mut Vec<(u64, Vec<u8>)>) {
    for i in 0..output.len() {
        let (offset, packet) = output.get(i).unwrap();
        target.push((at + u64::from(offset), packet.to_vec()));
    }
}
fn default_restore(initial_note: Option<(u8, u8)>) -> Vec<(u64, Vec<u8>)> {
    let mut result = Vec::new();
    for channel in 0..16 {
        for packet in [
            [0xe0 | channel, 0, 64],
            [0xb0 | channel, 64, 0],
            [0xb0 | channel, 69, 0],
            [0xb0 | channel, 66, 0],
        ] {
            result.push((0, packet.to_vec()));
        }
        if channel == 0 {
            if let Some((key, velocity)) = initial_note {
                result.push((0, vec![0x90, key, velocity]));
            }
        }
    }
    result
}
#[shoop_test]
fn indexed_packets_sysex_equal_times_and_trailing_silence_match_absolute_oracle() {
    let mut sysex = vec![0xf0];
    sysex.extend((0..255).map(|n| (n % 128) as u8));
    sysex.push(0xf7);
    let events = vec![
        (0, vec![0x90, 60, 80]),
        (0, vec![0xa0, 60, 40]),
        (2, sysex),
        (4, vec![0x80, 60, 7]),
        (5, vec![0x90, 60, 0]),
        (8, vec![0xb0, 0, 2]),
        (9, vec![0xc0, 3]),
        (10, vec![0x90, 62, 90]),
        (12, vec![0x80, 62, 0]),
    ];
    let mut expected = default_restore(Some((55, 31)));
    expected.extend(events.clone());
    expected.push((13, vec![0x80, 55, 0]));
    for partitions in [vec![13], vec![1; 13], vec![3, 2, 4, 4]] {
        let mut store = store();
        let mut initial = MidiState::default();
        initial.apply_packet(&[0x90, 55, 31], 3).unwrap();
        let snapshot = upload(&mut store, &events, 13, &initial);
        let index = store.read_index(snapshot.id).unwrap();
        let mut renderer = MidiRenderer::prepare(64, 2048).unwrap();
        let mut output = MidiBuffer::new(512, 2048).unwrap();
        let mut actual = Vec::new();
        let mut at = 0;
        for frames in partitions {
            let report = assert_no_heap(|| {
                output.clear();
                renderer.render(
                    &mut store.render_io(),
                    Playback {
                        source: Some(index),
                        position: at,
                        discontinuity: at == 0,
                    },
                    frames,
                    &mut output,
                )
            })
            .unwrap();
            assert_eq!(report.output_error, None);
            collect(&output, at, &mut actual);
            at += frames as u64;
        }
        assert_no_heap(|| {
            output.clear();
            let report = renderer
                .render(
                    &mut store.render_io(),
                    Playback {
                        source: None,
                        position: 0,
                        discontinuity: false,
                    },
                    1,
                    &mut output,
                )
                .unwrap();
            assert_eq!(report.output_error, None);
        });
        collect(&output, 13, &mut actual);
        assert_eq!(actual, expected);
        assert_no_heap(|| {
            output.clear();
            assert_eq!(
                renderer
                    .render(
                        &mut store.render_io(),
                        Playback {
                            source: None,
                            position: 0,
                            discontinuity: false
                        },
                        1,
                        &mut output
                    )
                    .unwrap()
                    .attempted,
                0
            );
        });
        cleanup(&mut store, &[snapshot]);
    }
}
#[shoop_test]
fn seek_wrap_and_replacement_cleanup_precede_restoration_and_equal_time_packets() {
    let mut store = store();
    let first = upload(
        &mut store,
        &[
            (0, vec![0x90, 60, 70]),
            (2, vec![0x90, 61, 80]),
            (4, vec![0x80, 60, 0]),
            (4, vec![0x90, 62, 90]),
        ],
        6,
        &MidiState::default(),
    );
    let mut initial = MidiState::default();
    initial.apply_packet(&[0x90, 63, 97], 3).unwrap();
    let second = upload(&mut store, &[(0, vec![0x80, 63, 3])], 2, &initial);
    let a = store.read_index(first.id).unwrap();
    let b = store.read_index(second.id).unwrap();
    let mut renderer = MidiRenderer::prepare(64, 2048).unwrap();
    let mut output = MidiBuffer::new(512, 2048).unwrap();
    let mut actual = Vec::new();
    for (at, index, position, frames) in [(0, a, 0, 2), (2, a, 4, 1), (3, a, 0, 1), (4, b, 0, 1)] {
        assert_no_heap(|| {
            output.clear();
            let report = renderer
                .render(
                    &mut store.render_io(),
                    Playback {
                        source: Some(index),
                        position,
                        discontinuity: true,
                    },
                    frames,
                    &mut output,
                )
                .unwrap();
            assert_eq!(report.output_error, None);
        });
        collect(&output, at, &mut actual);
    }
    actual.retain(|(_, p)| matches!(p[0], 0x80 | 0x90));
    let expected = vec![
        (0, vec![0x90, 60, 70]),
        (2, vec![0x80, 60, 0]),
        (2, vec![0x90, 60, 70]),
        (2, vec![0x90, 61, 80]),
        (2, vec![0x80, 60, 0]),
        (2, vec![0x90, 62, 90]),
        (3, vec![0x80, 61, 0]),
        (3, vec![0x80, 62, 0]),
        (3, vec![0x90, 60, 70]),
        (4, vec![0x80, 60, 0]),
        (4, vec![0x90, 63, 97]),
        (4, vec![0x80, 63, 3]),
    ];
    assert_eq!(actual, expected);
    cleanup(&mut store, &[first, second]);
}
#[shoop_test]
fn stopped_input_tracking_supplies_capture_initial_state_and_exact_frozen_packets() {
    let mut store = store();
    let mut capture = MidiCapture::prepare(4, 2, 32).unwrap();
    let mut input = MidiBuffer::new(4, 64).unwrap();
    input.push(0, 2, &[0x90, 60, 99]).unwrap();
    input.push(1, 2, &[0xb0, 64, 127]).unwrap();
    assert_no_heap(|| capture.process(&mut store.render_io(), &input, 2, None)).unwrap();
    let initial = *capture.state();
    let resource = store.create().unwrap();
    let group = store
        .bind_capture_group(&[CaptureMember {
            resource,
            format: Format::Midi,
            midi_events: 2,
            midi_bytes: 32,
        }])
        .unwrap();
    let mut frozen = [None];
    assert_no_heap(|| {
        let take = store.prepare_capture_take(group, &[initial]).unwrap();
        store.activate_capture_take(take, Frame(2)).unwrap();
        let interval = store.reserve_capture_interval(take, Frame(2), 2).unwrap();
        input.clear();
        input.push(0, 2, &[0x80, 60, 0]).unwrap();
        input.push(1, 2, &[0xf0, 1, 2, 3, 0xf7]).unwrap();
        capture
            .process(&mut store.render_io(), &input, 2, Some((interval, 0)))
            .unwrap();
        assert!(!capture.state().channel(0).unwrap().note(60).unwrap().held());
        assert_eq!(
            capture
                .state()
                .channel(0)
                .unwrap()
                .note(60)
                .unwrap()
                .velocity(),
            99
        );
        assert_eq!(store.info(resource).unwrap().description.frames, 0);
        store.commit_capture_interval(interval).unwrap();
        store
            .freeze_capture_take(take, Frame(4), None, &mut frozen)
            .unwrap();
        let index = store.read_index(frozen[0].unwrap().id).unwrap();
        assert_eq!(
            store.render_io().midi_seek(index, 0).unwrap().state,
            initial
        );
        assert!(!store
            .render_io()
            .midi_seek(index, 1)
            .unwrap()
            .state
            .channel(0)
            .unwrap()
            .note(60)
            .unwrap()
            .held());
        let mut cursor = store.render_io().midi_cursor(index).unwrap();
        let mut packet = [0; 32];
        for event_index in 0..2 {
            let event = store.render_io().midi_next(&mut cursor).unwrap().unwrap();
            let (offset, expected) = input.get(event_index).unwrap();
            assert_eq!(event.frame(), u64::from(offset));
            let count = store.render_io().midi_read(event, &mut packet).unwrap();
            assert_eq!(&packet[..count], expected);
        }
        assert_eq!(store.render_io().midi_next(&mut cursor), Ok(None));
        store.release_capture_group(group).unwrap();
    });
    cleanup(&mut store, &[frozen[0].unwrap()]);
}
#[shoop_test]
fn shape_scope_and_continuity_refusals_preserve_valid_state_and_zero_frames_do_nothing() {
    let mut store = store();
    let snapshot = upload(
        &mut store,
        &[(0, vec![0x90, 60, 90])],
        4,
        &MidiState::default(),
    );
    let index = store.read_index(snapshot.id).unwrap();
    let mut renderer = MidiRenderer::prepare(4, 2048).unwrap();
    let mut output = MidiBuffer::new(256, 2048).unwrap();
    let mut capture = MidiCapture::prepare(4, 2, 16).unwrap();
    let mut input = MidiBuffer::new(4, 32).unwrap();
    assert_no_heap(|| {
        renderer
            .render(
                &mut store.render_io(),
                Playback {
                    source: Some(index),
                    position: 0,
                    discontinuity: false,
                },
                1,
                &mut output,
            )
            .unwrap();
        output.clear();
        let before = *renderer.state();
        assert_eq!(
            renderer.render(
                &mut store.render_io(),
                Playback {
                    source: Some(index),
                    position: 2,
                    discontinuity: false
                },
                1,
                &mut output
            ),
            Err(Error::InvalidFrame)
        );
        assert_eq!(*renderer.state(), before);
        assert!(output.is_empty());
        assert_eq!(
            renderer.render(
                &mut store.render_io(),
                Playback {
                    source: Some(index),
                    position: u64::MAX,
                    discontinuity: true
                },
                1,
                &mut output
            ),
            Err(Error::InvalidFrame)
        );
        renderer
            .render(
                &mut store.render_io(),
                Playback {
                    source: None,
                    position: 0,
                    discontinuity: true,
                },
                0,
                &mut output,
            )
            .unwrap();
        assert_eq!(*renderer.state(), before);
        renderer
            .render(
                &mut store.render_io(),
                Playback {
                    source: Some(index),
                    position: 1,
                    discontinuity: false,
                },
                1,
                &mut output,
            )
            .unwrap();
        assert!(output.is_empty());
        for key in 0..3 {
            input.push(0, 1, &[0x90, key, 64]).unwrap();
        }
        let before = *capture.state();
        assert_eq!(
            capture.process(&mut store.render_io(), &input, 1, None),
            Err(Error::Capacity)
        );
        assert_eq!(*capture.state(), before);
        capture
            .process(&mut store.render_io(), &input, 0, None)
            .unwrap();
        assert_eq!(*capture.state(), before);
        input.clear();
        input.push(3, 4, &[0x90, 61, 65]).unwrap();
        assert_eq!(
            capture.process(&mut store.render_io(), &input, 2, None),
            Err(Error::InvalidFrame)
        );
        assert_eq!(*capture.state(), before);
        store.release_snapshot(snapshot.id).unwrap();
        assert_eq!(
            renderer.render(
                &mut store.render_io(),
                Playback {
                    source: Some(index),
                    position: 2,
                    discontinuity: false
                },
                1,
                &mut output
            ),
            Err(Error::InvalidId)
        );
        store.remove(snapshot.resource).unwrap();
        for _ in 0..272 {
            store.begin_callback();
            store.reclaim();
        }
        assert_eq!(store.usage().free_pages, 256);
    });
    for (frames, bytes) in [(0, 3), (1, 2), (1, usize::MAX)] {
        assert!(matches!(
            MidiRenderer::prepare(frames, bytes),
            Err(Error::InvalidConfig)
        ));
        assert!(matches!(
            MidiCapture::prepare(frames, 2, bytes),
            Err(Error::InvalidConfig)
        ));
    }
}
struct RuntimeNode {
    renderer: Option<(MidiRenderer, ReadIndex)>,
    tracker: MidiCapture,
    observed: Arc<AtomicUsize>,
}
impl Processor for RuntimeNode {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: usize::from(self.renderer.is_none()),
            audio_outputs: 1,
            midi_inputs: usize::from(self.renderer.is_none()),
            midi_outputs: usize::from(self.renderer.is_some()),
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        if let Some((renderer, index)) = &mut self.renderer {
            block.audio.output(0).unwrap().fill(0.25);
            let report = renderer.render(
                &mut block.resources.render(),
                Playback {
                    source: Some(*index),
                    position: block.frame.0,
                    discontinuity: block.frame == Frame(0),
                },
                block.frames,
                &mut block.midi_outputs[0],
            )?;
            assert_eq!(
                renderer
                    .state()
                    .channel(0)
                    .unwrap()
                    .note(60)
                    .unwrap()
                    .velocity(),
                99
            );
            assert_eq!(
                renderer
                    .state()
                    .channel(0)
                    .unwrap()
                    .note(61)
                    .unwrap()
                    .velocity(),
                88
            );
            assert_eq!(report.output_error, Some(Error::Full));
            self.observed.fetch_add(1, Ordering::Relaxed);
            report.output_error.map_or(Ok(()), Err)
        } else {
            let (input, output) = block.audio.pair(0, 0).unwrap();
            output.copy_from_slice(input);
            assert!(block.midi_inputs[0].is_recovery());
            assert_eq!(block.midi_inputs[0].len(), 32);
            self.tracker.process(
                &mut block.resources.render(),
                &block.midi_inputs[0],
                block.frames,
                None,
            )?;
            for channel in 0..16 {
                for key in 0..128 {
                    assert_eq!(
                        self.tracker
                            .state()
                            .channel(channel)
                            .unwrap()
                            .note(key)
                            .unwrap()
                            .velocity(),
                        0
                    );
                }
            }
            self.observed.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }
}
#[shoop_test]
fn runtime_overflow_resets_downstream_and_advances_media_without_interrupting_audio() {
    let cfg = Config {
        max_frames: 2,
        max_nodes: 2,
        max_ports: 8,
        audio_inputs: 0,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 1,
        midi_events_per_port: 2,
        midi_bytes_per_port: 2048,
        resource_slots: 8,
        resource_pages: 256,
        page_bytes: 16,
        budgets: Budgets {
            transfer_bytes: 16,
            validation_steps: 4096,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut store = store_with(cfg);
    let snapshot = upload(
        &mut store,
        &[(0, vec![0x90, 60, 99]), (1, vec![0x90, 61, 88])],
        2,
        &MidiState::default(),
    );
    let index = store.read_index(snapshot.id).unwrap();
    let observed = Arc::new(AtomicUsize::new(0));
    let mut tracker = MidiCapture::prepare(2, 1, 3).unwrap();
    let mut held = MidiBuffer::new(1, 3).unwrap();
    held.push(0, 1, &[0x90, 60, 64]).unwrap();
    assert_no_heap(|| tracker.process(&mut store.render_io(), &held, 1, None)).unwrap();
    assert_eq!(
        tracker
            .state()
            .channel(0)
            .unwrap()
            .note(60)
            .unwrap()
            .velocity(),
        64
    );
    let a = NodeId::from_parts(0, 1).unwrap();
    let b = NodeId::from_parts(1, 1).unwrap();
    let nodes = vec![
        PreparedNode::new(
            a,
            RuntimeNode {
                renderer: Some((MidiRenderer::prepare(2, 2048).unwrap(), index)),
                tracker: MidiCapture::prepare(2, 2, 2048).unwrap(),
                observed: observed.clone(),
            },
        ),
        PreparedNode::new(
            b,
            RuntimeNode {
                renderer: None,
                tracker,
                observed: observed.clone(),
            },
        ),
    ];
    let (mut control, mut runtime) =
        GraphRuntime::prepare_with_resources(cfg, nodes, store).unwrap();
    let edge = |from, to, kind| Connection {
        source: Endpoint {
            node: from,
            kind,
            port: 0,
        },
        destination: Endpoint {
            node: to,
            kind,
            port: 0,
        },
    };
    let plan = graph::compile(
        runtime.catalog(),
        &GraphDescription {
            nodes: vec![a, b],
            connections: vec![
                edge(Some(a), Some(b), PortType::Audio),
                edge(Some(a), Some(b), PortType::Midi),
                edge(Some(b), None, PortType::Audio),
                edge(Some(a), None, PortType::Midi),
            ],
        },
    )
    .unwrap();
    control
        .submit(Lane::Normal, Command::commit(runtime.epoch(), plan))
        .unwrap_or_else(|e| panic!("{:?}", e.error));
    let mut audio = [0.; 2];
    let mut midi = [MidiBuffer::new(2, 2048).unwrap()];
    let mut reset = MidiBuffer::new(32, 96).unwrap();
    assert_no_heap(|| {
        runtime
            .process(
                AudioBlock {
                    frame: Frame(0),
                    frames: 2,
                    inputs: &[],
                    outputs: &mut [&mut audio],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut midi,
                },
            )
            .unwrap();
        assert_eq!(audio, [0.25; 2]);
        assert_eq!(runtime.next_frame(), Frame(2));
        assert_eq!(observed.load(Ordering::Relaxed), 2);
        assert!(runtime.diagnostics().midi_overflows > 0);
        assert_eq!(runtime.drain_midi_recovery(0, &mut reset, 2), Ok(32));
        for i in 0..32 {
            assert_eq!(
                reset.get(i).unwrap(),
                (
                    0,
                    &[0xb0 | (i / 2) as u8, if i % 2 == 0 { 120 } else { 123 }, 0][..]
                )
            );
        }
    });
    drop(control);
    let mut store = runtime.into_resources();
    cleanup(&mut store, &[snapshot]);
}
#[shoop_test]
fn one_sample_midi_wraps_are_lossless_across_host_partitions() {
    use shoop_loop_engine::{
        compile, Limits, LoopId, LoopKind, LoopSpec, Request, Resolver, Sequence, Timing,
        TopologyDescription, Transition,
    };
    let id = LoopId::from_parts(0, 1).unwrap();
    let mut expected = Vec::new();
    for frame in 0..128 {
        if frame != 0 {
            expected.push((frame, vec![0x80, 60, 0]));
        }
        expected.extend(
            default_restore(None)
                .into_iter()
                .map(|(_, packet)| (frame, packet)),
        );
        expected.push((frame, vec![0x90, 60, 77]));
    }
    for partitions in [vec![64, 64], vec![1; 128], vec![7, 53, 19, 49]] {
        let mut store = store();
        let snapshot = upload(
            &mut store,
            &[(0, vec![0x90, 60, 77])],
            1,
            &MidiState::default(),
        );
        let index = store.read_index(snapshot.id).unwrap();
        let topology = compile(
            Limits {
                journal_events: 0,
                ..Limits::default()
            },
            &TopologyDescription {
                loops: vec![LoopSpec {
                    id,
                    kind: LoopKind::Primitive,
                    length: 1,
                    sync_source: None,
                }],
                triggers: vec![],
                programs: vec![],
            },
        )
        .unwrap();
        let mut resolver = Resolver::prepare(topology).unwrap();
        let mut renderer = MidiRenderer::prepare(64, 2048).unwrap();
        let mut output = MidiBuffer::new(128, 512).unwrap();
        let mut actual = Vec::new();
        assert_no_heap(|| {
            resolver.submit(Request {
                target: id,
                sequence: Sequence(1),
                at: Frame(0),
                timing: Timing::At,
                transition: Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            })
        })
        .unwrap();
        let mut end = 0;
        for frames in partitions {
            end += frames;
            loop {
                let interval = assert_no_heap(|| {
                    output.clear();
                    let interval = resolver.preview(Frame(end), &[]).unwrap();
                    let decision = resolver.decisions().unwrap()[0];
                    resolver.commit().unwrap();
                    let report = renderer
                        .render(
                            &mut store.render_io(),
                            Playback {
                                source: Some(index),
                                position: decision.state.position,
                                discontinuity: decision.playback_discontinuity,
                            },
                            (interval.end.0 - interval.start.0) as usize,
                            &mut output,
                        )
                        .unwrap();
                    assert_eq!(report.output_error, None);
                    resolver.finish(interval.end).unwrap();
                    interval
                });
                collect(&output, interval.start.0, &mut actual);
                if interval.end.0 == end {
                    break;
                }
            }
        }
        assert_eq!(actual, expected);
        assert_eq!(resolver.state(id).unwrap().cycles, 127);
        cleanup(&mut store, &[snapshot]);
    }
}
