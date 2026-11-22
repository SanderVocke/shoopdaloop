//! Metadata indexing, detached radix roots, and coherent shared cache transport.
use crate::{
    Result,
    clock::Edges,
    drivers::request::Request,
    memory::{Memory, Mode, Shared},
    model::{Design, Model, Probe},
    random::Random,
    sim::{Agent, Sim},
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
fn design(top: &str, width: u64, words: u64, large: u64) -> Design {
    let root = crate::root();
    let mut sources: Vec<_> = "axi_master chunk_dma word_dma axi_burst_mux shared_axi"
        .split_whitespace()
        .map(|n| root.join(format!("fpga/axi_memory/rtl/{n}.sv")))
        .collect();
    sources.push(root.join("fpga/chunk_memory/rtl/chunk_ram.sv"));
    sources.extend(
        "media_ram metadata_cache media_metadata"
            .split_whitespace()
            .map(|n| root.join(format!("fpga/media_store/rtl/{n}.sv"))),
    );
    sources.push(root.join("fpga/axi_memory/tests/axi_direct_memory.sv"));
    sources.push(root.join(format!("fpga/media_store/tests/{top}.sv")));
    let mut parameters = vec![("DATA_WIDTH", width), ("LINE_WORDS", words)];
    if top == "media_metadata_tb" {
        parameters.push(("LARGE", large));
    }
    Design::new(top, sources)
        .parameters(&parameters)
        .administration_at(
            if large == 2 { 3 * 5760000 + 1024 } else { 8192 },
            "dut.ram",
        )
}
struct Metadata {
    port: Request,
    memory: Shared,
    slots: u64,
}
impl Metadata {
    fn reset(&self, s: &mut Sim<'_>) {
        self.port.set(
            s,
            &[
                ("request_valid", 0),
                ("request_opcode", 0),
                ("request_view", 0),
                ("request_index", 0),
                ("request_data", 0),
                ("response_ready", 0),
            ],
        );
        s.model.set("reset", 1);
        s.idle(1);
        s.model.set("reset", 0);
        s.idle(1);
    }
    fn request(
        &self,
        s: &mut Sim<'_>,
        op: u64,
        view: u64,
        index: u64,
        data: u64,
        error: u64,
    ) -> u64 {
        self.port
            .transaction(
                s,
                &[
                    ("request_opcode", op),
                    ("request_view", view),
                    ("request_index", index),
                    ("request_data", data),
                ],
                error,
                0,
            )
            .get(0, 32)
    }
}
fn indexed_metadata_reuse_and_limits(d: &Metadata, s: &mut Sim<'_>) {
    let slots = d.slots;
    let levels = (64 - (slots - 1).leading_zeros()).div_ceil(4);
    let r =
        |s: &mut Sim<'_>, op, view, index, data, error| d.request(s, op, view, index, data, error);
    r(s, 7, 0, 0, 0, 0);
    r(s, 0, 0, 0, 0, 1);
    r(s, 1, 0, slots, 123, 1);
    r(s, 1, 0, 1, 123, 4);
    assert_eq!(s.model.get("metadata.high_water"), 0);
    for i in 0..32 {
        r(s, 1, 0, i, 100 + i, 0);
    }
    for i in 0..16 {
        r(s, 1, 1, i, 200 + i, 0);
    }
    let high = s.model.get("metadata.high_water");
    r(s, 7, 2, slots, 0, 3);
    r(s, 1, 2, slots - 1, 123, 3);
    assert_eq!(s.model.get("metadata.high_water"), high);
    for (view, count) in [(0, 32), (1, 16)] {
        for i in (0..count).rev() {
            assert_eq!(r(s, 0, view, i, 0, 0), 100 * (view + 1) + i);
        }
    }
    for i in 0..70 {
        r(s, 3, 0, i, i + 1, 0);
        r(s, 5, 0, i, 69 - i, 0);
    }
    for i in (0..70).rev() {
        assert_eq!(r(s, 2, 0, i, 0, 0), i + 1);
        assert_eq!(r(s, 4, 0, i, 0, 0), 69 - i);
    }
    r(s, 6, 0, 17, 0, 0);
    r(s, 0, 0, 17, 0, 1);
    r(s, 6, 0, 18, 0, 4);
    assert_eq!(s.model.get("metadata.free_pages"), 0);
    r(s, 6, 0, 16, 0, 0);
    assert_eq!(s.model.get("metadata.free_pages"), 1);
    r(s, 6, 0, 0, 0, 0);
    assert_eq!(s.model.get("metadata.free_pages"), u64::from(levels + 1));
    for i in 0..32 {
        r(s, 1, 2, i, 300 + i, 0);
    }
    assert_eq!(s.model.get("metadata.high_water"), high);
    for i in 0..32 {
        assert_eq!(r(s, 0, 2, i, 0, 0), 300 + i);
    }
    if slots > 256 {
        for i in 0..272 {
            r(s, 1, 3, i, 400 + i, 0);
        }
        for i in [0, 15, 16, 255, 256, 271] {
            assert_eq!(r(s, 0, 3, i, 0, 0), 400 + i);
        }
        let before = s.model.get("metadata.free_pages");
        let high = s.model.get("metadata.high_water");
        r(s, 6, 3, 256, 0, 0);
        assert_eq!(s.model.get("metadata.free_pages"), before + 2);
        assert_eq!(r(s, 0, 3, 255, 0, 0), 655);
        r(s, 6, 3, 240, 0, 0);
        assert_eq!(s.model.get("metadata.free_pages"), before + 3);
        r(s, 6, 3, 0, 0, 0);
        for i in 0..16 {
            r(s, 1, 3, i, 700 + i, 0);
        }
        assert_eq!(s.model.get("metadata.high_water"), high);
    }
    r(s, 9, 0, 0, 0, 0);
    let image = Memory::read(&d.memory, s);
    assert_eq!(
        &image[slots as usize..slots as usize + 70],
        &(1..71).collect::<Vec<u32>>()
    );
    assert_eq!(
        &image[(2 * slots) as usize..(2 * slots) as usize + 70],
        &(0..70).rev().collect::<Vec<u32>>()
    );
    d.memory.borrow_mut().control.fail_next = true;
    r(s, 0, 2, 0, 0, 5);
    assert_ne!(s.model.get("fault"), 0);
    for op in [2, 4, 3, 5] {
        d.reset(s);
        if op == 3 || op == 5 {
            r(s, op, 0, 0, 123, 0);
            d.memory.borrow_mut().control.fail_next = true;
            r(s, 8, 0, 0, 0, 5);
        } else {
            d.memory.borrow_mut().control.fail_next = true;
            r(s, op, 0, 0, 0, 5);
        }
        assert_ne!(s.model.get("fault"), 0);
    }
}
fn detached_tables_survive_reuse_and_recover_pages(d: &Metadata, s: &mut Sim<'_>) {
    let r =
        |s: &mut Sim<'_>, op, view, index, data, error| d.request(s, op, view, index, data, error);
    r(s, 12, 0, 0, 0, 2);
    r(s, 11, 0, 0, 0, 2);
    r(s, 10, 4, 0, 0, 1);
    r(s, 10, 0, 0, 0, 0);
    assert_eq!(s.model.get("retired_count"), 0);
    for (view, count, base) in [(0, 32, 100), (1, 16, 200)] {
        for i in 0..count {
            r(s, 1, view, i, base + i, 0);
            r(s, 3, 0, base + i, 1, 0);
        }
    }
    let high = r(s, 14, 0, 0, 0, 0);
    let free = r(s, 14, 0, 1, 0, 0);
    let walks = (
        s.model.get("metadata.directory_reads"),
        s.model.get("metadata.table_reads"),
    );
    assert_eq!(r(s, 10, 0, 0, 0, 0), 32);
    assert_eq!(r(s, 10, 1, 0, 0, 0), 16);
    assert_eq!(
        (
            s.model.get("metadata.directory_reads"),
            s.model.get("metadata.table_reads")
        ),
        walks
    );
    assert_eq!(s.model.get("retired_count"), 2);
    assert_eq!(s.model.get("retired_references"), 48);
    assert_eq!(r(s, 14, 0, 0, 0, 0), high);
    assert_eq!(r(s, 14, 0, 1, 0, 0), free);
    for i in 0..32 {
        assert_eq!(r(s, 2, 0, 100 + i, 0, 0), 1);
    }
    r(s, 0, 0, 0, 0, 1);
    r(s, 1, 0, 0, 250, 0);
    r(s, 10, 0, 0, 0, 3);
    assert_eq!(r(s, 0, 0, 0, 0, 0), 250);
    for (_view, count, base) in [(0, 32, 100), (1, 16, 200)] {
        for i in (0..count).rev() {
            assert_eq!(r(s, 11, 999, 0, 0, 0), base + i);
            r(s, 11, 0, 0, 0, 2);
            assert_eq!(r(s, 0, 0, 0, 0, 0), 250);
            assert_eq!(r(s, 12, 0, 0, 0, 0), i);
            r(s, 12, 0, 0, 0, 2);
        }
    }
    assert_eq!(s.model.get("retired_count"), 0);
    assert_eq!(s.model.get("retired_references"), 0);
    r(s, 10, 0, 0, 0, 0);
    assert_eq!(r(s, 11, 0, 0, 0, 0), 250);
    r(s, 12, 0, 0, 0, 0);
    assert_eq!(r(s, 14, 0, 1, 0, 0), r(s, 14, 0, 0, 0, 0));
    for _ in 0..6 {
        r(s, 1, 0, 0, 17, 0);
        r(s, 10, 0, 0, 0, 0);
        assert_eq!(r(s, 11, 0, 0, 0, 0), 17);
        r(s, 12, 0, 0, 0, 0);
        assert_eq!(s.model.get("retired_count"), 0);
    }
    assert_eq!(s.model.get("retired_high_water"), 2);
    assert_eq!(r(s, 14, 0, 1, 0, 0), r(s, 14, 0, 0, 0, 0));
}
pub fn tables(width: u64, large: u64) -> Result<()> {
    let design = design("media_metadata_tb", width, 16, large).probes(
        [
            "metadata.high_water",
            "metadata.free_pages",
            "metadata.directory_reads",
            "metadata.table_reads",
        ]
        .iter()
        .map(|n| Probe::observe(n))
        .collect(),
    );
    let compiled = design.build(&crate::root().join("work/marlin-suite/metadata"))?;
    let slots = match large {
        0 => 256,
        1 => 512,
        2 => 5760000,
        _ => panic!("invalid metadata profile"),
    };
    let words = if large == 2 { 3 * slots + 1024 } else { 8192 };
    for test in [
        indexed_metadata_reuse_and_limits,
        detached_tables_survive_reuse_and_recover_pages,
    ] {
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        let memory = Memory::attach(
            &mut s,
            Mode::Instant,
            words as usize,
            0x01000000,
            &crate::artifact_directory("metadata"),
            7654,
        )?;
        let d = Metadata {
            port: Request::new(&mut s, ""),
            memory,
            slots,
        };
        d.reset(&mut s);
        Memory::fill(&d.memory, &mut s, 0);
        test(&d, &mut s);
    }
    println!("metadata/AXI{width}/large={large}: passed");
    Ok(())
}

struct CacheOp {
    op: u64,
    address: u64,
    data: u64,
    expected: Option<u64>,
}
struct CacheStream {
    port: Request,
    queue: VecDeque<CacheOp>,
    phase: u64,
    hold: u64,
    done: bool,
}
impl Agent for CacheStream {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if !e.rises(0) || self.done {
            return;
        }
        let prefix = if self.port.get_prefix() == "c0_" {
            "c0_"
        } else {
            "c1_"
        };
        let set = |m: &mut Model<'_>, n: &str, v| m.set(&format!("{prefix}{n}"), v);
        if let Some(op) = self.queue.front() {
            set(m, "request_valid", u64::from(self.phase == 0));
            set(m, "request_opcode", op.op);
            set(m, "request_address", op.address);
            set(m, "request_data", op.data);
            set(m, "request_invalidate", 0);
            set(m, "response_ready", u64::from(self.phase == 2));
        } else {
            set(m, "request_valid", 0);
            set(m, "response_ready", 0);
            self.done = true;
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if !e.rises(0) || self.done {
            return;
        }
        let prefix = self.port.get_prefix();
        let get = |n| m.get(&format!("{prefix}{n}"));
        if self.phase == 0 && get("request_ready") != 0 {
            self.phase = 1;
        } else if self.phase == 1 && get("response_valid") != 0 {
            assert_eq!(get("response_error"), 0);
            if let Some(value) = self.queue.front().unwrap().expected {
                assert_eq!(get("response_data"), value);
            }
            self.hold += 1;
            if self.hold >= 4 {
                self.phase = 2;
            }
        } else if self.phase == 2 && get("response_valid") != 0 {
            self.queue.pop_front();
            self.phase = 0;
            self.hold = 0;
        }
    }
}
struct Stalls {
    memory: Shared,
    cycles: u64,
}
impl Agent for Stalls {
    fn drive(&mut self, _: &mut Model<'_>, e: Edges) {
        if e.rises(0) && self.cycles <= 88 {
            let mut m = self.memory.borrow_mut();
            let index = self.cycles / 22;
            let hold = self.cycles % 22 < 15;
            let c = &mut m.control;
            c.hold_ar = index == 0 && hold;
            c.hold_aw = index == 1 && hold;
            c.hold_w = index == 2 && hold;
            c.pause_responses = index == 3 && hold;
            self.cycles += 1;
        }
    }
}
pub fn coherent_cache_and_transport(width: u64, words: u64) -> Result<()> {
    coherent_cache_lines(width, words, 2)
}
pub fn coherent_cache_lines(width: u64, words: u64, lines: u64) -> Result<()> {
    let mut design = design("metadata_cache_tb", width, words, 0);
    design
        .parameters
        .insert("CACHE_LINES".into(), lines.to_string());
    let compiled = design.build(&crate::root().join("work/marlin-suite/metadata"))?;
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    let memory = Memory::attach(
        &mut s,
        Mode::Instant,
        8192,
        0x01000000,
        &crate::artifact_directory("cache"),
        7654,
    )?;
    let ports = [Request::new(&mut s, "c0_"), Request::new(&mut s, "c1_")];
    s.model.set("reset", 1);
    s.idle(1);
    s.model.set("reset", 0);
    s.idle(1);
    let mut rng = Random::new(7654);
    let mut image: Vec<_> = (0..8192).map(|_| rng.next() as u32).collect();
    Memory::load(&memory, &mut s, &image);
    let base = 0x01000000;
    let mut streams = Vec::new();
    for (client, port) in ports.into_iter().enumerate() {
        let mut queue = VecDeque::new();
        for row in 0..8 {
            let index = client * 2048 + row * words as usize;
            for j in [0, words as usize - 1] {
                queue.push_back(CacheOp {
                    op: 0,
                    address: base + 4 * (index + j) as u64,
                    data: 0,
                    expected: Some(u64::from(image[index + j])),
                });
            }
            let value = rng.next() as u32;
            queue.push_back(CacheOp {
                op: 1,
                address: base + 4 * index as u64,
                data: u64::from(value),
                expected: None,
            });
            image[index] = value;
            queue.push_back(CacheOp {
                op: 0,
                address: base + 4 * index as u64,
                data: 0,
                expected: Some(u64::from(value)),
            });
        }
        queue.push_back(CacheOp {
            op: 3,
            address: 0,
            data: 0,
            expected: None,
        });
        let stream = Rc::new(RefCell::new(CacheStream {
            port,
            queue,
            phase: 0,
            hold: 0,
            done: false,
        }));
        s.agent(stream.clone());
        streams.push(stream);
    }
    s.agent(Stalls {
        memory: memory.clone(),
        cycles: 0,
    });
    s.until(200_000, "two concurrent cache streams", |_| {
        streams.iter().all(|c| c.borrow().done)
    });
    assert_eq!(Memory::read(&memory, &mut s), image);
    let request = |s: &mut Sim<'_>, client: usize, op, address, data, invalidate, error| {
        let port = streams[client].borrow().port.clone();
        port.transaction(
            s,
            &[
                ("request_opcode", op),
                ("request_address", address),
                ("request_data", data),
                ("request_invalidate", invalidate),
            ],
            error,
            3,
        )
        .get(0, 32)
    };
    let fills = s.model.get("c0_fills");
    for _ in 0..5 {
        assert_eq!(
            request(&mut s, 0, 0, base + 4 * 7 * words, 0, 0, 0),
            u64::from(image[(7 * words) as usize])
        );
    }
    assert_eq!(s.model.get("c0_fills"), fills);
    request(&mut s, 0, 2, base, 0, 0, 0);
    image[..words as usize].fill(0);
    request(&mut s, 0, 3, 0, 0, 1, 0);
    assert_eq!(Memory::read(&memory, &mut s), image);
    memory.borrow_mut().control.fail_next = true;
    request(&mut s, 0, 0, base, 0, 0, 1);
    assert_ne!(s.model.get("c0_fault"), 0);
    s.model.set("reset", 1);
    s.idle(1);
    s.model.set("reset", 0);
    s.idle(1);
    assert_eq!(s.model.get("c0_fault"), 0);
    assert_eq!(request(&mut s, 0, 0, base, 0, 0, 0), 0);
    assert_eq!(Memory::read(&memory, &mut s), image);
    memory.borrow_mut().control.pause_responses = true;
    s.model.set_all(&[
        ("c1_request_valid", 1),
        ("c1_request_opcode", 0),
        ("c1_request_address", base + 4 * 3000),
    ]);
    s.until(1000, "cache reset in flight", |m| m.get("sim_busy") != 0);
    s.model.set_all(&[("c1_request_valid", 0), ("reset", 1)]);
    s.idle(1);
    s.model.set("reset", 0);
    memory.borrow_mut().control.pause_responses = false;
    s.idle(5);
    assert_eq!(s.model.get("sim_busy"), 0);
    assert_eq!(
        request(&mut s, 1, 0, base + 4 * 3000, 0, 0, 0),
        u64::from(image[3000])
    );
    request(&mut s, 1, 1, base + 4 * 3000, 123, 0, 0);
    memory.borrow_mut().control.fail_next = true;
    request(&mut s, 1, 3, 0, 0, 0, 1);
    assert_ne!(s.model.get("c1_fault"), 0);
    Ok(())
}

