use super::*;
use crate::{
    midi_state::{
        indexed::{self, store, Cut},
        MidiState,
    },
    Frame,
};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

fn resources() -> ResourceStore {
    ResourceStore::new(
        crate::Config {
            page_bytes: 64,
            resource_pages: 64,
            ..crate::Config::default()
        }
        .prepare()
        .unwrap(),
        crate::Quantum::new(1).unwrap(),
    )
    .unwrap()
}
fn config() -> Config {
    Config {
        pages: 16,
        bodies: 1024,
        records: 64,
        max_packet: 512,
        bases: 4,
    }
}
fn cleanup(resources: &mut ResourceStore, packets: &mut Packets) {
    packets.begin_cleanup(&mut resources.render_io()).unwrap();
    while packets.maintain(&mut resources.render_io(), 1).unwrap() > 0 {}
    packets.finish_cleanup(resources).unwrap();
    assert_eq!(resources.usage().free_pages, 64);
    assert_eq!(resources.usage().reserved_pages, 0);
    assert_eq!(packets.free_slabs.len(), packets.config.pages);
    assert_eq!(packets.free_records.len(), packets.config.records);
    assert_eq!(packets.free_bodies.len(), packets.config.bodies);
}

#[shoop_test]
fn short_packets_pack_across_callbacks_without_changing_retained_ranges() {
    let mut resources = resources();
    let mut packets = Packets::prepare(&mut resources, config()).unwrap();
    let mut bodies = Vec::with_capacity(1000);
    assert_no_heap(|| {
        for index in 0usize..1000 {
            resources.begin_callback();
            bodies.push(packets.append(&mut resources.render_io(), &[0xf8]).unwrap());
            assert_eq!(packets.available_pages(), 16 - (index + 1).div_ceil(64));
        }
        assert_eq!(packets.available_pages(), 0);
        for &body in &bodies {
            let mut bytes = [0; 8];
            assert_eq!(
                packets.read_retained_body(&mut resources.render_io(), body, 0, &mut bytes),
                Ok(1)
            );
            assert_eq!(bytes[0], 0xf8);
            assert_eq!(
                packets.read_retained_body(&mut resources.render_io(), body, 1, &mut bytes),
                Ok(0)
            );
            packets.release_body(body).unwrap();
        }
        cleanup(&mut resources, &mut packets);
    });
}

#[shoop_test]
fn complete_spanning_sysex_remains_immutable_after_later_appends_and_shared_records() {
    let mut resources = resources();
    let mut packets = Packets::prepare(&mut resources, config()).unwrap();
    let mut message = [37; 200];
    message[0] = 0xf0;
    message[199] = 0xf7;
    let mut read = [0; 256];
    assert_no_heap(|| {
        let short = packets
            .append(&mut resources.render_io(), &[0xf0, 1, 2, 3, 0xf7])
            .unwrap();
        let body = packets
            .append(&mut resources.render_io(), &message)
            .unwrap();
        let a = packets
            .make_record(
                body,
                Point {
                    frame: Frame(0),
                    order: 1,
                },
            )
            .unwrap();
        let b = packets
            .make_record(
                body,
                Point {
                    frame: Frame(u64::from(u32::MAX) + 10),
                    order: 2,
                },
            )
            .unwrap();
        let av = packets.value(a).unwrap();
        let bv = packets.value(b).unwrap();
        packets.release_body(body).unwrap();
        let later = packets
            .append(&mut resources.render_io(), &[0xf0, 4, 5, 6, 0xf7])
            .unwrap();
        packets.release_body(short).unwrap();
        for value in [av, bv] {
            assert_eq!(packets.packet_len(value), 200);
            assert_eq!(
                packets.read(&mut resources.render_io(), value, 0, &mut read),
                Ok(200)
            );
            assert_eq!(&read[..200], &message);
            assert_eq!(
                packets.read(&mut resources.render_io(), value, 59, &mut read[..70]),
                Ok(70)
            );
            assert_eq!(&read[..70], &message[59..129]);
        }
        packets.release_record(a).unwrap();
        while packets.maintain(&mut resources.render_io(), 1).unwrap() > 0 {}
        assert_eq!(
            packets.read(&mut resources.render_io(), bv, 0, &mut read),
            Ok(200)
        );
        assert_eq!(&read[..200], &message);
        assert_eq!(packets.value(a), Err(Error::InvalidId));
        packets.release_record(b).unwrap();
        packets.release_body(later).unwrap();
        cleanup(&mut resources, &mut packets);
        assert_eq!(
            packets.read_retained_body(&mut resources.render_io(), body, 0, &mut read),
            Err(Error::InvalidId)
        );
    });
}

