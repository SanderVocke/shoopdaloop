use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    fixed::{self, Quantum, Runtime},
    graph::{Connection, Endpoint, GraphDescription, PortType},
    resource::{Description, Format, ResourceStore},
    Error, Frame, NodeId, Revision,
};
use shoop_loops::{
    fixed::{Bank, Binding, Command, Node, Reply},
    media, LoopId, Mode,
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
fn config(q: usize, pages: usize, midi: bool) -> fixed::PreparedConfig {
    fixed::Config {
        quantum: Quantum::new(q).unwrap(),
        max_host_frames: 512,
        capacities: rt_runtime::Config {
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: usize::from(midi),
            midi_outputs: usize::from(midi),
            resource_pages: pages,
            page_bytes: 256,
            resource_slots: 64,
            midi_events_per_port: 512,
            midi_bytes_per_port: 4096,
            ..Default::default()
        },
    }
    .prepare()
    .unwrap()
}
fn binding(slot: u32, midi: bool) -> Binding {
    Binding {
        target: id(slot),
        playback: node(slot * 2),
        capture: node(slot * 2 + 1),
        channels: 2,
        midi,
        gain: 1.,
        smoothing: shoop_loops::audio::Smoothing::None,
    }
}
fn graph(count: u32, midi: bool) -> GraphDescription {
    let mut g = GraphDescription {
        nodes: vec![],
        connections: vec![],
    };
    for slot in 0..count {
        g.nodes.extend([node(slot * 2), node(slot * 2 + 1)]);
        for channel in 0..2 {
            g.connections.push(Connection {
                source: Endpoint::host(PortType::Audio, channel),
                destination: Endpoint::node(node(slot * 2 + 1), PortType::Audio, channel),
            });
            g.connections.push(Connection {
                source: Endpoint::node(node(slot * 2), PortType::Audio, channel),
                destination: Endpoint::host(PortType::Audio, channel),
            });
        }
        if midi {
            for (source, destination) in [
                (
                    Endpoint::host(PortType::Midi, 0),
                    Endpoint::node(node(slot * 2 + 1), PortType::Midi, 0),
                ),
                (
                    Endpoint::node(node(slot * 2), PortType::Midi, 0),
                    Endpoint::host(PortType::Midi, 0),
                ),
            ] {
                g.connections.push(Connection {
                    source,
                    destination,
                });
            }
        }
    }
    g
}
fn source(
    store: &mut ResourceStore,
    target: LoopId,
    frames: usize,
) -> (media::Input, rt_runtime::ResourceId) {
    let bytes: Vec<_> = (0..frames)
        .flat_map(|i| {
            [i as f32 + 1., -(i as f32 + 1.)]
                .into_iter()
                .flat_map(f32::to_le_bytes)
        })
        .collect();
    let resource = store.create().unwrap();
    let u = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Audio { channels: 2 },
                frames: frames as u64,
                bytes: bytes.len(),
            },
        )
        .unwrap();
    for (i, data) in bytes.chunks(256).enumerate() {
        store.begin_callback();
        store.write_upload(u, i * 256, data).unwrap();
    }
    store.finish_upload(u).unwrap();
    store.commit(u).unwrap();
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    let index = store.read_index(snapshot.id).unwrap();
    (
        media::Input {
            description: media::Description {
                owner: target,
                revision: Revision(0),
                frames: frames as u64,
                channels: 2,
                midi: false,
            },
            audio: Some(media::Source { snapshot, index }),
            midi: None,
        },
        resource,
    )
}
fn render(
    r: &mut Runtime<Node, Bank>,
    input: &[Vec<f32>],
    output: &mut [Vec<f32>],
    mi: &[MidiBuffer],
    mo: &mut [MidiBuffer],
    frames: usize,
) -> Result<(), Error> {
    let [left, right] = output else {
        panic!("stereo")
    };
    assert_no_heap(|| {
        r.process(
            AudioBlock {
                frame: r.next_frame(),
                frames,
                inputs: &[&input[0], &input[1]],
                outputs: &mut [&mut left[..], &mut right[..]],
            },
            MidiBlock {
                inputs: mi,
                outputs: mo,
            },
        )
    })
}

