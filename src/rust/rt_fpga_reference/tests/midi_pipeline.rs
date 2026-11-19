#[path = "support/midi_session.rs"]
pub mod support;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
use support::*;
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;

#[shoop_test]
fn independent_record_play_repeat_stop_trace_preserves_audio_at_all_quanta() {
    for q in [1, 16, 32, 64, 128] {
        let mut f = Fixture::new(q, 1);
        f.run(&[
            event(0, &[0xb0, 0, 3]),
            event(0, &[0xb0, 32, 4]),
            event(0, &[0xc0, 9]),
            event(0, &[0xb0, 0, 8]),
            event(0, &[0xb0, 64, 127]),
            event(0, &[0x90, 48, 60]),
            event(0, &[0x80, 48, 0]),
        ]);
        f.command(0, Operation::Record);
        let first = [
            event(0, &[0x90, 60, 10]),
            event(0, &[0x90, 60, 20]),
            event(q - 1, &[0x90, 60, 0]),
        ];
        let second = [event(0, &[0xb0, 64, 0]), event(q - 1, &[0xa0, 61, 5])];
        assert_eq!(f.run(&first).error, None);
        assert_eq!(f.run(&second).error, None);
        f.command(0, Operation::Stop);
        assert_eq!(f.run(&[]).error, None);
        let view = f.midi.content(0).unwrap().unwrap();
        let d = f.midi.store().describe(view).unwrap();
        assert_eq!((d.frames, d.events), (u64::from(q) * 2, 5));
        assert!(f.emitted.is_empty());
        f.command(
            0,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
        assert_eq!(f.run(&[]).error, None);
        let split = f.emitted.len();
        assert_eq!(f.run(&[]).error, None);
        let expected = [
            msg(&[0xb0, 0, 3]),
            msg(&[0xb0, 32, 4]),
            msg(&[0xc0, 9]),
            msg(&[0xb0, 0, 8]),
            msg(&[0xb0, 64, 127]),
            msg(&[0x90, 48, 60]),
            msg(&[0x80, 48, 0]),
        ];
        assert!(f.emitted[..7].iter().map(|e| e.message).eq(expected));
        assert!(f.emitted[7..split]
            .iter()
            .map(|e| e.message)
            .eq(first.map(|e| e.message)));
        assert!(f.emitted[split..]
            .iter()
            .map(|e| e.message)
            .eq(second.map(|e| e.message)));
        f.emitted.clear();
        assert_eq!(f.run(&[]).error, None);
        assert!(f.emitted.iter().all(|e| e.message.as_bytes()[0] != 0xc0));
        assert!(f.emitted.iter().any(|e| e.message == msg(&[0x90, 48, 60])));
        f.command(0, Operation::Stop);
        assert_eq!(f.run(&[]).error, None);
        assert_eq!(
            f.midi
                .output_state(0)
                .unwrap()
                .channel(0)
                .unwrap()
                .note(48)
                .unwrap()
                .velocity(),
            0
        );
        f.command(0, Operation::Record);
        f.run(&[]);
        f.command(0, Operation::Stop);
        assert_eq!(f.run(&[]).error, None);
        assert_eq!(
            f.midi
                .store()
                .describe(f.midi.content(0).unwrap().unwrap())
                .unwrap()
                .events,
            0
        );
    }
}

#[shoop_test]
fn split_and_multi_quantum_calls_have_identical_midi_and_loop_results() {
    for q in [1, 16, 32, 64, 128] {
        let mut a = Fixture::new(q, 1);
        let mut b = Fixture::new(q, 1);
        let events = [
            event(0, &[0x90, 60, 90]),
            event(q - 1, &[0xb0, 64, 127]),
            event(q, &[0x80, 60, 0]),
            event(2 * q - 1, &[0xb0, 64, 0]),
        ];
        a.command(0, Operation::Record);
        b.command(0, Operation::Record);
        assert_no_heap(|| {
            assert_eq!(a.run_frames(&events, 2 * q as usize).error, None);
        });
        b.run(&events[..2]);
        let tail = [events[2], events[3]].map(|e| InputEvent {
            offset: e.offset - q,
            ..e
        });
        b.run(&tail);
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
        a.run_frames(&[], 2 * q as usize);
        b.run(&[]);
        let split = b.emitted.len();
        b.run(&[]);
        for e in &mut b.emitted[split..] {
            e.offset += q;
        }
        assert_eq!(a.emitted, b.emitted);
        assert_eq!(
            a.pipeline.state().loop_state(id(0)).unwrap().state,
            b.pipeline.state().loop_state(id(0)).unwrap().state
        );
    }
}

#[shoop_test]
fn output_exhaustion_uses_acknowledged_recovery_and_releases_take_after_stop() {
    let mut f = Fixture::new(16, 1);
    f.command(0, Operation::Record);
    f.run(&[event(0, &[0x90, 60, 99])]);
    f.command(0, Operation::Stop);
    f.run(&[]);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    f.capacity = 0;
    assert_eq!(f.run(&[]).error, Some(Error::Capacity));
    assert!(f.midi.recovery_pending(0).unwrap());
    f.command(0, Operation::Stop);
    f.capacity = 1;
    f.emitted.clear();
    for i in 0..32 {
        f.run(&[]);
        assert_eq!(f.emitted.len(), i + 1);
        assert_eq!(
            f.emitted[i].message,
            msg(&[0xb0 | (i / 2) as u8, if i % 2 == 0 { 120 } else { 123 }, 0])
        );
    }
    assert!(!f.midi.recovery_pending(0).unwrap());
    f.capacity = usize::MAX;
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    assert_eq!(f.run(&[]).error, None);
    assert_eq!(f.emitted.last().unwrap().message, msg(&[0x90, 60, 99]));
}

#[shoop_test]
fn incomplete_capture_cannot_play_and_does_not_interrupt_audio_or_input_tracking() {
    let mut f = Fixture::new(16, 1);
    f.midi.set_record_limit(0, 1).unwrap();
    f.command(0, Operation::Record);
    assert_eq!(
        f.run(&[event(0, &[0x90, 60, 99]), event(15, &[0x80, 60, 0])])
            .error,
        Some(Error::Capacity)
    );
    assert_eq!(f.midi.take_status(0).unwrap(), TakeStatus::Incomplete);
    assert!(!f
        .midi
        .input_state(0)
        .unwrap()
        .channel(0)
        .unwrap()
        .note(60)
        .unwrap()
        .held());
    f.command(0, Operation::Stop);
    assert_eq!(f.run(&[]).error, Some(Error::Incomplete));
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    assert_eq!(f.run(&[]).error, Some(Error::Incomplete));
    assert!(f.emitted.is_empty());
    f.command(0, Operation::Record);
    assert_eq!(f.run(&[]).error, None);
    f.command(0, Operation::Stop);
    assert_eq!(f.run(&[]).error, None);
}

#[shoop_test]
fn bad_windows_and_unsupported_seeks_are_refused_before_clock_or_midi_mutation() {
    let mut f = Fixture::new(16, 1);
    assert_eq!(
        f.try_run_frames(&[event(16, &[0x90, 60, 1])], 16),
        Err(ProcessError::Midi(Error::InvalidEvent))
    );
    assert_eq!(f.pipeline.state().frame(), 0);
    f.command(0, Operation::Record);
    f.run(&[]);
    f.run(&[]);
    f.command(0, Operation::Stop);
    f.run(&[]);
    let before = f.pipeline.state().frame();
    f.command(
        0,
        Operation::Play {
            position: 16,
            retrigger: true,
        },
    );
    assert_eq!(
        f.try_run_frames(&[], 16),
        Err(ProcessError::Midi(Error::Unsupported))
    );
    assert_eq!(f.pipeline.state().frame(), before);
}

#[shoop_test]
fn refused_note_off_triggers_recovery_and_never_replays_a_delayed_event() {
    let mut f = Fixture::new(32, 1);
    f.command(0, Operation::Record);
    f.run(&[event(0, &[0x90, 60, 80]), event(31, &[0x80, 60, 0])]);
    f.command(0, Operation::Stop);
    f.run(&[]);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    f.capacity = 1;
    assert_eq!(f.run(&[]).error, Some(Error::Capacity));
    assert_eq!(f.emitted.last().unwrap().message, msg(&[0x90, 60, 80]));
    let before = f.emitted.len();
    f.command(0, Operation::Stop);
    f.capacity = usize::MAX;
    assert_eq!(f.run(&[]).error, None);
    assert_eq!(f.emitted.len() - before, 32);
    assert!(f.emitted[before..]
        .iter()
        .all(|e| e.message.as_bytes()[0] & 0xf0 == 0xb0));
    assert!(!f.midi.recovery_pending(0).unwrap());
}

#[shoop_test]
fn recovery_preserves_pedal_ownership_until_stop_releases_it() {
    let mut f = Fixture::new(16, 1);
    f.command(0, Operation::Record);
    f.run(&[
        event(0, &[0xb0, 64, 127]),
        event(1, &[0x90, 60, 80]),
        event(15, &[0x80, 60, 0]),
    ]);
    f.command(0, Operation::Stop);
    f.run(&[]);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    f.capacity = 2;
    assert_eq!(f.run(&[]).error, Some(Error::Capacity));
    let before = f.emitted.len();
    f.command(0, Operation::Stop);
    f.capacity = usize::MAX;
    assert_eq!(f.run(&[]).error, None);
    assert_eq!(f.emitted.len() - before, 33);
    assert_eq!(f.emitted.last().unwrap().message, msg(&[0xb0, 64, 0]));
}

#[shoop_test]
fn a_partially_restored_pedal_is_owned_until_recovery_and_stop_finish() {
    let mut f = Fixture::new(16, 1);
    f.run(&[
        event(0, &[0xb0, 64, 127]),
        event(0, &[0x90, 60, 80]),
        event(0, &[0x80, 60, 0]),
    ]);
    f.command(0, Operation::Record);
    f.run(&[]);
    f.command(0, Operation::Stop);
    f.run(&[]);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    f.capacity = 2;
    assert_eq!(f.run(&[]).error, Some(Error::Capacity));
    assert_eq!(f.emitted.last().unwrap().message, msg(&[0x90, 60, 80]));
    let before = f.emitted.len();
    f.command(0, Operation::Stop);
    f.capacity = usize::MAX;
    assert_eq!(f.run(&[]).error, None);
    assert_eq!(f.emitted.len() - before, 33);
    assert_eq!(f.emitted.last().unwrap().message, msg(&[0xb0, 64, 0]));
}

#[shoop_test]
fn cleanup_resumes_one_message_at_a_time_without_restarting_recovery() {
    let mut f = Fixture::new(16, 1);
    f.command(0, Operation::Record);
    f.run(&[
        event(0, &[0x90, 60, 80]),
        event(0, &[0xb0, 64, 127]),
        event(0, &[0xb0, 66, 127]),
        event(0, &[0xb0, 69, 127]),
    ]);
    f.command(0, Operation::Stop);
    f.run(&[]);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    f.run(&[]);
    f.command(0, Operation::Stop);
    f.capacity = 1;
    f.emitted.clear();
    for i in 0..4 {
        let report = f.run(&[]);
        assert_eq!(
            report.error,
            if i == 3 { None } else { Some(Error::Capacity) }
        );
        assert!(!f.midi.recovery_pending(0).unwrap());
    }
    assert!(f.emitted.iter().map(|e| e.message).eq([
        msg(&[0xb0, 64, 0]),
        msg(&[0xb0, 66, 0]),
        msg(&[0xb0, 69, 0]),
        msg(&[0x80, 60, 0])
    ]));
}

#[shoop_test]
fn recording_directly_from_playback_releases_the_previous_output_notes() {
    let mut f = Fixture::new(16, 1);
    f.command(0, Operation::Record);
    f.run(&[event(0, &[0x90, 60, 80])]);
    f.command(0, Operation::Stop);
    f.run(&[]);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    f.run(&[]);
    let before = f.emitted.len();
    f.command(0, Operation::Record);
    assert_eq!(f.run(&[event(0, &[0x90, 61, 90])]).error, None);
    assert_eq!(f.emitted.len() - before, 1);
    assert_eq!(f.emitted.last().unwrap().message, msg(&[0x80, 60, 0]));
}

#[shoop_test]
fn silent_takes_repeat_and_input_budget_refusal_preserves_the_clock() {
    let mut l = limits(1);
    l.input_events_per_quantum = 1;
    let mut f = Fixture::with_limits(16, l);
    assert_eq!(
        f.try_run_frames(&[event(0, &[0x90, 60, 80]), event(1, &[0x80, 60, 0])], 16),
        Err(ProcessError::Midi(Error::Capacity))
    );
    assert_eq!(f.pipeline.state().frame(), 0);
    f.command(0, Operation::Record);
    f.run(&[]);
    f.command(0, Operation::Stop);
    f.run(&[]);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    for _ in 0..5 {
        assert_eq!(f.run(&[]).error, None);
    }
    assert!(f.emitted.is_empty());
}

#[shoop_test]
fn a_fatal_audio_fault_silences_audio_and_leaves_midi_recovery_drainable() {
    use rt_fpga_reference::{jobs::Memory, storage::ViewId as AudioView};
    #[derive(Default)]
    struct Fault(core::cell::Cell<u32>);
    impl Memory for Fault {
        fn chunk_frames(&self) -> usize {
            1
        }
        fn begin_capture(&mut self, _: AudioView) -> Result<(), rt_fpga_reference::Error> {
            Ok(())
        }
        fn read(
            &mut self,
            _: AudioView,
            _: u64,
            _: u64,
            _: &mut [Fixed],
        ) -> Result<(), rt_fpga_reference::Error> {
            Ok(())
        }
        fn write(&mut self, _: AudioView, _: &[Fixed]) -> Result<(), rt_fpga_reference::Error> {
            Ok(())
        }
        fn fault(&self) -> Option<rt_fpga_reference::Error> {
            let n = self.0.get();
            self.0.set(n + 1);
            (n != 0).then_some(rt_fpga_reference::Error::MemoryFull)
        }
    }
    let mut f = Fixture::new(16, 1);
    f.command(0, Operation::Record);
    f.run(&[event(0, &[0x90, 60, 80])]);
    f.command(0, Operation::Stop);
    f.run(&[]);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    f.run(&[]);
    let result = assert_no_heap(|| {
        f.pipeline.process_midi_session(
            &mut f.jobs,
            &mut Fault::default(),
            &[&f.input[..16]],
            &mut [&mut f.output[..16]],
            16,
            &mut f.midi,
            &[],
            &mut |_| Ok(()),
        )
    });
    assert!(matches!(result, Err(ProcessError::Pipeline(_))));
    assert!(f.output[..16].iter().all(|s| *s == Fixed::ZERO));
    assert!(f.midi.recovery_pending(0).unwrap());
    f.output[..16].fill(Fixed(7));
    let again = f.pipeline.process_midi_session(
        &mut f.jobs,
        &mut Fault::default(),
        &[&f.input[..16]],
        &mut [&mut f.output[..16]],
        16,
        &mut f.midi,
        &[],
        &mut |_| Ok(()),
    );
    assert_eq!(again, result);
    assert!(f.output[..16].iter().all(|s| *s == Fixed::ZERO));
    let mut count = 0;
    assert_no_heap(|| {
        f.midi.drain_recovery(0, &mut |_| {
            count += 1;
            Ok(())
        })
    })
    .unwrap();
    assert_eq!(count, 64);
}

#[shoop_test]
fn synchronization_wait_releases_notes_and_resumes_only_on_the_source_pulse() {
    let mut f = Fixture::new(16, 1);
    let plan = TransitionPlan::compile(
        PlanDescription {
            quantum: 16,
            loops: vec![
                LoopSpec {
                    id: id(0),
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: Some(id(1)),
                    repeat_sync: true,
                    timeline: None,
                },
                LoopSpec {
                    id: id(1),
                    kind: LoopKind::Primitive,
                    length: 96,
                    sync_source: None,
                    repeat_sync: false,
                    timeline: None,
                },
            ],
        },
        TransitionLimits {
            loop_slots: 2,
            events: 8,
        },
    )
    .unwrap();
    let program = Program {
        sync_instructions: vec![],
        quantum: 16,
        chunk_frames: 1,
        jobs_per_quantum: 2,
        instructions: vec![
            Instruction::ProcessInput {
                port: 0,
                destination: BufferId(0),
            },
            Instruction::ProcessOutput {
                source: BufferId(0),
                port: 0,
            },
        ],
    };
    f.pipeline = Pipeline::prepare(plan, program, vec![], 2).unwrap();
    f.command(
        1,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    f.command(0, Operation::Record);
    f.run(&[event(0, &[0x90, 60, 80])]);
    f.command(0, Operation::Stop);
    f.run(&[]);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    f.run(&[]);
    let before = f.emitted.len();
    for _ in 0..3 {
        assert_eq!(f.run(&[]).error, None);
    }
    assert_eq!(f.emitted.len(), before + 1);
    assert_eq!(f.emitted.last().unwrap().message, msg(&[0x80, 60, 0]));
    assert_eq!(f.run(&[]).error, None);
    assert_eq!(f.emitted.last().unwrap().message, msg(&[0x90, 60, 80]));
}
