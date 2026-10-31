//! Test-only native/AudioWorklet fixture. No Shoop application or device adapter.

#[path = "../../../src/rust/shoop_wasm_test_support/src/heap.rs"]
mod allocation_guard;
#[global_allocator]
static ALLOCATOR: allocation_guard::TrackingAllocator = allocation_guard::TrackingAllocator;

mod bridge;
mod controller;
mod node;
pub mod wire;
pub use bridge::Bridge;
pub use controller::Controller;

use rt_runtime::graph::PortCounts;
use rt_runtime::{Budgets, Config, NodeId, PreparedConfig};
pub const QUANTUM: usize = 128;
static INSTANCE_MARKER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub(crate) fn marker() -> u32 {
    INSTANCE_MARKER.load(std::sync::atomic::Ordering::Relaxed)
}
pub(crate) fn set_marker(value: u32) {
    INSTANCE_MARKER.store(value, std::sync::atomic::Ordering::Relaxed);
}
pub fn config() -> PreparedConfig {
    Config {
        sample_rate: 48_000,
        max_frames: QUANTUM,
        audio_inputs: 0,
        audio_outputs: 1,
        midi_inputs: 1,
        midi_outputs: 1,
        max_nodes: 1,
        max_ports: 8,
        max_edges: 4,
        midi_events_per_port: 8,
        midi_bytes_per_port: 64,
        scheduled_events: 8,
        resource_slots: 16,
        resource_pages: 4096,
        page_bytes: 64,
        resource_io_bytes: 1792,
        resource_io_steps: 16,
        command_slots: 4,
        budgets: Budgets {
            commands: 5,
            publications: 5,
            transfer_bytes: 2048,
            validation_steps: 512,
            ..Budgets::default()
        },
    }
    .prepare()
    .expect("fixed valid fixture configuration")
}
pub fn node_id() -> NodeId {
    NodeId::from_parts(0, 1).expect("nonzero fixture generation")
}
pub fn ports() -> PortCounts {
    PortCounts {
        audio_outputs: 2,
        midi_inputs: 1,
        midi_outputs: 1,
        ..PortCounts::default()
    }
}
pub fn guarded<R>(run: impl FnOnce() -> R) -> R {
    if allocation_guard::is_rendering() {
        run()
    } else {
        allocation_guard::assert_no_heap(run)
    }
}
pub fn allocator_canaries() -> bool {
    let (_, counts) = allocation_guard::measure(|| {
        let mut bytes = Vec::with_capacity(1);
        for index in 0..128 {
            bytes.push(std::hint::black_box(index as u8));
        }
        std::hint::black_box(&bytes);
        drop(bytes);
    });
    counts.allocations > 0 && counts.reallocations > 0 && counts.deallocations > 0
}
