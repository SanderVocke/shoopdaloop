use super::*;
use shoop_app_protocol::composite::{Edit, Versioned};

fn run(f: &mut Fixture, client: &mut Client, command: Command, encoded: bool) -> Outcome {
    let id = if encoded {
        let message = client.command(command, None).unwrap();
        let ClientMessage::Submit { operation, .. } = &message else {
            panic!()
        };
        let id = *operation;
        let mut bytes = [0; shoop_transport::MAX_RECORD_BYTES];
        let used = codec::encode(&WireMessage::Client(message), &mut bytes).unwrap();
        let WireMessage::Client(message) = codec::decode(&bytes[..used]).unwrap() else {
            panic!()
        };
        f.service.handle(ConnectionId(1), message).unwrap();
        id
    } else {
        submit(&mut f.service, client, ConnectionId(1), command)
    };
    complete(&mut f.service, client, ConnectionId(1), id, encoded)
}
fn action(track: TrackId, loop_id: LoopId, action: LoopAction) -> Command {
    Command::Loop {
        track,
        loop_id,
        action,
    }
}
fn clear_then_copy(
    f: &mut Fixture,
    client: &mut Client,
    track: TrackId,
    source: LoopId,
    target: LoopId,
    encoded: bool,
) -> Outcome {
    let cleared = run(
        f,
        client,
        Command::Control(ControlOperation::ClearLoops {
            loops: vec![target],
        }),
        encoded,
    );
    if matches!(cleared, Outcome::Refused { .. }) {
        return cleared;
    }
    run(
        f,
        client,
        action(track, source, LoopAction::DuplicateTo(target)),
        encoded,
    )
}
fn setup(encoded: bool) -> (Fixture, Client, TrackId, Vec<LoopId>) {
    let mut f = fixture();
    let mut client = attach(&mut f.service, 1, ConnectionId(1), encoded);
    run(
        &mut f,
        &mut client,
        Command::Global(GlobalControlAction::SetSync(false)),
        encoded,
    );
    let (track, loops) = created(run(&mut f, &mut client, new_track("Copies", true), encoded));
    for (id, ticks) in [(loops[0], 4), (loops[1], 1)] {
        run(
            &mut f,
            &mut client,
            action(track, id, LoopAction::RecordClicked),
            encoded,
        );
        for _ in 0..ticks {
            f.service.tick(STEP);
        }
        run(
            &mut f,
            &mut client,
            action(track, id, LoopAction::StopClicked),
            encoded,
        );
    }
    (f, client, track, loops)
}
fn view(f: &Fixture, id: LoopId) -> LoopState {
    f.service
        .snapshot()
        .tracks
        .iter()
        .flat_map(|t| t.loops.iter())
        .find(|l| l.id == id)
        .unwrap()
        .clone()
}
fn audio(f: &mut Fixture, client: &Client, id: LoopId) -> shoop_backend::BackendAudioData {
    for _ in 0..100 {
        if let Some(data) = f
            .service
            .read_audio(ConnectionId(1), client.authority().unwrap(), id)
            .unwrap()
        {
            return data;
        }
        f.service.tick(STEP);
    }
    panic!("media read did not finish")
}

