#![no_std]
extern crate alloc;

#[cfg(any(target_os = "none", test))]
mod reboot;
#[cfg(any(target_os = "none", test))]
mod runtime_restart;

#[cfg(target_os = "none")]
mod embedded;
#[cfg(all(target_os = "none", feature = "native-i2s"))]
pub use embedded::native_pcm_step;
#[cfg(all(target_os = "none", feature = "fpga"))]
pub use embedded::run_fpga;
#[cfg(all(target_os = "none", feature = "fixture"))]
pub use embedded::{fixture_stats, fixture_step};
#[cfg(target_os = "none")]
pub use embedded::{render_step, render_xruns, run};

pub const SAMPLE_RATE: u32 = 48_000;
pub const PROCESSING_QUANTUM: usize = 32;
pub const QUANTUM: usize = if cfg!(feature = "native-i2s") {
    PROCESSING_QUANTUM
} else {
    256
};
const AUDIO_INPUTS: usize = if cfg!(feature = "native-i2s") { 1 } else { 2 };
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
        host: shoop_rt_engine::HostPorts::numbered(AUDIO_INPUTS, 2, MIDI_PORTS),
        max_frames: QUANTUM,
        midi_bytes: MIDI_BLOCK_BYTES,
        resource_pages: RECORDING_BYTES / 4096,
        segments: alloc::vec![shoop_rt_engine::Segment {
            strips: ENGINE_TRACKS,
            loops: ENGINE_LOOPS,
        }],
        pending_transitions: (2 * ENGINE_LOOPS as usize).max(128),
        control_lead_frames: QUANTUM as u32,
        smoothing_limit_frames: 4800,
        ..Default::default()
    }
}
