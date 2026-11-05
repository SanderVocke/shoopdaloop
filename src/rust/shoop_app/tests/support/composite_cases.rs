use super::*;
use shoop_app_protocol::composite as arrangement;

fn transition(loops: Vec<LoopId>, mode: LoopMode) -> Command {
    Command::Control(ControlOperation::Transition {
        loops,
        mode,
        cycles_delay: None,
        align_to_sync_at: None,
    })
}
fn setup_composites(encoded: bool) -> (Fixture, Client, TrackId, Vec<LoopId>) {
    let mut f = fixture();
    let mut client = attach(&mut f.service, 7001, ConnectionId(1), encoded);
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::Global(GlobalControlAction::SetSync(false))
        ),
        Outcome::Confirmed { .. }
    ));
    let Outcome::TrackCreated { track, loops, .. } = command(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        new_track("Arrangements", false),
    ) else {
        panic!("track creation");
    };
    for source in &loops[..2] {
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                transition(vec![*source], LoopMode::Recording)
            ),
            Outcome::Confirmed { .. }
        ));
        f.service.tick(STEP);
        f.service.tick(STEP);
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                transition(vec![*source], LoopMode::Stopped)
            ),
            Outcome::Confirmed { .. }
        ));
    }
    (f, client, track, loops)
}
fn edit(target: LoopId, add: Vec<LoopId>, parallel: bool) -> Command {
    Command::Composite(arrangement::Edit::Append {
        target,
        add,
        parallel,
        convert_if_empty: true,
    })
}

#[shoop_test]
fn composite_documents_geometry_and_pending_versions_publish_only_after_render_confirmation() {
    for encoded in [false, true] {
        let (mut f, mut client, _, loops) = setup_composites(encoded);
        let target = loops[2];
        let operation = submit(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            edit(target, loops[..2].to_vec(), false),
        );
        let snapshot = f.service.snapshot();
        let view = &snapshot.composites[0];
        assert_eq!(view.loop_id, target);
        assert!(view.active.is_none());
        assert!(view.pending.is_some());
        assert!(snapshot.valid());
        assert!(matches!(
            complete(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                operation,
                encoded
            ),
            Outcome::Confirmed { .. }
        ));
        let snapshot = f.service.snapshot();
        let view = &snapshot.composites[0];
        let active = view.active.as_ref().unwrap();
        assert!(view.pending.is_none());
        assert_eq!(active.document.instances.len(), 2);
        assert_eq!(
            active.prepared.instances[0].end_frame,
            active.prepared.instances[1].start_frame
        );
        let state = snapshot
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|entry| entry.id == target)
            .unwrap();
        assert_eq!(state.composite_kind, CompositeKind::Regular);
        assert_eq!(
            state.length_frames,
            active.prepared.unit_frames * active.prepared.iterations
        );
        assert!(snapshot.valid());
        assert_eq!(f.waits.get(), f.initial_waits);
    }
}

