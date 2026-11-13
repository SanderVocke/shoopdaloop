//! Fatal processing faults silence partial output and require coordinated reload.
use super::production::{self, physical};
use crate::{Result, bits::Bits, drivers::loops::Loops, memory::Mode, sim::Sim};
use serde_json::{Value, json};
fn row(inputs: &Value, commands: Vec<Value>) -> Value {
    json!({"transition":{"collect_completed":true,"collect":[],"cancel":[],"commands":commands},"writes":[],"work":{"kind":"audio","value":{"inputs":inputs}}})
}
fn command(kind: &str) -> Value {
    json!({"target":{"slot":0,"generation":1},"sequence":1,"at":0,"timing":{"kind":"at"},"operation":if kind == "play" {json!({"kind":"play","position":0,"retrigger":false})}else{json!({"kind":"record"})}})
}
fn compile(scenario: &Value) -> Value {
    let scenario = serde_json::from_value(scenario.clone()).unwrap();
    serde_json::to_value(rt_backend_testbench::rtl::compile(&scenario).unwrap()).unwrap()
}
fn failed(value: &Value, stage: &str, error: &str, frame: u64) {
    assert_eq!(value["error"], json!({"stage":stage,"error":error}));
    assert_eq!(value["audio"]["outputs"], json!([vec![0; 32]]));
    assert_eq!(value["transition"]["next_frame"], frame);
}
pub fn fatal_processing_faults_silence_hold_and_reset(
    d: &mut Loops,
    s: &mut Sim<'_>,
    depths: [u64; 10],
) {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../../fixtures/production-faults.json")).unwrap();
    let inputs = &cases[0]["steps"][0]["inputs"];
    let base = compile(&cases[0]);
    let loaded = compile(&cases[1]);
    let g = d.pipeline.store.geometry.clone();
    let mut image = base.clone();
    image["program"]["instructions"]
        .as_array_mut()
        .unwrap()
        .insert(1, json!({"kind":"process_output","source":0,"port":0}));
    let budget = image["program"]["jobs_per_quantum"].as_u64().unwrap();
    image["program"]["jobs_per_quantum"] = json!(budget + 1);
    let mut case = physical(&image, depths, &g);
    let capacity = g.block_chunks.max(32 / g.chunk_frames);
    let filled = capacity * g.chunk_frames / 32;
    case["media"]["chunk_limit"] = json!(capacity);
    d.initialize(s, &case, &image["transition"]);
    assert!(d.step(s, &row(inputs, vec![command("record")]), true)["error"].is_null());
    for _ in 1..filled {
        assert!(d.step(s, &row(inputs, vec![]), true)["error"].is_null());
    }
    let fault = d.step(s, &row(inputs, vec![]), true);
    failed(&fault, "jobs", "memory_full", filled * 32);
    assert_ne!(s.model.get("faulted"), 0);
    assert_eq!(s.model.get("run_ready"), 0);
    failed(
        &d.step(s, &row(inputs, vec![]), true),
        "jobs",
        "memory_full",
        filled * 32,
    );
    let case = physical(&image, depths, &g);
    d.initialize(s, &case, &image["transition"]);
    assert!(d.step(s, &row(inputs, vec![command("record")]), true)["error"].is_null());
    let case = physical(&loaded, depths, &g);
    d.initialize(s, &case, &loaded["transition"]);
    d.pipeline.store.memory.borrow_mut().control.fail_next = true;
    failed(
        &d.step(s, &row(inputs, vec![command("play")]), true),
        "jobs",
        "memory_fault",
        0,
    );
    failed(
        &d.step(s, &row(inputs, vec![]), true),
        "jobs",
        "memory_fault",
        0,
    );
    d.initialize(s, &case, &loaded["transition"]);
    assert!(d.step(s, &row(inputs, vec![command("play")]), true)["error"].is_null());
    let mut stale = base.clone();
    let instruction = stale["program"]["instructions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|i| i["kind"] == "process_logical_playback")
        .unwrap();
    instruction["loop_id"]["generation"] = json!(2);
    let case = physical(&stale, depths, &g);
    d.initialize(s, &case, &stale["transition"]);
    failed(
        &d.step(s, &row(inputs, vec![]), true),
        "state",
        "invalid_state",
        0,
    );
    let case = physical(&base, depths, &g);
    d.initialize(s, &case, &base["transition"]);
    assert!(d.step(s, &row(inputs, vec![]), true)["error"].is_null());
    let mut exhausted = base.clone();
    let address = exhausted["program"]["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["kind"] == "process_logical_capture")
        .unwrap()["view"]
        .as_u64()
        .unwrap() as usize;
    exhausted["initial_state"]["memory"][address]["value"] = json!(g.views);
    exhausted["media"]["views"] = json!(vec![json!({"chunks":[],"frames":0}); g.views as usize]);
    let case = physical(&exhausted, depths, &g);
    d.initialize(s, &case, &exhausted["transition"]);
    failed(
        &d.step(s, &row(inputs, vec![command("record")]), true),
        "jobs",
        "memory_full",
        0,
    );
    exhausted["initial_state"]["memory"][address]["value"] = json!(0);
    let case = physical(&exhausted, depths, &g);
    d.initialize(s, &case, &exhausted["transition"]);
    assert!(d.step(s, &row(inputs, vec![command("record")]), true)["error"].is_null());
    exhausted["media"]["views"] = json!([]);
    let case = physical(&exhausted, depths, &g);
    d.initialize(s, &case, &exhausted["transition"]);
    assert!(d.step(s, &row(inputs, vec![command("record")]), true)["error"].is_null());
    let case = physical(&loaded, depths, &g);
    d.initialize(s, &case, &loaded["transition"]);
    d.pipeline.store.fence(s);
    d.pipeline.store.write_config(s, 5, 0, 1);
    d.pipeline.store.memory.borrow_mut().control.fail_next = true;
    failed(
        &d.step(s, &row(inputs, vec![command("play")]), true),
        "jobs",
        "memory_fault",
        0,
    );
    d.initialize(s, &case, &loaded["transition"]);
    d.pipeline.store.memory.borrow_mut().control.pause_responses = true;
    d.submit(s, &command("play"));
    for (port, data) in inputs.as_array().unwrap().iter().enumerate() {
        for (frame, value) in data.as_array().unwrap().iter().enumerate() {
            d.pipeline.audio.sample_config(
                s,
                0,
                port as u64 * depths[6] + frame as u64,
                Some(value.as_i64().unwrap() as i32),
                false,
            );
        }
    }
    s.model.set_all(&[
        ("run_buffers", d.pipeline.audio.buffers),
        ("run_inputs", inputs.as_array().unwrap().len() as u64),
        ("run_outputs", d.pipeline.audio.outputs),
    ]);
    d.pipeline.audio.program.run_inputs(s, &[]);
    let mut admitted = false;
    for _ in 0..10_000 {
        if s.tick(&["run_ready"]).0.get("run_ready") != 0 {
            admitted = true;
            break;
        }
    }
    assert!(admitted);
    s.model.set("run_valid", 0);
    s.until(10_000, "outstanding memory before reset", |_| {
        d.pipeline.store.memory.borrow().active().is_some()
    });
    d.transition.reset(s);
    assert!(d.pipeline.store.memory.borrow().active().is_none());
    d.pipeline.store.memory.borrow_mut().control.pause_responses = false;
    d.initialize(s, &case, &loaded["transition"]);
    assert!(d.step(s, &row(inputs, vec![command("play")]), true)["error"].is_null());
    let mut case = physical(&base, depths, &g);
    let slots = g.block_chunks.max(32 / g.chunk_frames);
    case["media"] = json!({"block_chunks":g.block_chunks,"chunk_frames":g.chunk_frames,"samples":vec![0;(slots*g.chunk_frames) as usize],"free_chunks":(0..slots).collect::<Vec<_>>(),"views":[{"chunks":[],"frames":0}]});
    d.initialize(s, &case, &base["transition"]);
    {
        let mut memory = d.pipeline.store.memory.borrow_mut();
        memory.control.pause_responses = true;
        memory.control.fail_next = true;
    }
    let first = d.step(s, &row(inputs, vec![command("record")]), false);
    assert!(first["error"].is_null());
    assert_eq!(first["transition"]["next_frame"], 32);
    d.pipeline.store.memory.borrow_mut().control.pause_responses = false;
    s.until(10_000, "posted writeback fault", |m| m.get("faulted") != 0);
    failed(
        &d.step(s, &row(inputs, vec![]), false),
        "jobs",
        "memory_fault",
        32,
    );
}
fn admit(d: &mut Loops, s: &mut Sim<'_>, host: bool, gc: &str) -> u64 {
    if host {
        s.model.set_all(&[
            ("command_valid", 1),
            ("command_opcode", 16),
            ("command_index", 0),
            ("command_data", 0),
            ("response_ready", 1),
        ]);
    } else {
        d.pipeline.audio.program.run_inputs(s, &[]);
        s.model
            .set_all(&[("run_buffers", 0), ("run_inputs", 0), ("run_outputs", 0)]);
    }
    let get = |m: &crate::model::Model<'_>, n| m.get(&format!("{gc}.{n}"));
    let ready = if host { "command_ready" } else { "run_ready" };
    for wait in 1..=10_000 {
        s.model.eval();
        if get(&s.model, "lock_held") == 0 {
            for (valid, ready) in [
                ("lock_request", "lock_grant"),
                ("request_valid", "request_ready"),
                ("retire_valid", "retire_ready"),
            ] {
                assert_eq!(
                    get(&s.model, valid) & get(&s.model, ready),
                    0,
                    "cleanup admitted ahead of waiting iteration"
                );
            }
        }
        let remaining = super::media::remaining_cycles(&s.model, gc);
        let before = s.tick(&[ready]).0;
        if before.get(ready) != 0 {
            s.model
                .set(if host { "command_valid" } else { "run_valid" }, 0);
            assert!(remaining > d.pipeline.store.geometry.reclaim_guard);
            return wait;
        }
    }
    panic!("cleanup starved iteration admission");
}
pub fn early_iterations_preempt_cleanup_without_waiting_for_its_allowance(
    d: &mut Loops,
    s: &mut Sim<'_>,
    depths: [u64; 10],
) {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../../fixtures/production-faults.json")).unwrap();
    let image = compile(&cases[0]);
    let g = d.pipeline.store.geometry.clone();
    let gc = if d.pipeline.store.mode == Mode::Legacy {
        "audio.media.reclaimer"
    } else {
        "core.audio.media.reclaimer"
    };
    let core = if d.pipeline.store.mode == Mode::Legacy {
        "phase"
    } else {
        "core.phase"
    };
    for settle in [false, true] {
        let mut case = physical(&image, depths, &g);
        case["program"]["instructions"] = json!([]);
        case["program"]["jobs_per_quantum"] = json!(0);
        case["initial_state"]["memory"] = json!([]);
        case["buffers"] = json!(0);
        case["output_ports"] = json!(0);
        case["media"] = super::media::empty(&d.pipeline.store);
        d.initialize(s, &case, &image["transition"]);
        s.model.set("maintenance_enable", 0);
        let blocks = g.blocks().min(25);
        let bf = g.chunk_frames * g.block_chunks;
        for block in 0..blocks {
            d.pipeline
                .store
                .write_config(s, 2, d.pipeline.store.table_address(0, block), block);
            d.pipeline.store.write_config(s, 3, block, 1);
        }
        let mut descriptor = Bits::from_u64(1);
        descriptor.set(64, 64, blocks * bf).set(128, 32, blocks);
        d.pipeline
            .store
            .configure(s, 1, 0, Some(&descriptor), false);
        d.pipeline.store.write_config(s, 0, 0, 1);
        d.pipeline.store.write_config(s, 0, 1, blocks);
        assert_eq!(
            d.pipeline
                .store
                .command(s, &json!({"kind":"clear","view":0}), false, false, false)["status"]["kind"],
            "completed"
        );
        d.pipeline.store.grant(s, 0, Some(100_000));
        s.model.set("maintenance_enable", 1);
        s.until(10_000, "cleanup handoff boundary", |m| {
            m.get(&format!("{gc}.phase")) == if settle { 4 } else { 1 }
        });
        admit(d, s, settle, gc);
        if settle {
            s.until(10_000, "host settlement", |m| m.get(core) == 9);
            s.model.set("response_ready", 0);
            let remaining = super::media::remaining_cycles(&s.model, gc);
            admit(d, s, false, gc);
            assert!(super::media::remaining_cycles(&s.model, gc) <= remaining);
        }
        for run in 0..2 {
            let start = s.cycles;
            s.until(10_000, "empty processing", |m| m.get("done") != 0);
            assert_eq!(s.model.get("error_code"), 0);
            assert!(s.cycles - start < g.reclaim_cycles);
            if run == 0 {
                admit(d, s, false, gc);
            }
        }
    }
}
pub fn run(mode: Mode, bc: u64) -> Result<()> {
    let depths = production::DEFAULT;
    let g = crate::drivers::store::Geometry::new([32, 128 * bc, 32], bc);
    let compiled = production::design(depths, &g, 64, mode)
        .build(&crate::root().join("work/marlin-suite/production"))?;
    for test in [
        early_iterations_preempt_cleanup_without_waiting_for_its_allowance,
        fatal_processing_faults_silence_hold_and_reset,
    ] {
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        let mut d = Loops::new(&mut s, depths, g.clone(), mode, 20261113)?;
        test(&mut d, &mut s, depths);
        d.pipeline.store.memory.borrow_mut().report(true);
    }
    Ok(())
}
