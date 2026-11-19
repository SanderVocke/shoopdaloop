#[path = "support/midi_session.rs"]
mod support;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
use support::*;
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;

#[shoop_test]
fn record_multiple_channels_restore_only_differences_and_keep_exact_event_offsets() {
    for q in [1, 16, 32, 64, 128] {
        let mut f = Fixture::new(q, 2);
        assert_no_heap(|| {
            assert_eq!(f.run(&[event(0, &[0xb0, 1, 40])]).error, None);
            f.command(0, Operation::Record);
            f.command(1, Operation::Record);
            assert_eq!(
                f.run(&[event(0, &[0x90, 60, 80]), event(q - 1, &[0xb1, 7, 100])])
                    .error,
                None
            );
            f.command(0, Operation::Stop);
            f.command(1, Operation::Stop);
            assert_eq!(f.run(&[]).error, None);
            f.midi
                .send(
                    OutputEvent {
                        destination: 0,
                        offset: 0,
                        message: msg(&[0xb0, 1, 90]),
                    },
                    &mut |_| Ok(()),
                )
                .unwrap();
            f.command(
                0,
                Operation::Play {
                    position: 0,
                    retrigger: true,
                },
            );
            f.command(
                1,
                Operation::Play {
                    position: 0,
                    retrigger: true,
                },
            );
            f.emitted.clear();
            assert_eq!(f.run(&[]).error, None);
            assert!(f.emitted.iter().map(|e| e.message).eq([
                msg(&[0xb0, 1, 40]),
                msg(&[0x90, 60, 80]),
                msg(&[0xb1, 7, 100])
            ]));
            assert_eq!(f.emitted.last().unwrap().offset, q - 1);
        });
    }
}

#[shoop_test]
fn history_grab_has_correct_pre_event_state_and_shares_one_snapshot() {
    let mut f = Fixture::new(16, 8);
    for i in 0..6 {
        assert_eq!(
            f.run(&[
                event(0, &[0xb0, 1, (i * 10) as u8]),
                event(10, &[0x90, 60, 80])
            ])
            .error,
            None
        );
    }
    let before = f.midi.store().usage();
    let slots = [0, 1, 2, 3, 4, 5, 6, 7];
    assert_no_heap(|| {
        f.pipeline
            .grab_midi(&mut f.midi, 0, 19, 83, &slots)
            .unwrap();
    });
    let view = f.midi.content(0).unwrap().unwrap();
    assert!(slots
        .iter()
        .all(|&s| f.midi.content(s).unwrap() == Some(view)));
    let d = f.midi.store().describe(view).unwrap();
    assert_eq!(d.frames, 64);
    assert_eq!(
        f.midi
            .store()
            .state(d.initial)
            .unwrap()
            .channel(0)
            .unwrap()
            .controller(1),
        Some(10)
    );
    assert_eq!(f.midi.store().usage().snapshots, before.snapshots + 1);
    for _ in 0..40 {
        f.run(&[event(0, &[0xb0, 1, 99])]);
    }
    assert_eq!(f.midi.store().describe(view).unwrap(), d);
    assert_eq!(
        f.midi
            .store()
            .state(d.initial)
            .unwrap()
            .channel(0)
            .unwrap()
            .controller(1),
        Some(10)
    );
    assert_eq!(f.midi.history_bounds(0).unwrap().0, 96 + 640 - 128);
}

#[shoop_test]
fn imports_exports_and_duplicate_retain_coherent_old_content_during_retake() {
    let mut f = Fixture::new(16, 2);
    let mut initial = MidiState::default();
    initial.apply_packet(&[0xb0, 1, 50], 3).unwrap();
    let view = f.midi.begin_import(&initial).unwrap();
    f.midi
        .import_events(
            view,
            &[
                Record {
                    frame: 5,
                    order: 0,
                    message: msg(&[0x90, 60, 90]),
                },
                Record {
                    frame: 5,
                    order: 1,
                    message: msg(&[0x80, 60, 0]),
                },
            ],
        )
        .unwrap();
    while !f.midi.finish_import(view, 32, 128).unwrap() {
        assert_eq!(f.run(&[]).error, None);
    }
    f.pipeline.replace_midi(&mut f.midi, &[0], view).unwrap();
    f.midi.release(view).unwrap();
    f.pipeline.duplicate_midi(&mut f.midi, 0, 1).unwrap();
    let export = f.midi.export(view).unwrap();
    f.command(0, Operation::Record);
    assert_eq!(f.run(&[event(0, &[0xb0, 1, 70])]).error, None);
    f.command(0, Operation::Stop);
    assert_eq!(f.run(&[]).error, None);
    let mut buffer = [Record {
        frame: 0,
        order: 0,
        message: msg(&[0xb0, 1, 0]),
    }; 8];
    assert_no_heap(|| {
        assert_eq!(f.midi.export_events(export, 0, &mut buffer).unwrap(), 2);
    });
    assert_eq!(buffer[0].frame, 5);
    assert_eq!(buffer[1].message, msg(&[0x80, 60, 0]));
    assert_eq!(f.midi.content(1).unwrap(), Some(export));
    assert_ne!(f.midi.content(0).unwrap(), Some(export));
    f.midi.release(export).unwrap();
}

