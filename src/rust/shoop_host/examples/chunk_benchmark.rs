use anyhow::{ensure, Context, Result};
use clap::Parser;
use shoop_app_api::*;
use shoop_app_client::{Client, State};
use shoop_app_protocol::{
    ClientMessage, Command, ManagementReply, ManagementRequest, Outcome, VERSION,
};
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

#[derive(Parser)]
struct Args {
    #[arg(long)]
    host: PathBuf,
    #[arg(long, default_value = "current")]
    mode: String,
    #[arg(long, default_value = "idle")]
    scenario: String,
    #[arg(long, default_value = "2")]
    seconds: f64,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value = "32")]
    quantum: u32,
    #[arg(long, default_value = "256")]
    buffer: u32,
    #[arg(long, default_value = "4")]
    tracks: u32,
    #[arg(long, default_value = "16")]
    loops: u32,
    #[arg(long, default_value = "4")]
    voices: usize,
    #[arg(long)]
    callbacks: Option<u64>,
    #[arg(long, default_value = "0.5")]
    warmup: f64,
    #[arg(long, default_value = "signal")]
    input: String,
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn app_error(error: shoop_app_client::Error) -> anyhow::Error {
    anyhow::anyhow!("app connection: {error:?}")
}

fn pump(client: &mut Client, connection: &mut Connection, seconds: f64) -> Result<()> {
    let end = Instant::now() + Duration::from_secs_f64(seconds);
    while Instant::now() < end {
        connection.poll(client).map_err(app_error)?;
        thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

fn command(client: &mut Client, connection: &mut Connection, value: Command) -> Result<Outcome> {
    let message = client.command(value, None).map_err(app_error)?;
    let ClientMessage::Submit { operation, .. } = message else {
        anyhow::bail!("expected submit");
    };
    connection.send(message).map_err(app_error)?;
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        connection.poll(client).map_err(app_error)?;
        if let Some(outcome) = client.results().get(&operation) {
            ensure!(
                !matches!(outcome, Outcome::Refused { .. }),
                "operation refused: {outcome:?}"
            );
            return Ok(outcome.clone());
        }
        ensure!(Instant::now() < end, "operation timed out");
        thread::sleep(Duration::from_millis(2));
    }
}

fn verify_media(
    client: &mut Client,
    connection: &mut Connection,
    loop_id: LoopId,
    signal: &str,
    output: &std::path::Path,
    label: &str,
) -> Result<()> {
    let read_end = Instant::now() + Duration::from_secs(10);
    loop {
        connection
            .send(client.read_media(loop_id).map_err(app_error)?)
            .map_err(app_error)?;
        while client.media().is_none() {
            pump(client, connection, 0.005)?;
            ensure!(Instant::now() < read_end, "media read timeout");
        }
        if matches!(
            client.media(),
            Some(Err(shoop_app_protocol::Refusal::NotReady))
        ) {
            ensure!(Instant::now() < read_end, "media did not become ready");
            pump(client, connection, 0.02)?;
        } else {
            break;
        }
    }
    let media = client
        .media()
        .unwrap()
        .as_ref()
        .map_err(|error| anyhow::anyhow!("media: {error:?}"))?;
    ensure!(
        signal == "silence"
            || media
                .audio
                .iter()
                .any(|channel| channel.samples.iter().any(|sample| sample.abs() > 0.001)),
        "captured audio is silent"
    );
    let observed:Vec<_> = media.audio.iter().map(|channel| serde_json::json!({"frames":channel.samples.len(),"first_samples":channel.samples.iter().take(16).copied().collect::<Vec<_>>()})).collect();
    std::fs::write(
        output.join(format!("media-{label}.json")),
        serde_json::to_vec_pretty(&observed)?,
    )?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(
        args.seconds > 0. && args.seconds.is_finite(),
        "invalid duration"
    );
    ensure!(
        matches!(
            args.scenario.as_str(),
            "idle"
                | "record"
                | "play"
                | "grab"
                | "ramps"
                | "shared"
                | "continue"
                | "record-play"
                | "churn"
        ),
        "invalid scenario"
    );
    ensure!(
        matches!(args.input.as_str(), "signal" | "silence"),
        "invalid input"
    );
    ensure!(
        args.voices > 0 && args.voices < args.loops as usize,
        "voices exceed loop capacity"
    );
    std::fs::create_dir_all(&args.output)?;
    let output = args.output.canonicalize()?;
    let config = serde_json::json!({
        "driver": {"Dummy": {"sample_rate":48000,"buffer_size":args.buffer}},
        "processors": {"builtin_fx":"off","oxisynth":"off","fx_instances":0,"synth_instances":0,"memory_mb":16},
        "pools": {"quantum":args.quantum,"tracks":args.tracks,"loops":args.loops,"channels":2,"loop_memory_mb":16},
        "transport":"socket"
    });
    std::fs::write(
        output.join("config.json"),
        serde_json::to_vec_pretty(&config)?,
    )?;
    let profile = output.join("profile.json");
    if profile.exists() {
        std::fs::remove_file(&profile)?;
    }
    let mut child = ChildGuard(
        Process::new(&args.host)
            .args(["--config"])
            .arg(output.join("config.json"))
            .arg("--driver-preferences")
            .arg(output.join("driver-preferences.json"))
            .arg("--export-profile")
            .arg(&profile)
            .env("SHOOP_AUDIO_RENDER", &args.mode)
            .env("SHOOP_AUDIO_BENCH_AUDIO_ONLY", "1")
            .env(
                "SHOOP_AUDIO_BENCH_INPUT",
                if args.scenario == "idle" {
                    "silence"
                } else {
                    args.input.as_str()
                },
            )
            .env("SHOOP_AUDIO_BENCH_OUTPUT", output.join("callbacks.csv"))
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(output.join("host.log"))?)
            .spawn()?,
    );
    let end = Instant::now() + Duration::from_secs(30);
    while !profile.exists() {
        ensure!(
            child.0.try_wait()?.is_none(),
            "host exited; inspect host.log"
        );
        ensure!(Instant::now() < end, "host readiness timeout");
        thread::sleep(Duration::from_millis(10));
    }
    let target = Target::read(&profile)?;
    let mut connection = connection::connect(&target, Duration::from_secs(5))?;
    let mut client = Client::new();
    connection
        .send(client.attach().map_err(app_error)?)
        .map_err(app_error)?;
    while client.state() != State::Ready {
        connection.poll(&mut client).map_err(app_error)?;
        if let Some(message) = client.ready_message() {
            connection.send(message).map_err(app_error)?;
            client.ready_sent().map_err(app_error)?;
        }
        ensure!(Instant::now() < end, "app attachment timeout");
        thread::sleep(Duration::from_millis(2));
    }
    pump(&mut client, &mut connection, 0.1)?;
    ensure!(
        !client
            .snapshot()
            .context("snapshot missing")?
            .tracks
            .is_empty(),
        "default sync track missing"
    );
    let mut active = None;
    if args.scenario != "idle" {
        command(
            &mut client,
            &mut connection,
            Command::Global(GlobalControlAction::SetSync(false)),
        )?;
        let outcome = command(
            &mut client,
            &mut connection,
            Command::AddTrack(TrackSpec {
                name: "Benchmark".into(),
                topology: TrackSpecTopology::Direct {
                    audio_channels: 2,
                    midi: false,
                },
                latency: Default::default(),
                initial_output_bus_names: vec!["Master".into()],
                creation_request_id: None,
            }),
        )?;
        let Outcome::TrackCreated {
            track, mut loops, ..
        } = outcome
        else {
            anyhow::bail!("track creation: {outcome:?}");
        };
        while loops.len() < args.voices.max(2) {
            let outcome = command(&mut client, &mut connection, Command::AddLoop { track })?;
            let Outcome::LoopCreated { loop_id, .. } = outcome else {
                anyhow::bail!("loop creation: {outcome:?}");
            };
            loops.push(loop_id);
        }
        active = Some((track, loops.clone()));
        pump(&mut client, &mut connection, 0.1)?;
        let inputs: Vec<_> = client.snapshot().unwrap().connections.application_ports.iter()
            .filter(|port| matches!(port.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == track) && port.role == PortRole::AudioInput)
            .map(|port| port.id).collect();
        let hosts: Vec<_> = client
            .snapshot()
            .unwrap()
            .connections
            .host_ports
            .iter()
            .filter(|port| {
                port.data_type == PortDataType::Audio && port.direction == PortDirection::Output
            })
            .map(|port| port.id.clone())
            .collect();
        for (port, host_port) in inputs.into_iter().zip(hosts) {
            command(
                &mut client,
                &mut connection,
                Command::SetPortConnected {
                    port,
                    host_port,
                    connected: true,
                },
            )?;
        }
        command(
            &mut client,
            &mut connection,
            Command::Loop {
                track,
                loop_id: loops[0],
                action: LoopAction::RecordClicked,
            },
        )?;
        pump(&mut client, &mut connection, 0.2)?;
        if args.scenario != "record" {
            command(
                &mut client,
                &mut connection,
                Command::Loop {
                    track,
                    loop_id: loops[0],
                    action: LoopAction::PlayClicked,
                },
            )?;
            pump(&mut client, &mut connection, 0.1)?;
            verify_media(
                &mut client,
                &mut connection,
                loops[0],
                &args.input,
                &output,
                "recorded",
            )?;
        }
        if matches!(args.scenario.as_str(), "play" | "ramps" | "record-play") {
            for &loop_id in loops.iter().take(args.voices).skip(1) {
                command(
                    &mut client,
                    &mut connection,
                    Command::Loop {
                        track,
                        loop_id,
                        action: LoopAction::RecordClicked,
                    },
                )?;
            }
            pump(&mut client, &mut connection, 0.2)?;
            if args.scenario != "record-play" {
                for &loop_id in loops.iter().take(args.voices).skip(1) {
                    command(
                        &mut client,
                        &mut connection,
                        Command::Loop {
                            track,
                            loop_id,
                            action: LoopAction::PlayClicked,
                        },
                    )?;
                }
            }
        }
        if matches!(args.scenario.as_str(), "grab" | "shared" | "continue") {
            let sync = client
                .snapshot()
                .unwrap()
                .tracks
                .iter()
                .find(|track| track.is_sync)
                .unwrap();
            let sync_id = sync.id;
            let sync_loop = sync.loops[0].id;
            command(
                &mut client,
                &mut connection,
                Command::Loop {
                    track: sync_id,
                    loop_id: sync_loop,
                    action: LoopAction::RecordClicked,
                },
            )?;
            pump(&mut client, &mut connection, 0.1)?;
            command(
                &mut client,
                &mut connection,
                Command::Loop {
                    track: sync_id,
                    loop_id: sync_loop,
                    action: LoopAction::PlayClicked,
                },
            )?;
            pump(&mut client, &mut connection, 0.2)?;
            let targets = if args.scenario == "grab" {
                loops[1..2].to_vec()
            } else {
                loops[..args.voices].to_vec()
            };
            let operation = if args.scenario == "continue" {
                ControlOperation::AdoptRingbuffers {
                    loops: targets,
                    reverse_cycle_start: 1,
                    cycles_length: 1,
                    go_to_cycle: 1,
                    go_to_mode: LoopMode::Recording,
                }
            } else {
                ControlOperation::Grab { loops: targets }
            };
            command(&mut client, &mut connection, Command::Control(operation))?;
            if args.scenario != "continue" {
                verify_media(
                    &mut client,
                    &mut connection,
                    loops[1],
                    &args.input,
                    &output,
                    "grabbed",
                )?;
            }
        }
    }
    pump(&mut client, &mut connection, args.warmup)?;
    let snapshot = client.snapshot().unwrap();
    let setup = serde_json::to_value(snapshot)?;
    if let Some((track, ref loops)) = active {
        let state = snapshot
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .context("track absent")?;
        ensure!(
            state
                .loops
                .iter()
                .any(|entry| entry.id == loops[0] && entry.length_frames > 0),
            "loop never captured"
        );
    }
    let first_frame = (snapshot
        .status
        .processed_frames
        .div_ceil(args.buffer as u64)
        + 4)
        * args.buffer as u64;
    let callbacks = args
        .callbacks
        .unwrap_or_else(|| (args.seconds * 48000. / args.buffer as f64).ceil() as u64);
    ensure!(callbacks > 0, "empty measurement");
    let last_frame = first_frame + callbacks * args.buffer as u64;
    let end = Instant::now()
        + Duration::from_secs_f64(
            args.seconds
                .max(callbacks as f64 * args.buffer as f64 / 48000.)
                * 4.
                + 30.,
        );
    let mut tick = 0;
    let mut next_control = Instant::now();
    while client.snapshot().unwrap().status.processed_frames < last_frame + args.buffer as u64 {
        ensure!(Instant::now() < end, "measurement stalled");
        if args.scenario == "ramps" && Instant::now() >= next_control {
            let (track, ref loops) = active.as_ref().unwrap();
            let value = if tick % 2 == 0 { -12. } else { 0. };
            command(
                &mut client,
                &mut connection,
                Command::Track {
                    track: *track,
                    action: TrackAction::OutputGainChanged(value),
                },
            )?;
            command(
                &mut client,
                &mut connection,
                Command::Track {
                    track: *track,
                    action: TrackAction::OutputBalanceChanged(if tick % 2 == 0 {
                        -0.7
                    } else {
                        0.5
                    }),
                },
            )?;
            command(
                &mut client,
                &mut connection,
                Command::Control(ControlOperation::SetLoopGain {
                    loops: loops[..args.voices].to_vec(),
                    gain: if tick % 2 == 0 { 0.5 } else { 1. },
                }),
            )?;
            tick += 1;
            next_control = Instant::now() + Duration::from_millis(4);
        } else if args.scenario == "churn" && Instant::now() >= next_control {
            let (track, _) = active.as_ref().unwrap();
            command(
                &mut client,
                &mut connection,
                Command::Track {
                    track: *track,
                    action: TrackAction::Remove,
                },
            )?;
            let result = command(
                &mut client,
                &mut connection,
                Command::AddTrack(TrackSpec {
                    name: "Benchmark churn".into(),
                    topology: TrackSpecTopology::Direct {
                        audio_channels: 2,
                        midi: false,
                    },
                    latency: Default::default(),
                    initial_output_bus_names: vec!["Master".into()],
                    creation_request_id: None,
                }),
            )?;
            let Outcome::TrackCreated { track, loops, .. } = result else {
                anyhow::bail!("recreate: {result:?}");
            };
            pump(&mut client, &mut connection, 0.02)?;
            let snapshot = client.snapshot().unwrap();
            let ports:Vec<_> = snapshot.connections.application_ports.iter().filter(|port|matches!(port.owner,ApplicationPortOwner::Track {track_id,..} if track_id==track) && port.role==PortRole::AudioInput).map(|port|port.id).collect();
            let hosts: Vec<_> = snapshot
                .connections
                .host_ports
                .iter()
                .filter(|port| {
                    port.data_type == PortDataType::Audio && port.direction == PortDirection::Output
                })
                .map(|port| port.id.clone())
                .collect();
            for (port, host_port) in ports.into_iter().zip(hosts) {
                command(
                    &mut client,
                    &mut connection,
                    Command::SetPortConnected {
                        port,
                        host_port,
                        connected: true,
                    },
                )?;
            }
            command(
                &mut client,
                &mut connection,
                Command::Loop {
                    track,
                    loop_id: loops[0],
                    action: LoopAction::RecordClicked,
                },
            )?;
            active = Some((track, loops));
            tick += 1;
            next_control = Instant::now() + Duration::from_millis(50);
        }
        pump(&mut client, &mut connection, 0.002)?;
    }
    std::fs::write(
        output.join("run.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "mode":args.mode,"scenario":args.scenario,"first_frame":first_frame,"last_frame":last_frame,
            "quantum":args.quantum,"buffer":args.buffer,"seconds":args.seconds,"config":config,
            "host":args.host,"architecture":std::env::consts::ARCH,"callbacks":callbacks,"warmup_seconds":args.warmup,"voices":args.voices,"input":args.input,"setup":setup,"control_updates":tick
        }))?,
    )?;
    if matches!(
        args.scenario.as_str(),
        "record" | "record-play" | "continue" | "churn"
    ) {
        let (track, loops) = active.as_ref().unwrap();
        let index = usize::from(matches!(args.scenario.as_str(), "record-play" | "continue"));
        command(
            &mut client,
            &mut connection,
            Command::Loop {
                track: *track,
                loop_id: loops[index],
                action: LoopAction::PlayClicked,
            },
        )?;
        verify_media(
            &mut client,
            &mut connection,
            loops[index],
            &args.input,
            &output,
            "finished-capture",
        )?;
    }
    std::fs::write(
        output.join("final-state.json"),
        serde_json::to_vec_pretty(client.snapshot().unwrap())?,
    )?;
    let ClientMessage::Shutdown {
        authority,
        operation,
    } = client.shutdown().map_err(app_error)?
    else {
        anyhow::bail!("shutdown message");
    };
    let mut reply = connection::management(
        &target,
        ManagementRequest::Shutdown {
            authority,
            operation,
        },
        Duration::from_secs(5),
    )?;
    let finish = Instant::now() + Duration::from_secs(10);
    while !matches!(reply, ManagementReply::Finished { .. }) {
        ensure!(
            matches!(reply, ManagementReply::Accepted { .. }),
            "shutdown: {reply:?}"
        );
        ensure!(Instant::now() < finish, "shutdown timeout");
        thread::sleep(Duration::from_millis(10));
        reply = connection::management(
            &target,
            ManagementRequest::Result {
                authority,
                operation,
            },
            Duration::from_secs(2),
        )?;
    }
    while child.0.try_wait()?.is_none() {
        ensure!(Instant::now() < finish, "host exit timeout");
        thread::sleep(Duration::from_millis(10));
    }
    let _ = VERSION;
    println!(
        "{} {}: measured frames {first_frame}..{last_frame}; {}",
        args.mode,
        args.scenario,
        output.display()
    );
    Ok(())
}