#[shoop_test]
fn editor_group_copy_delete_duration_and_modes_keep_stable_event_identity_and_refuse_atomically() {
    let (mut f, mut client, _, loops) = setup_composites(false);
    let target = loops[2];
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            edit(target, loops[..2].to_vec(), false)
        ),
        Outcome::Confirmed { .. }
    ));
    let active = f.service.snapshot().composites[0].active.clone().unwrap();
    let original = active
        .document
        .instances
        .iter()
        .map(|event| event.instance_id)
        .collect::<Vec<_>>();
    for value in [
        arrangement::Edit::Relocate {
            target,
            events: original.clone(),
            start_cycle: 150,
            duplicate: true,
        },
        arrangement::Edit::Duration {
            target,
            event: original[0],
            n_cycles: Some(12),
        },
        arrangement::Edit::Kind {
            target,
            kind: CompositeKind::Script,
        },
        arrangement::Edit::Mode {
            target,
            event: original[0],
            mode: LoopMode::Recording,
        },
    ] {
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                Command::Composite(value)
            ),
            Outcome::Confirmed { .. }
        ));
    }
    let active = f.service.snapshot().composites[0].active.clone().unwrap();
    assert_eq!(active.document.instances.len(), 4);
    assert_eq!(active.document.instances[0].n_cycles, Some(12));
    assert_eq!(active.document.instances[0].mode, Some(LoopMode::Recording));
    assert_eq!(
        active.document.instances[3].start_cycle - active.document.instances[2].start_cycle,
        active.document.instances[1].start_cycle - active.document.instances[0].start_cycle
    );
    let copied = active.document.instances[3].instance_id;
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::Composite(arrangement::Edit::Delete {
                target,
                events: vec![copied]
            })
        ),
        Outcome::Confirmed { .. }
    ));
    let before = f.service.snapshot().composites[0].active.clone().unwrap();
    for value in [
        arrangement::Edit::Delete {
            target,
            events: vec![original[0], copied],
        },
        arrangement::Edit::Insert {
            target,
            source: target,
            start_cycle: 0,
        },
        arrangement::Edit::Mode {
            target,
            event: original[0],
            mode: LoopMode::Replacing,
        },
    ] {
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                Command::Composite(value)
            ),
            Outcome::Refused { .. }
        ));
        assert_eq!(
            f.service.snapshot().composites[0].active.as_ref(),
            Some(&before)
        );
    }
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            edit(target, vec![loops[0]], false)
        ),
        Outcome::Confirmed { .. }
    ));
    let active = f.service.snapshot().composites[0].active.clone().unwrap();
    assert!(active.document.instances.last().unwrap().instance_id > copied);
}

#[shoop_test]
fn child_commands_do_not_move_parent_frames_and_new_authored_actions_still_apply() {
    let (mut f, mut client, _, loops) = setup_composites(true);
    let target = loops[2];
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            edit(target, loops[..2].to_vec(), false)
        ),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![target], LoopMode::Playing)
        ),
        Outcome::Confirmed { .. }
    ));
    let before = f
        .service
        .snapshot()
        .tracks
        .iter()
        .flat_map(|track| &track.loops)
        .find(|entry| entry.id == target)
        .unwrap()
        .position;
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![loops[0]], LoopMode::Stopped)
        ),
        Outcome::Confirmed { .. }
    ));
    let snapshot = f.service.snapshot();
    let parent = snapshot
        .tracks
        .iter()
        .flat_map(|track| &track.loops)
        .find(|entry| entry.id == target)
        .unwrap();
    assert_eq!(parent.mode, LoopMode::Playing);
    assert!(parent.position > before);
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![target], LoopMode::Recording)
        ),
        Outcome::Refused { .. }
    ));
}

#[shoop_test]
fn compose_then_play_in_one_client_turn_starts_the_installed_arrangement() {
    let (mut f, mut client, _, loops) = setup_composites(false);
    let target = loops[2];
    let compose = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        edit(target, vec![loops[0]], false),
    );
    let play = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        transition(vec![target], LoopMode::Playing),
    );
    assert!(matches!(
        complete(&mut f.service, &mut client, ConnectionId(1), compose, false),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        complete(&mut f.service, &mut client, ConnectionId(1), play, false),
        Outcome::Confirmed { .. }
    ));
    for _ in 0..3 {
        f.service.tick(STEP);
    }
    let snapshot = f.service.snapshot();
    let state = snapshot
        .tracks
        .iter()
        .flat_map(|track| &track.loops)
        .find(|entry| entry.id == target)
        .unwrap();
    assert_eq!(state.mode, LoopMode::Playing);
    assert_eq!(
        snapshot
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|entry| entry.id == loops[0])
            .unwrap()
            .mode,
        LoopMode::Playing
    );
}

