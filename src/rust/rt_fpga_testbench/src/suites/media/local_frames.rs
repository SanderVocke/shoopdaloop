//! Explicit duration-limit checks and independent retained sample vectors.
use crate::{
    Result,
    drivers::store::{Geometry, Store},
    memory::Mode,
    sim::Sim,
};
use serde_json::{Value, json};

fn request(
    d: &mut Store,
    s: &mut Sim<'_>,
    rows: &mut Vec<Value>,
    c: Value,
    error: Option<&str>,
) -> Value {
    let before = (
        s.cycles,
        d.memory.borrow().reads(),
        d.memory.borrow().writes(),
    );
    let r = d.command(s, &c, true, false, false);
    if let Some(e) = error {
        assert_eq!(r["status"], json!({"kind":"failed","value":{"error":e}}));
    } else {
        assert_eq!(r["status"], json!({"kind":"completed"}));
    }
    rows.push(json!({"op":c["kind"],"error":error.unwrap_or("OK"),"command":c,"result":r,"cycles":s.cycles-before.0,
        "axi_reads":d.memory.borrow().reads()-before.1,"axi_writes":d.memory.borrow().writes()-before.2}));
    r
}

fn refuse_count(d: &mut Store, s: &mut Sim<'_>, rows: &mut Vec<Value>, view: u64, count: u64) {
    let globals: Vec<_> = (0..4).map(|a| d.read_config(s, 0, a)).collect();
    let descriptor = d.configure(s, 1, view, None, false);
    let start = s.cycles;
    d.admit(s, 5, view, 0, 0, count);
    d.set(
        s,
        &[
            ("cmd_opcode", 0),
            ("cmd_view", u64::MAX),
            ("cmd_start", u64::MAX),
            ("cmd_length", u64::MAX),
            ("cmd_count", 0),
            ("write_valid", 1),
            ("write_data", 0xdeadbeef),
            ("read_ready", 1),
        ],
    );
    let mut done = false;
    for _ in 0..10000 {
        let (before, after) = d.tick(s);
        assert_eq!(before.get("write_ready") | before.get("read_valid"), 0);
        if after.get("done") != 0 {
            done = true;
            break;
        }
    }
    assert!(done, "oversized append refusal");
    assert_eq!(d.get(s, "error_code"), 3);
    d.set(s, &[("write_valid", 0), ("read_ready", 0)]);
    d.tick(s);
    rows.push(
        json!({"op":"append_raw_count","error":"memory_full","count":count,"cycles":s.cycles-start,
        "scope":"accepted request through refusal; poisoned command inputs, no sample accepted"}),
    );
    assert_eq!(d.configure(s, 1, view, None, false), descriptor);
    assert_eq!(
        (0..4).map(|a| d.read_config(s, 0, a)).collect::<Vec<_>>(),
        globals
    );
}