#[shoop_test]
fn primitive_copy_overwrites_playing_media_keeps_slot_selection_and_unrelated_recording() {
    for encoded in [false, true] {
        let (mut f, mut client, track, loops) = setup(encoded);
        for change in [
            LoopAction::NameChanged("A copy".into()),
            LoopAction::GainChanged(0.42),
            LoopAction::BalanceChanged(-0.25),
        ] {
            run(
                &mut f,
                &mut client,
                action(track, loops[0], change),
                encoded,
            );
        }
        run(
            &mut f,
            &mut client,
            Command::SelectLoops {
                loops: vec![loops[1]],
            },
            encoded,
        );
        run(
            &mut f,
            &mut client,
            Command::Control(ControlOperation::Transition {
                loops: vec![loops[1]],
                mode: LoopMode::Playing,
                cycles_delay: None,
                align_to_sync_at: None,
            }),
            encoded,
        );
        run(
            &mut f,
            &mut client,
            Command::Control(ControlOperation::Transition {
                loops: vec![loops[2]],
                mode: LoopMode::Recording,
                cycles_delay: None,
                align_to_sync_at: None,
            }),
            encoded,
        );
        let source = audio(&mut f, &client, loops[0]);
        let before = view(&f, loops[1]);
        let outcome = run(
            &mut f,
            &mut client,
            action(track, loops[0], LoopAction::DuplicateTo(loops[1])),
            encoded,
        );
        assert!(matches!(outcome, Outcome::Confirmed { .. }), "{outcome:?}");
        let copied = view(&f, loops[1]);
        assert_eq!(copied.id, loops[1]);
        assert_eq!(copied.name, "A copy");
        assert_eq!((copied.gain, copied.balance), (0.42, -0.25));
        assert!(copied.content_revision > before.content_revision);
        assert_eq!(f.content_applies.get(), 1);
        assert!(copied.selected);
        assert_eq!((copied.mode, copied.position), (LoopMode::Stopped, 0.));
        assert_eq!(view(&f, loops[2]).mode, LoopMode::Recording);
        assert_eq!(audio(&mut f, &client, loops[1]), source);
        run(
            &mut f,
            &mut client,
            Command::SelectLoops { loops: vec![] },
            encoded,
        );
        run(
            &mut f,
            &mut client,
            Command::Control(ControlOperation::ClearLoops {
                loops: vec![loops[0]],
            }),
            encoded,
        );
        assert_eq!(audio(&mut f, &client, loops[1]), source);
        let outcome = run(
            &mut f,
            &mut client,
            action(track, loops[0], LoopAction::DuplicateTo(loops[1])),
            encoded,
        );
        assert!(matches!(outcome, Outcome::Confirmed { .. }), "{outcome:?}");
        assert!(view(&f, loops[1]).empty);
        assert!(audio(&mut f, &client, loops[1])
            .channels
            .iter()
            .all(|c| c.samples.is_empty()));
        assert_eq!(f.waits.get(), f.initial_waits);
    }
}

#[shoop_test]
fn copy_reports_failures_on_both_sides_of_media_commit_and_does_not_replay() {
    for failure in [CopyFailure::Stop, CopyFailure::Apply, CopyFailure::Gain] {
        let (mut f, mut client, track, loops) = setup(true);
        let before = audio(&mut f, &client, loops[1]);
        let source = audio(&mut f, &client, loops[0]);
        let revision = view(&f, loops[1]).content_revision;
        f.refuse_copy.set(Some(failure));
        let command = action(track, loops[0], LoopAction::DuplicateTo(loops[1]));
        let message = client.command(command, None).unwrap();
        let ClientMessage::Submit { operation, .. } = message.clone() else {
            panic!()
        };
        f.service.handle(ConnectionId(1), message.clone()).unwrap();
        let outcome = complete(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            operation,
            true,
        );
        let Outcome::Refused {
            message: reason, ..
        } = &outcome
        else {
            panic!("{outcome:?}")
        };
        if matches!(failure, CopyFailure::Gain) {
            assert!(reason.contains("Content copied"), "{reason}");
            assert_eq!(audio(&mut f, &client, loops[1]), source);
            assert!(view(&f, loops[1]).content_revision > revision);
        } else {
            assert_eq!(audio(&mut f, &client, loops[1]), before);
        }
        let completed_revision = view(&f, loops[1]).content_revision;
        let applies = f.content_applies.get();
        f.service.handle(ConnectionId(1), message).unwrap();
        for _ in 0..5 {
            f.service.tick(STEP);
        }
        drain(&mut f.service, &mut client, ConnectionId(1), true);
        assert_eq!(client.results()[&operation], outcome);
        assert_eq!(f.content_applies.get(), applies);
        assert_eq!(view(&f, loops[1]).content_revision, completed_revision);
    }
}

