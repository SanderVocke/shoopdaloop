use super::{attach, command, finish, profiling, pump, Client, Connection};
use shoop_app_api::{
    ApplicationPortOwner, BackendType, LoopId, LoopMode, PortDataType, PortDirection, PortRole,
    TrackId, TrackSpec, TrackSpecTopology, TransitionMode,
};
use shoop_app_protocol::{immediate, Command, Outcome};
use shoop_host::{connection, remote::RemoteProfile};
use std::time::{Duration, Instant};

pub fn run(profile: &RemoteProfile) {
    let (mut client, mut peer) = attach(profile);
    let report = connection::host_report(profile, Duration::from_secs(5)).expect("host settings");
    assert_eq!(report.requested.unwrap().backend, BackendType::RtlSim);
    let spec = TrackSpec {
        name: "Z-turn mono".into(),
        topology: TrackSpecTopology::Direct {
            audio_channels: 1,
            midi: false,
        },
        latency: Default::default(),
        initial_output_bus_names: vec!["Master".into()],
        creation_request_id: None,
    };
    let Outcome::TrackCreated { track, loops, .. } =
        command(&mut client, &mut peer, Command::AddTrack(spec.clone()))
    else {
        panic!("FPGA track creation failed");
    };
    let voice = loops[0];
    for (transition, observed, gain) in [
        (TransitionMode::Recording, LoopMode::Recording, 0.375),
        (TransitionMode::Stopped, LoopMode::Stopped, 0.5),
    ] {
        peer.send(
            client
                .immediate(vec![
                    immediate::Command::LoopGain(voice, gain),
                    immediate::Command::Transition(voice, transition.into()),
                ])
                .expect("loop control"),
        )
        .expect("send loop control");
        let until = Instant::now() + Duration::from_secs(5);
        while !client
            .snapshot()
            .unwrap()
            .tracks
            .iter()
            .flat_map(|t| &t.loops)
            .any(|row| row.id == voice && row.mode == observed && row.gain == gain)
        {
            assert!(
                Instant::now() < until,
                "loop state did not reach {observed:?} / {gain}"
            );
            pump(&mut client, &mut peer);
        }
    }
    assert!(matches!(
        command(&mut client, &mut peer, Command::AddLoop { track }),
        Outcome::LoopCreated { .. }
    ));
    let mut unsupported = spec;
    unsupported.topology = TrackSpecTopology::Direct {
        audio_channels: 1,
        midi: true,
    };
    assert!(matches!(
        command(&mut client, &mut peer, Command::AddTrack(unsupported)),
        Outcome::Refused { .. }
    ));
    assert!(client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .flat_map(|t| &t.loops)
        .any(|row| row.id == voice && row.mode == LoopMode::Stopped && row.gain == 0.5));
    profiling::run(&mut client, &mut peer, "fpga-owner");
    if let Ok(seconds) = std::env::var("SHOOP_ZYNQ_FPGA_BENCH_SECONDS") {
        let duration = Duration::from_secs(seconds.parse().expect("benchmark duration in seconds"));
        benchmark(profile, &mut client, &mut peer, track, duration);
    }
    drop(peer);
    let (mut client, mut peer) = attach(profile);
    let request = client.shutdown().expect("shutdown request");
    assert!(matches!(
        finish(&mut client, &mut peer, request),
        Outcome::Confirmed { .. }
    ));
    println!("FPGA endpoint passed authenticated graph activation, live controls, revision replacement, refusal, reconnect and shutdown");
}

