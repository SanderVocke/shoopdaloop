mod support;
use rt_runtime::{midi_state::MidiState, resource::*, Budgets, Config, Error, Frame, Revision};
use shoop_wasm_test_support::shoop_test;
use std::collections::BTreeMap;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn config(page: usize, steps: usize) -> rt_runtime::PreparedConfig {
    Config {
        max_frames: 64,
        resource_slots: 8,
        resource_pages: 512,
        page_bytes: page,
        midi_bytes_per_port: 512,
        midi_events_per_port: 32,
        budgets: Budgets {
            transfer_bytes: page,
            validation_steps: steps,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap()
}
fn store(
    versions: usize,
    checkpoints: usize,
    span: usize,
    page: usize,
    steps: usize,
) -> ResourceStore {
    ResourceStore::with_midi_indexing(
        config(page, steps),
        MidiIndexConfig {
            versions,
            checkpoints_per_version: checkpoints,
            replay_events: span,
        },
    )
    .unwrap()
}
fn bytes(records: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let mut result = Vec::new();
    for (frame, data) in records {
        let mut row = vec![0; 12 + data.len()];
        encode_midi_record(*frame, data, &mut row).unwrap();
        result.extend(row);
    }
    result
}
fn upload(
    store: &mut ResourceStore,
    records: &[(u64, Vec<u8>)],
    frames: u64,
    initial: &MidiState,
) -> Snapshot {
    let data = bytes(records);
    let resource = store.create().unwrap();
    let id = assert_no_heap(|| {
        store.begin_indexed_midi_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Midi,
                frames,
                bytes: data.len(),
            },
            initial,
        )
    })
    .unwrap();
    for (offset, byte) in data.iter().enumerate() {
        assert_no_heap(|| {
            store.begin_callback();
            store
                .write_upload(id, offset, std::slice::from_ref(byte))
                .unwrap();
        });
    }
    assert_no_heap(|| {
        store.finish_upload(id).unwrap();
        store.commit(id).unwrap();
        store.snapshot(resource, Frame(0))
    })
    .unwrap()
}
fn packet(store: &mut ResourceStore, event: MidiEvent, expected: &[u8]) {
    let mut data = [0; 512];
    let n = store.render_io().midi_read(event, &mut data).unwrap();
    assert_eq!(&data[..n], expected);
}
#[shoop_test]
fn indexed_positions_match_linear_event_and_independent_key_state_oracles() {
    let mut store = store(2, 40, 8, 16, 1);
    let mut initial = MidiState::default();
    for p in [
        &[0x90, 100, 111][..],
        &[0xb0, 7, 90],
        &[0xd1, 12],
        &[0xc2, 7],
    ] {
        initial.apply_packet(p, 3).unwrap();
    }
    let mut records = Vec::new();
    for f in 0..64u64 {
        records.push((f, vec![0x90, (f % 8) as u8, (f % 127 + 1) as u8]));
        records.push((f, vec![0xb0, 64, if f % 7 < 3 { 127 } else { 0 }]));
        records.push((f, vec![0x80, ((f + 2) % 8) as u8, 64]));
        records.push((f, vec![0xc1, f as u8]));
        if f == 10 {
            for key in 80..96 {
                records.push((f, vec![0x90, key, key - 79]));
            }
        }
        if f == 20 {
            let mut sysex = vec![1; 257];
            sysex[0] = 0xf0;
            sysex[256] = 0xf7;
            records.push((f, sysex));
        }
    }
    let snapshot = upload(&mut store, &records, 80, &initial);
    let index = store.read_index(snapshot.id).unwrap();
    for frame in 0..=80 {
        let mut notes = BTreeMap::from([(100u8, (111u8, true))]);
        let mut sustain = false;
        let mut program = None;
        for (_, data) in records.iter().filter(|(f, _)| *f < frame) {
            match data[0] {
                0x90 => {
                    notes.insert(data[1], (data[2], true));
                }
                0x80 => {
                    if sustain {
                        if let Some(n) = notes.get_mut(&data[1]) {
                            n.1 = false;
                        }
                    } else {
                        notes.remove(&data[1]);
                    }
                }
                0xb0 => {
                    sustain = data[2] >= 64;
                    if !sustain {
                        notes.retain(|_, (_, held)| *held);
                    }
                }
                0xc1 => program = Some(data[1]),
                _ => {}
            }
        }
        assert_no_heap(|| {
            store.begin_callback();
            let mut position = store.render_io().midi_seek(index, frame).unwrap();
            assert!(position.replayed_events < 8);
            assert_eq!(
                store.midi_index_work().replayed_records,
                position.replayed_events
            );
            assert_eq!(store.copy_remaining(), 16);
            assert_eq!(store.steps_remaining(), 1);
            if frame == 21 {
                assert!(
                    store.render_work().copied_bytes < 200,
                    "seek must skip the long SysEx body"
                );
            }
            let channel = position.state.channel(0).unwrap();
            assert_eq!(channel.controller(64).unwrap() >= 64, sustain);
            assert_eq!(channel.controller(7), Some(90));
            for key in 0..128 {
                let expected = notes.get(&key).copied().unwrap_or((0, false));
                let actual = channel.note(key).unwrap();
                assert_eq!((actual.velocity(), actual.held()), expected);
            }
            assert_eq!(position.state.channel(1).unwrap().program(), program);
            assert_eq!(position.state.channel(1).unwrap().pressure(), Some(12));
            assert_eq!(position.state.channel(2).unwrap().program(), Some(7));
            let next = store.render_io().midi_next(&mut position.cursor).unwrap();
            match (records.iter().find(|(f, _)| *f >= frame), next) {
                (None, None) => {}
                (Some((f, data)), Some(event)) => {
                    assert_eq!(event.frame(), *f);
                    packet(&mut store, event, data);
                }
                _ => panic!("wrong seek boundary"),
            }
        });
    }
    assert_no_heap(|| {
        let mut cursor = store.render_io().midi_cursor(index).unwrap();
        for (frame, data) in &records {
            let event = store.render_io().midi_next(&mut cursor).unwrap().unwrap();
            assert_eq!(event.frame(), *frame);
            packet(&mut store, event, data);
        }
        assert_eq!(store.render_io().midi_next(&mut cursor), Ok(None));
    });
}
#[shoop_test]
fn captured_prefixes_keep_their_state_after_later_appends_and_freeze_needs_no_build() {
    let mut store = store(2, 8, 2, 16, 1);
    let resource = store.create().unwrap();
    let mut initial = MidiState::default();
    initial.apply_packet(&[0xc0, 17], 3).unwrap();
    let capture = store
        .begin_indexed_midi_capture(resource, Revision(0), Frame(100), &initial)
        .unwrap();
    let mut first = rt_runtime::buffer::MidiBuffer::new(3, 9).unwrap();
    for (offset, data) in [
        (0, [0x90, 60, 100]),
        (1, [0xb0, 64, 127]),
        (3, [0x80, 60, 0]),
    ] {
        first.push(offset, 4, &data).unwrap();
    }
    let mut second = rt_runtime::buffer::MidiBuffer::new(3, 9).unwrap();
    for (offset, data) in [(0, [0xb0, 64, 0]), (0, [0x90, 61, 80]), (3, [0x80, 61, 0])] {
        second.push(offset, 4, &data).unwrap();
    }
    assert_no_heap(|| {
        store.begin_callback();
        store
            .render_io()
            .capture_midi(capture, Frame(100), 4, &first)
            .unwrap();
        let old = store.snapshot(resource, Frame(104)).unwrap();
        let old_index = store.read_index(old.id).unwrap();
        store
            .render_io()
            .capture_midi(capture, Frame(104), 4, &second)
            .unwrap();
        let work = store.midi_index_work();
        store.stop_capture(capture).unwrap();
        let frozen = store.snapshot(resource, Frame(108)).unwrap();
        let frozen_index = store.read_index(frozen.id).unwrap();
        assert_eq!(store.midi_index_work(), work);
        let old_state = store.render_io().midi_seek(old_index, 4).unwrap().state;
        assert_eq!(
            old_state.channel(0).unwrap().note(60).unwrap().velocity(),
            100
        );
        assert!(!old_state.channel(0).unwrap().note(60).unwrap().held());
        assert_eq!(old_state.channel(0).unwrap().controller(64), Some(127));
        assert_eq!(
            store.render_io().midi_seek(old_index, 5),
            Err(Error::InvalidFrame)
        );
        let state = store.render_io().midi_seek(frozen_index, 8).unwrap().state;
        assert_eq!(state.channel(0).unwrap().note(60).unwrap().velocity(), 0);
        assert_eq!(state.channel(0).unwrap().note(61).unwrap().velocity(), 0);
        assert_eq!(state.channel(0).unwrap().program(), Some(17));
        store.remove(resource).unwrap();
        assert_eq!(store.midi_index_usage().free_versions, 1);
        assert_eq!(
            store.render_io().midi_seek(old_index, 4).unwrap().state,
            old_state
        );
        store.release_snapshot(old.id).unwrap();
        store.release_snapshot(frozen.id).unwrap();
        assert_eq!(store.midi_index_usage().free_versions, 2);
    });
}
#[shoop_test]
fn checkpoint_exhaustion_refuses_before_upload_or_capture_prefix_mutation() {
    let mut store = store(1, 2, 2, 64, 64);
    let resource = store.create().unwrap();
    let initial = MidiState::default();
    let records: Vec<_> = (0..4).map(|f| (f, vec![0x90, f as u8, 100])).collect();
    let data = bytes(&records);
    let id = store
        .begin_indexed_midi_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Midi,
                frames: 4,
                bytes: 60,
            },
            &initial,
        )
        .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        store.write_upload(id, 0, &data[..45]).unwrap();
        let work = store.midi_index_work();
        assert_eq!(
            store.write_upload(id, 45, &data[45..]),
            Err(Error::Capacity)
        );
        assert_eq!(store.midi_index_work(), work);
        assert_eq!(store.work().copied_bytes, 45);
        assert_eq!(store.finish_upload(id), Err(Error::Incomplete));
        store.cancel_upload(id).unwrap();
        assert_eq!(store.midi_index_usage().free_versions, 1);
    });
    let capture = store
        .begin_indexed_midi_capture(resource, Revision(0), Frame(0), &initial)
        .unwrap();
    let mut three = rt_runtime::buffer::MidiBuffer::new(3, 9).unwrap();
    for i in 0..3 {
        three.push(i, 4, &[0x90, i as u8, 100]).unwrap();
    }
    let mut one = rt_runtime::buffer::MidiBuffer::new(1, 3).unwrap();
    one.push(0, 1, &[0x90, 80, 100]).unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        store
            .render_io()
            .capture_midi(capture, Frame(0), 4, &three)
            .unwrap();
        let info = store.midi_index_info(resource).unwrap();
        assert_eq!(
            store.render_io().capture_midi(capture, Frame(4), 1, &one),
            Err(Error::Capacity)
        );
        assert_eq!(store.midi_index_info(resource), Ok(info));
        let content = store.info(resource).unwrap();
        assert_eq!(content.description.bytes, 45);
        assert_eq!(content.description.frames, 4);
        assert_eq!(content.capture.unwrap().incomplete, Some(Error::Capacity));
        let snapshot = store.snapshot(resource, Frame(4)).unwrap();
        let index = store.read_index(snapshot.id).unwrap();
        let state = store.render_io().midi_seek(index, 4).unwrap().state;
        assert_eq!(state.channel(0).unwrap().note(80).unwrap().velocity(), 0);
    });
}
#[shoop_test]
fn malformed_chunk_does_not_publish_earlier_records_or_checkpoint_state() {
    let mut store = store(1, 3, 1, 64, 64);
    let resource = store.create().unwrap();
    let initial = MidiState::default();
    let good = bytes(&[(0, vec![0x90, 60, 40]), (0, vec![0x90, 61, 50])]);
    let mut bad = good.clone();
    bad[29] = 0x80;
    let id = store
        .begin_indexed_midi_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Midi,
                frames: 2,
                bytes: 30,
            },
            &initial,
        )
        .unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        assert_eq!(store.write_upload(id, 0, &bad), Err(Error::InvalidEvent));
        assert_eq!(store.midi_index_work(), MidiIndexWork::default());
        assert_eq!(store.work().copied_bytes, 0);
        store.begin_callback();
        store.write_upload(id, 0, &good).unwrap();
        store.finish_upload(id).unwrap();
        store.commit(id).unwrap();
        let snapshot = store.snapshot(resource, Frame(0)).unwrap();
        let index = store.read_index(snapshot.id).unwrap();
        let mut at_zero = store.render_io().midi_seek(index, 0).unwrap();
        assert_eq!(at_zero.state, initial);
        assert_eq!(
            store
                .render_io()
                .midi_next(&mut at_zero.cursor)
                .unwrap()
                .unwrap()
                .frame(),
            0
        );
        let state = store.render_io().midi_seek(index, 1).unwrap().state;
        assert_eq!(state.channel(0).unwrap().note(60).unwrap().velocity(), 40);
        assert_eq!(state.channel(0).unwrap().note(61).unwrap().velocity(), 50);
    });
}
#[shoop_test]
fn scoped_midi_events_and_cursors_refuse_foreign_or_released_leases() {
    let mut a = store(1, 2, 1, 16, 1);
    let mut b = store(1, 2, 1, 16, 1);
    let records = [(0, vec![0x90, 60, 100])];
    let sa = upload(&mut a, &records, 2, &MidiState::default());
    let sb = upload(&mut b, &records, 2, &MidiState::default());
    assert_eq!(sa.id, sb.id);
    let index = a.read_index(sa.id).unwrap();
    let mut cursor = a.render_io().midi_cursor(index).unwrap();
    assert_no_heap(|| {
        let before = cursor;
        assert_eq!(b.render_io().midi_next(&mut cursor), Err(Error::InvalidId));
        assert_eq!(cursor, before);
        assert_eq!(b.render_io().midi_seek(index, 0), Err(Error::InvalidId));
        let event = a.render_io().midi_next(&mut cursor).unwrap().unwrap();
        let mut output = [55; 3];
        assert_eq!(
            b.render_io().midi_read(event, &mut output),
            Err(Error::InvalidId)
        );
        assert_eq!(output, [55; 3]);
        assert_eq!(
            a.render_io().midi_read(event, &mut [0; 2]),
            Err(Error::Capacity)
        );
        a.release_snapshot(sa.id).unwrap();
        let replacement = a.snapshot(sa.resource, Frame(0)).unwrap();
        assert_ne!(replacement.id.generation, sa.id.generation);
        assert_eq!(
            a.render_io().midi_read(event, &mut output),
            Err(Error::InvalidId)
        );
        assert_eq!(a.render_io().midi_seek(index, 1), Err(Error::InvalidId));
        assert_eq!(output, [55; 3]);
    });
}
#[shoop_test]
fn checkpoint_rows_retain_old_revisions_and_recycle_without_scanning_them() {
    let mut store = store(2, 4, 2, 16, 1);
    let baseline = store.usage();
    let initial = MidiState::default();
    let first = upload(
        &mut store,
        &[(0, vec![0x90, 60, 30]), (1, vec![0x80, 60, 0])],
        4,
        &initial,
    );
    let index = store.read_index(first.id).unwrap();
    let next = store
        .begin_indexed_midi_upload(
            first.resource,
            first.revision,
            Description {
                format: Format::Midi,
                frames: 4,
                bytes: 0,
            },
            &initial,
        )
        .unwrap();
    store.finish_upload(next).unwrap();
    store.commit(next).unwrap();
    assert_no_heap(|| {
        assert_eq!(store.midi_index_usage().free_versions, 0);
        assert_eq!(
            store
                .render_io()
                .midi_seek(index, 1)
                .unwrap()
                .state
                .channel(0)
                .unwrap()
                .note(60)
                .unwrap()
                .velocity(),
            30
        );
        let work = store.midi_index_work();
        store.release_snapshot(first.id).unwrap();
        assert_eq!(store.midi_index_work(), work);
        assert_eq!(store.midi_index_usage().free_versions, 1);
        store.remove(first.resource).unwrap();
        assert_eq!(store.midi_index_usage().free_versions, 2);
        for _ in 0..520 {
            store.begin_callback();
            assert!(store.reclaim() <= 1);
        }
        assert_eq!(store.usage(), baseline);
    });
    let reused = upload(&mut store, &[(0, vec![0x90, 62, 44])], 2, &initial);
    let fresh = store.read_index(reused.id).unwrap();
    assert_no_heap(|| {
        assert_eq!(
            store
                .render_io()
                .midi_seek(fresh, 1)
                .unwrap()
                .state
                .channel(0)
                .unwrap()
                .note(62)
                .unwrap()
                .velocity(),
            44
        );
        assert_eq!(store.render_io().midi_seek(index, 1), Err(Error::InvalidId));
    });
}
#[shoop_test]
fn empty_and_max_frame_content_keep_initial_state_and_half_open_boundaries() {
    let mut store = store(2, 2, 1, 16, 1);
    let mut initial = MidiState::default();
    initial.apply_packet(&[0x90, 100, 44], 3).unwrap();
    let empty = upload(&mut store, &[], 0, &initial);
    let empty_index = store.read_index(empty.id).unwrap();
    let last = upload(
        &mut store,
        &[(u64::MAX - 1, vec![0x90, 60, 50])],
        u64::MAX,
        &initial,
    );
    let index = store.read_index(last.id).unwrap();
    assert_no_heap(|| {
        let mut position = store.render_io().midi_seek(empty_index, 0).unwrap();
        assert_eq!(position.state, initial);
        assert_eq!(store.render_io().midi_next(&mut position.cursor), Ok(None));
        let mut before = store.render_io().midi_seek(index, u64::MAX - 1).unwrap();
        assert_eq!(before.state, initial);
        assert_eq!(
            store
                .render_io()
                .midi_next(&mut before.cursor)
                .unwrap()
                .unwrap()
                .frame(),
            u64::MAX - 1
        );
        let mut end = store.render_io().midi_seek(index, u64::MAX).unwrap();
        assert_eq!(
            end.state.channel(0).unwrap().note(60).unwrap().velocity(),
            50
        );
        assert_eq!(store.render_io().midi_next(&mut end.cursor), Ok(None));
    });
}
#[shoop_test]
fn preparation_bounds_and_legacy_unindexed_media_are_explicit() {
    for indexing in [
        MidiIndexConfig {
            versions: 0,
            checkpoints_per_version: 1,
            replay_events: 1,
        },
        MidiIndexConfig {
            versions: 1,
            checkpoints_per_version: 0,
            replay_events: 1,
        },
        MidiIndexConfig {
            versions: 1,
            checkpoints_per_version: 1,
            replay_events: 0,
        },
    ] {
        assert!(matches!(
            ResourceStore::with_midi_indexing(config(16, 1), indexing),
            Err(Error::InvalidConfig)
        ));
    }
    assert!(matches!(
        ResourceStore::with_midi_indexing(
            config(16, 1),
            MidiIndexConfig {
                versions: usize::MAX,
                checkpoints_per_version: 2,
                replay_events: 1
            }
        ),
        Err(Error::Capacity)
    ));
    let mut store = ResourceStore::new(config(16, 1)).unwrap();
    let resource = store.create().unwrap();
    let initial = MidiState::default();
    let description = Description {
        format: Format::Midi,
        frames: 0,
        bytes: 0,
    };
    let baseline = store.usage();
    assert_no_heap(|| {
        assert_eq!(
            store.begin_indexed_midi_upload(resource, Revision(0), description, &initial),
            Err(Error::InvalidState)
        );
        assert_eq!(store.usage(), baseline);
        let id = store
            .begin_upload(resource, Revision(0), description)
            .unwrap();
        store.finish_upload(id).unwrap();
        store.commit(id).unwrap();
        let snapshot = store.snapshot(resource, Frame(0)).unwrap();
        let index = store.read_index(snapshot.id).unwrap();
        assert_eq!(
            store.render_io().midi_seek(index, 0),
            Err(Error::InvalidState)
        );
        let mut cursor = store.render_io().midi_cursor(index).unwrap();
        assert_eq!(store.render_io().midi_next(&mut cursor), Ok(None));
    });
}
