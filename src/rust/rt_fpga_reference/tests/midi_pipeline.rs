use rt_fpga_reference::{
    fixed::Fixed,
    jobs::{BufferId, JobProcessor},
    midi::{Engine, Error, Event, Message, ProcessError, Report, TakeStatus},
    pipeline::Pipeline,
    records::Identity,
    state::{Instruction, PlannedTransition, Program},
    storage::MediaStore,
    transitions::*,
};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;

const ID: Identity = Identity {
    slot: 0,
    generation: 7,
};
fn msg(bytes: &[u8]) -> Message {
    Message::managed(bytes).unwrap()
}
fn event(offset: u32, bytes: &[u8]) -> Event {
    Event {
        offset,
        message: msg(bytes),
    }
}

struct Fixture {
    pipeline: Pipeline,
    jobs: JobProcessor,
    media: MediaStore,
    midi: Engine,
    q: usize,
    sequence: u64,
    input: Vec<Fixed>,
    output: Vec<Fixed>,
    emitted: Vec<(u64, Message)>,
    limit: usize,
}
impl Fixture {
    fn new(q: u32, capacity: u32) -> Self {
        let plan = TransitionPlan::compile(
            PlanDescription {
                quantum: q,
                loops: vec![LoopSpec {
                    id: ID,
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: None,
                    repeat_sync: false,
                    timeline: None,
                }],
            },
            TransitionLimits {
                loop_slots: 1,
                events: 8,
            },
        )
        .unwrap();
        let program = Program {
            sync_instructions: vec![],
            quantum: q,
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
        Self {
            pipeline: Pipeline::prepare(plan, program, vec![], 2).unwrap(),
            jobs: JobProcessor::new(q as usize, 1),
            media: MediaStore::new(1).unwrap(),
            midi: Engine::prepare(ID, capacity, 16).unwrap(),
            q: q as usize,
            sequence: 0,
            input: (0..q * 2).map(|i| Fixed(i as i32 - 100)).collect(),
            output: vec![Fixed::ZERO; q as usize * 2],
            emitted: Vec::with_capacity(20000),
            limit: usize::MAX,
        }
    }
    fn command(&mut self, operation: Operation) {
        self.sequence += 1;
        self.pipeline
            .write_plans(
                ID,
                [
                    Some(PlannedTransition {
                        sequence: self.sequence,
                        operation,
                        on_pulse: false,
                        effective: self.pipeline.state().frame(),
                        remaining_ticks: 0,
                    }),
                    None,
                ],
            )
            .unwrap();
    }
    fn run(&mut self, frames: usize, events: &[Event]) -> Result<Report, ProcessError> {
        let start = self.pipeline.state().frame();
        let before = self.emitted.len();
        let limit = self.limit;
        let emitted = &mut self.emitted;
        self.pipeline.process_midi_block(
            &mut self.jobs,
            &mut self.media,
            &[&self.input[..frames]],
            &mut [&mut self.output[..frames]],
            frames,
            &mut self.midi,
            events,
            &mut |event| {
                if emitted.len() - before == limit {
                    return Err(Error::Capacity);
                }
                emitted.push((start + u64::from(event.offset), event.message));
                Ok(())
            },
        )
    }
    fn tick(&mut self, events: &[Event]) -> Report {
        let report = assert_no_heap(|| self.run(self.q, events)).unwrap();
        assert_eq!(&self.output[..self.q], &self.input[..self.q]);
        report
    }
}

fn seed_restore() -> Vec<Message> {
    let mut messages = vec![
        msg(&[0xb0, 0, 3]),
        msg(&[0xb0, 32, 4]),
        msg(&[0xc0, 9]),
        msg(&[0xb0, 0, 8]),
        msg(&[0xe0, 0, 64]),
        msg(&[0xb0, 64, 127]),
        msg(&[0xb0, 69, 0]),
        msg(&[0xb0, 66, 0]),
        msg(&[0x90, 48, 60]),
        msg(&[0x80, 48, 0]),
    ];
    for ch in 1..16 {
        messages.extend([
            msg(&[0xe0 | ch, 0, 64]),
            msg(&[0xb0 | ch, 64, 0]),
            msg(&[0xb0 | ch, 69, 0]),
            msg(&[0xb0 | ch, 66, 0]),
        ]);
    }
    messages
}

#[shoop_test]
fn independent_record_play_repeat_stop_trace_preserves_audio_at_all_quanta() {
    for q in [1, 16, 32, 64, 128] {
        let mut f = Fixture::new(q, 32);
        let seed = [
            event(0, &[0xb0, 0, 3]),
            event(0, &[0xb0, 32, 4]),
            event(0, &[0xc0, 9]),
            event(0, &[0xb0, 0, 8]),
            event(0, &[0xb0, 64, 127]),
            event(0, &[0x90, 48, 60]),
            event(0, &[0x80, 48, 0]),
        ];
        assert_eq!(f.tick(&seed).error, None);
        f.command(Operation::Record);
        let first = [
            event(0, &[0x90, 60, 10]),
            event(0, &[0x90, 60, 20]),
            event(q - 1, &[0x90, 60, 0]),
        ];
        assert_eq!(f.tick(&first).error, None);
        let second = [event(0, &[0xb0, 64, 0]), event(q - 1, &[0xa0, 61, 5])];
        assert_eq!(f.tick(&second).error, None);
        f.command(Operation::Stop);
        assert_eq!(f.tick(&[]).error, None);
        assert_eq!(f.midi.take().frames(), u64::from(q) * 2);
        assert_eq!(f.midi.take().records().len(), 5);
        assert_eq!(
            f.midi.take().initial().channel(0).unwrap().controller(64),
            Some(127)
        );
        assert!(f.emitted.is_empty());
        f.command(Operation::Play {
            position: 0,
            retrigger: true,
        });
        let base = f.pipeline.state().frame();
        assert_eq!(f.tick(&[]).error, None);
        assert_eq!(f.tick(&[]).error, None);
        let mut expected: Vec<_> = seed_restore().into_iter().map(|m| (base, m)).collect();
        expected.extend(first.map(|e| (base + u64::from(e.offset), e.message)));
        expected.extend(second.map(|e| (base + u64::from(q) + u64::from(e.offset), e.message)));
        assert_eq!(f.emitted, expected);
        let repeat = base + u64::from(q) * 2;
        assert_eq!(f.tick(&[]).error, None);
        expected.extend(seed_restore().into_iter().map(|m| (repeat, m)));
        expected.extend(first.map(|e| (repeat + u64::from(e.offset), e.message)));
        f.command(Operation::Stop);
        assert_eq!(f.tick(&[]).error, None);
        let stop = repeat + u64::from(q);
        expected.extend(
            [
                msg(&[0xb0, 64, 0]),
                msg(&[0x80, 48, 0]),
                msg(&[0x80, 60, 0]),
            ]
            .map(|m| (stop, m)),
        );
        assert_eq!(f.emitted, expected);
        f.command(Operation::Record);
        assert_eq!(f.tick(&[]).error, None);
        f.command(Operation::Stop);
        assert_eq!(f.tick(&[]).error, None);
        assert_eq!(f.midi.take().status(), TakeStatus::Complete);
        assert!(f.midi.take().records().is_empty());
    }
}

#[shoop_test]
fn split_and_multi_quantum_calls_have_identical_midi_and_loop_results() {
    for q in [1, 16, 32, 64, 128] {
        let mut a = Fixture::new(q, 16);
        let mut b = Fixture::new(q, 16);
        let events = [
            event(0, &[0x90, 60, 90]),
            event(q - 1, &[0xb0, 64, 127]),
            event(q, &[0x80, 60, 0]),
            event(q * 2 - 1, &[0xb0, 64, 0]),
        ];
        a.command(Operation::Record);
        b.command(Operation::Record);
        assert_eq!(
            assert_no_heap(|| a.run(q as usize * 2, &events))
                .unwrap()
                .error,
            None
        );
        assert_eq!(b.tick(&events[..2]).error, None);
        let tail = [events[2], events[3]].map(|e| Event {
            offset: e.offset - q,
            message: e.message,
        });
        assert_eq!(b.tick(&tail).error, None);
        a.command(Operation::Stop);
        b.command(Operation::Stop);
        a.tick(&[]);
        b.tick(&[]);
        a.command(Operation::Play {
            position: 0,
            retrigger: true,
        });
        b.command(Operation::Play {
            position: 0,
            retrigger: true,
        });
        assert_eq!(
            assert_no_heap(|| a.run(q as usize * 2, &[])).unwrap().error,
            None
        );
        b.tick(&[]);
        b.tick(&[]);
        assert_eq!(a.emitted, b.emitted);
        assert_eq!(a.midi.take().records(), b.midi.take().records());
        assert_eq!(
            a.pipeline.state().loop_state(ID).unwrap().state,
            b.pipeline.state().loop_state(ID).unwrap().state
        );
    }
}

#[shoop_test]
fn output_exhaustion_uses_acknowledged_recovery_and_releases_take_after_stop() {
    let mut f = Fixture::new(16, 4);
    f.command(Operation::Record);
    f.tick(&[event(0, &[0x90, 60, 99])]);
    f.command(Operation::Stop);
    f.tick(&[]);
    f.command(Operation::Play {
        position: 0,
        retrigger: true,
    });
    f.limit = 0;
    assert_eq!(f.tick(&[]).error, Some(Error::Capacity));
    assert!(f.midi.recovery_pending());
    assert_eq!(f.midi.output_state(), Err(Error::Busy));
    f.command(Operation::Stop);
    f.limit = 1;
    for i in 0..32 {
        let before = f.emitted.len();
        f.tick(&[]);
        assert_eq!(f.emitted.len() - before, 1);
        assert_eq!(
            f.emitted.last().unwrap().1,
            msg(&[0xb0 | (i / 2) as u8, if i % 2 == 0 { 120 } else { 123 }, 0])
        );
    }
    assert!(!f.midi.recovery_pending());
    f.limit = usize::MAX;
    f.command(Operation::Play {
        position: 0,
        retrigger: true,
    });
    assert_eq!(f.tick(&[]).error, None);
    assert_eq!(f.emitted.last().unwrap().1, msg(&[0x90, 60, 99]));
}

#[shoop_test]
fn incomplete_capture_cannot_play_and_does_not_interrupt_audio_or_input_tracking() {
    let mut f = Fixture::new(16, 1);
    f.command(Operation::Record);
    assert_eq!(
        f.tick(&[event(0, &[0x90, 60, 99]), event(15, &[0x80, 60, 0])])
            .error,
        Some(Error::Capacity)
    );
    assert_eq!(f.midi.take().status(), TakeStatus::Incomplete);
    assert_eq!(f.midi.take().records().len(), 1);
    assert!(!f
        .midi
        .input_state()
        .channel(0)
        .unwrap()
        .note(60)
        .unwrap()
        .held());
    f.command(Operation::Stop);
    assert_eq!(f.tick(&[]).error, Some(Error::Incomplete));
    f.command(Operation::Play {
        position: 0,
        retrigger: true,
    });
    assert_eq!(f.tick(&[]).error, Some(Error::Incomplete));
    assert!(f.emitted.is_empty());
    f.command(Operation::Record);
    assert_eq!(f.tick(&[]).error, None);
    f.command(Operation::Stop);
    assert_eq!(f.tick(&[]).error, None);
}

#[shoop_test]
fn bad_windows_and_unsupported_seeks_are_refused_before_clock_or_midi_mutation() {
    let mut f = Fixture::new(16, 8);
    assert_eq!(
        f.run(16, &[event(16, &[0x90, 60, 1])]),
        Err(ProcessError::Midi(Error::InvalidEvent))
    );
    assert_eq!(f.pipeline.state().frame(), 0);
    f.command(Operation::Record);
    f.tick(&[]);
    f.tick(&[]);
    f.command(Operation::Stop);
    f.tick(&[]);
    let before = f.pipeline.state().frame();
    f.command(Operation::Play {
        position: 16,
        retrigger: true,
    });
    assert_eq!(f.run(16, &[]), Err(ProcessError::Midi(Error::Unsupported)));
    assert_eq!(f.pipeline.state().frame(), before);
    assert!(f.emitted.is_empty());
}

#[shoop_test]
fn refused_note_off_triggers_recovery_and_never_replays_a_delayed_event() {
    let mut f = Fixture::new(32, 8);
    f.command(Operation::Record);
    f.tick(&[event(0, &[0x90, 60, 80]), event(31, &[0x80, 60, 0])]);
    f.command(Operation::Stop);
    f.tick(&[]);
    f.command(Operation::Play {
        position: 0,
        retrigger: true,
    });
    f.limit = 65;
    assert_eq!(f.tick(&[]).error, Some(Error::Capacity));
    assert_eq!(f.emitted.last().unwrap().1, msg(&[0x90, 60, 80]));
    assert!(f.midi.recovery_pending());
    let count = f.emitted.len();
    f.command(Operation::Stop);
    f.limit = usize::MAX;
    assert_eq!(f.tick(&[]).error, None);
    assert_eq!(f.emitted.len(), count + 32);
    assert_eq!(f.emitted[count].1, msg(&[0xb0, 120, 0]));
    assert!(f.emitted[count..]
        .iter()
        .all(|(_, message)| message.as_bytes()[0] & 0xf0 == 0xb0));
    assert!(!f.midi.recovery_pending());
    assert_eq!(
        f.midi
            .output_state()
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
fn recovery_preserves_pedal_ownership_until_stop_releases_it() {
    let mut f = Fixture::new(16, 8);
    f.command(Operation::Record);
    f.tick(&[
        event(0, &[0xb0, 64, 127]),
        event(1, &[0x90, 60, 80]),
        event(15, &[0x80, 60, 0]),
    ]);
    f.command(Operation::Stop);
    f.tick(&[]);
    f.command(Operation::Play {
        position: 0,
        retrigger: true,
    });
    f.limit = 66;
    assert_eq!(f.tick(&[]).error, Some(Error::Capacity));
    let before = f.emitted.len();
    f.command(Operation::Stop);
    f.limit = usize::MAX;
    assert_eq!(f.tick(&[]).error, None);
    assert_eq!(f.emitted.len() - before, 33);
    assert_eq!(f.emitted.last().unwrap().1, msg(&[0xb0, 64, 0]));
}

#[shoop_test]
fn a_partially_restored_pedal_is_owned_until_recovery_and_stop_finish() {
    let mut f = Fixture::new(16, 8);
    f.tick(&[
        event(0, &[0xb0, 64, 127]),
        event(0, &[0x90, 60, 80]),
        event(0, &[0x80, 60, 0]),
    ]);
    f.command(Operation::Record);
    f.tick(&[]);
    f.command(Operation::Stop);
    f.tick(&[]);
    f.command(Operation::Play {
        position: 0,
        retrigger: true,
    });
    f.limit = 5;
    assert_eq!(f.tick(&[]).error, Some(Error::Capacity));
    assert_eq!(f.emitted.last().unwrap().1, msg(&[0x90, 60, 80]));
    let before = f.emitted.len();
    f.command(Operation::Stop);
    f.limit = usize::MAX;
    assert_eq!(f.tick(&[]).error, None);
    assert_eq!(f.emitted.len() - before, 33);
    assert_eq!(f.emitted.last().unwrap().1, msg(&[0xb0, 64, 0]));
}

#[shoop_test]
fn cleanup_resumes_one_message_at_a_time_without_restarting_recovery() {
    let mut f = Fixture::new(16, 8);
    f.command(Operation::Record);
    f.tick(&[
        event(0, &[0x90, 60, 80]),
        event(0, &[0xb0, 64, 127]),
        event(0, &[0xb0, 66, 127]),
        event(0, &[0xb0, 69, 127]),
    ]);
    f.command(Operation::Stop);
    f.tick(&[]);
    f.command(Operation::Play {
        position: 0,
        retrigger: true,
    });
    f.tick(&[]);
    f.command(Operation::Stop);
    f.limit = 1;
    let before = f.emitted.len();
    for i in 0..4 {
        let report = f.tick(&[]);
        assert_eq!(
            report.error,
            if i == 3 { None } else { Some(Error::Capacity) }
        );
        assert!(!f.midi.recovery_pending());
        assert_eq!(f.midi.cleanup_pending(), i != 3);
        if i != 3 {
            assert_eq!(f.midi.output_state(), Err(Error::Busy));
        }
    }
    assert_eq!(
        f.emitted[before..]
            .iter()
            .map(|(_, m)| *m)
            .collect::<Vec<_>>(),
        [
            msg(&[0xb0, 64, 0]),
            msg(&[0xb0, 66, 0]),
            msg(&[0xb0, 69, 0]),
            msg(&[0x80, 60, 0])
        ]
    );
}

#[shoop_test]
fn recording_directly_from_playback_releases_the_previous_output_notes() {
    let mut f = Fixture::new(16, 8);
    f.command(Operation::Record);
    f.tick(&[event(0, &[0x90, 60, 80])]);
    f.command(Operation::Stop);
    f.tick(&[]);
    f.command(Operation::Play {
        position: 0,
        retrigger: true,
    });
    f.tick(&[]);
    let before = f.emitted.len();
    f.command(Operation::Record);
    assert_eq!(f.tick(&[event(0, &[0x90, 61, 90])]).error, None);
    assert_eq!(
        f.emitted[before..]
            .iter()
            .map(|(_, m)| *m)
            .collect::<Vec<_>>(),
        [msg(&[0x80, 60, 0])]
    );
    assert_eq!(f.midi.take().records()[0].message, msg(&[0x90, 61, 90]));
}

#[shoop_test]
fn a_fatal_audio_fault_silences_audio_and_leaves_midi_recovery_drainable() {
    use rt_fpga_reference::{jobs::Memory, storage::ViewId};
    #[derive(Default)]
    struct Fault(core::cell::Cell<u32>);
    impl Memory for Fault {
        fn chunk_frames(&self) -> usize {
            1
        }
        fn begin_capture(&mut self, _: ViewId) -> Result<(), rt_fpga_reference::Error> {
            Ok(())
        }
        fn read(
            &mut self,
            _: ViewId,
            _: u64,
            _: u64,
            _: &mut [Fixed],
        ) -> Result<(), rt_fpga_reference::Error> {
            Ok(())
        }
        fn write(&mut self, _: ViewId, _: &[Fixed]) -> Result<(), rt_fpga_reference::Error> {
            Ok(())
        }
        fn fault(&self) -> Option<rt_fpga_reference::Error> {
            let calls = self.0.get();
            self.0.set(calls + 1);
            (calls != 0).then_some(rt_fpga_reference::Error::MemoryFull)
        }
    }
    let mut f = Fixture::new(16, 8);
    f.command(Operation::Record);
    f.tick(&[event(0, &[0x90, 60, 80])]);
    f.command(Operation::Stop);
    f.tick(&[]);
    f.command(Operation::Play {
        position: 0,
        retrigger: true,
    });
    f.tick(&[]);
    let result = assert_no_heap(|| {
        f.pipeline.process_midi_block(
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
    assert!(f.pipeline.fault().is_some());
    assert!(f.midi.recovery_pending());
    f.output[..16].fill(Fixed(7));
    let again = assert_no_heap(|| {
        f.pipeline.process_midi_block(
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
    assert_eq!(count, 32);
}

#[shoop_test]
fn silent_takes_repeat_and_input_budget_refusal_preserves_the_clock() {
    let mut f = Fixture::new(16, 4);
    let dense = [event(0, &[0x90, 60, 1]); 17];
    assert_eq!(f.run(16, &dense), Err(ProcessError::Midi(Error::Capacity)));
    assert_eq!(f.pipeline.state().frame(), 0);
    f.command(Operation::Record);
    f.tick(&[]);
    f.command(Operation::Stop);
    f.tick(&[]);
    f.command(Operation::Play {
        position: 0,
        retrigger: true,
    });
    for _ in 0..4 {
        assert_eq!(f.tick(&[]).error, None);
    }
    assert_eq!(f.emitted.len(), 4 * 64);
    assert!(f.midi.take().records().is_empty());
}