pub fn cache_area_workloads(width: u64) -> Result<serde_json::Value> {
    use serde_json::json;
    let mut design = design("metadata_cache_tb", width, 16, 0);
    design.parameters.insert("CACHE_LINES".into(), "128".into());
    let compiled = design.build(&crate::root().join("work/marlin-suite/metadata-area"))?;
    let mut s = Sim::new(compiled.benchmark_session()?, &[("clk", 10_000, 0)]);
    let memory = Memory::attach(
        &mut s,
        Mode::Instant,
        8192,
        0x01000000,
        &crate::artifact_directory("metadata-cache-area"),
        7654,
    )?;
    memory.borrow_mut().control.stalls = false;
    let port = Request::new(&mut s, "c0_");
    s.model.set("reset", 1);
    s.idle(1);
    s.model.set("reset", 0);
    s.idle(1);
    let mut rng = Random::new(7654);
    let mut image: Vec<_> = (0..8192).map(|_| rng.next() as u32).collect();
    Memory::load(&memory, &mut s, &image);
    let counters =
        |s: &Sim<'_>| ["hits", "misses", "fills", "writebacks"].map(|name| port.get(s, name));
    let mut measurements = Vec::new();
    for (kind, order) in [
        ("sequential", (0..128).collect::<Vec<_>>()),
        ("strided", (0..128).map(|i| i * 17 % 128).collect()),
        ("set_conflict", (0..5).map(|i| i * 32).collect()),
    ] {
        port.transaction(
            &mut s,
            &[("request_opcode", 3), ("request_invalidate", 1)],
            0,
            0,
        );
        for pass in 0..3 {
            for &line in &order {
                let word = line * 16 + (pass * 7 % 16);
                let write = kind == "set_conflict" && pass != 1;
                let value = rng.next() as u32;
                let before = counters(&s);
                let start = s.cycles;
                let actual = port
                    .transaction(
                        &mut s,
                        &[
                            ("request_opcode", u64::from(write)),
                            ("request_invalidate", 0),
                            ("request_address", 0x01000000 + word as u64 * 4),
                            ("request_data", u64::from(value)),
                        ],
                        0,
                        0,
                    )
                    .get(0, 32);
                let cycles = s.cycles - start;
                let after = counters(&s);
                if write {
                    image[word] = value;
                } else {
                    assert_eq!(actual, u64::from(image[word]));
                }
                measurements.push(json!({
                    "kind":kind,"pass":pass,"line":line,"word":word,"write":write,
                    "cycles":cycles,"hits":after[0]-before[0],"misses":after[1]-before[1],
                    "fills":after[2]-before[2],"writebacks":after[3]-before[3]
                }));
            }
        }
        let start = s.cycles;
        port.transaction(
            &mut s,
            &[("request_opcode", 3), ("request_invalidate", 1)],
            0,
            0,
        );
        measurements.push(json!({"kind":format!("{kind}_fence"),"cycles":s.cycles-start}));
        assert_eq!(Memory::read(&memory, &mut s), image);
    }
    Ok(json!({
        "schema_version":1,"scope":"cache plus shared AXI and direct-memory timing model; request assertion through response acceptance; hold=0; preparation/readback excluded",
        "cache_lines":128,"line_words":16,"axi_data_width":width,"seed":7654,
        "validation":"read values and final fenced memory match an independent word array; protocol agents check held responses",
        "measurements":measurements,"provenance":crate::report::provenance()
    }))
}

