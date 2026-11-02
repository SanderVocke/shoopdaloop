mod support;
use shoop_backend::*;
use shoop_wasm_test_support::shoop_test;
use support::*;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn loaded(
    backend: &mut Stack,
    data: &BackendSessionData,
) -> anyhow::Result<BackendSessionReplacement> {
    for _ in 0..2000 {
        match backend.replace_session_async(data)? {
            BackendAsyncResult::Ready(mapping) => return Ok(mapping),
            BackendAsyncResult::Pending(_) => backend.clock_mut().render(1),
        }
    }
    panic!("replacement did not settle");
}
fn fixture() -> (Stack, BackendTrackCreation, BackendSessionData) {
    let mut backend = stack(&description());
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "music".to_owned(),
            audio_channels: 1,
            midi: true,
            initial_loops: 1,
        })
        .unwrap();
    backend.wait_idle();
    let session = backend.capture_session().unwrap();
    (backend, track, session)
}
#[shoop_test]
fn private_session_image_publishes_fresh_mappings_and_lossless_canonical_media() {
    let (mut backend, old, mut session) = fixture();
    let content = &mut session.tracks[0].loops[0];
    content.length = 4;
    content.gain = 0.75;
    content.balance = -0.5;
    content.audio[0].samples = vec![0., 0.25, 0.5, 0.75, 1., 0.125].into();
    content.audio[0].start_offset = 1;
    content.audio[0].gain = 0.5;
    content.midi[0].length = 8;
    content.midi[0].start_offset = 1;
    content.midi[0].start_state = vec![vec![0xb0, 7, 99]];
    content.midi[0].events = vec![
        BackendMidiEvent {
            frame: 2,
            data: vec![0x90, 60, 100].into(),
        },
        BackendMidiEvent {
            frame: 5,
            data: vec![0x80, 60, 0].into(),
        },
    ]
    .into();
    let mut expected = content.clone();
    let mapping = loaded(&mut backend, &session).unwrap();
    let new = mapping.loops[&old.loops[0].raw()];
    assert_ne!(new, old.loops[0]);
    let snapshot = backend.poll().unwrap();
    assert!(!snapshot.loops.contains_key(&old.loops[0]));
    assert_eq!(snapshot.loops[&new].mode, BackendLoopMode::Stopped);
    assert_eq!(snapshot.loops[&new].length, 4);
    expected.source_id = new.raw();
    let saved = backend.capture_session().unwrap();
    assert_eq!(saved.tracks[0].loops[0], expected);
    assert!(std::sync::Arc::ptr_eq(
        &saved.tracks[0].loops[0].audio[0].samples,
        &expected.audio[0].samples
    ));
    assert_eq!(saved.tracks[0].port_name_base, "music");
}
#[shoop_test]
fn cancelled_or_invalid_replacement_preserves_all_live_identities_and_media() {
    let (mut backend, old, mut session) = fixture();
    let before = backend.capture_session().unwrap();
    session.tracks[0].loops[0].length = 4;
    session.tracks[0].loops[0].audio[0].samples = vec![1.; 4].into();
    assert!(matches!(
        backend.replace_session_async(&session).unwrap(),
        BackendAsyncResult::Pending(_)
    ));
    backend.cancel_session_replacement().unwrap();
    assert!(loaded(&mut backend, &session)
        .unwrap_err()
        .to_string()
        .contains("cancelled"));
    assert!(backend.poll().unwrap().loops.contains_key(&old.loops[0]));
    assert_eq!(backend.capture_session().unwrap(), before);
    session.tracks[0].loops[0].audio[0].preplay = 1;
    assert!(backend.replace_session_async(&session).is_err());
    assert_eq!(backend.capture_session().unwrap(), before);
}
#[shoop_test]
fn captured_recorded_samples_remain_exact_and_active_capture_is_refused() {
    let (mut backend, track, initial) = fixture();
    let input = track
        .ports
        .iter()
        .find(|port| port.role == BackendPortRole::AudioInput)
        .unwrap()
        .id;
    backend
        .set_port_connected(input, "audio_in_1", true)
        .unwrap();
    backend.wait_idle();
    backend.clock_mut().capture = |_| 0.25;
    backend
        .transition_loop(track.loops[0], BackendLoopMode::Recording, None)
        .unwrap();
    backend.wait_idle();
    backend.clock_mut().render(8);
    assert!(backend
        .capture_session()
        .unwrap_err()
        .to_string()
        .contains("recording"));
    assert!(backend
        .replace_session_async(&initial)
        .unwrap_err()
        .to_string()
        .contains("recording"));
    assert_eq!(
        backend.poll().unwrap().loops[&track.loops[0]].mode,
        BackendLoopMode::Recording
    );
    backend
        .transition_loop(track.loops[0], BackendLoopMode::Playing, None)
        .unwrap();
    backend.wait_idle();
    let saved = backend.capture_session().unwrap();
    let content = &saved.tracks[0].loops[0];
    assert!(!content.audio[0].samples.is_empty());
    assert!(content.audio[0]
        .samples
        .iter()
        .all(|sample| *sample == 0.25));
    let expected = content.audio[0].samples.clone();
    loaded(&mut backend, &saved).unwrap();
    assert_eq!(
        backend.capture_session().unwrap().tracks[0].loops[0].audio[0].samples,
        expected
    );
}

