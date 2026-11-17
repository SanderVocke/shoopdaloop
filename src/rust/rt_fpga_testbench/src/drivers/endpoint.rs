use crate::{
    Result,
    memory::{Memory, Mode, Shared, Stable},
    model::Design,
    random::Random,
    sim::{Sim, Snapshot},
};
pub const BASE: u64 = 0x01000000;
pub const WORDS: usize = 8192;
pub const RAW: &[&str] = &[
    "command_ready",
    "write_ready",
    "read_valid",
    "read_data",
    "read_error",
    "read_last",
    "completion_valid",
    "completion_error",
];
pub const WORD: &[&str] = &[
    "memory_ready",
    "memory_response_valid",
    "memory_response_error",
    "memory_read_data",
];
pub const CHUNK: &[&str] = &[
    "memory_ready",
    "memory_write_ready",
    "memory_read_valid",
    "memory_read_data",
    "memory_response_valid",
    "memory_response_error",
];
pub fn design(top: &str, width: u64, mode: Mode) -> Design {
    let root = crate::root();
    let design = Design::new(
        top,
        "axi_master chunk_dma word_dma word_axi chunk_axi"
            .split_whitespace()
            .map(|n| root.join(format!("fpga/axi_memory/rtl/{n}.sv")))
            .collect(),
    )
    .parameters(&[("DATA_WIDTH", width)]);
    if mode == Mode::Instant {
        design.memory(WORDS, BASE as u32)
    } else {
        design
    }
}
pub struct Endpoint {
    pub memory: Shared,
    pub random: Random,
    pub word: bool,
    pub chunk: bool,
    pub logical_base: u64,
    loaded: bool,
}
impl Endpoint {
    pub fn new(s: &mut Sim<'_>, mode: Mode, word: bool, chunk: bool) -> Result<Self> {
        let output = crate::artifact_directory(&format!(
            "axi/dram-{}-{}-{}",
            s.model.width("m_axi_wdata"),
            word,
            chunk
        ));
        let memory = Memory::attach(s, mode, WORDS, BASE, &output, 57)?;
        memory.borrow_mut().control.stalls = true;
        Ok(Self {
            memory,
            random: Random::new(731),
            word,
            chunk,
            logical_base: 0x1234000000,
            loaded: false,
        })
    }
    pub fn tick(&mut self, s: &mut Sim<'_>) -> Snapshot {
        s.tick(if self.chunk {
            CHUNK
        } else if self.word {
            WORD
        } else {
            RAW
        })
        .0
    }
    pub fn reset(&mut self, s: &mut Sim<'_>) {
        if self.word {
            self.logical_base = 0x1234000000;
            s.model.set_all(&[
                ("memory_valid", 0),
                ("memory_write", 0),
                ("memory_address", 0),
                ("memory_write_data", 0),
                ("memory_response_ready", 0),
                ("arena_base", self.logical_base),
                ("arena_bytes", (WORDS * 4) as u64),
                ("physical_base", BASE),
            ]);
            if self.chunk {
                s.model.set_all(&[
                    ("memory_count", 0),
                    ("memory_write_valid", 0),
                    ("memory_read_ready", 0),
                ]);
            }
        } else {
            s.model.set_all(&[
                ("command_valid", 0),
                ("command_write", 0),
                ("command_address", 0),
                ("command_length", 0),
                ("command_size", 2),
                ("write_valid", 0),
                ("write_data", 0),
                ("write_strobe", 0),
                ("read_ready", 0),
                ("completion_ready", 0),
            ]);
        }
        s.model.set("reset", 1);
        self.tick(s);
        s.model.set("reset", 0);
        self.tick(s);
        if !self.loaded {
            Memory::load(
                &self.memory,
                s,
                &(0..WORDS)
                    .map(|i| 0x12345678 ^ i as u32)
                    .collect::<Vec<_>>(),
            );
            self.loaded = true;
        }
    }
    pub fn transaction(
        &mut self,
        s: &mut Sim<'_>,
        address: u64,
        size: u64,
        count: u64,
        data: Option<&[u64]>,
        strobes: &[u64],
        error: bool,
    ) -> Vec<u64> {
        s.model.set_all(&[
            ("command_valid", 1),
            ("command_write", u64::from(data.is_some())),
            ("command_address", address),
            ("command_length", count - 1),
            ("command_size", size),
        ]);
        let mut accepted = false;
        for _ in 0..10_000 {
            if self.tick(s).get("command_ready") != 0 {
                accepted = true;
                break;
            }
        }
        assert!(accepted, "AXI descriptor admission");
        s.model.set("command_valid", 0);
        let mut sent = 0;
        let mut received = Vec::new();
        let mut held = Stable::default();
        let mut done = false;
        for _ in 0..20_000 {
            let offered = data.is_some_and(|d| sent < d.len());
            let ready = self.random.yes(3);
            s.model.set_all(&[
                ("write_valid", u64::from(offered)),
                ("write_data", if offered { data.unwrap()[sent] } else { 0 }),
                ("write_strobe", if offered { strobes[sent] } else { 0 }),
                ("read_ready", u64::from(ready)),
                ("completion_ready", u64::from(ready)),
            ]);
            let b = self.tick(s);
            if offered && b.get("write_ready") != 0 {
                sent += 1;
            }
            let (valid, payload) = if data.is_some() {
                (b.get("completion_valid"), vec![b.get("completion_error")])
            } else {
                (
                    b.get("read_valid"),
                    vec![b.get("read_data"), b.get("read_error"), b.get("read_last")],
                )
            };
            held.check("endpoint response", valid != 0, ready, payload.clone());
            if valid != 0 && ready {
                if data.is_some() {
                    assert_eq!(payload[0] != 0, error);
                    done = true;
                    break;
                }
                assert_eq!(payload[1] != 0, error);
                received.push(payload[0]);
                if payload[2] != 0 {
                    assert!(error || received.len() as u64 == count);
                    done = true;
                    break;
                }
            }
        }
        assert!(done, "AXI transfer completion");
        s.model.set_all(&[
            ("write_valid", 0),
            ("read_ready", 0),
            ("completion_ready", 0),
        ]);
        received
    }
    pub fn word_access(
        &mut self,
        s: &mut Sim<'_>,
        offset: i64,
        data: Option<u32>,
        error: bool,
    ) -> u32 {
        s.model.set_all(&[
            ("memory_valid", 1),
            ("memory_write", u64::from(data.is_some())),
            (
                "memory_address",
                self.logical_base.wrapping_add_signed(offset),
            ),
            ("memory_write_data", u64::from(data.unwrap_or(0))),
            ("memory_response_ready", 0),
        ]);
        let mut accepted = false;
        for _ in 0..10_000 {
            if self.tick(s).get("memory_ready") != 0 {
                accepted = true;
                break;
            }
        }
        assert!(accepted);
        s.model.set("memory_valid", 0);
        let mut response = None;
        for _ in 0..20_000 {
            let b = self.tick(s);
            if b.get("memory_response_valid") != 0 {
                response = Some(b);
                break;
            }
        }
        let before = response.expect("word response timeout");
        for _ in 0..4 {
            let b = self.tick(s);
            assert_ne!(b.get("memory_response_valid"), 0);
            assert_eq!(b.get("memory_read_data"), before.get("memory_read_data"));
        }
        assert_eq!(before.get("memory_response_error") != 0, error);
        s.model.set("memory_response_ready", 1);
        self.tick(s);
        before.get("memory_read_data") as u32
    }
    pub fn begin(&mut self, s: &mut Sim<'_>, offset: i64, count: u64, write: bool) {
        s.model.set_all(&[
            ("memory_valid", 1),
            ("memory_write", u64::from(write)),
            (
                "memory_address",
                self.logical_base.wrapping_add_signed(offset),
            ),
            ("memory_count", count),
            ("memory_response_ready", 0),
        ]);
        let mut accepted = false;
        for _ in 0..10_000 {
            if self.tick(s).get("memory_ready") != 0 {
                accepted = true;
                break;
            }
        }
        assert!(accepted, "chunk admission");
        s.model.set_all(&[
            ("memory_valid", 0),
            ("memory_count", 0),
            ("memory_address", u64::MAX),
            ("memory_write", u64::from(!write)),
        ]);
    }
    pub fn finish(
        &mut self,
        s: &mut Sim<'_>,
        count: u64,
        data: Option<&[u32]>,
        error: bool,
        stop_after: Option<usize>,
        release: bool,
    ) -> Vec<u32> {
        let mut sent = 0;
        let mut received = Vec::new();
        let mut held = Stable::default();
        let mut offered = false;
        let mut done = false;
        for cycle in 0..100_000 {
            if release && cycle == 30 {
                let mut m = self.memory.borrow_mut();
                m.control.hold_aw = false;
                m.control.hold_w = false;
            }
            offered = offered || (data.is_some_and(|d| sent < d.len()) && self.random.yes(3));
            let ready = self.random.yes(3) && stop_after.is_none_or(|n| received.len() < n);
            s.model.set_all(&[
                ("memory_write_valid", u64::from(offered)),
                (
                    "memory_write_data",
                    if offered {
                        u64::from(data.unwrap()[sent])
                    } else {
                        0
                    },
                ),
                ("memory_read_ready", u64::from(ready)),
            ]);
            let b = self.tick(s);
            if offered && b.get("memory_write_ready") != 0 {
                sent += 1;
                offered = false;
            }
            held.check(
                "chunk read stream",
                b.get("memory_read_valid") != 0,
                ready,
                b.get("memory_read_data"),
            );
            if b.get("memory_read_valid") != 0 && ready {
                received.push(b.get("memory_read_data") as u32);
            }
            if b.get("memory_response_valid") != 0 {
                assert_eq!(b.get("memory_response_error") != 0, error);
                assert!(
                    self.memory.borrow().active().is_none(),
                    "completion before AXI retirement"
                );
                if !error {
                    assert_eq!(
                        if data.is_some() { sent } else { received.len() } as u64,
                        count
                    );
                }
                done = true;
                break;
            }
        }
        assert!(done, "chunk completion timeout");
        s.model
            .set_all(&[("memory_write_valid", 0), ("memory_read_ready", 0)]);
        for _ in 0..5 {
            let b = self.tick(s);
            assert_ne!(b.get("memory_response_valid"), 0);
            assert_eq!(b.get("memory_response_error") != 0, error);
            assert_eq!(b.get("memory_ready"), 0);
        }
        s.model.set("memory_response_ready", 1);
        self.tick(s);
        received
    }
    pub fn chunk_access(
        &mut self,
        s: &mut Sim<'_>,
        offset: i64,
        count: u64,
        data: Option<&[u32]>,
        error: bool,
    ) -> Vec<u32> {
        self.begin(s, offset, count, data.is_some());
        self.finish(s, count, data, error, None, false)
    }
}
