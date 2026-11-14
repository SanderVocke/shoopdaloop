use super::*;
use crate::{Budgets, Config as RuntimeConfig};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

fn setup(chunks: usize, horizon: u64) -> (ResourceStore, MidiStore, MidiBuffer) {
    let mut store = ResourceStore::new(
        RuntimeConfig {
            page_bytes: 16,
            resource_pages: 4096,
            budgets: Budgets {
                transfer_bytes: 16,
                validation_steps: 1,
                ..Budgets::default()
            },
            ..RuntimeConfig::default()
        }
        .prepare()
        .unwrap(),
        crate::Quantum::new(1).unwrap(),
    )
    .unwrap();
    let midi = MidiStore::prepare(
        &mut store,
        Config {
            history_frames: horizon,
            chunks,
            views: 8,
            pages: 4000,
            max_events: 32,
            max_bytes: 256,
            max_packet: 48,
            transactions: 2,
            edit_steps: 3,
        },
        Frame(0),
        &MidiState::default(),
    )
    .unwrap();
    (store, midi, MidiBuffer::new(32, 256).unwrap())
}

fn check(
    store: &mut ResourceStore,
    midi: &mut MidiStore,
    take: TakeId,
    duration: u64,
    events: &[(u64, &[u8])],
    initial: &MidiState,
) {
    let index = midi.take_index(take).unwrap();
    assert_eq!(midi.info(index).unwrap().frames, duration);
    assert_eq!(midi.info(index).unwrap().events, events.len());
    assert_eq!(
        *midi.initial_state(index).unwrap(),
        initial.basic_projection()
    );
    let mut cursor = midi.cursor(index).unwrap();
    let mut packet = [0; 48];
    for &(frame, data) in events {
        let event = midi.next(&mut cursor).unwrap().unwrap();
        assert_eq!(event.frame(), frame);
        assert_eq!(
            midi.read_event(&mut store.render_io(), event, &mut packet)
                .unwrap(),
            data.len()
        );
        assert_eq!(&packet[..data.len()], data);
    }
    assert_eq!(midi.next(&mut cursor), Ok(None));
    for frame in 0..=duration {
        let mut expected = *initial;
        for &(_, data) in events.iter().filter(|(at, _)| *at < frame) {
            expected.apply_packet(data, 48).unwrap();
        }
        let position = midi.seek(&mut store.render_io(), index, frame).unwrap();
        assert_eq!(position.state, expected.basic_projection(), "at {frame}");
        assert_eq!(
            midi.parameter_history_untracked(&mut store.render_io(), index, frame)
                .unwrap(),
            (0..16).any(|channel| expected
                .channel(channel)
                .unwrap()
                .parameter_history_untracked())
        );
        assert!(position.replayed_events <= EVENTS_PER_CHUNK);
        let mut cursor = position.cursor;
        assert_eq!(
            midi.next(&mut cursor).unwrap().map(|e| e.frame()),
            events.iter().find(|(at, _)| *at >= frame).map(|e| e.0)
        );
    }
}

fn drain(store: &mut ResourceStore, midi: &mut MidiStore) {
    for _ in 0..100_000 {
        if midi.retired.is_empty() {
            return;
        }
        store.begin_callback();
        assert!(midi.maintain(store).unwrap() <= 1);
    }
    panic!("retirement did not finish");
}

fn cleanup(store: &mut ResourceStore, midi: &mut MidiStore) {
    drain(store, midi);
    midi.begin_cleanup().unwrap();
    drain(store, midi);
    store.begin_callback();
    midi.maintain(store).unwrap();
    assert_eq!(store.usage().reserved_pages, 0);
    assert_eq!(store.usage().free_pages, 4096);
    assert_eq!(midi.free_nodes.len(), midi.nodes.len());
    assert_eq!(midi.free_chunks.len(), midi.chunks.len());
    assert_eq!(midi.free_blocks.len(), midi.blocks.len());
}

