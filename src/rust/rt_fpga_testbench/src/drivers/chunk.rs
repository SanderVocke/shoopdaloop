use crate::{
    Result,
    memory::{Memory, Mode, Shared},
    random::Random,
    sim::{Sim, Snapshot},
};
use serde_json::Value;
pub const SIGNALS: &[&str] = &[
    "cmd_ready",
    "write_ready",
    "read_valid",
    "read_data",
    "read_last",
    "response_valid",
    "response_id",
    "response_error",
    "prefetch_ready",
    "busy",
    "fault",
    "read_ready",
    "response_ready",
];
pub struct Chunk {
    pub capacities: [u64; 4],
    pub memory: Shared,
    pub base: u64,
    pub random: Random,
    loaded: bool,
}
impl Chunk {
    pub fn new(s: &mut Sim<'_>, c: [u64; 4], mode: Mode, seed: u64) -> Result<Self> {
        let base = 0x1234000000;
        let memory = Memory::attach(
            s,
            mode,
            (c[0] * c[1]) as usize,
            if mode == Mode::Legacy {
                base
            } else {
                0x01000000
            },
            &crate::artifact_directory("chunk/dram"),
            seed,
        )?;
        memory.borrow_mut().control.stalls = true;
        s.agent(crate::protocol::ReadyValid::new(
            "read_valid",
            "read_ready",
            &["read_data", "read_last"],
        ));
        s.agent(crate::protocol::ReadyValid::new(
            "response_valid",
            "response_ready",
            &["response_id", "response_error"],
        ));
        if mode == Mode::Legacy {
            s.agent(Bursts {
                base,
                frames: c[1],
                active: None,
            });
        }
        Ok(Self {
            capacities: c,
            memory,
            base,
            random: Random::new(seed),
            loaded: false,
        })
    }
    pub fn tick(&mut self, s: &mut Sim<'_>) -> (Snapshot, Snapshot) {
        s.tick(SIGNALS)
    }
    pub fn reset(&mut self, s: &mut Sim<'_>, samples: Option<&[i32]>) {
        s.model.set_all(&[
            ("retire_valid", 0),
            ("retire_chunk", 0),
            ("reset", 1),
            ("cmd_valid", 0),
            ("cmd_opcode", 0),
            ("cmd_id", 0),
            ("cmd_chunk", 0),
            ("cmd_offset", 0),
            ("cmd_count", 0),
            ("write_valid", 0),
            ("write_data", 0),
            ("read_ready", 0),
            ("response_ready", 0),
            ("prefetch_valid", 0),
            ("prefetch_chunk", 0),
            ("arena_base", self.base),
        ]);
        self.tick(s);
        s.model.set("reset", 0);
        self.tick(s);
        if let Some(samples) = samples {
            Memory::load(
                &self.memory,
                s,
                &samples.iter().map(|&v| v as u32).collect::<Vec<_>>(),
            );
            self.loaded = true;
        } else if !self.loaded {
            Memory::fill(&self.memory, s, 0);
            self.loaded = true;
        }
        assert_ne!(s.model.get("cmd_ready"), 0);
        assert_eq!(s.model.get("fault"), 0);
    }
    pub fn hint(&mut self, s: &mut Sim<'_>, chunk: u64) {
        s.model
            .set_all(&[("prefetch_valid", 1), ("prefetch_chunk", chunk)]);
        for _ in 0..100_000 {
            if self.tick(s).0.get("prefetch_ready") != 0 {
                s.model.set("prefetch_valid", 0);
                return;
            }
        }
        panic!("chunk hint admission timeout");
    }
    pub fn command(
        &mut self,
        s: &mut Sim<'_>,
        operation: &Value,
        id: u64,
        stalls: bool,
    ) -> (u64, Vec<i32>) {
        let kind = operation["kind"].as_str().unwrap();
        let opcode = ["read", "write", "prefetch", "fence", "retire"]
            .iter()
            .position(|&k| k == kind)
            .unwrap() as u64;
        let source: Vec<i32> = operation
            .get("samples")
            .map(|v| serde_json::from_value(v.clone()).unwrap())
            .unwrap_or_default();
        let count = if kind == "write" {
            source.len() as u64
        } else {
            operation["frames"].as_u64().unwrap_or(0)
        };
        s.model.set_all(&[
            ("cmd_valid", 1),
            ("cmd_opcode", opcode),
            ("cmd_id", id),
            ("cmd_chunk", operation["chunk"].as_u64().unwrap_or(0)),
            ("cmd_offset", operation["offset"].as_u64().unwrap_or(0)),
            ("cmd_count", count),
            ("read_ready", 0),
            ("write_valid", 0),
            ("response_ready", 0),
        ]);
        let mut accepted = false;
        for _ in 0..100_000 {
            if self.tick(s).0.get("cmd_ready") != 0 {
                accepted = true;
                break;
            }
        }
        assert!(accepted, "chunk command admission: {operation}");
        s.model.set_all(&[
            ("cmd_valid", 0),
            ("cmd_chunk", u32::MAX as u64),
            ("cmd_offset", u32::MAX as u64),
            ("cmd_count", u32::MAX as u64),
            ("cmd_id", u32::MAX as u64),
        ]);
        let mut sent = 0;
        let mut samples = Vec::new();
        let mut offered = false;
        for _ in 0..100_000 {
            offered = sent < source.len() && (offered || !stalls || self.random.yes(4));
            let ready = !stalls || self.random.yes(4);
            let response = !stalls || self.random.yes(4);
            s.model.set_all(&[
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
                ("response_ready", u64::from(response)),
            ]);
            let (before, _) = self.tick(s);
            if offered && before.get("write_ready") != 0 {
                sent += 1;
                offered = false;
            }
            if before.get("read_valid") != 0 && ready {
                samples.push(before.get("read_data") as i32);
                assert_eq!(before.get("read_last") != 0, samples.len() as u64 == count);
            }
            if before.get("response_valid") != 0 && response {
                assert_eq!(before.get("response_id"), id);
                let code = before.get("response_error");
                if code == 0 {
                    assert_eq!(sent, source.len());
                }
                s.model
                    .set_all(&[("write_valid", 0), ("read_ready", 0), ("response_ready", 0)]);
                return (code, samples);
            }
        }
        panic!("chunk command completion: {operation}");
    }
    pub fn image(&self, s: &mut Sim<'_>) -> Vec<i32> {
        Memory::read(&self.memory, s)
            .into_iter()
            .map(|v| v as i32)
            .collect()
    }
    pub fn load_image(&self, s: &mut Sim<'_>, data: &[i32]) {
        Memory::load(
            &self.memory,
            s,
            &data.iter().map(|&v| v as u32).collect::<Vec<_>>(),
        );
    }
}

struct Bursts {
    base: u64,
    frames: u64,
    active: Option<(u64, u64, u64)>,
}
impl crate::sim::Agent for Bursts {
    fn sample(&mut self, m: &crate::model::Model<'_>, e: crate::clock::Edges) {
        if !e.rises(0) {
            return;
        }
        if m.get("reset") != 0 {
            self.active = None;
            return;
        }
        if m.get("memory_valid") != 0 && m.get("memory_ready") != 0 {
            let word = (m.get("memory_address") - self.base) / 4;
            let write = m.get("memory_write");
            let active = self.active.get_or_insert_with(|| {
                assert_eq!(word % self.frames, 0);
                (write, word, 0)
            });
            assert_eq!(
                (write, word),
                (active.0, active.1 + active.2),
                "chunk transfers interleaved"
            );
            active.2 += 1;
            if active.2 == self.frames {
                self.active = None;
            }
        }
    }
}
