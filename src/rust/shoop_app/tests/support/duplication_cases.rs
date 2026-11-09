use super::*;

fn run(f: &mut Fixture, client: &mut Client, command: Command, encoded: bool) -> Outcome {
    let id = submit(&mut f.service, client, ConnectionId(1), command);
    complete(&mut f.service, client, ConnectionId(1), id, encoded)
}
fn action(track: TrackId, loop_id: LoopId, action: LoopAction) -> Command {
    Command::Loop {
        track,
        loop_id,
        action,
    }
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
