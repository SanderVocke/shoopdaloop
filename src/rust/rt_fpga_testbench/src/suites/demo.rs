//! Autonomous prepared loader and coordinated recovery across outstanding AXI responses.
use super::continuous::{self, Config};
use crate::{
    Result,
    drivers::frames::Frames,
    memory::{Memory, Mode},
    sim::Sim,
};
use serde_json::{Value, json};
fn recover(s: &mut Sim<'_>, clock: &Frames, epoch: u64) -> u64 {
    for _ in 0..100 {
        clock.pulse(s, None);
        if s.model.get("epoch") >= epoch && s.model.get("boot_done") != 0 {
            clock.settle(s, 5);
            return s.model.get("session_frame");
        }
    }
    panic!(
        "demo recovery timed out: epoch {}, boot fault {}, memory quiescent {}, recovery {}",
        s.model.get("epoch"),
        s.model.get("boot_fault"),
        s.model.get("memory_quiescent"),
        s.model.get("recovery_reason")
    );
}
pub fn prepared_demo_and_recovery(c: &Config, mode: Mode) -> Result<Value> {
    let prepared = continuous::prepare(c)?;
    let compiled = continuous::design(c, mode, true, &prepared)
        .build(&crate::root().join("work/marlin-suite/demo"))?;
    let mut s = Sim::new(
        compiled.session(None)?,
        &[("clk", 10_000, 0), ("audio_clk", c.period_ps, c.phase_ps)],
    );
    let memory = Memory::attach(
        &mut s,
        mode,
        continuous::geometry(c).words(),
        0x01000000,
        &crate::artifact_directory("demo/dram"),
        20261112,
    )?;
    memory.borrow_mut().control.stalls = c.stalls;
    let clock = Frames {
        ticks: c.ticks,
        period_ps: c.period_ps,
    };
    s.model.set_all(&[("reset", 1), ("job_enable", 1)]);
    Memory::fill(&memory, &mut s, 0);
    clock.ns(&mut s, 200);
    s.model.set("reset", 0);
    let mut loaded = false;
    for _ in 0..100_000 {
        if s.model.get("boot_done") != 0 {
            loaded = true;
            break;
        }
        assert_eq!(s.model.get("boot_fault"), 0);
        s.idle(1);
    }
    assert!(loaded, "prepared image loading timeout");
    clock.settle(&mut s, 5);
    let count = 13 * c.q + c.p;
    continuous::frames(
        &mut s,
        &clock,
        c,
        &prepared.expected,
        0,
        count,
        false,
        &[],
        None,
    );
    assert_eq!(s.model.get("recoveries"), 0);
    let before = s.model.get("audio_frame");
    s.model.set("job_enable", 0);
    for _ in 0..10 * c.q {
        clock.pulse(&mut s, None);
        if s.model.get("recoveries") != 0 {
            break;
        }
    }
    assert_eq!(s.model.get("recoveries"), 1);
    assert_eq!(s.model.get("last_recovery_reason"), 1);
    s.model.set("job_enable", 1);
    let start = recover(&mut s, &clock, 2);
    continuous::frames(
        &mut s,
        &clock,
        c,
        &prepared.expected,
        start,
        count - start,
        false,
        &[],
        None,
    );
    assert!(s.model.get("audio_frame") > before);
    assert_eq!(s.model.get("recoveries"), 1);
    s.model.set("restart", 1);
    s.idle(1);
    s.model.set("restart", 0);
    recover(&mut s, &clock, 3);
    memory.borrow_mut().control.fail_next = true;
    for _ in 0..10 * c.q {
        clock.pulse(&mut s, None);
        if s.model.get("recoveries") >= 3 {
            break;
        }
    }
    assert_eq!(
        s.model.get("recoveries"),
        3,
        "epoch {}, boot {}, reason {}, memory {:?}, fail {}",
        s.model.get("epoch"),
        s.model.get("boot_done"),
        s.model.get("last_recovery_reason"),
        memory.borrow().active(),
        memory.borrow().control.fail_next
    );
    assert_eq!(s.model.get("last_recovery_reason"), 3);
    assert_eq!(s.model.get("last_error_stage"), 3);
    let start = recover(&mut s, &clock, 4);
    continuous::frames(
        &mut s,
        &clock,
        c,
        &prepared.expected,
        start,
        count - start,
        false,
        &[],
        None,
    );
    assert_eq!(s.model.get("recoveries"), 3);
    memory.borrow_mut().control.pause_responses = true;
    for _ in 0..4 * c.q {
        clock.pulse(&mut s, None);
        if memory.borrow().active().is_some() {
            break;
        }
    }
    assert!(memory.borrow().active().is_some());
    assert_eq!(s.model.get("memory_quiescent"), 0);
    s.model.set("restart", 1);
    s.idle(1);
    s.model.set("restart", 0);
    for _ in 0..2 {
        clock.pulse(&mut s, None);
        assert_eq!(
            s.model.get("epoch"),
            4,
            "reset crossed an unretired response"
        );
        assert!(s.model.bits("tx_samples").0.iter().all(|&v| v == 0));
    }
    memory.borrow_mut().control.pause_responses = false;
    let start = recover(&mut s, &clock, 5);
    assert!(
        start <= count,
        "recovered session frame {start} exceeds count {count}, epoch {}, recoveries {}",
        s.model.get("epoch"),
        s.model.get("recoveries")
    );
    continuous::frames(
        &mut s,
        &clock,
        c,
        &prepared.expected,
        start,
        count - start,
        false,
        &[],
        None,
    );
    assert_eq!(s.model.get("recoveries"), 4);
    let report = json!({"q":c.q,"p":c.p,"channels":c.channels,"generated_input":true,"prepared_loader":true,"recoveries":s.model.get("recoveries"),"epoch":s.model.get("epoch"),"input_exhaustion_recovered":true,"memory_fault_recovered":true,"pending_response_drained":true,"memory":memory.borrow_mut().report(true)});
    let output = crate::artifact_directory("demo-results");
    std::fs::create_dir_all(&output)?;
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}
