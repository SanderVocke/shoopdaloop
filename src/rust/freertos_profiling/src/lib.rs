//! Portable FreeRTOS task samples; no kernel or platform dependency.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
use alloc::vec::Vec;
use profiling_protocol::{Name, StreamId};
use serde::{Deserialize, Serialize};
pub const SOURCE: u32 = 2;
pub const REPORT_TYPE: u32 = 3;
pub const SCHEMA: u16 = 1;
pub const MAX_TASKS: usize = 32;
pub fn stream(epoch: u64) -> StreamId {
    StreamId {
        source: SOURCE,
        epoch,
        report_type: REPORT_TYPE,
        schema: SCHEMA,
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub name: Name,
    pub state: u8,
    pub priority: u32,
    pub base_priority: Option<u32>,
    pub runtime: Option<u64>,
    pub stack_free_bytes: Option<u64>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub generation: u64,
    pub clock_domain: u64,
    pub captured_us: u64,
    pub collection_us: u64,
    pub total_runtime: Option<u64>,
    pub counter_bits: u8,
    pub counter_hz: u64,
    pub complete: bool,
    pub tasks: Vec<Task>,
}
impl Snapshot {
    pub fn valid(&self) -> bool {
        self.generation != 0
            && self.clock_domain != 0
            && self.tasks.len() <= MAX_TASKS
            && matches!(self.counter_bits, 0 | 32 | 64)
            && (self.total_runtime.is_none() || (self.counter_bits != 0 && self.counter_hz != 0))
            && self.tasks.iter().enumerate().all(|(i, t)| {
                t.id != 0
                    && t.name.valid()
                    && t.state <= 5
                    && self.tasks[..i].iter().all(|old| old.id != t.id)
            })
    }
    pub fn cpu_percent(&self, previous: &Self, task: &Task) -> Option<f64> {
        if !self.valid()
            || !previous.valid()
            || !self.complete
            || !previous.complete
            || self.generation != previous.generation
            || self.clock_domain != previous.clock_domain
            || self.counter_bits != previous.counter_bits
            || self.counter_hz != previous.counter_hz
            || self.captured_us <= previous.captured_us
        {
            return None;
        }
        let old = previous.tasks.iter().find(|old| old.id == task.id)?;
        let delta = |now: u64, old: u64| -> Option<u64> {
            if self.counter_bits == 32 {
                let period_us = ((1u128 << 32) * 1_000_000 / u128::from(self.counter_hz)) as u64;
                if self.captured_us - previous.captured_us >= period_us {
                    return None;
                }
                Some(u64::from((now as u32).wrapping_sub(old as u32)))
            } else {
                now.checked_sub(old)
            }
        };
        let total = delta(self.total_runtime?, previous.total_runtime?)?;
        let used = delta(task.runtime?, old.runtime?)?;
        (total > 0 && used <= total).then(|| 100. * used as f64 / total as f64)
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    baseline_valid: bool,
    pub sample: Option<Snapshot>,
    pub cpu_percent: Vec<Option<f64>>,
}
impl View {
    pub fn pause(&mut self) {
        self.baseline_valid = false;
    }
    pub fn receive(&mut self, sample: Snapshot) {
        self.cpu_percent = sample
            .tasks
            .iter()
            .map(|task| {
                self.sample
                    .as_ref()
                    .filter(|_| self.baseline_valid)
                    .and_then(|old| sample.cpu_percent(old, task))
            })
            .collect();
        self.sample = Some(sample);
        self.baseline_valid = true;
    }
}