#[shoop_test]
fn overlapping_cuts_share_packets_and_preserve_strict_boundary_state() {
    let (mut store, mut midi, mut input) = setup(8, 64);
    let control = [0xb0, 1, 35];
    let original = [
        (0, &[0x90, 60, 90][..]),
        (3, &[0xb0, 64, 127][..]),
        (3, &[0x80, 60, 0][..]),
        (5, &control[..]),
        (8, &[0xb0, 64, 0][..]),
    ];
    for &(at, data) in &original {
        input.push(at as u32, 12, data).unwrap();
    }
    assert_no_heap(|| {
        store.begin_callback();
        store.charge_external(16, 1).unwrap();
        midi.append(&mut store.render_io(), Frame(0), 12, &input)
            .unwrap();
        let before = store.usage().free_pages;
        let a = midi
            .grab(&mut store.render_io(), Frame(3)..Frame(10))
            .unwrap();
        let b = midi
            .grab(&mut store.render_io(), Frame(4)..Frame(8))
            .unwrap();
        let fork = midi.fork(a).unwrap();
        let export = midi.export(a).unwrap();
        assert_eq!(store.usage().free_pages, before);
        let mut at3 = MidiState::default();
        at3.apply_packet(original[0].1, 48).unwrap();
        check(
            &mut store,
            &mut midi,
            a,
            7,
            &[
                (0, original[1].1),
                (0, original[2].1),
                (2, &control),
                (5, original[4].1),
            ],
            &at3,
        );
        let mut at4 = at3;
        at4.apply_packet(original[1].1, 48).unwrap();
        at4.apply_packet(original[2].1, 48).unwrap();
        check(&mut store, &mut midi, b, 4, &[(1, &control)], &at4);
        input.clear();
        midi.append(&mut store.render_io(), Frame(12), 100, &input)
            .unwrap();
        let empty = midi
            .grab(&mut store.render_io(), Frame(90)..Frame(100))
            .unwrap();
        let mut final_state = at4;
        final_state.apply_packet(original[3].1, 48).unwrap();
        final_state.apply_packet(original[4].1, 48).unwrap();
        check(&mut store, &mut midi, empty, 10, &[], &final_state);
        assert_eq!(midi.begin_cleanup(), Err(Error::Busy));
        midi.release_take(a).unwrap();
        assert_eq!(midi.take_index(a), Err(Error::InvalidId));
        assert!(midi.export_index(export).is_ok());
        midi.release_take(b).unwrap();
        midi.release_take(fork).unwrap();
        midi.release_take(empty).unwrap();
        midi.release_export(export).unwrap();
        cleanup(&mut store, &mut midi);
    });
}

#[shoop_test]
fn seeded_density_and_time_eviction_match_a_linear_history_across_u32() {
    for (chunks, horizon) in [(2, 500), (8, 7)] {
        let (mut store, mut midi, mut input) = setup(chunks, horizon);
        let origin = u64::from(u32::MAX) - 10;
        midi.discontinuity(&mut store.render_io(), Frame(origin), &MidiState::default())
            .unwrap();
        let mut oracle = Vec::<(u64, [u8; 3])>::with_capacity(4096);
        let mut selected = Vec::<(u64, &[u8])>::with_capacity(4096);
        let mut intervals = Vec::with_capacity(128);
        let mut seed = 0x527104abu32;
        let mut next = origin;
        for tick in 0..128 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let frames = 1 + (seed as usize % 64);
            let count = (seed as usize / 64 % 17).min(frames);
            let first = oracle.len();
            for event in 0..count {
                let frame = event * frames / count;
                let packet = [0xb0 | (tick % 16) as u8, 1, (oracle.len() % 128) as u8];
                oracle.push((next + frame as u64, packet));
            }
            intervals.push((next, frames, first, oracle.len()));
            next += frames as u64;
        }
        assert_no_heap(|| {
            let mut pinned = None;
            for &(mut next, frames, first, count) in &intervals {
                input.clear();
                for &(at, packet) in &oracle[first..count] {
                    input.push((at - next) as u32, frames, &packet).unwrap();
                }
                store.begin_callback();
                store.charge_external(16, 1).unwrap();
                midi.append(&mut store.render_io(), Frame(next), frames, &input)
                    .unwrap();
                next += frames as u64;
                // Three-byte packets pack five to a sixteen-byte page.
                let allocated = count.div_ceil(5);
                let pressure = if allocated > chunks {
                    oracle[(allocated - chunks) * 5 - 1].0 + 1
                } else {
                    origin
                };
                let start = origin.max(next.saturating_sub(horizon)).max(pressure);
                assert_eq!(midi.coverage(), Ok(Frame(start)..Frame(next)));
                if start < next {
                    let cut = start + (next - start) / 3;
                    let take = midi
                        .grab(&mut store.render_io(), Frame(cut)..Frame(next))
                        .unwrap();
                    let mut initial = MidiState::default();
                    for (_, packet) in oracle[..count].iter().filter(|(at, _)| *at < cut) {
                        initial.apply_packet(packet, 48).unwrap();
                    }
                    selected.clear();
                    selected.extend(
                        oracle[..count]
                            .iter()
                            .filter(|(at, _)| *at >= cut)
                            .map(|(at, p)| (*at - cut, &p[..])),
                    );
                    check(&mut store, &mut midi, take, next - cut, &selected, &initial);
                    if pinned.is_none() {
                        pinned = Some((take, cut, next, initial, count));
                    } else {
                        midi.release_take(take).unwrap();
                    }
                }
                if let Some((take, cut, end, initial, count)) = pinned {
                    selected.clear();
                    selected.extend(
                        oracle[..count]
                            .iter()
                            .filter(|(at, _)| *at >= cut && *at < end)
                            .map(|(at, p)| (*at - cut, &p[..])),
                    );
                    check(&mut store, &mut midi, take, end - cut, &selected, &initial);
                }
                drain(&mut store, &mut midi);
                assert!(store.usage().free_pages >= store.usage().reserved_pages);
            }
            if let Some((take, _, _, _, _)) = pinned {
                midi.release_take(take).unwrap();
            }
            cleanup(&mut store, &mut midi);
        });
    }
}

