//! Public native stereo/MIDI composition scenario with complete deterministic oracles.
#![forbid(unsafe_code)]
mod one_track {
    pub mod scenario;
}
use one_track::scenario::{run, Partition};
use shoop_wasm_test_support::heap::TrackingAllocator;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn main() {
    one_track::scenario::verify_stopped_input();
    let inject = std::env::args().find_map(|arg| arg.strip_prefix("--inject=").map(str::to_owned));
    if let Some(kind) = &inject {
        assert!(
            ["sample", "packet", "outcome"].contains(&kind.as_str()),
            "unknown injection"
        );
    }
    for mode in [Partition::Whole, Partition::Single, Partition::Irregular] {
        for alternate in [false, true] {
            let result = run(mode, alternate, inject.as_deref());
            println!("one_track {mode:?} alternate={alternate}: frames={} packets={} copied_media_bytes={} callbacks={}",
                result.frames,result.packets,result.copied_bytes,result.callbacks);
        }
    }
}
