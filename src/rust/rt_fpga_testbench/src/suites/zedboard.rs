//! Deployed runner controls and independently measured hardware counter boundaries.
use crate::{
    Result,
    clock::Edges,
    drivers::store::Geometry,
    memory::{Memory, Mode},
    model::{Design, Model},
    sim::{Agent, Sim},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};
#[derive(Default)]
struct Counters {
    cycle: u64,
    starts: Vec<u64>,
    ends: Vec<u64>,
}
impl Agent for Counters {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0) {
            if m.get("run_pulse") != 0 {
                self.starts.push(self.cycle);
            }
            if m.get("done_pulse") != 0 {
                self.ends.push(self.cycle);
            }
            self.cycle += 1;
        }
    }
}
pub fn prepare_example() -> Result<PathBuf> {
    let scenario: Value =
        serde_json::from_str(include_str!("../../fixtures/zedboard-scenario.json"))?;
    let loops = scenario["loops"].as_array().unwrap().len() as u64;
    let tracks = scenario["output_ports"].as_u64().unwrap();
    let q = scenario["quantum"].as_u64().unwrap();
    let record_quanta = 2u64;
    let views = 32.max(loops * 4);
    let slots = loops * (record_quanta.div_ceil(32) + 2) * 32;
    let p = crate::prepared::prepare_with_views(scenario, 32, slots, views)?;
    let image = &p.image;
    let limits = &image["transition"]["limits"];
    let parameters = json!({"LOOP_DEPTH":limits["loop_slots"],"REQUEST_DEPTH":limits["pending_requests"],"EVENT_DEPTH":limits["events"],"PROGRAM_DEPTH":2.max(image["program"]["instructions"].as_array().unwrap().len()),"STATE_DEPTH":2.max(image["initial_state"]["memory"].as_array().unwrap().len()),"JOB_DEPTH":2.max(image["program"]["jobs_per_quantum"].as_u64().unwrap()),"BUFFERS":2.max(image["buffers"].as_u64().unwrap()),"MAX_FRAMES":q,"CHUNK_FRAMES":q,"INPUT_PORTS":1,"OUTPUT_PORTS":tracks,"VIEWS":views,"SLOTS":slots,"BLOCK_CHUNKS":32,"RETIRED_TABLES":256,"RECLAIM_BLOCKS":0,"RECLAIM_CYCLES":5000,"RECLAIM_GUARD":1024,"AXI_DATA_WIDTH":64,"EXTERNAL_IO":1,"INSTRUCTION_COUNT":image["program"]["instructions"].as_array().unwrap().len(),"STATE_COUNT":image["initial_state"]["memory"].as_array().unwrap().len(),"BUFFER_COUNT":image["buffers"],"JOB_BUDGET":image["program"]["jobs_per_quantum"],"BOOT_WORDS":p.boot.len(),"EVENT_WORDS":p.events.len()});
    std::fs::write(
        p.directory.join("config.json"),
        serde_json::to_vec_pretty(
            &json!({"parameters":parameters,"warmup_quanta":record_quanta+8,"loops":loops,"tracks":tracks,"record_quanta":record_quanta,"mixed":true}),
        )?,
    )?;
    Ok(p.directory)
}
fn pulse(s: &mut Sim<'_>, name: &str) {
    s.model.set(name, 1);
    s.idle(1);
    s.model.set(name, 0);
    s.idle(1);
}
fn stopped(s: &mut Sim<'_>, quanta: u64) {
    s.until(
        2_000_000.max(quanta * 1_000_000),
        "benchmark stopped",
        |m| m.get("status") & 7 == 5,
    );
    s.idle(2);
}
fn stats(s: &Sim<'_>) -> Vec<u64> {
    let b = s.model.bits("statistics");
    (0..16).map(|i| b.get(i * 64, 64)).collect()
}
pub fn deployed_runner(workload: &Path, quanta: u64, controls: bool, mode: Mode) -> Result<Value> {
    assert_ne!(mode, Mode::Legacy);
    let config: Value = serde_json::from_slice(&std::fs::read(workload.join("config.json"))?)?;
    let p = &config["parameters"];
    let warmup = config["warmup_quanta"].as_u64().unwrap();
    assert!(quanta > warmup);
    let g = Geometry::new(
        [
            p["VIEWS"].as_u64().unwrap(),
            p["SLOTS"].as_u64().unwrap(),
            p["CHUNK_FRAMES"].as_u64().unwrap(),
        ],
        p["BLOCK_CHUNKS"].as_u64().unwrap(),
    );
    let mut parameters: Vec<_> = p
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_u64().unwrap()))
        .collect();
    parameters.extend([
        ("PHYSICAL_BASE", 0x01000000),
        ("READ_BUFFERS", 4),
        ("WRITE_BUFFERS", 4),
        ("TABLE_LINES", 8),
        ("REF_LINES", 4),
        ("STACK_LINES", 2),
    ]);
    let mut sources = super::continuous::sources(true);
    sources.push(crate::root().join("fpga/zedboard/rtl/timing_bench.sv"));
    let mut d = Design::new("timing_bench", sources).parameters(&parameters);
    d.includes.push(crate::root().join("fpga/zedboard/rtl"));
    for name in ["BOOT", "EVENT"] {
        d.parameters.insert(
            format!("{name}_FILE"),
            format!(
                "\"{}\"",
                workload
                    .join(format!(
                        "{}.hex",
                        if name == "BOOT" { "boot" } else { "events" }
                    ))
                    .display()
            ),
        );
    }
    if mode == Mode::Instant {
        d = d.memory(g.words(), 0x01000000);
    }
    let compiled = d.build(&crate::root().join("work/marlin-suite/zedboard"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    let memory = Memory::attach(
        &mut s,
        mode,
        g.words(),
        0x01000000,
        &crate::artifact_directory("zedboard/dram"),
        20261112,
    )?;
    let counters = Rc::new(RefCell::new(Counters::default()));
    s.agent(counters.clone());
    s.model.set_all(&[
        ("reset", 1),
        ("quanta_limit", quanta),
        ("warmup_quanta", warmup),
        ("gap_cycles", 17),
        ("watchdog_cycles", 1_000_000),
    ]);
    s.idle(1);
    Memory::fill(&memory, &mut s, 0xa5a5a5a5);
    s.model.set("reset", 0);
    s.idle(1);
    memory.borrow_mut().control.stalls = true;
    pulse(&mut s, "start");
    stopped(&mut s, quanta);
    assert_eq!(s.model.get("status") & 16, 0);
    let counters = counters.borrow();
    assert_eq!(counters.starts.len(), counters.ends.len());
    let observed: Vec<_> = counters
        .starts
        .iter()
        .zip(&counters.ends)
        .map(|(a, b)| b - a)
        .collect();
    let selected = &observed[warmup as usize..];
    let result = stats(&s);
    assert_eq!(result[1], observed.len() as u64);
    assert_eq!(result[1], quanta);
    assert_eq!(result[2], selected.len() as u64);
    assert_eq!(
        &result[3..7],
        &[
            selected.iter().sum(),
            *selected.last().unwrap(),
            *selected.iter().min().unwrap(),
            *selected.iter().max().unwrap()
        ]
    );
    for i in [8, 9, 10, 11, 14] {
        assert_ne!(result[i], 0);
    }
    assert!(
        counters
            .ends
            .iter()
            .zip(counters.starts.iter().skip(1))
            .all(|(a, b)| b - a >= 17)
    );
    drop(counters);
    if controls {
        s.model.set_all(&[
            ("quanta_limit", 0),
            ("warmup_quanta", 0),
            ("gap_cycles", 0),
            ("watchdog_cycles", 10000),
        ]);
        memory.borrow_mut().control.pause_responses = true;
        pulse(&mut s, "start");
        s.until(200_000, "accepted AXI before watchdog", |_| {
            memory.borrow().active().is_some()
        });
        s.until(200_000, "watchdog drain request", |m| {
            m.get("status") & 7 == 4
        });
        for _ in 0..20 {
            s.idle(1);
            assert_eq!(
                s.model.get("status") & 7,
                4,
                "reset crossed accepted AXI transaction"
            );
        }
        memory.borrow_mut().control.pause_responses = false;
        stopped(&mut s, quanta);
        assert_ne!(s.model.get("status") & 16, 0);
        s.model
            .set_all(&[("quanta_limit", 0), ("watchdog_cycles", 1_000_000)]);
        pulse(&mut s, "start");
        s.until(200_000, "runner restart work", |m| {
            m.get("debug_quantum") >= 3
        });
        pulse(&mut s, "snapshot");
        assert!(stats(&s)[1] >= 3);
        pulse(&mut s, "stop");
        stopped(&mut s, quanta);
        assert_eq!(s.model.get("status") & 16, 0);
        assert_ne!(s.model.get("status") & 8, 0);
        pulse(&mut s, "start");
        s.until(100, "runner loading", |m| m.get("status") & 7 == 2);
        pulse(&mut s, "stop");
        stopped(&mut s, quanta);
        assert_eq!(s.model.get("status") & 16, 0);
        s.model.set("quanta_limit", 1);
        pulse(&mut s, "start");
        stopped(&mut s, quanta);
        assert_eq!(s.model.get("debug_quantum"), 1);
        assert_eq!(s.model.get("status") & 16, 0);
    }
    let output = json!({"memory":memory.borrow_mut().report(true),"cycles":observed,"statistics":result,"hardware_counter_boundaries_match":true,"output_correctness_checked":false,"watchdog_drain_restart":controls,"snapshot_and_stop":controls,"stop_during_loading":controls});
    let directory = crate::artifact_directory("zedboard-results");
    std::fs::create_dir_all(&directory)?;
    std::fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&output)?,
    )?;
    Ok(output)
}