#[shoop_test]
fn source_resize_does_not_retime_an_active_document_and_repeat_disable_retains_its_source() {
    let (mut f, mut client, _, loops) = setup_composites(false);
    let target = loops[2];
    let sync = f
        .service
        .snapshot()
        .tracks
        .iter()
        .find(|track| track.is_sync)
        .unwrap()
        .loops[0]
        .id;
    for mode in [LoopMode::Recording, LoopMode::Stopped] {
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                transition(vec![sync], mode)
            ),
            Outcome::Confirmed { .. }
        ));
    }
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::Global(GlobalControlAction::SetSync(true))
        ),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            edit(target, vec![loops[0]], false)
        ),
        Outcome::Confirmed { .. }
    ));
    let original = f.service.snapshot().composites[0].active.clone().unwrap();
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![target], LoopMode::Playing)
        ),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![sync], LoopMode::Recording)
        ),
        Outcome::Confirmed { .. }
    ));
    for _ in 0..8 {
        f.service.tick(STEP);
    }
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![sync], LoopMode::Stopped)
        ),
        Outcome::Confirmed { .. }
    ));
    assert_eq!(
        f.service.snapshot().composites[0].active.as_ref(),
        Some(&original)
    );
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::Control(ControlOperation::SetRepeatSync {
                loops: vec![target],
                active: false
            })
        ),
        Outcome::Confirmed { .. }
    ));
    let snapshot = f.service.snapshot();
    assert!(snapshot.sync_sources.contains(&(target, sync)));
    assert!(!snapshot.composites[0].repeat_sync);
    assert_eq!(
        snapshot
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|entry| entry.id == target)
            .unwrap()
            .mode,
        LoopMode::Playing
    );
}

#[shoop_test]
fn natural_child_duration_refresh_is_versioned_and_keeps_the_existing_basis() {
    let (mut f, mut client, _, loops) = setup_composites(false);
    let target = loops[2];
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            edit(target, vec![loops[0]], false)
        ),
        Outcome::Confirmed { .. }
    ));
    let original = f.service.snapshot().composites[0].active.clone().unwrap();
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![loops[0]], LoopMode::Recording)
        ),
        Outcome::Confirmed { .. }
    ));
    assert_eq!(
        f.service.snapshot().composites[0].active.as_ref(),
        Some(&original)
    );
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![loops[0]], LoopMode::Stopped)
        ),
        Outcome::Confirmed { .. }
    ));
    for _ in 0..12 {
        f.service.tick(STEP);
    }
    let refreshed = f.service.snapshot().composites[0].active.clone().unwrap();
    assert!(refreshed.version > original.version);
    assert_eq!(refreshed.document.basis, original.document.basis);
    assert!(refreshed.prepared.iterations < original.prepared.iterations);
}

#[shoop_test]
fn invalid_mixed_transport_or_iteration_refuses_the_complete_target_set() {
    let (mut f, mut client, _, loops) = setup_composites(false);
    for (target, cycles) in [(loops[2], 10), (loops[3], 2)] {
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                edit(target, vec![loops[0]], false)
            ),
            Outcome::Confirmed { .. }
        ));
        let event = f
            .service
            .snapshot()
            .composites
            .iter()
            .find(|view| view.loop_id == target)
            .unwrap()
            .active
            .as_ref()
            .unwrap()
            .document
            .instances[0]
            .instance_id;
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                Command::Composite(arrangement::Edit::Duration {
                    target,
                    event,
                    n_cycles: Some(cycles)
                })
            ),
            Outcome::Confirmed { .. }
        ));
    }
    for value in [
        transition(vec![loops[0], loops[2]], LoopMode::Recording),
        Command::Control(ControlOperation::Transition {
            loops: vec![loops[2], loops[3]],
            mode: LoopMode::Playing,
            cycles_delay: None,
            align_to_sync_at: Some(9),
        }),
    ] {
        assert!(matches!(
            command(&mut f.service, &mut client, ConnectionId(1), value),
            Outcome::Refused { .. }
        ));
        assert!(f
            .service
            .snapshot()
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .filter(|entry| [loops[0], loops[2], loops[3]].contains(&entry.id))
            .all(|entry| entry.mode == LoopMode::Stopped));
    }
}

