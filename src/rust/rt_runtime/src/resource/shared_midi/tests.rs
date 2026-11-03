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
    assert_eq!(midi.initial_state(index).unwrap(), initial);
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
        assert_eq!(position.state, expected, "at {frame}");
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
    let mut sysex = [7; 35];
    sysex[0] = 0xf0;
    sysex[34] = 0xf7;
    let original = [
        (0, &[0x90, 60, 90][..]),
        (3, &[0xb0, 64, 127][..]),
        (3, &[0x80, 60, 0][..]),
        (5, &sysex[..]),
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
                (2, &sysex),
                (5, original[4].1),
            ],
            &at3,
        );
        let mut at4 = at3;
        at4.apply_packet(original[1].1, 48).unwrap();
        at4.apply_packet(original[2].1, 48).unwrap();
        check(&mut store, &mut midi, b, 4, &[(1, &sysex)], &at4);
        input.clear();
        midi.append(&mut store.render_io(), Frame(12), 100, &input)
            .unwrap();
        let empty = midi
            .grab(&mut store.render_io(), Frame(90)..Frame(100))
            .unwrap();
        let mut final_state = at4;
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
        (capacity.chunks + 9) * size_of::<MidiState>()
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
