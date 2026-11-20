#![cfg(not(target_arch = "wasm32"))]

//! Native Carla worker support shared by the host implementations.

#[cfg(feature = "buffered")]
pub mod buffered;
mod carla_completion;
pub mod carla_shared_memory;
#[cfg(feature = "native")]
pub mod child;
#[cfg(feature = "native")]
pub mod native;
#[cfg(feature = "native")]
pub mod processor;
#[cfg(feature = "buffered")]
pub mod service;
#[cfg(feature = "native")]
pub mod state;
#[cfg(feature = "native")]
pub mod subprocess;

#[cfg(test)]
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;
