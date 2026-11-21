#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_reference::midi::{performance::*, *};
use rt_fpga_testbench::suites::midi::expression::{Packets, command, packed};
use rt_fpga_testbench::{model::Design, sim::Sim, suites::midi::Memory};
use shoop_wasm_test_support::shoop_test;
use std::{cell::RefCell, rc::Rc};
fn msg(b: &[u8]) -> Message {
    Message::parse(b).unwrap()
}
#[shoop_test]
fn ddr_expression_fields_bitmaps_and_pedal_repairs_match_rust() {
    let compiled = Design::new(
        "midi_expression_state",
        vec![rt_fpga_testbench::root().join("fpga/midi/rtl/midi_expression_state.sv")],
    )
    .parameters(&[
        ("LOOPS", 2),
        ("INPUTS", 1),
        ("DESTINATIONS", 2),
        ("VIEWS", 8),
    ])
    .build(&rt_fpga_testbench::root().join("work/marlin-suite/midi-expression"))
    .unwrap();
    let cases: &[&[&[u8]]] = &[
        &[],
        &[&[0xb0, 7, 55]],
        &[
            &[0x90, 60, 80],
            &[0xa0, 60, 40],
            &[0xe0, 22, 80],
            &[0xd0, 60],
            &[0xb0, 67, 90],
        ],
        &[&[0xb0, 1, 20]],
        &[&[0x90, 60, 70], &[0xb0, 64, 127], &[0x80, 60, 0]],
        &[
            &[0x90, 60, 70],
            &[0xb0, 66, 127],
            &[0x90, 64, 90],
            &[0x80, 60, 0],
            &[0xa0, 60, 33],
        ],
    ];
    for case in cases {
        let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
        sim.max_ps = 1_000_000_000_000_000;
        let memory = Rc::new(RefCell::new(Memory::new(
            sim.model.get("arena_bytes") as usize / 4 + 16,
        )));
        let packets = Rc::new(RefCell::new(Packets::default()));
        sim.agent(memory.clone());
        sim.agent(packets.clone());
        sim.agent(rt_fpga_testbench::protocol::ReadyValid::new(
            "memory_valid",
            "memory_ready",
            &["memory_address", "memory_write", "memory_write_data"],
        ));
        sim.agent(rt_fpga_testbench::protocol::ReadyValid::new(
            "packet_valid",
            "packet_ready",
            &["packet", "destination"],
        ));
        sim.model.set("reset", 1);
        sim.idle(2);
        sim.model.set("reset", 0);
        assert_eq!(command(&mut sim, 0, [0; 4]), 0);
        let mut target = State::new();
        target.known_reset(1);
        for f in 0..258 {
            assert_eq!(
                command(&mut sim, 10, [0, 0, f as u64, target.word(f) as u64]),
                0
            );
            if !case.is_empty() {
                assert_eq!(
                    command(&mut sim, 10, [1, 0, f as u64, target.word(f) as u64]),
                    0
                );
            }
        }
        for bytes in *case {
            target.apply(msg(bytes));
            assert_eq!(command(&mut sim, 1, [0, packed(bytes), 0, 0]), 0);
        }
        let mut m = MidiMemory::prepare(
            StoreLimits {
                block_events: 8,
                blocks: 16,
                max_events: 32,
                views: 8,
                retirements: 8,
            },
            2,
        )
        .unwrap();
        m.prepare_state(1, 2).unwrap();
        let mut reset = State::new();
        if !case.is_empty() {
            reset.known_reset(1);
        }
        m.expression
            .as_mut()
            .unwrap()
            .seed_output(0, &reset, &m.store)
            .unwrap();
        let view = m.store.writer().unwrap();
        m.store.seed_snapshot(view, &target, 1).unwrap();
        m.store.finish(view, 64).unwrap();
        m.replace(SequenceId(0), Some(view)).unwrap();
        let handle = (u64::from(view.generation) << 32) | u64::from(view.slot);
        let owner = 1u64 << 32;
        m.bind_state(
            SequenceId(0),
            StateBinding {
                owner: rt_fpga_reference::records::Identity {
                    slot: 0,
                    generation: 1,
                },
                input: 0,
                outputs: 1,
                channels: 1,
            },
        )
        .unwrap();
        assert_eq!(command(&mut sim, 9, [0, 0, owner, 0x00010100]), 0);
        assert_eq!(command(&mut sim, 3, [0, handle, 0, 0]), 0);
        for bytes in [
            &[0xb0, 1, 90][..],
            &[0x90, 67, 80],
            &[0xb0, 64, 127],
            &[0x80, 67, 0],
        ] {
            m.expression
                .as_mut()
                .unwrap()
                .accept(0, msg(bytes), None, &m.store)
                .unwrap();
            assert_eq!(
                command(&mut sim, 2, [0, packed(bytes), u32::MAX as u64, 0]),
                0
            );
        }
        let mut expected = Vec::new();
        m.expression
            .as_mut()
            .unwrap()
            .restore(0, view, 0, 0, &m.store, &mut |e| {
                expected.push(e.message.as_bytes().to_vec());
                Ok(())
            })
            .unwrap();
        assert_eq!(command(&mut sim, 5, [0, handle, 0, owner]), 0);
        assert_eq!(packets.borrow().packets, expected);
        m.expression
            .as_mut()
            .unwrap()
            .accept(0, msg(&[0xb0, 1, 99]), None, &m.store)
            .unwrap();
        assert_eq!(
            command(
                &mut sim,
                2,
                [0, packed(&[0xb0, 1, 99]), u64::from(u32::MAX), 0]
            ),
            0
        );
        let mut expected = Vec::new();
        m.expression
            .as_mut()
            .unwrap()
            .restore(0, view, 0, 0, &m.store, &mut |e| {
                expected.push(e.message.as_bytes().to_vec());
                Ok(())
            })
            .unwrap();
        assert_eq!(
            expected.len(),
            1,
            "controller-only repair must keep matching notes playing"
        );
        packets.borrow_mut().packets.clear();
        assert_eq!(command(&mut sim, 5, [0, handle, 0, owner]), 0);
        assert_eq!(packets.borrow().packets, expected);
        {
            let words = &memory.borrow().words;
            for f in 0..258 {
                assert_eq!(
                    words[8192 + f],
                    m.expression.as_ref().unwrap().output(0).unwrap().word(f),
                    "field {f}"
                );
            }
            let track_base = (1 + 2 + 8 + 1) * 8192;
            for f in 0..8192 {
                assert_eq!(
                    words[track_base + f / 32] & (1u32 << (f % 32)) != 0
                        && words[track_base + 256 + f / 1024] & (1u32 << ((f / 32) % 32)) != 0,
                    m.expression
                        .as_ref()
                        .unwrap()
                        .differences(0, 0)
                        .unwrap()
                        .contains(f),
                    "difference {f}"
                );
            }
        }
        packets.borrow_mut().packets.clear();
        assert_eq!(command(&mut sim, 5, [0, handle, 0, owner]), 0);
        assert!(packets.borrow().packets.is_empty());
        if case.is_empty() {
            let mut other = target.clone();
            other.apply(msg(&[0xb0, 1, 40]));
            let view1 = m.store.writer().unwrap();
            m.store.seed_snapshot(view1, &other, 1).unwrap();
            m.store.finish(view1, 64).unwrap();
            m.replace(SequenceId(1), Some(view1)).unwrap();
            m.bind_state(
                SequenceId(1),
                StateBinding {
                    owner: rt_fpga_reference::records::Identity {
                        slot: 1,
                        generation: 1,
                    },
                    input: 0,
                    outputs: 1,
                    channels: 1,
                },
            )
            .unwrap();
            let handle1 = (u64::from(view1.generation) << 32) | u64::from(view1.slot);
            assert_eq!(command(&mut sim, 9, [1, 0, owner | 1, 0x10100]), 0);
            assert_eq!(command(&mut sim, 1, [0, packed(&[0xb0, 1, 40]), 0, 0]), 0);
            assert_eq!(command(&mut sim, 3, [1, handle1, 0, 0]), 0);
            for (seq, view, handle, owner) in
                [(1, view1, handle1, owner | 1), (0, view, handle, owner)]
            {
                let mut expected = Vec::new();
                m.expression
                    .as_mut()
                    .unwrap()
                    .restore(seq, view, 0, 0, &m.store, &mut |e| {
                        expected.push(e.message.as_bytes().to_vec());
                        Ok(())
                    })
                    .unwrap();
                packets.borrow_mut().packets.clear();
                assert_eq!(command(&mut sim, 5, [seq as u64, handle, 0, owner]), 0);
                assert_eq!(packets.borrow().packets, expected);
            }
            let before = memory.borrow().transfers;
            assert_eq!(command(&mut sim, 6, [0, handle, 0, owner]), 0);
            assert_eq!(memory.borrow().transfers, before);
        }
        let policy = DEFAULT_CONTROLLERS | (1 << 7);
        m.expression
            .as_mut()
            .unwrap()
            .policy(0, policy, &m.store)
            .unwrap();
        assert_eq!(
            command(&mut sim, 12, [0, policy as u64, (policy >> 64) as u64, 0]),
            0
        );
        let mut expected = Vec::new();
        m.expression
            .as_mut()
            .unwrap()
            .restore(0, view, 0, 0, &m.store, &mut |e| {
                expected.push(e.message.as_bytes().to_vec());
                Ok(())
            })
            .unwrap();
        packets.borrow_mut().packets.clear();
        assert_eq!(command(&mut sim, 5, [0, handle, 0, owner]), 0);
        assert_eq!(packets.borrow().packets, expected);
        for bytes in [
            &[0xb0, 123, 0][..],
            &[0xb0, 120, 0],
            &[0xb0, 121, 0],
            &[0xff],
        ] {
            m.expression
                .as_mut()
                .unwrap()
                .accept(0, msg(bytes), None, &m.store)
                .unwrap();
            assert_eq!(
                command(&mut sim, 2, [0, packed(bytes), u64::from(u32::MAX), 0]),
                0
            );
            {
                let memory = memory.borrow();
                for ch in 0..16 {
                    for f in 0..258 {
                        let field = ch * 512 + f;
                        assert_eq!(
                            memory.words[8192 + field],
                            m.expression
                                .as_ref()
                                .unwrap()
                                .output(0)
                                .unwrap()
                                .word(field),
                            "reset field {field}, message {bytes:?}"
                        );
                    }
                }
            }
            let mut expected = Vec::new();
            m.expression
                .as_mut()
                .unwrap()
                .restore(0, view, 0, 0, &m.store, &mut |e| {
                    expected.push(e.message.as_bytes().to_vec());
                    Ok(())
                })
                .unwrap();
            packets.borrow_mut().packets.clear();
            assert_eq!(command(&mut sim, 5, [0, handle, 0, owner]), 0);
            assert_eq!(packets.borrow().packets, expected);
        }
        let before = m.expression.as_ref().unwrap().output(0).unwrap().word(129);
        assert_eq!(
            command(&mut sim, 2, [0, packed(&[0xb0, 1, 99]), u32::MAX as u64, 0]),
            0
        );
        packets.borrow_mut().packets.clear();
        packets.borrow_mut().refused = Some(0);
        assert_eq!(command(&mut sim, 5, [0, handle, 0, owner]), 9);
        assert_eq!(command(&mut sim, 11, [1, 0, 129, 0]), 0);
        assert_eq!(sim.model.get("result"), u64::from(KNOWN | 99));
        assert_ne!(before, KNOWN | 99);
        assert_eq!(command(&mut sim, 12, [0, 1 << 6, 0, 0]), 2);
        assert_eq!(command(&mut sim, 5, [0, handle, 0, owner + (1 << 32)]), 4);
        memory.borrow_mut().fail = Some(8192 + 129);
        assert_eq!(command(&mut sim, 11, [1, 0, 129, 0]), 7);
    }
}