#[shoop_test]
fn refusal_gap_recovery_and_foreign_handles_do_not_publish_partial_history() {
    let (mut store, mut midi, mut input) = setup(4, 32);
    let (mut foreign, mut other, _other_input) = setup(4, 32);
    assert_no_heap(|| {
        input.push(0, 4, &[0x90, 60, 80]).unwrap();
        midi.append(&mut store.render_io(), Frame(0), 4, &input)
            .unwrap();
        let take = midi
            .grab(&mut store.render_io(), Frame(1)..Frame(4))
            .unwrap();
        assert_eq!(other.take_index(take), Err(Error::InvalidId));
        let before = midi.usage();
        assert_eq!(
            midi.append(&mut foreign.render_io(), Frame(4), 4, &input),
            Err(Error::InvalidId)
        );
        assert_eq!(
            midi.append(&mut store.render_io(), Frame(4), 0, &input),
            Err(Error::InvalidFrame)
        );
        assert_eq!(
            midi.append(&mut store.render_io(), Frame(5), 4, &input),
            Err(Error::InvalidFrame)
        );
        assert_eq!(midi.usage(), before);
        input.clear();
        midi.append(&mut store.render_io(), Frame(4), 0, &input)
            .unwrap();
        assert_eq!(midi.coverage(), Ok(Frame(0)..Frame(4)));
        let mut observed = MidiState::default();
        observed.apply_packet(&[0xb0, 1, 99], 48).unwrap();
        midi.discontinuity(&mut store.render_io(), Frame(20), &observed)
            .unwrap();
        assert_eq!(midi.coverage(), Ok(Frame(20)..Frame(20)));
        assert_eq!(
            midi.grab(&mut store.render_io(), Frame(3)..Frame(21)),
            Err(Error::InvalidFrame)
        );
        midi.append(&mut store.render_io(), Frame(20), 8, &input)
            .unwrap();
        let after = midi
            .grab(&mut store.render_io(), Frame(21)..Frame(28))
            .unwrap();
        check(&mut store, &mut midi, after, 7, &[], &observed);
        let mut held = MidiState::default();
        held.apply_packet(&[0x90, 60, 80], 48).unwrap();
        check(&mut store, &mut midi, take, 3, &[], &held);
        midi.release_take(take).unwrap();
        midi.release_take(after).unwrap();
        cleanup(&mut store, &mut midi);
        cleanup(&mut foreign, &mut other);
    });
}

#[shoop_test]
fn prepared_memory_reports_state_separately_from_packets() {
    let (_store, midi, _input) = setup(8, 64);
    let capacity = midi.capacity();
    assert_eq!(
        capacity.state_bytes,
        (capacity.chunks + 15) * size_of::<MidiState>()
    );
    assert!(capacity.metadata_bytes > capacity.state_bytes);
    #[cfg(not(target_arch = "wasm32"))]
    {
        extern crate std;
        std::println!(
            "MIDI capacity: {capacity:?}; projection={} bytes; payload reservation=64000 bytes",
            size_of::<MidiState>()
        );
    }
}

