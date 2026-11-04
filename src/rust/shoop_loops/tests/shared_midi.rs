#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use rt_runtime::{
    buffer::MidiBuffer,
    midi_state::MidiState,
    resource::{self, ResourceStore, Snapshot},
    Budgets, Config, Error,
};
use shoop_loops::{
    audio::{AudioRenderer, Smoothing},
    midi::{MidiCapture, MidiRenderer, Playback},
    midi_restore, *,
};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

type Events = Vec<(u64, Vec<u8>)>;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Observed {
    frame: u64,
    voice: usize,
    len: usize,
    packet: [u8; 48],
}
fn emit(target: &mut Vec<Observed>, frame: u64, voice: usize, data: &[u8]) -> Result<(), Error> {
    let mut packet = [0; 48];
    packet[..data.len()].copy_from_slice(data);
    target.push(Observed {
        frame,
        voice,
        len: data.len(),
        packet,
    });
    Ok(())
}
fn input() -> Events {
    let mut sysex = vec![1; 35];
    sysex[0] = 0xf0;
    sysex[34] = 0xf7;
    let mut events = vec![
        (0, vec![0x90, 60, 80]),
        (3, vec![0xb0, 64, 127]),
        (4, vec![0xb0, 1, 22]),
        (4, vec![0xc0, 5]),
        (5, vec![0x80, 60, 0]),
        (6, vec![0xe0, 1, 64]),
        (8, sysex),
    ];
    for channel in 0..16 {
        events.push((9, vec![0xb0 | channel, 7, 100]));
    }
    events.extend([
        (12, vec![0xb0, 121, 0]),
        (18, vec![0x90, 62, 91]),
        (18, vec![0xb0, 1, 70]),
        (19, vec![0x80, 62, 0]),
        (20, vec![0xb0, 64, 127]),
        (24, vec![0xb0, 1, 99]),
        (26, vec![0xff]),
        (27, vec![0x9f, 70, 88]),
        (40, vec![0x8f, 70, 0]),
    ]);
    events
}
fn projected(events: &Events, initial: MidiState, before: u64) -> MidiState {
    let mut state = initial;
    for (_, packet) in events.iter().filter(|(at, _)| *at < before) {
        state.apply_packet(packet, 48).unwrap();
    }
    state
}
fn window(events: &Events, start: u64) -> (Events, MidiState) {
    (
        events
            .iter()
            .filter(|(at, _)| *at >= start && *at < start + 7)
            .map(|(at, p)| (*at - start, p.clone()))
            .collect(),
        projected(events, MidiState::default(), start),
    )
}
fn splice(
    events: &mut Events,
    initial: &mut MidiState,
    source: &Events,
    at: u64,
    position: u64,
    frames: u64,
) {
    let end = position + frames;
    events.retain(|(frame, _)| {
        if end > 7 {
            !(*frame >= position || *frame < end - 7)
        } else {
            !(*frame >= position && *frame < end)
        }
    });
    if position == 0 || end > 7 {
        let zero = if position == 0 { at } else { at + 7 - position };
        *initial = projected(source, MidiState::default(), zero);
    }
    for (frame, packet) in source
        .iter()
        .filter(|(frame, _)| *frame >= at && *frame < at + frames)
    {
        events.push(((position + frame - at) % 7, packet.clone()));
    }
    events.sort_by_key(|e| e.0);
}
fn oracle(source: &Events) -> (Vec<Observed>, [(Events, MidiState); 3]) {
    let (mut a, mut initial) = window(source, 4);
    let (b, b_initial) = window(source, 6);
    let old = (a.clone(), initial);
    let mut middle = old.clone();
    let mut states = [MidiState::default(); 2];
    let mut active = [false; 2];
    let mut output = Vec::new();
    for frame in 0u64..48 {
        if frame == 25 {
            splice(&mut a, &mut initial, source, 18, 5, 3);
            middle = (a.clone(), initial);
        }
        if frame == 32 {
            splice(&mut a, &mut initial, source, 24, 4, 4);
        }
        for voice in 0..2 {
            let plays = if voice == 0 {
                (13..18).contains(&frame) || (32..33).contains(&frame) || (36..46).contains(&frame)
            } else {
                (13..46).contains(&frame)
            };
            let position = if voice == 0 && frame >= 36 {
                (2 + frame - 36) % 7
            } else {
                frame.saturating_sub(13) % 7
            };
            let (events, initial) = if voice == 0 {
                (&a, initial)
            } else {
                (&b, b_initial)
            };
            if !plays {
                if active[voice] {
                    midi_restore::cleanup(&states[voice], |p| emit(&mut output, frame, voice, p));
                }
                states[voice] = MidiState::default();
                active[voice] = false;
                continue;
            }
            if !active[voice] || position == 0 {
                if active[voice] {
                    midi_restore::cleanup(&states[voice], |p| emit(&mut output, frame, voice, p));
                }
                states[voice] = projected(events, initial, position);
                midi_restore::restore(&states[voice], |p| emit(&mut output, frame, voice, p));
                active[voice] = true;
            }
            for (_, packet) in events.iter().filter(|(at, _)| *at == position) {
                emit(&mut output, frame, voice, packet).unwrap();
                states[voice].apply_packet(packet, 48).unwrap();
            }
        }
    }
    (output, [old, middle, (a, initial)])
}
fn check(store: &mut ResourceStore, snapshot: Snapshot, events: &Events, initial: &MidiState) {
    let index = store.read_index(snapshot.id).unwrap();
    let mut cursor = store.render_io().midi_cursor(index).unwrap();
    let mut packet = [0; 48];
    for (at, data) in events {
        let event = store.render_io().midi_next(&mut cursor).unwrap().unwrap();
        assert_eq!(event.frame(), *at);
        let count = store.render_io().midi_read(event, &mut packet).unwrap();
        assert_eq!(&packet[..count], data);
    }
    assert_eq!(store.render_io().midi_next(&mut cursor), Ok(None));
    for frame in 0..=7 {
        let state = store.render_io().midi_seek(index, frame).unwrap().state;
        assert_eq!(state, projected(events, *initial, frame));
    }
}