#[shoop_test]
fn expression_bounded_tables_and_relocated_arena_restore_last_owner() {
    for (loops, outputs, views) in [(1, 1, 1), (3, 3, 5)] {
        let compiled = Design::new(
            "midi_expression_state",
            vec![rt_fpga_testbench::root().join("fpga/midi/rtl/midi_expression_state.sv")],
        )
        .parameters(&[
            ("LOOPS", loops),
            ("INPUTS", 1),
            ("DESTINATIONS", outputs),
            ("VIEWS", views),
        ])
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/midi-expression-capacities"))
        .unwrap();
        let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
        sim.max_ps = 1_000_000_000_000_000;
        let base = 256;
        sim.model.set("arena_base", base);
        let memory = Rc::new(RefCell::new(Memory::new(
            (base + sim.model.get("arena_bytes")) as usize / 4 + 16,
        )));
        let packets = Rc::new(RefCell::new(Packets::default()));
        sim.agent(memory.clone());
        sim.agent(packets.clone());
        sim.model.set("reset", 1);
        sim.idle(2);
        sim.model.set("reset", 0);
        assert_eq!(command(&mut sim, 0, [0; 4]), 0);
        let sequence = (loops - 1) as u64;
        let output = (outputs - 1) as u64;
        let owner = (0x8000_0001u64 << 32) | sequence;
        let view = (1u64 << 32) | (views - 1) as u64;
        assert_eq!(command(&mut sim, 1, [0, packed(&[0xb0, 1, 20]), 0, 0]), 0);
        assert_eq!(
            command(
                &mut sim,
                2,
                [output, packed(&[0xb0, 1, 99]), u32::MAX as u64, 0]
            ),
            0
        );
        assert_eq!(
            command(
                &mut sim,
                9,
                [sequence, 0, owner, 0x10000 | (1 << (8 + output))]
            ),
            0
        );
        assert_eq!(command(&mut sim, 3, [sequence, view, 0, 0]), 0);
        assert_eq!(command(&mut sim, 5, [sequence, view, 0, owner]), 0);
        assert_eq!(packets.borrow().packets, vec![vec![0xb0, 1, 20]]);
        assert_eq!(sim.model.get("destination"), output);
        assert_eq!(command(&mut sim, 11, [1, output, 129, 0]), 0);
        assert_eq!(sim.model.get("result"), u64::from(KNOWN | 20));
        let transfers = memory.borrow().transfers;
        assert_eq!(command(&mut sim, 6, [sequence, view, 0, owner]), 0);
        assert_eq!(memory.borrow().transfers, transfers);
        assert!(
            memory.borrow().words[..base as usize / 4]
                .iter()
                .all(|v| *v == 0xdeadbeef)
        );
        if loops == 3 {
            assert_eq!(
                command(&mut sim, 2, [0, packed(&[0xb0, 1, 20]), u32::MAX as u64, 0]),
                0
            );
            for seq in 0..2 {
                let owner = (1u64 << 32) | seq;
                assert_eq!(command(&mut sim, 9, [seq, 0, owner, 0x10500]), 0);
                assert_eq!(command(&mut sim, 3, [seq, (1u64 << 32) | seq, 0, 0]), 0);
            }
            for seq in [0, 1] {
                assert_eq!(
                    command(&mut sim, 2, [2, packed(&[0xb0, 1, 90]), u32::MAX as u64, 0]),
                    0
                );
                packets.borrow_mut().packets.clear();
                assert_eq!(
                    command(
                        &mut sim,
                        5,
                        [seq, (1u64 << 32) | seq, 0, (1u64 << 32) | seq]
                    ),
                    0
                );
                assert_eq!(packets.borrow().packets, vec![vec![0xb0, 1, 20]]);
                assert_eq!(sim.model.get("destination"), 2);
                packets.borrow_mut().packets.clear();
                let other = 1 - seq;
                assert_eq!(
                    command(
                        &mut sim,
                        5,
                        [other, (1u64 << 32) | other, 0, (1u64 << 32) | other]
                    ),
                    0
                );
                assert!(packets.borrow().packets.is_empty());
            }
        }
    }
}