#[shoop_test]
fn preparation_sizes_checkpoints_by_admitted_owners_and_keeps_live_append_reserved() {
    let runtime = RuntimeConfig {
        page_bytes: 16,
        resource_pages: 4096,
        budgets: Budgets {
            validation_steps: 1,
            ..Budgets::default()
        },
        ..RuntimeConfig::default()
    }
    .prepare()
    .unwrap();
    let config = Config {
        history_frames: 64,
        chunks: 8,
        views: 128,
        // 192 history pages, 49 append workspace pages, four retained chunks.
        pages: 337,
        max_events: 32,
        max_bytes: 256,
        max_packet: 48,
        transactions: 2,
        edit_steps: 3,
    };
    let estimate = assert_no_heap(|| config.capacity(runtime).unwrap());
    assert_eq!(estimate.owners, 4);
    let fewer_handles = Config { views: 8, ..config }.capacity(runtime).unwrap();
    assert_eq!(estimate.chunks, fewer_handles.chunks);
    assert_eq!(estimate.index_nodes, fewer_handles.index_nodes);
    assert!(estimate.state_bytes < 256 * size_of::<MidiState>());

    let mut store = ResourceStore::new(runtime, crate::Quantum::new(1).unwrap()).unwrap();
    let mut midi = MidiStore::prepare(&mut store, config, Frame(0), &MidiState::default()).unwrap();
    assert_eq!(midi.capacity(), estimate);
    let mut input = MidiBuffer::new(32, 256).unwrap();
    input.push(0, 8, &[0x90, 60, 90]).unwrap();
    input.push(4, 8, &[0x80, 60, 0]).unwrap();
    assert_no_heap(|| {
        store.begin_callback();
        midi.append(&mut store.render_io(), Frame(0), 8, &input)
            .unwrap();
        let take = midi
            .grab(&mut store.render_io(), Frame(0)..Frame(8))
            .unwrap();
        let old = midi.export(take).unwrap();
        let a = midi.fork(take).unwrap();
        let b = midi.fork(take).unwrap();
        assert_eq!(midi.fork(take), Err(Error::Capacity));

        for tick in 1..128 {
            store.begin_callback();
            midi.append(&mut store.render_io(), Frame(tick * 8), 8, &input)
                .unwrap();
        }
        check(
            &mut store,
            &mut midi,
            take,
            8,
            &[(0, &[0x90, 60, 90]), (4, &[0x80, 60, 0])],
            &MidiState::default(),
        );
        midi.release_take(a).unwrap();
        midi.release_take(b).unwrap();
        drain(&mut store, &mut midi);

        input.clear();
        input.push(0, 2, &[0xb0, 7, 42]).unwrap();
        store.begin_callback();
        let edit = midi
            .prepare_replace(
                &mut store.render_io(),
                take,
                Revision(0),
                2,
                2,
                &input,
                &MidiState::default(),
            )
            .unwrap();
        while midi.progress_edit(&mut store.render_io(), edit, 3).unwrap() == EditStatus::Building {
            store.begin_callback();
        }
        midi.commit_edit(&mut store.render_io(), edit).unwrap();
        check(
            &mut store,
            &mut midi,
            take,
            8,
            &[
                (0, &[0x90, 60, 90]),
                (2, &[0xb0, 7, 42]),
                (4, &[0x80, 60, 0]),
            ],
            &MidiState::default(),
        );
        let mut cursor = midi.cursor(midi.export_index(old).unwrap()).unwrap();
        let mut packet = [0; 48];
        for (frame, data) in [(0, [0x90, 60, 90]), (4, [0x80, 60, 0])] {
            let event = midi.next(&mut cursor).unwrap().unwrap();
            assert_eq!(event.frame(), frame);
            assert_eq!(
                midi.read_event(&mut store.render_io(), event, &mut packet),
                Ok(3)
            );
            assert_eq!(&packet[..3], &data);
        }
        assert_eq!(midi.next(&mut cursor), Ok(None));
        midi.release_take(take).unwrap();
        midi.release_export(old).unwrap();
        cleanup(&mut store, &mut midi);
    });
}

#[shoop_test]
fn preparation_estimates_refuse_invalid_and_oversized_shapes_without_allocating() {
    let runtime = RuntimeConfig::default().prepare().unwrap();
    let config = Config {
        history_frames: 48_000 * 30,
        chunks: 8,
        views: 128,
        pages: 256,
        max_events: 32,
        max_bytes: 256,
        max_packet: 48,
        transactions: 2,
        edit_steps: 3,
    };
    assert_no_heap(|| {
        assert!(config.capacity(runtime).is_ok());
        assert_eq!(
            Config {
                chunks: 3,
                ..config
            }
            .capacity(runtime),
            Err(Error::InvalidConfig)
        );
        assert_eq!(
            Config { pages: 1, ..config }.capacity(runtime),
            Err(Error::Capacity)
        );
        assert_eq!(
            Config {
                views: usize::MAX,
                ..config
            }
            .capacity(runtime),
            Err(Error::Capacity)
        );
        assert_eq!(
            Config {
                transactions: usize::MAX,
                ..config
            }
            .capacity(runtime),
            Err(Error::Capacity)
        );
    });
}

