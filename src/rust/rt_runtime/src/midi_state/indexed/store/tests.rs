use super::*;
use crate::{
    midi::Message,
    midi_state::{indexed, MidiState},
    Frame,
};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

#[derive(Clone, Copy)]
struct Record {
    point: Point,
    event: Option<Event>,
    bytes: [u8; 8],
    len: usize,
}
struct Records {
    rows: Vec<Record>,
    references: Vec<usize>,
}
impl Lookup<NonZeroU32> for Records {
    fn point(&self, value: NonZeroU32) -> Point {
        self.rows[slot(value)].point
    }
    fn event(&self, value: NonZeroU32) -> Option<Event> {
        self.rows[slot(value)].event
    }
}
impl Values<NonZeroU32> for Records {
    fn retain(&mut self, value: NonZeroU32) {
        self.references[slot(value)] += 1;
    }
    fn release(&mut self, value: NonZeroU32) {
        let count = &mut self.references[slot(value)];
        assert!(*count > 0);
        *count -= 1;
    }
}
fn id(index: usize) -> NonZeroU32 {
    NonZeroU32::new(index as u32 + 1).unwrap()
}
fn config() -> Config {
    Config {
        leases: 32,
        versions: 16,
        category_pages: 512,
        packet_nodes: 8192,
        scalar_nodes: 8192,
        hold_nodes: 2048,
        sostenuto_nodes: 2048,
    }
}
fn record(point: Point, packet: &[u8]) -> Record {
    let mut bytes = [0; 8];
    bytes[..packet.len()].copy_from_slice(packet);
    Record {
        point,
        bytes,
        len: packet.len(),
        event: Event::from_message(point, Message::parse(packet, 8).unwrap()),
    }
}
fn same(actual: &MidiState, expected: &MidiState) {
    assert_eq!(actual.system_reset_seen(), expected.system_reset_seen());
    for ch in 0..16 {
        assert_eq!(actual.channel(ch), expected.channel(ch), "channel {ch}");
    }
}
fn check(
    store: &Store<NonZeroU32>,
    version: Version,
    records: &Records,
    expected: &[Option<usize>; 128],
    initial: &MidiState,
) {
    let end = Cut::frame(Frame(u64::MAX));
    let mut after = Cut::frame(Frame(0));
    let mut count = 0;
    for index in expected.iter().flatten() {
        let next = store.first(version, after, end, records).unwrap().unwrap();
        assert_eq!(next, id(*index));
        after = Cut::at(records.point(next));
        count += 1;
    }
    assert_eq!(store.first(version, after, end, records).unwrap(), None);
    assert_eq!(store.len(version).unwrap(), count);
    for frame in [0, 1, 8, 16, 32, 33] {
        let cut = Cut::frame(Frame(u64::from(u32::MAX) + frame));
        let mut state = *initial;
        for &index in expected.iter().flatten() {
            let row = &records.rows[index];
            if cut.includes(row.point) {
                state.apply_packet(&row.bytes[..row.len], 8).unwrap();
            }
        }
        same(
            &indexed::state(&store.timeline(version, records).unwrap(), initial, cut),
            &state,
        );
    }
}
fn recovered(store: &mut Store<NonZeroU32>, records: &mut Records) {
    while store.maintain(1, records) > 0 {}
    assert_eq!(store.leases, 0);
    assert_eq!(store.free_tables.len(), store.config.versions);
    assert_eq!(store.free_pages.len(), store.config.category_pages);
    assert_eq!(store.packets.free_nodes(), store.config.packet_nodes);
    assert_eq!(store.scalars.free_nodes(), store.config.scalar_nodes);
    assert_eq!(store.holds.free_nodes(), store.config.hold_nodes);
    assert_eq!(store.sostenuto.free_nodes(), store.config.sostenuto_nodes);
    assert!(records.references.iter().all(|&count| count == 0));
}

