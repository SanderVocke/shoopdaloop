//! Adapter behaviour beyond the shared contracts: refusals, capacity, engine refusals,
//! asynchronous reads and the connection lifecycle.

mod support;

use shoop_app_api::{BackendCapabilities, BackendCapability};
use shoop_backend::{
    Backend, BackendBusRequest, BackendDriverState, BackendLoopMode, BackendMidiEvent,
    BackendMutationKind, BackendTrackControl, BackendTrackTopology, DirectTrackRequest,
    TrackRequest,
};
use shoop_rt_backend::RtBackend;
use shoop_rt_engine::Segment;
use shoop_rt_protocol::{EngineMessage, Request, SlotId, StripId, StripShape};
use shoop_wasm_test_support::shoop_test;
use support::*;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn track(audio_channels: u32, midi: bool, initial_loops: usize) -> DirectTrackRequest {
    DirectTrackRequest {
        port_name_base: "track".to_owned(),
        audio_channels,
        midi,
        initial_loops,
    }
}

#[shoop_test]
fn only_midi_injection_is_reported_and_the_rest_is_refused_with_its_reason() {
    let mut backend = stack(&description());
    assert_eq!(
        backend.capabilities(),
        BackendCapabilities::NONE.with(BackendCapability::MidiInjection, true)
    );
    let created = backend.create_direct_track(track(1, false, 1)).unwrap();
    let loop_id = created.loops[0];
    let refusals = [
        (
            BackendCapability::BusManagement,
            backend
                .create_bus(BackendBusRequest {
                    name: "Cue".to_owned(),
                    channel_count: 2,
                    fx: None,
                })
                .err(),
        ),
        (
            BackendCapability::SessionCapture,
            backend.capture_session().err(),
        ),
        (
            BackendCapability::ReplacingMode,
            backend
                .transition_loop(loop_id, BackendLoopMode::Replacing, None)
                .err(),
        ),
        (
            BackendCapability::DryWetTracks,
            backend
                .create_track(TrackRequest {
                    port_name_base: "dry_wet".to_owned(),
                    topology: BackendTrackTopology::DryWetExternal {
                        dry_audio_channels: 1,
                        wet_audio_channels: 1,
                        dry_midi: false,
                    },
                    initial_loops: 1,
                })
                .err(),
        ),
        (
            BackendCapability::LoopTimingEdits,
            backend.set_loop_length(loop_id, 4).err(),
        ),
    ];
    for (capability, error) in refusals {
        let error = error.unwrap_or_else(|| panic!("{capability:?} was accepted"));
        assert_eq!(error.to_string(), capability.unavailable_reason());
    }
    backend.wait_idle();
    assert!(backend.poll().unwrap().mutation_failures.is_empty());
}

#[shoop_test]
fn exhausted_pools_are_refused_with_a_visible_message() {
    let mut description = description();
    description.segments = vec![Segment {
        strips: 2,
        loops: 3,
    }];
    let mut backend = stack(&description);
    let first = backend.create_direct_track(track(1, false, 2)).unwrap();
    let error = backend
        .create_direct_track(track(1, false, 2))
        .unwrap_err()
        .to_string();
    assert!(error.contains("loop slots"), "{error}");
    backend.create_direct_track(track(2, false, 1)).unwrap();
    let error = backend
        .create_direct_track(track(1, false, 0))
        .unwrap_err()
        .to_string();
    assert!(error.contains("all 2 prepared track slots"), "{error}");
    let error = backend
        .create_direct_track(track(3, false, 0))
        .unwrap_err()
        .to_string();
    assert!(error.contains("at most 2 audio channels"), "{error}");
    backend.remove_track(first.track_id).unwrap();
    backend.create_direct_track(track(1, false, 2)).unwrap();
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    assert_eq!((snapshot.tracks.len(), snapshot.loops.len()), (2, 3));
    assert!(snapshot.mutation_failures.is_empty());
}

#[shoop_test]
fn engine_refusals_arrive_as_mutation_failures() {
    let mut backend = stack(&description());
    backend.set_loop_smoothing_ms(4).unwrap();
    let error = backend
        .set_loop_smoothing_ms(60_000)
        .unwrap_err()
        .to_string();
    assert!(error.contains("limited to 4800 ms"), "{error}");
    let created = backend.create_direct_track(track(1, false, 2)).unwrap();
    let (first, second) = (created.loops[0], created.loops[1]);
    backend.set_loop_sync_source(second, Some(first)).unwrap();
    backend.wait_idle();
    assert!(backend.poll().unwrap().mutation_failures.is_empty());
    backend.set_loop_sync_source(first, Some(second)).unwrap();
    backend.wait_idle();
    let failures = backend.poll().unwrap().mutation_failures;
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, BackendMutationKind::LoopControl);
    assert!(failures[0].message.contains("sync source"), "{failures:?}");
    assert!(backend.poll().unwrap().mutation_failures.is_empty());
}

#[shoop_test]
fn a_new_backend_uses_newer_generations_after_reconnecting() {
    let mut description = description();
    description.segments = vec![Segment {
        strips: 2,
        loops: 2,
    }];
    let mut backend = stack(&description);
    for _ in 0..3 {
        let created = backend.create_direct_track(track(1, false, 2)).unwrap();
        backend.remove_track(created.track_id).unwrap();
    }
    backend.create_direct_track(track(1, false, 1)).unwrap();
    backend.wait_idle();
    assert!(backend.poll().unwrap().mutation_failures.is_empty());
    backend.disconnect();

    let (client, clock) = backend.into_parts();
    let mut backend = RtBackend::connect(client, clock).unwrap();
    let created = [
        backend.create_direct_track(track(1, false, 1)).unwrap(),
        backend.create_direct_track(track(1, false, 1)).unwrap(),
    ];
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    assert!(
        snapshot.mutation_failures.is_empty(),
        "{:?}",
        snapshot.mutation_failures
    );
    for created in &created {
        assert!(snapshot.tracks.contains_key(&created.track_id));
        assert!(snapshot.loops.contains_key(&created.loops[0]));
    }
}

