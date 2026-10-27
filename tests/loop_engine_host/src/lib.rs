//! Independent test-only loop host. Device/application adapters are not dependencies.
//!
//! The initial native scenario is not yet an AudioWorklet bridge.
#[path = "../../../src/rust/shoop_wasm_test_support/src/heap.rs"]
mod allocation_guard;
#[global_allocator]
static ALLOCATOR: allocation_guard::TrackingAllocator = allocation_guard::TrackingAllocator;

mod scenario;
pub use scenario::{run, Report};

pub(crate) fn guarded<R>(run: impl FnOnce() -> R) -> R {
    allocation_guard::assert_no_heap(run)
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
