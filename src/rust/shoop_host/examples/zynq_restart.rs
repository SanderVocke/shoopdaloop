//! Destructive, supervised physical FPGA restart qualification (65 C cutoff).
use anyhow::{anyhow, bail, ensure, Context, Result};
use shoop_app_api::{
    host_health, LoopMode, PlannedTransition, TrackSpec, TrackSpecTopology, TransitionMode,
    TransitionPlan,
};
use shoop_app_client::{Client, State};
use shoop_app_protocol::{
    immediate, BackendRestartPhase as Phase, BackendRestartStatus, ClientMessage, Command,
    ManagementReply, ManagementRequest, OperationId, Outcome, Refusal, VERSION,
};
use shoop_host::{
    connection::{self, Connection},
    remote::RemoteProfile,
};
use shoop_transport::RecordPort;
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

struct Session {
    client: Client,
    peer: Connection,
    previous: Instant,
}
struct Health {
    last: Instant,
    printed: Instant,
    samples: usize,
}
impl Health {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            last: now,
            printed: now - Duration::from_secs(1),
            samples: 0,
        }
    }
    fn deadline(&self) -> Result<()> {
        ensure!(
            self.last.elapsed() < Duration::from_secs(3),
            "no fresh temperature for three seconds"
        );
        Ok(())
    }
    fn check(&mut self, session: &Session) -> Result<()> {
        if let Some(source) = session
            .client
            .diagnostics()
            .sources
            .iter()
            .find(|s| s.descriptor.stream.source == host_health::SOURCE)
        {
            ensure!(
                source.supported && source.descriptor.available,
                "health source unavailable"
            );
            if source.age_ms == 0 && self.printed.elapsed() > Duration::from_millis(400) {
                if let Some(report) = session.client.diagnostics().health {
                    let sensor = report
                        .temperatures
                        .iter()
                        .flatten()
                        .next()
                        .context("no temperature sensor")?;
                    let current = sensor
                        .current_millicelsius
                        .context("temperature unavailable")?;
                    ensure!(
                        current < 65000 && report.overtemperature != Some(true),
                        "65 C/overtemperature cutoff reached"
                    );
                    self.last = Instant::now();
                    self.printed = self.last;
                    self.samples += 1;
                    println!(
                        "{}",
                        serde_json::json!({"event":"health","current_millicelsius":current,"peak_millicelsius":sensor.peak_millicelsius,"overtemperature":report.overtemperature,"frames":session.client.snapshot().map(|s|s.status.processed_frames),"xruns":session.client.snapshot().map(|s|s.status.xruns)})
                    );
                }
            }
        }
        self.deadline()
    }
}
impl Session {
    fn pump(&mut self, health: &mut Health) -> Result<()> {
        let now = Instant::now();
        self.client
            .advance_diagnostics(now.duration_since(self.previous));
        self.previous = now;
        self.peer
            .poll(&mut self.client)
            .map_err(|e| anyhow!("data poll: {e:?}"))?;
        if let Some(message) = self.client.ready_message() {
            self.peer
                .send(message)
                .map_err(|e| anyhow!("Ready: {e:?}"))?;
            self.client
                .ready_sent()
                .map_err(|e| anyhow!("Ready state: {e:?}"))?;
        }
        health.check(self)?;
        thread::sleep(Duration::from_millis(2));
        Ok(())
    }
    fn attach(profile: &RemoteProfile, health: &mut Health) -> Result<Self> {
        let initial = health.samples == 0;
        let mut peer =
            connection::connect(profile, Duration::from_secs(if initial { 5 } else { 1 }))?;
        if initial {
            health.last = Instant::now();
        }
        let mut client = Client::new();
        peer.send(client.attach().map_err(|e| anyhow!("attach: {e:?}"))?)
            .map_err(|e| anyhow!("attach send: {e:?}"))?;
        let mut session = Self {
            client,
            peer,
            previous: Instant::now(),
        };
        while session.client.state() != State::Ready {
            session.pump(health)?;
        }
        Ok(session)
    }
    fn observe(&mut self, health: &mut Health, seconds: u64) -> Result<()> {
        let first = self.client.snapshot().unwrap().status.processed_frames;
        let end = Instant::now() + Duration::from_secs(seconds);
        while Instant::now() < end {
            self.pump(health)?;
        }
        ensure!(
            self.client.snapshot().unwrap().status.processed_frames > first,
            "FPGA frame progress stopped"
        );
        Ok(())
    }
    fn command(&mut self, health: &mut Health, command: Command) -> Result<Outcome> {
        let message = self
            .client
            .command(command, None)
            .map_err(|e| anyhow!("command: {e:?}"))?;
        let ClientMessage::Submit { operation, .. } = message else {
            bail!("expected command operation")
        };
        self.peer
            .send(message)
            .map_err(|e| anyhow!("command send: {e:?}"))?;
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            self.pump(health)?;
            if let Some(outcome) = self.client.results().get(&operation) {
                match outcome {
                    Outcome::Refused { .. } => bail!("command refused: {outcome:?}"),
                    _ => return Ok(outcome.clone()),
                }
            }
            ensure!(Instant::now() < until, "command timeout");
        }
    }
    fn prepare(&mut self, health: &mut Health, count: usize) -> Result<Vec<shoop_app_api::LoopId>> {
        let spec = TrackSpec {
            name: "restart qualification".into(),
            topology: TrackSpecTopology::Direct {
                audio_channels: 1,
                midi: false,
            },
            latency: Default::default(),
            initial_output_bus_names: vec!["Master".into()],
            creation_request_id: None,
        };
        let Outcome::TrackCreated {
            track, mut loops, ..
        } = self.command(health, Command::AddTrack(spec))?
        else {
            bail!("track not created")
        };
        while loops.len() < count {
            let Outcome::LoopCreated { loop_id, .. } =
                self.command(health, Command::AddLoop { track })?
            else {
                bail!("loop not created")
            };
            loops.push(loop_id);
        }
        Ok(loops.into_iter().take(count).collect())
    }
}
fn status(profile: &RemoteProfile) -> Result<BackendRestartStatus> {
    connection::backend_restart_status(profile, Duration::from_millis(800))?
        .context("backend restart unsupported")
}
fn restart(
    profile: &RemoteProfile,
    mut old: Session,
    health: &mut Health,
    sequence: u64,
) -> Result<Session> {
    let started = Instant::now();
    let before = status(profile)?;
    let authority = old.client.authority().context("old UI authority")?;
    let epoch = old.client.snapshot().unwrap().persistence_epoch;
    let operation = OperationId {
        client: authority.client,
        sequence,
    };
    let request = ManagementRequest::RestartBackend {
        version: VERSION,
        operation,
        expected_generation: before.generation,
    };
    let reply = connection::management(profile, request.clone(), Duration::from_millis(800))?;
    let ManagementReply::BackendRestart(accepted) = reply else {
        bail!("restart refused: {reply:?}")
    };
    ensure!(
        accepted.generation == before.generation + 1 && accepted.operation == Some(operation),
        "wrong acknowledgment generation/operation"
    );
    let duplicate = connection::management(profile, request, Duration::from_millis(800))?;
    ensure!(
        matches!(duplicate,ManagementReply::BackendRestart(s) if s.generation==accepted.generation && s.operation==Some(operation)),
        "duplicate was not idempotent"
    );
    let conflict = connection::management(
        profile,
        ManagementRequest::RestartBackend {
            version: VERSION,
            operation: OperationId {
                sequence: sequence + 100,
                ..operation
            },
            expected_generation: before.generation,
        },
        Duration::from_millis(800),
    )?;
    ensure!(
        matches!(
            conflict,
            ManagementReply::Refused(Refusal::Busy | Refusal::StaleAuthority)
        ),
        "conflicting operation accepted"
    );
    let mut previous = None;
    loop {
        health.deadline()?;
        let state = status(profile)?;
        ensure!(
            state.operation == Some(operation) && state.generation == accepted.generation,
            "restart superseded"
        );
        if previous != Some(state.phase) {
            println!(
                "{}",
                serde_json::json!({"event":"restart_progress","generation":state.generation,"phase":format!("{:?}",state.phase),"elapsed_ms":started.elapsed().as_millis()})
            );
            previous = Some(state.phase);
        }
        ensure!(
            state.phase != Phase::Failed,
            "restart failed: {:?}",
            state.failure
        );
        if state.phase == Phase::Ready {
            break;
        }
        let _ = old.peer.poll(&mut old.client);
        thread::sleep(Duration::from_millis(10));
    }
    let ready_ms = started.elapsed().as_millis();
    let mut fresh = Session::attach(profile, health)?;
    ensure!(
        fresh.client.authority().unwrap().lease != authority.lease,
        "UI lease reused"
    );
    ensure!(
        fresh.client.snapshot().unwrap().persistence_epoch > epoch,
        "persistence epoch reused"
    );
    ensure!(
        !fresh
            .client
            .snapshot()
            .unwrap()
            .tracks
            .iter()
            .any(|t| t.name == "restart qualification"),
        "old session survived destructive restart"
    );
    let recovery_ms = started.elapsed().as_millis();
    let retired = started + Duration::from_millis(2700);
    while old.peer.port.failure().is_none() {
        fresh.pump(health)?;
        let _ = old.peer.poll(&mut old.client);
        ensure!(
            Instant::now() < retired,
            "old data transport exceeded the broker's two-second close grace"
        );
    }
    let retirement_ms = started.elapsed().as_millis();
    drop(old);
    fresh.observe(health, 11)?;
    println!(
        "{}",
        serde_json::json!({"event":"restart_pass","generation":accepted.generation,"old_lease_fenced":true,"session_discarded":true,"initial_recovery_ms":recovery_ms,"ready_ms":ready_ms,"old_transport_retired_ms":retirement_ms})
    );
    Ok(fresh)
}
fn run(profile: &RemoteProfile, cycles: u64) -> Result<()> {
    let mut health = Health::new();
    let mut session = Session::attach(profile, &mut health)?;
    session.observe(&mut health, 3)?;
    for cycle in 0..cycles {
        let loops = session.prepare(&mut health, if cycle == 0 { 16 } else { 1 })?;
        let message = session
            .client
            .immediate(vec![immediate::Command::Transition(
                loops[0],
                TransitionMode::Recording.into(),
            )])
            .map_err(|e| anyhow!("record: {e:?}"))?;
        session
            .peer
            .send(message)
            .map_err(|e| anyhow!("record send: {e:?}"))?;
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            session.pump(&mut health)?;
            if session
                .client
                .snapshot()
                .unwrap()
                .tracks
                .iter()
                .flat_map(|t| &t.loops)
                .any(|l| l.id == loops[0] && l.mode == LoopMode::Recording)
            {
                break;
            }
            ensure!(
                Instant::now() < until,
                "single-loop recording did not start"
            );
        }
        session.observe(&mut health, 2)?;
        println!(
            "{}",
            serde_json::json!({"event":"healthy_restart_begin","cycle":cycle+1,"recording":true})
        );
        session = restart(profile, session, &mut health, 1000 + cycle)?;
    }
    let loops = session.prepare(&mut health, 16)?;
    let commands = loops
        .into_iter()
        .map(|id| {
            immediate::Command::Transition(
                id,
                TransitionPlan {
                    first: Some(PlannedTransition {
                        mode: TransitionMode::Recording,
                        skip: 1,
                    }),
                    ..Default::default()
                },
            )
        })
        .collect();
    session
        .peer
        .send(
            session
                .client
                .immediate(commands)
                .map_err(|e| anyhow!("bulk: {e:?}"))?,
        )
        .map_err(|e| anyhow!("bulk send: {e:?}"))?;
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        session.pump(&mut health)?;
        let reply = connection::management(
            profile,
            ManagementRequest::Status { version: VERSION },
            Duration::from_millis(800),
        )?;
        if matches!(reply, ManagementReply::Status { ready: false, .. }) {
            break;
        }
        ensure!(
            Instant::now() < end,
            "bulk arming did not reproduce failed owner"
        );
    }
    println!(
        "{}",
        serde_json::json!({"event":"failed_owner_management_responsive","ready":false})
    );
    session = restart(profile, session, &mut health, 2000)?;
    session.observe(&mut health, 3)?;
    drop(session);
    println!(
        "BACKEND_RESTART_PASS healthy_cycles={cycles} failed_owner_cycles=1 health_samples={}",
        health.samples
    );
    Ok(())
}
fn main() -> Result<()> {
    let path = std::env::args().nth(1).context("profile path required")?;
    let cycles = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "3".into())
        .parse::<u64>()?;
    ensure!((1..=8).contains(&cycles), "cycles must be 1..8");
    let profile = RemoteProfile::read(Path::new(&path))?;
    if let Err(error) = run(&profile, cycles) {
        eprintln!("BACKEND_RESTART_ABORT: {error:#}; requesting one safety device reboot");
        connection::reboot(&profile, Duration::from_secs(2)).context(
            "SAFETY REBOOT UNCONFIRMED: disconnect both power sources if hot or unresponsive",
        )?;
        return Err(error);
    }
    Ok(())
}
