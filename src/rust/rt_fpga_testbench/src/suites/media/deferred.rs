//! Retired ownership stays independent of live descriptors and bounded maintenance.
use super::{empty, read_equals};
use crate::{
    bits::Bits,
    drivers::store::Store,
    model::Model,
    sim::{Sim, Snapshot},
};
use serde_json::{Value, json};

fn remaining_cycles(model: &Model<'_>) -> u64 {
    super::remaining_cycles(model, "core.reclaimer")
}

pub fn complete(d: &mut Store, s: &mut Sim<'_>, command: Value) -> Value {
    let r = d.command(s, &command, false, false, true);
    assert_eq!(r["status"], json!({"kind":"completed"}), "{command}: {r}");
    r
}
fn initial(d: &mut Store, s: &mut Sim<'_>) {
    d.load(s, &empty(d));
    complete(d, s, json!({"kind":"create"}));
}
fn finish(d: &mut Store, s: &mut Sim<'_>, mut after: Snapshot, code: u64) {
    for _ in 0..100_000 {
        if after.get("done") != 0 {
            assert_eq!(after.get("error_code"), code);
            d.tick(s);
            return;
        }
        after = d.tick(s).1;
    }
    panic!("deferred command completion timeout");
}
fn wait(d: &mut Store, s: &mut Sim<'_>, name: &str, value: u64) {
    s.until(100_000, name, |m| m.get(name) == value);
    let _ = d;
}
fn append(d: &mut Store, s: &mut Sim<'_>, view: u64, value: i32, count: u64) {
    complete(
        d,
        s,
        json!({"kind":"append","view":view,"samples":vec![value;count as usize]}),
    );
}