#[shoop_test]
fn clone_selects_empty_slots_adds_aligned_rows_and_prunes_failed_creation() {
    let (mut f, mut client, track, loops) = setup(false);
    let (_, other) = created(run(&mut f, &mut client, new_track("Aligned", false), false));
    assert!(matches!(
        run(
            &mut f,
            &mut client,
            action(track, loops[0], LoopAction::Duplicate),
            false
        ),
        Outcome::Confirmed { .. }
    ));
    assert_eq!(
        audio(&mut f, &client, loops[2]),
        audio(&mut f, &client, loops[0])
    );
    let source = *loops.last().unwrap();
    for fail in [true, false] {
        let count = f
            .service
            .snapshot()
            .tracks
            .iter()
            .find(|t| t.id == track)
            .unwrap()
            .loops
            .len();
        f.refuse_next_loop.set(fail);
        let outcome = run(
            &mut f,
            &mut client,
            action(track, source, LoopAction::Duplicate),
            false,
        );
        assert_eq!(
            matches!(outcome, Outcome::Refused { .. }),
            fail,
            "{outcome:?}"
        );
        let state = f.service.snapshot();
        assert_eq!(
            state
                .tracks
                .iter()
                .find(|t| t.id == track)
                .unwrap()
                .loops
                .len(),
            count + usize::from(!fail)
        );
        assert!(state
            .tracks
            .iter()
            .flat_map(|t| &t.loops)
            .all(|l| l.structural_state == StructuralState::Confirmed));
    }
    assert!(f
        .service
        .snapshot()
        .tracks
        .iter()
        .any(|t| t.loops.iter().any(|l| l.id == other[0])));
}

#[shoop_test]
fn accepted_copy_survives_disconnect_before_capture_and_after_application() {
    for ticks in [0, 8] {
        let (mut f, mut client, track, loops) = setup(true);
        let before = view(&f, loops[1]).content_revision;
        let message = client
            .command(
                action(track, loops[0], LoopAction::DuplicateTo(loops[1])),
                None,
            )
            .unwrap();
        let ClientMessage::Submit { operation, .. } = message.clone() else {
            panic!()
        };
        f.service.handle(ConnectionId(1), message).unwrap();
        for _ in 0..ticks {
            f.service.tick(STEP);
        }
        let conflict = run(&mut f, &mut client, Command::AddLoop { track }, true);
        assert!(matches!(
            conflict,
            Outcome::Refused {
                reason: Refusal::Busy,
                ..
            }
        ));
        f.service.detach(ConnectionId(1));
        for _ in 0..100 {
            f.service.tick(STEP);
        }
        let mut reconnect = attach(&mut f.service, 2, ConnectionId(2), true);
        assert!(view(&f, loops[1]).content_revision > before);
        assert_eq!(f.content_applies.get(), 1);
        assert_eq!(
            view(&f, loops[1]).length_frames,
            view(&f, loops[0]).length_frames
        );
        assert!(reconnect.snapshot().unwrap().pending.is_empty());
        drain(&mut f.service, &mut reconnect, ConnectionId(2), true);
        assert!(f
            .service
            .snapshot()
            .results
            .iter()
            .any(|result| result.id == operation
                && matches!(result.outcome, Outcome::Confirmed { .. })));
    }
}

#[shoop_test]
fn copy_refuses_stale_cross_track_and_recording_targets_before_changing_them() {
    let (mut f, mut client, track, loops) = setup(false);
    let (_, other) = created(run(&mut f, &mut client, new_track("Other", true), false));
    let before = audio(&mut f, &client, loops[1]);
    for target in [loops[0], other[0], LoopId::from_raw(u64::MAX)] {
        assert!(matches!(
            run(
                &mut f,
                &mut client,
                action(track, loops[0], LoopAction::DuplicateTo(target)),
                false
            ),
            Outcome::Refused { .. }
        ));
    }
    for id in [loops[0], loops[1]] {
        run(
            &mut f,
            &mut client,
            action(track, id, LoopAction::RecordClicked),
            false,
        );
        assert!(matches!(
            run(
                &mut f,
                &mut client,
                action(track, loops[0], LoopAction::DuplicateTo(loops[1])),
                false
            ),
            Outcome::Refused {
                reason: Refusal::Busy,
                ..
            }
        ));
        assert_eq!(view(&f, id).mode, LoopMode::Recording);
        if id == loops[0] {
            assert_eq!(audio(&mut f, &client, loops[1]), before);
        }
        run(
            &mut f,
            &mut client,
            action(track, id, LoopAction::StopClicked),
            false,
        );
    }
    assert_eq!(f.content_applies.get(), 0);
}

