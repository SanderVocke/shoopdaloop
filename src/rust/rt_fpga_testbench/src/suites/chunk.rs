//! Chunk visibility, coherence, buffer ownership and retirement protocols.
use crate::{Result, drivers::chunk::Chunk, memory::Mode, model::Design, random::Random, sim::Sim};
use serde_json::{Value, json};
pub const CONFIGS: &[(&str, [u64; 4])] = &[
    ("default", [16, 32, 4, 4]),
    ("small", [8, 4, 2, 2]),
    ("odd", [9, 8, 3, 3]),
    ("minimum", [3, 1, 1, 1]),
];
pub fn design(c: [u64; 4], width: u64, mode: Mode, retire: u64) -> Design {
    let root = crate::root();
    let mut sources = vec![
        root.join("fpga/chunk_memory/rtl/chunk_ram.sv"),
        root.join("fpga/chunk_memory/rtl/chunk_memory.sv"),
    ];
    let mut parameters = vec![
        ("SLOTS", c[0]),
        ("CHUNK_FRAMES", c[1]),
        ("READ_BUFFERS", c[2]),
        ("WRITE_BUFFERS", c[3]),
        ("RETIRE_CHUNKS", retire),
    ];
    let top = if mode == Mode::Legacy {
        "chunk_memory"
    } else {
        sources.extend(
            "axi_master chunk_dma word_dma word_axi chunk_axi chunk_memory_axi"
                .split_whitespace()
                .map(|n| root.join(format!("fpga/axi_memory/rtl/{n}.sv"))),
        );
        parameters.extend([("AXI_DATA_WIDTH", width), ("PHYSICAL_BASE", 0x01000000)]);
        "chunk_memory_axi"
    };
    let design = Design::new(top, sources).parameters(&parameters);
    if mode == Mode::Instant {
        design.memory((c[0] * c[1]) as usize, 0x01000000)
    } else {
        design
    }
}
pub fn run(name: &str, c: [u64; 4], width: u64, mode: Mode, random_cases: u64) -> Result<()> {
    let compiled =
        design(c, width, mode, 1).build(&crate::root().join("work/marlin-suite/chunk"))?;
    for test in [buffering_protocol, faults_and_reset, read_stream_protocol] {
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        let mut d = Chunk::new(&mut s, c, mode, 20261111)?;
        test(&mut d, &mut s);
        d.memory.borrow_mut().report(true);
    }
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    let mut d = Chunk::new(&mut s, c, mode, 20261111)?;
    conformance(&mut d, &mut s, random_cases);
    d.memory.borrow_mut().report(true);
    println!("chunk/{name}/{width}/{mode:?}: 4 groups, {random_cases} seeded cases");
    Ok(())
}
pub fn retirement(name: &str, mut c: [u64; 4], width: u64, mode: Mode, bc: u64) -> Result<()> {
    c[0] *= bc;
    let compiled =
        design(c, width, mode, bc).build(&crate::root().join("work/marlin-suite/chunk"))?;
    for test in [
        block_retirement,
        background_retirement_preserves_unrelated_commands,
    ] {
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        let mut d = Chunk::new(&mut s, c, mode, 20261112)?;
        d.memory.borrow_mut().control.stalls = false;
        test(&mut d, &mut s, bc);
        d.memory.borrow_mut().report(true);
    }
    println!("chunk-retirement/{name}/{bc}/{width}/{mode:?}: 2 groups");
    Ok(())
}
fn op(kind: &str, chunk: u64, offset: u64, frames: u64, samples: &[i32]) -> Value {
    json!({"kind":kind,"chunk":chunk,"offset":offset,"frames":frames,"samples":samples})
}
fn command(d: &mut Chunk, s: &mut Sim<'_>, operation: Value, id: u64) -> Vec<i32> {
    let (code, samples) = d.command(s, &operation, id, false);
    assert_eq!(code, 0, "{operation}");
    samples
}
fn compare(d: &mut Chunk, s: &mut Sim<'_>, mut operations: Vec<Value>, seed: u64) {
    let c = d.capacities;
    let mut random = Random::new(seed);
    let mut visible: Vec<_> = (0..c[0] * c[1]).map(|_| random.signed()).collect();
    d.reset(s, Some(&visible));
    operations.push(op("fence", 0, 0, 0, &[]));
    for (i, operation) in operations.iter().enumerate() {
        let kind = operation["kind"].as_str().unwrap();
        let chunk = operation["chunk"].as_u64().unwrap_or(0);
        let offset = operation["offset"].as_u64().unwrap_or(0);
        let mut expected = Vec::new();
        let mut code = 0;
        if kind != "fence" {
            if chunk >= c[0] {
                code = 1;
            } else if kind == "read" || kind == "write" {
                let data: Vec<i32> = serde_json::from_value(operation["samples"].clone()).unwrap();
                let count = if kind == "write" {
                    data.len() as u64
                } else {
                    operation["frames"].as_u64().unwrap()
                };
                if offset > c[1] || count > c[1] - offset {
                    code = 4;
                } else {
                    let first = (chunk * c[1] + offset) as usize;
                    if kind == "read" {
                        expected = visible[first..first + count as usize].to_vec();
                    } else {
                        visible[first..first + count as usize].copy_from_slice(&data);
                    }
                }
            }
        }
        assert_eq!(
            d.command(s, operation, u64::from(u32::MAX) - i as u64, true),
            (code, expected),
            "{operation}"
        );
        if kind == "fence" {
            assert_eq!(d.image(s), visible);
        }
    }
    assert!(command(d, s, op("fence", 0, 0, 0, &[]), 0).is_empty());
    assert_eq!(d.image(s), visible);
    assert!(d.memory.borrow().active().is_none());
}
fn conformance(d: &mut Chunk, s: &mut Sim<'_>, count: u64) {
    let [slots, frames, _, _] = d.capacities;
    let mut operations: Vec<_> = (0..slots)
        .map(|chunk| {
            op(
                "write",
                chunk,
                0,
                0,
                &(0..frames)
                    .map(|i| (chunk * 19) as i32 - i as i32)
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    operations.extend([
        op("read", 0, 0, frames, &[]),
        op("prefetch", slots - 1, 0, 0, &[]),
        op("write", 0, frames - 1, 0, &[i32::MIN]),
        op("retire", 0, 0, 0, &[]),
        op("write", 0, 0, 0, &[i32::MAX]),
        op("read", 0, 0, frames, &[]),
        op("read", 0, frames, 0, &[]),
        op("write", 0, frames, 0, &[]),
        op("read", 0, u32::MAX as u64, 0, &[]),
        op("read", 0, 0, u32::MAX as u64, &[]),
        op("read", u32::MAX as u64, 0, 0, &[]),
        op("prefetch", u32::MAX as u64, 0, 0, &[]),
        op("retire", slots, 0, 0, &[]),
    ]);
    compare(d, s, operations, 20261111);
    for seed in 0..count {
        let mut random = Random::new(seed);
        let operations = (0..100)
            .map(|_| {
                let kind = [
                    "read", "read", "write", "write", "prefetch", "retire", "fence",
                ][random.below(7) as usize];
                let chunk = if kind == "fence" {
                    0
                } else {
                    random.below(slots)
                };
                let offset = if matches!(kind, "read" | "write") {
                    random.below(frames + 1)
                } else {
                    0
                };
                let n = if matches!(kind, "read" | "write") {
                    random.below(frames - offset + 1)
                } else {
                    0
                };
                let data: Vec<_> = if kind == "write" {
                    (0..n).map(|_| random.signed()).collect()
                } else {
                    vec![]
                };
                op(
                    kind,
                    chunk,
                    offset,
                    if kind == "read" { n } else { 0 },
                    &data,
                )
            })
            .collect();
        compare(d, s, operations, seed);
    }
}
fn start(d: &mut Chunk, s: &mut Sim<'_>, opcode: u64, id: u64, chunk: u64, count: u64) {
    start_at(d, s, opcode, id, chunk, 0, count);
}
fn start_at(
    d: &mut Chunk,
    s: &mut Sim<'_>,
    opcode: u64,
    id: u64,
    chunk: u64,
    offset: u64,
    count: u64,
) {
    s.model.set_all(&[
        ("cmd_valid", 1),
        ("cmd_opcode", opcode),
        ("cmd_id", id),
        ("cmd_chunk", chunk),
        ("cmd_offset", offset),
        ("cmd_count", count),
        ("response_ready", 0),
    ]);
    let mut accepted = false;
    for _ in 0..1000 {
        if d.tick(s).0.get("cmd_ready") != 0 {
            accepted = true;
            break;
        }
    }
    assert!(accepted);
    s.model.set("cmd_valid", 0);
}
fn finish(d: &mut Chunk, s: &mut Sim<'_>, id: u64) {
    for _ in 0..100_000 {
        if s.model.get("response_valid") != 0 {
            assert_eq!(s.model.get("response_id"), id);
            assert_eq!(s.model.get("response_error"), 0);
            s.model.set("response_ready", 1);
            d.tick(s);
            s.model.set("response_ready", 0);
            return;
        }
        d.tick(s);
    }
    panic!("chunk held response timeout");
}
fn buffering_protocol(d: &mut Chunk, s: &mut Sim<'_>) {
    let [slots, frames, _, writes] = d.capacities;
    let initial: Vec<_> = (0..slots * frames).map(|i| i as i32).collect();
    let payload: Vec<_> = (0..frames).map(|i| -71 + i as i32).collect();
    let replacement: Vec<_> = (0..frames).map(|i| 109 - i as i32).collect();
    d.memory.borrow_mut().control.stalls = false;
    d.reset(s, Some(&initial));
    d.memory.borrow_mut().control.pause = true;
    command(d, s, op("write", 0, 0, 0, &payload), 1);
    assert_eq!(d.memory.borrow().writes(), 0);
    assert_eq!(command(d, s, op("read", 0, 0, frames, &[]), 2), payload);
    assert_eq!(d.memory.borrow().reads(), 0);
    for _ in 0..frames + 10 {
        d.tick(s);
    }
    d.hint(s, slots - 1);
    start(d, s, 3, 77, 0, 0);
    for _ in 0..12 {
        assert_eq!(d.tick(s).1.get("response_valid"), 0);
    }
    d.memory.borrow_mut().control.pause = false;
    s.until(10_000, "fence drain", |m| m.get("response_valid") != 0);
    for _ in 0..12 {
        let b = d.tick(s).0;
        assert_ne!(b.get("response_valid"), 0);
        assert_eq!(b.get("response_id"), 77);
    }
    finish(d, s, 77);
    assert_eq!(d.image(s)[..frames as usize], payload);
    command(d, s, op("retire", 0, 0, 0, &[]), 3);
    command(d, s, op("write", 0, 0, 0, &[91]), 4);
    let mut wanted = payload.clone();
    wanted[0] = 91;
    assert_eq!(command(d, s, op("read", 0, 0, frames, &[]), 5), wanted);
    command(d, s, op("fence", 0, 0, 0, &[]), 6);
    d.hint(s, slots - 1);
    s.until(10_000, "prefetch completion", |m| m.get("busy") == 0);
    let reads = d.memory.borrow().reads();
    assert_eq!(
        command(d, s, op("read", slots - 1, 0, frames, &[]), 7),
        initial[initial.len() - frames as usize..]
    );
    assert_eq!(d.memory.borrow().reads(), reads);
    // Supersede an accepted prefetch before its old read is published.
    d.reset(s, Some(&initial));
    d.hint(s, 0);
    for _ in 0..20 {
        d.tick(s);
        if d.memory.borrow().active().is_some() {
            break;
        }
    }
    assert!(d.memory.borrow().active().is_some());
    d.memory.borrow_mut().control.pause = true;
    let requests = d.memory.borrow().requests();
    command(d, s, op("write", 0, 0, 0, &payload), 70);
    assert_eq!(command(d, s, op("read", 0, 0, frames, &[]), 71), payload);
    command(d, s, op("write", 0, 0, 0, &replacement), 72);
    assert_eq!(d.memory.borrow().requests(), requests);
    d.memory.borrow_mut().control.pause = false;
    command(d, s, op("fence", 0, 0, 0, &[]), 73);
    assert_eq!(
        command(d, s, op("read", 0, 0, frames, &[]), 74),
        replacement
    );
    assert_eq!(d.image(s)[..frames as usize], replacement);
    // Completed and queued speculative lines both relinquish their old ownership.
    d.reset(s, Some(&initial));
    d.hint(s, 0);
    s.until(10_000, "prefetch resident", |m| m.get("busy") == 0);
    let reads = d.memory.borrow().reads();
    d.memory.borrow_mut().control.pause = true;
    command(d, s, op("write", 0, 0, 0, &payload), 75);
    assert_eq!(command(d, s, op("read", 0, 0, frames, &[]), 76), payload);
    assert_eq!(d.memory.borrow().reads(), reads);
    d.memory.borrow_mut().control.pause = false;
    command(d, s, op("fence", 0, 0, 0, &[]), 77);
    d.reset(s, Some(&initial));
    d.hint(s, 1);
    for _ in 0..20 {
        d.tick(s);
        if d.memory.borrow().active().is_some() {
            break;
        }
    }
    assert!(d.memory.borrow().active().is_some());
    d.memory.borrow_mut().control.pause = true;
    d.hint(s, 0);
    command(d, s, op("write", 0, 0, 0, &payload), 78);
    d.hint(s, 2);
    assert_eq!(command(d, s, op("read", 0, 0, frames, &[]), 79), payload);
    d.memory.borrow_mut().control.pause = false;
    command(d, s, op("fence", 0, 0, 0, &[]), 80);
    command(d, s, op("retire", 0, 0, 0, &[]), 81);
    command(d, s, op("write", 0, 0, 0, &replacement), 82);
    assert_eq!(
        command(d, s, op("read", 0, 0, frames, &[]), 83),
        replacement
    );
    d.reset(s, Some(&initial));
    d.memory.borrow_mut().control.pause = true;
    command(d, s, op("write", 0, 0, 0, &payload), 8);
    for _ in 0..frames + 12 {
        d.tick(s);
    }
    d.hint(s, 0);
    start(d, s, 4, 88, 0, 0);
    for _ in 0..12 {
        assert_eq!(d.tick(s).1.get("response_valid"), 0);
    }
    d.memory.borrow_mut().control.pause = false;
    finish(d, s, 88);
    assert!(d.memory.borrow().active().is_none());
    assert_eq!(s.model.get("busy"), 0);
    command(d, s, op("write", 0, 0, 0, &[92]), 9);
    let mut wanted = payload.clone();
    wanted[0] = 92;
    assert_eq!(command(d, s, op("read", 0, 0, frames, &[]), 10), wanted);
    command(d, s, op("fence", 0, 0, 0, &[]), 11);
    d.reset(s, Some(&initial));
    d.memory.borrow_mut().control.pause = true;
    for chunk in 0..writes {
        command(d, s, op("write", chunk, 0, 0, &payload), 20 + chunk);
    }
    s.model.set_all(&[
        ("cmd_valid", 1),
        ("cmd_opcode", 1),
        ("cmd_id", 99),
        ("cmd_chunk", writes),
        ("cmd_offset", 0),
        ("cmd_count", frames),
    ]);
    for _ in 0..12 {
        let b = d.tick(s).0;
        assert_eq!(b.get("cmd_ready") | b.get("write_ready"), 0);
    }
    s.model.set("cmd_valid", 0);
    d.memory.borrow_mut().control.pause = false;
    command(d, s, op("fence", 0, 0, 0, &[]), 30);
    d.reset(s, Some(&initial));
    s.model.set("read_ready", 0);
    start(d, s, 0, 100, 0, frames);
    s.until(10_000, "pinned read", |m| m.get("read_valid") != 0);
    d.hint(s, slots - 1);
    for _ in 0..2 * frames + 10 {
        let b = d.tick(s).1;
        assert_ne!(b.get("read_valid"), 0);
        assert_eq!(b.get("read_data") as i32, initial[0]);
    }
    let mut observed = Vec::new();
    s.model.set_all(&[("read_ready", 1), ("response_ready", 1)]);
    let mut done = false;
    for _ in 0..10_000 {
        let b = d.tick(s).0;
        if b.get("read_valid") != 0 {
            observed.push(b.get("read_data") as i32);
        }
        if b.get("response_valid") != 0 {
            done = true;
            break;
        }
    }
    assert!(done);
    assert_eq!(observed, initial[..frames as usize]);
    s.model.set_all(&[("read_ready", 0), ("response_ready", 0)]);
    command(d, s, op("fence", 0, 0, 0, &[]), 101);
}
fn faults_and_reset(d: &mut Chunk, s: &mut Sim<'_>) {
    let length = (d.capacities[0] * d.capacities[1]) as usize;
    let frames = d.capacities[1];
    d.reset(s, Some(&vec![5; length]));
    d.memory.borrow_mut().control.fail_next = true;
    assert_eq!(d.command(s, &op("read", 0, 0, 1, &[]), 10, true).0, 5);
    assert_ne!(s.model.get("fault"), 0);
    assert_eq!(s.model.get("cmd_ready"), 0);
    d.reset(s, Some(&vec![5; length]));
    d.memory.borrow_mut().control.pause = true;
    command(d, s, op("write", 0, 0, 0, &vec![8; frames as usize]), 11);
    {
        let mut m = d.memory.borrow_mut();
        m.control.fail_next = true;
        m.control.pause = false;
    }
    s.until(10_000, "posted write fault", |m| m.get("fault") != 0);
    assert_eq!(s.model.get("cmd_ready"), 0);
    d.reset(s, Some(&vec![9; length]));
    assert_eq!(command(d, s, op("read", 0, 0, 1, &[]), 12), vec![9]);
    s.model.set("arena_base", d.base + 4);
    assert_eq!(
        d.command(s, &op("read", 0, 0, 1, &[]), 13, true),
        (4, vec![])
    );
    s.model.set("arena_base", d.base);
    assert_eq!(command(d, s, op("read", 0, 0, 1, &[]), 14), vec![9]);
    d.reset(s, Some(&vec![9; length]));
    command(d, s, op("prefetch", 0, 0, 0, &[]), 15);
    for _ in 0..10_000 {
        d.tick(s);
        if d.memory.borrow().active().is_some() {
            break;
        }
    }
    assert!(d.memory.borrow().active().is_some());
    d.memory.borrow_mut().control.pause = true;
    d.reset(s, Some(&vec![11; length]));
    d.memory.borrow_mut().control.pause = false;
    assert_eq!(command(d, s, op("read", 0, 0, 1, &[]), 16), vec![11]);
}
fn read_span(
    d: &mut Chunk,
    s: &mut Sim<'_>,
    offset: u64,
    count: u64,
    wanted: &[i32],
    stall_at: Option<usize>,
) {
    s.model.set("read_ready", 0);
    start_at(d, s, 0, 123, 0, offset, count);
    s.model.set_all(&[
        ("cmd_offset", u32::MAX as u64),
        ("cmd_count", u32::MAX as u64),
    ]);
    let mut samples = Vec::new();
    let mut clocks = Vec::new();
    let mut held = crate::memory::Stable::default();
    let mut stalled = false;
    let mut hold: u64 = 0;
    let mut done = false;
    for _ in 0..10_000 {
        if stall_at == Some(samples.len()) && !stalled {
            stalled = true;
            hold = 7;
        }
        let ready = hold == 0;
        hold = hold.saturating_sub(1);
        s.model.set("read_ready", u64::from(ready));
        let b = d.tick(s).0;
        let payload = (b.get("read_data"), b.get("read_last"));
        held.check("resident read", b.get("read_valid") != 0, ready, payload);
        if b.get("read_valid") != 0 && ready {
            samples.push(payload.0 as i32);
            clocks.push(s.cycles);
            assert_eq!(payload.1 != 0, samples.len() as u64 == count);
        }
        if b.get("response_valid") != 0 {
            assert_eq!(b.get("read_valid"), 0);
            assert_eq!(samples.len() as u64, count);
            done = true;
            break;
        }
    }
    assert!(done);
    assert_eq!(samples, wanted);
    if stall_at.is_none() {
        assert!(
            clocks.windows(2).all(|w| w[1] - w[0] == 1),
            "resident read throughput"
        );
    }
    s.model.set("read_ready", 0);
    for _ in 0..5 {
        let b = d.tick(s).0;
        assert_ne!(b.get("response_valid"), 0);
        assert_eq!(b.get("response_id"), 123);
        assert_eq!(
            b.get("response_error") | b.get("read_valid") | b.get("cmd_ready"),
            0
        );
    }
    finish(d, s, 123);
}
fn read_stream_protocol(d: &mut Chunk, s: &mut Sim<'_>) {
    let frames = d.capacities[1];
    let initial: Vec<_> = (0..d.capacities[0] * frames)
        .map(|i| 0x81234567u32.wrapping_add(0x1020305u32.wrapping_mul(i as u32)) as i32)
        .collect();
    d.memory.borrow_mut().control.stalls = false;
    for forwarded in [false, true] {
        d.memory.borrow_mut().control.pause = false;
        d.reset(s, Some(&initial));
        let mut wanted = initial[..frames as usize].to_vec();
        if forwarded {
            d.memory.borrow_mut().control.pause = true;
            wanted.reverse();
            command(d, s, op("write", 0, 0, 0, &wanted), 0);
        } else {
            assert_eq!(command(d, s, op("read", 0, 0, frames, &[]), 0), wanted);
            d.memory.borrow_mut().control.pause = true;
        }
        for (offset, count) in [(0, 0), (0, 1), (0, frames), (1, frames - 1)]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
        {
            read_span(
                d,
                s,
                offset,
                count,
                &wanted[offset as usize..(offset + count) as usize],
                None,
            );
        }
        for stall in 0..frames as usize {
            read_span(d, s, 0, frames, &wanted, Some(stall));
        }
        d.memory.borrow_mut().control.pause = false;
        command(d, s, op("fence", 0, 0, 0, &[]), 0);
    }
    for delay in 0..(frames + 6).min(10) {
        d.reset(s, Some(&initial));
        assert_eq!(
            command(d, s, op("read", 0, 0, frames, &[]), 0),
            initial[..frames as usize]
        );
        s.model.set("read_ready", delay % 2);
        start(d, s, 0, 456, 0, frames);
        for _ in 0..delay {
            d.tick(s);
        }
        let replacement: Vec<_> = initial
            .iter()
            .map(|&v| (v as u32).wrapping_add(delay as u32 + 91) as i32)
            .collect();
        d.reset(s, Some(&replacement));
        assert_eq!(s.model.get("read_valid") | s.model.get("response_valid"), 0);
        assert_eq!(
            command(d, s, op("read", 0, 0, frames, &[]), 0),
            replacement[..frames as usize]
        );
    }
}
fn block_retirement(d: &mut Chunk, s: &mut Sim<'_>, bc: u64) {
    let [slots, frames, _, _] = d.capacities;
    let source: Vec<_> = (0..slots * frames).map(|i| i as i32 * 19 - 1000).collect();
    d.reset(s, Some(&source));
    for chunk in [1, bc - 1] {
        assert_eq!(
            command(d, s, op("read", chunk, 0, frames, &[]), 0),
            source[(chunk * frames) as usize..((chunk + 1) * frames) as usize]
        );
    }
    for (chunk, value) in [(0, 101), (bc - 1, 131), (bc, 202)] {
        command(
            d,
            s,
            op("write", chunk, 0, 0, &vec![value; frames as usize]),
            0,
        );
    }
    d.memory.borrow_mut().control.pause = true;
    start(d, s, 4, 73, 0, 0);
    for _ in 0..20 {
        d.tick(s);
    }
    assert_eq!(
        s.model.get("response_valid"),
        0,
        "retirement published reuse before dirty data drained"
    );
    d.memory.borrow_mut().control.pause = false;
    s.until(100_000, "retirement drain", |m| {
        m.get("response_valid") != 0
    });
    for _ in 0..4 {
        assert_eq!(s.model.get("response_id"), 73);
        assert_eq!(s.model.get("response_error"), 0);
        assert_ne!(s.model.get("response_valid"), 0);
        d.tick(s);
    }
    finish(d, s, 73);
    command(d, s, op("fence", 0, 0, 0, &[]), 0);
    let mut image = d.image(s);
    assert_eq!(image[..frames as usize], vec![101; frames as usize]);
    assert_eq!(
        image[((bc - 1) * frames) as usize..(bc * frames) as usize],
        vec![131; frames as usize]
    );
    assert_eq!(
        image[(bc * frames) as usize..((bc + 1) * frames) as usize],
        vec![202; frames as usize]
    );
    for chunk in [1, bc - 1] {
        image[(chunk * frames) as usize..((chunk + 1) * frames) as usize].fill(9000 + chunk as i32);
    }
    d.load_image(s, &image);
    for chunk in [1, bc - 1] {
        assert_eq!(
            command(d, s, op("read", chunk, 0, frames, &[]), 0),
            vec![9000 + chunk as i32; frames as usize]
        );
    }
    assert_eq!(
        d.command(s, &op("retire", 1, 0, 0, &[]), 0, false),
        (4, vec![])
    );
    d.reset(s, Some(&source));
    d.memory.borrow_mut().control.pause = true;
    d.hint(s, 2);
    for _ in 0..5 {
        d.tick(s);
    }
    start(d, s, 4, 74, 0, 0);
    for _ in 0..10 {
        d.tick(s);
    }
    assert_eq!(s.model.get("response_valid"), 0);
    d.memory.borrow_mut().control.pause = false;
    finish(d, s, 74);
    let mut image = d.image(s);
    image[(2 * frames) as usize..(3 * frames) as usize].fill(7777);
    d.load_image(s, &image);
    assert_eq!(
        command(d, s, op("read", 2, 0, frames, &[]), 0),
        vec![7777; frames as usize]
    );
    d.reset(s, Some(&source));
    command(d, s, op("write", 0, 0, 0, &vec![42; frames as usize]), 0);
    d.memory.borrow_mut().control.fail_next = true;
    assert_eq!(
        d.command(s, &op("retire", 0, 0, 0, &[]), 0, false),
        (5, vec![])
    );
    assert_ne!(s.model.get("fault"), 0);
    d.reset(s, Some(&source));
    assert_eq!(
        command(d, s, op("read", 0, 0, frames, &[]), 0),
        source[..frames as usize]
    );
}
fn background_start(d: &mut Chunk, s: &mut Sim<'_>, chunk: u64) {
    s.model
        .set_all(&[("retire_valid", 1), ("retire_chunk", chunk)]);
    for _ in 0..10_000 {
        s.model.eval();
        let ready = s.model.get("retire_ready");
        d.tick(s);
        if ready != 0 {
            s.model.set("retire_valid", 0);
            return;
        }
    }
    panic!("background retirement admission timeout");
}
fn background_done(d: &mut Chunk, s: &mut Sim<'_>) -> u64 {
    for _ in 0..100_000 {
        if s.model.get("retire_done") != 0 {
            return s.model.get("retire_error");
        }
        d.tick(s);
    }
    panic!("background retirement completion timeout");
}
fn background_retirement_preserves_unrelated_commands(d: &mut Chunk, s: &mut Sim<'_>, bc: u64) {
    let [slots, frames, _, _] = d.capacities;
    let source: Vec<_> = (0..slots * frames).map(|i| i as i32 * 7 - 1000).collect();
    d.reset(s, Some(&source));
    for (chunk, value) in [(0, 101), (bc, 202)] {
        command(
            d,
            s,
            op("write", chunk, 0, 0, &vec![value; frames as usize]),
            0,
        );
    }
    d.memory.borrow_mut().control.pause = true;
    background_start(d, s, 0);
    for _ in 0..20 {
        d.tick(s);
    }
    assert_ne!(s.model.get("retire_busy"), 0);
    assert_eq!(s.model.get("retire_done"), 0);
    assert_eq!(
        command(d, s, op("read", bc, 0, frames, &[]), 0),
        vec![202; frames as usize]
    );
    assert_ne!(
        s.model.get("retire_busy"),
        0,
        "unrelated command waited for retirement"
    );
    d.memory.borrow_mut().control.pause = false;
    assert_eq!(background_done(d, s), 0);
    command(d, s, op("fence", 0, 0, 0, &[]), 0);
    let mut image = d.image(s);
    image[..frames as usize].fill(7777);
    d.load_image(s, &image);
    assert_eq!(
        command(d, s, op("read", 0, 0, frames, &[]), 0),
        vec![7777; frames as usize]
    );
    d.reset(s, Some(&source));
    command(d, s, op("write", 0, 0, 0, &vec![42; frames as usize]), 0);
    d.memory.borrow_mut().control.fail_next = true;
    background_start(d, s, 0);
    assert_eq!(background_done(d, s), 5);
    assert_ne!(s.model.get("fault"), 0);
    d.reset(s, Some(&source));
    assert_eq!(
        command(d, s, op("read", 0, 0, frames, &[]), 0),
        source[..frames as usize]
    );
}
