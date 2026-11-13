//! Ownership-only preload campaigns and isolated reclamation timing measurements.
use crate::{
    Result,
    bits::Bits,
    drivers::store::{Geometry, Store},
    memory::Mode,
    sim::Sim,
};
use serde_json::{Value, json};
fn descriptor(d: &mut Store, s: &mut Sim<'_>, view: u64, frames: u64, count: u64) {
    let mut value = Bits::from_u64(1);
    value.set(64, 64, frames).set(128, 32, count);
    d.configure(s, 1, view, Some(&value), false);
}
fn command(d: &mut Store, s: &mut Sim<'_>, c: Value) -> Value {
    let r = d.command(s, &c, false, false, false);
    assert_eq!(r["status"]["kind"], "completed", "{c}: {r}");
    r
}
pub fn stress(loops: u64, channels: u64, seconds: u64) -> Result<Value> {
    assert!(seconds > 0 && seconds <= 30);
    let mut g = Geometry::new([64, 45056 * 32, 32], 32);
    g.table_lines = 128;
    let compiled = super::deferred_design(&g, 64, Mode::Instant)
        .build(&crate::root().join("work/marlin-suite/stress"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    s.max_ps = 100_000_000_000_000;
    let mut d = Store::new(&mut s, g.clone(), Mode::Instant, 20261112, "")?;
    d.reset(&mut s);
    d.memory.borrow_mut().control.stalls = false;
    let count = loops * channels;
    let frames = seconds * 48000;
    let bf = g.chunk_frames * g.block_chunks;
    let blocks = frames.div_ceil(bf);
    let total = count * blocks;
    assert!(count <= g.views && total <= g.blocks());
    for view in 0..count {
        for ordinal in 0..blocks {
            let block = view * blocks + ordinal;
            d.write_config(&mut s, 2, d.table_address(view, ordinal), block);
            d.write_config(&mut s, 3, block, 1);
        }
        descriptor(&mut d, &mut s, view, frames, blocks);
    }
    d.write_config(&mut s, 0, 0, count);
    d.write_config(&mut s, 0, 1, total);
    d.write_config(&mut s, 0, 2, 0);
    assert!(d.read_config(&mut s, 6, 11) > 0);
    let before = (
        s.model.get("core.metadata.directory_reads"),
        s.model.get("core.metadata.table_reads"),
    );
    let clear_start = s.cycles;
    let mut times = Vec::new();
    for view in 0..count {
        command(&mut d, &mut s, json!({"kind":"clear","view":view}));
        times.push(d.timing);
    }
    assert_eq!(
        (
            s.model.get("core.metadata.directory_reads"),
            s.model.get("core.metadata.table_reads")
        ),
        before
    );
    assert_eq!(s.model.get("reclaim_pending"), count);
    assert_eq!(s.model.get("reclaim_references"), total);
    let mass_clear_cycles = s.cycles - clear_start;
    let pages_before = d.read_config(&mut s, 6, 11);
    let mut progress = Vec::new();
    for iteration in 0..40 {
        d.grant(&mut s, 0, None);
        for view in 0..count {
            command(
                &mut d,
                &mut s,
                json!({"kind":"append","view":view,"samples":vec![view as i32+1;32]}),
            );
        }
        s.idle(100);
        progress.push(json!({"iteration":iteration,"free_blocks":d.read_config(&mut s,0,2),"pending_references":d.read_config(&mut s,6,1),"freed_blocks":s.model.get("reclaim_freed")}));
    }
    assert!(s.model.get("reclaim_freed") > count);
    d.drain(&mut s);
    assert_eq!(s.model.get("reclaim_pending"), 0);
    assert_eq!(s.model.get("reclaim_blocks"), total);
    let maintenance_cycles = s.model.get("reclaim_cycles");
    let pages_after = d.read_config(&mut s, 6, 11);
    for view in 0..count {
        let r = command(
            &mut d,
            &mut s,
            json!({"kind":"read","view":view,"position":0,"length":1280,"frames":1280}),
        );
        assert_eq!(
            r["value"],
            json!({"kind":"samples","value":vec![view as i32+1;1280]})
        );
        command(&mut d, &mut s, json!({"kind":"clear","view":view}));
    }
    d.drain(&mut s);
    assert_eq!(d.read_config(&mut s, 6, 11), 0);
    let allocated = d.read_config(&mut s, 0, 1);
    assert_eq!(d.read_config(&mut s, 0, 2), allocated);
    let recovered: std::collections::BTreeSet<_> = (0..allocated)
        .map(|i| d.read_config(&mut s, 4, i))
        .collect();
    assert_eq!(recovered, (0..allocated).collect());
    let report = json!({"scope":"media-store ownership stress; old payloads unobserved; new captures/readback checked","loops":loops,"channels_per_loop":channels,"recording_frames":frames,"sample_rate":48000,"recording_seconds":seconds,"blocks_per_take":blocks,"released_references":total,"clear_cycles":times,"mass_clear_cycles":mass_clear_cycles,"logical_bytes_retired":count*frames*4,"allocated_bytes_retired":total*bf*4,"block_id_bytes_retired":total*4,"queue_record_bytes":count*64,"metadata_pages_before_cleanup":pages_before,"metadata_pages_after_cleanup":pages_after,"recording_progress":progress,"maintenance_cycles":maintenance_cycles,"recovered_unique_blocks":recovered.len(),"memory":d.memory.borrow_mut().report(true),"result":"passed"});
    let output = crate::artifact_directory("stress");
    std::fs::create_dir_all(&output)?;
    std::fs::write(
        output.join(format!("stress-{loops}x{channels}.json")),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}
pub fn isolated_twenty_releases(d: &mut Store, s: &mut Sim<'_>) -> Value {
    let bf = d.geometry.chunk_frames * d.geometry.block_chunks;
    let count = 20;
    assert!(d.geometry.blocks() >= count);
    let mut rows = Vec::new();
    for (cold, owners, groups) in [
        (false, 1, 1),
        (true, 1, 1),
        (false, 1, 20),
        (true, 1, 20),
        (true, 2, 1),
    ] {
        d.reset(s);
        d.memory.borrow_mut().control.stalls = false;
        let per_table = count / groups;
        for group in 0..groups {
            for owner in 0..owners {
                let view = group * owners + owner;
                for ordinal in 0..per_table {
                    d.write_config(
                        s,
                        2,
                        d.table_address(view, ordinal),
                        group * per_table + ordinal,
                    );
                }
                descriptor(d, s, view, per_table * bf, per_table);
            }
        }
        for block in 0..count {
            d.write_config(s, 3, block, owners);
        }
        d.write_config(s, 0, 0, owners * groups);
        d.write_config(s, 0, 1, count);
        d.write_config(s, 0, 2, 0);
        d.fence(s);
        for group in 0..groups {
            command(d, s, json!({"kind":"clear","view":group*owners}));
        }
        if cold {
            d.write_config(s, 5, 0, 0);
        }
        let start = s.cycles;
        let active_start = s.model.get("reclaim_cycles");
        let (reads, writes) = {
            let m = d.memory.borrow();
            (m.reads(), m.writes())
        };
        d.grant(s, count, Some(100_000));
        s.until(100_000, "twenty reference releases", |m| {
            m.get("reclaim_blocks") == count && m.get("reclaim_active") == 0
        });
        let elapsed = s.cycles - start;
        assert_eq!(
            s.model.get("reclaim_freed"),
            if owners == 1 { count } else { 0 }
        );
        assert_eq!(s.model.get("reclaim_pending"), 0);
        let mut row = json!({"cold_metadata":cold,"owners_per_block":owners,"retired_tables":groups,"released_references":count,"freed_blocks":s.model.get("reclaim_freed"),"elapsed_cycles":elapsed,"active_collector_cycles":s.model.get("reclaim_cycles")-active_start,"axi_read_bursts":d.memory.borrow().reads()-reads,"axi_write_bursts":d.memory.borrow().writes()-writes,"microseconds_at_100mhz":elapsed as f64/100.0,"quantum_fraction_at_48khz_q32":elapsed as f64/100_000_000.0/(32.0/48000.0)});
        d.fence(s);
        let settled = s.cycles - start;
        row["fence_cycles"] = json!(settled - elapsed);
        row["settled_cycles"] = json!(settled);
        row["settled_microseconds_at_100mhz"] = json!(settled as f64 / 100.0);
        row["axi_read_bursts_including_fence"] = json!(d.memory.borrow().reads() - reads);
        row["axi_write_bursts_including_fence"] = json!(d.memory.borrow().writes() - writes);
        rows.push(row);
    }
    json!({"scope":"Isolated maintenance; no competing audio or dirty sample payloads; includes queue/page cleanup and grant issue overhead.","profiling_grant":{"block_ceiling":count,"cycles":100000},"batches":rows})
}
pub fn timing(width: u64, mode: Mode) -> Result<Value> {
    let g = Geometry::new([32, 32 * 32, 4], 32);
    let compiled = super::deferred_design(&g, width, mode)
        .build(&crate::root().join("work/marlin-suite/reclaim-timing"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    let mut d = Store::new(&mut s, g, mode, 20261113, "")?;
    let mut report = isolated_twenty_releases(&mut d, &mut s);
    report["memory"] = d.memory.borrow_mut().report(true);
    let output = crate::artifact_directory("reclaim-timing");
    std::fs::create_dir_all(&output)?;
    std::fs::write(
        output.join("reclaim-timing.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}
