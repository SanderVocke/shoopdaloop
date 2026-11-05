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

struct Inventory(std::sync::Arc<std::sync::Mutex<Option<Vec<shoop_rt_backend::HostPortAlias>>>>);

impl shoop_rt_backend::HostPortInventory for Inventory {
    fn poll(&mut self) -> Option<Vec<shoop_rt_backend::HostPortAlias>> {
        self.0.lock().unwrap().take()
    }
}

#[shoop_test]
fn physical_host_ids_route_to_prepared_slots_and_preserve_disconnected_bindings() {
    use shoop_backend::{BackendPortDataType, BackendPortDirection};
    use shoop_rt_backend::HostPortAlias;
    let mut config = description();
    config.host = shoop_rt_engine::HostPorts::numbered(2, 2, 2);
    let aliases = |available| {
        vec![
            HostPortAlias {
                slot: "midi_in_2".into(),
                id: "usb/synth/in/0".into(),
                name: "Synth input".into(),
                available,
            },
            HostPortAlias {
                slot: "midi_in_1".into(),
                id: "usb/keyboard/in/0".into(),
                name: "Keyboard input".into(),
                available: true,
            },
            HostPortAlias {
                slot: "midi_out_2".into(),
                id: String::new(),
                name: String::new(),
                available: false,
            },
        ]
    };
    let state = std::sync::Arc::new(std::sync::Mutex::new(Some(aliases(true))));
    let mut backend = stack(&config).with_host_port_inventory(Box::new(Inventory(state.clone())));
    let ports = backend.poll().unwrap().connections.host_ports;
    assert_eq!(ports["usb/synth/in/0"].name, "Synth input");
    assert!(ports["usb/synth/in/0"].available);
    assert!(!ports.contains_key("midi_in_1") && !ports.contains_key("midi_in_2"));
    assert!(!ports.contains_key("midi_out_2"));
    let created = backend.create_direct_track(track(1, true, 1)).unwrap();
    backend.wait_idle();
    let input = created
        .ports
        .iter()
        .find(|port| {
            port.data_type == BackendPortDataType::Midi
                && port.direction == BackendPortDirection::Input
        })
        .unwrap()
        .id;
    backend
        .set_port_connected(input, "usb/synth/in/0", true)
        .unwrap();
    backend.wait_idle();
    assert!(backend
        .poll()
        .unwrap()
        .connections
        .confirmed_links
        .iter()
        .any(|link| link.host_port_id == "usb/synth/in/0"));
    *state.lock().unwrap() = Some(aliases(false));
    let snapshot = backend.poll().unwrap();
    assert!(!snapshot.connections.host_ports["usb/synth/in/0"].available);
    assert!(snapshot
        .connections
        .confirmed_links
        .iter()
        .any(|link| link.host_port_id == "usb/synth/in/0"));
    assert!(!backend.release_host_port_identity("usb/synth/in/0"));
    let mut replacement = aliases(true);
    replacement[0].id = "usb/replacement/in/0".into();
    *state.lock().unwrap() = Some(replacement);
    assert!(backend.poll().is_err(), "a bound slot changed identity");
    assert!(backend
        .poll()
        .unwrap()
        .connections
        .host_ports
        .contains_key("usb/synth/in/0"));
    assert!(backend
        .set_port_connected(input, "usb/synth/in/0", true)
        .is_err());
    backend
        .set_port_connected(input, "usb/synth/in/0", false)
        .unwrap();
    backend.wait_idle();
    assert!(backend
        .poll()
        .unwrap()
        .connections
        .confirmed_links
        .is_empty());
    assert!(backend.release_host_port_identity("usb/synth/in/0"));
    *state.lock().unwrap() = Some(aliases(false));
    assert!(!backend
        .poll()
        .unwrap()
        .connections
        .host_ports
        .contains_key("usb/synth/in/0"));
    assert!(backend
        .set_port_connected(input, "usb/synth/in/0", true)
        .is_err());
    *state.lock().unwrap() = Some(vec![HostPortAlias {
        slot: "midi_in_1".into(),
        id: "usb/synth/in/0".into(),
        name: "Synth input".into(),
        available: true,
    }]);
    backend.poll().unwrap();
    backend
        .set_port_connected(input, "usb/synth/in/0", true)
        .unwrap();
    backend.wait_idle();
    assert!(backend
        .poll()
        .unwrap()
        .connections
        .confirmed_links
        .iter()
        .any(|link| link.host_port_id == "usb/synth/in/0"));
}

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
fn bus_pool_limits_reuse_and_fx_refusals_preserve_confirmed_state() {
    let mut backend = stack(&description());
    let limits = backend.bus_limits();
    assert_eq!(limits.channels, 8);
    assert!(!backend.capabilities().supports(BackendCapability::BusFx));
    let mut created = Vec::new();
    for index in 1..limits.buses {
        created.push(
            backend
                .create_bus(BackendBusRequest {
                    name: format!("Bus {index}"),
                    channel_count: limits.channels,
                    fx: None,
                })
                .unwrap(),
        );
    }
    assert!(backend
        .create_bus(BackendBusRequest {
            name: "Overflow".into(),
            channel_count: 1,
            fx: None
        })
        .is_err());
    backend.wait_idle();
    assert_eq!(
        backend.poll().unwrap().mixer.buses.len(),
        limits.buses as usize
    );
    let removed = created.remove(0);
    backend.remove_bus(removed.bus_id).unwrap();
    backend.wait_idle();
    assert!(backend
        .set_bus_control(removed.bus_id, shoop_backend::BackendBusControl::Mute(true))
        .is_err());
    let next = backend
        .create_bus(BackendBusRequest {
            name: "Reused slot".into(),
            channel_count: 1,
            fx: None,
        })
        .unwrap();
    assert_ne!(removed.bus_id, next.bus_id);
    assert!(next.channels.iter().all(|c| removed
        .channels
        .iter()
        .all(|old| c.id != old.id && c.output_port_id != old.output_port_id)));
    assert!(backend
        .create_bus(BackendBusRequest {
            name: "Too wide".into(),
            channel_count: 9,
            fx: None
        })
        .is_err());
    assert!(backend
        .create_bus(BackendBusRequest {
            name: "FX".into(),
            channel_count: 1,
            fx: Some(shoop_backend::BackendBusFxRequest {
                processor_type: "builtin_fx".into(),
                audio_channels: 1
            })
        })
        .unwrap_err()
        .to_string()
        .contains("bus FX"));
    backend.wait_idle();
    let ids = backend
        .poll()
        .unwrap()
        .mixer
        .buses
        .keys()
        .copied()
        .collect::<Vec<_>>();
    for id in ids {
        backend.remove_bus(id).unwrap();
    }
    backend.wait_idle();
    assert!(backend.capture_session().unwrap().buses.is_empty());
    assert!(backend.poll().unwrap().mixer.buses.is_empty());
}

