use rt_fpga_reference::{
    midi::{performance::*, *},
    records::Identity,
};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;
fn msg(bytes: &[u8]) -> Message {
    Message::parse(bytes).unwrap()
}
fn reset() -> State {
    let mut s = State::new();
    s.known_reset(1);
    s
}
fn memory(count: u16) -> MidiMemory {
    let mut m = MidiMemory::prepare(
        StoreLimits {
            block_events: 8,
            blocks: 128,
            max_events: 128,
            views: count as u32 + 8,
            retirements: 128,
        },
        count,
    )
    .unwrap();
    m.prepare_state(1, 2).unwrap();
    for output in 0..2 {
        m.expression
            .as_mut()
            .unwrap()
            .seed_output(output, &reset(), &m.store)
            .unwrap();
    }
    m
}
fn install(m: &mut MidiMemory, seq: u16, target: &State) -> ViewId {
    let view = m.store.writer().unwrap();
    m.store.seed_snapshot(view, target, 1).unwrap();
    m.store.finish(view, 64).unwrap();
    m.replace(SequenceId(seq), Some(view)).unwrap();
    m.bind_state(
        SequenceId(seq),
        StateBinding {
            owner: Identity {
                slot: seq as u32,
                generation: 1,
            },
            input: 0,
            outputs: 1,
            channels: 1,
        },
    )
    .unwrap();
    m.store.release(view).unwrap();
    view
}
fn output(m: &mut MidiMemory, bytes: &[u8]) {
    m.expression
        .as_mut()
        .unwrap()
        .accept(0, msg(bytes), None, &m.store)
        .unwrap();
}
fn restore(m: &mut MidiMemory, seq: u16) -> Vec<Vec<u8>> {
    let view = m.view(SequenceId(seq)).unwrap().unwrap();
    let mut packets = Vec::new();
    m.expression
        .as_mut()
        .unwrap()
        .restore(seq, view, 0, 0, &m.store, &mut |e| {
            packets.push(e.message.as_bytes().to_vec());
            Ok(())
        })
        .unwrap();
    packets
}
fn fresh(m: &MidiMemory, seq: u16) {
    let target = m
        .store
        .snapshot(m.view(SequenceId(seq)).unwrap().unwrap())
        .unwrap();
    let e = m.expression.as_ref().unwrap();
    let mut count = 0;
    for field in 0..FIELDS {
        let f = field % STRIDE;
        let managed = field < STRIDE
            && (f < 128
                || f == PITCH
                || f == PRESSURE
                || (128..256).contains(&f) && DEFAULT_CONTROLLERS & (1 << (f - 128)) != 0);
        let dirty = managed
            && !rt_fpga_reference::midi::performance::matches(
                target.word(field),
                e.output(0).unwrap().word(field),
                field,
            );
        assert_eq!(
            e.differences(seq, 0).unwrap().contains(field),
            dirty,
            "field {field}"
        );
        count += u16::from(dirty);
    }
    assert_eq!(e.differences(seq, 0).unwrap().count, count);
}
#[shoop_test]
fn controller_drift_return_and_output_channel_directory_are_incremental() {
    let mut m = memory(2);
    let mut target = reset();
    target.apply(msg(&[0xb0, 1, 20]));
    install(&mut m, 0, &target);
    install(&mut m, 1, &target);
    output(&mut m, &[0xb0, 1, 20]);
    assert_eq!(
        m.expression
            .as_ref()
            .unwrap()
            .differences(0, 0)
            .unwrap()
            .count,
        0
    );
    let before = m.expression.as_ref().unwrap().work.tracker_visits;
    output(&mut m, &[0xb1, 1, 60]);
    assert_eq!(m.expression.as_ref().unwrap().work.tracker_visits, before);
    for value in [60, 80, 90, 20, 100] {
        output(&mut m, &[0xb0, 1, value]);
        fresh(&m, 0);
        fresh(&m, 1);
    }
    assert_eq!(restore(&mut m, 0), vec![vec![0xb0, 1, 20]]);
    assert!(restore(&mut m, 1).is_empty());
    fresh(&m, 0);
    fresh(&m, 1);
}
#[shoop_test]
fn sustain_repair_releases_unwanted_notes_before_engagement() {
    let mut m = memory(1);
    let mut target = reset();
    for b in [[0xb0, 64, 127], [0x90, 60, 90], [0x80, 60, 0]] {
        target.apply(msg(&b));
    }
    install(&mut m, 0, &target);
    output(&mut m, &[0x90, 64, 80]);
    let packets = restore(&mut m, 0);
    let off = packets.iter().position(|b| b == &[0x80, 64, 0]).unwrap();
    let pedal = packets.iter().position(|b| b == &[0xb0, 64, 127]).unwrap();
    assert!(off < pedal);
    fresh(&m, 0);
    assert_eq!(
        m.expression.as_ref().unwrap().output(0).unwrap().word(60) & NOTE_CORE,
        90 | SUSTAIN
    );
    assert_eq!(
        m.expression
            .as_ref()
            .unwrap()
            .differences(0, 0)
            .unwrap()
            .count,
        0
    );
}
#[shoop_test]
fn sostenuto_captures_only_the_intended_notes_and_preserves_pressure() {
    let mut m = memory(1);
    let mut target = reset();
    for b in [
        [0x90, 60, 90],
        [0xb0, 66, 127],
        [0x80, 60, 0],
        [0x90, 64, 70],
        [0xa0, 64, 45],
    ] {
        target.apply(msg(&b));
    }
    install(&mut m, 0, &target);
    let packets = restore(&mut m, 0);
    let c = packets.iter().position(|b| b == &[0x90, 60, 90]).unwrap();
    let pedal = packets.iter().position(|b| b == &[0xb0, 66, 127]).unwrap();
    let e = packets.iter().position(|b| b == &[0x90, 64, 70]).unwrap();
    assert!(c < pedal && pedal < e);
    assert_eq!(packets.last().unwrap(), &[0xa0, 64, 45]);
    fresh(&m, 0);
    assert_eq!(
        m.expression
            .as_ref()
            .unwrap()
            .differences(0, 0)
            .unwrap()
            .count,
        0
    );
    output(&mut m, &[0xb0, 66, 100]);
    assert_eq!(
        m.expression.as_ref().unwrap().output(0).unwrap().word(64) & SOSTENUTO,
        0
    );
}
#[shoop_test]
fn velocity_and_pressure_repairs_do_not_restart_unrelated_notes() {
    let mut m = memory(1);
    let mut target = reset();
    target.apply(msg(&[0x90, 60, 90]));
    target.apply(msg(&[0xa0, 60, 50]));
    install(&mut m, 0, &target);
    output(&mut m, &[0x90, 60, 90]);
    assert_eq!(restore(&mut m, 0), vec![vec![0xa0, 60, 50]]);
    output(&mut m, &[0x90, 60, 40]);
    assert_eq!(
        restore(&mut m, 0),
        vec![vec![0x80, 60, 0], vec![0x90, 60, 90], vec![0xa0, 60, 50]]
    );
}
#[shoop_test]
fn bitmap_updates_match_full_comparison_across_pedals_resets_and_pressure() {
    let mut m = memory(2);
    let mut target = reset();
    for b in [
        [0x90, 60, 80],
        [0xb0, 66, 127],
        [0x80, 60, 0],
        [0xb0, 1, 30],
    ] {
        target.apply(msg(&b));
    }
    install(&mut m, 0, &target);
    install(&mut m, 1, &reset());
    let mut seed = 7u32;
    for n in 0..600 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let value = ((seed >> 12) & 127) as u8;
        let key = 60 + ((seed >> 20) & 7) as u8;
        let packet = match n % 11 {
            0 => [0xb0, 64, value],
            1 => [0xb0, 66, value],
            2 => [0xb0, 121, 0],
            3 => [0xb0, 123, 0],
            4 => [0xb0, 120, 0],
            5 => [0x90, key, value],
            6 => [0x80, key, 0],
            7 => [0xa0, key, value],
            8 => [0xe0, value, 32],
            _ => [0xb0, 1, value],
        };
        output(&mut m, &packet);
        fresh(&m, 0);
        fresh(&m, 1);
        if n % 41 == 0 {
            restore(&mut m, 0);
            fresh(&m, 0);
            fresh(&m, 1);
            assert_eq!(
                m.expression
                    .as_ref()
                    .unwrap()
                    .differences(0, 0)
                    .unwrap()
                    .count,
                0
            );
        }
    }
}
#[shoop_test]
fn snapshots_follow_ranges_and_retained_exports_across_retakes() {
    let mut m = memory(1);
    let mut initial = reset();
    initial.apply(msg(&[0xb0, 1, 20]));
    let view = m.store.writer().unwrap();
    m.store.seed_snapshot(view, &initial, 1).unwrap();
    m.store.append(view, 5, msg(&[0xb0, 1, 80])).unwrap();
    m.store.append(view, 16, msg(&[0xb0, 1, 90])).unwrap();
    m.store.finish(view, 64).unwrap();
    let range = m.store.range(view, 16, 48).unwrap();
    assert_eq!(m.store.snapshot(range).unwrap().word(129), KNOWN | 80);
    m.replace(SequenceId(0), Some(range)).unwrap();
    m.bind_state(
        SequenceId(0),
        StateBinding {
            owner: Identity {
                slot: 0,
                generation: 1,
            },
            input: 0,
            outputs: 1,
            channels: 1,
        },
    )
    .unwrap();
    assert_eq!(restore(&mut m, 0), vec![vec![0xb0, 1, 80]]);
    m.store.retain(range).unwrap();
    m.replace(SequenceId(0), None).unwrap();
    m.store.maintenance(128);
    assert_eq!(m.store.snapshot(range).unwrap().word(129), KNOWN | 80);
}
#[shoop_test]
fn refusal_retains_accepted_prefix_and_does_not_clean_the_tracker() {
    let mut m = memory(1);
    let mut target = reset();
    target.apply(msg(&[0xb0, 1, 20]));
    target.apply(msg(&[0xe0, 1, 70]));
    let view = install(&mut m, 0, &target);
    let mut accepted = 0;
    let result = m
        .expression
        .as_mut()
        .unwrap()
        .restore(0, view, 0, 0, &m.store, &mut |_| {
            if accepted == 1 {
                Err(Error::OutputRefused)
            } else {
                accepted += 1;
                Ok(())
            }
        });
    assert_eq!(result, Err(Error::OutputRefused));
    fresh(&m, 0);
    assert_eq!(
        m.expression.as_ref().unwrap().output(0).unwrap().word(129),
        KNOWN | 20
    );
    assert!(
        m.expression
            .as_ref()
            .unwrap()
            .differences(0, 0)
            .unwrap()
            .count
            > 0
    );
}
#[shoop_test]
fn full_prepared_tracker_capacity_and_restore_do_not_allocate() {
    let mut m = memory(128);
    let mut target = reset();
    target.apply(msg(&[0xb0, 1, 20]));
    for seq in 0..128 {
        install(&mut m, seq, &target);
    }
    let view = m.view(SequenceId(0)).unwrap().unwrap();
    assert_no_heap(|| {
        output(&mut m, &[0xb0, 1, 90]);
        let mut count = 0;
        m.expression
            .as_mut()
            .unwrap()
            .restore(0, view, 0, 0, &m.store, &mut |_| {
                count += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(
            m.expression
                .as_ref()
                .unwrap()
                .differences(127, 0)
                .unwrap()
                .count,
            0
        );
    });
}

// Deliberately uses separate key/voice arrays rather than the engine's packed fields.
struct Receiver {
    down: [bool; 128],
    velocity: [u8; 128],
    captured: [bool; 128],
    pressure: [u8; 128],
    sustain: bool,
    sostenuto: bool,
    cc: [u8; 128],
}
impl Receiver {
    fn new() -> Self {
        Self {
            down: [false; 128],
            velocity: [0; 128],
            captured: [false; 128],
            pressure: [0; 128],
            sustain: false,
            sostenuto: false,
            cc: [0; 128],
        }
    }
    fn receive(&mut self, b: &[u8]) {
        let k = b.get(1).copied().unwrap_or(0) as usize;
        match b[0] & 0xf0 {
            0x90 if b[2] != 0 => {
                self.down[k] = true;
                self.velocity[k] = b[2];
                self.pressure[k] = 0;
            }
            0x80 | 0x90 => {
                self.down[k] = false;
                if !self.sustain && !self.captured[k] {
                    self.velocity[k] = 0;
                }
            }
            0xa0 => {
                if self.velocity[k] != 0 {
                    self.pressure[k] = b[2];
                }
            }
            0xb0 => {
                self.cc[k] = b[2];
                if k == 64 {
                    self.sustain = b[2] >= 64;
                }
                if k == 66 {
                    let on = b[2] >= 64;
                    if on && !self.sostenuto {
                        self.captured = self.down;
                    }
                    if !on {
                        self.captured.fill(false);
                    }
                    self.sostenuto = on;
                }
                if k == 64 || k == 66 {
                    for n in 0..128 {
                        if !self.down[n] && !self.sustain && !self.captured[n] {
                            self.velocity[n] = 0;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
#[shoop_test]
fn pedal_reconstruction_reaches_an_independent_receiver_and_continues_matching_notes() {
    let cases: &[&[&[u8]]] = &[
        &[
            &[0x90, 60, 70],
            &[0xb0, 64, 127],
            &[0x80, 60, 0],
            &[0x90, 64, 90],
        ],
        &[
            &[0x90, 60, 70],
            &[0xb0, 66, 127],
            &[0x90, 64, 90],
            &[0x80, 60, 0],
            &[0xa0, 60, 33],
        ],
        &[
            &[0x90, 60, 70],
            &[0xb0, 66, 127],
            &[0xb0, 64, 127],
            &[0x80, 60, 0],
            &[0x90, 64, 90],
            &[0x80, 64, 0],
        ],
    ];
    for events in cases {
        let mut target = reset();
        let mut expected = Receiver::new();
        for bytes in *events {
            target.apply(msg(bytes));
            expected.receive(bytes);
        }
        let mut m = memory(1);
        install(&mut m, 0, &target);
        let mut actual = Receiver::new();
        for bytes in [&[0x90, 67, 50][..], &[0xb0, 64, 127], &[0x80, 67, 0]] {
            output(&mut m, bytes);
            actual.receive(bytes);
        }
        for bytes in restore(&mut m, 0) {
            actual.receive(&bytes);
        }
        assert_eq!(actual.velocity, expected.velocity);
        assert_eq!(actual.down, expected.down);
        assert_eq!(actual.captured, expected.captured);
        for k in 0..128 {
            if expected.velocity[k] != 0 {
                assert_eq!(actual.pressure[k], expected.pressure[k]);
            }
        }
        assert_eq!(
            (actual.sustain, actual.sostenuto),
            (expected.sustain, expected.sostenuto)
        );
        assert!(restore(&mut m, 0).is_empty());
        fresh(&m, 0);
    }
}
#[shoop_test]
fn unknown_outputs_and_seeks_get_truthful_comparisons() {
    let mut m = memory(1);
    let mut target = reset();
    target.apply(msg(&[0xb0, 1, 20]));
    let view = install(&mut m, 0, &target);
    m.expression
        .as_mut()
        .unwrap()
        .seed_output(0, &State::new(), &m.store)
        .unwrap();
    restore(&mut m, 0);
    fresh(&m, 0);
    assert_eq!(
        m.expression
            .as_ref()
            .unwrap()
            .differences(0, 0)
            .unwrap()
            .count,
        0
    );
    assert!(m
        .store
        .snapshot(ViewId {
            slot: view.slot,
            generation: view.generation + 1
        })
        .is_err());
}

#[shoop_test]
fn conflicting_targets_on_one_channel_update_each_other_after_corrections() {
    let mut m = memory(2);
    let mut a = reset();
    a.apply(msg(&[0xb0, 1, 20]));
    let mut b = a.clone();
    b.apply(msg(&[0xb0, 1, 40]));
    install(&mut m, 0, &a);
    install(&mut m, 1, &b);
    output(&mut m, &[0xb0, 1, 20]);
    assert_eq!(restore(&mut m, 1), vec![vec![0xb0, 1, 40]]);
    assert_eq!(
        m.expression
            .as_ref()
            .unwrap()
            .differences(0, 0)
            .unwrap()
            .count,
        1
    );
    assert_eq!(restore(&mut m, 0), vec![vec![0xb0, 1, 20]]);
    assert_eq!(
        m.expression
            .as_ref()
            .unwrap()
            .differences(1, 0)
            .unwrap()
            .count,
        1
    );
    fresh(&m, 0);
    fresh(&m, 1);
}
#[shoop_test]
fn matching_live_notes_are_not_claimed_by_a_controller_only_restore() {
    let mut m = memory(1);
    let mut s = reset();
    s.apply(msg(&[0x90, 60, 80]));
    s.apply(msg(&[0xb0, 1, 20]));
    let v = install(&mut m, 0, &s);
    m.expression
        .as_mut()
        .unwrap()
        .seed_output(0, &s, &m.store)
        .unwrap();
    output(&mut m, &[0xb0, 1, 80]);
    assert_eq!(restore(&mut m, 0), vec![vec![0xb0, 1, 20]]);
    m.expression
        .as_mut()
        .unwrap()
        .cleanup(0, 0, &m.store, &mut |_| {
            panic!("matching live note was claimed")
        })
        .unwrap();
    assert_eq!(
        m.expression.as_ref().unwrap().output(0).unwrap().word(60) & 127,
        80
    );
    assert!(m.store.snapshot(v).is_ok());
}

#[shoop_test]
fn all_1024_snapshot_output_pairs_update_and_restore_without_heap_work() {
    let mut m = memory(128);
    m.expression = Some(
        Expression::prepare(StateLimits {
            inputs: 1,
            outputs: 8,
            sequences: 128,
        })
        .unwrap(),
    );
    let mut target = reset();
    target.apply(msg(&[0xb0, 1, 20]));
    for output in 0..8 {
        m.expression
            .as_mut()
            .unwrap()
            .seed_output(output, &target, &m.store)
            .unwrap();
    }
    for seq in 0..128 {
        install(&mut m, seq, &target);
        m.bind_state(
            SequenceId(seq),
            StateBinding {
                owner: Identity {
                    slot: seq as u32,
                    generation: 1,
                },
                input: 0,
                outputs: 255,
                channels: 1,
            },
        )
        .unwrap();
    }
    let view = m.view(SequenceId(0)).unwrap().unwrap();
    let mut count = 0;
    assert_no_heap(|| {
        for output in 0..8 {
            m.expression
                .as_mut()
                .unwrap()
                .accept(output, msg(&[0xb0, 1, 90]), None, &m.store)
                .unwrap();
        }
        for seq in 0..128 {
            for output in 0..8 {
                assert_eq!(
                    m.expression
                        .as_ref()
                        .unwrap()
                        .differences(seq, output)
                        .unwrap()
                        .count,
                    1
                );
            }
        }
        m.expression
            .as_mut()
            .unwrap()
            .restore(0, view, 0, 0, &m.store, &mut |_| {
                count += 1;
                Ok(())
            })
            .unwrap();
        for seq in 0..128 {
            for output in 0..8 {
                assert_eq!(
                    m.expression
                        .as_ref()
                        .unwrap()
                        .differences(seq, output)
                        .unwrap()
                        .count,
                    0
                );
            }
        }
    });
    assert_eq!(count, 8);
}

#[shoop_test]
fn pedal_group_restore_establishes_known_silence_for_previously_unknown_keys() {
    let mut m = memory(1);
    install(&mut m, 0, &reset());
    m.expression
        .as_mut()
        .unwrap()
        .seed_output(0, &State::new(), &m.store)
        .unwrap();
    output(&mut m, &[0x90, 67, 80]);
    output(&mut m, &[0xb0, 64, 127]);
    output(&mut m, &[0x80, 67, 0]);
    restore(&mut m, 0);
    fresh(&m, 0);
    assert_eq!(
        m.expression
            .as_ref()
            .unwrap()
            .differences(0, 0)
            .unwrap()
            .count,
        0
    );
    assert!(restore(&mut m, 0).is_empty());
}

#[shoop_test]
fn unbound_and_out_of_scope_playback_do_not_claim_cleanup_ownership() {
    let mut m = memory(2);
    install(&mut m, 0, &reset());
    for (port, owner, bytes) in [
        (0, Some(1), &[0x90, 60, 80][..]),
        (1, Some(0), &[0x90, 64, 80][..]),
        (0, Some(0), &[0x91, 67, 80][..]),
    ] {
        m.expression
            .as_mut()
            .unwrap()
            .accept(port, msg(bytes), owner, &m.store)
            .unwrap();
    }
    for seq in 0..2 {
        m.expression
            .as_mut()
            .unwrap()
            .cleanup(seq, 0, &m.store, &mut |_| {
                panic!("claimed an out of scope note")
            })
            .unwrap();
    }
}

#[shoop_test]
fn a_stale_seek_does_not_drop_the_prepared_target_or_allocate() {
    let mut m = memory(1);
    let view = install(&mut m, 0, &reset());
    let stale = ViewId {
        slot: view.slot,
        generation: view.generation + 1,
    };
    assert_no_heap(|| {
        assert_eq!(
            m.expression
                .as_mut()
                .unwrap()
                .restore(0, stale, 16, 0, &m.store, &mut |_| Ok(())),
            Err(Error::InvalidState)
        );
        m.expression
            .as_mut()
            .unwrap()
            .restore(0, view, 16, 0, &m.store, &mut |_| Ok(()))
            .unwrap();
    });
}
