//! Packaged companion canary using the public application protocol.
use anyhow::{anyhow, ensure, Result};
use shoop_app_client::{Client, State};
use shoop_app_protocol::{ClientId, ClientMessage, Command, Outcome};
use shoop_egui::{
    AudioDriverConfig, DummyAudioDriverConfig, EngineTransportKind, GlobalControlAction,
    LoopAction, LoopMode, MidiNote, PianoAction, TrackAction, TrackSpec, TrackSpecTopology,
};
use shoop_host::{
    config::AppTransport,
    config::HostConfig,
    connection::{self, Connection},
    remote::Target as HostEntry,
};
use std::{
    process::Child,
    time::{Duration, Instant},
};

struct ProbeHost(Child);
impl Drop for ProbeHost {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn wait(
    client: &mut Client,
    connection: &mut Connection,
    ready: impl Fn(&Client) -> bool,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready(client) {
        connection
            .poll(client)
            .map_err(|error| anyhow!("probe connection: {error:?}"))?;
        if let Some(message) = client.ready_message() {
            connection
                .send(message)
                .map_err(|error| anyhow!("probe ready: {error:?}"))?;
            client
                .ready_sent()
                .map_err(|error| anyhow!("probe readiness: {error:?}"))?;
        }
        ensure!(client.state() != State::Lost, "probe client lost its host");
        ensure!(
            Instant::now() < deadline,
            "session probe timed out: state={:?}, pending={:?}, profiling={:?}",
            client.state(),
            client.pending(),
            client.snapshot().map(|s| &s.profiling)
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}
fn attach(entry: &HostEntry) -> Result<(Client, Connection)> {
    attach_as(entry, ClientId(shoop_host::remote::identity()?))
}
fn attach_as(entry: &HostEntry, _actor: ClientId) -> Result<(Client, Connection)> {
    let mut connection = connection::connect(entry, Duration::from_secs(3))?;
    let mut client = Client::new();
    connection
        .send(client.attach().map_err(|error| anyhow!("{error:?}"))?)
        .map_err(|error| anyhow!("{error:?}"))?;
    wait(&mut client, &mut connection, |client| {
        client.state() == State::Ready
    })?;
    Ok((client, connection))
}
fn command(client: &mut Client, connection: &mut Connection, command: Command) -> Result<Outcome> {
    let message = client
        .command(command, None)
        .map_err(|error| anyhow!("{error:?}"))?;
    let ClientMessage::Submit { operation, .. } = &message else {
        unreachable!()
    };
    let operation = *operation;
    connection
        .send(message)
        .map_err(|error| anyhow!("{error:?}"))?;
    wait(client, connection, |client| {
        client.results().contains_key(&operation)
    })?;
    let result = client.results()[&operation].clone();
    ensure!(
        matches!(
            result,
            Outcome::Confirmed { .. } | Outcome::TrackCreated { .. }
        ),
        "probe operation failed: {result:?}"
    );
    let revision = match &result {
        Outcome::Confirmed { revision, .. } | Outcome::TrackCreated { revision, .. } => *revision,
        _ => unreachable!(),
    };
    wait(client, connection, |client| {
        client
            .snapshot()
            .is_some_and(|state| state.revision >= revision)
    })?;
    Ok(result)
}
fn launch(
    transport: EngineTransportKind,
    memory_mb: u32,
) -> Result<(tempfile::TempDir, ProbeHost, HostEntry)> {
    ensure!(
        transport != EngineTransportKind::InProcess,
        "session companion probe requires socket or shared_memory"
    );
    let root = tempfile::tempdir()?;
    let config = HostConfig {
        settings: shoop_host::config::HostSettings {
            driver: AudioDriverConfig::Dummy(DummyAudioDriverConfig::default()),
            pools: shoop_egui::NewEnginePools {
                quantum: 32,
                buses: 8,
                bus_channels: 8,
                tracks: 3,
                loops: 20,
                channels: 2,
                loop_memory_mb: memory_mb,
            },
            ..Default::default()
        },
        transport: if transport == EngineTransportKind::Socket {
            AppTransport::Socket
        } else {
            AppTransport::SharedMemory
        },
        ..Default::default()
    };
    let companion = shoop_host::launch::companion_path(&std::env::current_exe()?)?;
    let lan = std::env::var("SHOOP_PROBE_LAN").is_ok_and(|value| value == "1");
    ensure!(
        !lan || transport == EngineTransportKind::Socket,
        "LAN probes require socket delivery"
    );
    let mut options = shoop_host::serve::StartOptions::new()?;
    options.lan = lan;
    let (child, entry) = shoop_host::launch::spawn(&config, options, &companion)?;
    let host = ProbeHost(child);
    ensure!(
        std::fs::read_dir(root.path())?.count() == 0,
        "ordinary launch wrote host configuration or records"
    );
    for address in [&entry.endpoint, &entry.management] {
        let description = connection::describe(address, Duration::from_secs(2))?;
        ensure!(
            description.instance == entry.instance
                && description.process == entry.process
                && description.can_attach,
            "inspection changed the host or consumed its attachment"
        );
    }
    shoop_host_manager::verify_host_discovery(&entry, Some((false, true)))?;
    Ok((root, host, entry))
}

pub fn run(transport: EngineTransportKind) -> Result<()> {
    let (root, mut host, entry) = launch(transport, 16)?;
    let (mut client, mut connection) = attach(&entry)?;
    ensure!(
        !client.snapshot().unwrap().profiling.requested,
        "profiling must default off"
    );
    command(&mut client, &mut connection, Command::SetProfiling(true))?;
    wait(&mut client, &mut connection, |client| {
        client.snapshot().unwrap().profiling.active
    })?;
    command(
        &mut client,
        &mut connection,
        Command::Global(GlobalControlAction::SetSync(false)),
    )?;
    let Outcome::TrackCreated { track, loops, .. } = command(
        &mut client,
        &mut connection,
        Command::AddTrack(TrackSpec {
            name: "Packaged session".into(),
            topology: TrackSpecTopology::Direct {
                audio_channels: 2,
                midi: true,
            },
            latency: Default::default(),
            initial_output_bus_names: vec!["Master".into()],
            creation_request_id: None,
        }),
    )?
    else {
        unreachable!()
    };
    command(
        &mut client,
        &mut connection,
        Command::Track {
            track,
            action: TrackAction::InputMonitoringChanged {
                enabled: true,
                respect_auto_mute: false,
            },
        },
    )?;
    command(
        &mut client,
        &mut connection,
        Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::RecordClicked,
        },
    )?;
    wait(&mut client, &mut connection, |client| {
        client.snapshot().unwrap().tracks[1].loops[0].mode == LoopMode::Recording
    })?;
    command(
        &mut client,
        &mut connection,
        Command::Piano(PianoAction::Press(MidiNote::new(60).unwrap())),
    )?;
    command(
        &mut client,
        &mut connection,
        Command::Piano(PianoAction::Release(MidiNote::new(60).unwrap())),
    )?;
    command(
        &mut client,
        &mut connection,
        Command::SetLoopTimeline {
            loop_id: loops[0],
            start_offset: Some(-3),
            preplay_samples: Some(17),
            loop_length: None,
        },
    )?;
    ensure!(
        client.snapshot().unwrap().tracks[1].loops[0]
            .channel_timings
            .iter()
            .all(|channel| channel.start_offset == -3 && channel.preplay_samples == 17),
        "recording timing was not confirmed"
    );
    wait(&mut client, &mut connection, |client| {
        client
            .snapshot()
            .unwrap()
            .profiling
            .live
            .rows
            .iter()
            .any(|row| row.samples > 0)
    })?;
    let profile_generation = client.snapshot().unwrap().profiling.generation;
    ensure!(
        client
            .snapshot()
            .unwrap()
            .profiling
            .live
            .rows
            .iter()
            .any(|row| row.name.contains("playback")),
        "profiling lost node metadata"
    );
    let before = client.snapshot().unwrap().status.processed_frames;
    shoop_host_manager::verify_host_discovery(
        &entry,
        Some((true, transport == EngineTransportKind::Socket)),
    )?;
    if transport == EngineTransportKind::Socket {
        drop(connection);
        std::thread::sleep(Duration::from_millis(50));
        (client, connection) = attach(&entry)?;
        let state = client.snapshot().unwrap();
        ensure!(
            state.tracks[1].id == track
                && state.tracks[1].loops[0].id == loops[0]
                && state.tracks[1].name == "Packaged session",
            "session identity changed on recovery"
        );
        ensure!(
            state.tracks[1].loops[0].mode == LoopMode::Recording
                && state.tracks[1].loops[0]
                    .channel_timings
                    .iter()
                    .all(|channel| channel.start_offset == -3 && channel.preplay_samples == 17)
                && state.status.processed_frames > before,
            "recording did not survive UI loss"
        );
    }
    command(
        &mut client,
        &mut connection,
        Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::PlayClicked,
        },
    )?;
    wait(&mut client, &mut connection, |client| {
        client.snapshot().unwrap().tracks[1].loops[0].mode == LoopMode::Playing
    })?;
    connection
        .send(
            client
                .read_media(loops[0])
                .map_err(|error| anyhow!("{error:?}"))?,
        )
        .map_err(|error| anyhow!("{error:?}"))?;
    wait(&mut client, &mut connection, |client| {
        client.media().is_some()
    })?;
    let media = client
        .media()
        .unwrap()
        .as_ref()
        .map_err(|error| anyhow!("probe media: {error:?}"))?;
    ensure!(
        media.audio.len() == 2
            && !media.audio[0].samples.is_empty()
            && media.midi[0].events.len() == 2,
        "recorded audio/MIDI was not preserved"
    );
    let length = client.snapshot().unwrap().tracks[1].loops[0].length_frames;
    let retained = media.audio[0].samples.len();
    command(
        &mut client,
        &mut connection,
        Command::SetLoopTimeline {
            loop_id: loops[0],
            start_offset: Some(1),
            preplay_samples: Some(31),
            loop_length: Some((length / 64).max(1) * 32),
        },
    )?;
    let state = &client.snapshot().unwrap().tracks[1].loops[0];
    ensure!(
        state.length_frames == (length / 64).max(1) * 32
            && state
                .channel_timings
                .iter()
                .all(|channel| channel.start_offset == 1 && channel.preplay_samples == 31),
        "playing timing did not settle"
    );
    connection
        .send(
            client
                .read_media(loops[0])
                .map_err(|error| anyhow!("{error:?}"))?,
        )
        .map_err(|error| anyhow!("{error:?}"))?;
    wait(&mut client, &mut connection, |client| {
        client.media().is_some()
    })?;
    ensure!(
        client
            .media()
            .unwrap()
            .as_ref()
            .map_err(|error| anyhow!("{error:?}"))?
            .audio[0]
            .samples
            .len()
            == retained,
        "timing truncated raw media"
    );
    ensure!(
        client.snapshot().unwrap().profiling.generation >= profile_generation,
        "profiling generation regressed"
    );
    command(&mut client, &mut connection, Command::SetProfiling(false))?;
    wait(&mut client, &mut connection, |client| {
        !client.snapshot().unwrap().profiling.active
    })?;
    ensure!(
        !client.snapshot().unwrap().profiling.live.rows.is_empty(),
        "disabling lost the final report"
    );
    command(&mut client, &mut connection, Command::SetProfiling(true))?;
    wait(&mut client, &mut connection, |client| {
        let profiling = &client.snapshot().unwrap().profiling;
        profiling.active && profiling.generation > profile_generation
    })?;
    println!(
        "Profiling {}: default off, live names/statistics, recovery and runtime toggles passed",
        transport.id()
    );
    let ticket = client.shutdown().map_err(|error| anyhow!("{error:?}"))?;
    crate::recovery::shutdown(&entry, ticket)
        .0
        .map_err(anyhow::Error::msg)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = host.0.try_wait()? {
            ensure!(status.success(), "session host failed on exit");
            break;
        }
        ensure!(Instant::now() < deadline, "host did not exit after cleanup");
        std::thread::sleep(Duration::from_millis(5));
    }
    shoop_host_manager::verify_host_discovery(&entry, None)?;
    ensure!(
        std::fs::read_dir(root.path())?.count() == 0,
        "normal host workflow wrote persistent metadata"
    );
    Ok(())
}

