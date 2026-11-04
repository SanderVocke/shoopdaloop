mod support;
use shoop_backend::*;
use shoop_wasm_test_support::shoop_test;
use support::*;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn finish(backend: &mut Stack, update: &BackendLoopFamilyUpdate) -> anyhow::Result<()> {
    for _ in 0..4000 {
        match backend.replace_loop_family_async(update)? {
            BackendAsyncResult::Ready(()) => return Ok(()),
            BackendAsyncResult::Pending(_) => backend.clock_mut().render(16),
        }
    }
    panic!("family import did not settle");
}

fn fixture(quantum: u32) -> (Stack, BackendLoopFamilyUpdate, BackendLoopId) {
    let mut description = description();
    description.quantum = quantum as usize;
    let mut backend = stack(&description);
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "family".into(),
            audio_channels: 1,
            midi: true,
            initial_loops: 3,
        })
        .unwrap();
    backend.wait_idle();
    let root = track.loops[0];
    let child = track.loops[1];
    backend.set_loop_sync_source(child, Some(root)).unwrap();
    backend.wait_idle();
    for (id, cycles) in [(root, 3), (child, 12)] {
        backend
            .replace_loop_content(
                id,
                &BackendLoopContentUpdate {
                    length: Some(cycles * quantum),
                    audio: vec![BackendAudioChannelUpdate {
                        channel: 0,
                        samples: vec![0.25; (cycles * quantum + 4) as usize].into(),
                        start_offset: Some(2),
                        capture_alignment_frames: None,
                        preplay: None,
                    }],
                    ..Default::default()
                },
            )
            .unwrap();
    }
    backend.wait_idle();
    let saved = backend.capture_session().unwrap();
    let snapshot = backend.poll().unwrap();
    assert_eq!(snapshot.loops[&child].sync_source, Some(root));
    let members = saved.tracks[0]
        .loops
        .iter()
        .filter(|content| content.source_id != track.loops[2].raw())
        .map(|content| {
            let id = BackendLoopId::from_raw(content.source_id);
            let mut next = content.clone();
            next.length = if id == root {
                4 * quantum
            } else {
                16 * quantum
            };
            next.audio[0].samples = vec![0.5; next.length as usize + 4].into();
            BackendLoopFamilyMember {
                loop_id: id,
                sync_source: snapshot.loops[&id].sync_source,
                expected_revision: snapshot.loops[&id].content_revision,
                expected_frames: content.length,
                fence_content: true,
                restart: id == root,
                content: Some(next),
            }
        })
        .collect();
    (
        backend,
        BackendLoopFamilyUpdate {
            root,
            sample_rate: 1000,
            quantum_frames: quantum,
            members,
        },
        track.loops[2],
    )
}

#[shoop_test]
fn adapter_family_preserves_canonical_history_controls_and_unrelated_recording() {
    for quantum in [16, 32, 64] {
        let (mut backend, update, unrelated) = fixture(quantum);
        backend
            .transition_loop(unrelated, BackendLoopMode::Recording, None)
            .unwrap();
        backend.wait_idle();
        assert!(matches!(
            backend.replace_loop_family_async(&update).unwrap(),
            BackendAsyncResult::Pending(_)
        ));
        backend.set_loop_gain(update.root, 0.75).unwrap();
        finish(&mut backend, &update).unwrap();
        let state = backend.poll().unwrap();
        assert_eq!(state.loops[&unrelated].mode, BackendLoopMode::Recording);
        assert_eq!(state.loops[&update.root].gain, 0.75);
        for member in &update.members {
            assert_eq!(
                state.loops[&member.loop_id].length,
                member.content.as_ref().unwrap().length
            );
            let data = backend
                .loop_audio_data_with_metadata(member.loop_id)
                .unwrap()
                .unwrap();
            assert_eq!(
                data.channels[0].samples,
                member.content.as_ref().unwrap().audio[0].samples
            );
            assert_eq!(data.channels[0].start_offset, 2);
        }
    }
}

#[shoop_test]
fn adapter_family_cancellation_and_stale_fences_preserve_live_content() {
    let (mut backend, update, _) = fixture(32);
    let before = backend.capture_session().unwrap();
    let mut stale = update.clone();
    stale.members[0].expected_revision += 1;
    assert!(backend.replace_loop_family_async(&stale).is_err());
    assert!(matches!(
        backend.replace_loop_family_async(&update).unwrap(),
        BackendAsyncResult::Pending(_)
    ));
    backend.cancel_session_replacement().unwrap();
    assert!(finish(&mut backend, &update)
        .unwrap_err()
        .to_string()
        .contains("cancelled"));
    assert_eq!(backend.capture_session().unwrap(), before);
    finish(&mut backend, &update).unwrap();
}

#[shoop_test]
fn adapter_family_refuses_incomplete_closure_and_content_changed_after_staging() {
    let (mut backend, update, _) = fixture(32);
    let mut incomplete = update.clone();
    incomplete
        .members
        .retain(|member| member.loop_id == update.root);
    assert!(finish(&mut backend, &incomplete).is_err());
    assert_eq!(backend.poll().unwrap().loops[&update.root].length, 96);
    assert!(matches!(
        backend.replace_loop_family_async(&update).unwrap(),
        BackendAsyncResult::Pending(_)
    ));
    backend.clear_loop(update.root).unwrap();
    assert!(finish(&mut backend, &update).is_err());
    let state = backend.poll().unwrap();
    assert_eq!(state.loops[&update.root].length, 0);
    let child = update
        .members
        .iter()
        .find(|member| member.loop_id != update.root)
        .unwrap();
    assert_eq!(state.loops[&child.loop_id].length, 384);
}
