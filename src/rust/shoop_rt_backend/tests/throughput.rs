//! Reading a long loop's content: the number of poll rounds a 10 s stereo loop at
//! 48 kHz needs when the application polls every 16 ms.

mod support;

use std::time::Duration;

use shoop_backend::{Backend, BackendLoopMode, BackendPortRole, DirectTrackRequest};
use shoop_rt_engine::{EngineDescription, HostPorts, Segment};
use shoop_wasm_test_support::shoop_test;
use support::*;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

const SAMPLE_RATE: u32 = 48_000;
const CALLBACK: usize = 256;
/// Frames rendered between two polls 16 ms apart.
const ROUND: usize = 768;
/// Rounds in 2 s.
const MAX_ROUNDS: usize = 125;

#[shoop_test]
fn a_ten_second_stereo_loop_is_read_within_two_seconds_of_polling() {
    let description = EngineDescription {
        sample_rate: SAMPLE_RATE,
        max_frames: CALLBACK,
        host: HostPorts::numbered(2, 2, 1),
        segments: vec![Segment {
            strips: 1,
            loops: 1,
        }],
        master_connections: Vec::new(),
        ..EngineDescription::default()
    };
    let mut backend = stack(&description);
    backend.clock_mut().capture = |_| 0.5;
    let created = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "long".to_owned(),
            audio_channels: 2,
            midi: false,
            initial_loops: 1,
        })
        .unwrap();
    for port in created
        .ports
        .iter()
        .filter(|port| port.role == BackendPortRole::AudioInput)
    {
        backend
            .set_port_connected(port.id, "audio_in_1", true)
            .unwrap();
    }
    let loop_id = created.loops[0];
    backend
        .transition_loop(loop_id, BackendLoopMode::Recording, None)
        .unwrap();
    backend.wait_idle();
    backend.advance(Duration::from_secs(10));
    backend
        .transition_loop(loop_id, BackendLoopMode::Playing, None)
        .unwrap();
    backend.wait_idle();
    let length = backend.poll().unwrap().loops[&loop_id].length as usize;
    assert!(length >= 10 * SAMPLE_RATE as usize, "length {length}");

    let mut rounds = 0;
    let audio = loop {
        if let Some(audio) = backend.loop_audio_data(loop_id).unwrap() {
            break audio;
        }
        rounds += 1;
        assert!(rounds <= 4 * MAX_ROUNDS, "the read did not finish");
        backend.clock_mut().render(ROUND);
    };
    eprintln!("read {length} stereo frames in {rounds} poll rounds");
    assert!(rounds <= MAX_ROUNDS, "{rounds} poll rounds");
    assert_eq!(audio.len(), 2);
    for channel in &audio {
        assert_eq!(channel.len(), length);
        assert!(channel.iter().all(|sample| *sample == 0.5));
    }
}
