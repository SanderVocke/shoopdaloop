#[path = "support/midi_session.rs"]
pub mod support;
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

#[shoop_test]
fn session_records_128_loops_and_grabs_all_of_them_while_transfers_and_history_continue() {
    let evidence = support::dense_session_workload(3000, |operation| assert_no_heap(operation));
    assert_eq!(evidence.events_per_loop, 3000);
    assert!(evidence.grab_replay_steps <= 31);
    assert_eq!(evidence.final_usage.views, 128);
}

#[shoop_test]
fn control_transfer_grants_are_shared_and_cancelled_imports_release_all_owners() {
    let mut f = Fixture::new(16, 1);
    f.run(&[]);
    let initial = f.midi.store().usage();
    let view = f.midi.begin_import(&MidiState::default()).unwrap();
    let mut data = core::array::from_fn::<Record, 64, _>(|i| Record {
        frame: i as u64,
        order: i as u32,
        message: msg(&[0xb0, 1, i as u8]),
    });
    assert_no_heap(|| {
        assert_eq!(f.midi.import_events(view, &data).unwrap(), 64);
        data[0].frame = 64;
        assert_eq!(f.midi.import_events(view, &data[..1]), Err(Error::Busy));
        assert_eq!(f.midi.store().describe(view).unwrap().events, 64);
        f.run(&[]);
        f.midi.import_events(view, &data[..1]).unwrap();
        let before = f.midi.work().comparison_work;
        for _ in 0..20 {
            assert!(!f.midi.finish_import(view, 80, 256).unwrap());
        }
        assert!(f.midi.work().comparison_work - before <= 256);
        f.midi.release(view).unwrap();
        while f.midi.maintenance(32) != 0 {}
    });
    assert_eq!(f.midi.store().usage(), initial);
}

#[shoop_test]
fn background_import_reserves_capacity_for_admitted_capture_and_history() {
    let mut l = limits(1);
    l.store.blocks = 12;
    l.store.max_events = 16;
    let mut f = Fixture::with_limits(16, l);
    f.command(0, Operation::Record);
    assert_eq!(f.run(&[]).error, None);
    let a = f.midi.begin_import(&MidiState::default()).unwrap();
    let b = f.midi.begin_import(&MidiState::default()).unwrap();
    let data = core::array::from_fn::<Record, 16, _>(|i| Record {
        frame: i as u64,
        order: i as u32,
        message: msg(&[0xb0, 1, i as u8]),
    });
    f.midi.import_events(a, &data).unwrap();
    assert_eq!(f.midi.import_events(b, &data), Err(Error::Capacity));
    assert_eq!(f.midi.store().describe(b).unwrap().events, 0);
    let input = core::array::from_fn::<InputEvent, 16, _>(|i| event(i as u32, &[0xb0, 2, i as u8]));
    assert_no_heap(|| {
        assert_eq!(f.run(&input).error, None);
    });
    f.command(0, Operation::Stop);
    assert_eq!(f.run(&[]).error, None);
    assert_eq!(f.midi.take_status(0).unwrap(), TakeStatus::Complete);
    f.midi.release(a).unwrap();
    f.midi.release(b).unwrap();
}

#[shoop_test]
fn matching_state_needs_no_restoration_and_unsupported_channel_overlap_emits_nothing() {
    let mut f = Fixture::new(16, 1);
    let mut target = MidiState::default();
    target.apply_packet(&[0xb0, 1, 40], 3).unwrap();
    f.midi
        .send(
            OutputEvent {
                destination: 0,
                offset: 0,
                message: msg(&[0xb0, 1, 40]),
            },
            &mut |_| Ok(()),
        )
        .unwrap();
    let view = f.midi.begin_import(&target).unwrap();
    while !f.midi.finish_import(view, 16, 256).unwrap() {}
    f.pipeline.replace_midi(&mut f.midi, &[0], view).unwrap();
    f.midi.release(view).unwrap();
    let before = f.midi.work().comparison_work;
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    assert_eq!(f.run(&[]).error, None);
    assert!(f.emitted.is_empty());
    assert_eq!(f.midi.work().comparison_work, before);
    let mut f = Fixture::new(16, 2);
    f.midi = MidiSession::prepare(limits(2)).unwrap();
    for i in 0..2 {
        f.midi
            .bind(
                i,
                Binding {
                    loop_id: id(i as usize),
                    input: 0,
                    destination: 0,
                    channels: 1,
                },
            )
            .unwrap();
    }
    f.run(&[]);
    f.pipeline
        .grab_midi(&mut f.midi, 0, 0, 16, &[0, 1])
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
    assert_eq!(f.run(&[]).error, Some(Error::Unsupported));
    assert!(f.emitted.is_empty());
}