#[shoop_test]
fn resolver_capture_grabs_prepared_replacement_and_mixed_playback_match_partition_oracles() {
    let source = input();
    let (expected, versions) = oracle(&source);
    let (sibling, sibling_initial) = window(&source, 6);
    for partitions in [vec![48], vec![1; 48], vec![5, 3, 7, 2, 11, 1, 8, 11]] {
        let mut store = ResourceStore::new(
            Config {
                max_frames: 48,
                resource_pages: 4096,
                resource_slots: 24,
                page_bytes: 16,
                budgets: Budgets {
                    transfer_bytes: 16,
                    validation_steps: 1,
                    ..Budgets::default()
                },
                ..Config::default()
            }
            .prepare()
            .unwrap(),
            rt_runtime::Quantum::new(1).unwrap(),
        )
        .unwrap();
        let baseline = store.usage();
        let midi = store
            .prepare_midi_store(
                resource::shared_midi::Config {
                    history_frames: 11,
                    chunks: 8,
                    views: 16,
                    pages: 4000,
                    max_events: 64,
                    max_bytes: 512,
                    max_packet: 48,
                    transactions: 2,
                    edit_steps: 8,
                },
                Frame(0),
                &MidiState::default(),
            )
            .unwrap();
        let audio = store
            .prepare_audio_store(
                resource::shared_audio::Config {
                    channels: 1,
                    history_frames: 11,
                    views: 8,
                    pages: 64,
                },
                Frame(0),
            )
            .unwrap();
        let control = store.create().unwrap();
        let upload = store
            .begin_upload(
                control,
                Revision(0),
                resource::Description {
                    format: resource::Format::Audio { channels: 1 },
                    frames: 4,
                    bytes: 16,
                },
            )
            .unwrap();
        store.begin_callback();
        store.write_upload(upload, 0, &[0; 16]).unwrap();
        store.finish_upload(upload).unwrap();
        store.commit(upload).unwrap();
        let control_snapshot = store.snapshot(control, Frame(0)).unwrap();
        let ids = [
            LoopId::from_parts(0, 1).unwrap(),
            LoopId::from_parts(1, 1).unwrap(),
        ];
        let topology = compile(
            Limits::default(),
            &TopologyDescription {
                loops: ids
                    .iter()
                    .map(|&id| LoopSpec {
                        id,
                        kind: LoopKind::Primitive,
                        length: 7,
                        sync_source: None,
                    })
                    .collect(),
                ..TopologyDescription::default()
            },
        )
        .unwrap();
        let mut resolver =
            Resolver::prepare(topology, rt_runtime::Quantum::new(1).unwrap()).unwrap();
        for (sequence, (voice, at, transition)) in [
            (
                0,
                13,
                Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            ),
            (
                1,
                13,
                Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            ),
            (
                0,
                18,
                Transition::Replace {
                    length: 7,
                    revision: Revision(1),
                    preserve_position: true,
                },
            ),
            (
                0,
                25,
                Transition::Replace {
                    length: 7,
                    revision: Revision(2),
                    preserve_position: true,
                },
            ),
            (
                0,
                32,
                Transition::Replace {
                    length: 7,
                    revision: Revision(3),
                    preserve_position: true,
                },
            ),
            (0, 33, Transition::Stop),
            (
                0,
                36,
                Transition::Play {
                    position: 2,
                    retrigger: false,
                },
            ),
            (0, 46, Transition::Stop),
            (1, 46, Transition::Stop),
        ]
        .into_iter()
        .enumerate()
        {
            resolver
                .submit(Request {
                    target: ids[voice],
                    sequence: Sequence(sequence as u64 + 1),
                    at: Frame(at),
                    timing: Timing::At,
                    transition,
                })
                .unwrap();
        }
        let mut capture = MidiCapture::prepare(48, 64, 512).unwrap();
        let mut input = MidiBuffer::new(64, 512).unwrap();
        let mut patches = [
            MidiBuffer::new(32, 256).unwrap(),
            MidiBuffer::new(32, 256).unwrap(),
        ];
        let mut initials = [MidiState::default(); 2];
        let mut renderers = [
            MidiRenderer::prepare(48, 512).unwrap(),
            MidiRenderer::prepare(48, 512).unwrap(),
        ];
        let mut outputs = [
            MidiBuffer::new(2048, 8192).unwrap(),
            MidiBuffer::new(2048, 8192).unwrap(),
        ];
        let mut audio_renderers = [
            AudioRenderer::prepare(1, 48, Smoothing::None).unwrap(),
            AudioRenderer::prepare(1, 48, Smoothing::None).unwrap(),
        ];
        let audio_input: [f32; 48] = core::array::from_fn(|i| i as f32 + 1.);
        let mut audio_output = [[0.; 48]; 2];
        let mut observed = Vec::with_capacity(8192);
        let mut resources = [None; 4];
        let mut snapshots = [None; 6];
        let mut pending = None;
        let mut revision = Revision(1);
        assert_no_heap(|| {
            let mut host_end = 0;
            let mut now = 0;
            for &frames in &partitions {
                host_end += frames;
                store.begin_callback();
                let mut cursor = store.cursor(control_snapshot.id).unwrap();
                assert_eq!(store.read(&mut cursor, &mut [0; 16]), Ok(16));
                store.reclaim();
                while now < host_end {
                    let forced = [
                        13, 18, 21, 22, 23, 24, 25, 28, 29, 30, 31, 32, 33, 36, 46, 48,
                    ]
                    .into_iter()
                    .find(|&cut| cut > now)
                    .unwrap();
                    let interval = resolver.preview_tick(&[]).unwrap();
                    assert!(interval.end <= Frame(host_end.min(forced)));
                    let decisions = [
                        resolver.decisions().unwrap()[0],
                        resolver.decisions().unwrap()[1],
                    ];
                    resolver.commit().unwrap();
                    let start = interval.start.0;
                    let end = interval.end.0;
                    let frames = (end - start) as usize;
                    if start == 13 {
                        for (voice, first) in [4, 6].into_iter().enumerate() {
                            let m = store
                                .grab_midi(midi, Frame(first)..Frame(first + 7))
                                .unwrap();
                            let a = store
                                .grab_audio(audio, Frame(first)..Frame(first + 7))
                                .unwrap();
                            resources[voice] = Some(m);
                            resources[voice + 2] = Some(a);
                            snapshots[voice] = Some(store.snapshot(m, Frame(13)).unwrap());
                            snapshots[voice + 4] = Some(store.snapshot(a, Frame(13)).unwrap());
                            assert_eq!(
                                snapshots[voice].unwrap().cut_frame,
                                snapshots[voice + 4].unwrap().cut_frame
                            );
                        }
                    }
                    if start == 18 {
                        initials[0] = *capture.state();
                    }
                    if start == 24 {
                        initials[1] = *capture.state();
                    }
                    if start == 21 || start == 28 {
                        let which = usize::from(start == 28);
                        pending = Some(
                            store
                                .render_io()
                                .prepare_midi_replace(
                                    resources[0].unwrap(),
                                    revision,
                                    if which == 0 { 5 } else { 4 },
                                    if which == 0 { 3 } else { 4 },
                                    &patches[which],
                                    &initials[which],
                                )
                                .unwrap(),
                        );
                    }
                    if let Some(edit) = pending {
                        if store.midi_edit_status(edit)
                            == Ok(resource::shared_midi::EditStatus::Building)
                        {
                            store.render_io().progress_midi_edit(edit, 8).unwrap();
                        }
                    }
                    if start == 25 || start == 32 {
                        revision = store
                            .render_io()
                            .commit_midi_edit(pending.take().unwrap())
                            .unwrap();
                        snapshots[if start == 25 { 2 } else { 3 }] =
                            Some(store.snapshot(resources[0].unwrap(), Frame(start)).unwrap());
                    }
                    for voice in 0..2 {
                        let state = decisions[voice].state;
                        let plays = state.plays() && !(voice == 0 && (18..32).contains(&start));
                        let source = if !plays {
                            None
                        } else {
                            snapshots[if voice == 1 {
                                1
                            } else if start >= 32 {
                                3
                            } else {
                                0
                            }]
                        };
                        let index = source.map(|s| store.read_index(s.id).unwrap());
                        outputs[voice].clear();
                        let report = renderers[voice]
                            .render(
                                &mut store.render_io(),
                                Playback {
                                    source: index,
                                    position: state.position,
                                    discontinuity: decisions[voice].playback_discontinuity,
                                },
                                frames,
                                &mut outputs[voice],
                            )
                            .unwrap();
                        assert_eq!(report.output_error, None);
                        for event in 0..outputs[voice].len() {
                            let (offset, packet) = outputs[voice].get(event).unwrap();
                            emit(&mut observed, start + u64::from(offset), voice, packet).unwrap();
                        }
                        let index = if plays {
                            Some(store.read_index(snapshots[voice + 4].unwrap().id).unwrap())
                        } else {
                            None
                        };
                        audio_renderers[voice]
                            .render(
                                &mut store.render_io(),
                                shoop_loops::audio::Playback {
                                    source: index,
                                    position: state.position,
                                    gain: 1.,
                                    balance: 0.,
                                    discontinuity: decisions[voice].playback_discontinuity,
                                },
                                &mut [&mut audio_output[voice][start as usize..end as usize]],
                            )
                            .unwrap();
                    }
                    input.clear();
                    for (at, packet) in source.iter().filter(|(at, _)| *at >= start && *at < end) {
                        input.push((at - start) as u32, frames, packet).unwrap();
                        for (which, (first, last)) in [(18, 21), (24, 28)].into_iter().enumerate() {
                            if *at >= first && *at < last {
                                patches[which]
                                    .push((at - first) as u32, (last - first) as usize, packet)
                                    .unwrap();
                            }
                        }
                    }
                    capture
                        .append_history(
                            &mut store.render_io(),
                            midi,
                            interval.start,
                            frames,
                            &input,
                        )
                        .unwrap();
                    store
                        .render_io()
                        .append_audio_history(
                            audio,
                            interval.start,
                            &audio_input[start as usize..end as usize],
                        )
                        .unwrap();
                    assert_eq!(
                        *capture.state(),
                        projected(&source, MidiState::default(), end)
                    );
                    assert_eq!(store.copy_remaining(), 0);
                    resolver.finish().unwrap();
                    now = end;
                }
            }
            for (slot, version) in [0, 2, 3].into_iter().zip(&versions) {
                check(&mut store, snapshots[slot].unwrap(), &version.0, &version.1);
            }
            check(
                &mut store,
                snapshots[1].unwrap(),
                &sibling,
                &sibling_initial,
            );
            for snapshot in snapshots.into_iter().flatten() {
                store.release_snapshot(snapshot.id).unwrap();
            }
            for resource in resources.into_iter().flatten() {
                store.remove(resource).unwrap();
            }
            store.release_snapshot(control_snapshot.id).unwrap();
            store.remove(control).unwrap();
            for _ in 0..10000 {
                store.begin_callback();
                store.reclaim();
            }
            store.cleanup_midi_store(midi).unwrap();
            store.cleanup_audio_store(audio).unwrap();
            for _ in 0..10000 {
                store.begin_callback();
                store.reclaim();
            }
            assert_eq!(store.usage(), baseline);
        });
        observed.sort_by_key(|e| (e.frame, e.voice));
        assert_eq!(observed, expected);
        for voice in 0..2 {
            for frame in 0usize..48 {
                let plays = if voice == 0 {
                    (13..18).contains(&frame)
                        || (32..33).contains(&frame)
                        || (36..46).contains(&frame)
                } else {
                    (13..46).contains(&frame)
                };
                let position = if voice == 0 && frame >= 36 {
                    (2 + frame - 36) % 7
                } else {
                    frame.saturating_sub(13) % 7
                };
                assert_eq!(
                    audio_output[voice][frame],
                    if plays {
                        (if voice == 0 { 4 } else { 6 }) as f32 + position as f32 + 1.
                    } else {
                        0.
                    }
                );
            }
        }
    }
}

