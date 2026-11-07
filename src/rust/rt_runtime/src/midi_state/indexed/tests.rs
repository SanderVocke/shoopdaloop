use super::*;
use alloc::vec::Vec;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

struct Linear(Vec<Event>);
impl Timeline for Linear {
    fn last(&self, key: Key, cut: Cut) -> Option<Event> {
        self.0
            .iter()
            .rev()
            .copied()
            .find(|event| cut.includes(event.point) && event.keys().contains(&Some(key)))
    }
    fn first(&self, key: Key, after: Cut, cut: Cut) -> Option<Event> {
        self.0.iter().copied().find(|event| {
            !after.includes(event.point)
                && cut.includes(event.point)
                && event.keys().contains(&Some(key))
        })
    }
    fn holds(&self, channel: u8, after: Cut, cut: Cut) -> HoldSummary {
        self.0
            .iter()
            .copied()
            .filter(|event| {
                !after.includes(event.point)
                    && cut.includes(event.point)
                    && event.keys().contains(&Some(Key::Hold(channel)))
            })
            .fold(HoldSummary::default(), |summary, event| {
                summary.then(HoldSummary::update(event.hold().unwrap()))
            })
    }
    fn sostenuto(&self, channel: u8, after: Cut, cut: Cut) -> SostenutoSummary {
        self.0
            .iter()
            .copied()
            .filter(|event| {
                !after.includes(event.point)
                    && cut.includes(event.point)
                    && event.keys().contains(&Some(Key::Sostenuto(channel)))
            })
            .fold(SostenutoSummary::default(), |summary, event| {
                summary.then(SostenutoSummary::update(
                    event.sostenuto().unwrap(),
                    event.point,
                ))
            })
    }
}

fn timeline(packets: &[(u64, &[u8])]) -> Linear {
    Linear(
        packets
            .iter()
            .enumerate()
            .filter_map(|(order, (frame, packet))| {
                Event::from_message(
                    Point {
                        frame: Frame(*frame),
                        order: order as u64,
                    },
                    Message::parse(packet, 4096).unwrap(),
                )
            })
            .collect(),
    )
}
fn same(actual: &MidiState, expected: &MidiState, cut: Cut) {
    assert_eq!(
        actual.system_reset_seen(),
        expected.system_reset_seen(),
        "at {cut:?}"
    );
    for ch in 0..16 {
        assert_eq!(
            actual.channel(ch),
            expected.channel(ch),
            "channel {ch} at {cut:?}"
        );
    }
}
fn check(initial: &MidiState, packets: &[(u64, &[u8])], frames: &[u64]) {
    let timeline = timeline(packets);
    assert_no_heap(|| {
        for &frame in frames {
            let mut expected = *initial;
            for &(_, packet) in packets.iter().filter(|(at, _)| *at < frame) {
                expected.apply_packet(packet, 4096).unwrap();
            }
            let cut = Cut::frame(Frame(frame));
            same(&state(&timeline, initial, cut), &expected, cut);
        }
    });
}
fn initial() -> MidiState {
    let mut state = MidiState::default();
    for packet in [
        &[0xb0, 0, 2][..],
        &[0xb0, 32, 3],
        &[0xc0, 7],
        &[0xb0, 0, 4],
        &[0x90, 60, 90],
        &[0xb0, 66, 127],
        &[0x80, 60, 0],
        &[0x90, 61, 100],
        &[0xb0, 64, 127],
        &[0x80, 61, 0],
        &[0xa0, 61, 127],
        &[0xb0, 99, 2],
        &[0xb0, 6, 42],
        &[0xd0, 3],
        &[0xe0, 1, 2],
        &[0x91, 62, 50],
        &[0xb1, 121, 0],
        &[0x92, 63, 70],
        &[0xb2, 69, 127],
        &[0x82, 63, 0],
    ] {
        state.apply_packet(packet, 4096).unwrap();
    }
    state
}

#[shoop_test]
fn indexed_projection_matches_initial_latches_banks_resets_and_channel_modes() {
    let packets: &[(u64, &[u8])] = &[
        (0, &[0xb0, 66, 0]),
        (0, &[0xb0, 64, 0]),
        (0, &[0x90, 60, 0]),
        (1, &[0xb0, 7, 0]),
        (1, &[0xb0, 39, 20]),
        (1, &[0xb0, 2, 88]),
        (1, &[0xb0, 0, 5]),
        (1, &[0xc0, 9]),
        (1, &[0xb0, 32, 6]),
        (2, &[0x90, 60, 127]),
        (2, &[0xb0, 66, 127]),
        (2, &[0x80, 60, 0]),
        (2, &[0xb0, 69, 127]),
        (2, &[0xb0, 66, 0]),
        (2, &[0xb0, 64, 127]),
        (3, &[0xb0, 69, 0]),
        (3, &[0xb0, 64, 0]),
        (3, &[0xb0, 124, 3]),
        (3, &[0xb0, 125, 0]),
        (3, &[0xb0, 126, 2]),
        (3, &[0xb0, 127, 0]),
        (3, &[0xb0, 97, 0]),
        (3, &[0xb0, 98, 4]),
        (3, &[0xb0, 100, 2]),
        (4, &[0xb0, 121, 0]),
        (4, &[0xa0, 60, 127]),
        (4, &[0xd0, 127]),
        (4, &[0xe0, 127, 127]),
        (4, &[0x91, 62, 127]),
        (4, &[0xb1, 123, 0]),
        (5, &[0xff]),
        (5, &[0x90, 60, 90]),
        (5, &[0xb0, 66, 127]),
        (5, &[0xb0, 120, 0]),
        (5, &[0x90, 60, 80]),
        (5, &[0xb0, 66, 127]),
        (6, &[0xb0, 121, 0]),
        (6, &[0xb0, 66, 127]),
        (6, &[0x80, 60, 0]),
        (7, &[0xb0, 66, 0]),
        (7, &[0xf0, 1, 0xf7]),
        (7, &[0xf8]),
    ];
    check(&initial(), packets, &[0, 1, 2, 3, 4, 5, 6, 7, 8]);
}