#[shoop_test]
fn cancellation_racing_an_activation_reconciles_the_version_that_actually_installed() {
    let (mut f, mut client, _, loops) = setup_composites(false);
    let target = loops[2];
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            edit(target, vec![loops[0]], false)
        ),
        Outcome::Confirmed { .. }
    ));
    let event = f.service.snapshot().composites[0]
        .active
        .as_ref()
        .unwrap()
        .document
        .instances[0]
        .instance_id;
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::Composite(arrangement::Edit::Duration {
                target,
                event,
                n_cycles: Some(1000)
            })
        ),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![target], LoopMode::Playing)
        ),
        Outcome::Confirmed { .. }
    ));
    let edited = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Composite(arrangement::Edit::Insert {
            target,
            source: loops[3],
            start_cycle: 400,
        }),
    );
    for _ in 0..20 {
        f.service.tick(STEP);
        if f.service.snapshot().composites[0].pending_stage
            == Some(arrangement::PendingStage::AwaitingRestart)
        {
            break;
        }
    }
    let snapshot = f.service.snapshot();
    let view = &snapshot.composites[0];
    assert_eq!(
        view.pending_stage,
        Some(arrangement::PendingStage::AwaitingRestart)
    );
    let version = view.pending.as_ref().unwrap().version;
    f.advance.set(false);
    f.render.borrow_mut().run(1100, |_, _| 0.25).unwrap();
    let cancelled = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Composite(arrangement::Edit::Cancel { target, version }),
    );
    for _ in 0..8 {
        f.service.tick(Duration::ZERO);
        f.render.borrow_mut().run(16, |_, _| 0.25).unwrap();
    }
    f.service.tick(Duration::ZERO);
    drain(&mut f.service, &mut client, ConnectionId(1), false);
    assert!(matches!(
        client.results().get(&edited),
        Some(Outcome::Confirmed { .. })
    ));
    assert!(matches!(
        client.results().get(&cancelled),
        Some(Outcome::Refused { .. })
    ));
    assert_eq!(
        f.service.snapshot().composites[0]
            .active
            .as_ref()
            .unwrap()
            .version,
        version
    );
    assert!(f.service.snapshot().composites[0].pending.is_none());
    assert_eq!(f.waits.get(), f.initial_waits);
}

#[shoop_test]
fn nested_clear_stops_ancestors_preserves_child_media_and_restores_a_primitive_slot() {
    for encoded in [false, true] {
        let (mut f, mut client, track, loops) = setup_composites(encoded);
        let child = loops[2];
        let parent = loops[3];
        for (target, sources) in [(child, vec![loops[0]]), (parent, vec![child, loops[1]])] {
            assert!(matches!(
                command(
                    &mut f.service,
                    &mut client,
                    ConnectionId(1),
                    edit(target, sources, false)
                ),
                Outcome::Confirmed { .. }
            ));
        }
        let before = f.service.snapshot();
        let media = before
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|entry| entry.id == loops[0])
            .unwrap()
            .length_frames;
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                transition(vec![parent], LoopMode::Playing)
            ),
            Outcome::Confirmed { .. }
        ));
        let result = command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::Control(ControlOperation::ClearLoops { loops: vec![child] }),
        );
        assert!(
            matches!(result, Outcome::AncestorsStopped { ref loops, .. } if loops == &[parent]),
            "{result:?}"
        );
        let snapshot = f.service.snapshot();
        assert!(snapshot.valid());
        assert_eq!(snapshot.composites.len(), 1);
        let remaining = &snapshot.composites[0];
        assert_eq!(remaining.loop_id, parent);
        assert_eq!(remaining.mode, LoopMode::Stopped);
        assert_eq!(
            remaining.active.as_ref().unwrap().document.instances[0].loop_id,
            child
        );
        assert!(remaining.working.is_none());
        let entries = &snapshot
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .unwrap()
            .loops;
        let primitive = entries.iter().find(|entry| entry.id == child).unwrap();
        assert_eq!(primitive.composite_kind, CompositeKind::None);
        assert!(primitive.empty && primitive.has_audio && primitive.show_gain);
        assert_eq!(
            entries
                .iter()
                .find(|entry| entry.id == loops[0])
                .unwrap()
                .length_frames,
            media
        );
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                transition(vec![child], LoopMode::Recording)
            ),
            Outcome::Confirmed { .. }
        ));
        f.service.tick(STEP);
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                transition(vec![child], LoopMode::Stopped)
            ),
            Outcome::Confirmed { .. }
        ));
        assert!(
            f.service
                .snapshot()
                .tracks
                .iter()
                .flat_map(|entry| &entry.loops)
                .find(|entry| entry.id == child)
                .unwrap()
                .length_frames
                > 0
        );
        assert_eq!(f.waits.get(), f.initial_waits);
    }
}

