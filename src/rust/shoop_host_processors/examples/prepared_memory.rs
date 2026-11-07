//! Whole prepared host memory, including optional processor banks and host buffers.
use shoop_rt_engine::{testing::OfflineHost, EngineDescription, HostPorts, Segment};
use shoop_rt_protocol::ProcessorSettings;
use shoop_wasm_test_support::heap::{assert_no_heap, measure_memory, TrackingAllocator};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let channels: u8 = args.get(1).map_or(2, |s| s.parse().unwrap());
    let max_frames: usize = args.get(2).map_or(512, |s| s.parse().unwrap());
    let processor_memory: u32 = args.get(3).map_or(256, |s| s.parse().unwrap());
    let d = EngineDescription {
        segments: vec![Segment {
            strips: 16,
            loops: 128,
        }],
        resource_pages: 65536,
        pending_transitions: 256,
        strip_channels: channels,
        max_frames,
        external_dry_wet: true,
        host: HostPorts::numbered(channels as usize, channels as usize, 1),
        processors: ProcessorSettings {
            memory_mb: processor_memory,
            ..ProcessorSettings::default()
        },
        ..EngineDescription::default()
    };
    let pages = d.history_capacity().unwrap().total_resource_pages;
    let started = std::time::Instant::now();
    let ((mut host, reserved), memory) = measure_memory(|| {
        let processors = shoop_host_processors::prepare(&d).unwrap();
        let reserved = processors.reserved_bytes;
        (
            OfflineHost::new_with_processors(&d, processors).unwrap(),
            reserved,
        )
    });
    let preparation_ms = started.elapsed().as_millis();
    assert_no_heap(|| host.process(32)).unwrap();
    let mut stopped = host.into_render().into_stopped();
    stopped.cleanup().unwrap();
    assert_eq!(stopped.resource_usage().free_pages, pages);
    println!("{{\"channels\":{channels},\"sources\":16,\"loops\":128,\"quantum\":32,\"host_max\":{max_frames},\"processor_budget_mb\":{processor_memory},\"processor_reserved\":{reserved},\"prepared_peak\":{},\"prepared_retained\":{},\"preparation_ms\":{preparation_ms},\"guarded_callback\":true,\"all_pages_recovered\":true}}", memory.peak, memory.live);
}
