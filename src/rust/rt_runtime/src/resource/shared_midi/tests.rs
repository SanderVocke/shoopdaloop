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