#[shoop_test]
fn failed_shared_capture_keeps_observed_state_and_requires_a_truthful_new_segment() {
    let mut store = ResourceStore::new(
        Config {
            page_bytes: 16,
            resource_pages: 128,
            ..Config::default()
        }
        .prepare()
        .unwrap(),
        rt_runtime::Quantum::new(1).unwrap(),
    )
    .unwrap();
    let history = store
        .prepare_midi_store(
            resource::shared_midi::Config {
                history_frames: 8,
                chunks: 1,
                views: 4,
                pages: 100,
                max_events: 1,
                max_bytes: 3,
                max_packet: 3,
                transactions: 1,
                edit_steps: 1,
            },
            Frame(0),
            &MidiState::default(),
        )
        .unwrap();
    let mut capture = MidiCapture::prepare(8, 4, 16).unwrap();
    let mut input = MidiBuffer::new(4, 16).unwrap();
    input.push(0, 1, &[0x90, 60, 80]).unwrap();
    input.push(0, 1, &[0x90, 61, 90]).unwrap();
    assert_no_heap(|| {
        assert_eq!(
            capture.append_history(&mut store.render_io(), history, Frame(0), 1, &input),
            Err(Error::Capacity)
        );
        assert_eq!(store.midi_coverage(history), Ok(Frame(0)..Frame(0)));
        let observed = *capture.state();
        assert!(observed.channel(0).unwrap().note(60).unwrap().held());
        assert!(observed.channel(0).unwrap().note(61).unwrap().held());
        store
            .render_io()
            .midi_history_discontinuity(history, Frame(1), capture.state())
            .unwrap();
        input.clear();
        capture
            .append_history(&mut store.render_io(), history, Frame(1), 3, &input)
            .unwrap();
        assert_eq!(
            store.grab_midi(history, Frame(0)..Frame(4)),
            Err(Error::InvalidFrame)
        );
        let resource = store.grab_midi(history, Frame(1)..Frame(4)).unwrap();
        let snapshot = store.snapshot(resource, Frame(4)).unwrap();
        let index = store.read_index(snapshot.id).unwrap();
        assert_eq!(
            store.render_io().midi_seek(index, 0).unwrap().state,
            observed
        );
        store.release_snapshot(snapshot.id).unwrap();
        store.remove(resource).unwrap();
        for _ in 0..256 {
            store.begin_callback();
            store.reclaim();
        }
        store.cleanup_midi_store(history).unwrap();
        for _ in 0..256 {
            store.begin_callback();
            store.reclaim();
        }
        assert_eq!(store.usage().free_pages, 128);
        assert_eq!(store.usage().reserved_pages, 0);
    });
}
