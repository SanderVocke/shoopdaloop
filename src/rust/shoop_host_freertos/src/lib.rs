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

pub fn engine_description() -> shoop_rt_engine::EngineDescription {
    shoop_rt_engine::EngineDescription {
        history: INPUT_HISTORY,
        quantum: PROCESSING_QUANTUM,
        host: shoop_rt_engine::HostPorts::numbered(2, 2, MIDI_PORTS),
        max_frames: QUANTUM,
        midi_bytes: MIDI_BLOCK_BYTES,
        resource_pages: RECORDING_BYTES / 4096,
        segments: alloc::vec![shoop_rt_engine::Segment {
            strips: ENGINE_TRACKS,
            loops: ENGINE_LOOPS,
        }],
        control_lead_frames: QUANTUM as u32,
        smoothing_limit_frames: 4800,
        ..Default::default()
    }
}