pub fn detachment_and_maintenance_budgets(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    let mut times = Vec::new();
    for count in [1, 17] {
        initial(d, s);
        append(d, s, 0, 101, count * bf);
        let walks = (
            s.model.get("core.metadata.directory_reads"),
            s.model.get("core.metadata.table_reads"),
        );
        complete(d, s, json!({"kind":"clear","view":0}));
        times.push(d.timing);
        assert_eq!(
            (
                s.model.get("core.metadata.directory_reads"),
                s.model.get("core.metadata.table_reads")
            ),
            walks
        );
        assert_eq!(d.read_config(s, 6, 0), 1);
        assert_eq!(d.read_config(s, 6, 1), count);
        assert_eq!(d.read_config(s, 0, 2), 0);
        if count > 3 {
            d.grant(s, 3, Some(d.geometry.reclaim_guard + 1));
            s.idle(100);
            assert_eq!(s.model.get("reclaim_blocks"), 0);
            d.grant(s, 3, Some(100_000));
            s.until(100_000, "bounded maintenance", |m| {
                m.get("reclaim_blocks") == 3 && m.get("reclaim_active") == 0
            });
            s.idle(100);
            assert_eq!(s.model.get("reclaim_blocks"), 3);
            assert_eq!(d.read_config(s, 6, 1), count - 3);
            assert_eq!(d.read_config(s, 0, 2), 3);
            d.snapshot(s);
        }
        d.drain(s);
        assert_eq!(d.read_config(s, 6, 0), 0);
        assert_eq!(d.read_config(s, 6, 1), 0);
        assert_eq!(d.read_config(s, 0, 2), count);
    }
    if d.mode != crate::memory::Mode::Dramsim3 {
        assert!(
            times.iter().max().unwrap() - times.iter().min().unwrap() < 32,
            "clear latency: {times:?}"
        );
    }
}
pub fn clear_cancels_an_outstanding_metadata_hint(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    initial(d, s);
    append(d, s, 0, 1901, 2 * bf);
    d.write_config(s, 5, 0, 0);
    d.memory.borrow_mut().control.pause_responses = true;
    d.set(
        s,
        &[
            ("hint_valid", 1),
            ("hint_view", 0),
            ("hint_position", 0),
            ("hint_length", 2 * bf),
            ("hint_count", d.geometry.chunk_frames),
        ],
    );
    let mut admitted = false;
    for _ in 0..10_000 {
        s.model.eval();
        let ready = d.get(s, "hint_ready");
        d.tick(s);
        if ready != 0 {
            admitted = true;
            break;
        }
    }
    assert!(admitted);
    d.set(s, &[("hint_valid", 0)]);
    s.until(10_000, "hint memory request", |_| {
        d.memory.borrow().active().is_some()
    });
    let after = d.admit(s, 2, 0, 0, 0, 0);
    assert_eq!(after.get("done"), 0);
    s.idle(10);
    d.memory.borrow_mut().control.pause_responses = false;
    let after = d.tick(s).1;
    finish(d, s, after, 0);
    append(d, s, 0, 7777, d.geometry.chunk_frames);
    d.drain(s);
    read_equals(
        d,
        s,
        0,
        &vec![7777; d.geometry.chunk_frames as usize],
        0,
        Some(d.geometry.chunk_frames),
    );
}
pub fn queue_full_preserves_the_live_recording(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    initial(d, s);
    let capacity = d.read_config(s, 6, 8);
    assert_eq!(capacity, 2);
    for v in 0..capacity {
        append(d, s, 0, v as i32 + 1, bf);
        complete(d, s, json!({"kind":"clear","view":0}));
    }
    append(d, s, 0, 1901, bf);
    let before = d.snapshot(s);
    let result = d.command(s, &json!({"kind":"clear","view":0}), true, false, true);
    assert_eq!(
        result["status"],
        json!({"kind":"failed","value":{"error":"memory_full"}})
    );
    assert_eq!(result["media"], before);
    assert_eq!(d.read_config(s, 6, 0), capacity);
    d.drain(s);
    read_equals(d, s, 0, &vec![1901; bf as usize], 0, Some(bf));
    complete(d, s, json!({"kind":"remove","view":0}));
    d.drain(s);
    assert_eq!(d.read_config(s, 0, 2), capacity + 1);
}
pub fn retained_ranges_and_repeated_view_reuse(d: &mut Store, s: &mut Sim<'_>) {
    let cf = d.geometry.chunk_frames;
    let bf = cf * d.geometry.block_chunks;
    let data: Vec<_> = (0..2 * bf + 7).map(|i| i as i32 * 37 - 13000).collect();
    initial(d, s);
    complete(d, s, json!({"kind":"append","view":0,"samples":data}));
    complete(
        d,
        s,
        json!({"kind":"slice","source":0,"start":bf-3,"frames":bf+6}),
    );
    complete(d, s, json!({"kind":"clear","view":0}));
    append(d, s, 0, 9001, cf);
    assert_eq!(d.read_config(s, 0, 1), 4);
    d.drain(s);
    assert_eq!(d.read_config(s, 0, 2), 0);
    read_equals(
        d,
        s,
        1,
        &data[(bf - 3) as usize..(2 * bf + 3) as usize],
        0,
        Some(bf + 6),
    );
    complete(d, s, json!({"kind":"remove","view":1}));
    d.drain(s);
    assert_eq!(d.read_config(s, 0, 2), 3);
    for v in 1..=6 {
        complete(d, s, json!({"kind":"clear","view":0}));
        d.drain(s);
        append(d, s, 0, v, cf);
    }
    read_equals(d, s, 0, &vec![6; cf as usize], 0, Some(cf));
    complete(d, s, json!({"kind":"clear","view":0}));
    d.drain(s);
    assert_eq!(d.read_config(s, 0, 2), d.read_config(s, 0, 1));
    initial(d, s);
    for ordinal in 0..2 {
        d.write_config(s, 2, d.table_address(0, ordinal), 0);
    }
    let mut descriptor = Bits::from_u64(3);
    descriptor.set(64, 64, 2 * bf).set(128, 32, 2);
    d.configure(s, 1, 0, Some(&descriptor), false);
    d.write_config(s, 3, 0, 2);
    d.write_config(s, 0, 1, 1);
    d.command(s, &json!({"kind":"clear","view":0}), false, false, false);
    d.drain(s);
    assert_eq!(d.read_config(s, 0, 2), 1);
    assert_eq!(d.read_config(s, 3, 0), 0);
    assert_eq!(d.read_config(s, 6, 11), 0);
}
pub fn reset_discards_active_cleanup_and_old_responses(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    initial(d, s);
    append(d, s, 0, 1901, 2 * bf);
    complete(d, s, json!({"kind":"clear","view":0}));
    d.write_config(s, 5, 0, 0);
    d.memory.borrow_mut().control.pause_responses = true;
    d.grant(s, 20, Some(100_000));
    s.until(10_000, "cleanup memory request", |_| {
        d.memory.borrow().active().is_some()
    });
    assert_ne!(s.model.get("reclaim_active"), 0);
    d.memory.borrow_mut().control.pause_responses = false;
    initial(d, s);
    assert_eq!(
        s.model.get("reclaim_pending") | s.model.get("reclaim_active"),
        0
    );
    append(d, s, 0, 7777, d.geometry.chunk_frames);
    read_equals(
        d,
        s,
        0,
        &vec![7777; d.geometry.chunk_frames as usize],
        0,
        Some(d.geometry.chunk_frames),
    );
}
pub fn invalid_retired_block_faults_before_reuse(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    initial(d, s);
    append(d, s, 0, 1901, bf);
    d.write_config(s, 2, d.table_address(0, 0), d.geometry.blocks() - 1);
    assert_eq!(
        d.command(s, &json!({"kind":"clear","view":0}), false, false, false)["status"]["kind"],
        "completed"
    );
    d.grant(s, 20, Some(100_000));
    wait(d, s, "memory_fault", 1);
    assert_eq!(s.model.get("reclaim_freed"), 0);
    assert_eq!(s.model.get("reclaim_pending"), 1);
    assert_eq!(d.read_config(s, 0, 2), 0);
    initial(d, s);
    append(d, s, 0, 7777, d.geometry.chunk_frames);
}
pub fn status_reads_do_not_deadlock_atomic_cleanup(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    initial(d, s);
    append(d, s, 0, 1901, 2 * bf);
    complete(d, s, json!({"kind":"clear","view":0}));
    d.grant(s, 20, Some(100_000));
    wait(d, s, "core.gc_lock_held", 1);
    assert_eq!(d.read_config(s, 6, 0), 1);
    d.drain(s);
    assert_eq!(d.read_config(s, 0, 2), 2);
    assert_eq!(s.model.get("memory_fault"), 0);
}
pub fn pause_at_ownership_and_checkpoint_boundaries(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    for target in 2..18 {
        initial(d, s);
        append(d, s, 0, 1901, bf);
        let shared = target == 6 || target == 7;
        if shared {
            complete(
                d,
                s,
                json!({"kind":"slice","source":0,"start":0,"frames":bf}),
            );
        }
        complete(d, s, json!({"kind":"clear","view":0}));
        d.grant(s, 20, Some(1_000_000));
        wait(d, s, "core.reclaimer.phase", target);
        s.model.set("maintenance_enable", 0);
        d.snapshot(s);
        let pending = s.model.get("reclaim_references");
        s.idle(20);
        assert_eq!(s.model.get("reclaim_references"), pending);
        s.model.set("maintenance_enable", 1);
        d.drain(s);
        assert_eq!(d.read_config(s, 0, 2), u64::from(!shared));
        if shared {
            read_equals(d, s, 1, &vec![1901; bf as usize], 0, Some(bf));
            complete(d, s, json!({"kind":"remove","view":1}));
            d.drain(s);
        }
        assert_eq!(d.read_config(s, 0, 2), 1);
        assert_eq!(d.read_config(s, 6, 11), 0);
    }
}
pub fn offline_drain_survives_budget_counter_expiration(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    initial(d, s);
    append(d, s, 0, 1901, 3 * bf);
    complete(d, s, json!({"kind":"clear","view":0}));
    s.model.set("maintenance_enable", 0);
    let after = d.admit(s, 10, 0, 0, 0, 0);
    assert_eq!(after.get("done"), 0);
    d.tick(s);
    for path in ["core.reclaimer.cycle_budget", "core.reclaimer.blocks_left"] {
        s.model.inject(path, Some(&Bits::from_u64(0)));
    }
    let after = d.tick(s).1;
    finish(d, s, after, 0);
    for path in ["core.reclaimer.cycle_budget", "core.reclaimer.blocks_left"] {
        s.model.inject(path, None);
    }
    assert_eq!(s.model.get("reclaim_pending"), 0);
    assert_eq!(d.read_config(s, 0, 2), 3);
    assert_eq!(d.read_config(s, 6, 11), 0);
    s.model.set("maintenance_enable", 1);
    append(d, s, 0, 7777, bf);
    complete(d, s, json!({"kind":"clear","view":0}));
    s.idle(1000);
    assert_eq!(s.model.get("reclaim_pending"), 1);
    d.drain(s);
}
pub fn cycle_budget_releases_more_than_twenty_and_yields_safely(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    for short in [false, true] {
        initial(d, s);
        append(d, s, 0, 1901, 25 * bf);
        complete(d, s, json!({"kind":"clear","view":0}));
        let guard = d.geometry.reclaim_guard;
        let budget = if short { guard + 600 } else { 100_000 };
        let start = s.model.get("reclaim_cycles");
        d.grant(s, 0, Some(budget));
        s.until(100_000, "cycle budget yield", |m| {
            (m.get("reclaim_pending") == 0 && m.get("reclaim_active") == 0)
                || (remaining_cycles(m) <= guard && m.get("reclaim_quiescent") != 0)
        });
        assert!(s.model.get("reclaim_cycles") - start <= budget);
        let released = s.model.get("reclaim_blocks");
        if short {
            assert!(released > 0 && released < 25);
            s.idle(100);
            assert_eq!(s.model.get("reclaim_blocks"), released);
            d.snapshot(s);
            d.drain(s);
        } else {
            assert_eq!(released, 25);
        }
        assert_eq!(d.read_config(s, 0, 2), 25);
    }
}
pub fn foreground_wait_preserves_the_cleanup_cycle_budget(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    initial(d, s);
    append(d, s, 0, 1901, 2 * bf);
    complete(d, s, json!({"kind":"clear","view":0}));
    complete(d, s, json!({"kind":"create"}));
    append(d, s, 1, 7777, bf);
    d.set(s, &[("read_ready", 0)]);
    d.admit(s, 6, 1, 0, bf, bf);
    wait(d, s, "read_valid", 1);
    s.model.set("maintenance_tick", 1);
    d.tick(s);
    s.model.set("maintenance_tick", 0);
    d.tick(s);
    s.until(20, "cleanup grant activation", |m| {
        m.get("reclaim_active") != 0
    });
    let remaining = remaining_cycles(&s.model);
    s.idle(d.geometry.reclaim_cycles + 100);
    assert_eq!(remaining_cycles(&s.model), remaining);
    assert_eq!(s.model.get("reclaim_blocks"), 0);
    d.set(s, &[("read_ready", 1)]);
    let mut samples = Vec::new();
    let mut done = false;
    for _ in 0..100_000 {
        let (before, after) = d.tick(s);
        if before.get("read_valid") != 0 {
            samples.push(before.get("read_data") as i32);
        }
        if after.get("done") != 0 {
            done = true;
            break;
        }
    }
    assert!(done);
    assert_eq!(samples, vec![7777; bf as usize]);
    d.set(s, &[("read_ready", 0)]);
    d.tick(s);
    s.until(100_000, "preserved cleanup grant", |m| {
        m.get("reclaim_pending") == 0 && m.get("reclaim_active") == 0
    });
    assert_eq!(d.read_config(s, 0, 2), 2);
    d.snapshot(s);
}