fn imported(
    backend: &mut Stack,
    id: BackendLoopId,
    update: &BackendLoopContentUpdate,
) -> anyhow::Result<()> {
    for _ in 0..2000 {
        match backend.replace_loop_content_async(id, update)? {
            BackendAsyncResult::Ready(()) => return Ok(()),
            BackendAsyncResult::Pending(_) => backend.clock_mut().render(1),
        }
    }
    panic!("content update did not settle");
}
#[shoop_test]
fn media_updates_preserve_opposite_media_identity_controls_and_playing_mode() {
    let (mut backend, track, mut session) = fixture();
    let content = &mut session.tracks[0].loops[0];
    content.length = 4;
    content.audio[0].samples = vec![0.25; 6].into();
    content.audio[0].start_offset = 1;
    content.audio[0].gain = 0.75;
    content.midi[0].length = 6;
    content.midi[0].events = vec![
        BackendMidiEvent {
            frame: 0,
            data: vec![0x90, 60, 99].into(),
        },
        BackendMidiEvent {
            frame: 3,
            data: vec![0x80, 60, 0].into(),
        },
    ]
    .into();
    let original_midi = content.midi.clone();
    let mapping = loaded(&mut backend, &session).unwrap();
    let id = mapping.loops[&track.loops[0].raw()];
    backend
        .transition_loop(id, BackendLoopMode::Playing, None)
        .unwrap();
    backend.wait_idle();
    imported(
        &mut backend,
        id,
        &BackendLoopContentUpdate {
            audio: vec![BackendAudioChannelUpdate {
                channel: 0,
                samples: vec![0.5; 8].into(),
                start_offset: Some(0),
                capture_alignment_frames: Some(0),
                preplay: None,
            }],
            length: Some(8),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        backend.poll().unwrap().loops[&id].mode,
        BackendLoopMode::Playing
    );
    let captured = backend.capture_session().unwrap();
    assert_eq!(captured.tracks[0].loops[0].midi, original_midi);
    let audio = captured.tracks[0].loops[0].audio.clone();
    imported(
        &mut backend,
        id,
        &BackendLoopContentUpdate {
            midi: vec![BackendMidiChannelUpdate {
                channel: 0,
                length: 8,
                start_state: vec![vec![0xb0, 7, 12], vec![0xb0, 7, 99]],
                events: vec![BackendMidiEvent {
                    frame: 0,
                    data: vec![0x90, 64, 87].into(),
                }]
                .into(),
                start_offset: Some(0),
                capture_alignment_frames: Some(0),
                preplay: None,
            }],
            ..Default::default()
        },
    )
    .unwrap();
    let captured = backend.capture_session().unwrap();
    assert_eq!(captured.tracks[0].loops[0].audio, audio);
    assert_eq!(
        captured.tracks[0].loops[0].midi[0].start_state,
        [vec![0xb0, 7, 12], vec![0xb0, 7, 99]]
    );
    assert_eq!(captured.tracks[0].loops[0].source_id, id.raw());
}
#[shoop_test]
fn missing_external_connection_intent_survives_load_and_save_with_a_diagnostic() {
    let (mut backend, _, mut session) = fixture();
    session.tracks[0].ports[0].external_connections = vec!["missing-device:input".to_owned()];
    loaded(&mut backend, &session).unwrap();
    assert_eq!(
        backend.capture_session().unwrap().tracks[0].ports[0].external_connections,
        ["missing-device:input"]
    );
    assert!(backend
        .poll()
        .unwrap()
        .connections
        .failures
        .iter()
        .any(|failure| failure.external_port == "missing-device:input"));
}

#[shoop_test]
fn sync_relationships_are_part_of_the_atomic_session_image() {
    use std::collections::BTreeMap;
    let (mut backend, _, mut session) = fixture();
    let first = session.tracks[0].loops[0].source_id;
    let mut second = session.tracks[0].loops[0].clone();
    second.source_id = 999;
    session.tracks[0].loops.push(second);
    let sync = BTreeMap::from([(first, 999)]);
    let mapping = (0..2000)
        .find_map(|_| {
            match backend
                .replace_session_with_sync_async(&session, &sync)
                .unwrap()
            {
                BackendAsyncResult::Ready(mapping) => Some(mapping),
                BackendAsyncResult::Pending(_) => {
                    backend.clock_mut().render(1);
                    None
                }
            }
        })
        .expect("session replacement completion");
    assert_eq!(
        backend.loop_sync_source(mapping.loops[&first]),
        Some(mapping.loops[&999])
    );
    let before = backend.capture_session().unwrap();
    assert!(backend
        .replace_session_with_sync_async(&session, &BTreeMap::from([(first, 999), (999, first)]))
        .is_err());
    assert_eq!(backend.capture_session().unwrap(), before);
}

#[shoop_test]
fn exhausted_old_plus_new_headroom_preserves_old_playback_and_reclaims_failed_stages() {
    let mut configured = description();
    configured.resource_pages = 64;
    let mut backend = stack(&configured);
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "music".to_owned(),
            audio_channels: 1,
            midi: false,
            initial_loops: 1,
        })
        .unwrap();
    backend.wait_idle();
    let mut session = backend.capture_session().unwrap();
    session.tracks[0].loops[0].length = 1024;
    session.tracks[0].loops[0].audio[0].samples = vec![0.25; 1024].into();
    let mapping = loaded(&mut backend, &session).unwrap();
    let old = mapping.loops[&track.loops[0].raw()];
    backend
        .transition_loop(old, BackendLoopMode::Playing, None)
        .unwrap();
    backend.wait_idle();
    let before = backend.capture_session().unwrap();
    let mut oversized = before.clone();
    oversized.tracks[0].loops[0].length = 1536;
    oversized.tracks[0].loops[0].audio[0].samples = vec![0.5; 1536].into();
    for _ in 0..8 {
        assert!(loaded(&mut backend, &oversized)
            .unwrap_err()
            .to_string()
            .contains("Capacity"));
        assert_eq!(
            backend.poll().unwrap().loops[&old].mode,
            BackendLoopMode::Playing
        );
        assert_eq!(backend.capture_session().unwrap(), before);
    }
    let mut small = before;
    small.tracks[0].loops[0].length = 16;
    small.tracks[0].loops[0].audio[0].samples = vec![0.5; 16].into();
    loaded(&mut backend, &small).unwrap();
    assert_eq!(
        backend.capture_session().unwrap().tracks[0].loops[0].audio[0].samples,
        [0.5; 16].to_vec().into()
    );
}

