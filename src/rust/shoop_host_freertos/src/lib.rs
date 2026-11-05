#![no_std]
extern crate alloc;

#[cfg(target_os = "none")]
mod embedded;
#[cfg(all(target_os = "none", feature = "fixture"))]
pub use embedded::{fixture_stats, fixture_step};
#[cfg(target_os = "none")]
pub use embedded::{render_step, render_xruns, run};

pub const SAMPLE_RATE: u32 = 48_000;
pub const PROCESSING_QUANTUM: usize = 32;
pub const QUANTUM: usize = 256;
pub const MIDI_PORTS: usize = if cfg!(feature = "usb-midi") { 16 } else { 1 };
pub const MIDI_BLOCK_BYTES: usize = if cfg!(feature = "usb-midi") {
    4096
} else {
    2048
};
include!(concat!(env!("OUT_DIR"), "/memory.rs"));
