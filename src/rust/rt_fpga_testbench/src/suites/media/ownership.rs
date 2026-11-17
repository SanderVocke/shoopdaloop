//! Allocator exhaustion, transactional admission, block sharing, and statistics.
use super::{completed, deferred::complete, empty, read_equals};
use crate::{bits::Bits, drivers::store::Store, sim::Sim};
use serde_json::json;
pub fn block_ownership(d: &mut Store, s: &mut Sim<'_>) {
    let cf = d.geometry.chunk_frames;
    let bc = d.geometry.block_chunks;
    let bf = bc * cf;
    d.load(s, &empty(d));
    completed(d, s, json!({"kind":"create"}));
    let values: Vec<_> = (0..2 * bf + 3).map(|i| i as i32 * 701 - 91000).collect();
    completed(d, s, json!({"kind":"append","view":0,"samples":values}));
    assert_eq!(
        completed(
            d,
            s,
            json!({"kind":"slice","source":0,"start":bf-3,"frames":bf+6})
        )["value"],
        json!({"kind":"view","value":1})
    );
    assert_eq!(
        completed(
            d,
            s,
            json!({"kind":"slice","source":1,"start":2,"frames":bf+3})
        )["value"],
        json!({"kind":"view","value":2})
    );
    completed(d, s, json!({"kind":"clear","view":0}));
    completed(
        d,
        s,
        json!({"kind":"append","view":0,"samples":vec![1901;cf as usize]}),
    );
    read_equals(
        d,
        s,
        1,
        &values[(bf - 3) as usize..(2 * bf + 3) as usize],
        bf + 5,
        Some(3 * cf + 7),
    );
    let saved = d.snapshot(s);
    d.load(s, &saved);
    assert_eq!(d.snapshot(s), saved);
    read_equals(d, s, 0, &vec![1901; cf as usize], 0, Some(cf));
    completed(d, s, json!({"kind":"remove","view":1}));
    for (data, finished) in [
        (&values[(bf - 1) as usize..(2 * bf) as usize], false),
        (&values[(bf - 1) as usize..bf as usize], false),
        (&[][..], true),
    ] {
        assert_eq!(
            completed(d, s, json!({"kind":"reclaim","view":2}))["value"],
            json!({"kind":"empty","value":finished})
        );
        if !data.is_empty() {
            read_equals(d, s, 2, data, 0, Some(2 * cf));
        }
    }
    completed(d, s, json!({"kind":"remove","view":2}));
    completed(d, s, json!({"kind":"clear","view":0}));
    let snapshot = d.snapshot(s);
    assert_eq!(snapshot["samples"].as_array().unwrap().len() as u64, 4 * bf);
    let mut free: Vec<u64> = serde_json::from_value(snapshot["free_chunks"].clone()).unwrap();
    free.sort_unstable();
    assert_eq!(free, (0..4 * bc).collect::<Vec<_>>());
    for _ in 0..8 {
        let count = 1 + d.random.below(bf + 2);
        let data: Vec<_> = (0..count)
            .map(|_| d.random.below(2_000_000) as i32 - 1_000_000)
            .collect();
        completed(d, s, json!({"kind":"append","view":0,"samples":data}));
        read_equals(d, s, 0, &data, 0, Some(count));
        completed(d, s, json!({"kind":"clear","view":0}));
    }
    let data: Vec<_> = (0..d.geometry.slots * cf)
        .map(|i| i as i32 - 12000)
        .collect();
    completed(d, s, json!({"kind":"append","view":0,"samples":data}));
    read_equals(d, s, 0, &data, 0, Some(data.len() as u64));
    let before = d.snapshot(s);
    let result = d.command(
        s,
        &json!({"kind":"append","view":0,"samples":[1]}),
        true,
        true,
        true,
    );
    assert_eq!(
        result["status"],
        json!({"kind":"failed","value":{"error":"memory_full"}})
    );
    assert_eq!(result["media"], before);
    completed(d, s, json!({"kind":"clear","view":0}));
}
pub fn full_pools_and_pause_after_reference_release(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    assert_eq!(d.geometry.table_pages, 3);
    let mut image = empty(d);
    image["chunk_limit"] = json!(17 * d.geometry.block_chunks);
    d.load(s, &image);
    completed(d, s, json!({"kind":"create"}));
    completed(
        d,
        s,
        json!({"kind":"append","view":0,"samples":vec![123;(17*bf) as usize]}),
    );
    assert_eq!(s.model.get("core.metadata.high_water"), 3);
    assert_eq!(s.model.get("core.metadata.free_pages"), 0);
    complete(d, s, json!({"kind":"clear","view":0}));
    assert_eq!(d.read_config(s, 0, 2), 0);
    assert_eq!(d.read_config(s, 6, 1), 17);
    let refused = d.command(
        s,
        &json!({"kind":"append","view":0,"samples":vec![456;d.geometry.chunk_frames as usize]}),
        true,
        false,
        true,
    );
    assert_eq!(
        refused["status"],
        json!({"kind":"failed","value":{"error":"memory_full"}})
    );
    d.grant(s, 17, Some(1_000_000));
    s.until(100_000, "full pool reference release", |m| {
        m.get("reclaim_released") != 0
    });
    s.model.set("maintenance_enable", 0);
    d.snapshot(s);
    assert_eq!(d.read_config(s, 6, 1), 17);
    assert_eq!(d.read_config(s, 0, 2), 1);
    s.model.set("maintenance_enable", 1);
    d.drain(s);
    assert_eq!(d.read_config(s, 0, 2), 17);
    assert_eq!(s.model.get("core.metadata.free_pages"), 3);
    completed(
        d,
        s,
        json!({"kind":"append","view":0,"samples":vec![456;(17*bf) as usize]}),
    );
    assert_eq!(s.model.get("core.metadata.high_water"), 3);
}
pub fn failed_detachment_preserves_ownership_until_reset(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    let image = empty(d);
    d.load(s, &image);
    completed(d, s, json!({"kind":"create"}));
    completed(
        d,
        s,
        json!({"kind":"append","view":0,"samples":vec![1901;bf as usize]}),
    );
    let descriptor = d.configure(s, 1, 0, None, false);
    d.memory.borrow_mut().control.fail_next = true;
    let result = d.command(s, &json!({"kind":"clear","view":0}), true, false, false);
    assert_eq!(result["status"]["kind"], "failed");
    assert_ne!(s.model.get("memory_fault"), 0);
    assert_eq!(d.configure(s, 1, 0, None, false), descriptor);
    assert_eq!(s.model.get("reclaim_pending"), 0);
    d.load(s, &image);
    completed(d, s, json!({"kind":"create"}));
    completed(
        d,
        s,
        json!({"kind":"append","view":0,"samples":vec![7777;bf as usize]}),
    );
    complete(d, s, json!({"kind":"clear","view":0}));
    d.drain(s);
    assert_eq!(d.read_config(s, 0, 2), 1);
}
pub fn published_retirement_fault_requires_a_coordinated_reset(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    let image = empty(d);
    d.load(s, &image);
    completed(d, s, json!({"kind":"create"}));
    completed(
        d,
        s,
        json!({"kind":"append","view":0,"samples":vec![1901;bf as usize]}),
    );
    d.command(s, &json!({"kind":"clear","view":0}), true, false, false);
    assert_eq!(d.configure(s, 1, 0, None, false).get(128, 32), 0);
    assert_eq!(s.model.get("reclaim_pending"), 1);
    assert_eq!(s.model.get("reclaim_references"), 1);
    d.memory.borrow_mut().control.fail_next = true;
    let after = d.admit(s, 7, 0, 0, 0, 0);
    if after.get("done") == 0 {
        s.until(100_000, "faulted fence", |m| m.get("done") != 0);
    }
    assert_eq!(s.model.get("error_code"), 5);
    assert_ne!(s.model.get("memory_fault"), 0);
    assert_eq!(s.model.get("reclaim_pending"), 1);
    assert_eq!(s.model.get("reclaim_freed"), 0);
    d.tick(s);
    assert_eq!(s.model.get("cmd_ready"), 0);
    d.load(s, &image);
    assert_eq!(s.model.get("reclaim_pending"), 0);
    completed(d, s, json!({"kind":"create"}));
    completed(
        d,
        s,
        json!({"kind":"append","view":0,"samples":vec![7777;bf as usize]}),
    );
}
pub fn removed_view_reuse_keeps_the_retired_root_independent(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    d.load(s, &empty(d));
    complete(d, s, json!({"kind":"create"}));
    complete(
        d,
        s,
        json!({"kind":"append","view":0,"samples":vec![1111;bf as usize]}),
    );
    complete(d, s, json!({"kind":"remove","view":0}));
    assert_eq!(d.read_config(s, 6, 0), 1);
    assert_eq!(
        complete(d, s, json!({"kind":"create"}))["value"],
        json!({"kind":"view","value":0})
    );
    assert_eq!(d.read_config(s, 0, 0), 1);
    complete(
        d,
        s,
        json!({"kind":"append","view":0,"samples":vec![7777;bf as usize]}),
    );
    d.drain(s);
    read_equals(d, s, 0, &vec![7777; bf as usize], 0, Some(bf));
    assert_eq!(d.read_config(s, 0, 2), 1);
    complete(d, s, json!({"kind":"remove","view":0}));
    d.drain(s);
    assert_eq!(d.read_config(s, 0, 2), 2);
    assert_eq!(d.read_config(s, 6, 11), 0);
    d.snapshot(s);
}
pub fn full_view_space_slice_checks_all_pages_before_copying(d: &mut Store, s: &mut Sim<'_>) {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    d.load(s, &empty(d));
    assert_eq!(d.geometry.table_pages, 7);
    complete(d, s, json!({"kind":"create"}));
    complete(
        d,
        s,
        json!({"kind":"append","view":0,"samples":vec![1111;bf as usize]}),
    );
    complete(d, s, json!({"kind":"create"}));
    complete(
        d,
        s,
        json!({"kind":"append","view":1,"samples":vec![7777;(17*bf) as usize]}),
    );
    for _ in 2..d.geometry.views {
        complete(d, s, json!({"kind":"create"}));
    }
    complete(d, s, json!({"kind":"remove","view":0}));
    let before = d.snapshot(s);
    assert_eq!(d.read_config(s, 6, 11), 5);
    assert_eq!(d.read_config(s, 6, 12), 2);
    let result = d.command(
        s,
        &json!({"kind":"slice","source":1,"start":0,"frames":17*bf}),
        true,
        false,
        true,
    );
    assert_eq!(
        result["status"],
        json!({"kind":"failed","value":{"error":"memory_full"}})
    );
    assert_eq!(result["media"], before);
    assert_eq!(d.read_config(s, 6, 11), 5);
    d.drain(s);
    assert_eq!(
        complete(
            d,
            s,
            json!({"kind":"slice","source":1,"start":0,"frames":17*bf})
        )["value"],
        json!({"kind":"view","value":0})
    );
    assert_eq!(d.read_config(s, 0, 0), d.geometry.views);
    read_equals(d, s, 0, &vec![7777; (17 * bf) as usize], 0, Some(17 * bf));
    complete(d, s, json!({"kind":"remove","view":0}));
    complete(d, s, json!({"kind":"remove","view":1}));
    d.drain(s);
    assert_eq!(d.read_config(s, 0, 2), 18);
    assert_eq!(d.read_config(s, 6, 11), 0);
    d.snapshot(s);
}
fn stats(d: &mut Store, s: &mut Sim<'_>) -> [u32; 5] {
    d.configure(s, 6, 16, None, false).array()
}
pub fn storage_statistics(d: &mut Store, s: &mut Sim<'_>) {
    let g = d.geometry.clone();
    let n = g.blocks() as u32;
    assert_eq!(g.table_pages, 2);
    assert!(n <= 16);
    d.reset(s);
    assert_eq!(stats(d, s), [0, n, 0, 2, 0]);
    let geometry = d.configure(s, 6, 17, None, false);
    assert_eq!(geometry.get(64, 32), g.chunk_frames);
    assert_eq!(geometry.get(96, 32), g.block_chunks);
    assert_eq!(geometry.get(128, 32), g.views);
    assert_eq!(geometry.get(0, 64), g.words() as u64 * 4);
    d.write_config(s, 0, 3, 2);
    completed(d, s, json!({"kind":"create"}));
    assert_eq!(stats(d, s), [0, 2, 0, 2, 1]);
    completed(d, s, json!({"kind":"append","view":0,"samples":[117]}));
    assert_eq!(stats(d, s), [1, 2, 1, 2, 1]);
    completed(
        d,
        s,
        json!({"kind":"slice","source":0,"start":0,"frames":1}),
    );
    assert_eq!(stats(d, s), [1, 2, 2, 2, 2]);
    let before = d.snapshot(s);
    assert_eq!(
        d.command(
            s,
            &json!({"kind":"slice","source":0,"start":0,"frames":1}),
            true,
            true,
            true
        )["status"],
        json!({"kind":"failed","value":{"error":"memory_full"}})
    );
    assert_eq!(stats(d, s), [1, 2, 2, 2, 2]);
    for a in [16, 17] {
        d.configure(s, 6, a, Some(&Bits::from_u64(0)), true);
    }
    for a in [15, 18] {
        d.configure(s, 6, a, None, true);
    }
    assert_eq!(d.snapshot(s), before);
    completed(d, s, json!({"kind":"clear","view":0}));
    assert_eq!(stats(d, s), [1, 2, 1, 2, 2]);
    completed(d, s, json!({"kind":"remove","view":1}));
    assert_eq!(stats(d, s), [0, 2, 0, 2, if g.reuse_views { 1 } else { 2 }]);
    completed(d, s, json!({"kind":"remove","view":0}));
    assert_eq!(stats(d, s), [0, 2, 0, 2, if g.reuse_views { 0 } else { 2 }]);
    completed(d, s, json!({"kind":"create"}));
    assert_eq!(stats(d, s), [0, 2, 0, 2, if g.reuse_views { 1 } else { 3 }]);
    d.reset(s);
    assert_eq!(stats(d, s), [0, n, 0, 2, 0]);
    for v in 0..4 {
        d.write_config(s, 1, v, u64::from(v == 3));
    }
    d.write_config(s, 0, 0, 4);
    assert_eq!(stats(d, s), [0, n, 0, 2, if g.reuse_views { 1 } else { 4 }]);
    d.write_config(s, 1, 3, 0);
    assert_eq!(stats(d, s), [0, n, 0, 2, if g.reuse_views { 0 } else { 4 }]);
}
pub fn statistics_count_retired_storage_without_draining(d: &mut Store, s: &mut Sim<'_>) {
    d.reset(s);
    let n = d.geometry.blocks() as u32;
    completed(d, s, json!({"kind":"create"}));
    completed(d, s, json!({"kind":"append","view":0,"samples":[117]}));
    assert_eq!(stats(d, s), [1, n, 1, 2, 1]);
    d.command(s, &json!({"kind":"clear","view":0}), true, false, false);
    assert_eq!(d.read_config(s, 6, 0), 1);
    assert_eq!(d.read_config(s, 6, 1), 1);
    assert_eq!(stats(d, s), [1, n, 1, 2, 1]);
    let geometry = d.configure(s, 6, 17, None, false);
    assert_eq!(geometry.get(64, 32), d.geometry.chunk_frames);
    assert_eq!(geometry.get(96, 32), d.geometry.block_chunks);
    assert_eq!(geometry.get(128, 32), d.geometry.views);
    assert_eq!(d.read_config(s, 6, 0), 1);
    assert_eq!(d.read_config(s, 6, 1), 1);
    d.drain(s);
    assert_eq!(d.read_config(s, 6, 0), 0);
    assert_eq!(d.read_config(s, 6, 1), 0);
    assert_eq!(stats(d, s), [0, n, 0, 2, 1]);
}