#[shoop_test]
fn indexed_projection_uses_packet_order_at_equal_wide_frames() {
    let frame = u64::from(u32::MAX) + 100;
    let packets: Vec<_> = [
        &[0xb0, 0, 2][..],
        &[0xb0, 32, 3],
        &[0xc0, 7],
        &[0xb0, 32, 4],
        &[0x90, 60, 90],
        &[0xb0, 66, 127],
        &[0x80, 60, 0],
        &[0xb0, 121, 0],
        &[0xb0, 66, 127],
        &[0xff],
        &[0x91, 61, 127],
        &[0xa1, 61, 0],
    ]
    .into_iter()
    .map(|packet| (frame, packet))
    .collect();
    let timeline = timeline(&packets);
    let initial = initial();
    assert_no_heap(|| {
        let mut expected = initial;
        for (order, (_, packet)) in packets.iter().enumerate() {
            let point = Point {
                frame: Frame(frame),
                order: order as u64,
            };
            same(
                &state(&timeline, &initial, Cut::before(point)),
                &expected,
                Cut::before(point),
            );
            expected.apply_packet(packet, 4096).unwrap();
            same(
                &state(&timeline, &initial, Cut::at(point)),
                &expected,
                Cut::at(point),
            );
        }
    });
}

#[shoop_test]
fn indexed_projection_matches_linear_packets_after_erasure_wrap_and_changed_zero_state() {
    let mut seed = 0x2c19_fa76_338a_a021u64;
    for _ in 0..8 {
        let mut packets = Vec::new();
        for order in 0..48 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let ch = ((seed >> 32) & 3) as u8;
            let key = 60 + ((seed >> 40) & 3) as u8;
            let value = ((seed >> 48) & 127) as u8;
            let packet = match (seed >> 24) % 12 {
                0 => [0x90 | ch, key, value],
                1 => [0x80 | ch, key, 0],
                2 => [0xa0 | ch, key, value],
                3 => [0xb0 | ch, [64, 66, 69][key as usize % 3], value],
                4 => [0xb0 | ch, [120, 121, 123][key as usize % 3], 0],
                5 => [0xb0 | ch, 124 + key % 4, value],
                6 => [0xb0 | ch, [0, 32, 7, 39][key as usize % 4], value],
                7 => [0xb0 | ch, [6, 38, 96, 97][key as usize % 4], value],
                8 => [0xb0 | ch, 98 + key % 4, value],
                9 => [0xc0 | ch, value, 0],
                10 => [0xd0 | ch, value, 0],
                _ => [0xe0 | ch, key, value],
            };
            packets.push((order / 3, packet));
        }
        let mut initial = initial();
        for variant in 0..3 {
            let mut edited: Vec<_> = packets
                .iter()
                .copied()
                .filter(|(at, _)| match variant {
                    0 => true,
                    1 => !(2..6).contains(at),
                    _ => (2..13).contains(at),
                })
                .collect();
            if variant > 0 {
                edited.extend([
                    (0, [0xb0, 66, 0]),
                    (5, [0xb0, 121, 0]),
                    (5, [0x90, 60, 127]),
                    (14, [0xb0, 64, 127]),
                ]);
                edited.sort_by_key(|(frame, _)| *frame);
            }
            if variant == 2 {
                initial = MidiState::default();
                initial.apply_packet(&[0x93, 62, 100], 3).unwrap();
                initial.apply_packet(&[0xb3, 66, 127], 3).unwrap();
                initial.apply_packet(&[0x83, 62, 0], 3).unwrap();
            }
            let borrowed: Vec<_> = edited
                .iter()
                .map(|(frame, packet)| {
                    (
                        *frame,
                        &packet[..if matches!(packet[0] & 0xf0, 0xc0 | 0xd0) {
                            2
                        } else {
                            3
                        }],
                    )
                })
                .collect();
            check(&initial, &borrowed, &[0, 5, 9, 16]);
        }
    }
}
