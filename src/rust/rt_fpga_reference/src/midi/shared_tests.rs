use super::{oracle, *};
use crate::jobs::Job;
use crate::transitions::Operation;
use alloc::vec::Vec;
use shoop_wasm_test_support::shoop_test;
#[path = "../../tests/support/midi_session.rs"]
#[allow(dead_code, unused_imports)]
mod fixture;

#[shoop_test]
fn shared_jobs_match_frozen_session_with_partial_output_and_pedals() {
    for q in [1, 16, 32, 64, 128] {
        let mut f = fixture::Fixture::new(q, 3);
        f.midi.unbind(0).unwrap();
        f.midi
            .bind(
                0,
                Binding {
                    loop_id: fixture::id(0),
                    input: 0,
                    destination: 1,
                    channels: 1,
                },
            )
            .unwrap();
        let l = fixture::limits(3);
        let mut old = oracle::MidiSession::prepare(oracle::SessionLimits {
            store: l.store,
            loops: l.loops,
            inputs: l.inputs,
            destinations: l.destinations,
            input_events_per_quantum: l.input_events_per_quantum,
            history_frames: l.history_frames,
            maintenance_steps: l.maintenance_steps,
        })
        .unwrap();
        for context in 0..3 {
            old.bind(
                context,
                oracle::Binding {
                    loop_id: fixture::id(usize::from(context)),
                    input: 0,
                    destination: if context == 0 { 1 } else { 0 },
                    channels: 1 << context,
                },
            )
            .unwrap();
        }
        for tick in 0..144 {
            let phase = tick % 24;
            let command = match phase {
                0 => Some(Operation::Record),
                8 | 16 => Some(Operation::Play {
                    position: 0,
                    retrigger: true,
                }),
                12 | 20 => Some(Operation::Stop),
                _ => None,
            };
            if let Some(command) = command {
                for i in 0..3 {
                    f.command(i, command);
                }
            }
            let mut events = Vec::new();
            for channel in 0..3 {
                let bytes = match phase % 6 {
                    0 => [0xb0 | channel, 1, tick as u8 % 127],
                    1 => [0x90 | channel, 60, 80],
                    2 => [0xb0 | channel, 64, 127],
                    3 => [0x80 | channel, 60, 0],
                    4 => [0xb0 | channel, 66, 127],
                    _ => [0xb0 | channel, 64, 0],
                };
                events.push(fixture::event(q - 1, &bytes));
            }
            let old_events = events
                .iter()
                .map(|e| oracle::InputEvent {
                    input: e.input,
                    offset: e.offset,
                    message: e.message,
                })
                .collect::<Vec<_>>();
            f.capacity = if tick % 17 == 0 { 1 } else { usize::MAX };
            old.validate(f.pipeline.state(), q as usize, &old_events)
                .unwrap();
            f.emitted.clear();
            let report = f.run(&events);
            let mut accepted = Vec::new();
            let error = old
                .quantum(f.pipeline.state(), 0, &old_events, &mut |e| {
                    if accepted.len() == f.capacity {
                        return Err(Error::Capacity);
                    }
                    accepted.push(OutputEvent {
                        destination: e.destination,
                        offset: e.offset,
                        message: e.message,
                    });
                    Ok(())
                })
                .err();
            assert_eq!(report.error, error, "q={q}, tick={tick}");
            assert_eq!(f.emitted, accepted, "q={q}, tick={tick}");
            for destination in 0..2 {
                assert_eq!(
                    f.midi.output_state(destination),
                    old.output_state(destination),
                    "q={q}, tick={tick}"
                );
                assert_eq!(
                    f.midi.recovery_pending(destination),
                    old.recovery_pending(destination)
                );
            }
            assert_eq!(f.midi.input_state(0), old.input_state(0));
            for context in 0..3 {
                assert_eq!(f.midi.take_status(context), old.take_status(context));
                let a = f.midi.content(context).unwrap();
                let b = old.content(context).unwrap();
                assert_eq!(a.is_some(), b.is_some());
                if let (Some(a), Some(b)) = (a, b) {
                    let a_desc = f.midi.store().describe(a).unwrap();
                    let b_desc = old.store().describe(b).unwrap();
                    assert_eq!(
                        (a_desc.frames, a_desc.events, a_desc.status),
                        (b_desc.frames, b_desc.events, b_desc.status)
                    );
                    assert_eq!(
                        f.midi.store().state(a_desc.initial),
                        old.store().state(b_desc.initial)
                    );
                    for n in 0..a_desc.events {
                        assert_eq!(f.midi.store().read(a, n), old.store().read(b, n));
                    }
                }
            }
        }
    }
}

