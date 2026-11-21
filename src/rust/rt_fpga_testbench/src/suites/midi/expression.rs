//! Isolated expression data-job measurements with checked Rust results.
use super::Memory;
use crate::{
    clock::Edges,
    model::{Design, Model},
    sim::{Agent, Sim},
};
use rt_fpga_reference::midi::{performance::*, *};
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
pub struct Packets {
    pub packets: Vec<Vec<u8>>,
    pub refused: Option<usize>,
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
pub fn packed(b: &[u8]) -> u64 {
    let mut p = (b.len() as u64) << 24;
    for (i, b) in b.iter().enumerate() {
        p |= u64::from(*b) << (8 * i);
    }
    p
}
pub fn command(sim: &mut Sim<'_>, op: u64, args: [u64; 4]) -> u64 {
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

pub fn scaling() -> crate::Result<serde_json::Value> {
    let design = Design::new(
        "midi_expression_state",
        vec![crate::root().join("fpga/midi/rtl/midi_expression_state.sv")],
    )
    .parameters(&[
        ("LOOPS", 128),
        ("INPUTS", 1),
        ("DESTINATIONS", 8),
        ("VIEWS", 256),
    ]);
    let compiled = design
        .build(&crate::root().join("work/marlin-suite/midi-expression-scaling"))
        .unwrap();
    let mut measurements = Vec::new();
    for delay in [0, 7] {
        let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
        sim.max_ps = 1_000_000_000_000_000;
        let memory = Rc::new(RefCell::new(Memory::fixed(
            sim.model.get("arena_bytes") as usize / 4 + 16,
            delay,
        )));
        let packets = Rc::new(RefCell::new(Packets::default()));
        sim.agent(memory.clone());
        sim.agent(packets.clone());
        sim.model.set("reset", 1);
        sim.idle(2);
        sim.model.set("reset", 0);
        assert_eq!(command(&mut sim, 0, [0; 4]), 0);
        let handle = 1u64 << 32;
        assert_eq!(command(&mut sim, 10, [1, 0, 129, u64::from(KNOWN | 20)]), 0);
        assert_eq!(command(&mut sim, 8, [handle, 0, 0, 0]), 0);
        assert_eq!(
            command(&mut sim, 10, [2, handle, 129, u64::from(KNOWN | 20)]),
            0
        );
        let mut m = MidiMemory::prepare(
            StoreLimits {
                block_events: 8,
                blocks: 16,
                max_events: 32,
                views: 256,
                retirements: 256,
            },
            128,
        )
        .unwrap();
        m.prepare_state(1, 8).unwrap();
        let mut state = State::new();
        state.set_word(129, KNOWN | 20).unwrap();
        m.expression
            .as_mut()
            .unwrap()
            .seed_output(0, &state, &m.store)
            .unwrap();
        let view = m.store.writer().unwrap();
        m.store.seed_snapshot(view, &state, 1).unwrap();
        m.store.finish(view, 64).unwrap();
        let mut bound = 0;
        for n in [1, 8, 64, 128] {
            for seq in bound..n {
                let owner = rt_fpga_reference::records::Identity {
                    slot: seq,
                    generation: 1,
                };
                m.replace(SequenceId(seq as u16), Some(view)).unwrap();
                m.bind_state(
                    SequenceId(seq as u16),
                    StateBinding {
                        owner,
                        input: 0,
                        outputs: 1,
                        channels: 1,
                    },
                )
                .unwrap();
                assert_eq!(
                    command(
                        &mut sim,
                        9,
                        [
                            u64::from(seq),
                            handle,
                            (1u64 << 32) | u64::from(seq),
                            0x10100
                        ]
                    ),
                    0
                );
            }
            bound = n;
            let start = sim.cycles;
            let requests = memory.borrow().transfers;
            assert_eq!(
                command(
                    &mut sim,
                    2,
                    [0, packed(&[0xb0, 1, 90]), u64::from(u32::MAX), 0]
                ),
                0
            );
            let update_cycles = sim.cycles - start;
            let update_requests = memory.borrow().transfers - requests;
            let engine = m.expression.as_mut().unwrap();
            engine.work = StateWork::default();
            let t = std::time::Instant::now();
            engine
                .accept(0, Message::parse(&[0xb0, 1, 90]).unwrap(), None, &m.store)
                .unwrap();
            let cpu_update_ns = t.elapsed().as_nanos() as u64;
            assert_eq!(engine.work.tracker_visits, u64::from(n));
            let start = sim.cycles;
            let requests = memory.borrow().transfers;
            packets.borrow_mut().packets.clear();
            assert_eq!(command(&mut sim, 5, [0, handle, 0, 1u64 << 32]), 0);
            let restore_cycles = sim.cycles - start;
            let restore_requests = memory.borrow().transfers - requests;
            assert_eq!(packets.borrow().packets, vec![vec![0xb0, 1, 20]]);
            let mut count = 0;
            let t = std::time::Instant::now();
            engine
                .restore(0, view, 0, 0, &m.store, &mut |_| {
                    count += 1;
                    Ok(())
                })
                .unwrap();
            let cpu_restore_ns = t.elapsed().as_nanos() as u64;
            assert_eq!(count, 1);
            for seq in 0..n {
                assert_eq!(engine.differences(seq as u16, 0).unwrap().count, 0);
            }
            let start = sim.cycles;
            let requests = memory.borrow().transfers;
            assert_eq!(command(&mut sim, 5, [0, handle, 0, 1u64 << 32]), 0);
            let quiet_cycles = sim.cycles - start;
            assert_eq!(memory.borrow().transfers, requests);
            let start = sim.cycles;
            let requests = memory.borrow().transfers;
            let visits = engine.work.tracker_visits;
            engine
                .accept(
                    0,
                    Message::parse(&[0xb1, 1, (n % 128) as u8]).unwrap(),
                    None,
                    &m.store,
                )
                .unwrap();
            assert_eq!(engine.work.tracker_visits, visits);
            assert_eq!(
                command(
                    &mut sim,
                    2,
                    [
                        0,
                        packed(&[0xb1, 1, (n % 128) as u8]),
                        u64::from(u32::MAX),
                        0
                    ]
                ),
                0
            );
            let unrelated_cycles = sim.cycles - start;
            let unrelated_requests = memory.borrow().transfers - requests;
            assert_eq!(unrelated_requests, 2);
            measurements.push(serde_json::json!({"trackers":n,"service_delay_clocks_per_word":delay,"update_cycles":update_cycles,"update_requests":update_requests,
                "restore_cycles":restore_cycles,"restore_requests":restore_requests,"quiet_restore_cycles":quiet_cycles,"unrelated_update_cycles":unrelated_cycles,
                "cpu_update_ns":cpu_update_ns,"cpu_restore_ns":cpu_restore_ns,"corrections":1,"tracker_visits_including_correction":engine.work.tracker_visits}));
        }
    }
    Ok(
        serde_json::json!({"scope":"isolated shared-JP expression functional unit; 32-bit word interface with fixed service latency, not the full AXI quantum benchmark", "clock_mhz":100,"validation":"Rust comparisons, one correction returning every target to clean; quiet restore makes no DDR requests; unrelated channel has only two current-state accesses", "measurements":measurements}),
    )
}
