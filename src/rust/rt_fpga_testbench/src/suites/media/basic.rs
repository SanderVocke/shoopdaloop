use super::{completed, empty, read_equals};
use crate::{
    bits::Bits,
    drivers::store::Store,
    memory::Memory,
    sim::{Sim, Snapshot},
};
use serde_json::json;

pub fn directed_media(d: &mut Store, s: &mut Sim<'_>) {
    let cf = d.geometry.chunk_frames;
    let initial = empty(d);
    d.load(s, &initial);
    assert_eq!(
        completed(d, s, json!({"kind":"create"}))["value"],
        json!({"kind":"view","value":0})
    );
    let values: Vec<_> = (1..=3 * cf).map(|i| i as i32).collect();
    completed(d, s, json!({"kind":"append","view":0,"samples":values}));
    let retained = values[cf as usize - 1..(2 * cf) as usize].to_vec();
    assert_eq!(
        completed(
            d,
            s,
            json!({"kind":"slice","source":0,"start":cf-1,"frames":cf+1})
        )["value"],
        json!({"kind":"view","value":1})
    );
    let saved = d.snapshot(s);
    completed(
        d,
        s,
        json!({"kind":"slice","source":1,"start":1,"frames":cf-1}),
    );
    read_equals(d, s, 2, &retained[1..cf as usize], 0, None);
    completed(d, s, json!({"kind":"remove","view":2}));
    completed(d, s, json!({"kind":"clear","view":0}));
    let replacement: Vec<_> = (0..cf / 4)
        .flat_map(|_| [i32::MIN, i32::MAX, -3, 3])
        .collect();
    completed(
        d,
        s,
        json!({"kind":"append","view":0,"samples":replacement}),
    );
    read_equals(d, s, 1, &retained, cf, None);
    for bad in [
        json!({"kind":"read","view":u32::MAX,"position":0,"length":1,"frames":1}),
        json!({"kind":"slice","source":0,"start":u64::MAX,"frames":u64::MAX}),
        json!({"kind":"append","view":1,"samples":[]}),
    ] {
        let before = d.snapshot(s);
        let result = d.command(s, &bad, true, true, true);
        assert_eq!(result["status"]["kind"], "failed");
        assert_eq!(
            result["media"], before,
            "refused operation changed ownership"
        );
    }
    d.load(s, &saved);
    read_equals(d, s, 0, &values, 0, None);
    read_equals(d, s, 1, &retained, 0, None);
    while completed(d, s, json!({"kind":"reclaim","view":1}))["value"]["value"] != true {}
    completed(d, s, json!({"kind":"remove","view":0}));
    completed(d, s, json!({"kind":"remove","view":1}));
    let snapshot = d.snapshot(s);
    let mut free = snapshot["free_chunks"].as_array().unwrap().clone();
    free.sort_by_key(|v| v.as_u64().unwrap());
    assert_eq!(json!(free), json!([0, 1, 2]));
    assert_eq!(snapshot["views"], json!([null, null]));
    let mut single = initial;
    single["chunk_limit"] = json!(1);
    d.load(s, &single);
    completed(d, s, json!({"kind":"create"}));
    let values: Vec<_> = (0..cf - 1).map(|v| v as i32).collect();
    completed(d, s, json!({"kind":"append","view":0,"samples":values}));
    completed(
        d,
        s,
        json!({"kind":"slice","source":0,"start":1,"frames":cf-2}),
    );
    completed(d, s, json!({"kind":"append","view":0,"samples":[i32::MAX]}));
    let before = d.snapshot(s);
    let result = d.command(
        s,
        &json!({"kind":"append","view":0,"samples":[i32::MIN]}),
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
    assert_eq!(
        d.command(
            s,
            &json!({"kind":"append","view":0,"samples":[i32::MIN]}),
            true,
            true,
            true
        )["status"]["kind"],
        "failed"
    );
    read_equals(
        d,
        s,
        1,
        &(1..cf - 1).map(|i| i as i32).collect::<Vec<_>>(),
        0,
        None,
    );
    completed(d, s, json!({"kind":"remove","view":1}));
    completed(d, s, json!({"kind":"append","view":0,"samples":[i32::MIN]}));
    read_equals(d, s, 0, &[i32::MIN], 0, None);
}
pub fn randomized_media(d: &mut Store, s: &mut Sim<'_>, count: u64) {
    let cf = d.geometry.chunk_frames;
    for _ in 0..count {
        d.load(s, &empty(d));
        completed(d, s, json!({"kind":"create"}));
        let data: Vec<_> = (0..2 * cf + 1).map(|_| d.random.signed()).collect();
        completed(d, s, json!({"kind":"append","view":0,"samples":data}));
        let start = d.random.below(cf);
        let length = 1 + d.random.below(cf);
        completed(
            d,
            s,
            json!({"kind":"slice","source":0,"start":start,"frames":length}),
        );
        completed(d, s, json!({"kind":"clear","view":0}));
        let mut replacement: Vec<_> = (0..cf).map(|_| d.random.signed()).collect();
        completed(
            d,
            s,
            json!({"kind":"append","view":0,"samples":replacement}),
        );
        let position = d.random.below(length);
        read_equals(
            d,
            s,
            1,
            &data[start as usize..(start + length) as usize],
            position,
            None,
        );
        read_equals(d, s, 0, &replacement, 0, None);
        completed(d, s, json!({"kind":"remove","view":1}));
        completed(d, s, json!({"kind":"clear","view":0}));
        replacement.reverse();
        completed(
            d,
            s,
            json!({"kind":"append","view":0,"samples":replacement}),
        );
        read_equals(d, s, 0, &replacement, 0, None);
    }
}
fn finish(d: &mut Store, s: &mut Sim<'_>, mut after: Snapshot, code: u64) {
    let mut done = false;
    for _ in 0..1000 {
        if after.get("done") != 0 {
            done = true;
            break;
        }
        after = d.tick(s).1;
    }
    assert!(done);
    assert_eq!(after.get("error_code"), code);
    d.tick(s);
}
pub fn media_protocol(d: &mut Store, s: &mut Sim<'_>) {
    let g = d.geometry.clone();
    let mut unpublished = empty(d);
    unpublished["views"] = json!([{"chunks":[],"frames":0}]);
    d.load(s, &unpublished);
    d.set(
        s,
        &[
            ("hint_valid", 1),
            ("hint_view", 0),
            ("hint_position", 0),
            ("hint_length", g.chunk_frames),
            ("hint_count", g.chunk_frames),
        ],
    );
    let mut accepted = false;
    for _ in 0..100 {
        s.model.eval();
        let ready = d.get(s, "hint_ready");
        d.tick(s);
        if ready != 0 {
            accepted = true;
            break;
        }
    }
    assert!(accepted);
    d.set(s, &[("hint_valid", 0)]);
    for _ in 0..20 {
        d.tick(s);
    }
    d.base += 0x1004;
    s.model.set("arena_base", d.base);
    completed(
        d,
        s,
        json!({"kind":"append","view":0,"samples":vec![117;g.chunk_frames as usize]}),
    );
    d.load(s, &empty(d));
    for (space, size) in [
        (0, 4),
        (1, g.views),
        (2, d.table_address(g.views, 0)),
        (3, g.slots),
        (4, g.slots),
        (6, 15),
        (7, g.retired_tables * 16),
    ] {
        d.configure(s, space, size, Some(&Bits::from_u64(123)), true);
        d.configure(s, space, size, None, true);
        d.configure(s, space, u32::MAX as u64, None, true);
    }
    d.configure(s, 6, 0, Some(&Bits::default()), true);
    d.configure(s, 7, 0, Some(&Bits::default()), true);
    if !g.slots.is_power_of_two() {
        d.configure(s, 2, d.table_address(0, g.slots), None, true);
    }
    d.configure(s, 0, 0, Some(&Bits::from_u64(g.views + 1)), true);
    for index in 0..g.views {
        assert_eq!(
            d.command(s, &json!({"kind":"create"}), true, true, true)["value"],
            json!({"kind":"view","value":index})
        );
    }
    assert_eq!(
        d.command(s, &json!({"kind":"create"}), true, true, true)["status"],
        json!({"kind":"failed","value":{"error":"memory_full"}})
    );
    d.reset(s);
    assert_eq!(d.read_config(s, 0, 0), 0);
    assert_eq!(d.read_config(s, 0, 1), 0);
    completed(d, s, json!({"kind":"create"}));
    completed(d, s, json!({"kind":"append","view":0,"samples":[i32::MAX]}));
    assert_eq!(
        completed(
            d,
            s,
            json!({"kind":"read","view":0,"position":0,"length":1,"frames":3})
        )["value"]["value"],
        json!(vec![i32::MAX; 3])
    );
    d.set(s, &[("write_valid", 0), ("read_ready", 0)]);
    let after = d.admit(s, 6, 1 << 32, 0, 1, 1);
    finish(d, s, after, 1);
    let original = d.snapshot(s);
    let _after = d.admit(s, 6, 0, 0, 1, 3);
    d.set(
        s,
        &[
            ("cfg_valid", 1),
            ("cfg_write", 1),
            ("cfg_space", 0),
            ("cfg_address", 0),
            ("cfg_write_data", 0),
            ("cmd_valid", 1),
            ("cmd_opcode", 0),
            ("cmd_view", u64::MAX),
            ("cmd_start", u64::MAX),
        ],
    );
    s.model.set("arena_base", d.base + 0x100000000);
    let mut visible = false;
    for _ in 0..200 {
        let (before, after) = d.tick(s);
        assert_eq!(before.get("cfg_ready") | before.get("cmd_ready"), 0);
        if after.get("read_valid") != 0 {
            visible = true;
            break;
        }
    }
    assert!(visible);
    for _ in 0..12 {
        let after = d.tick(s).1;
        assert_ne!(after.get("read_valid"), 0);
        assert_eq!(after.get("read_data") as i32, i32::MAX);
    }
    d.set(s, &[("cfg_valid", 0), ("cmd_valid", 0), ("read_ready", 1)]);
    let mut samples = Vec::new();
    let mut done = false;
    for _ in 0..1000 {
        let (before, after) = d.tick(s);
        if before.get("read_valid") != 0 {
            samples.push(before.get("read_data") as i32);
        }
        if after.get("done") != 0 {
            assert_eq!(after.get("error_code"), 0);
            done = true;
            break;
        }
    }
    assert!(done);
    assert_eq!(samples, vec![i32::MAX; 3]);
    d.set(s, &[("read_ready", 0)]);
    s.model.set("arena_base", d.base);
    d.tick(s);
    assert_eq!(d.snapshot(s), original);
    s.model.set("arena_base", d.base + 0x100000);
    let after = d.admit(s, 6, 0, 0, 1, 1);
    finish(d, s, after, 4);
    let after = d.admit(s, 5, 0, 0, 0, g.chunk_frames);
    finish(d, s, after, 4);
    assert_eq!(s.model.get("memory_fault"), 0);
    s.model.set("arena_base", d.base);
    let recovered = completed(
        d,
        s,
        json!({"kind":"read","view":0,"position":0,"length":1,"frames":1}),
    );
    assert_eq!(
        recovered["value"],
        json!({"kind":"samples","value":[i32::MAX]})
    );
    assert_eq!(recovered["media"], original);
    d.load(s, &original);
    d.memory.borrow_mut().control.fail_next = true;
    let after = d.admit(s, 6, 0, 0, 1, 1);
    d.set(s, &[("read_ready", 1)]);
    finish(d, s, after, 5);
    d.set(s, &[("cmd_valid", 1), ("cmd_opcode", 0)]);
    for _ in 0..5 {
        assert_eq!(d.tick(s).0.get("cmd_ready"), 0);
    }
    d.reset(s);
    assert_eq!(Memory::read(&d.memory, s)[0] as i32, i32::MAX);
    s.model.set("arena_base", d.base + 1);
    let after = d.admit(s, 0, 0, 0, 0, 0);
    finish(d, s, after, 4);
    s.model.set("arena_base", u64::MAX - 3);
    let after = d.admit(s, 0, 0, 0, 0, 0);
    finish(d, s, after, 4);
    s.model.set("arena_base", d.base);
    assert_eq!(d.read_config(s, 0, 0), 0);
}