#[shoop_test]
fn channel_handoff_cleans_the_old_owner_before_restoring_the_new_owner() {
    let mut f = Fixture::new(16, 2);
    f.midi = MidiSession::prepare(limits(2)).unwrap();
    for i in 0..2 {
        f.midi
            .bind(
                i,
                Binding {
                    loop_id: id(i as usize),
                    input: 0,
                    destination: 0,
                    channels: 1,
                },
            )
            .unwrap();
    }
    let mut target = MidiState::default();
    target.apply_packet(&[0x90, 60, 80], 3).unwrap();
    let new = f.midi.begin_import(&target).unwrap();
    while !f.midi.finish_import(new, 32, 256).unwrap() {}
    let old = f.midi.begin_import(&MidiState::default()).unwrap();
    f.midi
        .import_events(
            old,
            &[Record {
                frame: 0,
                order: 0,
                message: msg(&[0x90, 60, 40]),
            }],
        )
        .unwrap();
    while !f.midi.finish_import(old, 32, 256).unwrap() {}
    f.pipeline.replace_midi(&mut f.midi, &[0], new).unwrap();
    f.pipeline.replace_midi(&mut f.midi, &[1], old).unwrap();
    f.midi.release(new).unwrap();
    f.midi.release(old).unwrap();
    f.command(
        1,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    assert_eq!(f.run(&[]).error, None);
    f.emitted.clear();
    f.command(1, Operation::Stop);
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
    assert!(f
        .emitted
        .iter()
        .map(|e| e.message)
        .eq([msg(&[0x80, 60, 0]), msg(&[0x90, 60, 80])]));
    assert_eq!(
        f.midi
            .output_state(0)
            .unwrap()
            .channel(0)
            .unwrap()
            .note(60)
            .unwrap()
            .velocity(),
        80
    );
}

#[shoop_test]
fn rejected_channel_overlap_does_not_drop_the_established_owners_note_off() {
    let mut f = Fixture::new(16, 2);
    f.midi = MidiSession::prepare(limits(2)).unwrap();
    for i in 0..2 {
        f.midi
            .bind(
                i,
                Binding {
                    loop_id: id(i as usize),
                    input: 0,
                    destination: 0,
                    channels: 1,
                },
            )
            .unwrap();
    }
    let view = f.midi.begin_import(&MidiState::default()).unwrap();
    f.midi
        .import_events(
            view,
            &[
                Record {
                    frame: 0,
                    order: 0,
                    message: msg(&[0x90, 60, 40]),
                },
                Record {
                    frame: 20,
                    order: 1,
                    message: msg(&[0x80, 60, 0]),
                },
            ],
        )
        .unwrap();
    while !f.midi.finish_import(view, 32, 256).unwrap() {}
    f.pipeline.replace_midi(&mut f.midi, &[0, 1], view).unwrap();
    f.midi.release(view).unwrap();
    f.command(
        1,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    assert_eq!(f.run(&[]).error, None);
    f.emitted.clear();
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    assert_eq!(f.run(&[]).error, Some(Error::Unsupported));
    assert_eq!(f.emitted.len(), 1);
    assert_eq!(f.emitted[0].message, msg(&[0x80, 60, 0]));
    assert_eq!(f.emitted[0].offset, 4);
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
fn least_prepared_history_start_replays_31_distinct_controls_without_a_full_comparison() {
    let mut f = Fixture::new(16, 1);
    let controllers =
        core::array::from_fn::<u8, 31, _>(|i| if i < 5 { i as u8 + 1 } else { i as u8 + 2 });
    let a = core::array::from_fn::<InputEvent, 16, _>(|i| {
        event(i as u32, &[0xb0, controllers[i], 40 + i as u8])
    });
    let b = core::array::from_fn::<InputEvent, 15, _>(|i| {
        event(i as u32, &[0xb0, controllers[i + 16], 56 + i as u8])
    });
    f.run(&a);
    f.run(&b);
    f.run(&[]);
    let before = f.midi.work();
    assert_no_heap(|| {
        f.pipeline.grab_midi(&mut f.midi, 0, 31, 47, &[0]).unwrap();
    });
    let after = f.midi.work();
    assert_eq!(after.replay_steps - before.replay_steps, 31);
    assert!(after.comparison_work - before.comparison_work < 500);
    let view = f.midi.content(0).unwrap().unwrap();
    let state = f
        .midi
        .store()
        .state(f.midi.store().describe(view).unwrap().initial)
        .unwrap();
    for (i, &cc) in controllers.iter().enumerate() {
        assert_eq!(state.channel(0).unwrap().controller(cc), Some(40 + i as u8));
    }
}

#[shoop_test]
fn cold_grab_replaces_128_full_sequence_views_without_releasing_their_tables_inline() {
    let evidence = support::repeated_grab_workload(3, |operation| assert_no_heap(operation));
    assert_eq!(evidence.max_replay_steps, 31);
    assert!(evidence.max_comparison_work < 1000);
    assert!(evidence.max_storage_work < 12_000);
}

#[shoop_test]
fn plain_note_differences_preserve_matching_notes_and_order_velocity_replacement() {
    for replace_velocity in [false, true] {
        let mut f = Fixture::new(16, 1);
        let mut target = MidiState::default();
        target
            .apply_packet(&[0x90, 60, if replace_velocity { 100 } else { 80 }], 3)
            .unwrap();
        if !replace_velocity {
            target.apply_packet(&[0x90, 67, 90], 3).unwrap();
        }
        f.midi
            .send(
                OutputEvent {
                    destination: 0,
                    offset: 0,
                    message: msg(&[0x90, 60, 80]),
                },
                &mut |_| Ok(()),
            )
            .unwrap();
        if !replace_velocity {
            f.midi
                .send(
                    OutputEvent {
                        destination: 0,
                        offset: 0,
                        message: msg(&[0x90, 64, 70]),
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
        let expected = if replace_velocity {
            [msg(&[0x80, 60, 0]), msg(&[0x90, 60, 100])]
        } else {
            [msg(&[0x80, 64, 0]), msg(&[0x90, 67, 90])]
        };
        assert!(f.emitted.iter().map(|e| e.message).eq(expected));
    }
}