#[shoop_test]
fn common_queue_finishes_capture_before_restoring_and_emitting_it() {
    let mut f = fixture::Fixture::new(32, 2);
    f.pipeline.trace_jobs(true);
    for i in 0..2 {
        f.command(i, Operation::Record);
    }
    f.run(&[
        fixture::event(0, &[0x90, 60, 80]),
        fixture::event(31, &[0x91, 61, 90]),
    ]);
    for i in 0..2 {
        f.command(
            i,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
    }
    assert_eq!(f.run(&[]).error, None);
    let trace = f.pipeline.emitted_jobs();
    assert!(matches!(trace[0], Job::Input { .. }));
    assert!(trace.iter().any(|j| matches!(j, Job::Output { .. })));
    let finish = trace
        .iter()
        .rposition(|j| matches!(j, Job::Midi(MidiJob::FinishCapture { .. })))
        .unwrap();
    let read = trace
        .iter()
        .position(|j| matches!(j, Job::Midi(MidiJob::ReadUntil { .. })))
        .unwrap();
    let output = trace
        .iter()
        .position(|j| matches!(j, Job::Midi(MidiJob::Output)))
        .unwrap();
    assert!(finish < read && read < output);
    assert_eq!(
        f.emitted.iter().map(|e| e.offset).collect::<Vec<_>>(),
        [0, 31]
    );
    assert_eq!(f.midi.take_status(0).unwrap(), TakeStatus::Complete);
}

#[shoop_test]
fn context_reuse_requires_stopped_execution_and_keeps_export_pins() {
    let mut f = fixture::Fixture::new(32, 1);
    f.command(0, Operation::Record);
    f.run(&[fixture::event(0, &[0x90, 60, 80])]);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    f.run(&[]);
    assert_eq!(f.midi.unbind(0), Err(Error::Busy));
    let view = f.midi.export(f.midi.content(0).unwrap().unwrap()).unwrap();
    f.command(0, Operation::Stop);
    f.run(&[]);
    f.midi.unbind(0).unwrap();
    f.midi
        .bind(
            0,
            Binding {
                loop_id: crate::records::Identity {
                    slot: 0,
                    generation: 2,
                },
                input: 0,
                destination: 0,
                channels: 1,
            },
        )
        .unwrap();
    assert_eq!(f.midi.content(0), Ok(None));
    assert_eq!(
        f.midi.store().read(view, 0).unwrap().unwrap().message,
        fixture::msg(&[0x90, 60, 80])
    );
    let frame = f.pipeline.state().frame();
    assert_eq!(
        f.try_run_frames(&[], 32),
        Err(ProcessError::Midi(Error::InvalidState))
    );
    assert_eq!(f.pipeline.state().frame(), frame);
    f.midi.release(view).unwrap();
}

#[shoop_test]
fn mixed_queue_preflight_refuses_before_advancing_musical_time() {
    use crate::{jobs::JobQueue, state::StateProcessor, transitions::*};
    let f = fixture::Fixture::new(32, 1);
    let plan = TransitionPlan::compile(
        PlanDescription {
            quantum: 32,
            loops: alloc::vec![LoopSpec {
                id: fixture::id(0),
                kind: LoopKind::Primitive,
                length: 0,
                sync_source: None,
                repeat_sync: false,
                timeline: None
            }],
        },
        TransitionLimits {
            loop_slots: 1,
            events: 4,
        },
    )
    .unwrap();
    let mut state = StateProcessor::with_loops(plan, alloc::vec![]);
    let mut program = f.pipeline.program().clone();
    let bound = program.jobs_per_quantum;
    program.jobs_per_quantum -= 1;
    let mut queue = JobQueue::new(bound);
    assert_eq!(
        state.run(&program, &mut queue),
        Err(crate::Error::QueueFull)
    );
    assert_eq!(state.frame(), 0);
    assert!(queue.is_empty());
}
