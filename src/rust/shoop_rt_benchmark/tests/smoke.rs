use shoop_rt_benchmark::{
    cpu,
    fixture::{Fixture, Media, Pattern, Preset},
    reference,
    runner::{self, Driver, Measurement},
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

struct Guard<D>(D);
impl<D: Driver> Driver for Guard<D> {
    fn begin_fixture(&mut self) {
        self.0.begin_fixture();
    }
    fn next_frame(&self) -> u64 {
        self.0.next_frame()
    }
    fn fill_input(&mut self) {
        self.0.fill_input();
    }
    fn process(&mut self) -> anyhow::Result<Measurement> {
        assert_no_heap(|| self.0.process())
    }
    fn output(&self, channel: usize) -> &[f32] {
        self.0.output(channel)
    }
    fn midi_output(&self) -> &[shoop_rt_benchmark::fixture::MidiPacket] {
        self.0.midi_output()
    }
    fn midi_latency_frames(&self) -> usize {
        self.0.midi_latency_frames()
    }
    fn latency_frames(&self) -> usize {
        self.0.latency_frames()
    }
    fn supports_grab(&self) -> bool {
        self.0.supports_grab()
    }
    fn metadata(&self) -> serde_json::Value {
        self.0.metadata()
    }
}

#[shoop_test(no_wasm = "native dummy driver and host timing")]
fn common_fixture_checks_stereo_samples_and_simultaneous_transitions_on_both_rust_backends() {
    for media in [Media::Audio, Media::Midi, Media::AudioMidi] {
        for pattern in [Pattern::Lifecycle, Pattern::Mixed] {
            let fixture = Fixture::with_media(Preset::Smoke, pattern, media);
            let (control, driver) = cpu::prepare(&fixture).unwrap();
            let cpu = runner::run("cpu", fixture.clone(), control, Guard(driver));
            assert!(
                cpu.status == "completed" || (media.midi() && cpu.status == "unsupported"),
                "{}",
                cpu.error.as_deref().unwrap_or("unknown error")
            );
            if cpu.status == "unsupported" {
                assert!(cpu.callbacks.is_empty());
                assert!(cpu.events.is_empty());
            }
            let (control, driver) = reference::prepare(media).unwrap();
            let reference = runner::run("fpga-reference", fixture, control, Guard(driver));
            assert_eq!(
                reference.status,
                "completed",
                "{}",
                reference.error.as_deref().unwrap_or("unknown error")
            );
            assert_eq!(cpu.fixture_sha256, reference.fixture_sha256);
            assert_eq!(reference.metadata["chunk_frames"], 32);
            assert_eq!(reference.metadata["block_chunks"], 32);
            for run in [cpu, reference]
                .into_iter()
                .filter(|run| run.status == "completed")
            {
                assert_eq!(run.events[0].commands, 64);
                assert_eq!(run.events[0].groups, 2);
                assert!(run.events.iter().all(|event| event.same_frame));
                assert!(run.coverage["wraps"].as_u64().unwrap() >= 64);
                assert_eq!(run.coverage["cold_block_boundary_verified"], media.audio());
                if media.midi() {
                    assert_eq!(run.coverage["midi_block_boundary_verified"], true);
                    assert_eq!(run.coverage["midi_bytes_timestamps_order_checked"], true);
                    assert!(run.coverage["midi_events_checked"].as_u64().unwrap() > 256);
                    assert!(
                        run.coverage["midi_silent_playback_callbacks"]
                            .as_u64()
                            .unwrap()
                            > 0
                    );
                }
            }
        }
    }
}

#[shoop_test(no_wasm = "native dummy driver and host timing")]
fn unsupported_grabs_do_not_turn_into_a_cheaper_workload() {
    let (control, driver) = reference::prepare(Media::Audio).unwrap();
    let run = runner::run(
        "fpga-reference",
        Fixture::new(Preset::Smoke, Pattern::Grab),
        control,
        driver,
    );
    assert_eq!(run.status, "unsupported");
    assert!(run.callbacks.is_empty());
}
