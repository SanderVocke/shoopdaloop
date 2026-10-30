//! How many application polls structural changes take to show in the snapshot: the
//! offline host renders 256-frame callbacks at 48 kHz and the application polls every
//! 16 ms of frames.

mod support;

use shoop_backend::{
    Backend, BackendLoopId, BackendLoopMode, BackendPortRole, BackendSnapshot, DirectTrackRequest,
    MASTER_BUS_CHANNEL_IDS,
};
use shoop_rt_engine::{EngineDescription, HostPorts, Segment};
use shoop_wasm_test_support::shoop_test;
use support::*;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

/// Frames between two polls 16 ms apart.
const POLL: usize = 768;

fn description() -> EngineDescription {
    EngineDescription {
        sample_rate: 48_000,
        max_frames: 256,
        host: HostPorts::numbered(2, 2, 1),
        segments: vec![Segment {
            strips: 16,
            loops: 128,
        }],
        master_connections: Vec::new(),
        ..EngineDescription::default()
    }
}

fn track(backend: &mut Stack, loops: usize) -> shoop_backend::BackendTrackCreation {
    backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "track".to_owned(),
            audio_channels: 2,
            midi: false,
            initial_loops: loops,
        })
        .unwrap()
}

/// Polls every 16 ms of frames until `done`, returning the number of polls.
fn polls_until(backend: &mut Stack, done: impl Fn(&BackendSnapshot) -> bool) -> usize {
    for polls in 1..=1000 {
        backend.clock_mut().render(POLL);
        let snapshot = backend.poll().unwrap();
        assert!(
            snapshot.mutation_failures.is_empty(),
            "{:?}",
            snapshot.mutation_failures
        );
        if done(&snapshot) {
            return polls;
        }
    }
    panic!("the change never showed");
}

#[shoop_test]
fn a_track_with_eight_loops_shows_within_six_polls() {
    let mut backend = stack(&description());
    track(&mut backend, 1);
    backend.wait_idle();
    let created = track(&mut backend, 8);
    let polls = polls_until(&mut backend, |snapshot| {
        snapshot.tracks.contains_key(&created.track_id)
            && created
                .loops
                .iter()
                .all(|loop_id| snapshot.loops.contains_key(loop_id))
    });
    eprintln!("a track with 8 loops: {polls} polls");
    assert!(polls <= 6, "{polls} polls");
}

#[shoop_test]
fn an_immediate_record_after_clearing_120_sync_sources_takes_effect_within_six_polls() {
    let mut backend = stack(&description());
    let sync = track(&mut backend, 1).loops[0];
    let loops: Vec<BackendLoopId> = (0..15).flat_map(|_| track(&mut backend, 8).loops).collect();
    for loop_id in &loops {
        backend.set_loop_sync_source(*loop_id, Some(sync)).unwrap();
    }
    backend.wait_idle();
    for loop_id in &loops {
        backend.set_loop_sync_source(*loop_id, None).unwrap();
    }
    let recorded = loops[7];
    backend
        .transition_loop(recorded, BackendLoopMode::Recording, None)
        .unwrap();
    let polls = polls_until(&mut backend, |snapshot| {
        snapshot.loops[&recorded].mode == BackendLoopMode::Recording
    });
    eprintln!("an immediate record after 120 sync changes: {polls} polls");
    assert!(polls <= 6, "{polls} polls");
    backend.wait_idle();
}

#[shoop_test]
fn six_connections_and_routes_show_within_three_polls() {
    let mut backend = stack(&description());
    let tracks = [track(&mut backend, 1), track(&mut backend, 1)];
    backend.wait_idle();
    let mut links = Vec::new();
    let mut routes = Vec::new();
    for created in &tracks {
        let inputs = created
            .ports
            .iter()
            .filter(|port| port.role == BackendPortRole::AudioInput);
        for (port, host) in inputs.zip(["audio_in_1", "audio_in_2"]) {
            backend.set_port_connected(port.id, host, true).unwrap();
            links.push((port.id, host));
        }
        let output = created
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::AudioOutput)
            .unwrap();
        backend
            .set_mixer_route(output.id, MASTER_BUS_CHANNEL_IDS[0], true)
            .unwrap();
        routes.push(output.id);
    }
    let polls = polls_until(&mut backend, |snapshot| {
        links.iter().all(|(port, host)| {
            snapshot
                .connections
                .confirmed_links
                .iter()
                .any(|link| link.application_port_id == *port && link.host_port_id == *host)
        }) && routes.iter().all(|port| {
            snapshot
                .mixer
                .confirmed_links
                .iter()
                .any(|link| link.source_port_id == *port)
        })
    });
    eprintln!("six connections and routes: {polls} polls");
    assert!(polls <= 3, "{polls} polls");
}