#[shoop_test]
fn clipped_packet_owners_grow_and_shrink_edit_credits_without_mutating_old_exports() {
    let (mut store, mut midi, mut input) = setup(8, 64);
    for frame in 0..20 {
        input.push(frame, 32, &[0xb0, 7, frame as u8]).unwrap();
    }
    let mut initial = MidiState::default();
    initial.apply_packet(&[0xb0, 7, 4], 48).unwrap();
    let mut events: Vec<(u64, [u8; 3])> = vec![(0, [0xb0, 7, 5]), (1, [0xb0, 7, 6])];
    events.extend((32..48).map(|value| (2, [0xb0, 7, value])));
    events.extend([(3, [0xb0, 7, 8]), (4, [0xb0, 7, 9])]);
    let expected: Vec<_> = events
        .iter()
        .map(|(frame, data)| (*frame, &data[..]))
        .collect();
    assert_no_heap(|| {
        store.begin_callback();
        midi.append(&mut store.render_io(), Frame(0), 32, &input)
            .unwrap();
        let take = midi
            .grab(&mut store.render_io(), Frame(5)..Frame(10))
            .unwrap();
        let view = midi.view(take.0).unwrap();
        assert_eq!(view.credit, 1);
        for serial in 0..4 {
            assert_eq!(midi.find(view.root, serial).is_ok(), serial == 1);
        }
        let old = midi.export(take).unwrap();
        assert_eq!(midi.usage().retained_chunks, 2);
        input.clear();
        for value in 32..48 {
            input.push(0, 1, &[0xb0, 7, value]).unwrap();
        }
        let edit = midi
            .prepare_replace(
                &mut store.render_io(),
                take,
                Revision(0),
                2,
                1,
                &input,
                &MidiState::default(),
            )
            .unwrap();
        while midi.progress_edit(&mut store.render_io(), edit, 3).unwrap() == EditStatus::Building {
            store.begin_callback();
        }
        midi.commit_edit(&mut store.render_io(), edit).unwrap();
        drain(&mut store, &mut midi);
        assert_eq!(midi.usage().retained_chunks, 4);
        assert_eq!(midi.usage().owners, 2);
        check(&mut store, &mut midi, take, 5, &expected, &initial);

        input.clear();
        store.begin_callback();
        let edit = midi
            .prepare_replace(
                &mut store.render_io(),
                take,
                Revision(1),
                0,
                5,
                &input,
                &MidiState::default(),
            )
            .unwrap();
        while midi.progress_edit(&mut store.render_io(), edit, 3).unwrap() == EditStatus::Building {
            store.begin_callback();
        }
        midi.commit_edit(&mut store.render_io(), edit).unwrap();
        drain(&mut store, &mut midi);
        assert_eq!(midi.usage().retained_chunks, 2);
        assert_eq!(midi.usage().owners, 2);
        check(&mut store, &mut midi, take, 5, &[], &MidiState::default());
        let index = midi.export_index(old).unwrap();
        assert_eq!(midi.initial_state(index).unwrap(), &initial);
        let mut cursor = midi.cursor(index).unwrap();
        let mut packet = [0; 48];
        for frame in 0..5 {
            let event = midi.next(&mut cursor).unwrap().unwrap();
            assert_eq!(event.frame(), frame);
            assert_eq!(
                midi.read_event(&mut store.render_io(), event, &mut packet),
                Ok(3)
            );
            assert_eq!(&packet[..3], &[0xb0, 7, 5 + frame as u8]);
        }
        assert_eq!(midi.next(&mut cursor), Ok(None));
        midi.release_take(take).unwrap();
        midi.release_export(old).unwrap();
        cleanup(&mut store, &mut midi);
    });
}