pub fn translation_area_workloads(width: u64) -> Result<serde_json::Value> {
    use serde_json::json;
    let mut design = design("media_metadata_tb", width, 16, 1);
    design.parameters.insert("TABLE_PAGES".into(), "128".into());
    design.parameters.insert("TABLE_LINES".into(), "128".into());
    for name in ["directory_reads", "table_reads"] {
        design
            .probes
            .push(Probe::observe(&format!("metadata.{name}")));
    }
    for name in ["hits", "misses", "fills", "writebacks"] {
        design
            .probes
            .push(Probe::observe(&format!("metadata.banks[0].cache.{name}")));
    }
    let compiled = design.build(&crate::root().join("work/marlin-suite/translation-area"))?;
    let mut s = Sim::new(compiled.benchmark_session()?, &[("clk", 10_000, 0)]);
    let memory = Memory::attach(
        &mut s,
        Mode::Instant,
        8192,
        0x01000000,
        &crate::artifact_directory("metadata-translation-area"),
        91833,
    )?;
    memory.borrow_mut().control.stalls = false;
    let d = Metadata {
        port: Request::new(&mut s, ""),
        memory,
        slots: 512,
    };
    d.reset(&mut s);
    let mut expected = vec![vec![0_u64; 272]; 4];
    for (view, values) in expected.iter_mut().enumerate() {
        for (ordinal, value) in values.iter_mut().enumerate() {
            *value = 1 + view as u64 * 4096 + ordinal as u64;
            d.request(&mut s, 1, view as u64, ordinal as u64, *value, 0);
        }
    }
    let counters = |s: &Sim<'_>| {
        ["hits", "misses", "fills", "writebacks"]
            .map(|name| s.model.get(&format!("metadata.banks[0].cache.{name}")))
    };
    let mut measurements = Vec::new();
    for (kind, order) in [
        ("repeated_block", vec![0_usize; 32]),
        ("sequential", (0..272).collect::<Vec<_>>()),
        ("strided", (0..272).map(|i| i * 19 % 272).collect()),
        (
            "leaf_boundaries",
            vec![0, 15, 16, 255, 256, 271, 256, 255, 16, 15, 0],
        ),
    ] {
        d.request(&mut s, 9, 0, 0, 0, 0);
        for (step, &ordinal) in order.iter().enumerate() {
            for (view, values) in expected.iter().enumerate() {
                let before = counters(&s);
                let start = s.cycles;
                assert_eq!(
                    d.request(&mut s, 0, view as u64, ordinal as u64, 0, 0),
                    values[ordinal]
                );
                let elapsed = s.cycles - start;
                let after = counters(&s);
                measurements.push(
                    json!({"kind":kind,"step":step,"view":view,"ordinal":ordinal,
                    "cycles":elapsed,"hits":after[0]-before[0],"misses":after[1]-before[1],
                    "fills":after[2]-before[2],"writebacks":after[3]-before[3]}),
                );
            }
        }
    }
    // Keep a logical map independent of cache layout through mutations and reuse.
    for (view, values) in expected.iter_mut().enumerate() {
        for ordinal in [0, 7, 15, 16, 255, 256, 271, 0] {
            values[ordinal] += 65536;
            d.request(&mut s, 1, view as u64, ordinal as u64, values[ordinal], 0);
            for i in [0, 7, 15, 16, 255, 256, 271] {
                assert_eq!(d.request(&mut s, 0, view as u64, i as u64, 0, 0), values[i]);
            }
        }
        d.request(&mut s, 6, view as u64, 257, 0, 0);
        values.truncate(257);
        assert_eq!(d.request(&mut s, 0, view as u64, 256, 0, 0), values[256]);
        d.request(&mut s, 0, view as u64, 257, 0, 1);
        values.push(0x12340000 + view as u64);
        d.request(&mut s, 1, view as u64, 257, values[257], 0);
        assert_eq!(d.request(&mut s, 0, view as u64, 257, 0, 0), values[257]);
    }
    d.request(&mut s, 6, 0, 0, 0, 0);
    d.request(&mut s, 0, 0, 0, 0, 1);
    d.request(&mut s, 1, 0, 0, 0xaa, 0);
    assert_eq!(d.request(&mut s, 10, 0, 0, 0, 0), 1);
    d.request(&mut s, 1, 0, 0, 0xbb, 0);
    assert_eq!(d.request(&mut s, 11, 0, 0, 0, 0), 0xaa);
    assert_eq!(d.request(&mut s, 12, 0, 0, 0, 0), 0);
    assert_eq!(d.request(&mut s, 0, 0, 0, 0, 0), 0xbb);
    d.request(&mut s, 9, 0, 0, 0, 0);
    assert_eq!(d.request(&mut s, 0, 0, 0, 0, 0), 0xbb);
    d.reset(&mut s);
    d.request(&mut s, 0, 0, 0, 0, 1);
    d.request(&mut s, 1, 0, 0, 0xcc, 0);
    assert_eq!(d.request(&mut s, 0, 0, 0, 0, 0), 0xcc);
    Ok(
        json!({"schema_version":1,"axi_data_width":width,"seed":91833,
        "scope":"metadata controller plus coherent cache, AXI and direct-memory model; driver request assertion through response acceptance and cleanup; preparation excluded; four views with 272 entries",
        "validation":"reads, repeated writes, trim/regrow, global invalidation, detached GC with live view reuse, and reset agree with an independent logical map",
        "measurements":measurements,"provenance":crate::report::provenance()}),
    )
}

