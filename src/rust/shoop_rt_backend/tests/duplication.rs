mod support;
use shoop_backend::*;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn copy(
    backend: &mut Stack,
    source: BackendLoopId,
    target: Option<BackendLoopId>,
    additions: &[BackendTrackId],
) -> anyhow::Result<BackendLoopDuplication> {
    for _ in 0..3000 {
        match backend.duplicate_loop_async(source, target, additions, None)? {
            BackendAsyncResult::Ready(result) => return Ok(result),
            BackendAsyncResult::Pending(_) => backend.clock_mut().render(1),
        }
    }
    panic!("duplication did not settle")
}

#[shoop_test]
fn exact_copy_preserves_raw_media_controls_and_unrelated_recording() {
    let mut backend = stack(&description());
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "copy".into(),
            audio_channels: 1,
            midi: true,
            initial_loops: 3,
        })
        .unwrap();
    backend.wait_idle();
    let source = track.loops[0];
    let target = track.loops[1];
    let recording = track.loops[2];
    let update = BackendLoopContentUpdate {
        length: Some(4),
        audio: vec![BackendAudioChannelUpdate {
            channel: 0,
            samples: vec![0.25, -0.5, 0.75, 1., 0.125, 0.625].into(),
            start_offset: Some(1),
            capture_alignment_frames: Some(0),
            preplay: Some(1),
        }],
        midi: vec![BackendMidiChannelUpdate {
            channel: 0,
            length: 6,
            start_state: vec![vec![0xb0, 7, 99]],
            events: vec![
                BackendMidiEvent {
                    frame: 2,
                    data: vec![0x90, 60, 99].into(),
                },
                BackendMidiEvent {
                    frame: 3,
                    data: vec![0x80, 60, 0].into(),
                },
            ]
            .into(),
            start_offset: Some(1),
            capture_alignment_frames: Some(0),
            preplay: Some(1),
        }],
    };
    for _ in 0..2000 {
        match backend.replace_loop_content_async(source, &update).unwrap() {
            BackendAsyncResult::Ready(_) => break,
            BackendAsyncResult::Pending(_) => backend.clock_mut().render(1),
        }
    }
    backend
        .set_loop_timing(source, Some(2), Some(2), None)
        .unwrap();
    backend.wait_idle();
    backend.set_loop_gain(source, 0.42).unwrap();
    backend.set_loop_balance(source, -0.25).unwrap();
    backend.wait_idle();
    let before = match backend.capture_loop_content_async(source).unwrap() {
        BackendAsyncResult::Ready(c) => c,
        _ => panic!("canonical content"),
    };
    backend
        .transition_loop(recording, BackendLoopMode::Recording, None)
        .unwrap();
    backend.wait_idle();
    let result = copy(&mut backend, source, Some(target), &[]).unwrap();
    assert_eq!(result.target, target);
    assert!(result.added.is_empty());
    let state = backend.poll().unwrap();
    assert_eq!(state.loops[&recording].mode, BackendLoopMode::Recording);
    assert_eq!(state.loops[&target].mode, BackendLoopMode::Stopped);
    assert!(
        backend
            .loop_audio_data_with_metadata(target)
            .unwrap()
            .is_some(),
        "the destination must expose its own timing/content revision"
    );
    assert!(backend.loop_midi_data(target).unwrap().is_some());
    let actual = match backend.capture_loop_content_async(target).unwrap() {
        BackendAsyncResult::Ready(c) => c,
        _ => panic!("copied content"),
    };
    assert_eq!(actual.audio, before.audio);
    assert_eq!(actual.midi, before.midi);
    assert_eq!(actual.gain, before.gain);
    assert_eq!(actual.balance, before.balance);
    assert_eq!(actual.length, before.length);
    backend.clear_loop(source).unwrap();
    backend.wait_idle();
    let after = match backend.capture_loop_content_async(target).unwrap() {
        BackendAsyncResult::Ready(c) => c,
        _ => panic!("retained copy"),
    };
    assert_eq!(after.audio, before.audio);
    assert_eq!(after.midi, before.midi);
}

#[shoop_test]
fn row_additions_are_private_and_failed_copies_leave_layout_intact() {
    let mut backend = stack(&description());
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "rows".into(),
            audio_channels: 1,
            midi: false,
            initial_loops: 1,
        })
        .unwrap();
    backend.wait_idle();
    let source = track.loops[0];
    let result = copy(&mut backend, source, None, &[track.track_id]).unwrap();
    assert_eq!(result.added, vec![(track.track_id, result.target)]);
    assert_ne!(source, result.target);
    let before = backend
        .poll()
        .unwrap()
        .loops
        .keys()
        .copied()
        .collect::<Vec<_>>();
    assert!(copy(&mut backend, source, None, &vec![track.track_id; 100]).is_err());
    assert_eq!(
        backend
            .poll()
            .unwrap()
            .loops
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        before
    );
    assert!(copy(&mut backend, source, Some(source), &[]).is_err());
}

