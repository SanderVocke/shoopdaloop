#![forbid(unsafe_code)]
#[cfg(not(target_arch = "wasm32"))]
#[path = "latency_probe/current.rs"]
mod current;
#[path = "latency_probe/scenario.rs"]
mod scenario;
use scenario::{run, Capture, Partition, Signal};
use shoop_wasm_test_support::heap;
#[global_allocator]
static ALLOCATOR: heap::TrackingAllocator = heap::TrackingAllocator;
fn main() {
    for signal in [Signal::Ramp, Signal::Impulses] {
        for capture in [Capture::Raw, Capture::Tail, Capture::Margins] {
            let evidence = run(signal, capture, Partition::Whole);
            for partition in [Partition::Single, Partition::Irregular] {
                assert_eq!(run(signal, capture, partition), evidence);
            }
            println!(
                "case,{signal:?},{capture:?},interval={:?},physical_frames={},capture_frames={}",
                capture.interval(),
                evidence.physical_wet.len(),
                evidence.raw_capture.len()
            );
            for (at, wet) in evidence.physical_wet.iter().enumerate() {
                println!(
                    "physical,{at},input={},wet={wet}",
                    scenario::dry(signal, at as u64)
                );
            }
            for (i, values) in evidence.raw_capture.iter().enumerate() {
                println!(
                    "capture,{},dry={},wet={}",
                    capture.interval().start + i as u64,
                    values[0],
                    values[1]
                );
            }
            for (at, bytes) in &evidence.output_markers {
                println!("marker,{at},{bytes:?}");
            }
            println!("captured_midi_bytes,{:?}", evidence.raw_midi);
            for (offset, dry_start, wet_start) in [(0, 4, 7), (-2, 2, 5)] {
                println!(
                    "proposed_window,offset={offset},dry={:?},wet={:?}",
                    evidence.window(0, dry_start, dry_start + 8),
                    evidence.window(1, wet_start, wet_start + 8)
                );
            }
            #[cfg(not(target_arch = "wasm32"))]
            if capture == Capture::Raw {
                current::verify(&evidence, true);
            }
        }
    }
    println!("verified 18 physical cases; logical windows remain separate, not replacement media");
}