pub fn width_boundary_workloads(width: u64) -> Result<serde_json::Value> {
    use serde_json::json;
    let mut results = Vec::new();
    for (views, slots, pages, retired) in [(1, 1, 4, 1), (3, 17, 8, 3), (4, 180096, 64, 2)] {
        let words = (3 * slots + (pages * 16 + 96).max(1024)).max(8192);
        let mut design =
            design("media_metadata_tb", width, 16, 0).administration_at(words, "dut.ram");
        for (key, value) in [
            ("VIEWS", views),
            ("SLOTS", slots),
            ("TABLE_PAGES", pages),
            ("RETIRED_TABLES", retired),
        ] {
            design.parameters.insert(key.into(), value.to_string());
        }
        let compiled =
            design.build(&crate::root().join("work/marlin-suite/metadata-width-boundaries"))?;
        let mut s = Sim::new(compiled.benchmark_session()?, &[("clk", 10_000, 0)]);
        let memory = Memory::attach(
            &mut s,
            Mode::Instant,
            words,
            0x01000000,
            &crate::artifact_directory("metadata-width-boundaries"),
            20261122,
        )?;
        let d = Metadata {
            port: Request::new(&mut s, ""),
            memory,
            slots: slots as u64,
        };
        d.memory.borrow_mut().control.stalls = false;
        d.reset(&mut s);
        let count = if slots <= 17 { slots } else { 1 };
        for index in 0..count {
            d.request(&mut s, 1, 0, index as u64, 0x80000000 + index as u64, 0);
        }
        let mut rows = Vec::new();
        let mut request = |s: &mut Sim<'_>, op, view, index, data, error| {
            let start = s.cycles;
            let value = d.request(s, op, view, index, data, error);
            rows.push(json!({"opcode":op,"view":view,"index":index,"data":data,"error":error,"value":value,"cycles":s.cycles-start}));
            value
        };
        request(&mut s, 1, views as u64 - 1, 0, 0x80000000, 0);
        assert_eq!(request(&mut s, 0, views as u64 - 1, 0, 0, 0), 0x80000000);
        for view in [views as u64, 0x80000000, u64::from(u32::MAX)] {
            for op in [0, 1, 6, 7, 10] {
                request(&mut s, op, view, 0, 123, 1);
            }
        }
        for index in [slots as u64, 0x80000000, u64::from(u32::MAX)] {
            for op in [0, 1, 2, 3, 4, 5] {
                request(&mut s, op, 0, index, 123, 1);
            }
        }
        for index in [0x80000000, u64::from(u32::MAX)] {
            for op in [6, 7, 13, 14] {
                request(&mut s, op, 0, index, 0, 1);
            }
        }
        let mut nodes = 0;
        let mut level_count = slots;
        loop {
            level_count = level_count.div_ceil(16);
            nodes += level_count;
            if level_count == 1 {
                break;
            }
        }
        request(
            &mut s,
            7,
            0,
            slots as u64,
            0,
            if nodes > pages { 3 } else { 0 },
        );
        request(&mut s, 13, 0, (retired * 16) as u64, 0, 1);
        request(&mut s, 14, 0, 4, 0, 1);
        for op in [8, 9] {
            request(&mut s, op, u64::from(u32::MAX), u64::from(u32::MAX), 0, 0);
        }
        request(&mut s, 15, u64::from(u32::MAX), u64::from(u32::MAX), 0, 2);
        for index in 0..count {
            assert_eq!(
                request(&mut s, 0, 0, index as u64, 0, 0),
                0x80000000 + index as u64
            );
        }
        assert_eq!(request(&mut s, 10, 0, 0, 0, 0), count as u64);
        request(&mut s, 9, 0, 0, 0, 0);
        let queue_word = 3 * slots.div_ceil(16) * 16 + pages * 16 + 1;
        assert_eq!(s.model.get("sim_busy"), 0);
        s.model.set_all(&[
            ("sim_host_op", 1),
            ("sim_host_address", queue_word as u64),
            ("sim_host_write_data", u64::from(u32::MAX)),
        ]);
        s.idle(1);
        s.model.set("sim_host_op", 0);
        request(&mut s, 11, u64::from(u32::MAX), 0, 0, 2);
        request(&mut s, 12, u64::from(u32::MAX), 0, 0, 2);
        d.reset(&mut s);
        request(&mut s, 0, 0, 0, 0, 1);
        request(&mut s, 1, 0, 0, u64::from(u32::MAX), 0);
        assert_eq!(request(&mut s, 0, 0, 0, 0, 0), u64::from(u32::MAX));
        results.push(json!({"views":views,"slots":slots,"pages":pages,"retirements":retired,"measurements":rows}));
    }
    Ok(
        json!({"axi_data_width":width,"scope":"minimum, odd and primary-sized metadata counts; wide invalid input/error priority, opaque values, malformed retired DDR count and stale-RAM reset","validation":"all public responses match independent extent/capacity/opaque-value checks; full small pools and highest view IDs retained; malformed raw32 retirement count rejected before traversal","cases":results}),
    )
}