#[shoop_test]
fn referenced_track_removal_prunes_events_and_keeps_surviving_nested_plans_stopped() {
    let (mut f, mut client, removed_track, sources) = setup_composites(false);
    let Outcome::TrackCreated { track, loops, .. } = command(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        new_track("Survivors", false),
    ) else {
        panic!("track creation")
    };
    let child = loops[0];
    let parent = loops[1];
    for (target, sources) in [(child, vec![sources[0], sources[1]]), (parent, vec![child])] {
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                edit(target, sources, false)
            ),
            Outcome::Confirmed { .. }
        ));
    }
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![parent], LoopMode::Playing)
        ),
        Outcome::Confirmed { .. }
    ));
    let result = command(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Track {
            track: removed_track,
            action: TrackAction::Remove,
        },
    );
    assert!(
        matches!(result, Outcome::AncestorsStopped { ref loops, .. } if loops.contains(&child) && loops.contains(&parent)),
        "{result:?}"
    );
    let snapshot = f.service.snapshot();
    assert!(snapshot.valid());
    assert!(snapshot
        .tracks
        .iter()
        .all(|entry| entry.id != removed_track));
    assert!(snapshot.tracks.iter().any(|entry| entry.id == track));
    for view in &snapshot.composites {
        assert_eq!(view.mode, LoopMode::Stopped);
    }
    assert!(snapshot
        .composites
        .iter()
        .find(|view| view.loop_id == child)
        .unwrap()
        .active
        .as_ref()
        .unwrap()
        .document
        .instances
        .is_empty());
    assert_eq!(
        snapshot
            .composites
            .iter()
            .find(|view| view.loop_id == parent)
            .unwrap()
            .active
            .as_ref()
            .unwrap()
            .document
            .instances[0]
            .loop_id,
        child
    );
    assert_eq!(f.waits.get(), f.initial_waits);
}

#[shoop_test]
fn refused_nested_clear_restores_the_old_plans_and_media_and_ui_loss_keeps_cleanup_owned() {
    let (mut f, mut client, _, loops) = setup_composites(false);
    for (target, sources) in [(loops[2], vec![loops[0]]), (loops[3], vec![loops[2]])] {
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                edit(target, sources, false)
            ),
            Outcome::Confirmed { .. }
        ));
    }
    let old = f.service.snapshot().composites.clone();
    f.refuse_next_lifecycle.set(true);
    let failed = command(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Control(ControlOperation::ClearLoops {
            loops: vec![loops[2]],
        }),
    );
    assert!(matches!(failed, Outcome::Refused { .. }), "{failed:?}");
    let restored = f.service.snapshot();
    assert!(restored.valid());
    for view in &old {
        let current = restored
            .composites
            .iter()
            .find(|current| current.loop_id == view.loop_id)
            .unwrap();
        assert_eq!(
            current.active.as_ref().unwrap().document,
            view.active.as_ref().unwrap().document
        );
        assert_eq!(
            current.active.as_ref().unwrap().prepared,
            view.active.as_ref().unwrap().prepared
        );
        assert!(current.pending.is_none());
    }
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Control(ControlOperation::ClearLoops {
            loops: vec![loops[2]],
        }),
    );
    f.service.detach(ConnectionId(1));
    for _ in 0..100 {
        f.service.tick(STEP);
    }
    let recovered = attach(&mut f.service, 7002, ConnectionId(2), true);
    let state = recovered.snapshot().unwrap();
    assert!(state.valid());
    assert!(state.pending.is_empty());
    assert_eq!(state.composites.len(), 1);
    assert!(
        matches!(&state.results.iter().find(|result| result.id == operation).unwrap().outcome, Outcome::AncestorsStopped { loops: stopped, .. } if stopped == &[loops[3]])
    );
    assert_eq!(f.waits.get(), f.initial_waits);
}