#[shoop_test]
fn ordered_transition_reconstructs_loaded_bank_and_sostenuto_without_global_reset() {
    let mut f = Fixture::new(16, 1);
    let mut target = MidiState::default();
    for bytes in [
        &[0xb0, 0, 1][..],
        &[0xc0, 7][..],
        &[0xb0, 0, 9][..],
        &[0x90, 60, 80][..],
        &[0xb0, 66, 127][..],
        &[0x80, 60, 0][..],
        &[0x90, 64, 100][..],
    ] {
        target.apply_packet(bytes, 3).unwrap();
    }
    for bytes in [
        &[0xb0, 0, 2][..],
        &[0xc0, 7][..],
        &[0xb0, 0, 5][..],
        &[0x90, 67, 50][..],
    ] {
        f.midi
            .send(
                OutputEvent {
                    destination: 0,
                    offset: 0,
                    message: msg(bytes),
                },
                &mut |_| Ok(()),
            )
            .unwrap();
    }
    let view = f.midi.begin_import(&target).unwrap();
    while !f.midi.finish_import(view, 16, 256).unwrap() {}
    f.pipeline.replace_midi(&mut f.midi, &[0], view).unwrap();
    f.midi.release(view).unwrap();
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    assert_no_heap(|| {
        assert_eq!(f.run(&[]).error, None);
    });
    let out = f.midi.output_state(0).unwrap().channel(0).unwrap();
    assert_eq!(out.program_bank(), [Some(1), None]);
    assert_eq!(out.controller(0), Some(9));
    assert_eq!(out.program(), Some(7));
    assert!(out.note(60).unwrap().sostenuto());
    assert!(!out.note(60).unwrap().held());
    assert_eq!(out.note(60).unwrap().velocity(), 80);
    assert!(!out.note(64).unwrap().sostenuto());
    assert!(out.note(64).unwrap().held());
    assert_eq!(out.note(67).unwrap().velocity(), 0);
    assert!(f.emitted.iter().all(|e| e.message.as_bytes()[0] != 0xff
        && !(e.message.as_bytes()[0] & 0xf0 == 0xb0 && e.message.as_bytes()[1] == 121)));
    assert_eq!(f.emitted[0].message, msg(&[0xb0, 0, 1]));
    assert_eq!(f.emitted[1].message, msg(&[0xc0, 7]));
    assert_eq!(f.emitted[2].message, msg(&[0xb0, 0, 9]));
    let note = f
        .emitted
        .iter()
        .position(|e| e.message == msg(&[0x90, 60, 80]))
        .unwrap();
    let pedal = f
        .emitted
        .iter()
        .position(|e| e.message == msg(&[0xb0, 66, 127]))
        .unwrap();
    let other = f
        .emitted
        .iter()
        .position(|e| e.message == msg(&[0x90, 64, 100]))
        .unwrap();
    assert!(note < pedal && pedal < other);
}

#[shoop_test]
fn cleanup_retries_acknowledged_progress_with_one_output_message_per_callback() {
    let mut f = Fixture::new(16, 1);
    f.command(0, Operation::Record);
    f.run(&[event(0, &[0x90, 60, 90]), event(1, &[0xb0, 64, 127])]);
    f.command(0, Operation::Stop);
    f.run(&[]);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    assert_eq!(f.run(&[]).error, None);
    f.command(0, Operation::Stop);
    f.capacity = 1;
    f.emitted.clear();
    assert_no_heap(|| {
        assert_eq!(f.run(&[]).error, Some(Error::Capacity));
        assert_eq!(f.midi.output_state(0), Err(Error::Busy));
        assert_eq!(f.run(&[]).error, None);
    });
    assert!(f
        .emitted
        .iter()
        .map(|e| e.message)
        .eq([msg(&[0xb0, 64, 0]), msg(&[0x80, 60, 0])]));
    assert_eq!(
        f.midi
            .output_state(0)
            .unwrap()
            .channel(0)
            .unwrap()
            .note(60)
            .unwrap()
            .velocity(),
        0
    );
}

