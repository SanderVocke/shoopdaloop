use super::{attach, command, finish, pump};
use shoop_app_api::{BackendType, LoopMode, TrackSpec, TrackSpecTopology, TransitionMode};
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
    drop(peer);
    let (mut client, mut peer) = attach(profile);
    let request = client.shutdown().expect("shutdown request");
    assert!(matches!(
        finish(&mut client, &mut peer, request),
        Outcome::Confirmed { .. }
    ));
    println!("FPGA endpoint passed authenticated graph activation, live controls, revision replacement, refusal, reconnect and shutdown");
}
