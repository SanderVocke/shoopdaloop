#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[allow(dead_code, unused_imports)]
mod oracle {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../rt_fpga_reference/tests/support/midi_session.rs"
    ));
}
#[shoop_test]
fn midi_ddr_interpreter_matches_reference_with_delayed_memory() {
    rt_fpga_testbench::suites::midi::state_conformance().unwrap();
}
#[shoop_test]
fn midi_ddr_difference_initialization_and_incremental_updates_match_reference() {
    rt_fpga_testbench::suites::midi::difference_conformance().unwrap();
}
#[shoop_test]
fn midi_ddr_ordered_restoration_and_cleanup_match_reference() {
    rt_fpga_testbench::suites::midi::transition_conformance().unwrap();
}
#[shoop_test]
fn midi_ddr_restoration_updates_output_state_and_all_prepared_differences() {
    rt_fpga_testbench::suites::midi::service_conformance().unwrap();
}
#[shoop_test]
fn midi_ddr_shared_views_snapshots_capacity_and_incremental_retirement_match_reference() {
    rt_fpga_testbench::suites::midi::store_conformance().unwrap();
}

#[shoop_test]
fn midi_ddr_session_record_restore_repeat_stop_and_history_grab_match_rust() {
    use oracle::*;
    use rt_fpga_testbench::suites::midi::runtime::{Driver, compile};
    let compiled = compile(2).unwrap();
    let mut d = Driver::new(&compiled, 2).unwrap();
    let mut f = Fixture::new(16, 2);
    let compare = |f: &mut Fixture, d: &mut Driver<'_>, events: &[InputEvent]| {
        let start = f.pipeline.state().frame();
        let report = f.run(events);
        let plans: Vec<_> = (0..2)
            .map(|i| f.pipeline.state().loop_state(id(i)).unwrap().render)
            .collect();
        assert_eq!(report.error, None);
        assert_eq!(d.process(start, f.q, &plans, events), 0, "start {start}");
        assert_eq!(
            d.messages(),
            std::mem::take(&mut f.emitted),
            "start {start}"
        );
        d.verify_state(0, 0, f.midi.input_state(0).unwrap());
        d.verify_state(1, 0, f.midi.output_state(0).unwrap());
    };
    compare(&mut f, &mut d, &[event(0, &[0xb0, 1, 40])]);
    f.command(0, Operation::Record);
    f.command(1, Operation::Record);
    compare(
        &mut f,
        &mut d,
        &[event(0, &[0x90, 60, 80]), event(15, &[0xb1, 7, 100])],
    );
    compare(
        &mut f,
        &mut d,
        &[event(2, &[0xb0, 64, 127]), event(5, &[0x80, 60, 0])],
    );
    f.command(0, Operation::Stop);
    f.command(1, Operation::Stop);
    compare(&mut f, &mut d, &[]);
    for i in 0..2 {
        f.command(
            i,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
    }
    compare(&mut f, &mut d, &[]);
    compare(&mut f, &mut d, &[]);
    compare(&mut f, &mut d, &[]);
    f.command(0, Operation::Stop);
    f.command(1, Operation::Stop);
    compare(&mut f, &mut d, &[]);
    let end = f.pipeline.state().frame();
    f.pipeline
        .grab_midi(&mut f.midi, 0, end - 32, end, &[0, 1])
        .unwrap();
    d.write(d.scratch + 512, &[0, 1]);
    assert_eq!(d.issue(4, [0, end - 32, end, d.scratch + 512, 2]), 0);
    for i in 0..2 {
        f.command(
            i,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
    }
    compare(&mut f, &mut d, &[]);
    compare(&mut f, &mut d, &[]);
}

fn compare_quantum(
    f: &mut oracle::Fixture,
    d: &mut rt_fpga_testbench::suites::midi::runtime::Driver<'_>,
    events: &[oracle::InputEvent],
) {
    use oracle::*;
    let start = f.pipeline.state().frame();
    let report = f.run(events);
    let plans: Vec<_> = (0..2)
        .map(|i| f.pipeline.state().loop_state(id(i)).unwrap().render)
        .collect();
    let error = match report.error {
        None => 0,
        Some(Error::InvalidEvent) => 1,
        Some(Error::Unsupported) => 2,
        Some(Error::Capacity) => 3,
        Some(Error::InvalidState | Error::Incomplete) => 4,
        Some(Error::Busy) => 5,
        Some(Error::Overflow) => 6,
    };
    d.set_capacity(f.capacity);
    assert_eq!(
        d.process(start, f.q, &plans, events),
        error,
        "start {start}"
    );
    assert_eq!(
        d.messages(),
        std::mem::take(&mut f.emitted),
        "start {start}"
    );
    d.verify_state(0, 0, f.midi.input_state(0).unwrap());
    if let Ok(state) = f.midi.output_state(0) {
        d.verify_state(1, 0, state);
    } else {
        assert_eq!(d.issue(19, [1, 0, 0, 0, 0]), 5);
    }
}

#[shoop_test]
fn midi_ddr_import_export_duplicate_and_retake_stay_coherent_during_playback() {
    use oracle::*;
    use rt_fpga_testbench::suites::midi::runtime::{Driver, compile};
    let compiled = compile(2).unwrap();
    let mut d = Driver::new(&compiled, 2).unwrap();
    let mut f = Fixture::new(16, 2);
    let mut initial = MidiState::default();
    initial.apply_packet(&[0xb0, 1, 50], 3).unwrap();
    let mut words = Vec::new();
    for channel in 0..16 {
        let c = initial.channel(channel).unwrap();
        for key in 0..128 {
            let n = c.note(key).unwrap();
            words.push(
                u32::from(n.velocity())
                    | (u32::from(n.held()) << 8)
                    | (u32::from(n.sostenuto()) << 9)
                    | (u32::from(n.pressure().unwrap_or(128)) << 16),
            );
        }
        words.extend((0..128).map(|cc| u32::from(c.controller(cc).unwrap_or(128))));
        words.push(u32::from(c.program().unwrap_or(128)));
        words.extend(c.program_bank().map(|v| u32::from(v.unwrap_or(128))));
        words.extend([
            u32::from(c.pitch_bend()),
            u32::from(c.pressure().unwrap_or(128)),
            u32::from(c.reset_controllers_seen()),
            0,
            0,
        ]);
    }
    words.push(u32::from(initial.system_reset_seen()));
    d.write(d.scratch + 1024, &words);
    let view = f.midi.begin_import(&initial).unwrap();
    assert_eq!(d.issue(8, [d.scratch + 1024, 0, 0, 0, 0]), 0);
    let rtl_view = d.sim.model.get("response_handle");
    let records = [
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
    ];
    f.midi.import_events(view, &records).unwrap();
    for record in records {
        let packet = record
            .message
            .data()
            .iter()
            .enumerate()
            .fold((record.message.data().len() as u64) << 24, |n, (i, b)| {
                n | u64::from(*b) << (i * 8)
            });
        assert_eq!(d.issue(9, [rtl_view, record.frame, packet, 0, 0]), 0);
    }
    loop {
        let done = f.midi.finish_import(view, 32, 256).unwrap();
        assert_eq!(d.issue(10, [rtl_view, 32, 256, 0, 0]), 0);
        assert_eq!(d.sim.model.get("response_count"), u64::from(done));
        if done {
            break;
        }
    }
    f.pipeline.replace_midi(&mut f.midi, &[0], view).unwrap();
    assert_eq!(d.issue(5, [0, rtl_view, 0, 0, 0]), 0);
    f.pipeline.duplicate_midi(&mut f.midi, 0, 1).unwrap();
    assert_eq!(d.issue(6, [0, 1, 0, 0, 0]), 0);
    f.midi.release(view).unwrap();
    assert_eq!(d.issue(13, [rtl_view, 0, 0, 0, 0]), 0);
    let export = f.midi.export(view).unwrap();
    assert_eq!(d.issue(11, [0, 0, 0, 0, 0]), 0);
    let rtl_export = d.sim.model.get("response_handle");
    assert_eq!(d.read(d.sim.model.get("response_state"), 4225), words);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    compare_quantum(&mut f, &mut d, &[]);
    f.command(1, Operation::Record);
    compare_quantum(&mut f, &mut d, &[event(0, &[0xb1, 7, 70])]);
    f.command(1, Operation::Stop);
    compare_quantum(&mut f, &mut d, &[]);
    for (i, record) in records.iter().enumerate() {
        assert_eq!(d.issue(12, [rtl_export, i as u64, 0, 0, 0]), 0);
        assert_eq!(d.sim.model.get("response_frame"), record.frame);
    }
    let imported = f.midi.begin_import(&initial).unwrap();
    assert_eq!(d.issue(8, [d.scratch + 1024, 0, 0, 0, 0]), 0);
    let rtl_import = d.sim.model.get("response_handle");
    f.midi.import_events(imported, &records).unwrap();
    for record in records {
        let packet = record
            .message
            .data()
            .iter()
            .enumerate()
            .fold((record.message.data().len() as u64) << 24, |n, (i, b)| {
                n | u64::from(*b) << (i * 8)
            });
        assert_eq!(d.issue(9, [rtl_import, record.frame, packet, 0, 0]), 0);
    }
    loop {
        let done = f.midi.finish_import(imported, 32, 256).unwrap();
        assert_eq!(d.issue(10, [rtl_import, 32, 256, 0, 0]), 0);
        assert_eq!(d.sim.model.get("response_count"), u64::from(done));
        if done {
            break;
        }
        compare_quantum(&mut f, &mut d, &[]);
    }
    f.pipeline
        .replace_midi(&mut f.midi, &[1], imported)
        .unwrap();
    assert_eq!(d.issue(5, [1, rtl_import, 0, 0, 0]), 0);
    f.midi.release(export).unwrap();
    assert_eq!(d.issue(13, [rtl_export, 0, 0, 0, 0]), 0);
}

#[shoop_test]
fn midi_ddr_output_refusal_recovery_and_partial_stop_cleanup_match_rust() {
    use oracle::*;
    use rt_fpga_testbench::suites::midi::runtime::{Driver, compile};
    let compiled = compile(2).unwrap();
    let mut d = Driver::new(&compiled, 2).unwrap();
    let mut f = Fixture::new(16, 2);
    f.command(0, Operation::Record);
    compare_quantum(
        &mut f,
        &mut d,
        &[
            event(0, &[0x90, 60, 80]),
            event(0, &[0x90, 61, 90]),
            event(1, &[0xb0, 64, 127]),
        ],
    );
    compare_quantum(
        &mut f,
        &mut d,
        &[event(0, &[0x80, 60, 0]), event(1, &[0x80, 61, 0])],
    );
    f.command(0, Operation::Stop);
    compare_quantum(&mut f, &mut d, &[]);
    f.capacity = 1;
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    compare_quantum(&mut f, &mut d, &[]);
    compare_quantum(&mut f, &mut d, &[]);
    f.capacity = usize::MAX;
    compare_quantum(&mut f, &mut d, &[]);
    f.command(0, Operation::Stop);
    f.capacity = 1;
    compare_quantum(&mut f, &mut d, &[]);
    compare_quantum(&mut f, &mut d, &[]);
    compare_quantum(&mut f, &mut d, &[]);
    f.capacity = usize::MAX;
    compare_quantum(&mut f, &mut d, &[]);
}

#[shoop_test]
fn midi_ddr_axi32_and_axi64_record_playback_bounds_and_faults() {
    for width in [32, 64] {
        rt_fpga_testbench::suites::midi::axi_conformance(width).unwrap();
    }
}

#[shoop_test]
fn midi_ddr_128_loop_group_grab_and_eight_destination_playback_match_rust() {
    use oracle::*;
    use rt_fpga_testbench::suites::midi::runtime::{Driver, compile_limits};
    let mut limits = SessionLimits::session();
    limits.input_events_per_quantum = 16;
    let compiled = compile_limits(limits).unwrap();
    let mut d = Driver::new(&compiled, 128).unwrap();
    let mut f = Fixture::with_limits(16, limits);
    f.midi.set_history_retention(0, 128).unwrap();
    let events: Vec<_> = (0..16)
        .map(|ch| event(0, &[0x90 | ch, 60 + ch, 80]))
        .collect();
    let start = f.pipeline.state().frame();
    assert_eq!(f.run(&events).error, None);
    let plans: Vec<_> = (0..128)
        .map(|i| f.pipeline.state().loop_state(id(i)).unwrap().render)
        .collect();
    assert_eq!(d.process(start, 16, &plans, &events), 0);
    assert!(d.messages().is_empty());
    let slots: Vec<u16> = (0..128).collect();
    f.pipeline.grab_midi(&mut f.midi, 0, 0, 16, &slots).unwrap();
    d.write(d.scratch + 512, &(0..128).collect::<Vec<u32>>());
    assert_eq!(d.issue(4, [0, 0, 16, d.scratch + 512, 128]), 0);
    for slot in 0..128 {
        f.command(
            slot,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
    }
    let start = f.pipeline.state().frame();
    assert_eq!(f.run(&[]).error, None);
    let plans: Vec<_> = (0..128)
        .map(|i| f.pipeline.state().loop_state(id(i)).unwrap().render)
        .collect();
    assert_eq!(d.process(start, 16, &plans, &[]), 0);
    assert_eq!(d.messages(), f.emitted);
    assert_eq!(f.emitted.len(), 128);
    for destination in 0..8 {
        d.verify_state(
            1,
            destination,
            f.midi.output_state(destination as u16).unwrap(),
        );
    }
    assert_eq!(d.issue(11, [0, 0, 0, 0, 0]), 0);
    let first = d.sim.model.get("response_handle");
    assert_eq!(d.issue(11, [127, 0, 0, 0, 0]), 0);
    assert_eq!(d.sim.model.get("response_handle"), first);
}

#[shoop_test]
fn midi_ddr_all_quantum_sizes_and_history_checkpoint_edges_match_rust() {
    use oracle::*;
    use rt_fpga_testbench::suites::midi::runtime::{Driver, compile};
    let compiled = compile(2).unwrap();
    for q in [1, 16, 32, 64, 128] {
        let mut d = Driver::new(&compiled, 2).unwrap();
        d.sim.model.set("quantum_frames", u64::from(q));
        let mut f = Fixture::new(q, 2);
        f.command(0, Operation::Record);
        compare_quantum(
            &mut f,
            &mut d,
            &[
                event(0, &[0xb0, 0, 3]),
                event(0, &[0xc0, 9]),
                event(q - 1, &[0xb0, 0, 8]),
            ],
        );
        f.command(0, Operation::Stop);
        compare_quantum(&mut f, &mut d, &[]);
        f.command(
            0,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
        compare_quantum(&mut f, &mut d, &[]);
        f.command(0, Operation::Stop);
        compare_quantum(&mut f, &mut d, &[]);
    }
    let mut d = Driver::new(&compiled, 2).unwrap();
    let mut f = Fixture::new(16, 2);
    let controllers: Vec<u8> = (10..64).filter(|cc| *cc != 38).collect();
    for n in 0..6 {
        let events: Vec<_> = (0..8)
            .map(|k| {
                event(
                    k,
                    &[0xb0, controllers[(n * 8 + k) as usize], (n * 8 + k) as u8],
                )
            })
            .collect();
        compare_quantum(&mut f, &mut d, &events);
    }
    for (start, end) in [
        (0, 16),
        (55, 87),
        (56, 88),
        (57, 89),
        (64, 80),
        (80, 80),
        (88, 96),
    ] {
        if start == end {
            assert_eq!(
                f.midi.history_range(0, start, end).err(),
                Some(Error::InvalidEvent)
            );
            assert_eq!(d.issue(15, [0, start, end, 0, 0]), 1);
            continue;
        }
        let view = f.midi.history_range(0, start, end).unwrap();
        assert_eq!(d.issue(15, [0, start, end, 0, 0]), 0);
        let handle = d.sim.model.get("response_handle");
        let desc = f.midi.store().describe(view).unwrap();
        assert_eq!(d.issue(12, [handle, 0, 0, 0, 0]), 0);
        let record = f.midi.store().read(view, 0).unwrap();
        assert_eq!(
            d.sim.model.get("response_length"),
            record.map_or(0, |r| r.message.data().len() as u64)
        );
        if let Some(record) = record {
            assert_eq!(d.sim.model.get("response_frame"), record.frame);
        }
        d.verify_memory_state(
            d.sim.model.get("response_state"),
            f.midi.store().state(desc.initial).unwrap(),
        );
        f.midi.release(view).unwrap();
        assert_eq!(d.issue(13, [handle, 0, 0, 0, 0]), 0);
        assert_eq!(desc.frames, end - start);
    }
}

#[shoop_test]
fn midi_ddr_capacity_protects_capture_and_rejects_private_writer_edits() {
    use oracle::*;
    use rt_fpga_testbench::suites::midi::runtime::{Driver, compile};
    let compiled = compile(2).unwrap();
    let mut d = Driver::new(&compiled, 2).unwrap();
    let mut f = Fixture::new(16, 2);
    f.command(0, Operation::Record);
    compare_quantum(&mut f, &mut d, &[]);
    let record = Record {
        frame: 0,
        order: 0,
        message: msg(&[0xb0, 1, 50]),
    };
    for slot in [0, 1] {
        assert_eq!(
            f.midi
                .import_events(
                    ViewId {
                        slot,
                        generation: 1
                    },
                    &[record]
                )
                .err(),
            Some(Error::InvalidState)
        );
        assert_eq!(
            d.issue(9, [u64::from(slot) | (1 << 32), 0, 0x033201b0, 0, 0]),
            4
        );
    }
    let initial = MidiState::default();
    d.write_state(d.scratch + 1024, &initial);
    let mut staged = Vec::new();
    loop {
        let result = f.midi.begin_import(&initial);
        let error = d.issue(8, [d.scratch + 1024, 0, 0, 0, 0]);
        match result {
            Ok(view) => {
                assert_eq!(error, 0);
                staged.push((view, d.sim.model.get("response_handle")));
            }
            Err(e) => {
                assert_eq!(e, Error::Capacity);
                assert_eq!(error, 3);
                break;
            }
        }
    }
    assert!(!staged.is_empty());
    let mut full = false;
    for (view, handle) in staged {
        for frame in 0..32 {
            let record = Record {
                frame,
                order: frame as u32,
                message: msg(&[0xb0, 1, 50]),
            };
            let result = f.midi.import_events(view, &[record]);
            let error = d.issue(9, [handle, frame, 0x033201b0, 0, 0]);
            match result {
                Ok(_) => assert_eq!(error, 0),
                Err(Error::Busy) => {
                    assert_eq!(error, 5);
                    compare_quantum(&mut f, &mut d, &[]);
                    assert_eq!(f.midi.import_events(view, &[record]).unwrap(), 1);
                    assert_eq!(d.issue(9, [handle, frame, 0x033201b0, 0, 0]), 0);
                }
                Err(Error::Capacity) => {
                    assert_eq!(error, 3);
                    full = true;
                    break;
                }
                Err(e) => panic!("unexpected import refusal: {e:?}"),
            }
        }
        if full {
            break;
        }
    }
    assert!(full);
    compare_quantum(&mut f, &mut d, &[event(0, &[0x90, 60, 80])]);
    f.command(0, Operation::Stop);
    compare_quantum(&mut f, &mut d, &[]);
}

#[shoop_test]
fn midi_ddr_recording_exhaustion_keeps_input_tracking_and_rejects_playback() {
    use oracle::*;
    use rt_fpga_testbench::suites::midi::runtime::{Driver, compile};
    let compiled = compile(2).unwrap();
    let mut d = Driver::new(&compiled, 2).unwrap();
    let mut f = Fixture::new(16, 2);
    f.midi.set_record_limit(0, 1).unwrap();
    assert_eq!(d.issue(21, [0, 1, 0, 0, 0]), 0);
    f.command(0, Operation::Record);
    compare_quantum(
        &mut f,
        &mut d,
        &[event(0, &[0x90, 60, 80]), event(1, &[0x90, 61, 90])],
    );
    f.command(0, Operation::Stop);
    compare_quantum(&mut f, &mut d, &[]);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    compare_quantum(&mut f, &mut d, &[]);
    assert_eq!(
        f.midi
            .input_state(0)
            .unwrap()
            .channel(0)
            .unwrap()
            .note(61)
            .unwrap()
            .velocity(),
        90
    );
}

#[shoop_test]
fn midi_ddr_group_replace_clear_and_metadata_grants_are_atomic() {
    use oracle::*;
    use rt_fpga_testbench::suites::midi::runtime::{Driver, compile};
    let compiled = compile(2).unwrap();
    let mut d = Driver::new(&compiled, 2).unwrap();
    let mut f = Fixture::new(16, 2);
    compare_quantum(&mut f, &mut d, &[event(0, &[0x90, 60, 80])]);
    let view = f.midi.history_range(0, 0, 16).unwrap();
    assert_eq!(d.issue(15, [0, 0, 16, 0, 0]), 0);
    let rtl_view = d.sim.model.get("response_handle");
    d.write(d.scratch + 512, &[0, 1]);
    f.pipeline.replace_midi(&mut f.midi, &[0, 1], view).unwrap();
    assert_eq!(d.issue(23, [rtl_view, d.scratch + 512, 2, 0, 0]), 0);
    for _ in 0..63 {
        f.midi.export(view).unwrap();
        assert_eq!(d.issue(11, [0, 0, 0, 0, 0]), 0);
    }
    assert_eq!(f.midi.export(view).err(), Some(Error::Busy));
    assert_eq!(d.issue(11, [0, 0, 0, 0, 0]), 5);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    compare_quantum(&mut f, &mut d, &[]);
    d.write(d.scratch + 512, &[0, 0]);
    assert!(f.pipeline.replace_midi(&mut f.midi, &[0, 0], view).is_err());
    assert_eq!(d.issue(23, [rtl_view, d.scratch + 512, 2, 0, 0]), 4);
    compare_quantum(&mut f, &mut d, &[]);
    d.write(d.scratch + 512, &[0, 1]);
    f.pipeline.clear_midi(&mut f.midi, &[0, 1]).unwrap();
    assert_eq!(d.issue(22, [d.scratch + 512, 2, 0, 0, 0]), 0);
    compare_quantum(&mut f, &mut d, &[]);
}

#[shoop_test]
fn midi_ddr_independent_input_histories_and_incremental_snapshots_match_rust() {
    use oracle::*;
    use rt_fpga_testbench::suites::midi::runtime::{Driver, compile_limits};
    let mut l = limits(2);
    l.inputs = 2;
    let compiled = compile_limits(l).unwrap();
    let mut d = Driver::with_inputs(&compiled, 2, 2).unwrap();
    let mut f = Fixture::with_limits(16, l);
    let other = |offset, bytes: &[u8]| InputEvent {
        input: 1,
        offset,
        message: msg(bytes),
    };
    compare_quantum(&mut f, &mut d, &[other(0, &[0xb1, 1, 50])]);
    let sent = OutputEvent {
        destination: 0,
        offset: 7,
        message: msg(&[0xb1, 1, 90]),
    };
    let mut emitted = Vec::new();
    f.midi
        .send(sent, &mut |e| {
            emitted.push(e);
            Ok(())
        })
        .unwrap();
    assert_eq!(d.issue(14, [0, 0x035a01b1, 7, 0, 0]), 0);
    assert_eq!(d.messages(), emitted);
    f.command(0, Operation::Record);
    f.command(1, Operation::Record);
    compare_quantum(
        &mut f,
        &mut d,
        &[event(0, &[0x90, 60, 80]), other(1, &[0x91, 64, 90])],
    );
    d.verify_state(0, 1, f.midi.input_state(1).unwrap());
    f.command(0, Operation::Stop);
    f.command(1, Operation::Stop);
    compare_quantum(&mut f, &mut d, &[other(15, &[0xb1, 64, 127])]);
    for slot in 0..2 {
        f.command(
            slot,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
    }
    compare_quantum(&mut f, &mut d, &[]);
    f.command(0, Operation::Stop);
    f.command(1, Operation::Stop);
    compare_quantum(&mut f, &mut d, &[]);
    f.pipeline.grab_midi(&mut f.midi, 1, 16, 32, &[1]).unwrap();
    d.write(d.scratch + 512, &[1]);
    assert_eq!(d.issue(4, [1, 16, 32, d.scratch + 512, 1]), 0);
    f.command(
        1,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    compare_quantum(&mut f, &mut d, &[]);
    d.verify_state(0, 1, f.midi.input_state(1).unwrap());
}

#[shoop_test]
fn midi_ddr_existing_channel_owner_keeps_its_note_off_before_handoff() {
    use oracle::*;
    use rt_fpga_testbench::suites::midi::runtime::{Driver, compile};
    let compiled = compile(2).unwrap();
    let mut d = Driver::new(&compiled, 2).unwrap();
    let mut f = Fixture::new(16, 2);
    f.midi = MidiSession::prepare(limits(2)).unwrap();
    assert_eq!(d.issue(0, [0; 5]), 0);
    assert_eq!(d.issue(16, [0, 128, 0, 0, 0]), 0);
    for slot in 0..2 {
        f.midi
            .bind(
                slot,
                Binding {
                    loop_id: id(usize::from(slot)),
                    input: 0,
                    destination: 0,
                    channels: 1,
                },
            )
            .unwrap();
        assert_eq!(
            d.issue(1, [u64::from(slot), u64::from(slot) | (1 << 32), 0, 0, 1]),
            0
        );
        f.command(usize::from(slot), Operation::Record);
    }
    compare_quantum(&mut f, &mut d, &[event(0, &[0x90, 60, 80])]);
    compare_quantum(&mut f, &mut d, &[event(5, &[0x80, 60, 0])]);
    f.command(0, Operation::Stop);
    f.command(1, Operation::Stop);
    compare_quantum(&mut f, &mut d, &[]);
    f.command(
        0,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    compare_quantum(&mut f, &mut d, &[]);
    f.command(
        1,
        Operation::Play {
            position: 0,
            retrigger: true,
        },
    );
    compare_quantum(&mut f, &mut d, &[]);
    f.command(0, Operation::Stop);
    compare_quantum(&mut f, &mut d, &[]);
    compare_quantum(&mut f, &mut d, &[]);
}