#[shoop_test]
fn production_client_projects_frozen_composite_geometry_without_media_spinner_and_routes_timing_edits(
) {
    let (mut f, mut client, track, loops) = setup_composites(false);
    let target = loops[2];
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            edit(target, vec![loops[0]], false)
        ),
        Outcome::Confirmed { .. }
    ));
    f.service.detach(ConnectionId(1));
    let (mut runtime, port) = runtime(
        &mut f.service,
        shoop_app_client::runtime::Startup::Fresh(Vec::new()),
    );
    runtime
        .dispatch(AppIntent::Loop {
            track_id: track,
            loop_id: target,
            action: LoopAction::IconClicked(SelectionModifiers::default()),
        })
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 8);
    let snapshot = runtime.snapshot();
    let details = snapshot.details.as_ref().unwrap();
    assert!(!details.loading && !details.midi_loading);
    assert!(details.channels.is_empty() && details.midi_channels.is_empty());
    let composite = details.composite.as_ref().unwrap();
    assert_eq!(composite.events.len(), 1);
    assert_eq!(composite.events[0].loop_id, loops[0]);
    assert_eq!(composite.timing.unwrap().basis_sample_rate, 1000);
    let version = composite.execution.active_version.unwrap();
    runtime
        .dispatch(AppIntent::SetCompositeTimingBasis {
            target_loop_id: target,
            sample_rate: 1000,
            unit_frames: 16,
        })
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 12);
    let snapshot = runtime.snapshot();
    let composite = snapshot
        .details
        .as_ref()
        .unwrap()
        .composite
        .as_ref()
        .unwrap();
    assert_eq!(composite.timing.unwrap().basis_frames, 16);
    assert!(composite.execution.active_version.unwrap() > version);
    assert!(
        snapshot.notifications.is_empty(),
        "{:?}",
        snapshot.notifications
    );
    assert_eq!(f.waits.get(), f.initial_waits);
}

#[shoop_test]
fn failed_parent_restoration_keeps_working_geometry_and_an_editor_retry_can_recover_it() {
    let (mut f, mut client, _, loops) = setup_composites(false);
    for (target, sources) in [(loops[2], vec![loops[0]]), (loops[3], vec![loops[2]])] {
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                edit(target, sources, false)
            ),
            Outcome::Confirmed { .. }
        ));
    }
    let original = f
        .service
        .snapshot()
        .composites
        .iter()
        .find(|view| view.loop_id == loops[3])
        .unwrap()
        .active
        .clone()
        .unwrap();
    f.refuse_composite_after.set(Some(0));
    let result = command(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Control(ControlOperation::ClearLoops {
            loops: vec![loops[2]],
        }),
    );
    assert!(matches!(result, Outcome::Refused { .. }), "{result:?}");
    let snapshot = f.service.snapshot();
    assert!(snapshot.valid());
    let parent = snapshot
        .composites
        .iter()
        .find(|view| view.loop_id == loops[3])
        .unwrap();
    assert!(parent
        .active
        .as_ref()
        .unwrap()
        .document
        .instances
        .is_empty());
    assert_eq!(parent.working.as_ref().unwrap().document, original.document);
    assert_eq!(parent.working.as_ref().unwrap().prepared, original.prepared);
    assert!(parent.error.is_some());
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::Composite(arrangement::Edit::Kind {
                target: loops[3],
                kind: CompositeKind::Script
            })
        ),
        Outcome::Confirmed { .. }
    ));
    let snapshot = f.service.snapshot();
    let parent = snapshot
        .composites
        .iter()
        .find(|view| view.loop_id == loops[3])
        .unwrap();
    assert!(parent.working.is_none());
    assert_eq!(parent.active.as_ref().unwrap().document.instances.len(), 1);
    assert_eq!(
        parent.active.as_ref().unwrap().document.kind,
        CompositeKind::Script
    );
}

