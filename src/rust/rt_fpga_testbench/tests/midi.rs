#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_reference::{midi::*, transitions::Operation};
use rt_fpga_testbench::suites::midi::data::*;
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn midi_data_jobs_match_rust_for_dense_recording_merge_repeat_and_stop() {
    let compiled = compile().unwrap();
    let mut d = Driver::new(&compiled, 32).unwrap();
    let mut f = Oracle::new(32, false);
    let events: Vec<_> = (0..600)
        .flat_map(|i| {
            [
                event(1, 2, &[0xb1, 1, (i % 128) as u8]),
                event(0, 2, &[0xb0, 1, (i % 128) as u8]),
            ]
        })
        .collect();
    for slot in 0..2 {
        f.command(slot, Operation::Record);
    }
    d.quantum(&mut f, &events);
    for slot in 0..2 {
        f.command(
            slot,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
    }
    d.quantum(&mut f, &[]);
    assert_eq!(d.sink.borrow().events.len(), 1200);
    d.quantum(&mut f, &[]);
    for slot in 0..2 {
        f.command(slot, Operation::Stop);
    }
    d.quantum(&mut f, &[]);
    assert!(d.sink.borrow().events.is_empty());
    for slot in 0..2 {
        let r = d.control(11, [slot, 0, 0, 0, 0]);
        assert_eq!((r.0, r.2, r.3), (0, 32, 600));
    }
}
#[shoop_test]
fn midi_data_jobs_cross_partial_blocks_and_keep_offsets_for_all_quanta() {
    let compiled = compile().unwrap();
    for q in [1, 16, 32, 64, 128] {
        let mut d = Driver::new(&compiled, q).unwrap();
        let mut f = Oracle::new(q, false);
        f.command(0, Operation::Record);
        for n in 0..3 {
            d.quantum(&mut f, &vec![event(0, q - 1, &[0x90, 60, 80 + n]); 13]);
        }
        f.command(
            0,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
        for _ in 0..6 {
            d.quantum(&mut f, &[]);
        }
        f.command(
            0,
            Operation::Seek {
                position: u64::from(q),
                retrigger: false,
            },
        );
        d.quantum(&mut f, &[]);
        f.command(0, Operation::Stop);
        d.quantum(&mut f, &[]);
        assert!(d.sink.borrow().events.is_empty());
    }
}
#[shoop_test]
fn midi_data_jobs_shared_ranges_survive_retakes_and_stale_handles_are_rejected() {
    let compiled = compile().unwrap();
    let mut d = Driver::new(&compiled, 32).unwrap();
    let r = d.control(8, [0; 5]);
    assert_eq!(r.0, 0);
    let original = r.1;
    for frame in 0..64 {
        assert_eq!(
            d.control(
                9,
                [
                    original,
                    frame,
                    u64::from(packed(Message::parse(&[0xb0, 1, frame as u8]).unwrap())),
                    0,
                    0
                ]
            )
            .0,
            0
        );
    }
    assert_eq!(d.control(10, [original, 64, 0, 0, 0]).0, 0);
    let r = d.control(15, [original, 5, 37, 0, 0]);
    assert_eq!(r.0, 0);
    let range = r.1;
    assert_eq!(d.control(5, [0, original, 0, 0, 0]).0, 0);
    assert_eq!(d.control(5, [1, range, 0, 0, 0]).0, 0);
    assert_eq!(d.control(13, [original, 0, 0, 0, 0]).0, 0);
    assert_eq!(d.control(13, [range, 0, 0, 0, 0]).0, 0);
    assert_eq!(d.prepare(&[]), 0);
    assert_eq!(
        d.job(rt_fpga_reference::jobs::Job::MidiBeginCapture {
            sequence: SequenceId(0)
        }),
        0
    );
    d.finish();
    let r = d.control(11, [1, 0, 0, 0, 0]);
    assert_eq!((r.0, r.2, r.3), (0, 32, 32));
    let retained = r.1;
    for index in [0, 7, 8, 31] {
        let r = d.control(12, [retained, index, 0, 0, 0]);
        assert_eq!((r.0, r.2), (0, index));
    }
    assert_eq!(d.control(12, [original, 0, 0, 0, 0]).0, 4);
}

#[shoop_test]
fn compiled_midi_sp_and_shared_jp_match_rust_with_audio_and_dense_events() {
    use rt_fpga_testbench::suites::midi::data::pipeline;
    let compiled = pipeline::compile(32).unwrap();
    for mixed in [false, true] {
        let mut d = pipeline::Driver::new(&compiled, 32, mixed).unwrap();
        for slot in 0..2 {
            d.command(slot, Operation::Record);
        }
        let events: Vec<_> = (0..300)
            .flat_map(|i| {
                [
                    event(0, 2, &[0xb0, 1, (i % 128) as u8]),
                    event(1, 31, &[0xb1, 64, (i % 128) as u8]),
                ]
            })
            .collect();
        let mut events = events;
        events.sort_by_key(|e| e.offset);
        d.quantum(&events);
        d.quantum(&[event(0, 31, &[0x90, 60, 80])]);
        for slot in 0..2 {
            d.command(
                slot,
                Operation::Play {
                    position: 0,
                    retrigger: true,
                },
            );
        }
        d.quantum(&[]);
        d.quantum(&[]);
        d.quantum(&[]);
        d.command(
            0,
            Operation::Seek {
                position: 32,
                retrigger: false,
            },
        );
        d.quantum(&[]);
        d.command(
            0,
            Operation::Seek {
                position: 32,
                retrigger: false,
            },
        );
        d.quantum(&[]);
        for slot in 0..2 {
            d.command(slot, Operation::Stop);
        }
        d.quantum(&[]);
    }
}

#[shoop_test]
fn midi_data_axi32_matches_rust_with_stalls_and_memory_faults() {
    axi::conformance(32).unwrap();
}
#[shoop_test]
fn midi_data_axi64_matches_rust_with_stalls_and_memory_faults() {
    axi::conformance(64).unwrap();
}

#[shoop_test]
fn midi_idle_playback_reuses_future_event_without_ddr_access() {
    let compiled = compile().unwrap();
    let mut d = Driver::new(&compiled, 32).unwrap();
    let mut f = Oracle::new(32, false);
    let note = Message::parse(&[0x90, 60, 80]).unwrap();
    let view = f.midi.store.writer().unwrap();
    f.midi.store.append(view, 64, note).unwrap();
    f.midi.store.finish(view, 96).unwrap();
    f.pipeline
        .replace_midi_view(&mut f.midi, id(0), SequenceId(0), Some(view))
        .unwrap();
    let handle = d.control(8, [0; 5]).1;
    assert_eq!(
        d.control(9, [handle, 64, u64::from(packed(note)), 0, 0]).0,
        0
    );
    assert_eq!(d.control(10, [handle, 96, 0, 0, 0]).0, 0);
    assert_eq!(d.control(5, [0, handle, 0, 0, 0]).0, 0);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    d.quantum(&mut f, &[]);
    let before = d.memory.borrow().transfers;
    d.quantum(&mut f, &[]);
    assert_eq!(d.memory.borrow().transfers, before);
    assert_eq!(f.jobs.midi_work().record_reads, 0);
    d.quantum(&mut f, &[]);
    assert_eq!(d.sink.borrow().events.len(), 1);
}
#[shoop_test]
fn midi_scratch_and_storage_exhaustion_report_errors_without_false_completion() {
    use rt_fpga_reference::jobs::Job;
    for scratch in [true, false] {
        let mut design = design();
        design.parameters.insert(
            if scratch { "SCRATCH_EVENTS" } else { "BLOCKS" }.into(),
            if scratch { 4 } else { 1 }.to_string(),
        );
        let compiled = design
            .build(&rt_fpga_testbench::root().join("work/marlin-suite/midi-capacity"))
            .unwrap();
        let mut d = Driver::new(&compiled, 32).unwrap();
        let input = vec![event(0, 0, &[0xb0, 1, 20]); 9];
        assert_eq!(d.prepare(&input), 0);
        assert_eq!(
            d.job(Job::MidiBeginCapture {
                sequence: SequenceId(0)
            }),
            0
        );
        assert_eq!(
            d.job(Job::MidiInput {
                port: 0,
                destination: BufferId(0)
            }),
            if scratch { 3 } else { 0 }
        );
        if !scratch {
            assert_eq!(
                d.job(Job::MidiAppend {
                    sequence: SequenceId(0),
                    source: BufferId(0),
                    position: 0,
                    frames: 32
                }),
                3
            );
            let (generation, status, count) = {
                let m = d.memory.borrow();
                (m.words[0], m.words[2], m.words[5])
            };
            assert_eq!((status, count), (3, 8));
            assert_eq!(
                d.control(10, [u64::from(generation) << 32, 32, 0, 0, 0]).0,
                8
            );
        }
        assert!(d.sink.borrow().events.is_empty());
    }
}
#[shoop_test]
fn midi_filters_channels_and_passes_short_system_packets_without_instrument_state() {
    use rt_fpga_reference::jobs::Job;
    let compiled = compile().unwrap();
    let mut d = Driver::new(&compiled, 32).unwrap();
    let handle = d.control(8, [0; 5]).1;
    let packets = [
        Message::parse(&[0xb0, 6, 17]).unwrap(),
        Message::parse(&[0xb1, 64, 127]).unwrap(),
        Message::parse(&[0xf8]).unwrap(),
        Message::parse(&[0xf2, 1, 2]).unwrap(),
    ];
    for (i, &m) in packets.iter().enumerate() {
        assert_eq!(
            d.control(9, [handle, i as u64, u64::from(packed(m)), 0, 0])
                .0,
            0
        );
    }
    assert_eq!(d.control(10, [handle, 32, 0, 0, 0]).0, 0);
    assert_eq!(d.control(5, [0, handle, 0, 0, 0]).0, 0);
    assert_eq!(d.prepare(&[]), 0);
    assert_eq!(
        d.job(Job::MidiReadView {
            owner: id(0),
            sequence: SequenceId(0),
            cursor: CursorId(0),
            position: 0,
            frames: 32,
            restart: true,
            channels: 1,
            destination: BufferId(0)
        }),
        0
    );
    assert_eq!(
        d.job(Job::MidiOutput {
            source: BufferId(0),
            port: 0
        }),
        0
    );
    assert_eq!(
        d.sink
            .borrow()
            .events
            .iter()
            .map(|e| e.message)
            .collect::<Vec<_>>(),
        vec![packets[0], packets[2], packets[3]]
    );
}

#[shoop_test]
fn midi_three_thousand_messages_finish_within_one_quantum_transaction() {
    let compiled = compile().unwrap();
    let mut d = Driver::new(&compiled, 32).unwrap();
    let mut f = Oracle::new(32, false);
    let events: Vec<_> = (0..3000)
        .map(|i| event(0, 0, &[0xb0, 1, (i % 128) as u8]))
        .collect();
    f.command(0, Operation::Record);
    d.quantum(&mut f, &events);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    d.quantum(&mut f, &[]);
    assert_eq!(d.sink.borrow().events.len(), 3000);
}

#[shoop_test]
fn compiled_midi_rejects_bad_input_before_sp_and_keeps_shared_contents_on_retake() {
    use rt_fpga_testbench::suites::midi::data::pipeline;
    let compiled = pipeline::compile(32).unwrap();
    let mut d = pipeline::Driver::new(&compiled, 32, false).unwrap();
    d.rejected_input(&[event(0, 31, &[0x90, 60, 80]), event(0, 0, &[0x80, 60, 0])]);
    d.quantum(&[]);
    let note = Message::parse(&[0x90, 60, 80]).unwrap();
    d.install(0, &[(64, note)], 96);
    d.duplicate(0, 1);
    for slot in 0..2 {
        d.command(
            slot,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
    }
    d.quantum(&[]);
    let count = d.memory.borrow().transfers;
    d.quantum(&[]);
    assert_eq!(d.memory.borrow().transfers, count);
    d.quantum(&[]);
    assert_eq!(d.sink.borrow().events.len(), 2);
    d.command(0, Operation::Record);
    d.quantum(&[event(0, 0, &[0xb0, 1, 20])]);
    d.command(0, Operation::Stop);
    d.quantum(&[]);
    d.quantum(&[]);
    assert_eq!(d.sink.borrow().events.len(), 1);
    assert_eq!(d.sink.borrow().events[0].message, note);
}
#[shoop_test]
fn compiled_midi_output_refusal_stops_the_shared_jp_and_latches_the_fault() {
    use rt_fpga_testbench::suites::midi::data::pipeline;
    let compiled = pipeline::compile(32).unwrap();
    let mut d = pipeline::Driver::new(&compiled, 32, false).unwrap();
    d.install(
        0,
        &[
            (0, Message::parse(&[0x90, 60, 80]).unwrap()),
            (1, Message::parse(&[0x90, 61, 80]).unwrap()),
        ],
        32,
    );
    d.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    let state = d.reference.pipeline.state().loop_state(id(0)).unwrap();
    let spec = rt_fpga_reference::transitions::LoopSpec {
        id: id(0),
        kind: rt_fpga_reference::transitions::LoopKind::Primitive,
        length: 0,
        sync_source: None,
        repeat_sync: false,
        timeline: None,
    };
    let row = rt_fpga_testbench::loop_image::image(state, &spec, None);
    rt_fpga_testbench::loop_image::write_plans(&mut d.audio.program, &mut d.sim, 0, &row);
    d.sink.borrow_mut().remaining = Some(1);
    assert_eq!(
        d.audio.execute_with_limit(&mut d.sim, &[], true, 5_000_000),
        (2, 4)
    );
    assert_eq!(d.sim.model.get("midi_error_code"), 9);
    assert_eq!(d.sink.borrow().events.len(), 1);
    assert_eq!(d.sim.model.get("run_ready"), 0);
    d.sim.idle(100);
    assert_eq!(d.sink.borrow().events.len(), 1);
}
#[shoop_test]
fn pending_ui_command_cannot_deadlock_midi_quantum_completion() {
    use rt_fpga_testbench::{clock::Edges, model::Model, sim::Agent, suites::midi::data::pipeline};
    use std::{cell::RefCell, rc::Rc};
    #[derive(Default)]
    struct Pending {
        offered: bool,
        accepted: bool,
    }
    impl Agent for Pending {
        fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
            if e.rises(0) {
                if m.get("busy") != 0 {
                    self.offered = true;
                }
                m.set(
                    "midi_command_valid",
                    u64::from(self.offered && !self.accepted),
                );
                m.set("midi_command_op", 20);
                m.set("midi_command_arg0", 0);
                m.set("midi_response_ready", 1);
            }
        }
        fn sample(&mut self, m: &Model<'_>, e: Edges) {
            if e.rises(0) && m.get("midi_command_valid") != 0 && m.get("midi_command_ready") != 0 {
                self.accepted = true;
            }
        }
    }
    let compiled = pipeline::compile(32).unwrap();
    let mut d = pipeline::Driver::new(&compiled, 32, false).unwrap();
    let pending = Rc::new(RefCell::new(Pending::default()));
    d.sim.agent(pending.clone());
    d.quantum(&[]);
    d.sim.idle(30);
    assert!(pending.borrow().offered && pending.borrow().accepted);
}

#[shoop_test]
fn full_audio_midi_axi32_pipeline_matches_rust_with_delayed_ddr() {
    axi::compiled_pipeline(32, 7).unwrap();
}
#[shoop_test]
fn full_audio_midi_axi64_pipeline_matches_rust_with_ideal_ddr() {
    axi::compiled_pipeline(64, 0).unwrap();
}
