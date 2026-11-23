//! Temporary application-API client for live prepared reference graphs.
use anyhow::{ensure, Result};
#[cfg(not(test))]
use clap::Parser;
use shoop_app_api::*;
use shoop_app_client::{Client, State};
use shoop_app_protocol::{immediate, ClientMessage, Command, Outcome};
use shoop_host::{
    connection::{self, Connection},
    remote::Target,
};
use std::{
    path::PathBuf,
    process::{Child, Command as Process, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(not(test))]
#[derive(Parser)]
struct Args {
    #[arg(long)]
    host: PathBuf,
    #[arg(long, default_value = "work/fpga-reference-client")]
    output: PathBuf,
}
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn error(e: shoop_app_client::Error) -> anyhow::Error {
    anyhow::anyhow!("application API: {e:?}")
}
#[track_caller]
fn wait(
    client: &mut Client,
    connection: &mut Connection,
    predicate: impl Fn(&Client) -> bool,
) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        connection.poll(client).map_err(error)?;
        if let Some(ready) = client.ready_message() {
            connection.send(ready).map_err(error)?;
            client.ready_sent().map_err(error)?;
        }
        if predicate(client) {
            return Ok(());
        }
        ensure!(
            Instant::now() < until,
            "application observation timed out at {}; state={:?}",
            std::panic::Location::caller(),
            client.state()
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn command(client: &mut Client, connection: &mut Connection, command: Command) -> Result<Outcome> {
    let message = client.command(command, None).map_err(error)?;
    let ClientMessage::Submit { operation, .. } = message else {
        unreachable!()
    };
    connection.send(message).map_err(error)?;
    wait(client, connection, |c| c.results().contains_key(&operation))?;
    let result = client.results()[&operation].clone();
    if let Outcome::Confirmed { revision }
    | Outcome::TrackCreated { revision, .. }
    | Outcome::LoopCreated { revision, .. } = &result
    {
        wait(client, connection, |c| {
            c.snapshot().is_some_and(|s| s.revision >= *revision)
        })?;
    }
    Ok(result)
}
fn accepted(client: &mut Client, connection: &mut Connection, value: Command) -> Result<Outcome> {
    let description = format!("{value:?}");
    let result = command(client, connection, value)?;
    ensure!(
        !matches!(result, Outcome::Refused { .. }),
        "application command {description} refused: {result:?}"
    );
    Ok(result)
}
fn immediate(
    client: &Client,
    connection: &mut Connection,
    commands: Vec<immediate::Command>,
) -> Result<()> {
    connection
        .send(client.immediate(commands).map_err(error)?)
        .map_err(error)
}
fn loop_view(client: &Client, id: LoopId) -> Option<&LoopState> {
    client
        .snapshot()?
        .tracks
        .iter()
        .flat_map(|t| &t.loops)
        .find(|l| l.id == id)
}
pub fn run_with(host: PathBuf, output: PathBuf) -> Result<()> {
    run_backend(host, output, "rust-fpga")
}
pub fn run_backend(host: PathBuf, output: PathBuf, backend: &str) -> Result<()> {
    run_configuration(host, output, backend, "socket")
}
pub fn run_configuration(
    host: PathBuf,
    output: PathBuf,
    backend: &str,
    transport: &str,
) -> Result<()> {
    std::fs::create_dir_all(&output)?;
    let output = output.canonicalize()?;
    let config = serde_json::json!({ "backend": backend,
        "driver":{"Dummy":{"sample_rate":8000,"buffer_size":32}}, "loop_smoothing_ms":0,
        "processors":{"builtin_fx":"off","oxisynth":"off","fx_instances":0,"synth_instances":0,"memory_mb":16},
        "pools":{"tracks":2,"loops":16,"channels":1,"loop_memory_mb":1}, "transport":transport });
    std::fs::write(
        output.join("config.json"),
        serde_json::to_vec_pretty(&config)?,
    )?;
    let profile = output.join("profile.json");
    if profile.exists() {
        std::fs::remove_file(&profile)?;
    }
    let mut child = ChildGuard(
        Process::new(host)
            .arg("--backend")
            .arg(backend)
            .arg("--config")
            .arg(output.join("config.json"))
            .arg("--export-profile")
            .arg(&profile)
            .arg("--snapshot-hz")
            .arg("unlimited")
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(output.join("host.log"))?)
            .spawn()?,
    );
    // A cold RTL-sim start builds the Verilator model before publishing the profile.
    let startup_timeout = if backend == "rtl-sim" { 120 } else { 20 };
    let deadline = Instant::now() + Duration::from_secs(startup_timeout);
    while !profile.exists() {
        ensure!(
            child.0.try_wait()?.is_none(),
            "reference host exited; inspect host.log"
        );
        ensure!(Instant::now() < deadline, "host startup timeout");
        thread::sleep(Duration::from_millis(10));
    }
    let target = Target::read(&profile)?;
    let mut client = Client::new();
    let mut connection = connection::connect(&target, Duration::from_secs(5))?;
    connection
        .send(client.attach().map_err(error)?)
        .map_err(error)?;
    wait(&mut client, &mut connection, |c| c.state() == State::Ready)?;
    let sync = client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .find(|t| t.is_sync)
        .unwrap()
        .loops[0]
        .id;
    let created = accepted(
        &mut client,
        &mut connection,
        Command::AddTrack(TrackSpec {
            name: "Prepared mono".into(),
            topology: TrackSpecTopology::Direct {
                audio_channels: 1,
                midi: false,
            },
            latency: Default::default(),
            initial_output_bus_names: vec!["Master".into()],
            creation_request_id: None,
        }),
    )?;
    let Outcome::TrackCreated { track, loops, .. } = created else {
        anyhow::bail!("missing track identities");
    };
    let voice = loops[0];
    let input = client
        .snapshot()
        .unwrap()
        .connections
        .application_ports
        .iter()
        .find(|p| {
            matches!(p.owner, ApplicationPortOwner::Track {track_id, ..} if track_id == track)
                && p.role == PortRole::AudioInput
        })
        .unwrap()
        .id;
    let source = client
        .snapshot()
        .unwrap()
        .connections
        .host_ports
        .iter()
        .find(|p| p.data_type == PortDataType::Audio && p.direction == PortDirection::Output)
        .unwrap()
        .id
        .clone();
    accepted(
        &mut client,
        &mut connection,
        Command::SetPortConnected {
            port: input,
            host_port: source.clone(),
            connected: true,
        },
    )?;
    let unchanged = command(&mut client, &mut connection, Command::ActivatePreparedGraph)?;
    ensure!(
        matches!(unchanged, Outcome::Refused { .. }),
        "unchanged layout was submitted twice: {unchanged:?}"
    );
    immediate(
        &client,
        &mut connection,
        vec![immediate::Command::Transition(
            sync,
            TransitionMode::Recording.into(),
        )],
    )?;
    wait(&mut client, &mut connection, |c| {
        loop_view(c, sync).is_some_and(|l| l.mode == LoopMode::Recording && l.length_frames > 1024)
    })?;
    let capture_before = loop_view(&client, sync).unwrap().length_frames;
    immediate(
        &client,
        &mut connection,
        vec![immediate::Command::LoopGain(voice, 0.375)],
    )?;
    wait(&mut client, &mut connection, |c| {
        loop_view(c, voice).is_some_and(|l| l.gain == 0.375)
    })?;
    let created = accepted(&mut client, &mut connection, Command::AddLoop { track })?;
    let Outcome::LoopCreated { loop_id: added, .. } = created else {
        anyhow::bail!("missing added loop identity");
    };
    wait(&mut client, &mut connection, |c| {
        loop_view(c, sync)
            .is_some_and(|l| l.mode == LoopMode::Recording && l.length_frames > capture_before)
            && loop_view(c, added)
                .is_some_and(|l| l.mode == LoopMode::Stopped && l.length_frames == 0)
            && loop_view(c, voice).is_some_and(|l| l.gain == 0.375)
    })?;
    immediate(
        &client,
        &mut connection,
        vec![immediate::Command::Transition(
            sync,
            TransitionMode::Playing.into(),
        )],
    )?;
    wait(&mut client, &mut connection, |c| {
        loop_view(c, sync).is_some_and(|l| l.mode == LoopMode::Playing)
    })?;
    immediate(
        &client,
        &mut connection,
        vec![immediate::Command::Transition(
            voice,
            TransitionPlan {
                now: None,
                first: Some(PlannedTransition {
                    mode: TransitionMode::Recording,
                    skip: 0,
                }),
                second: Some(PlannedTransition {
                    mode: TransitionMode::Stopped,
                    skip: 2,
                }),
            },
        )],
    )?;
    wait(&mut client, &mut connection, |c| {
        loop_view(c, voice).is_some_and(|l| l.mode == LoopMode::Recording || l.length_frames > 0)
    })?;
    wait(&mut client, &mut connection, |c| {
        loop_view(c, voice).is_some_and(|l| l.mode == LoopMode::Stopped && l.length_frames > 0)
    })?;
    immediate(
        &client,
        &mut connection,
        vec![
            immediate::Command::LoopGain(voice, 0.5),
            immediate::Command::Transition(voice, TransitionMode::Playing.into()),
        ],
    )?;
    wait(&mut client, &mut connection, |c| {
        loop_view(c, voice).is_some_and(|l| l.mode == LoopMode::Playing && l.gain == 0.5)
    })?;
    let recorded = loop_view(&client, voice).unwrap().length_frames;
    accepted(
        &mut client,
        &mut connection,
        Command::SetPortConnected {
            port: input,
            host_port: source.clone(),
            connected: false,
        },
    )?;
    wait(&mut client, &mut connection, |c| {
        loop_view(c, voice).is_some_and(|l| {
            l.mode == LoopMode::Playing && l.gain == 0.5 && l.length_frames == recorded
        })
    })?;
    immediate(
        &client,
        &mut connection,
        vec![
            immediate::Command::ClearPlannedTransitions(voice, ClearPlanned::Both),
            immediate::Command::Transition(voice, TransitionMode::Stopped.into()),
            immediate::Command::Transition(sync, TransitionMode::Stopped.into()),
        ],
    )?;
    wait(&mut client, &mut connection, |c| {
        [voice, sync]
            .iter()
            .all(|&id| loop_view(c, id).is_some_and(|l| l.mode == LoopMode::Stopped))
    })?;
    accepted(
        &mut client,
        &mut connection,
        Command::Track {
            track,
            action: TrackAction::Remove,
        },
    )?;
    let replacement = accepted(
        &mut client,
        &mut connection,
        Command::AddTrack(TrackSpec {
            name: "Replacement mono".into(),
            topology: TrackSpecTopology::Direct {
                audio_channels: 1,
                midi: false,
            },
            latency: Default::default(),
            initial_output_bus_names: vec!["Master".into()],
            creation_request_id: None,
        }),
    )?;
    let Outcome::TrackCreated {
        track: replacement_track,
        loops,
        ..
    } = replacement
    else {
        anyhow::bail!("missing replacement track");
    };
    let replacement_voice = loops[0];
    let replacement_input=client.snapshot().unwrap().connections.application_ports.iter().find(|p|
        matches!(p.owner,ApplicationPortOwner::Track {track_id,..} if track_id==replacement_track) && p.role==PortRole::AudioInput).unwrap().id;
    accepted(
        &mut client,
        &mut connection,
        Command::SetPortConnected {
            port: replacement_input,
            host_port: source,
            connected: true,
        },
    )?;
    wait(&mut client, &mut connection, |c| {
        loop_view(c, replacement_voice)
            .is_some_and(|l| l.mode == LoopMode::Stopped && l.length_frames == 0 && l.gain == 1.0)
    })?;
    immediate(
        &client,
        &mut connection,
        vec![
            immediate::Command::LoopGain(replacement_voice, 0.75),
            immediate::Command::Transition(replacement_voice, TransitionMode::Recording.into()),
        ],
    )?;
    wait(&mut client, &mut connection, |c| {
        loop_view(c, replacement_voice).is_some_and(|l| {
            l.mode == LoopMode::Recording && l.length_frames >= 128 && l.gain == 0.75
        })
    })?;
    immediate(
        &client,
        &mut connection,
        vec![immediate::Command::Transition(
            replacement_voice,
            TransitionMode::Stopped.into(),
        )],
    )?;
    wait(&mut client, &mut connection, |c| {
        loop_view(c, replacement_voice)
            .is_some_and(|l| l.mode == LoopMode::Stopped && l.length_frames >= 128)
    })?;
    let arrangement = loops[1];
    accepted(
        &mut client,
        &mut connection,
        Command::Composite(shoop_app_protocol::composite::Edit::Append {
            target: arrangement,
            add: vec![replacement_voice],
            parallel: false,
            convert_if_empty: true,
        }),
    )?;
    wait(&mut client, &mut connection, |c| {
        loop_view(c, arrangement)
            .is_some_and(|l| l.composite_kind == CompositeKind::Regular && l.length_frames > 0)
    })?;
    let report = serde_json::json!({"result":"passed","boundary":"shoop_host application API through RtBackend", "recorded_frames": recorded,
        "live_graph_updates":3,"recording_and_gain_preserved":true,"recycled_logical_slots":true,"unchanged_activation_refused":true,"immediate_controls":true,"composite_edit_verified":true});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
#[cfg(not(test))]
fn main() {
    let args = Args::parse();
    if let Err(error) = run_with(args.host, args.output) {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