#[shoop_test]
fn private_splice_rebuilds_a_long_suffix_in_reserved_bounded_steps() {
    let (mut store, mut midi, mut input) = setup(16, 256);
    let packets: Vec<(u64, [u8; 3])> = (0..60)
        .map(|i| {
            (
                i / 2,
                match i {
                    0 => [0x90, 60, 80],
                    2 => [0xb0, 1, 10],
                    16 => [0x80, 60, 0],
                    _ => [0xb1, 2, i as u8],
                },
            )
        })
        .collect();
    let originals: Vec<(u64, &[u8])> = packets.iter().map(|(at, data)| (*at, &data[..])).collect();
    let inserted = [0xb0, 1, 99];
    let mut expected: Vec<(u64, &[u8])> = originals
        .iter()
        .copied()
        .filter(|(at, _)| !((1..10).contains(at)))
        .collect();
    expected.push((1, &inserted));
    expected.sort_by_key(|e| e.0);
    assert_no_heap(|| {
        for start in [0, 16] {
            input.clear();
            for &(at, data) in originals
                .iter()
                .filter(|(at, _)| *at >= start && *at < start + 16)
            {
                input.push((at - start) as u32, 16, data).unwrap();
            }
            midi.append(&mut store.render_io(), Frame(start), 16, &input)
                .unwrap();
        }
        let take = midi
            .grab(&mut store.render_io(), Frame(0)..Frame(32))
            .unwrap();
        let sibling = midi.fork(take).unwrap();
        let export = midi.export(take).unwrap();
        let mut stale = midi.cursor(midi.take_index(take).unwrap()).unwrap();
        input.clear();
        input.push(0, 9, &inserted).unwrap();
        let edit = midi
            .prepare_replace(
                &mut store.render_io(),
                take,
                Revision(0),
                1,
                9,
                &input,
                &MidiState::default(),
            )
            .unwrap();
        input.clear();
        assert_eq!(
            midi.commit_edit(&mut store.render_io(), edit),
            Err(Error::Busy)
        );
        assert_eq!(
            midi.progress_edit(&mut store.render_io(), edit, 0),
            Ok(EditStatus::Building)
        );
        let mut steps = 0;
        while midi.edit_status(edit) == Ok(EditStatus::Building) {
            store.begin_callback();
            store.charge_external(16, 1).unwrap();
            let before = midi.work();
            let pages = store.usage().free_pages;
            let status = midi.progress_edit(&mut store.render_io(), edit, 3).unwrap();
            assert_eq!(store.usage().free_pages, pages);
            let work = midi.work();
            assert!(work.records - before.records <= 3);
            if status == EditStatus::Ready {
                assert_eq!(work.records, before.records);
                assert_eq!(work.state_copies, before.state_copies);
                assert!(work.index_steps - before.index_steps <= 8 * (midi.depth + 1));
                assert!(work.reclaimed - before.reclaimed <= 4 * (midi.depth + 1));
            } else {
                assert!(work.index_steps - before.index_steps <= 2 * (midi.depth + 1));
            }
            check(
                &mut store,
                &mut midi,
                take,
                32,
                &originals,
                &MidiState::default(),
            );
            input.clear();
            input.push(0, 1, &[0xb2, 7, steps as u8]).unwrap();
            midi.append(&mut store.render_io(), Frame(32 + steps), 1, &input)
                .unwrap();
            steps += 1;
            assert!(steps < 100);
        }
        assert!(steps > 10);
        let pages = store.usage().free_pages;
        assert_eq!(
            midi.commit_edit(&mut store.render_io(), edit),
            Ok(Revision(1))
        );
        assert_eq!(store.usage().free_pages, pages);
        assert_eq!(midi.next(&mut stale), Err(Error::StaleRevision));
        check(
            &mut store,
            &mut midi,
            take,
            32,
            &expected,
            &MidiState::default(),
        );
        check(
            &mut store,
            &mut midi,
            sibling,
            32,
            &originals,
            &MidiState::default(),
        );
        let position = midi
            .seek(&mut store.render_io(), midi.take_index(take).unwrap(), 31)
            .unwrap();
        assert_eq!(position.state.channel(0).unwrap().controller(1), Some(99));
        assert!(position.state.channel(0).unwrap().note(60).unwrap().held());
        midi.release_take(take).unwrap();
        midi.release_take(sibling).unwrap();
        midi.release_export(export).unwrap();
        cleanup(&mut store, &mut midi);
    });
}

#[shoop_test]
fn cancellation_and_stale_ready_commit_preserve_the_complete_old_revision() {
    let (mut store, mut midi, mut input) = setup(4, 64);
    assert_no_heap(|| {
        input.push(0, 8, &[0x90, 60, 80]).unwrap();
        input.push(4, 8, &[0x80, 60, 0]).unwrap();
        midi.append(&mut store.render_io(), Frame(0), 8, &input)
            .unwrap();
        let take = midi
            .grab(&mut store.render_io(), Frame(0)..Frame(8))
            .unwrap();
        input.clear();
        input.push(0, 1, &[0xb0, 1, 20]).unwrap();
        let a = midi
            .prepare_replace(
                &mut store.render_io(),
                take,
                Revision(0),
                2,
                1,
                &input,
                &MidiState::default(),
            )
            .unwrap();
        let b = midi
            .prepare_replace(
                &mut store.render_io(),
                take,
                Revision(0),
                3,
                1,
                &input,
                &MidiState::default(),
            )
            .unwrap();
        assert_eq!(
            midi.prepare_replace(
                &mut store.render_io(),
                take,
                Revision(0),
                3,
                1,
                &input,
                &MidiState::default()
            ),
            Err(Error::Capacity)
        );
        for edit in [a, b] {
            while midi.progress_edit(&mut store.render_io(), edit, 1) == Ok(EditStatus::Building) {}
        }
        midi.commit_edit(&mut store.render_io(), a).unwrap();
        assert_eq!(
            midi.commit_edit(&mut store.render_io(), b),
            Err(Error::StaleRevision)
        );
        midi.cancel_edit(b).unwrap();
        assert_eq!(midi.edit_status(b), Err(Error::InvalidId));
        drain(&mut store, &mut midi);
        let c = midi
            .prepare_replace(
                &mut store.render_io(),
                take,
                Revision(1),
                0,
                1,
                &input,
                &MidiState::default(),
            )
            .unwrap();
        midi.progress_edit(&mut store.render_io(), c, 1).unwrap();
        midi.cancel_edit(c).unwrap();
        check(
            &mut store,
            &mut midi,
            take,
            8,
            &[
                (0, &[0x90, 60, 80]),
                (2, &[0xb0, 1, 20]),
                (4, &[0x80, 60, 0]),
            ],
            &MidiState::default(),
        );
        midi.release_take(take).unwrap();
        cleanup(&mut store, &mut midi);
    });
}

