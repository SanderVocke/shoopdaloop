#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;

use rt_runtime::{
    resource::{
        shared_audio::{AudioStore, Config as AudioConfig, ExportId, TakeId},
        Description, Format, ResourceStore, Snapshot,
    },
    Budgets, Config, Error,
};
use shoop_loops::{
    audio::{AudioCapture, AudioRenderer, SharedPlayback, Smoothing},
    *,
};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

const FRAMES: usize = 48;

fn input(frame: usize, channel: usize) -> f32 {
    if channel == 0 {
        frame as f32 + 1.
    } else {
        -(frame as f32 + 1.) * 2.
    }
}

fn store() -> ResourceStore {
    ResourceStore::new(
        Config {
            max_frames: FRAMES,
            resource_pages: 96,
            resource_slots: 8,
            page_bytes: 24,
            budgets: Budgets {
                transfer_bytes: 24,
                validation_steps: 1,
                ..Budgets::default()
            },
            ..Config::default()
        }
        .prepare()
        .unwrap(),
    )
    .unwrap()
}

fn control_content(store: &mut ResourceStore) -> Snapshot {
    let resource = store.create().unwrap();
    let upload = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: 6,
                bytes: 24,
            },
        )
        .unwrap();
    store.begin_callback();
    store.write_upload(upload, 0, &[0; 24]).unwrap();
    store.finish_upload(upload).unwrap();
    store.commit(upload).unwrap();
    store.snapshot(resource, Frame(0)).unwrap()
}

fn original(first: usize) -> [f32; 14] {
    core::array::from_fn(|index| input(first + index / 2, index % 2))
}

fn export(media: &AudioStore, store: &mut ResourceStore, export: ExportId) -> [f32; 14] {
    let mut bytes = [0; 56];
    media
        .read_bytes(
            &mut store.render_io(),
            media.export_index(export).unwrap(),
            0,
            &mut bytes,
        )
        .unwrap();
    core::array::from_fn(|index| {
        f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
    })
}

fn take(media: &AudioStore, store: &mut ResourceStore, take: TakeId) -> [f32; 14] {
    let mut bytes = [0; 56];
    media
        .read_bytes(
            &mut store.render_io(),
            media.take_index(take).unwrap(),
            0,
            &mut bytes,
        )
        .unwrap();
    core::array::from_fn(|index| {
        f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
    })
}

fn oracle() -> ([[[f32; FRAMES]; 2]; 2], [f32; 14], [f32; 14]) {
    let mut output = [[[0.; FRAMES]; 2]; 2];
    let mut content = original(4);
    let mut middle = content;
    let mut a_position = 0;
    let mut b_position = 0;
    for frame in 13..FRAMES {
        if frame == 23 {
            middle = content;
        }
        if frame == 36 {
            a_position = 2;
        }
        if (18..29).contains(&frame) {
            for channel in 0..2 {
                content[a_position * 2 + channel] = input(frame, channel);
            }
        }
        if !(18..29).contains(&frame) && !(33..36).contains(&frame) && frame < 46 {
            for channel in 0..2 {
                output[0][channel][frame] = content[a_position * 2 + channel];
            }
        }
        if frame < 46 {
            for channel in 0..2 {
                output[1][channel][frame] = input(6 + b_position, channel);
            }
            b_position = (b_position + 1) % 7;
        }
        if !(33..36).contains(&frame) && frame < 46 {
            a_position = (a_position + 1) % 7;
        }
    }
    (output, content, middle)
}