#[shoop_test]
fn new_session_retires_composite_owners_and_fresh_slots_remain_usable() {
    let (mut f, mut client, _, loops) = setup_composites(false);
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            edit(loops[2], vec![loops[0]], false)
        ),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![loops[2]], LoopMode::Playing)
        ),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::NewSession
        ),
        Outcome::Confirmed { .. }
    ));
    let snapshot = f.service.snapshot();
    assert!(snapshot.valid());
    assert!(snapshot.composites.is_empty());
    assert!(snapshot
        .tracks
        .iter()
        .flat_map(|track| &track.loops)
        .all(|entry| !loops.contains(&entry.id)));
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            new_track("After composite", false)
        ),
        Outcome::TrackCreated { .. }
    ));
    assert_eq!(f.waits.get(), f.initial_waits);
}

#[shoop_test]
fn unrelated_recording_can_be_admitted_during_owned_composite_cleanup() {
    let (mut f, mut client, _, loops) = setup_composites(false);
    for (target, sources) in [(loops[2], vec![loops[0]]), (loops[3], vec![loops[2]])] {
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                edit(target, sources, false)
            ),
            Outcome::Confirmed { .. }
        ));
    }
    let clear = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Control(ControlOperation::ClearLoops {
            loops: vec![loops[2]],
        }),
    );
    let record = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        transition(vec![loops[4]], LoopMode::Recording),
    );
    assert!(matches!(
        complete(&mut f.service, &mut client, ConnectionId(1), record, false),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        complete(&mut f.service, &mut client, ConnectionId(1), clear, false),
        Outcome::AncestorsStopped { .. }
    ));
    assert_eq!(
        f.service
            .snapshot()
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|entry| entry.id == loops[4])
            .unwrap()
            .mode,
        LoopMode::Recording
    );
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![loops[4]], LoopMode::Stopped)
        ),
        Outcome::Confirmed { .. }
    ));
}

#[shoop_test]
fn shutdown_settles_a_waiting_owned_edit_after_stopping_the_composite() {
    let (mut f, mut client, _, loops) = setup_composites(false);
    let target = loops[2];
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            edit(target, vec![loops[0]], false)
        ),
        Outcome::Confirmed { .. }
    ));
    let event = f.service.snapshot().composites[0]
        .active
        .as_ref()
        .unwrap()
        .document
        .instances[0]
        .instance_id;
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::Composite(arrangement::Edit::Duration {
                target,
                event,
                n_cycles: Some(1000)
            })
        ),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![target], LoopMode::Playing)
        ),
        Outcome::Confirmed { .. }
    ));
    let edit = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Composite(arrangement::Edit::Duration {
            target,
            event,
            n_cycles: Some(2000),
        }),
    );
    let request = client.shutdown().unwrap();
    f.service.handle(ConnectionId(1), request).unwrap();
    for _ in 0..100 {
        f.service.tick(STEP);
        if f.service.shutdown_ready()
            && f.service
                .snapshot()
                .results
                .iter()
                .any(|result| result.id == edit)
        {
            break;
        }
    }
    assert!(f.service.shutdown_ready());
    let snapshot = f.service.snapshot();
    assert!(snapshot.valid());
    assert!(snapshot
        .tracks
        .iter()
        .flat_map(|track| &track.loops)
        .all(|entry| entry.mode == LoopMode::Stopped));
    assert!(snapshot
        .results
        .iter()
        .any(|result| result.id == edit && matches!(result.outcome, Outcome::Confirmed { .. })));
    f.service.complete_shutdown(Ok(()));
    assert_eq!(f.waits.get(), f.initial_waits);
}