fn check_owned(
    store: &mut ResourceStore,
    midi: &mut MidiStore,
    take: TakeId,
    events: &[(u64, [u8; 3])],
    initial: &MidiState,
) {
    let mut borrowed = [(0, &[][..]); 128];
    for (out, (frame, packet)) in borrowed.iter_mut().zip(events) {
        *out = (*frame, &packet[..]);
    }
    check(store, midi, take, 32, &borrowed[..events.len()], initial);
}

#[shoop_test]
fn repeated_wrapping_edits_match_an_independent_raw_splice_oracle() {
    let (mut store, mut midi, mut input) = setup(16, 128);
    let mut oracle: Vec<(u64, [u8; 3])> = Vec::with_capacity(128);
    for event in 0..16 {
        let packet = if event % 2 == 0 {
            [0x90, 60 + event as u8 / 2, 80]
        } else {
            [0x80, 60 + event as u8 / 2, 0]
        };
        oracle.push((event * 2, packet));
        input.push(event as u32 * 2, 32, &packet).unwrap();
    }
    let original = oracle.clone();
    let mut intermediate = Vec::with_capacity(128);
    let mut initial = MidiState::default();
    let mut observed = MidiState::default();
    let mut seed = 0x12345678u32;
    assert_no_heap(|| {
        midi.append(&mut store.render_io(), Frame(0), 32, &input)
            .unwrap();
        let take = midi
            .grab(&mut store.render_io(), Frame(0)..Frame(32))
            .unwrap();
        let sibling = midi.fork(take).unwrap();
        let export = midi.export(take).unwrap();
        let mut middle = None;
        let mut middle_initial = MidiState::default();
        let mut revision = Revision(0);
        for pass in 0..128 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let position = u64::from(seed % 32);
            let frames = if pass % 17 == 0 {
                0
            } else {
                1 + seed as usize / 32 % 32
            };
            input.clear();
            for offset in 0..frames {
                if (offset + pass) % 3 == 0 {
                    let packet = match (offset + pass) % 7 {
                        0 => [0xb0, 64, 127],
                        1 => [0xb0, 64, 0],
                        2 => [0x90, 60, 91],
                        3 => [0x80, 60, 0],
                        4 => [0xb0, 121, 0],
                        _ => [0xb0, 1, pass as u8],
                    };
                    input.push(offset as u32, frames, &packet).unwrap();
                }
            }
            let end = position + frames as u64;
            if frames > 0 {
                oracle.retain(|(at, _)| {
                    if end > 32 {
                        !(*at >= position || *at < end - 32)
                    } else {
                        !(*at >= position && *at < end)
                    }
                });
                if position == 0 || end > 32 {
                    initial = observed.basic_projection();
                    let zero = if position == 0 { 0 } else { 32 - position };
                    for event in 0..input.len() {
                        let (offset, packet) = input.get(event).unwrap();
                        if u64::from(offset) < zero {
                            initial.apply_packet(packet, 48).unwrap();
                            initial = initial.basic_projection();
                        }
                    }
                }
                for event in 0..input.len() {
                    let (offset, packet) = input.get(event).unwrap();
                    let at = (position + u64::from(offset)) % 32;
                    let index = oracle
                        .iter()
                        .position(|(frame, _)| *frame > at)
                        .unwrap_or(oracle.len());
                    oracle.insert(index, (at, packet.try_into().unwrap()));
                }
            }
            store.begin_callback();
            store.charge_external(16, 1).unwrap();
            let edit = midi
                .prepare_replace(
                    &mut store.render_io(),
                    take,
                    revision,
                    position,
                    frames,
                    &input,
                    &observed.basic_projection(),
                )
                .unwrap();
            for event in 0..input.len() {
                observed
                    .apply_packet(input.get(event).unwrap().1, 48)
                    .unwrap();
                observed = observed.basic_projection();
            }
            while midi.progress_edit(&mut store.render_io(), edit, 3) == Ok(EditStatus::Building) {}
            if frames > 0 {
                revision = revision.checked_next().unwrap();
            }
            assert_eq!(midi.commit_edit(&mut store.render_io(), edit), Ok(revision));
            check_owned(&mut store, &mut midi, take, &oracle, &initial);
            check_owned(
                &mut store,
                &mut midi,
                sibling,
                &original,
                &MidiState::default(),
            );
            if pass == 32 {
                middle = Some(midi.export(take).unwrap());
                intermediate.extend_from_slice(&oracle);
                middle_initial = initial;
            }
            if let Some(export) = middle {
                check_owned(
                    &mut store,
                    &mut midi,
                    TakeId(export.0),
                    &intermediate,
                    &middle_initial,
                );
            }
            drain(&mut store, &mut midi);
        }
        midi.release_take(take).unwrap();
        midi.release_take(sibling).unwrap();
        midi.release_export(export).unwrap();
        midi.release_export(middle.unwrap()).unwrap();
        cleanup(&mut store, &mut midi);
    });
}

