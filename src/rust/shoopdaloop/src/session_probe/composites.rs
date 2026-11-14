//! Public editor and Lua acceptance through a packaged, independently running host.
use super::*;
use crate::application::{ApplicationHandle, ApplicationRuntime, ApplicationStartOptions};
use anyhow::Context;
use shoop_app_protocol::{composite, OperationId};
use shoop_egui::*;

pub(super) fn wait_for_controller_release(entry: &HostEntry) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let reply = connection::management(
            entry,
            shoop_app_protocol::ManagementRequest::Status {
                version: shoop_app_protocol::VERSION,
            },
            Duration::from_secs(2),
        )?;
        match reply {
            shoop_app_protocol::ManagementReply::Status {
                ready: true,
                busy: false,
                closed: false,
                ..
            } => return Ok(()),
            shoop_app_protocol::ManagementReply::Status { closed: false, .. } => {}
            reply => return Err(anyhow!("composite recovery management status: {reply:?}")),
        }
        ensure!(
            Instant::now() < deadline,
            "previous composite controller did not detach"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn reattach(entry: &HostEntry) -> Result<(Client, Connection)> {
    wait_for_controller_release(entry)?;
    attach(entry)
}

fn wait_for_state(
    client: &mut Client,
    connection: &mut Connection,
    stage: &str,
    ready: impl Fn(&Client) -> bool,
) -> Result<()> {
    wait(client, connection, ready).map_err(|error| {
        let snapshot = client.snapshot().unwrap();
        let loops = snapshot
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .map(|l| (l.id, l.mode, l.length_frames))
            .collect::<Vec<_>>();
        let composites = snapshot
            .composites
            .iter()
            .map(|v| {
                (
                    v.loop_id,
                    v.mode,
                    v.waiting_repeat,
                    v.passes,
                    v.active.as_ref().map(|p| p.version),
                    v.pending.as_ref().map(|p| p.version),
                    v.pending_stage,
                )
            })
            .collect::<Vec<_>>();
        anyhow!("composite {stage}: {error:#}; loops={loops:?}, composites={composites:?}")
    })
}

fn transition(id: LoopId, mode: LoopMode, delay: Option<u32>) -> Command {
    Command::Control(ControlOperation::Transition {
        loops: vec![id],
        mode,
        cycles_delay: delay,
        align_to_sync_at: None,
    })
}

fn submit(
    client: &mut Client,
    connection: &mut Connection,
    command: Command,
) -> Result<OperationId> {
    let message = client
        .command(command, None)
        .map_err(|e| anyhow!("{e:?}"))?;
    let ClientMessage::Submit { operation, .. } = &message else {
        unreachable!()
    };
    let operation = *operation;
    connection.send(message).map_err(|e| anyhow!("{e:?}"))?;
    Ok(operation)
}

fn socket_controls(entry: &HostEntry, parent: LoopId) -> Result<composite::Versioned> {
    let (mut client, mut connection) = reattach(entry)?;
    let source = client.snapshot().unwrap().tracks[0].loops[0].id;
    let original = client
        .snapshot()
        .unwrap()
        .composites
        .iter()
        .find(|v| v.loop_id == parent)
        .unwrap()
        .active
        .clone()
        .unwrap();
    let parent_length = client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .flat_map(|track| &track.loops)
        .find(|loop_| loop_.id == parent)
        .unwrap()
        .length_frames;
    ensure!(parent_length > 0, "probe arrangement has no period");
    command(
        &mut client,
        &mut connection,
        transition(source, LoopMode::Recording, None),
    )?;
    wait_for_state(&mut client, &mut connection, "longer repeat source", |c| {
        c.snapshot().unwrap().tracks[0].loops[0].length_frames > 2 * parent_length
    })?;
    command(
        &mut client,
        &mut connection,
        transition(source, LoopMode::Stopped, None),
    )?;
    wait_for_state(&mut client, &mut connection, "source recorded", |c| {
        let source = &c.snapshot().unwrap().tracks[0].loops[0];
        source.mode == LoopMode::Stopped && source.length_frames > 2 * parent_length
    })?;
    let source_length = client.snapshot().unwrap().tracks[0].loops[0].length_frames;
    command(
        &mut client,
        &mut connection,
        Command::Global(GlobalControlAction::SetSync(true)),
    )?;
    command(
        &mut client,
        &mut connection,
        transition(source, LoopMode::Playing, None),
    )?;
    command(
        &mut client,
        &mut connection,
        transition(parent, LoopMode::Playing, None),
    )?;
    wait_for_state(&mut client, &mut connection, "initial repeat wait", |c| {
        c.snapshot()
            .unwrap()
            .composites
            .iter()
            .any(|v| v.loop_id == parent && v.waiting_repeat)
    })?;
    let passes = client
        .snapshot()
        .unwrap()
        .composites
        .iter()
        .find(|v| v.loop_id == parent)
        .unwrap()
        .passes;
    drop(connection);
    std::thread::sleep(Duration::from_millis(850));
    (client, connection) = reattach(entry)?;
    let view = client
        .snapshot()
        .unwrap()
        .composites
        .iter()
        .find(|v| v.loop_id == parent)
        .unwrap();
    ensure!(
        view.mode == LoopMode::Playing
            && view.passes >= passes
            && view.active.as_ref() == Some(&original),
        "recovery changed the playing arrangement: mode={:?}, passes={}/{passes}, waiting={}, active_matches={}", view.mode, view.passes, view.waiting_repeat, view.active.as_ref() == Some(&original)
    );
    wait_for_state(&mut client, &mut connection, "recovered repeat wait", |c| {
        c.snapshot()
            .unwrap()
            .composites
            .iter()
            .any(|v| v.loop_id == parent && v.waiting_repeat)
    })?;
    command(
        &mut client,
        &mut connection,
        transition(source, LoopMode::Stopped, None),
    )?;
    wait_for_state(&mut client, &mut connection, "source-stop release", |c| {
        c.snapshot()
            .unwrap()
            .composites
            .iter()
            .any(|v| v.loop_id == parent && !v.waiting_repeat && v.mode == LoopMode::Playing)
    })?;
    command(
        &mut client,
        &mut connection,
        transition(source, LoopMode::Recording, None),
    )?;
    wait_for_state(&mut client, &mut connection, "source length growth", |c| {
        c.snapshot().unwrap().tracks[0].loops[0].length_frames > source_length + 128
    })?;
    command(
        &mut client,
        &mut connection,
        transition(source, LoopMode::Stopped, None),
    )?;
    let view = client
        .snapshot()
        .unwrap()
        .composites
        .iter()
        .find(|v| v.loop_id == parent)
        .unwrap();
    ensure!(
        view.mode == LoopMode::Playing && view.active.as_ref() == Some(&original),
        "resizing the source retimed or stopped the running arrangement"
    );
    command(
        &mut client,
        &mut connection,
        transition(source, LoopMode::Playing, None),
    )?;
    command(
        &mut client,
        &mut connection,
        transition(source, LoopMode::Stopped, None),
    )?;
    command(
        &mut client,
        &mut connection,
        transition(parent, LoopMode::Stopped, None),
    )?;
    let delayed = submit(
        &mut client,
        &mut connection,
        transition(parent, LoopMode::Playing, Some(0)),
    )?;
    wait_for_state(
        &mut client,
        &mut connection,
        "delayed launch admission",
        |c| {
            c.snapshot().unwrap().composites.iter().any(|v| {
                v.loop_id == parent
                    && v.mode == LoopMode::Stopped
                    && v.next_mode == Some(LoopMode::Playing)
            })
        },
    )?;
    drop(connection);
    std::thread::sleep(Duration::from_millis(50));
    (client, connection) = reattach(entry)?;
    ensure!(
        client
            .snapshot()
            .unwrap()
            .composites
            .iter()
            .any(|v| v.loop_id == parent
                && v.mode == LoopMode::Stopped
                && v.next_mode == Some(LoopMode::Playing)),
        "recovery lost a delayed launch awaiting a stopped source"
    );
    command(
        &mut client,
        &mut connection,
        transition(source, LoopMode::Playing, None),
    )?;
    wait_for_state(
        &mut client,
        &mut connection,
        "delayed launch delivery",
        |c| {
            c.snapshot()
                .unwrap()
                .composites
                .iter()
                .any(|v| v.loop_id == parent && v.mode == LoopMode::Playing)
        },
    )?;
    ensure!(
        client
            .snapshot()
            .unwrap()
            .results
            .iter()
            .any(|r| r.id == delayed && matches!(r.outcome, Outcome::Confirmed { .. })),
        "recovery lost the original delayed operation result"
    );
    let added_child = client.snapshot().unwrap().tracks[1].loops[6].id;
    let edit = submit(
        &mut client,
        &mut connection,
        Command::Composite(composite::Edit::Insert {
            target: parent,
            source: added_child,
            start_cycle: 0,
        }),
    )?;
    wait_for_state(
        &mut client,
        &mut connection,
        "deferred edit admission",
        |c| {
            c.snapshot().unwrap().composites.iter().any(|v| {
                v.loop_id == parent
                    && v.pending_stage == Some(composite::PendingStage::AwaitingRestart)
            })
        },
    )?;
    let pending = client
        .snapshot()
        .unwrap()
        .composites
        .iter()
        .find(|v| v.loop_id == parent)
        .unwrap()
        .pending
        .clone()
        .unwrap();
    drop(connection);
    std::thread::sleep(Duration::from_millis(50));
    (client, connection) = reattach(entry)?;
    command(
        &mut client,
        &mut connection,
        transition(parent, LoopMode::Stopped, None),
    )?;
    wait_for_state(
        &mut client,
        &mut connection,
        "deferred edit activation",
        |c| {
            c.snapshot().unwrap().composites.iter().any(|v| {
                v.loop_id == parent && v.active.as_ref() == Some(&pending) && v.pending.is_none()
            })
        },
    )?;
    ensure!(
        client
            .snapshot()
            .unwrap()
            .results
            .iter()
            .any(|r| r.id == edit && matches!(r.outcome, Outcome::Confirmed { .. })),
        "recovery lost an owned edit outcome"
    );
    println!("Composite socket recovery: repeat wait/source release, delayed launch and staged edit retained their identities/results without replay");
    Ok(pending)
}

fn loop_action(
    handle: &ApplicationHandle,
    track: TrackId,
    id: LoopId,
    action: LoopAction,
) -> Result<()> {
    handle.dispatch(AppIntent::Loop {
        track_id: track,
        loop_id: id,
        action,
    })?;
    Ok(())
}

fn details(
    handle: &ApplicationHandle,
    test: impl Fn(&CompositeDetailsState) -> bool,
) -> Result<CompositeDetailsState> {
    let state = file_wait(handle, |s| {
        s.details
            .as_ref()
            .and_then(|d| d.composite.as_ref())
            .is_some_and(&test)
    })?;
    Ok(state.details.as_ref().unwrap().composite.clone().unwrap())
}

pub fn run(transport: EngineTransportKind) -> Result<()> {
    let _span = tracing::info_span!(
        "frontend.composites.companion_probe",
        transport = transport.id()
    )
    .entered();
    let (root, mut host, entry) = launch(transport, 16)?;
    let options = ApplicationStartOptions {
        master_auto_connect: false,
        loop_smoothing_ms: 0,
        ..Default::default()
    };
    let mut app = ApplicationRuntime::attach(entry.clone(), options.clone())?;
    let mut handle = app.handle();
    file_wait(&handle, |s| !s.tracks.is_empty())?;
    handle.dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))?;
    handle.dispatch(AppIntent::AddTrack(DirectTrackSpec {
        name: "Composites canary".into(),
        audio_channels: 1,
        midi: true,
    }))?;
    let state = file_wait(&handle, |s| {
        s.tracks.len() == 2 && s.tracks[1].loops.len() == 8
    })?;
    ensure!(
        state
            .capabilities
            .supports(BackendCapability::CompositeLoops),
        "composite capability missing"
    );
    let track = state.tracks[1].id;
    let ids = state.tracks[1]
        .loops
        .iter()
        .map(|l| l.id)
        .collect::<Vec<_>>();
    handle.dispatch(AppIntent::Track {
        track_id: track,
        action: TrackAction::InputMonitoringChanged {
            enabled: true,
            respect_auto_mute: false,
        },
    })?;
    file_wait(&handle, |s| s.tracks[1].controls.input_monitoring)?;
    for id in &ids[..2] {
        loop_action(&handle, track, *id, LoopAction::RecordClicked)?;
        file_wait(&handle, |s| {
            s.tracks[1]
                .loops
                .iter()
                .any(|l| l.id == *id && l.mode == LoopMode::Recording)
        })?;
        handle.dispatch(AppIntent::Piano(PianoAction::Press(
            MidiNote::new(60).unwrap(),
        )))?;
        std::thread::sleep(Duration::from_millis(60));
        handle.dispatch(AppIntent::Piano(PianoAction::Release(
            MidiNote::new(60).unwrap(),
        )))?;
        std::thread::sleep(Duration::from_millis(40));
        loop_action(&handle, track, *id, LoopAction::StopClicked)?;
        file_wait(&handle, |s| {
            s.tracks[1]
                .loops
                .iter()
                .any(|l| l.id == *id && l.mode == LoopMode::Stopped && l.length_frames > 0)
        })?;
    }
    handle.dispatch(AppIntent::Track {
        track_id: track,
        action: TrackAction::InputMonitoringChanged {
            enabled: false,
            respect_auto_mute: false,
        },
    })?;
    file_wait(&handle, |s| !s.tracks[1].controls.input_monitoring)?;
    handle.dispatch(AppIntent::AddScriptSource {
        name: "composite-canary.lua".into(), kind: ScriptKind::User, enabled: true,
        source: std::sync::Arc::from("shoop_announce_api_version(1,0); local c=require('shoop_control'); c.loop_compose_add_to_end({0,2}, {{0,0},{0,1}}, false); c.loop_compose_add_to_end({0,3}, {{0,2}}, false); c.loop_trigger({0,3}, c.constants.LoopMode_Playing)"),
    })?;
    let state = file_wait(&handle, |s| s.tracks[1].loops[3].mode == LoopMode::Playing)?;
    ensure!(
        state.tracks[1].loops[2].length_frames == state.tracks[1].loops[3].length_frames,
        "nested Lua duration differs"
    );
    let before_frame = state.status.processed_frames;
    let before_pass = state.tracks[1].loops[3].composite_cycle_count;
    loop_action(&handle, track, ids[4], LoopAction::ConvertToComposite)?;
    file_wait(&handle, |s| {
        s.tracks[1].loops[4].composite_kind == CompositeKind::Regular
    })?;
    loop_action(
        &handle,
        track,
        ids[4],
        LoopAction::IconClicked(SelectionModifiers::default()),
    )?;
    details(&handle, |d| d.kind == CompositeKind::Regular)?;
    handle.dispatch(AppIntent::SetCompositeTimingBasis {
        target_loop_id: ids[4],
        sample_rate: state.status.sample_rate,
        unit_frames: 32,
    })?;
    details(&handle, |d| d.timing.is_some_and(|t| t.basis_frames == 32))?;
    handle.dispatch(AppIntent::ComposeLoopAt {
        target_loop_id: ids[4],
        source_loop_id: ids[5],
        start_iteration: 1500,
    })?;
    let event = details(&handle, |d| d.events.len() == 1)?.events[0].instance_id;
    handle.dispatch(AppIntent::SetCompositeLoopCycles {
        target_loop_id: ids[4],
        event: CompositeEventId { instance_id: event },
        n_cycles: Some(128),
    })?;
    details(&handle, |d| d.events[0].forced_n_cycles == Some(128))?;
    handle.dispatch(AppIntent::SetCompositeKind {
        target_loop_id: ids[4],
        kind: CompositeKind::Script,
    })?;
    details(&handle, |d| d.kind == CompositeKind::Script)?;
    handle.dispatch(AppIntent::SetCompositeEventMode {
        target_loop_id: ids[4],
        event: CompositeEventId { instance_id: event },
        mode: LoopMode::Recording,
    })?;
    details(&handle, |d| {
        d.events[0].mode.as_deref() == Some("recording") && d.execution.pending_version.is_none()
    })?;
    loop_action(&handle, track, ids[4], LoopAction::PlayClicked)?;
    file_wait(&handle, |s| s.tracks[1].loops[4].mode == LoopMode::Playing)?;
    if transport == EngineTransportKind::Socket {
        drop(handle);
        drop(app);
        std::thread::sleep(Duration::from_millis(1500));
        wait_for_controller_release(&entry)?;
        app = ApplicationRuntime::attach(entry.clone(), options.clone())?;
        handle = app.handle();
    } else {
        std::thread::sleep(Duration::from_millis(1500));
    }
    let state = file_wait(&handle, |s| {
        s.tracks.len() == 2
            && s.tracks[1].loops[4].mode == LoopMode::Stopped
            && s.tracks[1].loops[5].mode == LoopMode::Stopped
            && s.tracks[1].loops[5].length_frames == 4096
            // Loop and track telemetry can arrive in separate publications.
            && !s.tracks[1].controls.input_monitoring
    })?;
    ensure!(
        state.status.processed_frames > before_frame
            && state.tracks[1].loops[3].mode == LoopMode::Playing
            && state.tracks[1].loops[3].composite_cycle_count > before_pass,
        "independent playback did not continue"
    );
    if transport == EngineTransportKind::Socket {
        ensure!(
            state.scripting.scripts.is_empty(),
            "recovery restarted a client-local VM"
        );
    }
    ensure!(
        !state.tracks[1].controls.input_monitoring,
        "recording left manual monitoring enabled"
    );
    handle.dispatch(AppIntent::Global(GlobalControlAction::StopAll))?;
    file_wait(&handle, |s| {
        s.tracks
            .iter()
            .flat_map(|t| &t.loops)
            .all(|l| l.mode == LoopMode::Stopped)
    })?;
    for (source, target) in [(0, 6), (1, 6), (3, 6), (1, 6), (0, 7), (3, 7)] {
        loop_action(
            &handle,
            track,
            ids[source],
            LoopAction::DuplicateTo(ids[target]),
        )?;
        file_wait(&handle, |s| {
            let loops = &s.tracks[1].loops;
            loops[target].name == loops[source].name
                && loops[target].length_frames == loops[source].length_frames
                && loops[target].mode == LoopMode::Stopped
                && loops[target].position == 0.
        })?;
    }
    let before = file_save(&handle, &root.path().join("composites.shoop"))?;
    crate::copy_probe::verify(&before).map_err(anyhow::Error::msg)?;
    ensure!(before.media.values().any(|media| matches!(media, shoop_session::MediaPayload::Midi(midi) if midi.events.len() >= 2)), "public piano recording did not retain exact MIDI");
    ensure!(
        before
            .document
            .track_groups
            .iter()
            .flat_map(|g| &g.tracks)
            .nth(1)
            .unwrap()
            .loops
            .iter()
            .filter(|l| l.composite.is_some())
            .count()
            == 4,
        "capture lost composite documents"
    );
    ensure!(
        before
            .document
            .track_groups
            .iter()
            .flat_map(|g| &g.tracks)
            .nth(1)
            .unwrap()
            .loops[5]
            .length_frames
            == 4096,
        "recording section did not end at its authored frame"
    );
    let bytes = std::fs::read(root.path().join("composites.shoop"))?;
    handle.dispatch(AppIntent::LoadSessionBytes {
        name: "composites.shoop".into(),
        bytes: std::sync::Arc::new(bytes),
    })?;
    let loaded = file_done(&handle, IoTaskKind::LoadSession)?;
    ensure!(
        loaded.tracks[1].id != track
            && loaded.tracks[1]
                .loops
                .iter()
                .all(|l| l.mode == LoopMode::Stopped),
        "load did not publish fresh stopped owners"
    );
    let after = file_save(&handle, &root.path().join("resaved.shoop"))?;
    crate::copy_probe::verify(&after).map_err(anyhow::Error::msg)?;
    ensure!(
        before.media.values().collect::<Vec<_>>() == after.media.values().collect::<Vec<_>>(),
        "composite save/load changed PCM or exact MIDI"
    );
    let remap = before
        .document
        .track_groups
        .iter()
        .flat_map(|g| &g.tracks)
        .flat_map(|t| &t.loops)
        .zip(
            after
                .document
                .track_groups
                .iter()
                .flat_map(|g| &g.tracks)
                .flat_map(|t| &t.loops),
        )
        .map(|(old, new)| (old.id, new.id))
        .collect::<std::collections::BTreeMap<_, _>>();
    for (old, new) in before
        .document
        .track_groups
        .iter()
        .flat_map(|g| &g.tracks)
        .flat_map(|t| &t.loops)
        .zip(
            after
                .document
                .track_groups
                .iter()
                .flat_map(|g| &g.tracks)
                .flat_map(|t| &t.loops),
        )
    {
        let mut expected = old.composite.clone();
        if let Some(composite) = &mut expected {
            for event in &mut composite.instances {
                event.loop_id = remap[&event.loop_id];
            }
            for span in &mut composite.timing.as_mut().unwrap().spans {
                span.target = remap[&span.target];
            }
        }
        ensure!(
            expected == new.composite,
            "load changed prepared geometry or event identity"
        );
    }
    let restored_track = &loaded.tracks[1];
    let replacing = restored_track.loops[4].id;
    if !handle.snapshot().tracks[1].loops[4].selected {
        loop_action(
            &handle,
            restored_track.id,
            replacing,
            LoopAction::IconClicked(SelectionModifiers::default()),
        )?;
    }
    let mut confirmed =
        details(&handle, |d| d.events.len() == 1).context("select loaded Replacing program")?;
    let event = confirmed.events[0].instance_id;
    for (index, intent) in [
        AppIntent::RelocateCompositeEvents {
            target_loop_id: replacing,
            events: vec![CompositeEventId { instance_id: event }],
            start_iteration: 0,
            duplicate: false,
        },
        AppIntent::SetCompositeLoopCycles {
            target_loop_id: replacing,
            event: CompositeEventId { instance_id: event },
            n_cycles: Some(10_000),
        },
        AppIntent::SetCompositeEventMode {
            target_loop_id: replacing,
            event: CompositeEventId { instance_id: event },
            mode: LoopMode::Replacing,
        },
    ]
    .into_iter()
    .enumerate()
    {
        handle.dispatch(intent)?;
        confirmed = details(&handle, |d| {
            d.execution.active_version > confirmed.execution.active_version
                && d.execution.pending_version.is_none()
                && !d.execution.restoring
                && match index {
                    0 => d.events[0].start_frame == 0,
                    1 => d.events[0].forced_n_cycles == Some(10_000),
                    _ => d.events[0].mode.as_deref() == Some("replacing"),
                }
        })
        .with_context(|| {
            format!(
                "confirm loaded Replacing edit {index}: {:?}",
                handle.snapshot().details
            )
        })?;
    }
    let copied_replacing = restored_track.loops[6].id;
    loop_action(
        &handle,
        restored_track.id,
        replacing,
        LoopAction::DuplicateTo(copied_replacing),
    )?;
    file_wait(&handle, |s| {
        s.tracks[1].loops[6].composite_kind == CompositeKind::Script
            && s.tracks[1].loops[6].length_frames == s.tracks[1].loops[4].length_frames
            && s.tracks[1].loops[6].name == s.tracks[1].loops[4].name
    })?;
    let p17 = file_save(&handle, &root.path().join("p17-copy.shoop"))
        .context("save P17 copied definition")?;
    crate::copy_probe::verify_definition(&p17, 4, 6).map_err(anyhow::Error::msg)?;
    loop_action(
        &handle,
        restored_track.id,
        copied_replacing,
        LoopAction::PlayClicked,
    )?;
    file_wait(&handle, |s| {
        s.tracks[1].loops[5].mode == LoopMode::Replacing
    })
    .with_context(|| {
        format!(
            "start loaded Replacing: {:?}",
            handle.snapshot().tracks[1].loops
        )
    })?;
    loop_action(
        &handle,
        restored_track.id,
        copied_replacing,
        LoopAction::StopClicked,
    )?;
    let stopped = file_wait(&handle, |s| s.tracks[1].loops[5].mode == LoopMode::Stopped)?;
    ensure!(
        stopped.tracks[1].loops[5].length_frames == 4096,
        "authored Replacing changed loaded loop length"
    );
    let ticket = if transport == EngineTransportKind::Socket {
        drop(handle);
        drop(app);
        std::thread::sleep(Duration::from_millis(50));
        let installed = socket_controls(&entry, loaded.tracks[1].loops[3].id)?;
        wait_for_controller_release(&entry)?;
        app = ApplicationRuntime::attach(entry.clone(), options)?;
        handle = app.handle();
        file_wait(&handle, |s| s.tracks.len() == 2)?;
        let resized = file_save(&handle, &root.path().join("resized-source.shoop"))?;
        let saved = resized
            .document
            .track_groups
            .iter()
            .flat_map(|g| &g.tracks)
            .nth(1)
            .unwrap()
            .loops[3]
            .composite
            .as_ref()
            .unwrap();
        let timing = saved.timing.as_ref().unwrap();
        ensure!(
            timing.basis_sample_rate == installed.document.basis.sample_rate
                && timing.basis_unit_frames == installed.document.basis.unit_frames
                && timing.unit_frames == installed.prepared.unit_frames
                && timing.iterations == installed.prepared.iterations
                && timing.instances[0].instance_id == installed.document.instances[0].instance_id
                && timing.instances[0].end_frame == installed.prepared.instances[0].end_frame,
            "save after source resize changed accepted timing or event identity"
        );
        handle.dispatch(AppIntent::LoadSessionBytes {
            name: "resized-source.shoop".into(),
            bytes: std::sync::Arc::new(std::fs::read(root.path().join("resized-source.shoop"))?),
        })?;
        let restored = file_done(&handle, IoTaskKind::LoadSession)?;
        ensure!(
            restored.tracks[1].id != loaded.tracks[1].id
                && restored
                    .tracks
                    .iter()
                    .flat_map(|t| &t.loops)
                    .all(|l| l.mode == LoopMode::Stopped),
            "second load did not publish fresh stopped owners"
        );
        let resaved = file_save(&handle, &root.path().join("resized-source-resaved.shoop"))?;
        ensure!(
            resized.media.values().collect::<Vec<_>>()
                == resaved.media.values().collect::<Vec<_>>(),
            "source resize/recovery round trip changed PCM or exact MIDI"
        );
        let restored_composite = resaved
            .document
            .track_groups
            .iter()
            .flat_map(|g| &g.tracks)
            .nth(1)
            .unwrap()
            .loops[3]
            .composite
            .as_ref()
            .unwrap();
        let restored_timing = restored_composite.timing.as_ref().unwrap();
        ensure!(
            timing.instances == restored_timing.instances
                && timing.unit_frames == restored_timing.unit_frames
                && timing.iterations == restored_timing.iterations
                && timing.basis_unit_frames == restored_timing.basis_unit_frames,
            "second round trip changed frozen timing"
        );
        let ticket = handle.prepare_shutdown()?;
        drop(app);
        ticket
    } else {
        let ticket = handle.prepare_shutdown()?;
        drop(app);
        ticket
    };
    crate::recovery::shutdown(&entry, ticket)
        .0
        .map_err(anyhow::Error::msg)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = host.0.try_wait()? {
            ensure!(status.success(), "composite companion failed on exit");
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "composite companion did not exit"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    println!("Composite {}: public Lua/editor, nested playback, exact recording during client absence, fresh stopped load and exact resave passed", transport.id());
    Ok(())
}