#[shoop_test]
fn automatic_monitoring_and_recording_finish_without_ui_or_service_polling() {
    let (mut f, mut client, track, loops) = setup_composites(false);
    let target = loops[2];
    let child = loops[0];
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            edit(target, vec![child], false)
        ),
        Outcome::Confirmed { .. }
    ));
    let event = f.service.snapshot().composites[0]
        .active
        .as_ref()
        .unwrap()
        .document
        .instances[0]
        .instance_id;
    for edit in [
        arrangement::Edit::Kind {
            target,
            kind: CompositeKind::Script,
        },
        arrangement::Edit::Duration {
            target,
            event,
            n_cycles: Some(4),
        },
        arrangement::Edit::Mode {
            target,
            event,
            mode: LoopMode::Recording,
        },
    ] {
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                Command::Composite(edit)
            ),
            Outcome::Confirmed { .. }
        ));
    }
    f.advance.set(false);
    let start = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        transition(vec![target], LoopMode::Playing),
    );
    f.service.tick(Duration::ZERO);
    f.service.detach(ConnectionId(1));
    let mut output = Vec::new();
    for _ in 0..64 {
        let mut render = f.render.borrow_mut();
        render.input_mut(0).fill(0.5);
        render.process(1).unwrap();
        output.push(render.output(0)[0]);
    }
    assert!(output[..4].iter().any(|sample| *sample > 0.01));
    assert!(output[12..].iter().all(|sample| sample.abs() < 0.00001));
    f.service.tick(Duration::ZERO);
    let recovered = attach(&mut f.service, 7003, ConnectionId(2), true);
    let snapshot = recovered.snapshot().unwrap();
    assert!(snapshot.valid());
    assert!(matches!(
        &snapshot
            .results
            .iter()
            .find(|result| result.id == start)
            .unwrap()
            .outcome,
        Outcome::Confirmed { .. }
    ));
    assert!(snapshot.pending.is_empty());
    let view = snapshot
        .tracks
        .iter()
        .find(|entry| entry.id == track)
        .unwrap();
    assert!(!view.controls.input_monitoring);
    assert_eq!(
        view.loops
            .iter()
            .find(|entry| entry.id == child)
            .unwrap()
            .length_frames,
        4
    );
    assert_eq!(snapshot.composites[0].mode, LoopMode::Stopped);
    f.advance.set(true);
    let authority = recovered.authority().unwrap();
    let mut audio = None;
    for _ in 0..100 {
        audio = f
            .service
            .read_audio(ConnectionId(2), authority, child)
            .unwrap()
            .or(audio);
        if audio.is_some() {
            break;
        }
        f.service.tick(STEP);
    }
    assert_eq!(
        audio.expect("recorded audio").channels[0]
            .samples
            .as_slice(),
        &[0.5; 4]
    );
    assert_eq!(f.waits.get(), f.initial_waits);
}

#[shoop_test]
fn refused_automatic_policy_is_reconciled_without_losing_manual_monitoring_or_recording() {
    let (mut f, mut client, track, loops) = setup_composites(false);
    let manual = |enabled| Command::Track {
        track,
        action: TrackAction::InputMonitoringChanged {
            enabled,
            respect_auto_mute: false,
        },
    };
    assert!(matches!(
        command(&mut f.service, &mut client, ConnectionId(1), manual(true)),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![loops[4]], LoopMode::Recording)
        ),
        Outcome::Confirmed { .. }
    ));
    f.refuse_next_monitoring.set(true);
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::Global(GlobalControlAction::SetAutoArmTrackInputs(false))
        ),
        Outcome::Refused { .. }
    ));
    for _ in 0..8 {
        f.service.tick(STEP);
    }
    assert!(
        f.service
            .snapshot()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .unwrap()
            .controls
            .input_monitoring
    );
    assert!(matches!(
        command(&mut f.service, &mut client, ConnectionId(1), manual(false)),
        Outcome::Confirmed { .. }
    ));
    let state = f.service.snapshot();
    let entry = state.tracks.iter().find(|entry| entry.id == track).unwrap();
    assert!(!entry.controls.input_monitoring);
    assert_eq!(
        entry
            .loops
            .iter()
            .find(|entry| entry.id == loops[4])
            .unwrap()
            .mode,
        LoopMode::Recording
    );
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::Global(GlobalControlAction::SetAutoArmTrackInputs(true))
        ),
        Outcome::Confirmed { .. }
    ));
    for _ in 0..4 {
        f.service.tick(STEP);
    }
    assert!(
        f.service
            .snapshot()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .unwrap()
            .controls
            .input_monitoring
    );
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            transition(vec![loops[4]], LoopMode::Stopped)
        ),
        Outcome::Confirmed { .. }
    ));
    for _ in 0..4 {
        f.service.tick(STEP);
    }
    assert!(
        !f.service
            .snapshot()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .unwrap()
            .controls
            .input_monitoring
    );
    assert_eq!(f.waits.get(), f.initial_waits);
}
