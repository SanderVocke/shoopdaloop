//! Packaged companion canary using the public application protocol.
use anyhow::{anyhow, ensure, Result};
use shoop_app_client::{Client, State};
use shoop_app_protocol::{ClientId, ClientMessage, Command, Outcome};
use shoop_egui::{
    AudioDriverConfig, DummyAudioDriverConfig, EngineTransportKind, GlobalControlAction,
    LoopAction, LoopMode, MidiNote, PianoAction, TrackAction, TrackSpec, TrackSpecTopology,
};
use shoop_host::{
    config::HostConfig,
    connection::{self, Connection},
    registry::{AppTransport, HostEntry, Registry},
};
use std::{
    process::{Child, Command as Process, Stdio},
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
        ensure!(Instant::now() < deadline, "session probe timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}
fn attach(entry: &HostEntry) -> Result<(Client, Connection)> {
    let mut connection = connection::connect(entry, Duration::from_secs(3))?;
    let mut client = Client::new(
        ClientId(shoop_host::registry::identity()?),
        entry.host,
        entry.session,
    )
    .map_err(|error| anyhow!("{error:?}"))?;
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
    Ok(result)
}
pub fn run(transport: EngineTransportKind) -> Result<()> {
    ensure!(
        transport != EngineTransportKind::InProcess,
        "session companion probe requires socket or shared_memory"
    );
    let root = tempfile::tempdir()?;
    let registry = Registry::open(&root.path().join("registry"))?;
    let config = HostConfig {
        driver: AudioDriverConfig::Dummy(DummyAudioDriverConfig::default()),
        pools: shoop_egui::NewEnginePools {
            tracks: 3,
            loops: 20,
            channels: 2,
            loop_memory_mb: 16,
        },
        transport: if transport == EngineTransportKind::Socket {
            AppTransport::Socket
        } else {
            AppTransport::SharedMemory
        },
        ..Default::default()
    };
    let config_file = root.path().join("config.json");
    std::fs::write(&config_file, serde_json::to_vec(&config)?)?;
    let companion = shoop_host::launch::companion_path(&std::env::current_exe()?)?;
    let mut host = ProbeHost(
        Process::new(companion)
            .arg("--config")
            .arg(config_file)
            .arg("--registry")
            .arg(registry.root())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(12);
    let (path, entry) = loop {
        if let Some(entry) = registry.list()?.into_iter().next() {
            break entry;
        }
        ensure!(
            host.0.try_wait()?.is_none(),
            "session companion failed before readiness"
        );
        ensure!(
            Instant::now() < deadline,
            "session companion startup timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    let (mut client, mut connection) = attach(&entry)?;
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
    let before = client.snapshot().unwrap().status.processed_frames;
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
    ensure!(
        !path.exists() && !entry.credential.exists(),
        "session registration was not cleaned up"
    );
    Ok(())
}
