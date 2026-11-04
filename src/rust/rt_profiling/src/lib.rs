//! Bounded timestamp capture with statistics computed by the receiving thread.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

mod collector;
#[cfg(target_has_atomic = "64")]
mod recorder;
#[cfg(not(target_has_atomic = "64"))]
#[path = "unsupported.rs"]
mod recorder;
pub use collector::{Collector, Interval, Report, Row, Statistics, View};
pub use recorder::{prepare, Control, Reader, Recorder};

#[derive(Clone, Copy)]
pub struct Clock {
    pub now: fn() -> u64,
    pub resolution_ns: u64,
}

pub const CHUNK_SAMPLES: usize = 24;
pub const MAX_SAMPLES: usize = 65_536;
pub const PHASE: u64 = 1 << 63;
pub const LAYOUT: u64 = 1 << 62;
pub const CALLBACK: u32 = 1;
pub const FAULT: u32 = 2;
pub const CAPTURE_END: u32 = 4;
pub const TRUNCATED: u32 = 8;
pub const HOST: u32 = 16;
pub const HOST_XRUN: u32 = 1;
pub const DEADLINE: u32 = 2;

#[derive(Clone, Copy, Debug)]
#[repr(u64)]
pub enum Phase {
    CallbackStart,
    Commands,
    TickStart,
    StateSettled,
    GraphStart,
    OutputRouting,
    StateAdvanced,
    CallbackEnd,
    HostInput,
    HostOutput,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    pub key: u64,
    pub at: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Status {
    pub generation: u64,
    pub requested: bool,
    pub active: bool,
    pub available: bool,
    pub resolution_ns: u64,
    pub dropped: u64,
    pub skipped_captures: u64,
    pub high_water: u64,
    pub storage_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Packet {
    pub generation: u64,
    pub capture: u64,
    pub sequence: u64,
    pub callback: u64,
    pub callback_frame: u64,
    pub frame: u64,
    pub epoch: u64,
    pub frames: u32,
    pub start_ns: u64,
    pub end_ns: u64,
    pub index: u32,
    pub total: u32,
    pub len: u8,
    pub flags: u32,
    pub trigger: u32,
    pub samples: [Sample; CHUNK_SAMPLES],
}

impl Packet {
    pub fn valid(&self) -> bool {
        usize::from(self.len) <= CHUNK_SAMPLES
            && self.total as usize <= MAX_SAMPLES
            && self
                .index
                .checked_add(u32::from(self.len))
                .is_some_and(|end| end <= self.total)
            && (self.len != 0 || self.total == 0)
            && self.start_ns <= self.end_ns
    }
}
