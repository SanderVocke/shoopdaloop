#![cfg(all(feature = "rtl-sim", target_os = "linux"))]
use shoop_host::rtl_sim;
use shoop_rt_host_native::{DriverConfig, DummyConfig, DummyMode};
use shoop_rt_protocol::{DspMeasurement, EngineMessage, Telemetry};
use shoop_wasm_test_support::shoop_test;
use std::time::{Duration, Instant};
#[shoop_test(no_wasm = "native simulator shared Dummy driver")]
fn all_dummy_modes_and_partial_controlled_requests_use_the_rtl_owner() {
    for mode in [
        DummyMode::Automatic,
        DummyMode::Unpaced,
        DummyMode::Controlled,
    ] {
        let mut started = rtl_sim::start_driver(
            DriverConfig::Dummy(DummyConfig {
                mode,
                sample_rate: 1000,
                buffer_frames: 16,
                ..Default::default()
            }),
            16,
            1,
        )
        .unwrap();
        started.client.connect().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        if mode == DummyMode::Controlled {
            let demand = started.driver.dummy().unwrap();
            for frames in [3, 5, 8, 19, 13] {
                demand.request_frames(frames);
                assert!(demand.wait_rendered(Duration::from_secs(5)));
            }
            assert_eq!(started.driver.host().status().next_frame, 48);
        } else {
            while started.driver.host().status().next_frame < 48 {
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
        }
        loop {
            if let Some(EngineMessage::Telemetry(Telemetry::Engine {
                dsp: Some(sample), ..
            })) = started.client.poll()
            {
                let DspMeasurement::FpgaQuantum {
                    cycles,
                    clock_hz,
                    frames,
                    sample_rate,
                } = sample
                else {
                    panic!("expected RTL quantum timing, got {sample:?}");
                };
                assert!(cycles > 0);
                assert_eq!((clock_hz, frames, sample_rate), (150_000_000, 16, 1000));
                assert_eq!(sample.microseconds(), Some(cycles as f64 / 150.));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "missing RTL measurement in {mode:?}"
            );
            std::thread::yield_now();
        }
    }
}
#[cfg(feature = "cpal")]
#[allow(dead_code)]
#[path = "../../shoop_rt_host_native/tests/support/cpal_mock.rs"]
mod cpal_mock;
#[cfg(feature = "cpal")]
#[shoop_test(no_wasm = "native CPAL mock and serialized simulator")]
fn cpal_callback_priming_is_silent_and_does_not_wait_for_simulation() {
    use shoop_audio_drivers as drivers;
    let output = cpal_mock::MockDevice::output("RTL out", 2);
    let input = cpal_mock::MockDevice::input("RTL in", 1);
    let devices = cpal_mock::MockHost::new(vec![output.clone(), input.clone()]);
    let started = drivers::start_cpal(
        &devices,
        &drivers::CpalConfig {
            buffer_frames: Some(32),
            sample_rate: Some(48000),
            ..Default::default()
        },
        &rtl_sim::template(16, 1, 64),
        drivers::MidiDevices::default(),
    )
    .unwrap();
    input.stream().unwrap().capture(&[0.25; 32]);
    let samples = output.stream().unwrap().render(32);
    assert!(samples.iter().all(|s| *s == 0.));
    started.host.stop();
}

#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;
#[cfg(feature = "jack")]
#[shoop_test(no_wasm = "native JACK server and simulator lifecycle")]
fn direct_jack_uses_the_same_rtl_factory_and_fixed_media_ports() {
    let started = match rtl_sim::start_driver(
        DriverConfig::Jack(shoop_rt_host_native::JackConfig {
            client_name: format!("marlin-driver-{}", std::process::id()),
        }),
        16,
        1,
    ) {
        Ok(s) => s,
        Err(_) if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() => return,
        Err(e) => panic!("JACK start: {e:#}"),
    };
    assert_eq!(
        started.driver.host().status().driver.kind,
        shoop_rt_protocol::DriverKind::Jack
    );
    let layout = started.driver.host().layout();
    assert_eq!(layout.ports.audio_inputs.len(), 1);
    assert_eq!(layout.ports.audio_outputs.len(), 2);
    drop(started);
}
