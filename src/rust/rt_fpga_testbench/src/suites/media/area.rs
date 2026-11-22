//! Public storage operation latency with independent retained sample vectors.
use crate::{
    Result,
    drivers::store::{Geometry, Store},
    memory::Mode,
    sim::Sim,
};
use serde_json::{Value, json};

fn transaction(
    d: &mut Store,
    s: &mut Sim<'_>,
    rows: &mut Vec<Value>,
    label: &str,
    command: Value,
) -> Value {
    let before = (
        s.cycles,
        d.memory.borrow().reads(),
        d.memory.borrow().writes(),
    );
    let result = d.command(s, &command, false, false, false);
    assert_eq!(result["status"]["kind"], "completed", "{command}: {result}");
    rows.push(json!({"kind":label,"command":command,"result":result,
        "admission_through_cleanup_cycles":s.cycles-before.0,
        "accepted_through_done_cycles":d.timing,
        "axi_reads":d.memory.borrow().reads()-before.1,
        "axi_writes":d.memory.borrow().writes()-before.2,
        "retired_tables_pending":s.model.get("reclaim_pending"),
        "retired_references_pending":s.model.get("reclaim_references")}));
    result
}

fn read(
    d: &mut Store,
    s: &mut Sim<'_>,
    rows: &mut Vec<Value>,
    label: &str,
    view: u64,
    expected: &[i32],
) {
    let result = transaction(
        d,
        s,
        rows,
        label,
        json!({"kind":"read","view":view,"position":0,"length":expected.len(),"frames":expected.len()}),
    );
    assert_eq!(result["value"], json!({"kind":"samples","value":expected}));
}

pub fn operations(width: u64, block_chunks: u64) -> Result<Value> {
    let mut g = Geometry::new([512, 5_763_072, 32], block_chunks);
    g.table_lines = 128;
    g.reuse_views = true;
    let compiled = super::design(&g, width, Mode::Instant)
        .build(&crate::root().join("work/marlin-suite/store-area"))?;
    let mut s = Sim::new(compiled.benchmark_session()?, &[("clk", 10_000, 0)]);
    s.max_ps = 100_000_000_000_000;
    let mut d = Store::new(&mut s, g.clone(), Mode::Instant, 20261122, "")?;
    let start = s.cycles;
    d.reset(&mut s);
    let reset_cycles = s.cycles - start;
    s.model.set("maintenance_enable", 0);
    let mut rows = Vec::new();
    let mut takes = Vec::new();
    for channel in 0..128u64 {
        let created = transaction(
            &mut d,
            &mut s,
            &mut rows,
            "create",
            json!({"kind":"create"}),
        );
        let view = created["value"]["value"].as_u64().unwrap();
        let samples: Vec<i32> = (0..1057)
            .map(|frame| (channel as i32 * 4099 + frame * 17) - 131071)
            .collect();
        transaction(
            &mut d,
            &mut s,
            &mut rows,
            "capture",
            json!({"kind":"append","view":view,"samples":samples}),
        );
        takes.push((view, samples));
    }
    let mut retained = Vec::new();
    for (view, samples) in &takes {
        let sliced = transaction(
            &mut d,
            &mut s,
            &mut rows,
            "retain_offset_slice",
            json!({"kind":"slice","source":view,"start":1000,"frames":33}),
        );
        retained.push((
            sliced["value"]["value"].as_u64().unwrap(),
            samples[1000..1033].to_vec(),
        ));
    }
    for (channel, (view, _)) in takes.iter().enumerate() {
        transaction(
            &mut d,
            &mut s,
            &mut rows,
            "clear_retained_capture",
            json!({"kind":"clear","view":view}),
        );
        let replacement: Vec<i32> = (0..32)
            .map(|frame| -17 - channel as i32 * 43 - frame * 19)
            .collect();
        transaction(
            &mut d,
            &mut s,
            &mut rows,
            "retake",
            json!({"kind":"append","view":view,"samples":replacement}),
        );
        read(
            &mut d,
            &mut s,
            &mut rows,
            "retake_read",
            *view,
            &replacement,
        );
    }
    for (view, samples) in &retained {
        read(&mut d, &mut s, &mut rows, "retained_read", *view, samples);
    }
    for (view, _) in &takes {
        transaction(
            &mut d,
            &mut s,
            &mut rows,
            "remove",
            json!({"kind":"remove","view":view}),
        );
    }
    let before = (
        s.cycles,
        d.memory.borrow().reads(),
        d.memory.borrow().writes(),
    );
    d.drain(&mut s);
    rows.push(json!({"kind":"maintenance_drain","admission_through_cleanup_cycles":s.cycles-before.0,
        "axi_reads":d.memory.borrow().reads()-before.1,"axi_writes":d.memory.borrow().writes()-before.2}));
    for (view, samples) in &retained {
        read(
            &mut d,
            &mut s,
            &mut rows,
            "retained_after_maintenance",
            *view,
            samples,
        );
        transaction(
            &mut d,
            &mut s,
            &mut rows,
            "remove_retained",
            json!({"kind":"remove","view":view}),
        );
    }
    let before = (
        s.cycles,
        d.memory.borrow().reads(),
        d.memory.borrow().writes(),
    );
    d.drain(&mut s);
    rows.push(json!({"kind":"final_maintenance_drain","admission_through_cleanup_cycles":s.cycles-before.0,
        "axi_reads":d.memory.borrow().reads()-before.1,"axi_writes":d.memory.borrow().writes()-before.2}));
    assert_eq!(s.model.get("reclaim_pending"), 0);
    assert_eq!(d.read_config(&mut s, 6, 11), 0);
    let allocated = d.read_config(&mut s, 0, 1);
    assert_eq!(d.read_config(&mut s, 0, 2), allocated);
    let recovered: std::collections::BTreeSet<_> = (0..allocated)
        .map(|i| d.read_config(&mut s, 4, i))
        .collect();
    assert_eq!(recovered, (0..allocated).collect());
    Ok(
        json!({"scope":"public media store at primary capacity; 128 captures of1057 frames, offset33-frame exports, clear/retake/read and final retirement; ideal AXI memory, no SP/JP or DDR-added delay",
        "validation":"all commands complete; retained and replacement samples match independent Rust vectors; every allocated block is recovered uniquely and all metadata pages are freed",
        "axi_data_width":width,"block_chunks":block_chunks,"parameters":g.parameters(),"reset_cycles":reset_cycles,
        "measurements":rows,"allocated_blocks":allocated,"memory":d.memory.borrow_mut().report(true)}),
    )
}