#[shoop_test]
fn copy_reports_an_unknown_engine_outcome_without_retrying_the_apply() {
    let (mut f, mut client, track, loops) = setup(false);
    f.refuse_copy.set(Some(CopyFailure::Disconnect));
    let outcome = run(
        &mut f,
        &mut client,
        action(track, loops[0], LoopAction::DuplicateTo(loops[1])),
        false,
    );
    let Outcome::Refused { message, .. } = outcome else {
        panic!("{outcome:?}")
    };
    assert!(message.contains("unknown"), "{message}");
    assert_eq!(f.content_applies.get(), 1);
    assert!(f.service.snapshot().pending.is_empty());
}

fn definition(f: &Fixture, id: LoopId) -> Versioned {
    f.service
        .snapshot()
        .composites
        .iter()
        .find(|view| view.loop_id == id)
        .unwrap()
        .active
        .clone()
        .unwrap()
}
fn append(target: LoopId, add: Vec<LoopId>) -> Command {
    Command::Composite(Edit::Append {
        target,
        add,
        parallel: false,
        convert_if_empty: true,
    })
}
fn copied_definition(source: &Versioned, target: &Versioned) {
    assert_eq!(source.document.kind, target.document.kind);
    assert_eq!(source.document.basis, target.document.basis);
    assert_eq!(source.prepared.spans, target.prepared.spans);
    assert_eq!(source.prepared.unit_frames, target.prepared.unit_frames);
    for (a, b) in source
        .document
        .instances
        .iter()
        .zip(&target.document.instances)
    {
        assert_ne!(a.instance_id, b.instance_id);
        let mut expected = a.clone();
        expected.instance_id = b.instance_id;
        assert_eq!(&expected, b);
    }
    assert_eq!(
        source.document.instances.len(),
        target.document.instances.len()
    );
    assert_eq!(
        source
            .prepared
            .instances
            .iter()
            .map(|i| (i.start_frame, i.end_frame))
            .collect::<Vec<_>>(),
        target
            .prepared
            .instances
            .iter()
            .map(|i| (i.start_frame, i.end_frame))
            .collect::<Vec<_>>()
    );
}

#[shoop_test]
fn normal_clear_then_copy_supports_both_kind_changes() {
    for encoded in [false, true] {
        let (mut f, mut client, track, loops) = setup(encoded);
        run(
            &mut f,
            &mut client,
            append(loops[2], vec![loops[0]]),
            encoded,
        );
        let source = definition(&f, loops[2]);
        let outcome = clear_then_copy(&mut f, &mut client, track, loops[2], loops[1], encoded);
        assert!(matches!(outcome, Outcome::Confirmed { .. }), "{outcome:?}");
        copied_definition(&source, &definition(&f, loops[1]));
        let media = audio(&mut f, &client, loops[0]);
        assert!(matches!(
            clear_then_copy(&mut f, &mut client, track, loops[0], loops[2], encoded),
            Outcome::Confirmed { .. }
        ));
        assert_eq!(audio(&mut f, &client, loops[2]), media);
        assert_eq!(
            (view(&f, loops[2]).mode, view(&f, loops[2]).position),
            (LoopMode::Stopped, 0.)
        );
    }
}

