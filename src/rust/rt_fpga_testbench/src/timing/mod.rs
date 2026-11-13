//! Functional storage is separate from native DRAM admission and service timing.
pub mod dram;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Token {
    pub generation: u64,
    pub id: u64,
}
pub trait Timing {
    fn native_bytes(&self) -> u64;
    fn advance(&mut self);
    fn submit(&mut self, address: u64, write: bool) -> Option<Token>;
    fn ready(&self, token: Token) -> bool;
    fn reset(&mut self);
    fn report(&mut self, finalize: bool) -> Value;
    fn bus_snapshot(&self) -> Option<dram::Snapshot> {
        None
    }
    fn bus_utilization(&self, _start: dram::Snapshot, _end: dram::Snapshot) -> Option<Value> {
        None
    }
}
pub struct Fixed {
    cycle: u64,
    delay: u64,
    generation: u64,
    next: u64,
    due: BTreeMap<Token, u64>,
}
impl Fixed {
    pub fn new(delay: u64) -> Self {
        Self {
            cycle: 0,
            delay,
            generation: 0,
            next: 0,
            due: BTreeMap::new(),
        }
    }
}
impl Timing for Fixed {
    fn native_bytes(&self) -> u64 {
        16
    }
    fn advance(&mut self) {
        self.cycle += 1;
    }
    fn submit(&mut self, _address: u64, _write: bool) -> Option<Token> {
        let token = Token {
            generation: self.generation,
            id: self.next,
        };
        self.next += 1;
        self.due.insert(token, self.cycle + self.delay);
        Some(token)
    }
    fn ready(&self, token: Token) -> bool {
        self.due.get(&token).is_some_and(|&due| self.cycle >= due)
    }
    fn reset(&mut self) {
        self.generation += 1;
        self.due.clear();
    }
    fn report(&mut self, _finalize: bool) -> Value {
        json!({"cycles":self.cycle,"delay":self.delay,"native_bytes":16})
    }
}
pub fn summary(values: &[u64]) -> Value {
    let mut histogram = BTreeMap::new();
    for &value in values {
        *histogram.entry(value).or_insert(0) += 1;
    }
    histogram_summary(&histogram)
}
pub fn histogram_summary(histogram: &BTreeMap<u64, u64>) -> Value {
    let count: u64 = histogram.values().sum();
    if count == 0 {
        return json!({"count":0});
    }
    let at = |rank| {
        let mut n = 0;
        for (&v, &c) in histogram {
            n += c;
            if n >= rank {
                return v;
            }
        }
        unreachable!()
    };
    let mean = histogram
        .iter()
        .map(|(&v, &n)| v as f64 * n as f64)
        .sum::<f64>()
        / count as f64;
    let variance = histogram
        .iter()
        .map(|(&v, &n)| (v as f64 - mean).powi(2) * n as f64)
        .sum::<f64>()
        / count as f64;
    json!({"count":count,"mean":mean,"median":(at(count.div_ceil(2))+at(count/2+1)) as f64/2.0,"stddev":variance.sqrt(),"min":at(1),"max":at(count),"p95":at((count*95).div_ceil(100)),"p99":at((count*99).div_ceil(100))})
}