#[shoop_test]
fn shared_versions_preserve_complete_projection_and_original_order_through_repeated_edits() {
    let packets: &[&[u8]] = &[
        &[0x90, 60, 90],
        &[0x80, 60, 1],
        &[0xb0, 64, 127],
        &[0xb0, 69, 127],
        &[0xb0, 66, 127],
        &[0xb0, 66, 0],
        &[0xb0, 121, 0],
        &[0xb0, 120, 0],
        &[0xb0, 64, 0],
        &[0xb0, 69, 0],
        &[0xa0, 60, 127],
        &[0xb0, 0, 2],
        &[0xc0, 7],
        &[0xb0, 32, 3],
        &[0xb0, 98, 4],
        &[0xb0, 101, 6],
        &[0xb0, 6, 8],
        &[0xb0, 96, 1],
        &[0xd0, 77],
        &[0xe0, 3, 4],
        &[0xb0, 124, 1],
        &[0xb0, 125, 0],
        &[0xb0, 126, 2],
        &[0xb0, 127, 0],
        &[0xff],
        &[0xf0, 1, 2, 3, 4, 5, 6, 0xf7],
        &[0xf8],
    ];
    let rows: Vec<_> = (0..256)
        .map(|index| {
            let mut packet = [0; 8];
            let original = packets[(index * 11 + index / 128) % packets.len()];
            packet[..original.len()].copy_from_slice(original);
            if packet[0] < 0xf0 {
                packet[0] |= ((index / 8) % 16) as u8;
            }
            record(
                Point {
                    frame: Frame(u64::from(u32::MAX) + (index % 128 / 4) as u64),
                    order: (index % 128) as u64,
                },
                &packet[..original.len()],
            )
        })
        .collect();
    let mut records = Records {
        rows,
        references: vec![0; 256],
    };
    let mut store = Store::prepare(config()).unwrap();
    let mut root = store.empty().unwrap();
    let mut snapshots = [None; 8];
    let mut expected = [None; 128];
    let mut saved = [[None; 128]; 8];
    let mut initial = MidiState::default();
    for ch in 0..16 {
        for bytes in [
            [0x90 | ch, 60, 80],
            [0xb0 | ch, 66, 127],
            [0x80 | ch, 60, 0],
            [0xb0 | ch, 64, 127],
        ] {
            initial.apply_packet(&bytes, 8).unwrap();
        }
    }
    assert_no_heap(|| {
        let mut seed = 0x3456_789a_bced_1223u64;
        for step in 0..384 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let key = ((seed >> 32) & 127) as usize;
            if let Some(old) = expected[key] {
                root = store
                    .remove(root, records.rows[old].point, &mut records)
                    .unwrap();
                expected[key] = None;
            }
            if seed & 3 != 0 {
                let value = key + (((seed >> 44) & 1) * 128) as usize;
                root = store.insert(root, id(value), &mut records).unwrap();
                expected[key] = Some(value);
            }
            if step % 24 == 0 {
                let snapshot = step / 24 % snapshots.len();
                if let Some(old) = snapshots[snapshot] {
                    store.release(old).unwrap();
                }
                snapshots[snapshot] = Some(store.retain(root).unwrap());
                saved[snapshot] = expected;
                check(&store, root, &records, &expected, &initial);
                for (version, expected) in snapshots.iter().zip(&saved) {
                    if let Some(version) = version {
                        check(&store, *version, &records, expected, &initial);
                    }
                }
            }
            assert!(store.maintain(16, &mut records) <= 16);
        }
        store.release(root).unwrap();
        for version in snapshots.into_iter().flatten() {
            store.release(version).unwrap();
        }
        recovered(&mut store, &mut records);
    });
}

#[shoop_test]
fn every_prepared_index_limit_refuses_before_any_category_or_version_changes() {
    let rows = vec![
        record(
            Point {
                frame: Frame(0),
                order: 0,
            },
            &[0xb0, 121, 0],
        ),
        record(
            Point {
                frame: Frame(1),
                order: 0,
            },
            &[0xb0, 121, 0],
        ),
    ];
    for limit in 0..6 {
        let mut config = Config {
            leases: 4,
            versions: 2,
            category_pages: 4,
            packet_nodes: 64,
            scalar_nodes: 64,
            hold_nodes: 64,
            sostenuto_nodes: 64,
        };
        match limit {
            0 => config.versions = 1,
            1 => config.category_pages = 1,
            2 => config.packet_nodes = 1,
            3 => config.scalar_nodes = 1,
            4 => config.hold_nodes = 1,
            _ => config.sostenuto_nodes = 1,
        }
        let mut store = Store::prepare(config).unwrap();
        let mut records = Records {
            rows: rows.clone(),
            references: vec![0; 2],
        };
        let mut root = store.empty().unwrap();
        assert_no_heap(|| {
            root = store.insert(root, id(0), &mut records).unwrap();
            let old = store.retain(root).unwrap();
            let references = [records.references[0], records.references[1]];
            store.reset_work();
            assert_eq!(
                store.insert(root, id(1), &mut records),
                Err(Error::Capacity)
            );
            assert_eq!(store.len(root), Ok(1));
            assert_eq!(store.len(old), Ok(1));
            assert_eq!(records.references.as_slice(), &references);
            assert_eq!(store.work().0, Work::default());
            for work in store.work().1 {
                assert_eq!(work.copied, 0);
                assert_eq!(work.reclaimed, 0);
            }
            assert_eq!(
                store.insert(root, id(0), &mut records),
                Err(Error::InvalidEvent)
            );
            store.release(root).unwrap();
            store.release(old).unwrap();
            assert!(store.len(root).is_err());
            recovered(&mut store, &mut records);
            let fresh = store.empty().unwrap();
            assert_ne!(fresh, root);
            store.release(fresh).unwrap();
            recovered(&mut store, &mut records);
        });
    }
}

#[shoop_test]
fn version_leases_are_scoped_and_constant_work_while_category_copy_work_is_bounded() {
    let mut store = Store::<NonZeroU32>::prepare(config()).unwrap();
    let mut other = Store::<NonZeroU32>::prepare(config()).unwrap();
    let mut records = Records {
        rows: vec![
            record(
                Point {
                    frame: Frame(0),
                    order: 0,
                },
                &[0xb0, 121, 0],
            ),
            record(
                Point {
                    frame: Frame(1),
                    order: 0,
                },
                &[0xb0, 121, 0],
            ),
        ],
        references: vec![0; 2],
    };
    let mut version = store.empty().unwrap();
    let foreign = other.empty().unwrap();
    assert_no_heap(|| {
        version = store.insert(version, id(0), &mut records).unwrap();
        store.reset_work();
        assert_eq!(store.retain(foreign), Err(Error::InvalidId));
        let old = store.retain(version).unwrap();
        assert_eq!(store.work().0, Work::default());
        version = store.insert(version, id(1), &mut records).unwrap();
        assert_eq!(store.work().0.table_copies, 1);
        assert_eq!(store.work().0.category_copies, 1);
        assert!(store.work().0.root_retains <= 1 + LEAF);
        assert_eq!(store.len(old), Ok(1));
        assert_eq!(store.len(version), Ok(2));
        store.release(version).unwrap();
        store.release(old).unwrap();
        other.release(foreign).unwrap();
        recovered(&mut store, &mut records);
        recovered(&mut other, &mut records);
    });
}