#[shoop_test]
fn loop_content_is_read_asynchronously() {
    let mut backend = stack(&description());
    backend.clock_mut().capture = |_| 0.25;
    let created = backend.create_direct_track(track(1, true, 1)).unwrap();
    let (track_id, loop_id) = (created.track_id, created.loops[0]);
    backend
        .set_port_connected(created.ports[0].id, "audio_in_1", true)
        .unwrap();
    backend
        .set_track_control(track_id, BackendTrackControl::InputMonitoring(true))
        .unwrap();
    backend
        .transition_loop(loop_id, BackendLoopMode::Recording, None)
        .unwrap();
    backend.wait_idle();
    backend
        .inject_midi_input(
            track_id,
            &[BackendMidiEvent {
                time: 0,
                data: vec![0x90, 60, 100],
            }],
        )
        .unwrap();
    backend.advance(std::time::Duration::from_millis(300));
    backend
        .transition_loop(loop_id, BackendLoopMode::Playing, None)
        .unwrap();
    backend.wait_idle();
    let length = backend.poll().unwrap().loops[&loop_id].length as usize;
    assert!(length > 300, "length {length}");

    assert!(backend.loop_audio_data(loop_id).unwrap().is_none());
    assert!(backend.loop_midi_data(loop_id).unwrap().is_none());
    backend.wait_idle();
    let audio = backend.loop_audio_data(loop_id).unwrap().unwrap();
    assert_eq!(audio.len(), 1);
    assert_eq!(audio[0].len(), length);
    assert!(audio[0].iter().all(|sample| *sample == 0.25));
    let midi = backend.loop_midi_data(loop_id).unwrap().unwrap();
    assert_eq!(midi.channels.len(), 1);
    assert_eq!(midi.channels[0].length as usize, length);
    let events = &midi.channels[0].events;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data, [0x90, 60, 100]);

    backend.clear_loop(loop_id).unwrap();
    backend.wait_idle();
    let audio = backend.loop_audio_data(loop_id).unwrap().unwrap();
    assert!(audio.iter().all(|channel| channel.is_empty()));
}

#[shoop_test]
fn connecting_removes_engine_structure_the_application_does_not_know() {
    let mut description = description();
    description.segments = vec![Segment {
        strips: 1,
        loops: 1,
    }];
    let (mut clock, mut client) = parts(&description);
    client.connect().unwrap();
    while client.poll().is_some() {}
    let leftover = StripId(SlotId::new(0, 7));
    let correlation = client
        .request(Request::CreateStrip {
            strip: leftover,
            shape: StripShape {
                audio_inputs: 1,
                audio_outputs: 1,
                midi_input: false,
                midi_output: false,
            },
        })
        .unwrap();
    let mut answered = false;
    for _ in 0..100 {
        clock.render(1);
        while let Some(message) = client.poll() {
            answered |=
                matches!(message, EngineMessage::Result { correlation: c, .. } if c == correlation);
        }
        if answered {
            break;
        }
    }
    assert!(answered);
    client.disconnect();
    while client.poll().is_some() {}

    let mut backend = RtBackend::connect(client, clock).unwrap();
    let created = backend.create_direct_track(track(1, false, 1)).unwrap();
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    assert_eq!(
        snapshot.tracks.keys().copied().collect::<Vec<_>>(),
        [created.track_id]
    );
    assert!(snapshot.mutation_failures.is_empty());
}

#[shoop_test]
fn a_disconnected_engine_is_reported_failed_and_operations_are_refused() {
    let mut backend = stack(&description());
    let created = backend.create_direct_track(track(1, false, 1)).unwrap();
    backend.wait_idle();
    backend.disconnect();
    let snapshot = backend.poll().unwrap();
    assert_eq!(snapshot.status.driver_state, BackendDriverState::Failed);
    assert!(!snapshot.connections.available);
    assert!(backend.create_direct_track(track(1, false, 1)).is_err());
    assert!(backend
        .transition_loop(created.loops[0], BackendLoopMode::Recording, None)
        .is_err());
    backend.wait_idle();
}

#[cfg(not(target_arch = "wasm32"))]
#[shoop_test]
fn the_native_stack_can_move_to_the_application_thread() {
    fn send<T: Send>() {}
    send::<
        RtBackend<
            shoop_rt_protocol::carrier::InProcess<shoop_rt_engine::ControlEndpoint>,
            shoop_rt_backend::Sleep,
        >,
    >();
}

#[shoop_test]
fn xruns_are_reported_since_the_previous_poll() {
    let (mut clock, client) = parts(&description());
    clock.offline.host.add_xruns(5);
    clock.capture = |_| 0.5;
    let mut backend = RtBackend::connect(client, clock).unwrap();
    backend.wait_idle();
    let first = backend.poll().unwrap().status;
    assert_eq!(first.xruns, 0);
    backend.clock_mut().offline.host.add_xruns(3);
    backend.clock_mut().render(16);
    let status = backend.poll().unwrap().status;
    assert_eq!(status.xruns, 3);
    assert!(status.callback_count > first.callback_count);
    assert_eq!(status.input_peak, 0.5);
    backend.clock_mut().render(16);
    assert_eq!(backend.poll().unwrap().status.xruns, 0);
    backend.clock_mut().offline.host.add_xruns(2);
    backend.clock_mut().render(16);
    assert_eq!(backend.poll().unwrap().status.xruns, 2);
    assert_eq!(backend.poll().unwrap().status.xruns, 0);
}