pub const TESTS: &[(&str, fn(&mut Store, &mut Sim<'_>))] = &[
    (
        "detachment_and_maintenance_budgets",
        detachment_and_maintenance_budgets,
    ),
    (
        "clear_cancels_an_outstanding_metadata_hint",
        clear_cancels_an_outstanding_metadata_hint,
    ),
    (
        "queue_full_preserves_the_live_recording",
        queue_full_preserves_the_live_recording,
    ),
    (
        "retained_ranges_and_repeated_view_reuse",
        retained_ranges_and_repeated_view_reuse,
    ),
    (
        "reset_discards_active_cleanup_and_old_responses",
        reset_discards_active_cleanup_and_old_responses,
    ),
    (
        "invalid_retired_block_faults_before_reuse",
        invalid_retired_block_faults_before_reuse,
    ),
    (
        "status_reads_do_not_deadlock_atomic_cleanup",
        status_reads_do_not_deadlock_atomic_cleanup,
    ),
    (
        "pause_at_ownership_and_checkpoint_boundaries",
        pause_at_ownership_and_checkpoint_boundaries,
    ),
    (
        "offline_drain_survives_budget_counter_expiration",
        offline_drain_survives_budget_counter_expiration,
    ),
    (
        "cycle_budget_releases_more_than_twenty_and_yields_safely",
        cycle_budget_releases_more_than_twenty_and_yields_safely,
    ),
    (
        "foreground_wait_preserves_the_cleanup_cycle_budget",
        foreground_wait_preserves_the_cleanup_cycle_budget,
    ),
];