#[shoop_test]
fn tick_playback_seek_gain_and_receipts_match_an_independent_sample_model() {
    for q in [16, 32, 64, 128] {
        let c = config(q, 256, false);
        let mut store = Bank::resource_store(c, 2).unwrap();
        let (a, ra) = source(&mut store, id(0), q * 4);
        let (b, rb) = source(&mut store, id(1), q * 4);
        let prepared = Bank::prepare(
            c,
            store,
            vec![binding(0, false), binding(1, false)],
            vec![a, b],
        )
        .unwrap();
        let (mut control, mut runtime) = prepared.runtime(c, &graph(2, false)).unwrap();
        control
            .submit(
                Frame(0),
                Command::Play {
                    target: id(0),
                    position: 0,
                },
            )
            .unwrap();
        control
            .submit(
                Frame(0),
                Command::Play {
                    target: id(1),
                    position: 0,
                },
            )
            .unwrap();
        control
            .submit(
                Frame(q as u64 + 1),
                Command::Seek {
                    target: id(0),
                    position: q as u64,
                },
            )
            .unwrap();
        control
            .submit(
                Frame(q as u64 * 2),
                Command::Gain {
                    target: id(1),
                    value: 0.5,
                },
            )
            .unwrap();
        let frames = q * 4;
        let input = vec![vec![0.; frames]; 2];
        let mut output = vec![vec![0.; frames]; 2];
        render(&mut runtime, &input, &mut output, &[], &mut [], frames).unwrap();
        let mut positions = [0usize; 2];
        for frame in 0..frames {
            if frame == q * 2 {
                positions[0] = q;
            }
            let value = (positions[0] + 1) as f32
                + (positions[1] + 1) as f32 * if frame >= q * 2 { 0.5 } else { 1. };
            assert_eq!(output[0][frame], value);
            assert_eq!(output[1][frame], -value);
            for position in &mut positions {
                *position = (*position + 1) % (q * 4);
            }
        }
        let mut seen = 0;
        while let Some(receipt) = control.try_receipt() {
            assert_eq!(receipt.result, Ok(Reply::Done));
            if matches!(receipt.command, Some(Command::Seek { .. })) {
                assert_eq!(receipt.requested, Frame(q as u64 + 1));
                assert_eq!(receipt.actual, Frame(q as u64 * 2));
            }
            seen += 1;
        }
        assert_eq!(seen, 4);
        assert_eq!(
            runtime.state().status(id(0)).unwrap().position,
            (q * 3) as u64
        );
        let at = runtime.next_frame();
        let (mut bank, mut store) = runtime.into_parts();
        bank.cleanup(&mut store, at).unwrap();
        bank.cleanup(&mut store, at).unwrap();
        store.remove(ra).unwrap();
        store.remove(rb).unwrap();
        for _ in 0..256 {
            store.begin_callback();
            store.reclaim();
        }
        assert_eq!(store.usage().free_pages, 256);
    }
}

