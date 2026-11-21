#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_reference::midi::{performance::*, *};
use rt_fpga_testbench::{
    clock::Edges,
    model::{Design, Model},
    sim::{Agent, Sim},
    suites::midi::Memory,
};
use shoop_wasm_test_support::shoop_test;
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
struct Packets {
    packets: Vec<Vec<u8>>,
    refused: Option<usize>,
    ticks: u64,
}
impl Agent for Packets {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if e.rises(0) {
            self.ticks += 1;
            m.set("packet_ready", u64::from(self.ticks % 4 != 0));
            m.set(
                "packet_refused",
                u64::from(self.refused == Some(self.packets.len())),
            );
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0)
            && m.get("packet_valid") != 0
            && m.get("packet_ready") != 0
            && m.get("packet_refused") == 0
        {
            let p = m.get("packet") as u32;
            self.packets
                .push(p.to_le_bytes()[..((p >> 24) & 3) as usize].to_vec());
        }
    }
}
fn msg(b: &[u8]) -> Message {
    Message::parse(b).unwrap()
}
fn packed(b: &[u8]) -> u64 {
    let mut p = (b.len() as u64) << 24;
    for (i, b) in b.iter().enumerate() {
        p |= u64::from(*b) << (8 * i);
    }
    p
}
fn command(sim: &mut Sim<'_>, op: u64, args: [u64; 4]) -> u64 {
    sim.model.set("command_op", op);
    for (i, v) in args.iter().enumerate() {
        sim.model.set(&format!("arg{i}"), *v);
    }
    sim.model.set("command_valid", 1);
    sim.until(100, "state command admission", |m| {
        m.get("command_ready") != 0
    });
    sim.idle(1);
    sim.model.set("command_valid", 0);
    sim.until(5_000_000, "state command completion", |m| {
        m.get("done") != 0
    });
    let e = sim.model.get("error");
    sim.idle(1);
    e
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
            assert_eq!(
                command(&mut sim, 10, [1, 0, f as u64, target.word(f) as u64]),
                0
            );
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
        reset.known_reset(1);
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
    }
}
