pub mod axi;
pub mod legacy;
use crate::{
    Result,
    clock::Edges,
    model::Model,
    sim::{Agent, Sim},
    timing::{Fixed, dram::Dram},
};
use serde_json::{Value, json};
use std::{cell::RefCell, path::Path, rc::Rc};

#[derive(Default)]
pub struct Stable<T>(Option<T>);
impl<T: Eq + Clone + std::fmt::Debug> Stable<T> {
    pub fn check(&mut self, name: &str, valid: bool, ready: bool, value: T) {
        if let Some(held) = &self.0 {
            assert!(valid, "{name}: stalled valid disappeared");
            assert_eq!(&value, held, "{name}: stalled payload changed");
        }
        self.0 = (valid && !ready).then_some(value);
    }
}
#[derive(Default)]
pub struct Control {
    pub stalls: bool,
    pub pause: bool,
    pub pause_responses: bool,
    pub hold_aw: bool,
    pub hold_ar: bool,
    pub hold_w: bool,
    pub fail_next: bool,
    pub error_index: Option<usize>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Mode {
    Instant,
    Fixed,
    Dramsim3,
    Legacy,
}
pub enum Backend {
    Native {
        token: u64,
        reads: u64,
        writes: u64,
        active: Option<Active>,
    },
    Axi(Box<axi::Axi>),
    Legacy(legacy::Legacy),
}
#[derive(Clone, Copy, Debug)]
pub struct Active {
    pub write: bool,
    pub error: bool,
    pub index: usize,
}
pub struct Memory {
    pub control: Control,
    pub backend: Backend,
    pub words: usize,
    pub base: u64,
    pub width: usize,
    pub sample_words: Option<usize>,
    pub sample_transactions: u64,
}
pub type Shared = Rc<RefCell<Memory>>;
impl Memory {
    pub fn attach(
        sim: &mut Sim<'_>,
        mode: Mode,
        words: usize,
        base: u64,
        output: &Path,
        seed: u64,
    ) -> Result<Shared> {
        let width = if sim.model.has("m_axi_wdata") {
            sim.model.width("m_axi_wdata") / 8
        } else {
            4
        };
        let backend = if mode == Mode::Legacy {
            Backend::Legacy(legacy::Legacy::new(words, base, seed))
        } else if sim.model.has("sim_host_op") {
            assert_eq!(mode, Mode::Instant);
            assert_eq!(sim.model.get("sim_capacity"), words as u64);
            Backend::Native {
                token: 0,
                reads: 0,
                writes: 0,
                active: None,
            }
        } else {
            let timing: Box<dyn crate::timing::Timing> = match mode {
                Mode::Fixed => Box::new(Fixed::new(7)),
                Mode::Dramsim3 => Box::new(Dram::default(output, 100_000_000)?),
                _ => return Err("external AXI slave requires fixed or DRAMsim3 timing".into()),
            };
            Backend::Axi(Box::new(axi::Axi::new(words, base, width, timing, seed)))
        };
        let memory = Rc::new(RefCell::new(Self {
            control: Control::default(),
            backend,
            words,
            base,
            width,
            sample_words: None,
            sample_transactions: 0,
        }));
        sim.agent(memory.clone());
        Ok(memory)
    }
    pub fn native(&self) -> bool {
        matches!(self.backend, Backend::Native { .. })
    }
    pub fn bus_snapshot(&self) -> Option<crate::timing::dram::Snapshot> {
        match &self.backend {
            Backend::Axi(a) => a.timing.bus_snapshot(),
            _ => None,
        }
    }
    pub fn bus_utilization(
        &self,
        start: crate::timing::dram::Snapshot,
        end: crate::timing::dram::Snapshot,
    ) -> Option<Value> {
        match &self.backend {
            Backend::Axi(a) => a.timing.bus_utilization(start, end),
            _ => None,
        }
    }
    pub fn rebase(&mut self, base: u64) {
        if let Backend::Legacy(a) = &mut self.backend {
            a.rebase(base);
        }
    }
    pub fn reads(&self) -> u64 {
        match &self.backend {
            Backend::Native { reads, .. } => *reads,
            Backend::Axi(a) => a.reads,
            Backend::Legacy(a) => a.reads,
        }
    }
    pub fn writes(&self) -> u64 {
        match &self.backend {
            Backend::Native { writes, .. } => *writes,
            Backend::Axi(a) => a.writes,
            Backend::Legacy(a) => a.writes,
        }
    }
    pub fn requests(&self) -> u64 {
        self.reads() + self.writes()
    }
    pub fn active(&self) -> Option<Active> {
        match &self.backend {
            Backend::Native { active, .. } => *active,
            Backend::Axi(a) => a.active.as_ref().map(|a| Active {
                write: a.write,
                error: a.error,
                index: a.index,
            }),
            Backend::Legacy(a) => a.active(),
        }
    }
    pub fn word_only(&mut self) {
        if let Backend::Axi(a) = &mut self.backend {
            a.word_only = true;
        }
    }
    pub fn report(&mut self, finalize: bool) -> Value {
        match &mut self.backend {
            Backend::Native { reads, writes, .. } => {
                json!({"backend":"systemverilog","purpose":"functional","reads":reads,"writes":writes})
            }
            Backend::Axi(a) => {
                let mut report = a.report(finalize);
                let realistic = report["native"]["abi_version"] == 2;
                report["backend"] = json!("marlin");
                report["purpose"] = json!(if realistic { "realistic" } else { "diagnostic" });
                report["model"] = json!(if realistic { "dramsim3" } else { "fixed" });
                report
            }
            Backend::Legacy(a) => json!({"backend":"legacy","reads":a.reads,"writes":a.writes}),
        }
    }
    pub fn fill(memory: &Shared, s: &mut Sim<'_>, word: u32) {
        let mut m = memory.borrow_mut();
        match &mut m.backend {
            Backend::Axi(a) => {
                a.words.fill(word);
                return;
            }
            Backend::Legacy(a) => {
                a.words.fill(word);
                return;
            }
            _ => (),
        }
        drop(m);
        assert_eq!(s.model.get("sim_busy"), 0);
        s.model
            .set_all(&[("sim_host_op", 2), ("sim_host_write_data", u64::from(word))]);
        s.idle(1);
        s.model.set("sim_host_op", 0);
    }
    pub fn load(memory: &Shared, s: &mut Sim<'_>, words: &[u32]) {
        let mut m = memory.borrow_mut();
        assert_eq!(m.words, words.len());
        match &mut m.backend {
            Backend::Axi(a) => {
                a.words.copy_from_slice(words);
                return;
            }
            Backend::Legacy(a) => {
                a.words.copy_from_slice(words);
                return;
            }
            _ => (),
        }
        drop(m);
        assert_eq!(s.model.get("sim_busy"), 0);
        if words.iter().all(|&v| v == words[0]) {
            Self::fill(memory, s, words[0]);
            return;
        }
        s.model.set("sim_host_op", 1);
        for (i, &word) in words.iter().enumerate() {
            s.model.set_all(&[
                ("sim_host_address", i as u64),
                ("sim_host_write_data", u64::from(word)),
            ]);
            s.idle(1);
        }
        s.model.set("sim_host_op", 0);
    }
    pub fn read(memory: &Shared, s: &mut Sim<'_>) -> Vec<u32> {
        let m = memory.borrow();
        match &m.backend {
            Backend::Axi(a) => return a.words.clone(),
            Backend::Legacy(a) => return a.words.clone(),
            _ => (),
        }
        let words = m.words;
        drop(m);
        assert_eq!(s.model.get("sim_busy"), 0);
        s.model.set("sim_host_op", 0);
        (0..words)
            .map(|i| {
                s.model.set("sim_host_address", i as u64);
                s.model.eval();
                s.model.get("sim_host_read_data") as u32
            })
            .collect()
    }
    pub fn write_prefix(memory: &Shared, s: &mut Sim<'_>, words: &[u32]) {
        let mut m = memory.borrow_mut();
        assert!(words.len() <= m.words);
        match &mut m.backend {
            Backend::Axi(a) => {
                a.words[..words.len()].copy_from_slice(words);
                return;
            }
            Backend::Legacy(a) => {
                a.words[..words.len()].copy_from_slice(words);
                return;
            }
            _ => (),
        }
        drop(m);
        assert_eq!(s.model.get("sim_busy"), 0);
        s.model.set("sim_host_op", 1);
        for (i, &word) in words.iter().enumerate() {
            s.model.set_all(&[
                ("sim_host_address", i as u64),
                ("sim_host_write_data", u64::from(word)),
            ]);
            s.idle(1);
        }
        s.model.set("sim_host_op", 0);
    }
}
impl Agent for Memory {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if !e.rises(0) {
            return;
        }
        match &mut self.backend {
            Backend::Axi(a) => a.drive(m, &self.control),
            Backend::Legacy(a) => a.drive(m, &self.control),
            Backend::Native { token, .. } => {
                let seen = m.get("sim_fail_seen");
                if self.control.fail_next {
                    if *token == seen {
                        *token = seen.wrapping_add(1) & u64::from(u32::MAX);
                    }
                } else {
                    *token = seen;
                }
                let c = &self.control;
                let flags = u64::from(c.pause)
                    | (u64::from(c.pause_responses) << 1)
                    | (u64::from(c.hold_aw) << 2)
                    | (u64::from(c.hold_w) << 3)
                    | (u64::from(c.hold_ar) << 4);
                m.set_all(&[("sim_control", flags), ("sim_fail_token", *token)]);
            }
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if !e.rises(0) {
            return;
        }
        if m.get("reset") == 0
            && let Some(words) = self.sample_words
        {
            if m.has("m_axi_awvalid") {
                for (valid, ready, address) in [
                    ("m_axi_awvalid", "m_axi_awready", "m_axi_awaddr"),
                    ("m_axi_arvalid", "m_axi_arready", "m_axi_araddr"),
                ] {
                    if m.get(valid) != 0 && m.get(ready) != 0 {
                        let address = m.get(address);
                        if address >= self.base && address < self.base + words as u64 * 4 {
                            self.sample_transactions += 1;
                        }
                    }
                }
            } else if m.get("memory_valid") != 0 && m.get("memory_ready") != 0 {
                self.sample_transactions += 1;
            }
        }
        match &mut self.backend {
            Backend::Axi(a) => a.sample(m, &mut self.control),
            Backend::Legacy(a) => a.sample(m, &mut self.control),
            _ => (),
        }
    }
    fn observe(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0)
            && let Backend::Native {
                token,
                reads,
                writes,
                active,
            } = &mut self.backend
        {
            *reads = m.get("sim_reads");
            *writes = m.get("sim_writes");
            *active = (m.get("sim_active") != 0).then(|| Active {
                write: m.get("sim_write") != 0,
                error: m.get("sim_error") != 0,
                index: m.get("sim_index") as usize,
            });
            self.control.fail_next = *token != m.get("sim_fail_seen");
        }
    }
}
