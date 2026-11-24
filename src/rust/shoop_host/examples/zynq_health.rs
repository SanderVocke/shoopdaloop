//! Supervised health check; attempts shutdown unless a successful `--handoff` leaves the host running.
#[cfg(unix)]
mod probe {
    use anyhow::{anyhow, bail, Context, Result};
    use shoop_app_api::host_health;
    use shoop_app_client::{Client, State};
    use shoop_app_protocol::{ClientMessage, Outcome};
    use shoop_host::{
        connection::{self, Connection},
        remote::RemoteProfile,
    };
    use std::{
        path::Path,
        thread,
        time::{Duration, Instant},
    };

    fn pump(
        client: &mut Client,
        connection: &mut Connection,
        previous: &mut Instant,
    ) -> Result<()> {
        let now = Instant::now();
        client.advance_diagnostics(now.duration_since(*previous));
        *previous = now;
        connection
            .poll(client)
            .map_err(|error| anyhow!("poll: {error:?}"))?;
        if let Some(message) = client.ready_message() {
            connection
                .send(message)
                .map_err(|error| anyhow!("Ready: {error:?}"))?;
            client
                .ready_sent()
                .map_err(|error| anyhow!("Ready state: {error:?}"))?;
        }
        thread::sleep(Duration::from_millis(5));
        Ok(())
    }

    fn observe(
        client: &mut Client,
        connection: &mut Connection,
        previous: &mut Instant,
        seconds: u64,
    ) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while client.state() != State::Ready {
            if Instant::now() >= deadline {
                bail!("attachment timed out");
            }
            pump(client, connection, previous)?;
        }
        println!("ATTACHED health-only run; cutoff 65 C; duration {seconds} s");
        let started = Instant::now();
        let mut last_reading = started;
        let mut last_print = started - Duration::from_secs(1);
        let mut samples = 0;
        let first_frames = client.snapshot().map(|state| state.status.processed_frames);
        while started.elapsed() < Duration::from_secs(seconds) {
            pump(client, connection, previous)?;
            if let Some(source) = client
                .diagnostics()
                .sources
                .iter()
                .find(|source| source.descriptor.stream.source == host_health::SOURCE)
            {
                if !source.supported || !source.descriptor.available {
                    bail!("health source unavailable");
                }
                if source.age_ms == 0 && last_print.elapsed() >= Duration::from_millis(500) {
                    if let Some(sample) = client.diagnostics().health {
                        let sensors: Vec<_> = sample.temperatures.iter().flatten().collect();
                        if sensors.is_empty() {
                            bail!("no temperature sensor reported");
                        }
                        for sensor in sensors {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "elapsed_ms": started.elapsed().as_millis(),
                                    "sensor": sensor.name.as_str(),
                                    "current_c": sensor.current_millicelsius.map(|value| f64::from(value) / 1000.),
                                    "sensor_max_c": sensor.peak_millicelsius.map(|value| f64::from(value) / 1000.),
                                    "overtemperature": sample.overtemperature,
                                    "processed_frames": client.snapshot().map(|state| state.status.processed_frames),
                                    "xruns": client.snapshot().map(|state| state.status.xruns),
                                })
                            );
                            let current = sensor
                                .current_millicelsius
                                .ok_or_else(|| anyhow!("temperature read failed"))?;
                            if current >= 65000 || sample.overtemperature == Some(true) {
                                bail!("temperature/alarm stop threshold reached");
                            }
                        }
                        samples += 1;
                        last_reading = Instant::now();
                        last_print = last_reading;
                    }
                }
            }
            if last_reading.elapsed() > Duration::from_secs(3) {
                bail!("no fresh temperature for three seconds");
            }
        }
        if samples < 2 {
            bail!("too few temperature samples");
        }
        let last_frames = client.snapshot().map(|state| state.status.processed_frames);
        if !matches!((first_frames, last_frames), (Some(first), Some(last)) if last > first) {
            bail!("rendering did not advance during the health check");
        }
        println!("HEALTH_PASS samples={samples} frames={first_frames:?}->{last_frames:?}");
        Ok(())
    }

    fn shutdown(
        client: &mut Client,
        connection: &mut Connection,
        previous: &mut Instant,
    ) -> Result<()> {
        let message = client
            .shutdown()
            .map_err(|error| anyhow!("shutdown request: {error:?}"))?;
        let ClientMessage::Shutdown { operation, .. } = message else {
            unreachable!()
        };
        connection
            .send(message)
            .map_err(|error| anyhow!("shutdown send: {error:?}"))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            pump(client, connection, previous)?;
            if let Some(outcome) = client.results().get(&operation) {
                if matches!(outcome, Outcome::Confirmed { .. }) {
                    println!("SHUTDOWN_CONFIRMED");
                    return Ok(());
                }
                bail!("shutdown refused: {outcome:?}");
            }
        }
        bail!("shutdown confirmation timed out")
    }

    pub fn run() -> Result<()> {
        let path = std::env::args()
            .nth(1)
            .context("remote profile path required")?;
        let seconds = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "15".into())
            .parse::<u64>()?;
        if !(3..=30).contains(&seconds) {
            bail!("duration must be 3–30 seconds");
        }
        let handoff = match std::env::args().nth(3).as_deref() {
            None => false,
            Some("--handoff") => true,
            Some(_) => bail!("expected optional --handoff after duration"),
        };
        let profile = RemoteProfile::read(Path::new(&path))?;
        let mut connection = connection::connect(&profile, Duration::from_secs(5))?;
        let mut client = Client::new();
        connection
            .send(
                client
                    .attach()
                    .map_err(|error| anyhow!("attach: {error:?}"))?,
            )
            .map_err(|error| anyhow!("attach send: {error:?}"))?;
        let mut previous = Instant::now();
        let observed = observe(&mut client, &mut connection, &mut previous, seconds);
        if let Err(error) = &observed {
            eprintln!("HEALTH_ABORT: {error:#}; stopping host");
        }
        if handoff && observed.is_ok() {
            drop(connection);
            println!("HANDOFF_READY host left running; thermal cutoff no longer enforced");
            return Ok(());
        }
        let stopped = shutdown(&mut client, &mut connection, &mut previous);
        drop(connection);
        if let Err(error) = stopped {
            eprintln!("Data shutdown failed: {error:#}; trying authenticated ownerless management");
            thread::sleep(Duration::from_millis(250));
            connection::shutdown_ownerless(&profile, Duration::from_secs(5)).context(
                "SHUTDOWN UNCONFIRMED: disconnect board power if it is hot or unresponsive",
            )?;
            println!("MANAGEMENT_SHUTDOWN_CONFIRMED");
        }
        observed
    }
}

fn main() {
    #[cfg(unix)]
    if let Err(error) = probe::run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
    #[cfg(not(unix))]
    panic!("The Zynq health probe requires a Unix host");
}