#[shoop_test]
fn coordinated_audio_and_midi_recording_publishes_whole_ticks_without_an_observation_phase() {
    for q in [16, 32, 64, 128] {
        let c = config(q, 256, true);
        let store = Bank::resource_store(c, 1).unwrap();
        let prepared = Bank::prepare(c, store, vec![binding(0, true)], vec![]).unwrap();
        let (mut control, mut runtime) = prepared.runtime(c, &graph(1, true)).unwrap();
        control
            .submit(
                Frame(0),
                Command::Record {
                    target: id(0),
                    monitor_previous: false,
                },
            )
            .unwrap();
        control
            .submit(
                Frame(q as u64),
                Command::Play {
                    target: id(0),
                    position: 0,
                },
            )
            .unwrap();
        let frames = q * 2;
        let input = vec![
            (0..frames).map(|i| i as f32 + 1.).collect(),
            (0..frames).map(|i| -(i as f32 + 1.)).collect(),
        ];
        let mut output = vec![vec![0.; frames]; 2];
        let mut mi = vec![MidiBuffer::new(512, 4096).unwrap()];
        let mut mo = vec![MidiBuffer::new(512, 4096).unwrap()];
        mi[0].push(5, frames, &[0x90, 60, 100]).unwrap();
        render(&mut runtime, &input, &mut output, &mi, &mut mo, frames).unwrap();
        for ch in 0..2 {
            assert_eq!(&output[ch][..q], &vec![0.; q]);
            assert_eq!(&output[ch][q..], &input[ch][..q]);
        }
        let note_ons: Vec<_> = (0..mo[0].len())
            .filter_map(|i| {
                let (at, data) = mo[0].get(i).unwrap();
                (data == [0x90, 60, 100]).then_some(at)
            })
            .collect();
        assert_eq!(note_ons, [q as u32 + 5]);
        assert_eq!(runtime.state().status(id(0)).unwrap().length, q as u64);
        assert_eq!(runtime.state().status(id(0)).unwrap().mode, Mode::Playing);
        assert_eq!(
            runtime.state().status(id(0)).unwrap().content_revision,
            Revision(1)
        );
        while let Some(receipt) = control.try_receipt() {
            assert_eq!(receipt.result, Ok(Reply::Done));
        }
        let at = runtime.next_frame();
        let (mut bank, mut store) = runtime.into_parts();
        bank.cleanup(&mut store, at).unwrap();
        for _ in 0..256 {
            store.begin_callback();
            store.reclaim();
        }
        assert_eq!(store.usage().free_pages, 256);
        assert_eq!(store.capture_group_usage().free_groups, 2);
    }
}

#[shoop_test]
fn exhausted_capture_stops_at_the_last_complete_tick_and_failed_restart_preserves_playback() {
    let q = 32;
    let c = config(q, 2, false);
    let store = Bank::resource_store(c, 1).unwrap();
    let prepared = Bank::prepare(c, store, vec![binding(0, false)], vec![]).unwrap();
    let (mut control, mut runtime) = prepared.runtime(c, &graph(1, false)).unwrap();
    control
        .submit(
            Frame(0),
            Command::Record {
                target: id(0),
                monitor_previous: false,
            },
        )
        .unwrap();
    let input = vec![(0..128).map(|i| i as f32 + 1.).collect(), vec![0.; 128]];
    let mut output = vec![vec![0.; 128]; 2];
    render(&mut runtime, &input, &mut output, &[], &mut [], 128).unwrap();
    let status = runtime.state().status(id(0)).unwrap();
    assert_eq!(status.failure, Some(Error::Capacity));
    assert_eq!(status.mode, Mode::Stopped);
    assert_eq!(status.length, 64);
    control.try_receipt().unwrap();
    control
        .submit(
            Frame(128),
            Command::Play {
                target: id(0),
                position: 0,
            },
        )
        .unwrap();
    render(&mut runtime, &input, &mut output, &[], &mut [], 32).unwrap();
    assert_eq!(&output[0][..32], &input[0][..32]);
    control.try_receipt().unwrap();
    control
        .submit(
            Frame(160),
            Command::Record {
                target: id(0),
                monitor_previous: false,
            },
        )
        .unwrap();
    render(&mut runtime, &input, &mut output, &[], &mut [], 32).unwrap();
    assert_eq!(control.try_receipt().unwrap().result, Err(Error::Capacity));
    assert_eq!(runtime.state().status(id(0)).unwrap().mode, Mode::Playing);
    assert_eq!(&output[0][..32], &input[0][32..64]);
    let at = runtime.next_frame();
    let (mut bank, mut store) = runtime.into_parts();
    bank.cleanup(&mut store, at).unwrap();
    for _ in 0..64 {
        store.begin_callback();
        store.reclaim();
    }
    assert_eq!(store.usage().free_pages, 2);
}

#[shoop_test]
fn unaligned_imports_and_missing_consumers_are_rejected_before_rendering() {
    let c = config(32, 256, false);
    let mut store = Bank::resource_store(c, 1).unwrap();
    let (input, _) = source(&mut store, id(0), 95);
    assert!(matches!(
        Bank::prepare(c, store, vec![binding(0, false)], vec![input]),
        Err(Error::InvalidFrame)
    ));
    let store = Bank::resource_store(c, 1).unwrap();
    let p = Bank::prepare(c, store, vec![binding(0, false)], vec![]).unwrap();
    let mut g = graph(1, false);
    g.nodes.pop();
    assert!(matches!(p.runtime(c, &g), Err(Error::InvalidGraph)));
}

