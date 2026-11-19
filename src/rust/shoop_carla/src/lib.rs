#![cfg(not(target_arch = "wasm32"))]

//! Native Carla worker support shared by the host implementations.

mod carla_completion;
pub mod carla_shared_memory;

#[cfg(test)]
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;