#[shoop_test]
fn completed_adapter_capabilities_are_reported_and_other_operations_refuse_with_their_reason() {
    let mut backend = stack(&description());
    assert_eq!(
        backend.capabilities(),
        BackendCapabilities::NONE
            .with(BackendCapability::BusManagement, true)
            .with(BackendCapability::MidiInjection, true)
            .with(BackendCapability::SessionCapture, true)
            .with(BackendCapability::SessionContentLoad, true)
            .with(BackendCapability::LoopContentImport, true)
            .with(BackendCapability::LoopTimingEdits, true)
    );
    let created = backend.create_direct_track(track(1, false, 1)).unwrap();
    let loop_id = created.loops[0];
    let refusals = [
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
fn a_new_backend_uses_newer_generations_after_explicit_session_cleanup() {
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
    let last = backend.create_direct_track(track(1, false, 1)).unwrap();
    backend.remove_track(last.track_id).unwrap();
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
                frame: 0,
                data: vec![0x90, 60, 100].into(),
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
    assert_eq!(events[0].data, [0x90, 60, 100].into());

    backend.clear_loop(loop_id).unwrap();
    backend.wait_idle();
    let audio = backend.loop_audio_data(loop_id).unwrap().unwrap();
    assert!(audio.iter().all(|channel| channel.is_empty()));
}

#[shoop_test]
fn a_fresh_adapter_refuses_to_delete_another_services_live_structure() {
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

    let mut backend = RtBackend::begin(client, clock).unwrap();
    for _ in 0..100 {
        backend.pump();
        backend.clock_mut().render(1);
        if backend.poll().unwrap().status.driver_state == BackendDriverState::Failed {
            break;
        }
    }
    assert_eq!(
        backend.poll().unwrap().status.driver_state,
        BackendDriverState::Failed
    );
    assert_eq!(
        backend.mutation_cursor(),
        0,
        "reattachment must not queue cleanup"
    );
    let (mut client, mut clock) = backend.into_parts();
    client.disconnect();
    while client.poll().is_some() {}
    client.connect().unwrap();
    while client.poll().is_some() {}
    client.resync().unwrap();
    let mut preserved = false;
    for _ in 0..100 {
        clock.render(1);
        while let Some(message) = client.poll() {
            preserved |= matches!(message, EngineMessage::State(shoop_rt_protocol::StateRecord::Strip { strip, .. }) if strip == leftover);
        }
        if preserved {
            break;
        }
    }
    assert!(
        preserved,
        "the engine-owned strip must survive a refused adapter"
    );
}

#[shoop_test]
fn a_new_media_read_waits_for_all_prior_cancellations_without_losing_its_result() {
    let mut backend = stack(&description());
    backend.clock_mut().capture = |_| 0.25;
    let created = backend.create_direct_track(track(1, true, 1)).unwrap();
    let loop_id = created.loops[0];
    backend
        .set_port_connected(created.ports[0].id, "audio_in_1", true)
        .unwrap();
    backend
        .transition_loop(loop_id, BackendLoopMode::Recording, None)
        .unwrap();
    backend.wait_idle();
    backend
        .inject_midi_input(
            created.track_id,
            &[BackendMidiEvent {
                frame: 0,
                data: vec![0x90, 60, 100].into(),
            }],
        )
        .unwrap();
    backend.advance(std::time::Duration::from_millis(300));
    backend
        .transition_loop(loop_id, BackendLoopMode::Playing, None)
        .unwrap();
    backend.wait_idle();
    for _ in 0..8 {
        backend.cancel_media_reads();
        backend.wait_idle();
        assert!(backend.loop_audio_data(loop_id).unwrap().is_none());
        backend.clock_mut().render(1);
        backend.pump();
        backend.cancel_media_reads();
        assert!(backend.loop_audio_data(loop_id).unwrap().is_none());
        backend.cancel_media_reads();
        assert!(backend.loop_midi_data(loop_id).unwrap().is_none());
        backend
            .set_track_control(
                created.track_id,
                BackendTrackControl::InputMonitoring(false),
            )
            .unwrap();
        backend.wait_idle();
        let midi = backend.loop_midi_data(loop_id).unwrap().unwrap();
        assert_eq!(midi.channels[0].events.len(), 1);
        assert_eq!(midi.channels[0].events[0].data.as_ref(), &[0x90, 60, 100]);
    }
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

#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
#[shoop_test]
fn the_native_stack_can_move_to_the_application_thread() {
    fn send<T: Send>() {}
    send::<
        RtBackend<
            shoop_rt_transport::carrier::InProcess<shoop_rt_engine::ControlEndpoint>,
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

#[shoop_test]
fn mutation_outcomes_are_bounded_and_keep_submission_identity_through_refusal_and_loss() {
    let mut backend = stack(&description());
    let created = backend.create_direct_track(track(1, false, 2)).unwrap();
    let created_through = backend.mutation_cursor();
    assert!(backend.mutations_pending_through(created_through));
    backend.wait_idle();
    assert!(!backend.mutations_pending_through(created_through));
    let outcomes = backend.poll().unwrap().mutation_completions;
    assert_eq!(outcomes.len() as u64, created_through);
    assert!(outcomes.iter().all(|outcome| outcome.result.is_ok()));
    backend
        .set_loop_sync_source(created.loops[1], Some(created.loops[0]))
        .unwrap();
    backend.wait_idle();
    backend.poll().unwrap();
    backend
        .set_loop_sync_source(created.loops[0], Some(created.loops[1]))
        .unwrap();
    let refused_sequence = backend.mutation_cursor();
    backend.wait_idle();
    let outcomes = backend.poll().unwrap().mutation_completions;
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].sequence, refused_sequence);
    assert!(outcomes[0].result.is_err());
    backend.set_loop_gain(created.loops[0], 0.25).unwrap();
    let lost_sequence = backend.mutation_cursor();
    backend.disconnect();
    let outcomes = backend.poll().unwrap().mutation_completions;
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].sequence, lost_sequence);
    assert!(outcomes[0].result.is_err());
}

