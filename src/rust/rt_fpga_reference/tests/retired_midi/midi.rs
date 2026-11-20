use rt_fpga_reference::midi::{
    Emission, EmissionKind, Error, Interpreter, Message, MidiState, Work,
};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;

fn message(data: &[u8]) -> Message {
    Message::managed(data).unwrap()
}
fn apply(state: &mut Interpreter, data: &[u8]) {
    state.apply(message(data)).unwrap();
}

#[shoop_test]
fn message_policy_is_explicit_and_invalid_packets_are_atomic() {
    for status in 0x80..=0xef {
        let data = [status, 60, 42];
        let len = if status & 0xe0 == 0xc0 { 2 } else { 3 };
        assert_eq!(message(&data[..len]).as_bytes(), &data[..len]);
        assert_eq!(Message::managed(&data[..len - 1]), Err(Error::InvalidEvent));
    }
    for cc in [6, 38, 96, 97, 98, 99, 100, 101] {
        assert_eq!(Message::managed(&[0xb0, cc, 0]), Err(Error::Unsupported));
    }
    for status in 0xf0..=0xfe {
        assert_eq!(Message::managed(&[status]), Err(Error::Unsupported));
    }
    assert_eq!(Message::managed(&[0x90, 128, 0]), Err(Error::InvalidEvent));
    assert_eq!(Message::managed(&[]), Err(Error::InvalidEvent));
    assert_eq!(message(&[0xff]).as_bytes(), &[0xff]);
}

#[shoop_test]
fn stepped_state_matches_production_projection_across_channels_and_pedals() {
    let mut state = Interpreter::default();
    let mut oracle = rt_runtime::midi_state::MidiState::default();
    let mut seed = 73991u32;
    assert_no_heap(|| {
        for i in 0..1500 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let ch = ((seed >> 12) & 15) as u8;
            let key = ((seed >> 16) & 127) as u8;
            let value = ((seed >> 24) & 127) as u8;
            let data = match i % 16 {
                0 => [0xb0 | ch, 64, value],
                1 => [0xb0 | ch, 66, value],
                2 => [0xb0 | ch, 69, value],
                3 => [0xb0 | ch, 123, 0],
                4 => [0xb0 | ch, 121, 0],
                5 => [0xb0 | ch, 120, 0],
                6 => [0xb0 | ch, 0, value],
                7 => [0xb0 | ch, 32, value],
                8 => [0xc0 | ch, value, 0],
                9 => [0xa0 | ch, key, value],
                10 => [0xd0 | ch, value, 0],
                11 => [0xe0 | ch, key, value],
                12 => [0xb0 | ch, 124 + (key % 4), value],
                13 => [0x80 | ch, key, value],
                _ => [0x90 | ch, key, value],
            };
            let len = if data[0] & 0xe0 == 0xc0 { 2 } else { 3 };
            apply(&mut state, &data[..len]);
            oracle.apply_packet(&data[..len], 3).unwrap();
            for ch in 0..16 {
                let actual = state.state().unwrap().channel(ch).unwrap();
                let expected = oracle.channel(ch).unwrap();
                assert_eq!(actual.program(), expected.program());
                assert_eq!(actual.program_bank(), expected.program_bank());
                assert_eq!(actual.pitch_bend(), expected.pitch_bend());
                assert_eq!(actual.pressure(), expected.pressure());
                for key in 0..128 {
                    let a = actual.note(key).unwrap();
                    let b = expected.note(key).unwrap();
                    assert_eq!(
                        (a.velocity(), a.held(), a.sostenuto(), a.pressure()),
                        (b.velocity(), b.held(), b.sostenuto(), b.pressure())
                    );
                    if !matches!(key, 6 | 38 | 96..=101) {
                        assert_eq!(actual.controller(key), expected.controller(key));
                    }
                }
            }
        }
    });
}

#[shoop_test]
fn partial_updates_are_private_and_pedal_work_is_visible() {
    let mut state = Interpreter::default();
    apply(&mut state, &[0x90, 60, 80]);
    apply(&mut state, &[0xb0, 64, 127]);
    apply(&mut state, &[0x80, 60, 0]);
    let before = state.steps();
    state.begin(message(&[0xb0, 64, 0])).unwrap();
    assert_eq!(state.state(), Err(Error::Busy));
    assert_eq!(state.begin(message(&[0xff])), Err(Error::Busy));
    assert_no_heap(|| while !state.step() {});
    assert_eq!(state.steps() - before, 129);
    assert_eq!(
        state
            .state()
            .unwrap()
            .channel(0)
            .unwrap()
            .note(60)
            .unwrap()
            .velocity(),
        0
    );
    let before = state.steps();
    assert_no_heap(|| {
        apply(&mut state, &[0xff]);
    });
    assert_eq!(state.steps() - before, 4226);
    assert!(state.state().unwrap().system_reset_seen());
}

#[shoop_test]
fn direct_state_updates_keep_the_same_managed_policy_as_the_interpreter() {
    let mut state = MidiState::default();
    let mut stepped = Interpreter::default();
    for bytes in [
        [0x90, 60, 70],
        [0xb0, 64, 127],
        [0x80, 60, 0],
        [0xb0, 121, 0],
    ] {
        state.apply_packet(&bytes, 3).unwrap();
        stepped.apply(message(&bytes)).unwrap();
        assert_eq!(&state, stepped.state().unwrap());
        assert!(state.is_basic());
    }
    let before = state;
    assert_eq!(
        state.apply_packet(&[0xb0, 6, 1], 3),
        Err(Error::Unsupported)
    );
    assert_eq!(state, before);
    assert_eq!(
        Message::managed(&[0xf0, 1, 2, 0xf7]),
        Err(Error::Unsupported)
    );
}