#[shoop_test]
fn cancellation_reclaims_private_rows_and_a_later_copy_can_retry() {
    let mut backend = stack(&description());
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "cancel".into(),
            audio_channels: 1,
            midi: true,
            initial_loops: 1,
        })
        .unwrap();
    backend.wait_idle();
    let before = backend.capture_session().unwrap();
    assert!(matches!(
        backend
            .duplicate_loop_async(track.loops[0], None, &[track.track_id], None)
            .unwrap(),
        BackendAsyncResult::Pending(_)
    ));
    backend.cancel_session_replacement().unwrap();
    assert!(copy(&mut backend, track.loops[0], None, &[track.track_id]).is_err());
    assert_eq!(backend.capture_session().unwrap(), before);
    assert!(copy(&mut backend, track.loops[0], None, &[track.track_id]).is_ok());
}

#[shoop_test]
fn dry_wet_duplication_preserves_role_order_and_exact_sysex() {
    let mut description = description();
    description.external_dry_wet = true;
    let mut backend = stack(&description);
    let track = backend
        .create_track(TrackRequest {
            port_name_base: "roles".into(),
            topology: BackendTrackTopology::DryWetExternal {
                dry_audio_channels: 1,
                wet_audio_channels: 2,
                dry_midi: true,
            },
            initial_loops: 2,
        })
        .unwrap();
    backend.wait_idle();
    let update = BackendLoopContentUpdate {
        length: Some(4),
        audio: (0..3)
            .map(|channel| BackendAudioChannelUpdate {
                channel,
                samples: vec![(channel + 1) as f32 / 4.; 8].into(),
                start_offset: Some(1),
                capture_alignment_frames: Some(0),
                preplay: Some(1),
            })
            .collect(),
        midi: vec![BackendMidiChannelUpdate {
            channel: 0,
            length: 8,
            start_state: vec![vec![0xb0, 7, 99]],
            events: vec![BackendMidiEvent {
                frame: 2,
                data: vec![0xf0, 0x7d, 1, 2, 3, 0xf7].into(),
            }]
            .into(),
            start_offset: Some(0),
            capture_alignment_frames: Some(0),
            preplay: Some(1),
        }],
    };
    for _ in 0..2000 {
        match backend
            .replace_loop_content_async(track.loops[0], &update)
            .unwrap()
        {
            BackendAsyncResult::Ready(_) => break,
            BackendAsyncResult::Pending(_) => backend.clock_mut().render(1),
        }
    }
    let before = backend.capture_session().unwrap().tracks[0].loops[0].clone();
    copy(&mut backend, track.loops[0], Some(track.loops[1]), &[]).unwrap();
    let after = backend.capture_session().unwrap().tracks[0].loops[1].clone();
    assert_eq!(before.audio, after.audio);
    assert_eq!(before.midi, after.midi);
    assert_eq!(
        after.audio.iter().map(|c| c.mode).collect::<Vec<_>>(),
        vec![
            BackendChannelMode::Dry,
            BackendChannelMode::Wet,
            BackendChannelMode::Wet
        ]
    );
}

#[shoop_test]
fn replacing_playing_midi_emits_destination_cleanup_before_retirement() {
    let mut backend = stack(&description());
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "cleanup".into(),
            audio_channels: 0,
            midi: true,
            initial_loops: 2,
        })
        .unwrap();
    backend.wait_idle();
    let output = track
        .ports
        .iter()
        .find(|p| p.role == BackendPortRole::MidiOutput)
        .unwrap()
        .id;
    backend
        .set_port_connected(output, "midi_out_1", true)
        .unwrap();
    backend.wait_idle();
    let update = BackendLoopContentUpdate {
        length: Some(4096),
        audio: vec![],
        midi: vec![BackendMidiChannelUpdate {
            channel: 0,
            length: 4096,
            start_state: vec![vec![0x91, 62, 100]],
            events: vec![].into(),
            start_offset: Some(0),
            capture_alignment_frames: Some(0),
            preplay: Some(0),
        }],
    };
    for _ in 0..2000 {
        match backend
            .replace_loop_content_async(track.loops[1], &update)
            .unwrap()
        {
            BackendAsyncResult::Ready(_) => break,
            BackendAsyncResult::Pending(_) => backend.clock_mut().render(1),
        }
    }
    backend
        .transition_loop(track.loops[1], BackendLoopMode::Playing, None)
        .unwrap();
    backend.wait_idle();
    let mut cleanup = false;
    let mut completed = false;
    for _ in 0..3000 {
        match backend
            .duplicate_loop_async(track.loops[0], Some(track.loops[1]), &[], None)
            .unwrap()
        {
            BackendAsyncResult::Ready(_) => {
                completed = true;
                break;
            }
            BackendAsyncResult::Pending(_) => backend.clock_mut().render(1),
        }
        let output = backend.clock().offline.midi_output(0);
        cleanup |= (0..output.len()).any(|index| output.get(index).unwrap().1 == [0x81, 62, 0]);
    }
    assert!(
        completed && cleanup,
        "a playing destination must release its held note before its renderer is retired"
    );
}