#[shoop_test]
fn tracking_capacity_refuses_before_admission_and_returns_after_outcomes_are_consumed() {
    let mut backend = stack(&description());
    for _ in 0..shoop_rt_backend::MAX_TRACKED_MUTATIONS {
        backend.set_loop_smoothing_ms(0).unwrap();
    }
    let cursor = backend.mutation_cursor();
    assert_eq!(backend.mutation_capacity(), 0);
    assert!(backend.set_loop_smoothing_ms(0).is_err());
    assert!(backend.create_direct_track(track(1, false, 1)).is_err());
    assert_eq!(backend.mutation_cursor(), cursor);
    backend.wait_idle();
    let outcomes = backend.poll().unwrap().mutation_completions;
    assert_eq!(outcomes.len(), shoop_rt_backend::MAX_TRACKED_MUTATIONS);
    assert!(outcomes.iter().all(|outcome| outcome.result.is_ok()));
    assert_eq!(
        backend.mutation_capacity(),
        shoop_rt_backend::MAX_TRACKED_MUTATIONS
    );
    backend.set_loop_smoothing_ms(0).unwrap();
}

#[shoop_test]
fn a_saved_missing_device_reconnects_on_a_different_prepared_slot() {
    use shoop_backend::{BackendAsyncResult, BackendPortDataType, BackendPortDirection};
    use shoop_rt_backend::HostPortAlias;
    let mut config = description();
    config.host = shoop_rt_engine::HostPorts::numbered(2, 2, 2);
    let id = "usb0/1:serial/in/0";
    let inventory = |slot: usize| {
        (1..=2)
            .map(|index| HostPortAlias {
                slot: format!("midi_in_{index}"),
                id: if index == slot {
                    id.into()
                } else {
                    String::new()
                },
                name: if index == slot {
                    "Synth input".into()
                } else {
                    String::new()
                },
                available: index == slot,
            })
            .collect()
    };
    let original = std::sync::Arc::new(std::sync::Mutex::new(Some(inventory(2))));
    let mut source = stack(&config).with_host_port_inventory(Box::new(Inventory(original)));
    source.poll().unwrap();
    let track = source.create_direct_track(track(1, true, 1)).unwrap();
    source.wait_idle();
    let input = track
        .ports
        .iter()
        .find(|port| {
            port.data_type == BackendPortDataType::Midi
                && port.direction == BackendPortDirection::Input
        })
        .unwrap()
        .id;
    source.set_port_connected(input, id, true).unwrap();
    source.wait_idle();
    let saved = source.capture_session().unwrap();
    let state = std::sync::Arc::new(std::sync::Mutex::new(Some(inventory(0))));
    let mut restored = stack(&config).with_host_port_inventory(Box::new(Inventory(state.clone())));
    restored.poll().unwrap();
    let mut replacement = None;
    for _ in 0..2000 {
        match restored.replace_session_async(&saved).unwrap() {
            BackendAsyncResult::Ready(mapping) => {
                replacement = Some(mapping);
                break;
            }
            BackendAsyncResult::Pending(_) => {
                assert!(
                    restored.host_port_in_use(id),
                    "pending replacement lost its device pin"
                );
                assert!(!restored.release_host_port_identity(id));
                restored.clock_mut().render(1);
            }
        }
    }
    let mapping = replacement.expect("offline session replacement");
    restored.wait_idle();
    assert!(
        restored.host_port_in_use(id),
        "saved route did not pin its physical identity"
    );
    assert!(!restored
        .poll()
        .unwrap()
        .connections
        .confirmed_links
        .iter()
        .any(|link| link.host_port_id == id));
    *state.lock().unwrap() = Some(inventory(1));
    restored.poll().unwrap();
    restored.wait_idle();
    assert!(restored
        .poll()
        .unwrap()
        .connections
        .confirmed_links
        .iter()
        .any(|link| link.host_port_id == id));
    let track = mapping.tracks.values().next().unwrap();
    let output = track
        .ports
        .iter()
        .find(|port| {
            port.data_type == BackendPortDataType::Midi
                && port.direction == BackendPortDirection::Output
        })
        .unwrap()
        .id;
    restored
        .set_track_control(track.track_id, BackendTrackControl::InputMonitoring(true))
        .unwrap();
    restored
        .set_port_connected(output, "midi_out_1", true)
        .unwrap();
    restored.wait_idle();
    let host = &mut restored.clock_mut().offline;
    host.midi_input_mut(0)
        .push(0, 16, &[0x90, 60, 100])
        .unwrap();
    host.midi_input_mut(1)
        .push(0, 16, &[0x91, 99, 123])
        .unwrap();
    host.process(16).unwrap();
    let output = host.midi_output(0);
    assert!((0..output.len()).any(|i| output.get(i).unwrap().1 == [0x90, 60, 100]));
    assert!(!(0..output.len()).any(|i| output.get(i).unwrap().1 == [0x91, 99, 123]));
}
