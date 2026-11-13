use super::*;
use crate::midi_state::MidiState;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

#[shoop_test]
fn hold_intervals_compose_associatively_and_match_protocol_note_release() {
    assert_no_heap(|| {
        let mut seed = 0x2789_af13_7192_deacu64;
        for _ in 0..128 {
            let mut leaves = [HoldSummary::default(); 32];
            let mut packets = [[0; 3]; 32];
            for (leaf, packet) in leaves.iter_mut().zip(&mut packets) {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let down = seed & 1 != 0;
                let (update, cc) = match (seed >> 32) % 3 {
                    0 => (HoldUpdate::Sustain(down), 64),
                    1 => (HoldUpdate::Hold2(down), 69),
                    _ => (HoldUpdate::Reset, 121),
                };
                *leaf = HoldSummary::update(update);
                *packet = [0xb0, cc, if down && cc != 121 { 127 } else { 0 }];
            }
            let fold = |range: core::ops::Range<usize>| {
                leaves[range]
                    .iter()
                    .copied()
                    .fold(HoldSummary::default(), HoldSummary::then)
            };
            let (a, b, c) = (fold(0..5), fold(5..19), fold(19..32));
            let summary = a.then(b).then(c);
            assert_eq!(summary, a.then(b.then(c)));
            assert_eq!(summary, fold(0..32));
            assert_eq!(summary.then(HoldSummary::default()), summary);
            for sustain in [false, true] {
                for hold2 in [false, true] {
                    let initial = HoldState::new(sustain, hold2);
                    let mut state = MidiState::default();
                    for packet in [
                        [0x90, 60, 127],
                        [0xb0, 64, if sustain { 127 } else { 0 }],
                        [0xb0, 69, if hold2 { 127 } else { 0 }],
                        [0x80, 60, 0],
                    ] {
                        state.apply_packet(&packet, 3).unwrap();
                    }
                    for packet in packets {
                        state.apply_packet(&packet, 3).unwrap();
                    }
                    let result = summary.apply(initial);
                    let channel = state.channel(0).unwrap();
                    assert_eq!(
                        result.state.sustain(),
                        channel.controller(64).unwrap() >= 64
                    );
                    assert_eq!(result.state.hold2(), channel.controller(69).unwrap() >= 64);
                    let velocity = if !initial.active() || result.clear_seen {
                        0
                    } else {
                        127
                    };
                    assert_eq!(channel.note(60).unwrap().velocity(), velocity);
                }
            }
        }
    });
}

#[shoop_test]
fn sostenuto_edit_reclassifies_unchanged_suffix_without_restoration_packets() {
    assert_no_heap(|| {
        let frame = Frame(u64::from(u32::MAX) + 100);
        let first = Point { frame, order: 1 };
        let last = Point { frame, order: 3 };
        let prefix = SostenutoSummary::update(SostenutoUpdate::Pedal(true), first);
        let suffix = SostenutoSummary::update(SostenutoUpdate::Pedal(true), last);
        assert_eq!(prefix.then(suffix).apply(false).last_rising, Some(first));
        assert_eq!(
            SostenutoSummary::default()
                .then(suffix)
                .apply(false)
                .last_rising,
            Some(last)
        );

        let mut original = MidiState::default();
        let mut erased = MidiState::default();
        original.apply_packet(&[0x90, 60, 90], 3).unwrap();
        erased.apply_packet(&[0x90, 60, 90], 3).unwrap();
        original.apply_packet(&[0xb0, 66, 127], 3).unwrap();
        for state in [&mut original, &mut erased] {
            state.apply_packet(&[0x80, 60, 0], 3).unwrap();
            state.apply_packet(&[0xb0, 66, 127], 3).unwrap();
        }
        assert!(original.channel(0).unwrap().note(60).unwrap().sostenuto());
        assert!(!erased.channel(0).unwrap().note(60).unwrap().sostenuto());
        assert_eq!(
            original.channel(0).unwrap().note(60).unwrap().velocity(),
            90
        );
        assert_eq!(erased.channel(0).unwrap().note(60).unwrap().velocity(), 0);
    });
}

#[shoop_test]
fn sostenuto_interval_preserves_clear_reset_distinction_and_equal_frame_order() {
    assert_no_heap(|| {
        let frame = Frame(u64::from(u32::MAX) + 100);
        let p = |order| Point { frame, order };
        let first = SostenutoSummary::update(SostenutoUpdate::Pedal(true), p(1));
        let suffix = SostenutoSummary::update(SostenutoUpdate::Pedal(true), p(3));
        for (update, cc, rising) in [
            (SostenutoUpdate::Clear, 120, p(1)),
            (SostenutoUpdate::Reset, 121, p(3)),
        ] {
            let middle = SostenutoSummary::update(update, p(2));
            let summary = first.then(middle).then(suffix);
            assert_eq!(summary, first.then(middle.then(suffix)));
            assert_eq!(SostenutoSummary::default().then(summary), summary);
            let result = summary.apply(false);
            assert!(result.down);
            assert_eq!(result.first_clear, Some(p(2)));
            assert_eq!(result.last_clear, Some(p(2)));
            assert_eq!(result.last_rising, Some(rising));
            let mut state = MidiState::default();
            for packet in [
                [0x90, 60, 90],
                [0xb0, 66, 127],
                [0xb0, cc, 0],
                [0xb0, 66, 127],
            ] {
                state.apply_packet(&packet, 3).unwrap();
            }
            assert_eq!(
                state.channel(0).unwrap().note(60).unwrap().sostenuto(),
                cc == 121
            );
        }
        let up = SostenutoSummary::update(SostenutoUpdate::Pedal(false), p(4));
        assert_eq!(up.apply(false).first_clear, None);
        assert_eq!(up.apply(true).first_clear, Some(p(4)));
    });
}
