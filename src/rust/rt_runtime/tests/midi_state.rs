mod support;
use rt_runtime::{midi_state::MidiState, Error};
use shoop_wasm_test_support::shoop_test;
use std::collections::BTreeMap;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn send(state: &mut MidiState, data: &[u8]) {
    assert_no_heap(|| state.apply_packet(data, 4096)).unwrap();
}
fn note(state: &MidiState, channel: u8, key: u8) -> (u8, bool, bool) {
    let n = state.channel(channel).unwrap().note(key).unwrap();
    (n.velocity(), n.held(), n.sostenuto())
}
#[shoop_test]
fn held_and_pedal_latched_keys_keep_velocities_and_channels_independent() {
    let mut state = MidiState::default();
    send(&mut state, &[0x90, 60, 30]);
    send(&mut state, &[0x91, 60, 40]);
    send(&mut state, &[0xb0, 64, 127]);
    send(&mut state, &[0x90, 60, 0]);
    assert_eq!(note(&state, 0, 60), (30, false, false));
    assert_eq!(note(&state, 1, 60), (40, true, false));
    send(&mut state, &[0x90, 60, 80]);
    assert_eq!(note(&state, 0, 60), (80, true, false));
    send(&mut state, &[0xb0, 64, 0]);
    assert_eq!(note(&state, 0, 60), (80, true, false));
    send(&mut state, &[0xb0, 66, 127]);
    send(&mut state, &[0x90, 61, 70]);
    send(&mut state, &[0x80, 60, 1]);
    send(&mut state, &[0x80, 61, 1]);
    assert_eq!(note(&state, 0, 60), (80, false, true));
    assert_eq!(note(&state, 0, 61), (0, false, false));
    send(&mut state, &[0xb0, 64, 127]);
    send(&mut state, &[0xb0, 66, 0]);
    assert_eq!(note(&state, 0, 60), (80, false, false));
    send(&mut state, &[0xb0, 69, 127]);
    send(&mut state, &[0xb0, 64, 0]);
    assert_eq!(note(&state, 0, 60).0, 80);
    send(&mut state, &[0xb0, 69, 0]);
    assert_eq!(note(&state, 0, 60).0, 0);
    assert_eq!(note(&state, 1, 60), (40, true, false));
}
#[shoop_test]
fn channel_modes_and_reset_baselines_do_not_invent_unknown_controller_values() {
    let mut state = MidiState::default();
    for packet in [
        [0xb0, 7, 93],
        [0xb0, 10, 32],
        [0xb0, 0, 3],
        [0xb0, 32, 4],
        [0xb0, 74, 66],
        [0xb0, 91, 33],
        [0xb0, 1, 90],
        [0xb0, 2, 22],
        [0xb0, 64, 127],
        [0x90, 60, 50],
        [0x90, 61, 60],
        [0x80, 60, 0],
        [0xa0, 61, 77],
        [0xe0, 1, 2],
    ] {
        send(&mut state, &packet);
    }
    send(&mut state, &[0xc0, 12]);
    send(&mut state, &[0xd0, 45]);
    send(&mut state, &[0xb0, 121, 0]);
    let ch = state.channel(0).unwrap();
    for (cc, value) in [
        (7, 93),
        (10, 32),
        (0, 3),
        (32, 4),
        (74, 66),
        (91, 33),
        (1, 0),
        (11, 127),
        (64, 0),
        (65, 0),
        (66, 0),
        (67, 0),
        (69, 0),
        (98, 127),
        (99, 127),
        (100, 127),
        (101, 127),
    ] {
        assert_eq!(ch.controller(cc), Some(value));
    }
    assert_eq!(ch.controller(2), None);
    assert_eq!(ch.controller(121), None);
    assert!(ch.reset_controllers_seen());
    assert_eq!(ch.program(), Some(12));
    assert_eq!(ch.pitch_bend(), 8192);
    assert_eq!(ch.pressure(), Some(0));
    assert_eq!(ch.note(61).unwrap().pressure(), Some(0));
    assert_eq!(note(&state, 0, 60).0, 0);
    assert_eq!(note(&state, 0, 61), (60, true, false));
    send(&mut state, &[0xb0, 64, 127]);
    send(&mut state, &[0xb0, 123, 0]);
    assert_eq!(note(&state, 0, 61), (60, false, false));
    send(&mut state, &[0xb0, 120, 0]);
    assert_eq!(note(&state, 0, 61).0, 0);
    assert_eq!(state.channel(0).unwrap().controller(64), Some(127));
    for cc in 124..=127 {
        send(&mut state, &[0x90, 62, 100]);
        send(&mut state, &[0xb0, cc, 0]);
        assert!(!state.channel(0).unwrap().note(62).unwrap().held());
    }
    send(&mut state, &[0xff]);
    assert!(state.system_reset_seen());
    let initial = MidiState::default();
    for ch in 0..16 {
        assert_eq!(state.channel(ch), initial.channel(ch));
    }
}
#[shoop_test]
fn pressure_program_pitch_and_copy_snapshots_are_fixed_plain_data() {
    let mut state = MidiState::default();
    send(&mut state, &[0x9f, 127, 127]);
    send(&mut state, &[0xaf, 127, 83]);
    send(&mut state, &[0xcf, 99]);
    send(&mut state, &[0xdf, 27]);
    send(&mut state, &[0xef, 127, 127]);
    let snapshot = assert_no_heap(|| state);
    assert_eq!(
        snapshot.channel(15).unwrap().note(127).unwrap().pressure(),
        Some(83)
    );
    assert_eq!(snapshot.channel(15).unwrap().program(), Some(99));
    assert_eq!(snapshot.channel(15).unwrap().pressure(), Some(27));
    assert_eq!(snapshot.channel(15).unwrap().pitch_bend(), 16383);
    send(&mut state, &[0xbf, 120, 0]);
    assert_eq!(note(&snapshot, 15, 127).0, 127);
    assert_eq!(note(&state, 15, 127).0, 0);
    assert_eq!(state.channel(16), None);
    assert_eq!(state.channel(0).unwrap().note(128), None);
    assert_eq!(state.channel(0).unwrap().controller(128), None);
}
#[shoop_test]
fn malformed_packets_are_atomic_and_non_channel_packets_do_not_fabricate_state() {
    let mut state = MidiState::default();
    send(&mut state, &[0x90, 20, 70]);
    let before = state;
    for packet in [
        &[][..],
        &[0x90, 20],
        &[0xb0, 128, 0],
        &[0xf0, 128, 0xf7],
        &[0xf4],
        &[0xe0, 0, 128],
    ] {
        assert_no_heap(|| assert!(state.apply_packet(packet, 4096).is_err()));
        assert_eq!(state, before);
    }
    assert_eq!(
        assert_no_heap(|| state.apply_packet(&[0x90, 20, 90], 2)),
        Err(Error::Capacity)
    );
    assert_eq!(state, before);
    for packet in [
        &[0xf0, 1, 2, 3, 0xf7][..],
        &[0xf8],
        &[0xfa],
        &[0xfb],
        &[0xfc],
        &[0xfe],
        &[0xf1, 20],
        &[0xf2, 3, 4],
        &[0xf3, 1],
        &[0xf6],
    ] {
        send(&mut state, packet);
        assert_eq!(state, before);
    }
}
#[shoop_test]
fn generated_note_and_sustain_projection_matches_an_independent_map() {
    let mut state = MidiState::default();
    let mut notes = BTreeMap::<(u8, u8), (u8, bool)>::new();
    let mut pedal = [false; 16];
    let mut bend = [8192u16; 16];
    let mut seed = 0xad473b29385cc291u64;
    for step in 0..4096 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let ch = ((seed >> 32) % 16) as u8;
        let key = ((seed >> 40) % 128) as u8;
        let vel = (((seed >> 48) % 127) + 1) as u8;
        let kind = (seed >> 24) % 8;
        let packet = match kind {
            0 => {
                notes.insert((ch, key), (vel, true));
                [0x90 | ch, key, vel]
            }
            1 | 2 => {
                if pedal[ch as usize] {
                    if let Some(n) = notes.get_mut(&(ch, key)) {
                        n.1 = false;
                    }
                } else {
                    notes.remove(&(ch, key));
                }
                [if kind == 1 { 0x80 | ch } else { 0x90 | ch }, key, 0]
            }
            3 => {
                let down = seed & 1 != 0;
                pedal[ch as usize] = down;
                if !down {
                    notes.retain(|(c, _), (_, held)| *c != ch || *held);
                }
                [0xb0 | ch, 64, if down { 127 } else { 0 }]
            }
            4 => {
                if pedal[ch as usize] {
                    for ((c, _), (_, held)) in &mut notes {
                        if *c == ch {
                            *held = false;
                        }
                    }
                } else {
                    notes.retain(|(c, _), _| *c != ch);
                }
                [0xb0 | ch, 123, 0]
            }
            5 => {
                notes.retain(|(c, _), _| *c != ch);
                [0xb0 | ch, 120, 0]
            }
            6 => {
                pedal[ch as usize] = false;
                bend[ch as usize] = 8192;
                notes.retain(|(c, _), (_, held)| *c != ch || *held);
                [0xb0 | ch, 121, 0]
            }
            _ => {
                bend[ch as usize] = u16::from(key) | (u16::from(vel) << 7);
                [0xe0 | ch, key, vel]
            }
        };
        send(&mut state, &packet);
        if step % 64 == 63 {
            for channel in 0..16 {
                assert_eq!(
                    state.channel(channel).unwrap().pitch_bend(),
                    bend[channel as usize]
                );
                assert_eq!(
                    state.channel(channel).unwrap().controller(64).unwrap() >= 64,
                    pedal[channel as usize]
                );
                for key in 0..128 {
                    let expected = notes.get(&(channel, key)).copied().unwrap_or((0, false));
                    assert_eq!(note(&state, channel, key), (expected.0, expected.1, false));
                }
            }
        }
    }
}
