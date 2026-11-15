//! Clock-wide protocol guards, including while a transaction driver is waiting.
use crate::{bits::Bits, clock::Edges, model::Model, sim::Agent};
pub struct ReadyValid {
    valid: String,
    ready: String,
    payload: Vec<String>,
    held: Option<Vec<Bits>>,
}
impl ReadyValid {
    pub fn new(valid: &str, ready: &str, payload: &[&str]) -> Self {
        Self {
            valid: valid.into(),
            ready: ready.into(),
            payload: payload.iter().map(|s| (*s).into()).collect(),
            held: None,
        }
    }
}
impl Agent for ReadyValid {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if !e.rises(0) {
            return;
        }
        if m.get("reset") != 0 {
            self.held = None;
            return;
        }
        let valid = m.get(&self.valid) != 0;
        let ready = m.get(&self.ready) != 0;
        if self.held.is_some() || (valid && !ready) {
            let payload: Vec<_> = self.payload.iter().map(|n| m.bits(n)).collect();
            if let Some(held) = &self.held {
                assert!(valid, "{}: stalled valid disappeared", self.valid);
                assert_eq!(&payload, held, "{}: stalled payload changed", self.valid);
            }
            self.held = (valid && !ready).then_some(payload);
        }
    }
}
pub struct Exclusion {
    when: String,
    forbidden: Vec<String>,
}
impl Exclusion {
    pub fn new(when: &str, forbidden: &[&str]) -> Self {
        Self {
            when: when.into(),
            forbidden: forbidden.iter().map(|s| (*s).into()).collect(),
        }
    }
}
impl Agent for Exclusion {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0) && m.get("reset") == 0 && m.get(&self.when) != 0 {
            for name in &self.forbidden {
                assert_eq!(m.get(name), 0, "{} asserted while {}", name, self.when);
            }
        }
    }
}
pub struct Bound {
    signal: String,
    limit: u64,
}
impl Bound {
    pub fn new(signal: &str, limit: u64) -> Self {
        Self {
            signal: signal.into(),
            limit,
        }
    }
}
impl Agent for Bound {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0) && m.get("reset") == 0 {
            assert!(
                m.get(&self.signal) <= self.limit,
                "{} exceeded physical capacity {}",
                self.signal,
                self.limit
            );
        }
    }
}
