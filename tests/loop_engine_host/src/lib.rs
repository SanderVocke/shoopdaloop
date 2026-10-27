//! Independent test-only loop host. Device/application adapters are not dependencies.
//!
//! Native scenario and bounded command bridge; AudioWorklet integration is separate.
#[path = "../../../src/rust/shoop_wasm_test_support/src/heap.rs"]
mod allocation_guard;
#[global_allocator]
static ALLOCATOR: allocation_guard::TrackingAllocator = allocation_guard::TrackingAllocator;

mod bridge;
#[cfg(test)]
mod bridge_tests;
mod browser;
mod browser_graph;
mod browser_graph_client;
mod client;
mod graph_bridge;
pub mod graph_transfer;
pub mod graph_wire;
mod journal;
mod native;
mod scenario;
pub mod wire;
pub use bridge::Bridge;
#[cfg(not(target_arch = "wasm32"))]
pub use browser::native_graph::{run_graph, run_graph_two_thread, GraphReport};
pub use browser::{BrowserBridge, BrowserClient, HEAP_FAILURE};
pub use browser_graph::BrowserGraphReceiver;
pub use browser_graph_client::BrowserGraphClient;
pub use client::Client;
#[cfg(not(target_arch = "wasm32"))]
pub use native::run_two_thread;
pub use native::run_wire;
pub use scenario::{run, Report};

pub(crate) fn guarded<R>(run: impl FnOnce() -> R) -> R {
    if allocation_guard::is_rendering() {
        run()
    } else {
        allocation_guard::assert_no_heap(run)
    }
}

/// Positive control for the shared allocator instrumentation, outside rendering.
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
