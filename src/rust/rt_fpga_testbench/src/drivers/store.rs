//! Public media transactions and an independent audit of live/retired block ownership.
use super::config::Config;
use crate::{
    Result,
    bits::Bits,
    memory::{Memory, Mode, Shared},
    random::Random,
    sim::{Sim, Snapshot},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct Geometry {
    pub views: u64,
    pub slots: u64,
    pub chunk_frames: u64,
    pub block_chunks: u64,
    pub table_pages: u64,
    pub retired_tables: u64,
    pub reclaim_blocks: u64,
    pub reclaim_cycles: u64,
    pub reclaim_guard: u64,
    pub table_lines: u64,
    pub reference_lines: u64,
    pub stack_lines: u64,
    pub read_buffers: u64,
    pub write_buffers: u64,
    pub reuse_views: bool,
}
impl Geometry {
    pub fn new(c: [u64; 3], block_chunks: u64) -> Self {
        assert!(block_chunks.is_power_of_two() && c[1] % block_chunks == 0);
        Self {
            views: c[0],
            slots: c[1],
            chunk_frames: c[2],
            block_chunks,
            table_pages: (c[1] / block_chunks).div_ceil(15) + 8 * c[0] + 16,
            retired_tables: 256,
            reclaim_blocks: 0,
            reclaim_cycles: 5000,
            reclaim_guard: 1024,
            table_lines: 8,
            reference_lines: 4,
            stack_lines: 2,
            read_buffers: 4,
            write_buffers: 4,
            reuse_views: false,
        }
    }
    pub fn blocks(&self) -> u64 {
        self.slots / self.block_chunks
    }
    pub fn slot_bits(&self) -> u32 {
        (64 - (self.blocks() - 1).leading_zeros()).max(1)
    }
    pub fn words(&self) -> usize {
        (self.slots * self.chunk_frames).div_ceil(16) as usize * 16
            + 2 * self.blocks().div_ceil(16) as usize * 16
            + self.table_pages as usize * 16
            + self.retired_tables as usize * 16
    }
    pub fn parameters(&self) -> Vec<(&'static str, u64)> {
        vec![
            ("VIEWS", self.views),
            ("SLOTS", self.slots),
            ("CHUNK_FRAMES", self.chunk_frames),
            ("BLOCK_CHUNKS", self.block_chunks),
            ("TABLE_PAGES", self.table_pages),
            ("RETIRED_TABLES", self.retired_tables),
            ("RECLAIM_BLOCKS", self.reclaim_blocks),
            ("RECLAIM_CYCLES", self.reclaim_cycles),
            ("RECLAIM_GUARD", self.reclaim_guard),
            ("TABLE_LINES", self.table_lines),
            ("REF_LINES", self.reference_lines),
            ("STACK_LINES", self.stack_lines),
            ("READ_BUFFERS", self.read_buffers),
            ("WRITE_BUFFERS", self.write_buffers),
            ("REUSE_VIEWS", u64::from(self.reuse_views)),
        ]
    }
}
const SIGNALS: &[&str] = &[
    "busy",
    "done",
    "error_code",
    "cmd_ready",
    "result_view",
    "result_empty",
    "write_ready",
    "read_valid",
    "read_data",
    "read_ready",
    "cfg_ready",
    "cfg_response_valid",
    "cfg_response_error",
    "cfg_read_data",
];
pub struct Store {
    pub geometry: Geometry,
    pub mode: Mode,
    pub memory: Shared,
    pub base: u64,
    pub random: Random,
    pub timing: u64,
    pub chunk_limit: Option<u64>,
    prefix: String,
    owns_reset: bool,
    loaded: bool,
}
impl Store {
    pub fn new(s: &mut Sim<'_>, g: Geometry, mode: Mode, seed: u64, prefix: &str) -> Result<Self> {
        let memory = Memory::attach(
            s,
            mode,
            g.words(),
            if mode == Mode::Legacy {
                0x1234000000
            } else {
                0x01000000
            },
            &crate::artifact_directory("media/dram"),
            seed,
        )?;
        memory.borrow_mut().control.stalls = true;
        memory.borrow_mut().sample_words = Some((g.slots * g.chunk_frames) as usize);
        let port = |n| format!("{prefix}{n}");
        s.agent(crate::protocol::ReadyValid::new(
            &port("read_valid"),
            &port("read_ready"),
            &[&port("read_data")],
        ));
        s.agent(crate::protocol::Exclusion::new(
            &port("busy"),
            &[&port("cmd_ready"), &port("cfg_ready")],
        ));
        Ok(Self {
            geometry: g,
            mode,
            memory,
            base: 0x1234000000,
            random: Random::new(seed),
            timing: 0,
            chunk_limit: None,
            prefix: prefix.into(),
            owns_reset: prefix.is_empty(),
            loaded: false,
        })
    }
    pub fn port(&self, name: &str) -> String {
        format!("{}{name}", self.prefix)
    }
    pub fn set(&self, s: &mut Sim<'_>, values: &[(&str, u64)]) {
        for &(name, v) in values {
            s.model.set(&self.port(name), v);
        }
    }
    pub fn tick(&mut self, s: &mut Sim<'_>) -> (Snapshot, Snapshot) {
        let names: Vec<_> = SIGNALS.iter().map(|n| self.port(n)).collect();
        let names: Vec<_> = names.iter().map(String::as_str).collect();
        s.tick(&names)
    }

    pub fn get(&self, s: &Sim<'_>, name: &str) -> u64 {
        s.model.get(&self.port(name))
    }
    pub fn reset(&mut self, s: &mut Sim<'_>) {
        self.memory.borrow_mut().rebase(self.base);
        for (name, value) in [
            ("maintenance_enable", 1),
            ("maintenance_tick", 0),
            ("arena_base", self.base),
        ] {
            if s.model.has(name) {
                s.model.set(name, value);
            }
        }
        for name in [
            "hint_valid",
            "hint_view",
            "hint_position",
            "hint_length",
            "hint_count",
        ] {
            let p = self.port(name);
            if s.model.has(&p) {
                s.model.set(&p, 0);
            }
        }
        self.set(
            s,
            &[
                ("cmd_valid", 0),
                ("cmd_opcode", 0),
                ("cmd_view", 0),
                ("cmd_start", 0),
                ("cmd_length", 0),
                ("cmd_count", 0),
                ("write_valid", 0),
                ("write_data", 0),
                ("read_ready", 0),
                ("cfg_valid", 0),
                ("cfg_write", 0),
                ("cfg_space", 0),
                ("cfg_address", 0),
                ("cfg_write_data", 0),
            ],
        );
        if self.owns_reset {
            s.model.set("reset", 1);
            self.tick(s);
            s.model.set("reset", 0);
        }
        self.tick(s);
        assert_ne!(self.get(s, "cmd_ready"), 0);
        assert_eq!(self.get(s, "busy"), 0);
        if !self.loaded {
            Memory::fill(&self.memory, s, 0xa5a5a5a5);
            self.loaded = true;
        }
    }
    pub fn configure(
        &mut self,
        s: &mut Sim<'_>,
        space: u64,
        address: u64,
        value: Option<&Bits>,
        error: bool,
    ) -> Bits {
        Config::new(&self.port("cfg"), &self.port("cfg_space"))
            .access(s, space, address, value, error)
    }
    pub fn read_config(&mut self, s: &mut Sim<'_>, space: u64, address: u64) -> u64 {
        self.configure(s, space, address, None, false).get(0, 64)
    }
    pub fn write_config(&mut self, s: &mut Sim<'_>, space: u64, address: u64, value: u64) {
        self.configure(s, space, address, Some(&Bits::from_u64(value)), false);
    }
    pub fn table_address(&self, view: u64, ordinal: u64) -> u64 {
        (view << self.geometry.slot_bits()) | ordinal
    }
    pub fn load(&mut self, s: &mut Sim<'_>, media: &Value) {
        assert_eq!(
            media["pending_reclamation"].as_u64().unwrap_or(0),
            0,
            "drain reclamation before restoring a snapshot"
        );
        self.reset(s);
        assert_eq!(
            media["chunk_frames"].as_u64().unwrap(),
            self.geometry.chunk_frames
        );
        assert_eq!(
            media["block_chunks"].as_u64().unwrap_or(1),
            self.geometry.block_chunks
        );
        let samples: Vec<i32> = serde_json::from_value(media["samples"].clone()).unwrap();
        let slots = samples.len() as u64 / self.geometry.chunk_frames;
        let views = media["views"].as_array().unwrap();
        assert!(slots <= self.geometry.slots && views.len() as u64 <= self.geometry.views);
        self.chunk_limit = media["chunk_limit"].as_u64();
        self.write_config(
            s,
            0,
            3,
            self.chunk_limit.unwrap_or(self.geometry.slots) / self.geometry.block_chunks,
        );
        Memory::write_prefix(
            &self.memory,
            s,
            &samples.iter().map(|&v| v as u32).collect::<Vec<_>>(),
        );
        let mut refs = BTreeMap::<u64, u64>::new();
        for (index, view) in views.iter().enumerate() {
            let mut descriptor = Bits::zero(160);
            if !view.is_null() {
                let (blocks, offset) =
                    view_blocks(view, self.geometry.chunk_frames, self.geometry.block_chunks);
                descriptor
                    .set(0, 1, 1)
                    .set(1, 1, u64::from(!view["range"].is_null()))
                    .set(32, 32, offset)
                    .set(64, 64, view["frames"].as_u64().unwrap())
                    .set(128, 32, blocks.len() as u64);
                for (ordinal, &block) in blocks.iter().enumerate() {
                    self.write_config(
                        s,
                        2,
                        self.table_address(index as u64, ordinal as u64),
                        block,
                    );
                    *refs.entry(block).or_default() += 1;
                }
            }
            self.configure(s, 1, index as u64, Some(&descriptor), false);
        }
        for block in 0..slots / self.geometry.block_chunks {
            self.write_config(s, 3, block, refs.get(&block).copied().unwrap_or(0));
        }
        let free = media["free_chunks"].as_array().unwrap();
        for index in (0..free.len()).step_by(self.geometry.block_chunks as usize) {
            self.write_config(
                s,
                4,
                index as u64 / self.geometry.block_chunks,
                free[index].as_u64().unwrap() / self.geometry.block_chunks,
            );
        }
        for (i, value) in [
            views.len() as u64,
            slots / self.geometry.block_chunks,
            free.len() as u64 / self.geometry.block_chunks,
        ]
        .into_iter()
        .enumerate()
        {
            self.write_config(s, 0, i as u64, value);
        }
    }
    pub fn admit(
        &mut self,
        s: &mut Sim<'_>,
        opcode: u64,
        view: u64,
        start: u64,
        length: u64,
        count: u64,
    ) -> Snapshot {
        self.set(
            s,
            &[
                ("cmd_valid", 1),
                ("cmd_opcode", opcode),
                ("cmd_view", view),
                ("cmd_start", start),
                ("cmd_length", length),
                ("cmd_count", count),
            ],
        );
        for _ in 0..100_000 {
            let (before, after) = self.tick(s);
            if before.get(&self.port("cmd_ready")) != 0 {
                self.set(s, &[("cmd_valid", 0)]);
                return after;
            }
        }
        panic!("store command admission timeout");
    }
    fn finish_control(&mut self, s: &mut Sim<'_>, after: Snapshot, limit: u64, code: u64) {
        if after.get(&self.port("done")) == 0 {
            let name = self.port("done");
            s.until(limit, "store control completion", |m| m.get(&name) != 0);
        }
        assert_eq!(self.get(s, "error_code"), code);
        self.tick(s);
    }
    pub fn fence(&mut self, s: &mut Sim<'_>) {
        self.set(s, &[("write_valid", 0), ("read_ready", 0)]);
        let after = self.admit(s, 7, 0, 0, 0, 0);
        self.finish_control(s, after, 100_000, 0);
    }
    pub fn drain(&mut self, s: &mut Sim<'_>) {
        if self.read_config(s, 6, 0) == 0 {
            return;
        }
        let limit = 100_000 + s.model.get("reclaim_references") * 4096;
        let after = self.admit(s, 10, 0, 0, 0, 0);
        self.finish_control(s, after, limit, 0);
    }
    pub fn grant(&mut self, s: &mut Sim<'_>, blocks: u64, cycles: Option<u64>) {
        let after = self.admit(
            s,
            9,
            0,
            cycles.unwrap_or(self.geometry.reclaim_cycles),
            0,
            blocks,
        );
        assert_ne!(after.get(&self.port("done")), 0);
        assert_eq!(after.get(&self.port("error_code")), 0);
        self.tick(s);
    }
    pub fn command(
        &mut self,
        s: &mut Sim<'_>,
        command: &Value,
        stalls: bool,
        drain: bool,
        observe: bool,
    ) -> Value {
        self.memory.borrow_mut().rebase(self.base);
        self.memory.borrow_mut().control.stalls = stalls;
        let kind = command["kind"].as_str().unwrap();
        let opcode = [
            "create", "slice", "clear", "remove", "reclaim", "append", "read",
        ]
        .iter()
        .position(|&k| k == kind)
        .unwrap() as u64;
        let source: Vec<i32> = command
            .get("samples")
            .map(|v| serde_json::from_value(v.clone()).unwrap())
            .unwrap_or_default();
        let frames = command["frames"].as_u64().unwrap_or(0);
        let mut after = self.admit(
            s,
            opcode,
            command["source"]
                .as_u64()
                .or(command["view"].as_u64())
                .unwrap_or(0),
            command["start"]
                .as_u64()
                .or(command["position"].as_u64())
                .unwrap_or(0),
            if kind == "slice" {
                frames
            } else {
                command["length"].as_u64().unwrap_or(0)
            },
            if kind == "append" {
                source.len() as u64
            } else if kind == "read" {
                frames
            } else {
                0
            },
        );
        let accepted = s.cycles;
        let mut sent = 0;
        let mut samples = Vec::new();
        let mut offered = false;
        let mut done = false;
        for _ in 0..2000u64.saturating_add(
            64u64.saturating_mul(
                (self.geometry.chunk_frames * self.geometry.slots)
                    .saturating_add(source.len() as u64)
                    .saturating_add(frames),
            ),
        ) {
            if after.get(&self.port("done")) != 0 {
                done = true;
                break;
            }
            offered = sent < source.len() && (offered || !stalls || self.random.yes(4));
            let ready = !stalls || self.random.yes(4);
            self.set(
                s,
                &[
                    ("write_valid", u64::from(offered)),
                    (
                        "write_data",
                        if offered {
                            u64::from(source[sent] as u32)
                        } else {
                            0
                        },
                    ),
                    ("read_ready", u64::from(ready)),
                ],
            );
            let (before, next) = self.tick(s);
            after = next;
            if offered && before.get(&self.port("write_ready")) != 0 {
                sent += 1;
                offered = false;
            }
            if before.get(&self.port("read_valid")) != 0 && ready {
                samples.push(before.get(&self.port("read_data")) as i32);
            }
        }
        assert!(done, "store command timeout: {command}");
        self.timing = s.cycles - accepted;
        let code = after.get(&self.port("error_code"));
        let mut value = json!({"kind":"none"});
        if code == 0 {
            match kind {
                "create" | "slice" => {
                    value = json!({"kind":"view","value":after.get(&self.port("result_view"))})
                }
                "reclaim" => {
                    value =
                        json!({"kind":"empty","value":after.get(&self.port("result_empty")) != 0})
                }
                "read" => {
                    assert_eq!(samples.len() as u64, frames);
                    value = json!({"kind":"samples","value":samples});
                }
                "append" => assert_eq!(sent, source.len()),
                _ => (),
            }
        }
        self.set(s, &[("write_valid", 0), ("read_ready", 0)]);
        let (_, after) = self.tick(s);
        assert_eq!(
            after.get(&self.port("done")) | after.get(&self.port("busy")),
            0
        );
        let status = if code == 0 {
            json!({"kind":"completed"})
        } else {
            json!({"kind":"failed","value":{"error":match code {1=>"invalid_view",2=>"invalid_state",3=>"memory_full",4=>"invalid_shape",5=>"memory_fault",_=>panic!("store error {code}")}}})
        };
        if drain && code == 0 {
            self.drain(s);
        }
        let media = if observe {
            self.snapshot(s)
        } else {
            Value::Null
        };
        json!({"status":status,"value":value,"media":media})
    }
    pub fn snapshot(&mut self, s: &mut Sim<'_>) -> Value {
        let enabled = s
            .model
            .has("maintenance_enable")
            .then(|| s.model.get("maintenance_enable"));
        if enabled.is_some() {
            s.model.set("maintenance_enable", 0);
        }
        self.fence(s);
        let memory = Memory::read(&self.memory, s);
        let views = self.read_config(s, 0, 0);
        let blocks = self.read_config(s, 0, 1);
        let free = self.read_config(s, 0, 2);
        assert!(views <= self.geometry.views && free <= blocks && blocks <= self.geometry.blocks());
        let free_blocks: Vec<_> = (0..free).map(|i| self.read_config(s, 4, i)).collect();
        assert_eq!(
            free_blocks.iter().collect::<BTreeSet<_>>().len(),
            free as usize,
            "duplicate free block IDs"
        );
        assert!(free_blocks.iter().all(|&b| b < blocks));
        let bc = self.geometry.block_chunks;
        let mut result = json!({"chunk_frames":self.geometry.chunk_frames,"samples":memory[..(blocks*self.geometry.block_chunks*self.geometry.chunk_frames) as usize].iter().map(|&v| v as i32).collect::<Vec<_>>(),"free_chunks":free_blocks.iter().flat_map(|&b| (0..bc).map(move |i| b*bc+i)).collect::<Vec<_>>(),"views":[]});
        if self.geometry.block_chunks != 1 {
            result["block_chunks"] = json!(self.geometry.block_chunks);
        }
        if let Some(limit) = self.chunk_limit {
            result["chunk_limit"] = json!(limit);
        }
        let mut refs = vec![0u64; blocks as usize];
        let mut observed_views = Vec::new();
        for index in 0..views {
            let descriptor = self.configure(s, 1, index, None, false);
            let view = if descriptor.get(0, 1) == 0 {
                Value::Null
            } else {
                let ids: Vec<_> = (0..descriptor.get(128, 32))
                    .map(|ordinal| self.read_config(s, 2, self.table_address(index, ordinal)))
                    .collect();
                let frames = descriptor.get(64, 64);
                let offset = descriptor.get(32, 32);
                let first = offset / self.geometry.chunk_frames;
                let count = (offset % self.geometry.chunk_frames + frames)
                    .div_ceil(self.geometry.chunk_frames);
                let chunks: Vec<_> = (0..count)
                    .map(|i| {
                        ids[((first + i) / self.geometry.block_chunks) as usize]
                            * self.geometry.block_chunks
                            + (first + i) % self.geometry.block_chunks
                    })
                    .collect();
                for block in ids {
                    refs[block as usize] += 1;
                }
                let mut view = json!({"chunks":chunks,"frames":frames});
                if descriptor.get(1, 1) != 0 {
                    view["range"] = json!({"offset":offset%self.geometry.chunk_frames});
                }
                view
            };
            observed_views.push(view);
        }
        result["views"] = json!(observed_views);
        let pending = self.read_config(s, 6, 0);
        if pending != 0 {
            result["pending_reclamation"] = json!(pending);
        }
        let head = self.read_config(s, 6, 9);
        let capacity = self.read_config(s, 6, 8);
        assert!(pending <= capacity);
        let page_base = (self.geometry.slots * self.geometry.chunk_frames).div_ceil(16) * 16
            + 2 * self.geometry.blocks().div_ceil(16) * 16;
        let queue_base = page_base + self.geometry.table_pages * 16;
        let levels = (64 - (self.geometry.blocks() - 1).leading_zeros())
            .div_ceil(4)
            .max(1);
        let mut remaining = 0;
        for slot in 0..pending {
            let address = queue_base + ((head + slot) % capacity) * 16;
            let root = u64::from(memory[address as usize]);
            let count = u64::from(memory[address as usize + 1]);
            assert!(root != 0 && count > 0 && count <= self.geometry.blocks());
            remaining += count;
            for ordinal in 0..count {
                let mut page = root;
                for level in (0..levels).rev() {
                    assert!(page > 0 && page <= self.geometry.table_pages);
                    let value = u64::from(
                        memory[(page_base + (page - 1) * 16 + ((ordinal >> (4 * level)) & 15))
                            as usize],
                    );
                    if level != 0 {
                        page = value;
                    } else {
                        refs[value as usize] += 1;
                    }
                }
            }
        }
        assert_eq!(remaining, self.read_config(s, 6, 1));
        if self.read_config(s, 6, 7) & 2 != 0 {
            let block = self.read_config(s, 6, 6);
            refs[block as usize] -= 1;
        }
        for block in 0..blocks {
            assert_eq!(
                self.read_config(s, 3, block),
                refs[block as usize],
                "block {block} ownership count"
            );
        }
        assert_eq!(
            free_blocks.iter().copied().collect::<BTreeSet<_>>(),
            refs.iter()
                .enumerate()
                .filter_map(|(i, &n)| (n == 0).then_some(i as u64))
                .collect(),
            "unowned blocks differ from reusable IDs"
        );
        if self.geometry.block_chunks != 1 {
            canonicalize(&mut result);
        }
        if let Some(enabled) = enabled {
            s.model.set("maintenance_enable", enabled);
        }
        result
    }
}
pub fn view_blocks(view: &Value, cf: u64, bc: u64) -> (Vec<u64>, u64) {
    let ids = view["chunks"].as_array().unwrap();
    let first = ids.first().map_or(0, |v| v.as_u64().unwrap() % bc);
    let blocks = ids
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            (i == 0 || (first + i as u64) % bc == 0).then(|| v.as_u64().unwrap() / bc)
        })
        .collect();
    (
        blocks,
        first * cf + view["range"]["offset"].as_u64().unwrap_or(0),
    )
}
pub fn canonicalize(media: &mut Value) {
    let raw: Vec<i32> = serde_json::from_value(media["samples"].clone()).unwrap();
    let mut samples = vec![0; raw.len()];
    let cf = media["chunk_frames"].as_u64().unwrap();
    for view in media["views"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| !v.is_null())
    {
        let mut offset = view["range"]["offset"].as_u64().unwrap_or(0);
        let mut remaining = view["frames"].as_u64().unwrap();
        for chunk in view["chunks"].as_array().unwrap() {
            let count = (cf - offset).min(remaining) as usize;
            let start = (chunk.as_u64().unwrap() * cf + offset) as usize;
            samples[start..start + count].copy_from_slice(&raw[start..start + count]);
            remaining -= count as u64;
            offset = 0;
        }
        assert_eq!(remaining, 0);
    }
    media["samples"] = json!(samples);
}
