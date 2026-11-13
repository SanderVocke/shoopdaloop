//! One edge loop for autonomous agents and sequential transaction drivers.
use crate::{
    bits::Bits,
    clock::{Edges, Scheduler},
    model::Model,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub trait Agent {
    fn drive(&mut self, _model: &mut Model<'_>, _edges: Edges) {}
    fn sample(&mut self, _model: &Model<'_>, _edges: Edges) {}
    fn observe(&mut self, _model: &Model<'_>, _edges: Edges) {}
}
impl<T: Agent> Agent for Rc<RefCell<T>> {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        self.borrow_mut().drive(m, e);
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        self.borrow_mut().sample(m, e);
    }
    fn observe(&mut self, m: &Model<'_>, e: Edges) {
        self.borrow_mut().observe(m, e);
    }
}
#[derive(Default)]
pub struct Snapshot(BTreeMap<String, Bits>);
impl Snapshot {
    pub fn get(&self, name: &str) -> u64 {
        self.0[name].get(0, 64)
    }
    pub fn bits(&self, name: &str) -> &Bits {
        &self.0[name]
    }
    fn take(model: &Model<'_>, names: &[&str]) -> Self {
        Self(names.iter().map(|&n| (n.into(), model.bits(n))).collect())
    }
}
pub struct Sim<'a> {
    pub model: Model<'a>,
    clocks: Scheduler,
    clock_names: Vec<String>,
    agents: Vec<Box<dyn Agent + 'a>>,
    pub cycles: u64,
    pub max_ps: u64,
}
impl<'a> Sim<'a> {
    pub fn new(model: Model<'a>, clocks: &[(&str, u64, u64)]) -> Self {
        Self {
            model,
            clocks: Scheduler::new(&clocks.iter().map(|&(_, p, o)| (p, o)).collect::<Vec<_>>()),
            clock_names: clocks.iter().map(|&(n, _, _)| n.into()).collect(),
            agents: Vec::new(),
            cycles: 0,
            max_ps: 1_000_000_000_000,
        }
    }
    pub fn agent(&mut self, agent: impl Agent + 'a) {
        self.agents.push(Box::new(agent));
    }
    pub fn ps(&self) -> u64 {
        self.clocks.ps
    }
    fn step_sample(&mut self, names: &[&str]) -> (Edges, Snapshot) {
        let e = self.clocks.next();
        assert!(e.ps <= self.max_ps, "simulation watchdog at {} ps", e.ps);
        self.model.eval();
        for a in &mut self.agents {
            a.drive(&mut self.model, e);
        }
        self.model.eval();
        let before = Snapshot::take(&self.model, names);
        for a in &mut self.agents {
            a.sample(&self.model, e);
        }
        for (i, name) in self.clock_names.iter().enumerate() {
            if e.rises(i) {
                self.model.set(name, 1);
            }
            if e.falls(i) {
                self.model.set(name, 0);
            }
        }
        self.model.eval();
        for a in &mut self.agents {
            a.observe(&self.model, e);
        }
        self.model.dump(e.ps);
        self.cycles += u64::from(e.rises(0));
        (e, before)
    }
    pub fn step(&mut self) -> Edges {
        self.step_sample(&[]).0
    }
    pub fn edge(&mut self, clock: usize, rising: bool) {
        loop {
            let e = self.step();
            if if rising {
                e.rises(clock)
            } else {
                e.falls(clock)
            } {
                return;
            }
        }
    }
    pub fn tick(&mut self, signals: &[&str]) -> (Snapshot, Snapshot) {
        let before = loop {
            let (e, before) = self.step_sample(signals);
            if e.rises(0) {
                break before;
            }
        };
        let after = Snapshot::take(&self.model, signals);
        self.edge(0, false);
        (before, after)
    }
    pub fn idle(&mut self, cycles: u64) {
        for _ in 0..cycles {
            self.tick(&[]);
        }
    }
    pub fn until(&mut self, limit: u64, description: &str, predicate: impl Fn(&Model<'_>) -> bool) {
        for _ in 0..limit {
            self.model.eval();
            if predicate(&self.model) {
                return;
            }
            self.idle(1);
        }
        panic!(
            "{description} timed out at cycle {}, time {} ps",
            self.cycles,
            self.ps()
        );
    }
}