#[shoop_test]
fn resolver_driven_grab_replace_export_and_history_are_partition_independent() {
    let (expected, final_content, middle_content) = oracle();
    for partitions in [vec![48], vec![1; 48], vec![5, 3, 7, 2, 11, 1, 8, 11]] {
        for limited in [false, true] {
            let mut store = store();
            let baseline = store.usage();
            let mut media = AudioStore::prepare(
                &mut store,
                AudioConfig {
                    channels: 2,
                    history_frames: 11,
                    views: if limited { 3 } else { 6 },
                    pages: 80,
                },
                Frame(0),
            )
            .unwrap();
            let control = control_content(&mut store);
            let a_id = LoopId::from_parts(0, 1).unwrap();
            let b_id = LoopId::from_parts(1, 1).unwrap();
            let topology = compile(
                Limits::default(),
                &TopologyDescription {
                    loops: vec![a_id, b_id]
                        .into_iter()
                        .map(|id| LoopSpec {
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
            let mut resolver = Resolver::prepare(topology).unwrap();
            resolver
                .enable_fixed(rt_runtime::fixed::Quantum::new(1).unwrap())
                .unwrap();
            let mut renderers = [
                AudioRenderer::prepare(2, FRAMES, Smoothing::None).unwrap(),
                AudioRenderer::prepare(2, FRAMES, Smoothing::None).unwrap(),
            ];
            let mut capture = AudioCapture::prepare(2, FRAMES).unwrap();
            let left: [f32; FRAMES] = core::array::from_fn(|frame| input(frame, 0));
            let right: [f32; FRAMES] = core::array::from_fn(|frame| input(frame, 1));
            let mut output = [[[0.; FRAMES]; 2]; 2];
            let mut a = None;
            let mut b = None;
            let mut old = None;
            let mut middle = None;
            assert_no_heap(|| {
                let mut sequence = 0;
                for (target, at, transition) in [
                    (
                        a_id,
                        13,
                        Transition::Play {
                            position: 0,
                            retrigger: false,
                        },
                    ),
                    (
                        b_id,
                        13,
                        Transition::Play {
                            position: 0,
                            retrigger: false,
                        },
                    ),
                    (
                        a_id,
                        18,
                        Transition::Replace {
                            length: 7,
                            revision: Revision(1),
                            preserve_position: true,
                        },
                    ),
                    (
                        a_id,
                        23,
                        Transition::Replace {
                            length: 7,
                            revision: Revision(2),
                            preserve_position: true,
                        },
                    ),
                    (
                        a_id,
                        29,
                        Transition::Replace {
                            length: 7,
                            revision: Revision(3),
                            preserve_position: true,
                        },
                    ),
                    (a_id, 33, Transition::Stop),
                    (
                        a_id,
                        36,
                        Transition::Play {
                            position: 2,
                            retrigger: false,
                        },
                    ),
                    (a_id, 46, Transition::Stop),
                    (b_id, 46, Transition::Stop),
                ] {
                    sequence += 1;
                    resolver
                        .submit(Request {
                            target,
                            sequence: Sequence(sequence),
                            at: Frame(at),
                            timing: Timing::At,
                            transition,
                        })
                        .unwrap();
                }
                let mut ceiling = 0;
                for frames in &partitions {
                    ceiling += frames;
                    store.begin_callback();
                    let mut cursor = store.cursor(control.id).unwrap();
                    assert_eq!(store.read(&mut cursor, &mut [0; 24]), Ok(24));
                    assert_eq!(store.copy_remaining(), 0);
                    loop {
                        let interval = resolver.preview_fixed(&[]).unwrap();
                        assert!(interval.end <= Frame(ceiling));
                        let decisions = [
                            resolver.decisions().unwrap()[0],
                            resolver.decisions().unwrap()[1],
                        ];
                        resolver.commit().unwrap();
                        let start = interval.start.0 as usize;
                        let end = interval.end.0 as usize;
                        if start == 13 {
                            a = Some(media.grab(Frame(4)..Frame(11)).unwrap());
                            b = Some(media.grab(Frame(6)..Frame(13)).unwrap());
                            old = Some(media.export(a.unwrap()).unwrap());
                        }
                        if start == 23 {
                            let before = media.info(media.take_index(a.unwrap()).unwrap()).unwrap();
                            match media.export(a.unwrap()) {
                                Ok(export) => {
                                    assert!(!limited);
                                    middle = Some(export);
                                }
                                Err(Error::Capacity) => assert!(limited),
                                result => panic!("unexpected export: {result:?}"),
                            }
                            assert_eq!(
                                media.info(media.take_index(a.unwrap()).unwrap()).unwrap(),
                                before
                            );
                        }
                        if (18..29).contains(&start) {
                            let mut patch = [0.; FRAMES * 2];
                            for frame in start..end {
                                patch[(frame - start) * 2] = left[frame];
                                patch[(frame - start) * 2 + 1] = right[frame];
                            }
                            media
                                .replace(
                                    &mut store.render_io(),
                                    a.unwrap(),
                                    decisions[0].state.position,
                                    &patch[..(end - start) * 2],
                                )
                                .unwrap();
                        }
                        for index in 0..2 {
                            let state = decisions[index].state;
                            let owner = if index == 0 { a } else { b };
                            let source =
                                if state.plays() && !(index == 0 && (18..29).contains(&start)) {
                                    Some(media.take_index(owner.unwrap()).unwrap())
                                } else {
                                    None
                                };
                            let [left, right] = &mut output[index];
                            renderers[index]
                                .render_shared(
                                    &mut store.render_io(),
                                    &media,
                                    SharedPlayback {
                                        source,
                                        position: state.position,
                                        gain: 1.,
                                        balance: 0.,
                                        discontinuity: decisions[index].playback_discontinuity,
                                    },
                                    &mut [&mut left[start..end], &mut right[start..end]],
                                )
                                .unwrap();
                        }
                        capture
                            .append_shared(
                                &mut store.render_io(),
                                &mut media,
                                interval.start,
                                &[&left[start..end], &right[start..end]],
                            )
                            .unwrap();
                        if let Some(old) = old {
                            assert_eq!(export(&media, &mut store, old), original(4));
                        }
                        if let Some(middle) = middle {
                            assert_eq!(export(&media, &mut store, middle), middle_content);
                        }
                        assert_eq!(store.copy_remaining(), 0);
                        resolver.finish(interval.end).unwrap();
                        if interval.end.0 == ceiling {
                            break;
                        }
                    }
                }
                assert_eq!(output, expected);
                assert_eq!(take(&media, &mut store, a.unwrap()), final_content);
                assert_eq!(take(&media, &mut store, b.unwrap()), original(6));
                assert_eq!(
                    media
                        .info(media.take_index(a.unwrap()).unwrap())
                        .unwrap()
                        .frames,
                    7
                );
                let latest = media.grab(Frame(37)..Frame(48));
                if limited {
                    assert_eq!(latest, Err(Error::Capacity));
                } else {
                    let latest = latest.unwrap();
                    let mut bytes = [0; 88];
                    media
                        .read_bytes(
                            &mut store.render_io(),
                            media.take_index(latest).unwrap(),
                            0,
                            &mut bytes,
                        )
                        .unwrap();
                    for frame in 0..11 {
                        for channel in 0..2 {
                            let offset = (frame * 2 + channel) * 4;
                            assert_eq!(
                                f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()),
                                input(37 + frame, channel)
                            );
                        }
                    }
                    media.release_take(latest).unwrap();
                }
                media.release_take(a.unwrap()).unwrap();
                media.release_take(b.unwrap()).unwrap();
                media.release_export(old.unwrap()).unwrap();
                if let Some(middle) = middle {
                    media.release_export(middle).unwrap();
                }
                for _ in 0..media.capacity().index_nodes {
                    store.begin_callback();
                    assert!(media.maintain(&mut store).unwrap() <= 1);
                }
                media.begin_cleanup().unwrap();
                for _ in 0..media.capacity().index_nodes {
                    store.begin_callback();
                    assert!(media.maintain(&mut store).unwrap() <= 1);
                }
                store.release_snapshot(control.id).unwrap();
                store.remove(control.resource).unwrap();
                for _ in 0..2 {
                    store.begin_callback();
                    store.reclaim();
                }
                assert_eq!(store.usage(), baseline);
            });
        }
    }
}