#[shoop_test]
fn repeated_recording_reuses_banks_without_leaks_and_keeps_content_and_capture_revisions_distinct()
{
    let c = config(32, 8, false);
    let store = Bank::resource_store(c, 1).unwrap();
    let prepared = Bank::prepare(c, store, vec![binding(0, false)], vec![]).unwrap();
    let (mut control, mut runtime) = prepared.runtime(c, &graph(1, false)).unwrap();
    let input = vec![vec![1.; 96], vec![-1.; 96]];
    let mut output = vec![vec![0.; 96]; 2];
    for iteration in 0..32u64 {
        let at = runtime.next_frame();
        control
            .submit(
                at,
                Command::Record {
                    target: id(0),
                    monitor_previous: false,
                },
            )
            .unwrap();
        control
            .submit(at, Command::State { target: id(0) })
            .unwrap();
        control
            .submit(
                Frame(at.0 + 32),
                Command::Play {
                    target: id(0),
                    position: 0,
                },
            )
            .unwrap();
        control
            .submit(Frame(at.0 + 64), Command::Stop { target: id(0) })
            .unwrap();
        render(&mut runtime, &input, &mut output, &[], &mut [], 96).unwrap();
        while let Some(receipt) = control.try_receipt() {
            match receipt.result.unwrap() {
                Reply::Done => {}
                Reply::State(status) => {
                    assert_eq!(status.content_revision, Revision(iteration));
                    assert_eq!(status.capture_revision, Some(Revision(iteration + 1)));
                }
            }
        }
        for ch in 0..2 {
            assert_eq!(&output[ch][..32], &[0.; 32]);
            assert_eq!(&output[ch][32..64], &input[ch][..32]);
            assert_eq!(&output[ch][64..], &[0.; 32]);
        }
        let status = runtime.state().status(id(0)).unwrap();
        assert_eq!(status.failure, None);
        assert_eq!(status.content_revision, Revision(iteration + 1));
        assert_eq!(status.capture_revision, None);
    }
    let at = runtime.next_frame();
    let (mut bank, mut store) = runtime.into_parts();
    bank.cleanup(&mut store, at).unwrap();
    for _ in 0..64 {
        store.begin_callback();
        store.reclaim();
    }
    assert_eq!(store.usage().free_pages, 8);
}

#[shoop_test]
fn monitoring_previous_content_keeps_old_sources_until_the_new_take_is_published() {
    let c = config(32, 256, false);
    let mut store = Bank::resource_store(c, 1).unwrap();
    let (source, resource) = source(&mut store, id(0), 128);
    let p = Bank::prepare(c, store, vec![binding(0, false)], vec![source]).unwrap();
    let (mut control, mut runtime) = p.runtime(c, &graph(1, false)).unwrap();
    control
        .submit(
            Frame(0),
            Command::Record {
                target: id(0),
                monitor_previous: true,
            },
        )
        .unwrap();
    control
        .submit(
            Frame(64),
            Command::Play {
                target: id(0),
                position: 0,
            },
        )
        .unwrap();
    let input = vec![vec![7.; 128], vec![-7.; 128]];
    let mut output = vec![vec![0.; 128]; 2];
    render(&mut runtime, &input, &mut output, &[], &mut [], 128).unwrap();
    for i in 0..64 {
        assert_eq!(output[0][i], i as f32 + 1.);
        assert_eq!(output[1][i], -(i as f32 + 1.));
    }
    assert_eq!(&output[0][64..], &[7.; 64]);
    assert_eq!(&output[1][64..], &[-7.; 64]);
    assert_eq!(runtime.state().status(id(0)).unwrap().length, 64);
    while let Some(receipt) = control.try_receipt() {
        assert_eq!(receipt.result, Ok(Reply::Done));
    }
    let at = runtime.next_frame();
    let (mut bank, mut store) = runtime.into_parts();
    bank.cleanup(&mut store, at).unwrap();
    store.remove(resource).unwrap();
    for _ in 0..256 {
        store.begin_callback();
        store.reclaim();
    }
    assert_eq!(store.usage().free_pages, 256);
}