#[shoop_test]
fn compiled_expression_sp_jp_capture_repeat_seek_and_stop_match_rust() {
    use rt_fpga_reference::transitions::Operation;
    use rt_fpga_testbench::suites::midi::data::{event, pipeline};
    let mut design = pipeline::design(32);
    design
        .parameters
        .insert("MIDI_STATE_SUPPORTED".into(), "1".into());
    let compiled = design
        .build(&rt_fpga_testbench::root().join("work/marlin-suite/midi-expression-pipeline"))
        .unwrap();
    for mixed in [false, true] {
        let mut d = pipeline::Driver::build(&compiled, 32, mixed, true).unwrap();
        d.quantum(&[
            event(0, 0, &[0xb0, 1, 20]),
            event(0, 0, &[0x90, 60, 80]),
            event(0, 1, &[0xb0, 66, 127]),
        ]);
        d.command(0, Operation::Record);
        d.quantum(&[]);
        let open = d.reference.midi.view(SequenceId(0)).unwrap().unwrap();
        let open_handle = (u64::from(open.generation) << 32) | u64::from(open.slot);
        assert_eq!(
            d.control(22, [2, open_handle, 129, u64::from(KNOWN | 99), 0]),
            5
        );
        assert_eq!(
            d.reference.midi.store.seed_snapshot(open, &State::new(), 1),
            Err(Error::Busy)
        );
        d.quantum(&[event(0, 0, &[0xb0, 1, 90]), event(0, 2, &[0x80, 60, 0])]);
        d.quantum(&[
            event(0, 0, &[0x90, 64, 100]),
            event(0, 1, &[0xb0, 64, 127]),
            event(0, 2, &[0x80, 64, 0]),
        ]);
        d.command(
            0,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
        d.quantum(&[]);
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
        d.command(0, Operation::Stop);
        d.quantum(&[]);
        d.quantum(&[]);
        let original = d.reference.midi.view(SequenceId(0)).unwrap().unwrap();
        d.reference.midi.store.retain(original).unwrap();
        assert_eq!(d.control(11, [0; 5]), 0);
        let exported = d.sim.model.get("midi_response_handle");
        let range = d.reference.midi.store.range(original, 32, 64).unwrap();
        assert_eq!(d.control(15, [exported, 32, 64, 0, 0]), 0);
        let ranged = d.sim.model.get("midi_response_handle");
        for field in [60, 64, 129, 192, 194] {
            assert_eq!(d.control(24, [2, ranged, field, 0, 0]), 0);
            assert_eq!(
                d.sim.model.get("midi_response_count"),
                u64::from(
                    d.reference
                        .midi
                        .store
                        .snapshot(range)
                        .unwrap()
                        .word(field as usize)
                )
            );
        }
        d.reference
            .pipeline
            .replace_midi_view(
                &mut d.reference.midi,
                rt_fpga_testbench::suites::midi::data::id(0),
                SequenceId(0),
                Some(range),
            )
            .unwrap();
        assert_eq!(d.control(5, [0, ranged, 0, 0, 0]), 0);
        d.command(
            0,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
        d.quantum(&[]);
        d.command(0, Operation::Stop);
        d.quantum(&[]);
        d.command(0, Operation::Record);
        d.quantum(&[event(0, 0, &[0x90, 72, 100]), event(0, 2, &[0x80, 72, 0])]);
        d.command(0, Operation::Stop);
        d.quantum(&[]);
        assert_eq!(d.control(24, [2, exported, 129, 0, 0]), 0);
        assert_eq!(
            d.sim.model.get("midi_response_count"),
            u64::from(KNOWN | 20)
        );
    }
}

#[shoop_test]
fn expression_jobs_follow_all_supported_quantum_boundaries() {
    use rt_fpga_reference::transitions::Operation;
    use rt_fpga_testbench::suites::midi::data::{event, pipeline};
    for q in [1, 16, 64, 128] {
        let mut design = pipeline::design(q);
        design
            .parameters
            .insert("MIDI_STATE_SUPPORTED".into(), "1".into());
        let compiled = design
            .build(&rt_fpga_testbench::root().join("work/marlin-suite/midi-expression-quanta"))
            .unwrap();
        let mut d = pipeline::Driver::build(&compiled, q as u32, false, true).unwrap();
        d.quantum(&[
            event(0, 0, &[0xb0, 1, 20]),
            event(0, 0, &[0x90, 60, 80]),
            event(0, 0, &[0xb0, 66, 127]),
        ]);
        d.command(0, Operation::Record);
        d.quantum(&[event(0, 0, &[0xb0, 1, 90]), event(0, 0, &[0x80, 60, 0])]);
        d.command(
            0,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
        d.quantum(&[]);
        d.quantum(&[]);
        d.command(0, Operation::Stop);
        d.quantum(&[]);
    }
}

#[shoop_test]
fn expression_jobs_keep_state_through_axi32_and_axi64_stalls() {
    for width in [32, 64] {
        rt_fpga_testbench::suites::midi::data::axi::expression_conformance(width).unwrap();
    }
}