pub fn directory_area_workloads(width: u64) -> Result<serde_json::Value> {
    use serde_json::json;
    use std::collections::BTreeMap;
    const SLOTS: usize = 180096;
    const PAGES: usize = 8192;
    const WORDS: usize = 3 * SLOTS + PAGES * 16 + 96;
    let mut design = design("media_metadata_tb", width, 16, 2).administration_at(WORDS, "dut.ram");
    for (name, value) in [
        ("SLOTS", SLOTS),
        ("TABLE_PAGES", PAGES),
        ("TABLE_LINES", 128),
    ] {
        design.parameters.insert(name.into(), value.to_string());
    }
    for name in ["hits", "misses", "fills", "writebacks"] {
        design
            .probes
            .push(Probe::observe(&format!("metadata.banks[0].cache.{name}")));
    }
    let compiled = design.build(&crate::root().join("work/marlin-suite/directory-area"))?;
    let mut s = Sim::new(compiled.benchmark_session()?, &[("clk", 10_000, 0)]);
    let memory = Memory::attach(
        &mut s,
        Mode::Instant,
        WORDS,
        0x01000000,
        &crate::artifact_directory("metadata-directory-area"),
        100789,
    )?;
    memory.borrow_mut().control.stalls = false;
    let d = Metadata {
        port: Request::new(&mut s, ""),
        memory,
        slots: SLOTS as u64,
    };
    d.reset(&mut s);
    let counts = [65537_u64, 4097, 4097, 4097];
    let expected = |view: u64, index: u64| 1 + view * 4099 + index;
    let setup_start = s.cycles;
    let mut setup = Vec::new();
    for (view, &count) in counts.iter().enumerate() {
        let start = s.cycles;
        for index in 0..count {
            d.request(
                &mut s,
                1,
                view as u64,
                index,
                expected(view as u64, index),
                0,
            );
        }
        setup.push(json!({"view":view,"writes":count,"cycles":s.cycles-start}));
    }
    let setup_cycles = s.cycles - setup_start;
    let mut measurements = Vec::new();
    let counters = |s: &Sim<'_>| {
        ["hits", "misses", "fills", "writebacks"]
            .map(|name| s.model.get(&format!("metadata.banks[0].cache.{name}")))
    };
    let mut request = |s: &mut Sim<'_>, kind: &str, op, view, index, value, error| {
        let before = counters(s);
        let start = s.cycles;
        let actual = d.request(s, op, view, index, value, error);
        let after = counters(s);
        measurements.push(json!({"kind":kind,"opcode":op,"view":view,"index":index,
            "error":error,"value":actual,"cycles":s.cycles-start,
            "hits":after[0]-before[0],"misses":after[1]-before[1],
            "fills":after[2]-before[2],"writebacks":after[3]-before[3]}));
        actual
    };
    let positions = vec![0, 1, 15, 16, 255, 256, 4095, 4096, 65535, 65536];
    for (kind, order) in [
        ("forward", positions.clone()),
        ("reverse", positions.into_iter().rev().collect()),
        (
            "alternating",
            [255, 256, 4095, 4096, 65535, 65536].repeat(4),
        ),
    ] {
        request(&mut s, "invalidate", 9, 0, 0, 0, 0);
        for index in order {
            for (view, &count) in counts.iter().enumerate() {
                let error = u64::from(index >= count);
                let actual = request(&mut s, kind, 0, view as u64, index, 0, error);
                if error == 0 {
                    assert_eq!(actual, expected(view as u64, index));
                }
            }
        }
    }
    let mut changed = BTreeMap::new();
    for index in [255, 256, 4095, 4096, 65535, 65536] {
        let value = 0xf0000000 + index;
        request(&mut s, "rewrite", 1, 0, index, value, 0);
        changed.insert(index, value);
        assert_eq!(request(&mut s, "rewrite_read", 0, 0, index, 0, 0), value);
    }
    request(&mut s, "trim", 6, 0, 65536, 0, 0);
    request(&mut s, "trim_extent", 0, 0, 65536, 0, 1);
    assert_eq!(
        request(&mut s, "trim_read", 0, 0, 65535, 0, 0),
        changed[&65535]
    );
    request(&mut s, "regrow", 1, 0, 65536, 0xfeedc0de, 0);
    assert_eq!(
        request(&mut s, "regrow_read", 0, 0, 65536, 0, 0),
        0xfeedc0de
    );
    assert_eq!(request(&mut s, "detach", 10, 0, 0, 0, 0), 65537);
    request(&mut s, "live_reuse", 1, 0, 0, 0x55aaa5a5, 0);
    assert_eq!(request(&mut s, "gc_read", 11, 0, 0, 0, 0), 0xfeedc0de);
    assert_eq!(request(&mut s, "gc_trim", 12, 0, 0, 0, 0), 65536);
    assert_eq!(request(&mut s, "live_read", 0, 0, 0, 0, 0), 0x55aaa5a5);
    assert_eq!(request(&mut s, "gc_read", 11, 0, 0, 0, 0), changed[&65535]);
    assert_eq!(request(&mut s, "gc_trim", 12, 0, 0, 0, 0), 65535);
    assert_eq!(request(&mut s, "live_read", 0, 0, 0, 0, 0), 0x55aaa5a5);
    d.reset(&mut s);
    request(&mut s, "reset_extent", 0, 0, 0, 0, 1);
    request(&mut s, "reset_write", 1, 0, 0, 0x99117733, 0);
    assert_eq!(request(&mut s, "reset_read", 0, 0, 0, 0, 0), 0x99117733);
    Ok(
        json!({"schema_version":1,"axi_data_width":width,"slots":SLOTS,"pages":PAGES,
        "scope":"whole metadata driver call including admission/response cleanup; preparation writes timed separately; direct-memory model, no randomized stalls",
        "validation":"independent logical values across competing views, 16/256/4096/65536 boundaries, high opaque rewrites, trim/regrow, detached deep-root GC with live-ID reuse and stale-RAM reset",
        "setup_cycles":setup_cycles,"setup":setup,"measurements":measurements,
        "provenance":crate::report::provenance()}),
    )
}