#[shoop_test]
fn failed_group_grabs_do_not_mutate_any_binding_or_leak_temporary_state() {
    let mut f = Fixture::new(16, 2);
    f.run(&[event(0, &[0xb0, 1, 20])]);
    f.run(&[]);
    f.pipeline
        .grab_midi(&mut f.midi, 0, 0, 16, &[0, 1])
        .unwrap();
    let old = f.midi.content(0).unwrap();
    let before = f.midi.store().usage();
    assert_eq!(
        f.pipeline.grab_midi(&mut f.midi, 0, 16, 32, &[0, 99]),
        Err(Error::InvalidState)
    );
    assert_eq!(f.midi.content(0).unwrap(), old);
    assert_eq!(f.midi.content(1).unwrap(), old);
    while f.midi.maintenance(32) != 0 {}
    assert_eq!(f.midi.store().usage(), before);
    f.run(&[]);
    for _ in 0..32 {
        let v = f.midi.begin_import(&MidiState::default()).unwrap();
        f.midi.release(v).unwrap();
    }
    let full = f.midi.store().usage();
    assert_eq!(
        f.pipeline.grab_midi(&mut f.midi, 0, 16, 32, &[0, 1]),
        Err(Error::Capacity)
    );
    assert_eq!(f.midi.store().usage(), full);
    assert_eq!(f.midi.content(0).unwrap(), old);
}

#[shoop_test]
fn heavy_history_changes_and_same_frame_events_produce_correct_start_state() {
    let mut f = Fixture::new(16, 1);
    f.run(&[
        event(0, &[0x90, 60, 80]),
        event(1, &[0xb0, 64, 127]),
        event(1, &[0x80, 60, 0]),
        event(7, &[0xb0, 66, 127]),
    ]);
    f.run(&[event(3, &[0xb0, 64, 0]), event(3, &[0x90, 65, 90])]);
    let view = f.midi.history_range(0, 8, 24).unwrap();
    let d = f.midi.store().describe(view).unwrap();
    let state = f.midi.store().state(d.initial).unwrap();
    assert_eq!(state.channel(0).unwrap().note(60).unwrap().velocity(), 80);
    assert!(!state.channel(0).unwrap().note(60).unwrap().held());
    assert_eq!(state.channel(0).unwrap().controller(64), Some(127));
    assert_eq!(f.midi.store().read(view, 0).unwrap().unwrap().frame, 11);
    f.midi.release(view).unwrap();
    let before = f.midi.history_range(0, 19, 35);
    assert_eq!(before, Err(Error::InvalidEvent));
}

#[shoop_test]
fn callback_partitioning_keeps_recorded_order_and_playback_offsets() {
    let mut a = Fixture::new(16, 1);
    let mut b = Fixture::new(16, 1);
    a.command(0, Operation::Record);
    b.command(0, Operation::Record);
    a.run_frames(
        &[
            event(15, &[0xb0, 1, 50]),
            event(16, &[0xb0, 1, 60]),
            event(31, &[0xb0, 1, 70]),
        ],
        32,
    );
    b.run(&[event(15, &[0xb0, 1, 50])]);
    b.run(&[event(0, &[0xb0, 1, 60]), event(15, &[0xb0, 1, 70])]);
    a.command(0, Operation::Stop);
    b.command(0, Operation::Stop);
    a.run(&[]);
    b.run(&[]);
    a.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    b.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    a.run_frames(&[], 32);
    b.run(&[]);
    let split = b.emitted.len();
    b.run(&[]);
    for e in &mut b.emitted[split..] {
        e.offset += 16;
    }
    assert_eq!(a.emitted, b.emitted);
}

#[shoop_test]
fn clearing_content_reclaims_old_events_after_acknowledged_stop() {
    let mut f = Fixture::new(16, 2);
    let base = f.midi.store().usage();
    f.run(&[event(0, &[0xb0, 1, 50])]);
    f.run(&[]);
    f.pipeline
        .grab_midi(&mut f.midi, 0, 0, 16, &[0, 1])
        .unwrap();
    let view = f.midi.content(0).unwrap().unwrap();
    f.pipeline.clear_midi(&mut f.midi, &[0, 1]).unwrap();
    f.run(&[]);
    while f.midi.maintenance(32) != 0 {}
    assert_eq!(f.midi.content(0).unwrap(), None);
    assert_eq!(f.midi.store().describe(view), Err(Error::InvalidState));
    assert_eq!(f.midi.store().usage().views, base.views);
}