#[shoop_test]
fn edit_admission_refuses_event_packet_byte_index_and_owner_exhaustion() {
    let (mut store, mut midi, mut input) = setup(1, 64);
    let mut large = MidiBuffer::new(64, 1024).unwrap();
    let mut packet = [1; 49];
    packet[0] = 0xf0;
    packet[48] = 0xf7;
    assert_no_heap(|| {
        midi.append(&mut store.render_io(), Frame(0), 32, &input)
            .unwrap();
        let take = midi
            .grab(&mut store.render_io(), Frame(0)..Frame(32))
            .unwrap();
        assert_eq!(large.push(0, 32, &packet), Err(Error::Capacity));
        let before = midi.usage();
        large.clear();
        for _ in 0..33 {
            large.push(0, 32, &[0xff]).unwrap();
        }
        assert_eq!(
            midi.prepare_replace(
                &mut store.render_io(),
                take,
                Revision(0),
                0,
                32,
                &large,
                &MidiState::default()
            ),
            Err(Error::Capacity)
        );
        large.clear();
        for _ in 0..9 {
            large.push(0, 32, &[0x90, 60, 80]).unwrap();
        }
        assert_eq!(
            midi.prepare_replace(
                &mut store.render_io(),
                take,
                Revision(0),
                0,
                32,
                &large,
                &MidiState::default()
            ),
            Err(Error::Capacity)
        );
        large.clear();
        midi.config.max_bytes = 16;
        for _ in 0..6 {
            large.push(0, 32, &[0x90, 60, 80]).unwrap();
        }
        assert_eq!(
            midi.prepare_replace(
                &mut store.render_io(),
                take,
                Revision(0),
                0,
                32,
                &large,
                &MidiState::default()
            ),
            Err(Error::Capacity)
        );
        assert_eq!(
            midi.prepare_replace(
                &mut store.render_io(),
                take,
                Revision(1),
                0,
                1,
                &input,
                &MidiState::default()
            ),
            Err(Error::StaleRevision)
        );
        assert_eq!(midi.usage(), before);
        check_owned(&mut store, &mut midi, take, &[], &MidiState::default());
        midi.config.max_bytes = 256;
        let mut owners = [None; 7];
        for owner in &mut owners {
            *owner = Some(midi.fork(take).unwrap());
        }
        assert_eq!(midi.fork(take), Err(Error::Capacity));
        assert_eq!(
            midi.prepare_replace(
                &mut store.render_io(),
                take,
                Revision(0),
                0,
                1,
                &input,
                &MidiState::default()
            ),
            Err(Error::Capacity)
        );
        for owner in owners.into_iter().flatten() {
            midi.release_take(owner).unwrap();
        }
        midi.release_take(take).unwrap();
        input.clear();
        cleanup(&mut store, &mut midi);
    });
}

#[shoop_test]
fn basic_history_checkpoints_exclude_parameters_across_reset_boundaries() {
    let (mut store, mut midi, mut input) = setup(8, 64);
    let packets: &[(u64, &[u8])] = &[
        (0, &[0xb0, 7, 1]),
        (3, &[0xff]),
        (5, &[0xbf, 1, 0]),
        (6, &[0xbf, 123, 0]),
        (8, &[0xff]),
        (9, &[0xb1, 11, 2]),
    ];
    for start in [0, 5] {
        input.clear();
        for &(frame, packet) in packets
            .iter()
            .filter(|(frame, _)| *frame >= start && *frame < start + 5)
        {
            input.push((frame - start) as u32, 5, packet).unwrap();
        }
        assert_no_heap(|| midi.append(&mut store.render_io(), Frame(start), 5, &input)).unwrap();
    }
    let take = assert_no_heap(|| midi.grab(&mut store.render_io(), Frame(1)..Frame(10))).unwrap();
    let mut initial = MidiState::default();
    initial.apply_packet(packets[0].1, 48).unwrap();
    let shifted: Vec<_> = packets[1..]
        .iter()
        .map(|(frame, packet)| (frame - 1, *packet))
        .collect();
    assert_no_heap(|| check(&mut store, &mut midi, take, 9, &shifted, &initial));
    let index = midi.take_index(take).unwrap();
    let copies = midi.work().state_copies;
    for frame in 0..=9 {
        assert_no_heap(|| midi.parameter_history_untracked(&mut store.render_io(), index, frame))
            .unwrap();
    }
    assert_eq!(midi.work().state_copies, copies);
    assert_eq!(
        midi.parameter_history_untracked(&mut store.render_io(), index, 10),
        Err(Error::InvalidFrame)
    );
    midi.release_take(take).unwrap();
    cleanup(&mut store, &mut midi);
}