fn file_wait(
    handle: &crate::application::ApplicationHandle,
    ready: impl Fn(&shoop_egui::AppSnapshot) -> bool,
) -> Result<std::sync::Arc<shoop_egui::AppSnapshot>> {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let state = handle.snapshot();
        if ready(&state) {
            return Ok(state);
        }
        ensure!(
            handle.status().state != State::Lost,
            "file probe lost its host: {:?}",
            handle.status()
        );
        ensure!(
            Instant::now() < deadline,
            "file probe timed out: {:?} {:?}",
            state.io_task,
            state.notifications
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn file_done(
    handle: &crate::application::ApplicationHandle,
    kind: shoop_egui::IoTaskKind,
) -> Result<std::sync::Arc<shoop_egui::AppSnapshot>> {
    let state = file_wait(handle, |s| {
        s.io_task.as_ref().is_some_and(|t| {
            t.kind == kind
                && t.status != shoop_egui::IoTaskStatus::Running
                && t.status != shoop_egui::IoTaskStatus::AwaitingChannelMapping
        })
    })?;
    ensure!(
        state.io_task.as_ref().unwrap().status == shoop_egui::IoTaskStatus::Completed,
        "file probe task failed: {:?}",
        state.io_task
    );
    println!("Persistence file task: {kind:?} confirmed");
    Ok(state)
}
fn file_save(
    handle: &crate::application::ApplicationHandle,
    path: &std::path::Path,
) -> Result<shoop_session::SessionBundle> {
    use shoop_egui::*;
    handle.dispatch(AppIntent::RequestSaveSession)?;
    file_wait(handle, |s| {
        s.io_task
            .as_ref()
            .is_some_and(|t| t.kind == IoTaskKind::SaveSession && t.status == IoTaskStatus::Running)
    })?;
    let deadline = Instant::now() + Duration::from_secs(120);
    let output = loop {
        if let Some(output) = handle.take_file_output() {
            break output;
        }
        ensure!(
            Instant::now() < deadline,
            "file probe output timed out: {:?}",
            handle.snapshot().io_task
        );
        ensure!(
            handle
                .snapshot()
                .io_task
                .as_ref()
                .is_none_or(|t| t.status != IoTaskStatus::Failed),
            "file capture failed: {:?}",
            handle.snapshot().io_task
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    crate::atomic_replace(path, &output.bytes, output.task_id.raw())?;
    handle.dispatch(AppIntent::CompleteIoOutput {
        task_id: output.task_id,
        outcome: FileOutputOutcome::Saved,
    })?;
    file_done(handle, IoTaskKind::SaveSession)?;
    Ok(shoop_session::decode_session(&std::fs::read(path)?)?)
}
pub fn files(transport: EngineTransportKind) -> Result<()> {
    use shoop_egui::*;
    use shoop_session::*;
    let _span = tracing::info_span!(
        "frontend.persistence.companion_probe",
        transport = transport.id()
    )
    .entered();
    let (root, mut host, entry) = launch(transport, 192)?;
    let app = crate::application::ApplicationRuntime::attach(
        entry.clone(),
        crate::application::ApplicationStartOptions {
            master_auto_connect: false,
            loop_smoothing_ms: 0,
            ..Default::default()
        },
    )?;
    let handle = app.handle();
    file_wait(&handle, |s| !s.tracks.is_empty())?;
    handle.dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))?;
    handle.dispatch(AppIntent::AddTrack(DirectTrackSpec {
        name: "Persistence canary".into(),
        audio_channels: 1,
        midi: true,
    }))?;
    let state = file_wait(&handle, |s| {
        s.tracks.len() == 2 && s.tracks[1].loops.len() == 8
    })?;
    let old_track = state.tracks[1].id;
    let old_loop = state.tracks[1].loops[0].id;
    let rate = state.status.sample_rate;
    let frames = 33 * 1024 * 1024 / 4;
    let audio = LoopAudio {
        sample_rate: rate,
        channels: vec![LoopAudioChannel {
            label: "Mono".into(),
            role: "direct".into(),
            samples: (0..frames)
                .map(|i| if i % 2 == 0 { 0.25 } else { -0.5 })
                .collect(),
        }],
    };
    let audio_file = root.path().join("source.wav");
    std::fs::write(&audio_file, encode_float_wav(&audio)?)?;
    let mut input = crate::platform_input::InputSlot::default();
    let lease = input.reserve(TaskId::from_raw(1))?;
    let bytes = crate::read_session_input(&audio_file, &lease)?;
    ensure!(
        bytes.len() > 32 * 1024 * 1024,
        "large input fixture is too small"
    );
    drop(lease);
    handle.dispatch(AppIntent::ImportLoopAudioBytes {
        loop_id: old_loop,
        name: "source.wav".into(),
        bytes: bytes.into(),
        update_loop_length: true,
    })?;
    let mapping = file_wait(&handle, |s| {
        s.io_task.as_ref().is_some_and(|t| {
            t.status == IoTaskStatus::AwaitingChannelMapping || t.status == IoTaskStatus::Failed
        })
    })?;
    ensure!(
        mapping.io_task.as_ref().unwrap().status == IoTaskStatus::AwaitingChannelMapping,
        "large audio decode failed: {:?}",
        mapping.io_task
    );
    handle.dispatch(AppIntent::ConfirmAudioChannelMapping {
        task_id: mapping.io_task.as_ref().unwrap().id,
        source_for_destination: vec![0],
    })?;
    file_done(&handle, IoTaskKind::ImportLoopAudio)?;
    let midi = ExactMidi {
        sample_rate: rate,
        length_frames: frames as u64,
        start_state: vec![vec![0xb0, 7, 99]],
        events: vec![
            ExactMidiEvent {
                frame: 1,
                order: 0,
                data: vec![0x90, 60, 100],
            },
            ExactMidiEvent {
                frame: frames as u64 - 1,
                order: 1,
                data: vec![0x80, 60, 0],
            },
        ],
    };
    handle.dispatch(AppIntent::ImportLoopMidiBytes {
        loop_id: old_loop,
        name: "source.shoop-midi".into(),
        bytes: encode_exact_midi(&midi)?.into(),
        update_loop_length: false,
    })?;
    file_done(&handle, IoTaskKind::ImportLoopMidi)?;
    handle.dispatch(AppIntent::Loop {
        track_id: old_track,
        loop_id: old_loop,
        action: LoopAction::PlayClicked,
    })?;
    file_wait(&handle, |s| s.tracks[1].loops[0].mode == LoopMode::Playing)?;
    let session_file = root.path().join("roundtrip.shoop");
    let before = file_save(&handle, &session_file)?;
    ensure!(
        before
            .media
            .values()
            .any(|m| matches!(m, MediaPayload::Audio(a) if a.samples == audio.channels[0].samples)),
        "saved PCM changed"
    );
    ensure!(
        before
            .media
            .values()
            .any(|m| matches!(m, MediaPayload::Midi(m) if m == &midi)),
        "saved MIDI changed"
    );
    handle.dispatch(AppIntent::LoadSessionBytes {
        name: "corrupt.shoop".into(),
        bytes: vec![0, 1, 2].into(),
    })?;
    file_wait(&handle, |s| {
        s.io_task
            .as_ref()
            .is_some_and(|t| t.kind == IoTaskKind::LoadSession && t.status == IoTaskStatus::Failed)
    })?;
    let state = handle.snapshot();
    ensure!(
        state.tracks[1].loops[0].id == old_loop
            && state.tracks[1].loops[0].mode == LoopMode::Playing,
        "corrupt load changed playback"
    );
    handle.dispatch(AppIntent::RequestLoadSessionSource {
        source: session_file.to_string_lossy().into_owned(),
    })?;
    file_wait(&handle, |s| {
        s.io_task
            .as_ref()
            .is_some_and(|t| t.kind == IoTaskKind::LoadSession && t.status == IoTaskStatus::Running)
    })?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let request = loop {
        ensure!(
            Instant::now() < deadline,
            "file input request was not delivered"
        );
        if let Some(request) = handle.take_file_request() {
            break request;
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    let lease = input.reserve(request.task_id)?;
    let bytes = crate::read_session_input(
        std::path::Path::new(request.source.as_ref().unwrap()),
        &lease,
    )?;
    handle.dispatch(AppIntent::CompleteIoInput {
        task_id: request.task_id,
        name: "roundtrip.shoop".into(),
        bytes: bytes.into(),
    })?;
    drop(lease);
    let loaded = file_done(&handle, IoTaskKind::LoadSession)?;
    ensure!(
        loaded.tracks[1].id != old_track
            && loaded.tracks[1].loops[0].id != old_loop
            && loaded
                .tracks
                .iter()
                .flat_map(|t| &t.loops)
                .all(|l| l.mode == LoopMode::Stopped),
        "load did not publish fresh stopped owners"
    );
    let after = file_save(&handle, &root.path().join("resaved.shoop"))?;
    ensure!(
        before.media.values().collect::<Vec<_>>() == after.media.values().collect::<Vec<_>>(),
        "large media changed across save/load/save"
    );
    let entry_loop = &loaded.tracks[1].loops[0];
    handle.dispatch(AppIntent::Loop {
        track_id: loaded.tracks[1].id,
        loop_id: entry_loop.id,
        action: LoopAction::PlayClicked,
    })?;
    file_wait(&handle, |s| s.tracks[1].loops[0].mode == LoopMode::Playing)?;
    let ticket = if transport == EngineTransportKind::Socket {
        drop(handle);
        drop(app);
        std::thread::sleep(Duration::from_millis(50));
        socket_file_loss(&entry, before)?
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
            ensure!(status.success(), "file companion failed on exit");
            break;
        }
        ensure!(Instant::now() < deadline, "file companion did not exit");
        std::thread::sleep(Duration::from_millis(5));
    }
    ensure!(
        !root.path().join("config.json").exists() && !root.path().join("remote.json").exists(),
        "file companion workflow wrote host profiles/configuration"
    );
    println!("Persistence {}: >32 MiB WAV/exact-MIDI, native output/input, corrupt refusal, fresh load and exact resave passed", transport.id());
    Ok(())
}

fn persistence_reply(
    client: &mut Client,
    connection: &mut Connection,
    key: shoop_app_protocol::persistence::Key,
    ready: impl Fn(&shoop_app_protocol::persistence::Reply) -> bool,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        connection
            .poll(client)
            .map_err(|error| anyhow!("persistence connection: {error:?}"))?;
        while let Some(reply) = client.take_persistence_reply() {
            if reply.key() != key {
                continue;
            }
            ensure!(
                !matches!(
                    reply,
                    shoop_app_protocol::persistence::Reply::Refused { .. }
                ),
                "persistence refused: {reply:?}"
            );
            if ready(&reply) {
                return Ok(());
            }
        }
        ensure!(Instant::now() < deadline, "persistence reply timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn socket_file_loss(
    entry: &HostEntry,
    bundle: shoop_session::SessionBundle,
) -> Result<ClientMessage> {
    use shoop_app_protocol::{persistence::*, persistence_transfer::Source, TransferId};
    use shoop_persistence_codec::codec::{Decoded, Value, Work};
    let Value::Image(image) = Work::Prepare {
        decoded: Decoded::Session(bundle),
        mapping: Vec::new(),
        update_length: true,
    }
    .run()?
    else {
        unreachable!()
    };
    let source =
        Source::new(std::sync::Arc::clone(&image)).map_err(|error| anyhow!("{error:?}"))?;
    let actor = ClientId(shoop_host::remote::identity()?);
    let (mut client, mut connection) = attach_as(entry, actor)?;
    let epoch = client.snapshot().unwrap().persistence_epoch;
    let old_track = client.snapshot().unwrap().tracks[1].id;
    let old_loop = client.snapshot().unwrap().tracks[1].loops[0].id;
    for (transfer, capture) in [(100, false), (101, true)] {
        println!(
            "Socket file loss: abandoning {}",
            if capture { "capture" } else { "upload" }
        );
        let key = Key {
            transfer: TransferId(transfer),
            epoch,
        };
        let target = Target::Session {
            revision: client.snapshot().unwrap().revision,
        };
        let request = if capture {
            Request::Capture { key, target }
        } else {
            Request::Begin(source.description(key, target))
        };
        connection
            .send(
                client
                    .persistence(request)
                    .map_err(|error| anyhow!("{error:?}"))?,
            )
            .map_err(|error| anyhow!("{error:?}"))?;
        persistence_reply(&mut client, &mut connection, key, |reply| {
            if capture {
                matches!(reply, Reply::Captured(_))
            } else {
                matches!(reply, Reply::Begun { .. })
            }
        })?;
        if !capture {
            connection
                .send(
                    client
                        .persistence(Request::Object {
                            key,
                            object: source.object(0).map_err(|e| anyhow!("{e:?}"))?,
                        })
                        .map_err(|e| anyhow!("{e:?}"))?,
                )
                .map_err(|e| anyhow!("{e:?}"))?;
            persistence_reply(&mut client, &mut connection, key, |reply| {
                matches!(reply, Reply::Object { .. })
            })?;
        } else {
            connection
                .send(
                    client
                        .persistence(Request::Read {
                            key,
                            object: 0,
                            offset: 0,
                        })
                        .map_err(|e| anyhow!("{e:?}"))?,
                )
                .map_err(|e| anyhow!("{e:?}"))?;
            persistence_reply(&mut client, &mut connection, key, |reply| {
                matches!(reply, Reply::Chunk { .. })
            })?;
        }
        drop(connection);
        std::thread::sleep(Duration::from_millis(50));
        (client, connection) = attach_as(entry, actor)?;
        ensure!(
            client.snapshot().unwrap().persistence_epoch == epoch
                && client.snapshot().unwrap().tracks[1].id == old_track
                && client.snapshot().unwrap().tracks[1].loops[0].id == old_loop,
            "abandoned transfer mutated session identity"
        );
        command(
            &mut client,
            &mut connection,
            Command::Global(GlobalControlAction::SetSolo(capture)),
        )?;
    }
    let key = Key {
        transfer: TransferId(102),
        epoch,
    };
    let target = Target::Session {
        revision: client.snapshot().unwrap().revision,
    };
    connection
        .send(
            client
                .stage_image(key, target, image)
                .map_err(|e| anyhow!("{e:?}"))?,
        )
        .map_err(|e| anyhow!("{e:?}"))?;
    persistence_reply(&mut client, &mut connection, key, |reply| {
        matches!(reply, Reply::Ready { .. })
    })?;
    let message = client
        .command(Command::CommitPersistence(key), None)
        .map_err(|e| anyhow!("{e:?}"))?;
    let ClientMessage::Submit { operation, .. } = &message else {
        unreachable!()
    };
    let operation = *operation;
    connection.send(message).map_err(|e| anyhow!("{e:?}"))?;
    wait(&mut client, &mut connection, |client| {
        client.pending().contains_key(&operation) || client.results().contains_key(&operation)
    })?;
    drop(connection);
    std::thread::sleep(Duration::from_millis(50));
    (client, connection) = attach_as(entry, actor)?;
    ensure!(
        client.results().is_empty() && client.pending().is_empty(),
        "a new connection retained old request assumptions"
    );
    let deadline = Instant::now() + Duration::from_secs(120);
    let outcome = loop {
        if let Some(result) = client
            .snapshot()
            .unwrap()
            .results
            .iter()
            .find(|result| result.id == operation)
        {
            break result.outcome.clone();
        }
        connection.poll(&mut client).map_err(|e| anyhow!("{e:?}"))?;
        ensure!(
            Instant::now() < deadline,
            "accepted host work did not complete"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    ensure!(
        matches!(outcome, Outcome::Confirmed { .. }),
        "host work failed: {outcome:?}"
    );
    let state = client.snapshot().unwrap();
    let tracks = state.tracks.iter().map(|t| t.id).collect::<Vec<_>>();
    ensure!(
        state.persistence_epoch == epoch + 1
            && state.tracks[1].id != old_track
            && state
                .tracks
                .iter()
                .flat_map(|t| &t.loops)
                .all(|l| l.mode == LoopMode::Stopped),
        "lost commit did not restore one fresh stopped session"
    );
    command(
        &mut client,
        &mut connection,
        Command::Global(GlobalControlAction::SetSolo(false)),
    )?;
    ensure!(
        client
            .snapshot()
            .unwrap()
            .tracks
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>()
            == tracks
            && client.snapshot().unwrap().persistence_epoch == epoch + 1,
        "reconciliation replayed the load"
    );
    println!("Socket file loss: abandoned upload/capture reclaimed; fresh connection observed accepted host load once without replay");
    client.management_shutdown().map_err(|e| anyhow!("{e:?}"))
}