#[shoop_test]
fn restore_orders_loaded_bank_pending_bank_and_sostenuto_and_retries_exactly() {
    let mut state = Interpreter::default();
    for data in [
        [0xb0, 0, 3],
        [0xb0, 32, 4],
        [0xc0, 9, 0],
        [0xb0, 0, 8],
        [0x90, 60, 50],
        [0xb0, 66, 127],
        [0x80, 60, 0],
    ] {
        apply(&mut state, &data[..if data[0] == 0xc0 { 2 } else { 3 }]);
    }
    let mut emit = Emission::new(EmissionKind::Restore);
    let mut work = Work::default();
    let mut packets = Vec::new();
    let mut refuse = true;
    while !emit.complete() {
        let result = emit.step(state.state().unwrap(), &mut work, &mut |packet| {
            if refuse {
                refuse = false;
                return Err(Error::Capacity);
            }
            packets.push(packet);
            refuse = true;
            Ok(())
        });
        assert!(matches!(result, Ok(_) | Err(Error::Capacity)));
    }
    let expected = [[0xb0, 0, 3], [0xb0, 32, 4], [0xc0, 9, 0], [0xb0, 0, 8]];
    assert_eq!(
        &packets[..4],
        &expected.map(|p| message(&p[..if p[0] == 0xc0 { 2 } else { 3 }]))
    );
    let note = packets
        .iter()
        .position(|p| *p == message(&[0x90, 60, 50]))
        .unwrap();
    assert_eq!(packets[note - 1], message(&[0xb0, 66, 0]));
    assert_eq!(packets[note + 1], message(&[0xb0, 66, 127]));
    assert_eq!(packets[note + 2], message(&[0x80, 60, 0]));
    let mut reconstructed = Interpreter::default();
    for p in packets {
        reconstructed.apply(p).unwrap();
    }
    let actual = reconstructed.state().unwrap().channel(0).unwrap();
    let original = state.state().unwrap().channel(0).unwrap();
    assert_eq!(actual.note(60), original.note(60));
    assert_eq!(actual.program_bank(), original.program_bank());
    assert_eq!(work.accepted, 71);
}

#[shoop_test]
fn cleanup_and_recovery_are_allocation_free_and_acknowledged() {
    let mut state = Interpreter::default();
    apply(&mut state, &[0x9f, 127, 100]);
    apply(&mut state, &[0xbf, 64, 127]);
    let mut emit = Emission::new(EmissionKind::Cleanup);
    let mut work = Work::default();
    let mut packets = [message(&[0xff]); 32];
    let mut count = 0;
    assert_no_heap(|| {
        while !emit.complete() {
            emit.step(state.state().unwrap(), &mut work, &mut |p| {
                packets[count] = p;
                count += 1;
                Ok(())
            })
            .unwrap();
        }
    });
    assert_eq!(
        &packets[..count],
        &[message(&[0xbf, 64, 0]), message(&[0x8f, 127, 0])]
    );
    let mut emit = Emission::new(EmissionKind::Recovery);
    count = 0;
    assert!(emit
        .step(&MidiState::default(), &mut work, &mut |_| Err(
            Error::Capacity
        ))
        .is_err());
    assert_no_heap(|| {
        while !emit.complete() {
            emit.step(state.state().unwrap(), &mut work, &mut |p| {
                packets[count] = p;
                count += 1;
                Ok(())
            })
            .unwrap();
        }
    });
    assert_eq!(count, 32);
    for (i, p) in packets.into_iter().enumerate() {
        assert_eq!(
            p,
            message(&[0xb0 | (i / 2) as u8, if i % 2 == 0 { 120 } else { 123 }, 0])
        );
    }
}

#[shoop_test]
fn sustain_matches_an_independent_held_key_map() {
    use std::collections::BTreeMap;
    let mut keys = BTreeMap::<u8, (bool, u8)>::new();
    let mut sustain = false;
    let mut state = Interpreter::default();
    let mut random = 37u32;
    for i in 0..2000 {
        random = random.wrapping_mul(1103515245).wrapping_add(12345);
        let key = ((random >> 15) & 127) as u8;
        if i % 7 == 0 {
            sustain = !sustain;
            apply(&mut state, &[0xb0, 64, if sustain { 127 } else { 0 }]);
            if !sustain {
                keys.retain(|_, (held, _)| *held);
            }
        } else if i % 3 == 0 {
            apply(&mut state, &[0x80, key, 0]);
            if sustain {
                if let Some((held, _)) = keys.get_mut(&key) {
                    *held = false;
                }
            } else {
                keys.remove(&key);
            }
        } else {
            let velocity = 1 + (random % 127) as u8;
            apply(&mut state, &[0x90, key, velocity]);
            keys.insert(key, (true, velocity));
        }
        for key in 0..128 {
            let note = state
                .state()
                .unwrap()
                .channel(0)
                .unwrap()
                .note(key)
                .unwrap();
            assert_eq!(
                (note.held(), note.velocity()),
                keys.get(&key).copied().unwrap_or((false, 0))
            );
        }
    }
}
