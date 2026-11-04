//! Fixed-quantum audio/MIDI nodes with internal block processing and stream delay.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod timing;

pub use timing::{PreparedTiming, Timing, MAX_BLOCK_FRAMES, MAX_LATENCY_FRAMES};