#[shoop_test]
fn p17_copies_processed_modes_and_nested_definitions_without_cloning_children() {
    for encoded in [false, true] {
        for mode in [
            LoopMode::Replacing,
            LoopMode::PlayingDryThroughWet,
            LoopMode::RecordingDryIntoWet,
        ] {
            let mut f = fixture_with_dry_wet(2, true);
            let mut client = attach(&mut f.service, 1, ConnectionId(1), encoded);
            run(
                &mut f,
                &mut client,
                Command::Global(GlobalControlAction::SetSync(false)),
                encoded,
            );
            let (track, loops) = created(run(
                &mut f,
                &mut client,
                Command::AddTrack(TrackSpec {
                    name: "P17 copies".into(),
                    topology: TrackSpecTopology::DryWet {
                        dry_audio_channels: 1,
                        wet_audio_channels: 2,
                        dry_midi: true,
                        processor_type: TrackProcessorTypeId::new(TrackProcessorTypeId::EXTERNAL),
                        default_playback_mode: DefaultPlaybackMode::DryThroughWet,
                    },
                    latency: Default::default(),
                    initial_output_bus_names: Vec::new(),
                    creation_request_id: None,
                }),
                encoded,
            ));
            for action_ in [LoopAction::RecordClicked, LoopAction::StopClicked] {
                let result = run(
                    &mut f,
                    &mut client,
                    action(track, loops[0], action_),
                    encoded,
                );
                assert!(matches!(result, Outcome::Confirmed { .. }), "{result:?}");
                f.service.tick(STEP);
            }
            run(
                &mut f,
                &mut client,
                append(loops[2], vec![loops[0]]),
                encoded,
            );
            let event = definition(&f, loops[2]).document.instances[0].instance_id;
            for edit in [
                Edit::Kind {
                    target: loops[2],
                    kind: CompositeKind::Script,
                },
                Edit::Mode {
                    target: loops[2],
                    event,
                    mode,
                },
                Edit::Duration {
                    target: loops[2],
                    event,
                    n_cycles: Some(10_000),
                },
            ] {
                let result = run(&mut f, &mut client, Command::Composite(edit), encoded);
                assert!(matches!(result, Outcome::Confirmed { .. }), "{result:?}");
            }
            run(
                &mut f,
                &mut client,
                append(loops[3], vec![loops[2]]),
                encoded,
            );
            for (source, target) in [(loops[2], loops[4]), (loops[3], loops[5])] {
                let pinned = definition(&f, source);
                let result = run(
                    &mut f,
                    &mut client,
                    action(track, source, LoopAction::DuplicateTo(target)),
                    encoded,
                );
                assert!(matches!(result, Outcome::Confirmed { .. }), "{result:?}");
                copied_definition(&pinned, &definition(&f, target));
                assert_eq!(view(&f, target).mode, LoopMode::Stopped);
            }
            let copied = definition(&f, loops[4]);
            let event = definition(&f, loops[2]).document.instances[0].instance_id;
            run(
                &mut f,
                &mut client,
                Command::Composite(Edit::Delete {
                    target: loops[2],
                    events: vec![event],
                }),
                encoded,
            );
            assert_eq!(definition(&f, loops[4]), copied);
        }
    }
}

#[shoop_test]
fn p03_copies_empty_regular_script_and_nested_definitions_with_independent_ids() {
    for kind in 0..4 {
        let (mut f, mut client, track, loops) = setup(true);
        let mut source = loops[2];
        if kind == 0 {
            run(
                &mut f,
                &mut client,
                Command::Composite(Edit::Convert { target: source }),
                true,
            );
        } else {
            assert!(matches!(
                run(
                    &mut f,
                    &mut client,
                    append(source, vec![loops[0], loops[1], loops[0]]),
                    true
                ),
                Outcome::Confirmed { .. }
            ));
        }
        if kind == 2 {
            run(
                &mut f,
                &mut client,
                Command::Composite(Edit::Kind {
                    target: source,
                    kind: CompositeKind::Script,
                }),
                true,
            );
            for (event, mode) in definition(&f, source).document.instances.iter().zip([
                LoopMode::Playing,
                LoopMode::Stopped,
                LoopMode::Recording,
            ]) {
                run(
                    &mut f,
                    &mut client,
                    Command::Composite(Edit::Mode {
                        target: source,
                        event: event.instance_id,
                        mode,
                    }),
                    true,
                );
            }
        }
        if kind == 3 {
            let Outcome::LoopCreated { loop_id, .. } =
                run(&mut f, &mut client, Command::AddLoop { track }, true)
            else {
                panic!()
            };
            run(&mut f, &mut client, append(loop_id, vec![source]), true);
            source = loop_id;
        }
        let pinned = definition(&f, source);
        for _ in 0..2 {
            let outcome = run(
                &mut f,
                &mut client,
                action(track, source, LoopAction::DuplicateTo(loops[3])),
                true,
            );
            assert!(matches!(outcome, Outcome::Confirmed { .. }), "{outcome:?}");
            copied_definition(&pinned, &definition(&f, loops[3]));
            assert_eq!(
                (view(&f, loops[3]).mode, view(&f, loops[3]).position),
                (LoopMode::Stopped, 0.)
            );
        }
        let copied = definition(&f, loops[3]);
        if let Some(event) = pinned.document.instances.first() {
            run(
                &mut f,
                &mut client,
                Command::Composite(Edit::Delete {
                    target: source,
                    events: vec![event.instance_id],
                }),
                true,
            );
            assert_eq!(definition(&f, loops[3]), copied);
        }
        assert_eq!(f.content_applies.get(), 0);
    }
}