#[shoop_test]
fn selected_capture_ignores_other_recordings_and_imported_views_retain_original_metadata() {
    let (mut backend, track, mut session) = fixture();
    let content = &mut session.tracks[0].loops[0];
    content.length = 4;
    content.audio[0].samples = vec![0.25; 6].into();
    content.audio[0].start_offset = 1;
    content.midi[0].length = 8;
    content.midi[0].events = vec![BackendMidiEvent {
        frame: 5,
        data: vec![0x90, 60, 100].into(),
    }]
    .into();
    let mapping = loaded(&mut backend, &session).unwrap();
    let selected = mapping.loops[&track.loops[0].raw()];
    let other = backend
        .add_loop_to_track(mapping.tracks[&track.track_id.raw()].track_id)
        .unwrap();
    backend
        .transition_loop(other, BackendLoopMode::Recording, None)
        .unwrap();
    backend.wait_idle();
    let content = (0..500)
        .find_map(
            |_| match backend.capture_loop_content_async(selected).unwrap() {
                BackendAsyncResult::Ready(content) => Some(content),
                BackendAsyncResult::Pending(_) => {
                    backend.clock_mut().render(1);
                    None
                }
            },
        )
        .expect("selected capture completion");
    assert_eq!(content.audio[0].samples, [0.25; 6].to_vec().into());
    let audio = backend
        .loop_audio_data_with_metadata(selected)
        .unwrap()
        .unwrap();
    assert_eq!(audio.channels[0].samples.as_slice(), [0.25; 6]);
    assert_eq!(audio.channels[0].start_offset, 1);
    let chunk = backend.loop_audio_data_chunk(selected, 0, 1, 2).unwrap();
    assert_eq!(chunk.start_offset, 1);
    assert_eq!(chunk.total_samples, 6);
    assert_eq!(
        backend.loop_midi_data(selected).unwrap().unwrap().channels[0].events[0].frame,
        5
    );
    assert_eq!(
        backend.poll().unwrap().loops[&other].mode,
        BackendLoopMode::Recording
    );
    assert!(backend
        .capture_session()
        .unwrap_err()
        .to_string()
        .contains("recording"));
}
#[shoop_test]
fn engine_loss_finishes_a_pending_persistence_job_with_an_honest_unknown_outcome() {
    let (mut backend, _, mut session) = fixture();
    session.tracks[0].loops[0].length = 128;
    session.tracks[0].loops[0].audio[0].samples = vec![0.25; 128].into();
    assert!(matches!(
        backend.replace_session_async(&session).unwrap(),
        BackendAsyncResult::Pending(_)
    ));
    backend.disconnect();
    assert!(backend
        .replace_session_async(&session)
        .unwrap_err()
        .to_string()
        .contains("unknown"));
}
#[shoop_test]
fn untracked_initial_parameters_refuse_but_a_later_system_reset_is_preserved() {
    let (mut backend, old, mut session) = fixture();
    let before = backend.capture_session().unwrap();
    session.tracks[0].loops[0].midi[0].start_state = vec![vec![0xb0, 6, 40]];
    assert!(loaded(&mut backend, &session).is_err());
    assert_eq!(backend.capture_session().unwrap(), before);
    session.tracks[0].loops[0].midi[0]
        .start_state
        .push(vec![0xff]);
    let mapping = loaded(&mut backend, &session).unwrap();
    assert_ne!(mapping.loops[&old.loops[0].raw()], old.loops[0]);
    assert_eq!(
        backend.capture_session().unwrap().tracks[0].loops[0].midi[0].start_state,
        [vec![0xb0, 6, 40], vec![0xff]]
    );
}