pub fn boundaries(bits: u64, width: u64, mode: Mode) -> Result<Value> {
    assert!([5, 21, 64].contains(&bits));
    let mut g = Geometry::new([8, 256, 4], 4);
    g.reuse_views = true;
    let compiled = super::design(&g, width, mode)
        .parameters(&[("LOCAL_FRAME_BITS", bits)])
        .build(&crate::root().join("work/marlin-suite/local-frames"))?;
    let mut s = Sim::new(compiled.benchmark_session()?, &[("clk", 10_000, 0)]);
    s.max_ps = 100_000_000_000_000;
    let mut d = Store::new(&mut s, g, mode, 20261122, "")?;
    d.reset(&mut s);
    s.model.set("maintenance_enable", 0);
    let mut rows = Vec::new();
    let created = request(&mut d, &mut s, &mut rows, json!({"kind":"create"}), None);
    let view = created["value"]["value"].as_u64().unwrap();
    let mut samples: Vec<i32> = (0..31).map(|i| i * 7919 - 65537).collect();
    for part in [&samples[..18], &samples[18..]] {
        request(
            &mut d,
            &mut s,
            &mut rows,
            json!({"kind":"append","view":view,"samples":part}),
            None,
        );
    }
    let snapshot = d.configure(&mut s, 1, view, None, false);
    assert_eq!(snapshot.get(64, 64), 31);
    request(
        &mut d,
        &mut s,
        &mut rows,
        json!({"kind":"append","view":view,"samples":[-2147483648i32]}),
        (bits == 5).then_some("memory_full"),
    );
    if bits == 5 {
        assert_eq!(d.configure(&mut s, 1, view, None, false), snapshot);
    } else {
        samples.push(i32::MIN);
    }
    refuse_count(&mut d, &mut s, &mut rows, view, u64::from(u32::MAX));
    let read = request(
        &mut d,
        &mut s,
        &mut rows,
        json!({"kind":"read","view":view,"position":0,"length":samples.len(),"frames":samples.len()}),
        None,
    );
    assert_eq!(read["value"], json!({"kind":"samples","value":samples}));
    let wrapped: Vec<_> = (0..7)
        .map(|i| samples[(samples.len() - 2 + i) % samples.len()])
        .collect();
    let r = request(
        &mut d,
        &mut s,
        &mut rows,
        json!({"kind":"read","view":view,"position":samples.len()-2,"length":samples.len(),"frames":7}),
        None,
    );
    assert_eq!(r["value"], json!({"kind":"samples","value":wrapped}));
    let slice = request(
        &mut d,
        &mut s,
        &mut rows,
        json!({"kind":"slice","source":view,"start":7,"frames":19}),
        None,
    )["value"]["value"]
        .as_u64()
        .unwrap();
    let nested = request(
        &mut d,
        &mut s,
        &mut rows,
        json!({"kind":"slice","source":slice,"start":5,"frames":9}),
        None,
    )["value"]["value"]
        .as_u64()
        .unwrap();
    for c in [
        json!({"kind":"read","view":view,"position":1u64<<40,"length":31,"frames":1}),
        json!({"kind":"slice","source":view,"start":1u64<<40,"frames":1}),
        json!({"kind":"slice","source":view,"start":0,"frames":1u64<<40}),
    ] {
        request(&mut d, &mut s, &mut rows, c, Some("invalid_view"));
    }
    if bits < 64 {
        let original = d.configure(&mut s, 1, view, None, false);
        for frames in [1u64 << bits, 1u64 << 40, u64::MAX] {
            let mut invalid = original.clone();
            invalid.set(64, 64, frames);
            d.configure(&mut s, 1, view, Some(&invalid), true);
            assert_eq!(d.configure(&mut s, 1, view, None, false), original);
        }
        // Admission-only sentinel; existing payload is not observed under this synthetic descriptor.
        let max = (1u64 << bits) - 1;
        let mut sentinel = original.clone();
        sentinel.set(64, 64, max);
        d.configure(&mut s, 1, view, Some(&sentinel), false);
        assert_eq!(d.configure(&mut s, 1, view, None, false), sentinel);
        request(
            &mut d,
            &mut s,
            &mut rows,
            json!({"kind":"append","view":view,"samples":[1]}),
            Some("memory_full"),
        );
        assert_eq!(d.configure(&mut s, 1, view, None, false), sentinel);
        d.configure(&mut s, 1, view, Some(&original), false);
    }
    request(
        &mut d,
        &mut s,
        &mut rows,
        json!({"kind":"clear","view":view}),
        None,
    );
    request(
        &mut d,
        &mut s,
        &mut rows,
        json!({"kind":"append","view":view,"samples":[7,8,9]}),
        None,
    );
    let r = request(
        &mut d,
        &mut s,
        &mut rows,
        json!({"kind":"read","view":nested,"position":0,"length":9,"frames":9}),
        None,
    );
    assert_eq!(
        r["value"],
        json!({"kind":"samples","value":samples[12..21]})
    );
    for v in [view, slice, nested] {
        request(
            &mut d,
            &mut s,
            &mut rows,
            json!({"kind":"remove","view":v}),
            None,
        );
    }
    d.drain(&mut s);
    let empty = d.snapshot(&mut s);
    assert!(
        empty["views"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v.is_null())
    );
    d.reset(&mut s);
    request(
        &mut d,
        &mut s,
        &mut rows,
        json!({"kind":"read","view":view,"position":0,"length":1,"frames":1}),
        Some("invalid_view"),
    );
    let fresh =
        request(&mut d, &mut s, &mut rows, json!({"kind":"create"}), None)["value"]["value"]
            .as_u64()
            .unwrap();
    request(
        &mut d,
        &mut s,
        &mut rows,
        json!({"kind":"append","view":fresh,"samples":[17,-19,23]}),
        None,
    );
    let saved = d.snapshot(&mut s);
    d.load(&mut s, &saved);
    d.memory.borrow_mut().control.fail_next = true;
    request(
        &mut d,
        &mut s,
        &mut rows,
        json!({"kind":"read","view":fresh,"position":0,"length":3,"frames":1}),
        Some("memory_fault"),
    );
    assert_eq!(s.model.get("memory_fault"), 1);
    d.set(&mut s, &[("cmd_valid", 1), ("cmd_opcode", 0)]);
    for _ in 0..5 {
        assert_eq!(d.tick(&mut s).0.get("cmd_ready"), 0);
    }
    d.reset(&mut s);
    assert_eq!(s.model.get("memory_fault"), 0);
    Ok(
        json!({"local_frame_bits":bits,"axi_data_width":width,"mode":format!("{mode:?}"),
        "scope":"Duration/refusal and public retained sample checks. Synthetic max descriptor tests admission only; no synthetic payload read.","measurements":rows}),
    )
}