fn benchmark(
    profile: &RemoteProfile,
    client: &mut Client,
    peer: &mut Connection,
    track: TrackId,
    duration: Duration,
) {
    assert!(duration >= Duration::from_secs(10));
    let snapshot = client.snapshot().unwrap();
    let input = snapshot
        .connections
        .application_ports
        .iter()
        .find(|port| {
            matches!(port.owner, ApplicationPortOwner::Track {track_id, ..} if track_id == track)
                && port.role == PortRole::AudioInput
        })
        .expect("mono track input")
        .id;
    let source = snapshot
        .connections
        .host_ports
        .iter()
        .find(|port| {
            port.data_type == PortDataType::Audio && port.direction == PortDirection::Output
        })
        .expect("codec input")
        .id
        .clone();
    assert!(matches!(
        command(
            client,
            peer,
            Command::SetPortConnected {
                port: input,
                host_port: source,
                connected: true,
            }
        ),
        Outcome::Confirmed { .. }
    ));
    let mut loops: Vec<_> = client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .find(|candidate| candidate.id == track)
        .unwrap()
        .loops
        .iter()
        .map(|row| row.id)
        .collect();
    while loops.len() < 64 {
        let Outcome::LoopCreated { loop_id, .. } =
            command(client, peer, Command::AddLoop { track })
        else {
            panic!("FPGA benchmark could not create loop {}", loops.len() + 1);
        };
        loops.push(loop_id);
    }
    let set_mode = |client: &Client, peer: &mut Connection, mode: TransitionMode| {
        for batch in loops.chunks(8) {
            let transitions = batch
                .iter()
                .map(|&id| immediate::Command::Transition(id, mode.into()))
                .collect();
            peer.send(
                client
                    .immediate(transitions)
                    .expect("benchmark transitions"),
            )
            .expect("send transition");
        }
    };
    let wait_mode = |client: &mut Client, peer: &mut Connection, mode: LoopMode| {
        wait_visible(client, peer, |client| {
            client
                .snapshot()
                .unwrap()
                .tracks
                .iter()
                .flat_map(|t| &t.loops)
                .filter(|row| loops.contains(&row.id) && row.mode == mode)
                .count()
                == loops.len()
        });
    };
    set_mode(client, peer, TransitionMode::Recording);
    wait_mode(client, peer, LoopMode::Recording);
    let started = Instant::now();
    let until = started + duration;
    let initial_frames = client.snapshot().unwrap().status.processed_frames;
    while started.elapsed() < Duration::from_secs(1) {
        pump(client, peer);
    }
    set_mode(client, peer, TransitionMode::Playing);
    wait_mode(client, peer, LoopMode::Playing);
    let before_switch = Instant::now();
    assert!(matches!(
        command(client, peer, Command::AddLoop { track }),
        Outcome::LoopCreated { .. }
    ));
    let switch_us = before_switch.elapsed().as_micros();
    profiling::run(client, peer, "fpga-owner");
    let mut worst_us = 0f64;
    let mut max_control_us = 0u128;
    let mut management_requests = 0;
    let mut next_management = Instant::now();
    let mut next_edit = next_management;
    let mut gain = 0.25;
    while Instant::now() < until {
        let now = Instant::now();
        if now >= next_management {
            connection::host_report(profile, Duration::from_secs(5))
                .expect("concurrent management traffic");
            management_requests += 1;
            next_management = now + Duration::from_millis(100);
        }
        if now >= next_edit {
            let id = loops[0];
            let submitted = Instant::now();
            peer.send(
                client
                    .immediate(vec![immediate::Command::LoopGain(id, gain)])
                    .expect("benchmark gain"),
            )
            .expect("send gain");
            wait_visible(client, peer, |client| observed_gain(client, id, gain));
            max_control_us = max_control_us.max(submitted.elapsed().as_micros());
            gain = if gain == 0.25 { 0.5 } else { 0.25 };
            next_edit = now + Duration::from_secs(5);
        }
        pump(client, peer);
        let status = &client.snapshot().unwrap().status;
        worst_us = worst_us.max(
            status
                .dsp
                .and_then(|sample| sample.microseconds())
                .unwrap_or(0.),
        );
    }
    let status = &client.snapshot().unwrap().status;
    println!(
        "{}",
        serde_json::json!({
            "active_media_loops": 64,
            "seconds": started.elapsed().as_secs_f64(),
            "frames_processed": status.processed_frames - initial_frames,
            "quantum_frames": status.quantum_frames,
            "sample_rate": status.sample_rate,
            "xruns": status.xruns,
            "peak_quantum_us": worst_us,
            "max_gain_confirmation_us": max_control_us,
            "live_graph_confirmation_us": switch_us,
            "competing_management_requests": management_requests,
        })
    );
    assert_eq!(status.sample_rate, 48_000);
    assert_eq!(status.quantum_frames, 32);
    assert_eq!(status.xruns, 0);
    assert!((0. ..=1_000_000. / 1_500.).contains(&worst_us) && worst_us > 0.);
    assert!(status.processed_frames - initial_frames >= duration.as_secs() * 47_000);
}

fn observed_gain(client: &Client, id: LoopId, gain: f32) -> bool {
    client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .flat_map(|t| &t.loops)
        .any(|row| row.id == id && row.gain == gain)
}

fn wait_visible(
    client: &mut Client,
    peer: &mut Connection,
    mut visible: impl FnMut(&Client) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !visible(client) {
        assert!(
            Instant::now() < deadline,
            "FPGA benchmark state did not update"
        );
        pump(client, peer);
    }
}