#[shoop_test]
fn packet_limits_and_foreign_owners_refuse_before_copy_or_descriptor_mutation() {
    for limit in 0..3 {
        let mut resources = resources();
        let mut config = Config {
            pages: 4,
            bodies: 4,
            records: 2,
            max_packet: 64,
            bases: 4,
        };
        match limit {
            0 => config.pages = 1,
            1 => config.bodies = 1,
            _ => config.records = 1,
        }
        let mut packets = Packets::prepare(&mut resources, config).unwrap();
        let mut foreign = Packets::prepare(&mut resources, config).unwrap();
        let mut message = [7; 64];
        message[0] = 0xf0;
        message[63] = 0xf7;
        assert_no_heap(|| {
            let body = packets
                .append(&mut resources.render_io(), &message)
                .unwrap();
            let row = packets
                .make_record(
                    body,
                    Point {
                        frame: Frame(0),
                        order: 0,
                    },
                )
                .unwrap();
            let before = resources.render_work();
            if limit != 2 {
                assert_eq!(
                    packets.append(&mut resources.render_io(), &[0xf8]),
                    Err(Error::Capacity)
                );
            } else {
                assert_eq!(
                    packets.make_record(
                        body,
                        Point {
                            frame: Frame(1),
                            order: 0
                        }
                    ),
                    Err(Error::Capacity)
                );
            }
            assert_eq!(resources.render_work(), before);
            assert_eq!(
                packets.append(&mut resources.render_io(), &[0x90, 128, 0]),
                Err(Error::InvalidEvent)
            );
            assert_eq!(
                foreign.make_record(
                    body,
                    Point {
                        frame: Frame(1),
                        order: 0
                    }
                ),
                Err(Error::InvalidId)
            );
            assert_eq!(packets.body(body).unwrap().references, 2);
            packets.release_body(body).unwrap();
            packets.release_record(row).unwrap();
            foreign.begin_cleanup(&mut resources.render_io()).unwrap();
            foreign.finish_cleanup(&mut resources).unwrap();
            cleanup(&mut resources, &mut packets);
        });
    }
}

#[shoop_test]
fn ordered_versions_keep_complete_bodies_and_release_them_only_after_the_last_snapshot() {
    let mut resources = resources();
    let mut packets = Packets::prepare(&mut resources, config()).unwrap();
    let mut indexes = store::Store::prepare(store::Config {
        leases: 8,
        versions: 4,
        category_pages: 8,
        packet_nodes: 256,
        scalar_nodes: 256,
        hold_nodes: 64,
        sostenuto_nodes: 64,
    })
    .unwrap();
    let mut root = indexes.empty().unwrap();
    let mut values = [NonZeroU32::new(1).unwrap(); 3];
    let original: &[&[u8]] = &[
        &[0x90, 60, 90],
        &[0xb0, 64, 127],
        &[0xf0, 1, 2, 3, 4, 5, 6, 0xf7],
    ];
    assert_no_heap(|| {
        for (index, &bytes) in original.iter().enumerate() {
            let body = packets.append(&mut resources.render_io(), bytes).unwrap();
            let row = packets
                .make_record(
                    body,
                    Point {
                        frame: Frame(index as u64),
                        order: 0,
                    },
                )
                .unwrap();
            packets.release_body(body).unwrap();
            values[index] = packets.value(row).unwrap();
            root = indexes.insert(root, values[index], &mut packets).unwrap();
            packets.release_record(row).unwrap();
        }
        let old = indexes.retain(root).unwrap();
        root = indexes
            .remove(
                root,
                Point {
                    frame: Frame(0),
                    order: 0,
                },
                &mut packets,
            )
            .unwrap();
        let body = packets
            .append(&mut resources.render_io(), &[0x90, 61, 100])
            .unwrap();
        let row = packets
            .make_record(
                body,
                Point {
                    frame: Frame(0),
                    order: 0,
                },
            )
            .unwrap();
        packets.release_body(body).unwrap();
        root = indexes
            .insert(root, packets.value(row).unwrap(), &mut packets)
            .unwrap();
        packets.release_record(row).unwrap();
        let cut = Cut::frame(Frame(3));
        let initial = MidiState::default();
        let previous = indexed::state(&indexes.timeline(old, &packets).unwrap(), &initial, cut);
        let current = indexed::state(&indexes.timeline(root, &packets).unwrap(), &initial, cut);
        assert_eq!(
            previous.channel(0).unwrap().note(60).unwrap().velocity(),
            90
        );
        assert_eq!(current.channel(0).unwrap().note(60).unwrap().velocity(), 0);
        assert_eq!(
            current.channel(0).unwrap().note(61).unwrap().velocity(),
            100
        );
        for (value, &bytes) in values.into_iter().zip(original) {
            let mut read = [0; 8];
            assert_eq!(
                packets.read(&mut resources.render_io(), value, 0, &mut read),
                Ok(bytes.len())
            );
            assert_eq!(&read[..bytes.len()], bytes);
        }
        indexes.release(root).unwrap();
        while indexes.maintain(1, &mut packets) > 0 {}
        while packets.maintain(&mut resources.render_io(), 1).unwrap() > 0 {}
        assert_eq!(packets.record_count, 3);
        indexes.release(old).unwrap();
        while indexes.maintain(1, &mut packets) > 0 {}
        cleanup(&mut resources, &mut packets);
    });
}
