#![forbid(unsafe_code)]
#[cfg(not(target_arch = "wasm32"))]
#[path = "measure_service/current.rs"]
mod current;
#[cfg(not(target_arch = "wasm32"))]
#[path = "measure_service/host.rs"]
mod host;
use shoop_wasm_test_support::heap;
#[global_allocator]
static ALLOCATOR: heap::TrackingAllocator = heap::TrackingAllocator;
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    host::main();
}
#[cfg(target_arch = "wasm32")]
fn main() {
    panic!("native-only measurement host");
}