#[shoop_test]
fn large_private_validation_yields_and_checks_samples_outside_the_render_window() {
    use std::sync::Arc;
    let (mut backend, old, mut session) = fixture();
    session.tracks[0].loops[0].length = 4;
    session.tracks[0].loops[0].audio[0].samples = vec![0.25; 4].into();
    let mapping = loaded(&mut backend, &session).unwrap();
    let live = mapping.loops[&old.loops[0].raw()];
    let track = mapping.tracks[&old.track_id.raw()].track_id;
    backend
        .transition_loop(live, BackendLoopMode::Playing, None)
        .unwrap();
    backend.wait_idle();
    let before = backend.capture_session().unwrap();
    let mut candidate = before.clone();
    candidate.tracks[0].loops[0].length = 1;
    let mut samples = vec![0.25; 33 * 1024 * 1024 / 4];
    *samples.last_mut().unwrap() = f32::NAN;
    candidate.tracks[0].loops[0].audio[0].samples = Arc::new(samples);
    assert!(matches!(
        backend.replace_session_async(&candidate).unwrap(),
        BackendAsyncResult::Pending(_)
    ));
    assert!(backend
        .set_track_control(track, BackendTrackControl::InputGainDb(-6.0))
        .unwrap_err()
        .to_string()
        .contains("replacement"));
    for _ in 0..4 {
        backend.clock_mut().render(1);
        backend.poll().unwrap();
    }
    let state = backend.poll().unwrap();
    assert_eq!(state.tracks[&track].input_gain_db, 0.0);
    assert_eq!(state.loops[&live].mode, BackendLoopMode::Playing);
    assert!(matches!(
        backend.replace_session_async(&candidate).unwrap(),
        BackendAsyncResult::Pending(_)
    ));
    backend.cancel_session_replacement().unwrap();
    assert!(loaded(&mut backend, &candidate)
        .unwrap_err()
        .to_string()
        .contains("cancelled"));
    assert!(Arc::get_mut(&mut candidate.tracks[0].loops[0].audio[0].samples).is_some());
    assert!(loaded(&mut backend, &candidate)
        .unwrap_err()
        .to_string()
        .contains("non-finite"));
    assert_eq!(
        backend.poll().unwrap().loops[&live].mode,
        BackendLoopMode::Playing
    );
    assert_eq!(
        backend.capture_session().unwrap().tracks[0].loops,
        before.tracks[0].loops
    );
    *Arc::get_mut(&mut candidate.tracks[0].loops[0].audio[0].samples)
        .expect("failed preparation releases its image")
        .last_mut()
        .unwrap() = 0.25;
    loaded(&mut backend, &candidate).unwrap();
    let saved = backend.capture_session().unwrap();
    assert!(Arc::ptr_eq(
        &saved.tracks[0].loops[0].audio[0].samples,
        &candidate.tracks[0].loops[0].audio[0].samples
    ));
}