#[shoop_test]
fn p03_replacement_stops_children_preserves_old_definitions_on_failure_and_survives_detach() {
    let (mut f, mut client, track, loops) = setup(true);
    run(&mut f, &mut client, append(loops[2], vec![loops[0]]), true);
    run(&mut f, &mut client, append(loops[3], vec![loops[1]]), true);
    let event = definition(&f, loops[3]).document.instances[0].instance_id;
    run(
        &mut f,
        &mut client,
        Command::Composite(Edit::Duration {
            target: loops[3],
            event,
            n_cycles: Some(10_000),
        }),
        true,
    );
    let old = definition(&f, loops[3]);
    run(
        &mut f,
        &mut client,
        action(track, loops[3], LoopAction::PlayClicked),
        true,
    );
    assert_eq!(view(&f, loops[1]).mode, LoopMode::Playing);
    f.refuse_composite_after.set(Some(0));
    let outcome = run(
        &mut f,
        &mut client,
        action(track, loops[2], LoopAction::DuplicateTo(loops[3])),
        true,
    );
    assert!(matches!(outcome, Outcome::Refused { .. }), "{outcome:?}");
    assert_eq!(definition(&f, loops[3]), old);
    assert_eq!(view(&f, loops[1]).mode, LoopMode::Stopped);
    assert_eq!(view(&f, loops[3]).mode, LoopMode::Stopped);
    let pinned = definition(&f, loops[2]);
    f.refuse_copy.set(Some(CopyFailure::Repeat));
    let Outcome::Refused { message, .. } = run(
        &mut f,
        &mut client,
        action(track, loops[2], LoopAction::DuplicateTo(loops[3])),
        true,
    ) else {
        panic!()
    };
    assert!(message.contains("Content copied"), "{message}");
    copied_definition(&pinned, &definition(&f, loops[3]));
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        action(track, loops[2], LoopAction::DuplicateTo(loops[3])),
    );
    f.service.detach(ConnectionId(1));
    for _ in 0..100 {
        f.service.tick(STEP);
    }
    copied_definition(&pinned, &definition(&f, loops[3]));
    assert!(f
        .service
        .snapshot()
        .results
        .iter()
        .any(|r| r.id == operation && matches!(r.outcome, Outcome::Confirmed { .. })));
}

#[shoop_test]
fn composite_copy_refuses_cross_kind_cycles_and_preserves_shadowed_p17_events() {
    let (mut f, mut client, track, loops) = setup(false);
    run(&mut f, &mut client, append(loops[2], vec![loops[0]]), false);
    let old = definition(&f, loops[2]);
    let media = audio(&mut f, &client, loops[1]);
    for (source, target) in [(loops[0], loops[2]), (loops[2], loops[1])] {
        assert!(matches!(
            run(
                &mut f,
                &mut client,
                action(track, source, LoopAction::DuplicateTo(target)),
                false
            ),
            Outcome::Refused { .. }
        ));
    }
    assert_eq!(definition(&f, loops[2]), old);
    assert_eq!(audio(&mut f, &client, loops[1]), media);
    f.advance.set(false);
    let edit = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        append(loops[2], vec![loops[1]]),
    );
    assert!(matches!(
        run(
            &mut f,
            &mut client,
            action(track, loops[2], LoopAction::Duplicate),
            false
        ),
        Outcome::Refused {
            reason: Refusal::Busy,
            ..
        }
    ));
    assert_eq!(definition(&f, loops[2]), old);
    f.advance.set(true);
    assert!(matches!(
        complete(&mut f.service, &mut client, ConnectionId(1), edit, false),
        Outcome::Confirmed { .. }
    ));
    run(&mut f, &mut client, append(loops[2], vec![loops[3]]), false);
    assert!(matches!(
        run(
            &mut f,
            &mut client,
            action(track, loops[2], LoopAction::DuplicateTo(loops[3])),
            false
        ),
        Outcome::Refused { .. }
    ));
    assert!(view(&f, loops[3]).empty);
    let target = loops[2];
    let event = definition(&f, target).document.instances[0].instance_id;
    for edit in [
        Edit::Kind {
            target,
            kind: CompositeKind::Script,
        },
        Edit::Mode {
            target,
            event,
            mode: LoopMode::Replacing,
        },
        Edit::Relocate {
            target,
            events: vec![event],
            start_cycle: 0,
            duplicate: true,
        },
    ] {
        assert!(matches!(
            run(&mut f, &mut client, Command::Composite(edit), false),
            Outcome::Confirmed { .. }
        ));
    }
    let duplicate = definition(&f, target)
        .document
        .instances
        .iter()
        .filter(|i| i.loop_id == loops[0])
        .map(|i| i.instance_id)
        .max()
        .unwrap();
    run(
        &mut f,
        &mut client,
        Command::Composite(Edit::Mode {
            target,
            event: duplicate,
            mode: LoopMode::Playing,
        }),
        false,
    );
    assert!(definition(&f, target)
        .prepared
        .spans
        .iter()
        .all(|span| span.mode != LoopMode::Replacing));
    let pinned = definition(&f, target);
    let outcome = run(
        &mut f,
        &mut client,
        action(track, target, LoopAction::DuplicateTo(loops[4])),
        false,
    );
    assert!(matches!(outcome, Outcome::Confirmed { .. }), "{outcome:?}");
    let copy = definition(&f, loops[4]);
    copied_definition(&pinned, &copy);
}

