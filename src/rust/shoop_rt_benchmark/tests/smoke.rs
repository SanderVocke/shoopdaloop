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

#[shoop_test(no_wasm = "native endpoint-driven DDR comparison fixture")]
fn ddr_probe_preserves_peak_merge_growth_and_retakes_without_long_playback() {
    for media in [Media::Audio, Media::Midi] {
        let fixture = Fixture::with_media(Preset::Tiny, Pattern::Ddr, media);
        assert_eq!(fixture.callbacks, 38);
        let (control, driver) = reference::prepare(media).unwrap();
        let run = runner::run("fpga-reference", fixture, control, Guard(driver));
        assert_eq!(run.status, "completed", "{:?}", run.error);
        assert_eq!(run.coverage["stop_all_verified"], true);
        assert!(run.coverage["replaced_takes"].as_u64().unwrap() >= 64);
        assert_eq!(
            run.coverage["omitted_scenarios"].as_array().unwrap().len(),
            4
        );
        if media.midi() {
            assert_eq!(run.callbacks[33].midi_output_events, 2560);
            assert_eq!(run.coverage["midi_block_boundary_verified"], true);
        } else {
            assert_eq!(run.coverage["cold_block_boundary_verified"], true);
        }
    }
}

#[shoop_test(no_wasm = "native dummy driver and host timing")]
fn compact_stress_checks_required_coverage_and_labels_intentional_omissions() {
    for preset in [Preset::Tiny, Preset::Quick] {
        for media in [Media::Audio, Media::Midi, Media::AudioMidi] {
            let fixture = Fixture::with_media(preset, Pattern::Stress, media);
            assert_eq!(
                fixture.callbacks,
                if preset == Preset::Tiny { 52 } else { 172 }
            );
            let (control, driver) = cpu::prepare(&fixture).unwrap();
            let cpu = runner::run("cpu", fixture.clone(), control, Guard(driver));
            assert_eq!(
                cpu.status,
                if media.midi() {
                    "unsupported"
                } else {
                    "completed"
                },
                "{:?}",
                cpu.error
            );
            let (control, driver) = reference::prepare(media).unwrap();
            let reference = runner::run("fpga-reference", fixture, control, Guard(driver));
            assert_eq!(reference.status, "completed", "{:?}", reference.error);
            assert_eq!(reference.fixture_sha256, cpu.fixture_sha256);
            for run in [cpu, reference]
                .into_iter()
                .filter(|run| run.status == "completed")
            {
                assert_eq!(run.coverage["stop_all_verified"], true);
                assert!(run.coverage["replaced_takes"].as_u64().unwrap() >= 64);
                assert_eq!(run.coverage["cold_block_boundary_verified"], media.audio());
                if media.midi() {
                    assert_eq!(run.coverage["midi_block_boundary_verified"], true);
                    assert_eq!(
                        run.callbacks.iter().map(|row| row.midi_output_events).max(),
                        Some(2560)
                    );
                }
                if preset == Preset::Tiny {
                    assert_eq!(
                        run.coverage["omitted_scenarios"].as_array().unwrap().len(),
                        3
                    );
                } else {
                    assert!(run.coverage["omitted_scenarios"]
                        .as_array()
                        .unwrap()
                        .is_empty());
                    assert!(run.coverage["wraps"].as_u64().unwrap() >= 64);
                    assert!(run.coverage["retired_plan_slots"].as_u64().unwrap() >= 128);
                    assert_eq!(run.coverage["mixed_record_play_verified"], true);
                }
            }
        }
    }
}

#[shoop_test(no_wasm = "host runtime phase accounting")]
fn host_profiling_preserves_the_fixture_and_accounts_for_each_callback() {
    let fixture = Fixture::with_media(Preset::Full, Pattern::Lifecycle, Media::Midi);
    let (control, driver) = reference::prepare(Media::Midi).unwrap();
    let original = runner::run("fpga-reference", fixture.clone(), control, driver);
    shoop_rt_benchmark::host_timing::enable();
    let (control, driver) = reference::prepare(Media::Midi).unwrap();
    let profiled = runner::run("fpga-reference", fixture, control, driver);
    let timing = serde_json::to_value(shoop_rt_benchmark::host_timing::finish().unwrap()).unwrap();
    assert_eq!(profiled.status, "completed");
    assert_eq!(profiled.fixture_sha256, original.fixture_sha256);
    assert_eq!(profiled.coverage, original.coverage);
    assert_eq!(timing["phases"]["fixture.callback"]["calls"], 298);
    assert_eq!(timing["phases"]["fixture.process"]["calls"], 298);
    for phase in [
        "fixture.metadata",
        "fixture.finalization",
        "fixture.teardown",
    ] {
        assert_eq!(timing["phases"][phase]["calls"], 1);
    }
    let leaves = [
        "fixture.command_delivery",
        "fixture.input_stimulus",
        "fixture.process",
        "fixture.endpoint_receive",
        "fixture.validate_and_record",
        "fixture.metadata",
        "fixture.finalization",
        "fixture.teardown",
    ];
    let accounted: u64 = leaves
        .iter()
        .map(|name| timing["phases"][name]["elapsed_ns"].as_u64().unwrap())
        .sum();
    assert!(accounted <= timing["elapsed_ns"].as_u64().unwrap());
}

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
            let fixture = Fixture::with_media(Preset::Full, pattern, media);
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
        Fixture::new(Preset::Full, Pattern::Grab),
        control,
        driver,
    );
    assert_eq!(run.status, "unsupported");
    assert!(run.callbacks.is_empty());
}

#[shoop_test(no_wasm = "native dummy history capture")]
fn audio_grab_fixture_retains_exact_stereo_after_history_wrap_without_heap_work() {
    let fixture = Fixture::new(Preset::Full, Pattern::Grab);
    let (control, driver) = cpu::prepare(&fixture).unwrap();
    let run = runner::run("cpu", fixture, control, Guard(driver));
    assert_eq!(run.status, "completed", "{:?}", run.error);
    assert_eq!(run.coverage["stereo_samples_checked"], true);
    assert!(run
        .events
        .iter()
        .filter(|e| e.label == "grab-all")
        .all(|e| e.commands == 64 && e.same_frame));
    assert_eq!(
        run.events.iter().filter(|e| e.label == "grab-all").count(),
        2
    );
}
