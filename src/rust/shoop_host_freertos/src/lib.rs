#![no_std]
extern crate alloc;

#[cfg(target_os = "none")]
mod embedded;
#[cfg(all(target_os = "none", feature = "fixture"))]
pub use embedded::{fixture_stats, fixture_step};
#[cfg(target_os = "none")]
pub use embedded::{render_step, render_xruns, run};

pub const SAMPLE_RATE: u32 = 48_000;
pub const QUANTUM: usize = 256;
pub const RECORDING_BYTES: usize = 128 * 1024 * 1024;
pub const MEDIA_BYTES: u64 = 64 * 1024 * 1024;