#[shoop_test]
fn future_composite_recording_blocks_primitive_copy_without_stopping_the_parent() {
    let (mut f, mut client, track, loops) = setup(false);
    let parent = loops[2];
    run(&mut f, &mut client, append(parent, vec![loops[0]]), false);
    run(
        &mut f,
        &mut client,
        Command::Composite(Edit::Kind {
            target: parent,
            kind: CompositeKind::Script,
        }),
        false,
    );
    run(
        &mut f,
        &mut client,
        Command::Composite(Edit::Insert {
            target: parent,
            source: loops[0],
            start_cycle: 10_000,
        }),
        false,
    );
    let event = definition(&f, parent)
        .document
        .instances
        .last()
        .unwrap()
        .instance_id;
    run(
        &mut f,
        &mut client,
        Command::Composite(Edit::Mode {
            target: parent,
            event,
            mode: LoopMode::Recording,
        }),
        false,
    );
    run(
        &mut f,
        &mut client,
        action(track, parent, LoopAction::PlayClicked),
        false,
    );
    assert_eq!(view(&f, parent).mode, LoopMode::Playing);
    let before = audio(&mut f, &client, loops[1]);
    assert!(matches!(
        run(
            &mut f,
            &mut client,
            action(track, loops[0], LoopAction::DuplicateTo(loops[1])),
            false
        ),
        Outcome::Refused {
            reason: Refusal::Busy,
            ..
        }
    ));
    assert_eq!(view(&f, parent).mode, LoopMode::Playing);
    assert_eq!(audio(&mut f, &client, loops[1]), before);
}

#[shoop_test]
fn runtime_clone_is_available_without_file_io_and_lua_can_edit_the_confirmed_copy() {
    let (mut f, _client, track, loops) = setup(false);
    f.service.detach(ConnectionId(1));
    let (mut runtime, port) = runtime(
        &mut f.service,
        shoop_app_client::runtime::Startup::Fresh(vec![]),
    );
    assert!(runtime
        .snapshot()
        .capabilities
        .supports(BackendCapability::LoopDuplication));
    assert!(!runtime
        .snapshot()
        .capabilities
        .supports(BackendCapability::SessionCapture));
    runtime
        .dispatch(AppIntent::Loop {
            track_id: track,
            loop_id: loops[0],
            action: LoopAction::DuplicateTo(loops[1]),
        })
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 100);
    assert_eq!(
        view(&f, loops[1]).length_frames,
        view(&f, loops[0]).length_frames
    );
    assert_eq!(f.content_applies.get(), 1);
    runtime.dispatch(AppIntent::AddScriptSource {
        name: "copy.lua".into(), kind: ScriptKind::User, enabled: true,
        source: Arc::from("shoop_announce_api_version(1,0); local c = require('shoop_control'); if c.loop_get_gain({0,1})[1] ~= 1 then error('copy not confirmed') end; c.loop_set_gain({0,1}, 0.5)"),
    }).unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 20);
    assert!(runtime.snapshot().scripting.scripts[0]
        .latest_error
        .is_none());
    assert_eq!(view(&f, loops[1]).gain, 0.5);
    assert_eq!(view(&f, loops[0]).gain, 1.);
}
