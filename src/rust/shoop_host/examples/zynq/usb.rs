use super::{command, fixture, pump};
use fixture::{action, confirmed};
use shoop_app_api::*;
use shoop_app_client::Client;
use shoop_app_protocol::{persistence_image::Payload, Command, Outcome};
use shoop_host::connection::Connection;
use std::{
    ffi::OsString,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

pub(super) struct Peer {
    stream: UnixStream,
    bytes: Vec<u8>,
    cables: u8,
}
impl Peer {
    fn read(&mut self) {
        let mut buffer = [0; 4096];
        loop {
            match self.stream.read(&mut buffer) {
                Ok(0) => panic!("USB fixture closed"),
                Ok(n) => self.bytes.extend_from_slice(&buffer[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("USB fixture read: {e}"),
            }
        }
    }
    pub(super) fn finish(&mut self) {
        if std::env::var_os("SHOOP_ZYNQ_USB_FAULT").is_some() {
            return;
        }
        self.read();
        assert_eq!(self.bytes.len() % 4, 0, "partial USB packet");
        for cable in 0..self.cables {
            let resets: Vec<_> = self
                .bytes
                .chunks_exact(4)
                .filter(|p| {
                    p[0] == (cable << 4 | 11)
                        && p[1] & 0xf0 == 0xb0
                        && [120, 123].contains(&p[2])
                        && p[3] == 0
                })
                .collect();
            assert!(
                resets.len() >= 32,
                "cable {cable}: incomplete shutdown reset"
            );
            for (i, packet) in resets[resets.len() - 32..].iter().enumerate() {
                assert_eq!(
                    **packet,
                    [
                        cable << 4 | 11,
                        0xb0 | (i as u8 / 2),
                        if i % 2 == 0 { 120 } else { 123 },
                        0
                    ]
                );
            }
        }
        println!("Actual USB OUT completed all 32 shutdown resets per cable");
    }
}

fn packets(message: &[u8], cable: u8) -> Vec<u8> {
    let mut result = Vec::new();
    for (index, bytes) in message.chunks(3).enumerate() {
        let cin = if message[0] == 0xf0 {
            if (index + 1) * 3 < message.len() {
                4
            } else {
                4 + bytes.len() as u8
            }
        } else if message[0] < 0xf0 {
            message[0] >> 4
        } else if message[0] >= 0xf8 {
            15
        } else if message[0] == 0xf6 {
            5
        } else {
            bytes.len() as u8
        };
        result.push(cable << 4 | cin);
        result.extend_from_slice(bytes);
        result.resize(result.len() + 3 - bytes.len(), 0);
    }
    result
}

fn wait(
    client: &mut Client,
    connection: &mut Connection,
    mut predicate: impl FnMut(&Client) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !predicate(client) {
        assert!(Instant::now() < deadline, "USB application state timed out");
        pump(client, connection);
    }
}

fn topology(command: &str) {
    let socket = UnixStream::connect(std::env::var_os("SHOOP_ZYNQ_USB_CONTROL").unwrap()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let mut stream = BufReader::new(socket);
    writeln!(stream.get_mut(), "{command}").unwrap();
    let mut response = String::new();
    stream.read_line(&mut response).unwrap();
    assert_eq!(
        response.trim(),
        "ok",
        "USB topology command {command}: {response}"
    );
}

pub(super) fn run(client: &mut Client, connection: &mut Connection, path: OsString) -> Peer {
    let cables: u8 = std::env::var("SHOOP_ZYNQ_USB_CABLES")
        .unwrap()
        .parse()
        .unwrap();
    let cable = cables - 1;
    let devices: usize = std::env::var("SHOOP_ZYNQ_USB_DEVICES")
        .unwrap()
        .parse()
        .unwrap();
    let stream = UnixStream::connect(path).unwrap();
    stream.set_nonblocking(true).unwrap();
    let mut peer = Peer {
        stream,
        bytes: Vec::new(),
        cables,
    };
    wait(client, connection, |c| {
        c.snapshot()
            .unwrap()
            .connections
            .host_ports
            .iter()
            .filter(|p| p.data_type == PortDataType::Midi && p.available)
            .count()
            == 2 * cables as usize * devices
    });
    let inventory = client.snapshot().unwrap().connections.host_ports.clone();
    let input = inventory
        .iter()
        .find(|p| p.id.as_str().ends_with(&format!("/in/{cable}")))
        .unwrap()
        .id
        .clone();
    let output = HostPortId::new(input.as_str().replace("/in/", "/out/"));
    assert!(
        input.as_str().contains(":s"),
        "serial identity was not retained"
    );
    confirmed(command(
        client,
        connection,
        Command::Global(GlobalControlAction::SetSync(false)),
    ));
    confirmed(command(client, connection, Command::SetLoopSmoothingMs(0)));
    let Outcome::TrackCreated { track, loops, .. } = command(
        client,
        connection,
        Command::AddTrack(TrackSpec {
            name: "USB exact MIDI".into(),
            topology: TrackSpecTopology::Direct {
                audio_channels: 0,
                midi: true,
            },
            latency: Default::default(),
            initial_output_bus_names: vec![],
            creation_request_id: None,
        }),
    ) else {
        panic!("MIDI track creation")
    };
    for _ in 0..20 {
        pump(client, connection);
    }
    let ports = client
        .snapshot()
        .unwrap()
        .connections
        .application_ports
        .clone();
    let links = client
        .snapshot()
        .unwrap()
        .connections
        .confirmed_links
        .clone();
    for link in links.iter().filter(|link| {
        ports.iter().any(|port| port.id == link.application_port_id &&
        matches!(port.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == track))
    }) {
        confirmed(command(
            client,
            connection,
            Command::SetPortConnected {
                port: link.application_port_id,
                host_port: link.host_port_id.clone(),
                connected: false,
            },
        ));
    }
    for port in ports.iter().filter(
        |p| matches!(p.owner, ApplicationPortOwner::Track {track_id, ..} if track_id == track),
    ) {
        confirmed(command(
            client,
            connection,
            Command::SetPortConnected {
                port: port.id,
                host_port: if port.direction == PortDirection::Input {
                    input.clone()
                } else {
                    output.clone()
                },
                connected: true,
            },
        ));
    }
    let loop_id = loops[0];
    if let Ok(fault) = std::env::var("SHOOP_ZYNQ_USB_FAULT") {
        confirmed(command(
            client,
            connection,
            Command::Track {
                track,
                action: TrackAction::InputMonitoringChanged {
                    enabled: true,
                    respect_auto_mute: false,
                },
            },
        ));
        peer.stream
            .write_all(&packets(&[0x90, 60, 100], cable))
            .unwrap();
        if fault == "stall" {
            wait(client, connection, |c| {
                c.snapshot()
                    .unwrap()
                    .connections
                    .host_ports
                    .iter()
                    .any(|p| p.id == output && !p.available)
            });
        }
        let before = client.snapshot().unwrap().status.processed_frames;
        wait(client, connection, |c| {
            c.snapshot().unwrap().status.processed_frames > before + 1024
        });
        confirmed(command(client, connection, Command::SetLoopSmoothingMs(1)));
        println!("Injected USB {fault} preserved rendering and live control");
        return peer;
    }
    action(
        client,
        connection,
        track,
        loop_id,
        LoopAction::RecordClicked,
    );
    wait(client, connection, |c| {
        c.snapshot()
            .unwrap()
            .tracks
            .iter()
            .find(|t| t.id == track)
            .unwrap()
            .loops
            .iter()
            .find(|l| l.id == loop_id)
            .unwrap()
            .mode
            == LoopMode::Recording
    });
    let mut sysex = vec![0xf0];
    sysex.extend((1..2047).map(|i| (i % 128) as u8));
    sysex.push(0xf7);
    let mut messages = vec![
        vec![0x90, 60, 100],
        vec![0xa2, 60, 42],
        vec![0xb3, 74, 99],
        vec![0xc4, 12],
        vec![0xd5, 33],
        vec![0xe6, 0, 64],
        vec![0xf1, 7],
        vec![0xf2, 3, 4],
        vec![0xf3, 5],
        vec![0xf6],
        vec![0xf8],
        vec![0xfa],
        vec![0xfb],
        vec![0xfc],
        vec![0xfe],
        vec![0xff],
        sysex,
        vec![0x80, 60, 0],
    ];
    if cable > 0 {
        peer.stream
            .write_all(&packets(&[0x91, 99, 123], 0))
            .unwrap();
    }
    for message in &messages {
        let bytes = packets(message, cable);
        if message[0] == 0xf0 {
            peer.stream.write_all(&bytes[..128]).unwrap();
            peer.stream.write_all(&packets(&[0xf8], cable)).unwrap();
            peer.stream.write_all(&bytes[128..]).unwrap();
        } else {
            peer.stream.write_all(&bytes).unwrap();
        }
        for _ in 0..12 {
            pump(client, connection);
        }
    }
    messages.insert(messages.len() - 2, vec![0xf8]);
    let expected_count = messages.len();
    let consumption =
        std::thread::spawn(move || topology(&format!("consumed {cable} {expected_count}")));
    let deadline = Instant::now() + Duration::from_secs(30);
    while !consumption.is_finished() {
        assert!(
            Instant::now() < deadline,
            "USB consumption observation timed out"
        );
        pump(client, connection);
    }
    consumption.join().expect("USB consumption observation");
    action(client, connection, track, loop_id, LoopAction::StopClicked);
    wait(client, connection, |c| {
        c.snapshot()
            .unwrap()
            .tracks
            .iter()
            .find(|t| t.id == track)
            .unwrap()
            .loops
            .iter()
            .find(|l| l.id == loop_id)
            .unwrap()
            .mode
            == LoopMode::Stopped
    });
    for _ in 0..20 {
        pump(client, connection);
    }
    let image = fixture::capture(client, connection, 900);
    if let shoop_app_protocol::persistence_image::Manifest::Session(session) = &image.manifest {
        println!(
            "Recorded loop frames: {:?}",
            session
                .tracks
                .iter()
                .map(|track| track
                    .loops
                    .iter()
                    .map(|entry| entry.frames)
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>()
        );
    }
    let actual: Vec<_> = image
        .payloads
        .iter()
        .filter_map(|p| {
            if let Payload::Midi(m) = p {
                Some(m)
            } else {
                None
            }
        })
        .flat_map(|m| m.iter().map(|event| event.data.to_vec()))
        .collect();
    let mut recorded_expected = messages.clone();
    if std::env::var_os("SHOOP_ZYNQ_USB_CORRUPT").is_some() {
        recorded_expected[0][1] ^= 1;
    }
    assert_eq!(
        actual, recorded_expected,
        "actual USB IN recording changed payload, order or cable isolation"
    );
    println!("Actual USB IN recorded exact channel/system/realtime and 2048-byte SysEx messages");
    peer.read();
    peer.bytes.clear();
    let expected: Vec<_> = messages.iter().flat_map(|m| packets(m, cable)).collect();
    action(client, connection, track, loop_id, LoopAction::PlayClicked);
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        peer.read();
        if peer.bytes.windows(expected.len()).any(|w| w == expected) {
            break;
        }
        if Instant::now() >= deadline {
            let directory =
                std::path::PathBuf::from(std::env::var_os("SHOOP_ZYNQ_USB_SOCKET").unwrap());
            std::fs::write(directory.with_file_name("actual-out.bin"), &peer.bytes).unwrap();
            std::fs::write(directory.with_file_name("expected-out.bin"), &expected).unwrap();
        }
        assert!(
            Instant::now() < deadline,
            "USB OUT playback mismatch: {} observed bytes, expected {}",
            peer.bytes.len(),
            expected.len()
        );
        pump(client, connection);
    }
    action(client, connection, track, loop_id, LoopAction::StopClicked);
    println!("Actual USB OUT played exact recorded MIDI through the firmware engine");
    confirmed(command(
        client,
        connection,
        Command::Track {
            track,
            action: TrackAction::InputMonitoringChanged {
                enabled: true,
                respect_auto_mute: false,
            },
        },
    ));
    let cycles: usize = std::env::var("SHOOP_ZYNQ_USB_CYCLES")
        .unwrap()
        .parse()
        .unwrap();
    topology("drained");
    for cycle in 0..cycles {
        topology("detach");
        wait(client, connection, |c| {
            c.snapshot()
                .unwrap()
                .connections
                .host_ports
                .iter()
                .any(|p| p.id == input && !p.available)
        });
        assert!(client
            .snapshot()
            .unwrap()
            .connections
            .host_ports
            .iter()
            .any(|p| p.id == output && !p.available));
        topology("attach");
        wait(client, connection, |c| {
            c.snapshot()
                .unwrap()
                .connections
                .host_ports
                .iter()
                .any(|p| p.id == input && p.available)
        });
        assert!(client
            .snapshot()
            .unwrap()
            .connections
            .host_ports
            .iter()
            .any(|p| p.id == output && p.available));
        assert_eq!(
            client
                .snapshot()
                .unwrap()
                .connections
                .host_ports
                .iter()
                .filter(|p| p.data_type == PortDataType::Midi)
                .count(),
            2 * cables as usize * devices
        );
        peer.read();
        peer.bytes.clear();
        let note = (cycle % 128) as u8;
        let expected = [
            packets(&[0x90, note, 100], cable),
            packets(&[0x80, note, 0], cable),
        ]
        .concat();
        peer.stream.write_all(&expected).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            peer.read();
            if peer
                .bytes
                .windows(expected.len())
                .any(|bytes| bytes == expected)
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "bidirectional MIDI stopped after hotplug cycle {cycle}"
            );
            pump(client, connection);
        }
        topology("drained");
    }
    println!("Hotplug preserved physical IDs, offline bindings and live control");
    if cables == 1 && devices == 1 {
        let application_port = |direction| {
            ports.iter().find(|port| port.direction == direction &&
            matches!(port.owner, ApplicationPortOwner::Track {track_id, ..} if track_id == track)).unwrap().id
        };
        let in_port = application_port(PortDirection::Input);
        let out_port = application_port(PortDirection::Output);
        peer.read();
        peer.bytes.clear();
        for (port, host_port) in [(out_port, output.clone()), (in_port, input.clone())] {
            confirmed(command(
                client,
                connection,
                Command::SetPortConnected {
                    port,
                    host_port,
                    connected: false,
                },
            ));
        }
        topology("drained");
        topology("detach");
        wait(client, connection, |client| {
            !client
                .snapshot()
                .unwrap()
                .connections
                .host_ports
                .iter()
                .any(|port| port.id == input || port.id == output)
        });
        assert!(
            matches!(
                command(
                    client,
                    connection,
                    Command::SetPortConnected {
                        port: in_port,
                        host_port: input.clone(),
                        connected: true,
                    }
                ),
                Outcome::Refused { .. }
            ),
            "retired identity accepted a late connection"
        );
        topology("replace");
        wait(client, connection, |client| {
            client
                .snapshot()
                .unwrap()
                .connections
                .host_ports
                .iter()
                .any(|port| port.data_type == PortDataType::Midi && port.available)
        });
        let ports = &client.snapshot().unwrap().connections.host_ports;
        let replacement_input = ports
            .iter()
            .find(|port| {
                port.data_type == PortDataType::Midi
                    && port.id.as_str().ends_with("/in/0")
                    && port.available
            })
            .unwrap()
            .id
            .clone();
        let replacement_output = ports
            .iter()
            .find(|port| {
                port.data_type == PortDataType::Midi
                    && port.id.as_str().ends_with("/out/0")
                    && port.available
            })
            .unwrap()
            .id
            .clone();
        assert_ne!(replacement_input, input);
        assert_ne!(replacement_output, output);
        peer.read();
        peer.bytes.clear();
        let unrouted = [packets(&[0x90, 96, 100], 0), packets(&[0x80, 96, 0], 0)].concat();
        peer.stream.write_all(&unrouted).unwrap();
        let count = expected_count + 2 * cycles + 2;
        let consumption = std::thread::spawn(move || topology(&format!("consumed 0 {count}")));
        let deadline = Instant::now() + Duration::from_secs(30);
        while !consumption.is_finished() {
            assert!(
                Instant::now() < deadline,
                "replacement input was not consumed"
            );
            pump(client, connection);
        }
        consumption.join().unwrap();
        for _ in 0..20 {
            pump(client, connection);
        }
        peer.read();
        assert!(
            !peer
                .bytes
                .windows(unrouted.len())
                .any(|bytes| bytes == unrouted),
            "an old route delivered MIDI to a replacement device"
        );
        for (port, host_port) in [(in_port, replacement_input), (out_port, replacement_output)] {
            confirmed(command(
                client,
                connection,
                Command::SetPortConnected {
                    port,
                    host_port,
                    connected: true,
                },
            ));
        }
        let selected = [packets(&[0x90, 97, 105], 0), packets(&[0x80, 97, 0], 0)].concat();
        peer.stream.write_all(&selected).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            peer.read();
            if peer
                .bytes
                .windows(selected.len())
                .any(|bytes| bytes == selected)
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "selected replacement did not exchange MIDI"
            );
            pump(client, connection);
        }
        topology("drained");
        println!(
            "Retired identity stayed isolated; replacement exchanged MIDI after explicit selection"
        );
    }
    peer
}