#[shoop_test]
fn sparse_midi_upload_larger_than_one_callback_validation_budget_roundtrips() {
    let mut configured = description();
    configured.page_bytes = 4096;
    let mut backend = stack(&configured);
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "keys".into(),
            audio_channels: 0,
            midi: true,
            initial_loops: 1,
        })
        .unwrap();
    backend.wait_idle();
    let mut session = backend.capture_session().unwrap();
    let content = &mut session.tracks[0].loops[0];
    content.length = 16_000;
    content.midi[0].length = 16_000;
    content.midi[0].events = (0..1000)
        .map(|i| BackendMidiEvent {
            frame: i * 16,
            data: vec![if i % 2 == 0 { 0x90 } else { 0x80 }, 60, 64].into(),
        })
        .collect::<Vec<_>>()
        .into();
    let expected = content.midi[0].clone();
    let mapping = loaded(&mut backend, &session).unwrap();
    assert_ne!(mapping.loops[&track.loops[0].raw()], track.loops[0]);
    assert_eq!(
        backend.capture_session().unwrap().tracks[0].loops[0].midi[0],
        expected
    );
}

#[shoop_test]
fn upload_window_negotiates_wide_support_and_keeps_legacy_peers_at_eight() {
    use shoop_rt_protocol::{Capabilities, ClientMessage, EngineMessage, Request};
    use shoop_rt_transport::{
        carrier::{Carrier, CarrierError},
        client::ProtocolClient,
    };
    use std::{cell::Cell, collections::BTreeSet, rc::Rc};
    struct Measured<C> {
        carrier: C,
        wide: bool,
        writes: BTreeSet<u64>,
        peak: Rc<Cell<usize>>,
    }
    impl<C: Carrier> Carrier for Measured<C> {
        fn send(&mut self, message: ClientMessage) -> Result<(), CarrierError> {
            self.carrier.send(message)?;
            if let ClientMessage::Request {
                correlation,
                request: Request::ContentWrite { .. },
            } = message
            {
                self.writes.insert(correlation);
                self.peak.set(self.peak.get().max(self.writes.len()));
            }
            Ok(())
        }
        fn receive(&mut self) -> Option<EngineMessage> {
            let mut message = self.carrier.receive()?;
            match &mut message {
                EngineMessage::Welcome { capabilities, .. } if !self.wide => {
                    capabilities.0 &= !Capabilities::WIDE_CONTENT_UPLOAD.0
                }
                EngineMessage::Result { correlation, .. } => {
                    self.writes.remove(correlation);
                }
                _ => {}
            }
            Some(message)
        }
    }
    for (wide, expected) in [(false, 8), (true, 64)] {
        let mut configured = description();
        configured.reliable_credits = 128;
        let (offline, client) = shoop_rt_engine::testing::OfflineRender::new(&configured).unwrap();
        let peak = Rc::new(Cell::new(0));
        let client = ProtocolClient::new(Measured {
            carrier: client.into_carrier(),
            wide,
            writes: BTreeSet::new(),
            peak: Rc::clone(&peak),
        });
        let mut backend = shoop_rt_backend::RtBackend::connect(
            client,
            OfflineClock {
                offline,
                capture: |_| 0.,
                block: 16,
            },
        )
        .unwrap();
        backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "window".into(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        backend.wait_idle();
        let mut session = backend.capture_session().unwrap();
        session.tracks[0].loops[0].length = 4096;
        session.tracks[0].loops[0].audio[0].samples = vec![0.25; 4096].into();
        let mut finished = false;
        for _ in 0..2000 {
            if matches!(
                backend.replace_session_async(&session).unwrap(),
                BackendAsyncResult::Ready(_)
            ) {
                finished = true;
                break;
            }
            backend.clock_mut().render(1);
        }
        assert!(finished);
        assert_eq!(peak.get(), expected);
        assert_eq!(
            backend.capture_session().unwrap().tracks[0].loops[0].audio[0]
                .samples
                .as_ref(),
            &vec![0.25; 4096]
        );
    }
}
