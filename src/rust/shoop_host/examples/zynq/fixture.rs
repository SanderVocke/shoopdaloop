use super::{command, pump};
use shoop_app_api::*;
use shoop_app_client::Client;
use shoop_app_protocol::{persistence::*, persistence_image::*, *};
use shoop_host::connection::Connection;
use shoop_transport::RecordPort;
use std::{
    cell::{Cell, RefCell},
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    sync::Arc,
    time::{Duration, Instant},
};

struct Driver(BufReader<UnixStream>);
impl Driver {
    fn call(&mut self, command: &str) -> [u32; 11] {
        writeln!(self.0.get_mut(), "{command}").unwrap();
        let mut line = String::new();
        self.0.read_line(&mut line).expect("driver reply");
        let fields: Vec<_> = line.split_whitespace().collect();
        assert_eq!(fields.len(), 12, "{line:?}");
        assert_eq!(fields[0], "D");
        std::array::from_fn(|i| fields[i + 1].parse().unwrap())
    }
}
thread_local! { static DRIVER: RefCell<Option<Driver>> = const { RefCell::new(None) }; }
thread_local! {
    static CURRENT_KEY: Cell<Option<Key>> = const { Cell::new(None) };
    static RETIRED_KEY: Cell<Option<Key>> = const { Cell::new(None) };
}
pub fn advance() {
    DRIVER.with_borrow_mut(|driver| {
        if let Some(driver) = driver {
            driver.call("S 256");
        }
    });
}
fn drive(command: &str) -> [u32; 11] {
    DRIVER.with_borrow_mut(|driver| driver.as_mut().unwrap().call(command))
}
fn request(client: &Client, connection: &mut Connection, request: Request) {
    CURRENT_KEY.set(Some(request.key()));
    connection
        .send(client.persistence(request).unwrap())
        .expect("persistence request");
}
fn received(client: &mut Client, connection: &mut Connection) -> Reply {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        if let Some(reply) = client.take_persistence_reply() {
            if Some(reply.key()) != CURRENT_KEY.get() {
                assert_eq!(
                    Some(reply.key()),
                    RETIRED_KEY.get(),
                    "unexpected persistence transfer"
                );
                continue;
            }
            return reply;
        }
        assert!(Instant::now() < deadline, "persistence reply timeout");
        pump(client, connection);
    }
}
fn reply(client: &mut Client, connection: &mut Connection) -> Reply {
    let reply = received(client, connection);
    assert!(
        !matches!(reply, Reply::Refused { .. }),
        "persistence refused: {reply:?}"
    );
    reply
}
fn key(client: &Client, transfer: u64) -> Key {
    Key {
        transfer: TransferId(transfer),
        epoch: client.snapshot().unwrap().persistence_epoch,
    }
}
fn target(client: &Client) -> Target {
    Target::Session {
        revision: client.snapshot().unwrap().revision,
    }
}
fn release(client: &mut Client, connection: &mut Connection, key: Key) {
    request(client, connection, Request::Cancel { key });
    assert!(matches!(reply(client, connection), Reply::Closed { .. }));
}
fn capture(client: &mut Client, connection: &mut Connection, transfer: u64) -> Arc<Image> {
    let key = key(client, transfer);
    request(
        client,
        connection,
        Request::Capture {
            key,
            target: target(client),
        },
    );
    let Reply::Captured(description) = reply(client, connection) else {
        panic!("capture expected")
    };
    confirmed(command(client, connection, Command::SetLoopSmoothingMs(0)));
    let mut objects = Vec::new();
    for object in 0..description.objects {
        request(client, connection, Request::Describe { key, object });
        let Reply::Object {
            object: description,
            ..
        } = reply(client, connection)
        else {
            panic!("object expected")
        };
        let mut bytes = Vec::with_capacity(description.bytes as usize);
        let mut sent = 0;
        let mut outstanding = 0;
        while bytes.len() < description.bytes as usize {
            while outstanding < 4 && sent < description.bytes {
                request(
                    client,
                    connection,
                    Request::Read {
                        key,
                        object,
                        offset: sent,
                    },
                );
                sent = (sent + CHUNK_BYTES as u64).min(description.bytes);
                outstanding += 1;
            }
            let Reply::Chunk {
                offset,
                bytes: chunk,
                ..
            } = reply(client, connection)
            else {
                panic!("chunk expected")
            };
            assert_eq!(offset, bytes.len() as u64);
            bytes.extend_from_slice(&chunk);
            outstanding -= 1;
        }
        objects.push(Object { description, bytes });
    }
    release(client, connection, key);
    persistence_transfer::assemble(objects).expect("download assembly")
}
fn stage(
    client: &mut Client,
    connection: &mut Connection,
    transfer: u64,
    image: Arc<Image>,
) -> Key {
    let key = key(client, transfer);
    CURRENT_KEY.set(Some(key));
    connection
        .send(client.stage_image(key, target(client), image).unwrap())
        .unwrap();
    loop {
        match reply(client, connection) {
            Reply::Ready { key: received } => {
                assert_eq!(received, key);
                println!("Upload {transfer} staged");
                return key;
            }
            _ => {}
        }
    }
}
fn confirmed(outcome: Outcome) {
    assert!(matches!(outcome, Outcome::Confirmed { .. }), "{outcome:?}");
}
fn action(
    client: &mut Client,
    connection: &mut Connection,
    track: TrackId,
    loop_id: LoopId,
    action: LoopAction,
) {
    confirmed(command(
        client,
        connection,
        Command::Loop {
            track,
            loop_id,
            action,
        },
    ));
}
fn wait_playing(client: &mut Client, connection: &mut Connection, track: TrackId, loop_id: LoopId) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if client
            .snapshot()
            .unwrap()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .unwrap()
            .loops
            .iter()
            .find(|entry| entry.id == loop_id)
            .unwrap()
            .mode
            == LoopMode::Playing
        {
            return;
        }
        assert!(Instant::now() < deadline, "playing state was not observed");
        pump(client, connection);
    }
}
pub fn run(client: &mut Client, connection: &mut Connection) {
    let socket =
        UnixStream::connect(std::env::var_os("SHOOP_ZYNQ_DRIVER_SOCKET").unwrap()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let mut driver = Driver(BufReader::new(socket));
    driver.call("P 1");
    DRIVER.with_borrow_mut(|slot| *slot = Some(driver));
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
            name: "Zynq exact media".into(),
            topology: TrackSpecTopology::Direct {
                audio_channels: 2,
                midi: true,
            },
            latency: Default::default(),
            initial_output_bus_names: vec!["Master".into()],
            creation_request_id: None,
        }),
    ) else {
        panic!("track creation")
    };
    for _ in 0..20 {
        pump(client, connection);
    }
    let ports = client.snapshot().unwrap().connections.clone();
    let inputs: Vec<_> = ports
        .application_ports
        .iter()
        .filter(|p| {
            matches!(p.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == track)
                && p.direction == PortDirection::Input
        })
        .collect();
    let mut audio = 0;
    for port in inputs {
        let name = if port.data_type == PortDataType::Audio {
            audio += 1;
            format!("audio_in_{audio}")
        } else {
            "midi_in_1".into()
        };
        let host = ports
            .host_ports
            .iter()
            .find(|p| p.name == name)
            .expect("host input");
        confirmed(command(
            client,
            connection,
            Command::SetPortConnected {
                port: port.id,
                host_port: host.id.clone(),
                connected: true,
            },
        ));
    }
    let midi_output = ports
        .application_ports
        .iter()
        .find(|p| {
            matches!(p.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == track)
                && p.direction == PortDirection::Output
                && p.data_type == PortDataType::Midi
        })
        .expect("track MIDI output");
    let host_midi = ports
        .host_ports
        .iter()
        .find(|p| p.name == "midi_out_1")
        .unwrap();
    confirmed(command(
        client,
        connection,
        Command::SetPortConnected {
            port: midi_output.id,
            host_port: host_midi.id.clone(),
            connected: true,
        },
    ));
    let loop_id = loops[0];
    drive(&format!("I {}", 0.25f32.to_bits()));
    action(
        client,
        connection,
        track,
        loop_id,
        LoopAction::RecordClicked,
    );
    drive("M 1");
    drive("S 256");
    drive("S 256");
    drive("M 2");
    drive("S 256");
    action(client, connection, track, loop_id, LoopAction::StopClicked);
    for _ in 0..20 {
        pump(client, connection);
    }
    let recorded = capture(client, connection, 10);
    println!(
        "Captured deterministic recording: {:?}",
        recorded
            .payloads
            .iter()
            .map(|p| p.bytes())
            .collect::<Vec<_>>()
    );
    let audio: Vec<_> = recorded
        .payloads
        .iter()
        .filter_map(|p| {
            if let Payload::Audio(p) = p {
                Some(p)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(audio.len(), 2);
    for (channel, samples) in audio.iter().enumerate() {
        assert!(!samples.is_empty());
        let expected = if channel == 0 { 0.25f32 } else { -0.25f32 };
        assert!(
            samples.iter().all(|s| s.to_bits() == expected.to_bits()),
            "recorded PCM channel {channel} mismatch"
        );
    }
    let midi = recorded
        .payloads
        .iter()
        .find_map(|p| {
            if let Payload::Midi(p) = p {
                Some(p)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(midi.len(), 2);
    assert_eq!(midi[0].data.as_ref(), [0x90, 60, 100]);
    assert_eq!(midi[1].data.as_ref(), [0x80, 60, 0]);
    assert_eq!(midi[1].frame - midi[0].frame, 512);
    drive("I 0");
    let midi_before = drive("Q");
    action(client, connection, track, loop_id, LoopAction::PlayClicked);
    wait_playing(client, connection, track, loop_id);
    let mut expected = 2_166_136_261u32;
    for sample in [0.25f32, -0.25f32] {
        for _ in 0..256 {
            expected = expected.wrapping_mul(16_777_619) ^ sample.to_bits();
        }
    }
    let mut played_midi = false;
    for _ in 0..100 {
        let actual = drive("S 256");
        assert_eq!(actual[1], expected, "rendered playback PCM mismatch");
        played_midi |= actual[8] > midi_before[8] && actual[9] > midi_before[9];
        assert_eq!(actual[10], 0, "dummy MIDI output changed note bytes");
    }
    assert!(played_midi, "playback did not reach dummy MIDI output");
    assert!(
        matches!(
            command(
                client,
                connection,
                Command::Loop {
                    track,
                    loop_id,
                    action: LoopAction::RerecordClicked,
                }
            ),
            Outcome::Refused {
                reason: Refusal::Unsupported,
                ..
            }
        ),
        "unsupported dry/wet operation was accepted"
    );
    action(client, connection, track, loop_id, LoopAction::StopClicked);
    for _ in 0..20 {
        pump(client, connection);
    }
    println!("Exact recorded PCM, MIDI timing, rendered playback and dummy MIDI output passed");
    let mut large = (*recorded).clone();
    let Manifest::Session(session) = &mut large.manifest else {
        panic!("session capture")
    };
    let small = std::env::var_os("SHOOP_ZYNQ_SMALL_FIXTURE").is_some();
    let frames = if small { 4096 } else { 5 * 1024 * 1024 };
    for track in &mut session.tracks {
        if track.name != "Zynq exact media" {
            continue;
        }
        let content = &mut track.loops[0];
        content.frames = frames as u64;
        for channel in &mut content.audio {
            channel.frames = frames as u64;
            channel.start_offset = 0;
            channel.capture_alignment = 0;
            channel.preplay = 0;
            let index = channel.object.unwrap() as usize - 1;
            let samples = (0..frames)
                .map(|i| {
                    let sample = ((i % 257) as f32 - 128.) / 512.;
                    if index % 2 == 0 {
                        sample
                    } else {
                        -sample
                    }
                })
                .collect();
            large.payloads[index] = Payload::Audio(Arc::new(samples));
        }
        for channel in &mut content.midi {
            channel.channel.frames = frames as u64;
        }
    }
    assert_eq!(
        large
            .payloads
            .iter()
            .filter_map(|p| if let Payload::Audio(a) = p {
                Some(a.len() * 4)
            } else {
                None
            })
            .sum::<usize>(),
        frames * 8
    );
    let large = Arc::new(large);
    let staged = stage(client, connection, 11, large.clone());
    confirmed(command(
        client,
        connection,
        Command::CommitPersistence(staged),
    ));
    println!("{} audio bytes committed", frames * 8);
    let roundtrip = capture(client, connection, 12);
    assert_eq!(roundtrip.payloads.len(), large.payloads.len());
    for (index, (actual, expected)) in roundtrip.payloads.iter().zip(&large.payloads).enumerate() {
        match (actual, expected) {
            (Payload::Audio(actual), Payload::Audio(expected)) => {
                assert_eq!(actual.len(), expected.len(), "audio object {index} length");
                assert!(
                    actual
                        .iter()
                        .zip(expected.iter())
                        .all(|(a, b)| a.to_bits() == b.to_bits()),
                    "audio object {index} changed sample bits"
                );
            }
            (Payload::Midi(actual), Payload::Midi(expected)) => {
                assert!(actual == expected, "MIDI object {index} changed")
            }
            _ => panic!("object {index} changed type"),
        }
    }
    let Manifest::Session(actual) = &roundtrip.manifest else {
        panic!("session expected")
    };
    let Manifest::Session(expected_session) = &large.manifest else {
        unreachable!()
    };
    assert_eq!(actual.sample_rate, expected_session.sample_rate);
    assert_eq!(actual.global, expected_session.global);
    assert_eq!(actual.tracks.len(), expected_session.tracks.len());
    assert_eq!(actual.master.name, expected_session.master.name);
    assert_eq!(actual.master.gain_db, expected_session.master.gain_db);
    assert_eq!(actual.master.balance, expected_session.master.balance);
    assert_eq!(actual.master.muted, expected_session.master.muted);
    assert_eq!(
        actual
            .master
            .channels
            .iter()
            .map(|c| &c.label)
            .collect::<Vec<_>>(),
        expected_session
            .master
            .channels
            .iter()
            .map(|c| &c.label)
            .collect::<Vec<_>>()
    );
    assert_eq!(actual.routes.len(), expected_session.routes.len());
    for (actual, expected) in actual.tracks.iter().zip(&expected_session.tracks) {
        assert_eq!(
            (
                &actual.name,
                actual.sync,
                actual.audio_channels,
                actual.midi
            ),
            (
                &expected.name,
                expected.sync,
                expected.audio_channels,
                expected.midi
            )
        );
        assert_eq!(actual.controls, expected.controls);
        assert_eq!(actual.loops.len(), expected.loops.len());
        assert_eq!(actual.ports.len(), expected.ports.len());
        for (actual, expected) in actual.ports.iter().zip(&expected.ports) {
            assert_eq!(
                (
                    actual.state.role,
                    actual.state.data_type,
                    actual.state.direction
                ),
                (
                    expected.state.role,
                    expected.state.data_type,
                    expected.state.direction
                )
            );
            assert_eq!(actual.external_connections, expected.external_connections);
        }
        for (actual, expected) in actual.loops.iter().zip(&expected.loops) {
            assert_eq!(
                (
                    &actual.name,
                    actual.sync,
                    actual.frames,
                    actual.gain,
                    actual.balance
                ),
                (
                    &expected.name,
                    expected.sync,
                    expected.frames,
                    expected.gain,
                    expected.balance
                )
            );
            assert_eq!(actual.audio, expected.audio);
            assert_eq!(actual.midi, expected.midi);
        }
    }
    if !small {
        let track = client
            .snapshot()
            .unwrap()
            .tracks
            .iter()
            .find(|track| track.name == "Zynq exact media")
            .unwrap()
            .clone();
        action(
            client,
            connection,
            track.id,
            track.loops[0].id,
            LoopAction::PlayClicked,
        );
        wait_playing(client, connection, track.id, track.loops[0].id);
        drive("S 512");
        let hashes: Vec<u32> = (0..257)
            .map(|phase| {
                let mut hash = 2_166_136_261u32;
                for channel in 0..2 {
                    for frame in 0..256 {
                        let sample = ((phase + frame) % 257) as f32 / 512. - 0.25;
                        let sample = if channel == 0 { sample } else { -sample };
                        // Mixing into a cleared output turns negative zero into positive zero.
                        let sample = if sample == 0. { 0. } else { sample };
                        hash = hash.wrapping_mul(16_777_619) ^ sample.to_bits();
                    }
                }
                hash
            })
            .collect();
        for _ in 0..16 {
            assert!(
                hashes.contains(&drive("S 256")[1]),
                "large session did not render its distinct stereo PCM through the engine"
            );
        }
        println!("Loaded 40 MiB session rendered exact stereo PCM through the real engine");
    }
    println!(
        "{frames} stereo frames plus MIDI exact segmented session round trip passed; stats={:?}",
        drive("Q")
    );
    capacity(client, connection);
    soak(client, connection, recorded.clone());
    if !small {
        pool_exhaustion(client, connection, recorded);
    }
    drive("P 0");
    drive("F 1");
    DRIVER.with_borrow_mut(|slot| *slot = None);
}

fn capacity(client: &mut Client, connection: &mut Connection) {
    for (transfer, media_bytes, metadata_bytes, objects) in [
        (20, 64 * 1024 * 1024 + 1, 1, 1),
        (21, 0, 1024 * 1024 + 1, 1),
        (22, 0, 1, 513),
    ] {
        let key = key(client, transfer);
        request(
            client,
            connection,
            Request::Begin(Description {
                key,
                target: target(client),
                objects,
                metadata_bytes,
                media_bytes,
            }),
        );
        let response = received(client, connection);
        assert!(
            matches!(
                &response,
                Reply::Refused {
                    reason: Refusal::Overloaded,
                    ..
                }
            ),
            "capacity transfer {transfer} expected Overloaded, received {response:?}"
        );
    }
    let key = key(client, 23);
    request(
        client,
        connection,
        Request::Begin(Description {
            key,
            target: target(client),
            objects: 2,
            metadata_bytes: 1,
            media_bytes: 0,
        }),
    );
    reply(client, connection);
    request(
        client,
        connection,
        Request::Object {
            key,
            object: ObjectDescription {
                index: 0,
                kind: ObjectKind::Metadata,
                bytes: 2,
            },
        },
    );
    assert!(matches!(
        received(client, connection),
        Reply::Refused { .. }
    ));
    release(client, connection, key);
    let many = Key {
        transfer: TransferId(25),
        epoch: client.snapshot().unwrap().persistence_epoch,
    };
    request(
        client,
        connection,
        Request::Begin(Description {
            key: many,
            target: target(client),
            objects: 512,
            metadata_bytes: 1,
            media_bytes: 0,
        }),
    );
    reply(client, connection);
    for index in 0..512 {
        request(
            client,
            connection,
            Request::Object {
                key: many,
                object: ObjectDescription {
                    index,
                    kind: if index == 0 {
                        ObjectKind::Metadata
                    } else {
                        ObjectKind::Midi
                    },
                    bytes: u64::from(index == 0),
                },
            },
        );
        reply(client, connection);
        if index == 0 {
            request(
                client,
                connection,
                Request::Chunk {
                    key: many,
                    object: index,
                    offset: 0,
                    bytes: vec![0],
                },
            );
            reply(client, connection);
        }
        request(
            client,
            connection,
            Request::FinishObject {
                key: many,
                object: index,
            },
        );
        reply(client, connection);
    }
    request(
        client,
        connection,
        Request::Object {
            key: many,
            object: ObjectDescription {
                index: 512,
                kind: ObjectKind::Midi,
                bytes: 0,
            },
        },
    );
    assert!(matches!(
        received(client, connection),
        Reply::Refused { .. }
    ));
    release(client, connection, many);
    println!("Board media, metadata, object and inconsistent-total admission refusals passed");
    let state = client.snapshot().unwrap();
    let content = &state
        .tracks
        .iter()
        .find(|t| t.name == "Zynq exact media")
        .unwrap()
        .loops[0];
    let target = Target::Loop {
        loop_id: content.id,
        content_revision: content.content_revision,
    };
    let revision = content.content_revision;
    let key = Key {
        transfer: TransferId(24),
        epoch: client.snapshot().unwrap().persistence_epoch,
    };
    let events = (0..65_537)
        .map(|frame| MidiEventState {
            frame,
            data: Arc::from([0x90, 60, 100]),
        })
        .collect();
    let image = Arc::new(Image {
        manifest: Manifest::Update {
            sample_rate: 48000,
            frames: Some(70_000),
            audio: Vec::new(),
            midi: vec![MidiUpdate {
                channel: 0,
                content: MidiChannel {
                    channel: Channel {
                        object: Some(1),
                        frames: 70_000,
                        gain: 1.,
                        start_offset: 0,
                        capture_alignment: 0,
                        preplay: 0,
                        recording_started_at: None,
                    },
                    initial: Vec::new(),
                },
            }],
        },
        payloads: vec![Payload::Midi(Arc::new(events))],
    });
    CURRENT_KEY.set(Some(key));
    connection
        .send(client.stage_image(key, target, image).unwrap())
        .unwrap();
    loop {
        if matches!(received(client, connection), Reply::Refused { .. }) {
            break;
        }
    }
    RETIRED_KEY.set(Some(key));
    assert_eq!(
        client
            .snapshot()
            .unwrap()
            .tracks
            .iter()
            .find(|t| t.name == "Zynq exact media")
            .unwrap()
            .loops[0]
            .content_revision,
        revision
    );
    println!("65,537-event MIDI input refused without changing live content");
}

fn pool_exhaustion(client: &mut Client, connection: &mut Connection, small: Arc<Image>) {
    let mut minimal = (*small).clone();
    let Manifest::Session(session) = &mut minimal.manifest else {
        panic!("session")
    };
    for track in &mut session.tracks {
        track.loops.truncate(1);
    }
    let staged = stage(client, connection, 490, Arc::new(minimal));
    confirmed(command(
        client,
        connection,
        Command::CommitPersistence(staged),
    ));
    for _ in 0..20 {
        pump(client, connection);
    }
    let track = client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .find(|t| t.name == "Zynq exact media")
        .unwrap()
        .clone();
    let Outcome::TrackCreated { .. } = command(
        client,
        connection,
        Command::AddTrack(TrackSpec {
            name: "Pool capacity".into(),
            topology: TrackSpecTopology::Direct {
                audio_channels: 2,
                midi: false,
            },
            latency: Default::default(),
            initial_output_bus_names: vec!["Master".into()],
            creation_request_id: None,
        }),
    ) else {
        panic!("pool track")
    };
    for _ in 0..20 {
        pump(client, connection);
    }
    let mut minimal = (*capture(client, connection, 491)).clone();
    let Manifest::Session(session) = &mut minimal.manifest else {
        panic!("session")
    };
    for track in &mut session.tracks {
        track.loops.truncate(1);
    }
    let staged = stage(client, connection, 492, Arc::new(minimal));
    confirmed(command(
        client,
        connection,
        Command::CommitPersistence(staged),
    ));
    for _ in 0..20 {
        pump(client, connection);
    }
    let state = client.snapshot().unwrap();
    let track = state
        .tracks
        .iter()
        .find(|t| t.name == track.name)
        .unwrap()
        .clone();
    let recording = state
        .tracks
        .iter()
        .find(|t| t.name == "Pool capacity")
        .unwrap();
    let recording_track = recording.id;
    let recording_loop = recording.loops[0].id;
    action(
        client,
        connection,
        track.id,
        track.loops[0].id,
        LoopAction::PlayClicked,
    );
    action(
        client,
        connection,
        recording_track,
        recording_loop,
        LoopAction::RecordClicked,
    );
    let before = drive("Q")[0];
    let mut pressure_checked = false;
    let mut exhausted = false;
    for batch in 0..1200 {
        drive("S 16384");
        pump(client, connection);
        if batch % 256 == 0 {
            println!("Recording-pool fill batch {batch}");
        }
        if batch == 520 {
            action(
                client,
                connection,
                recording_track,
                recording_loop,
                LoopAction::StopClicked,
            );
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                pump(client, connection);
                let state = client.snapshot().unwrap();
                let content = &state
                    .tracks
                    .iter()
                    .find(|t| t.id == recording_track)
                    .unwrap()
                    .loops[0];
                if content.mode == LoopMode::Stopped && content.length_frames * 8 > 64 * 1024 * 1024
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "over-budget settled recording was not observed"
                );
            }
            let key = key(client, 493);
            request(
                client,
                connection,
                Request::Capture {
                    key,
                    target: target(client),
                },
            );
            assert!(
                matches!(received(client, connection), Reply::Refused { .. }),
                "over-budget recording export was admitted"
            );
            action(
                client,
                connection,
                recording_track,
                recording_loop,
                LoopAction::RecordClicked,
            );
            let deadline = Instant::now() + Duration::from_secs(30);
            while client
                .snapshot()
                .unwrap()
                .tracks
                .iter()
                .find(|t| t.id == recording_track)
                .unwrap()
                .loops[0]
                .mode
                != LoopMode::Recording
            {
                pump(client, connection);
                assert!(Instant::now() < deadline, "rerecord did not start");
            }
            pressure_checked = true;
        }
        let state = client.snapshot().unwrap();
        if state
            .tracks
            .iter()
            .find(|t| t.id == recording_track)
            .unwrap()
            .loops[0]
            .mode
            != LoopMode::Recording
        {
            exhausted = true;
            break;
        }
    }
    assert!(
        pressure_checked && exhausted,
        "recording capacity boundary was not reached"
    );
    for _ in 0..20 {
        pump(client, connection);
    }
    let state = client.snapshot().unwrap();
    let recording = &state
        .tracks
        .iter()
        .find(|t| t.id == recording_track)
        .unwrap()
        .loops[0];
    assert!(
        recording.length_frames > 0 && recording.length_frames < 300 * 65536,
        "recording did not stop at pool capacity: {}",
        recording.length_frames
    );
    assert_ne!(
        recording.mode,
        LoopMode::Recording,
        "pool exhaustion did not stop recording"
    );
    assert!(state.status.processed_frames > u64::from(before));
    assert_eq!(
        state
            .tracks
            .iter()
            .find(|t| t.id == track.id)
            .unwrap()
            .loops[0]
            .mode,
        LoopMode::Playing
    );
    let stats = drive("S 256");
    assert_ne!(
        stats[1], 1_299_645_893,
        "pool exhaustion silenced unrelated playback"
    );
    let key = key(client, 500);
    request(
        client,
        connection,
        Request::Capture {
            key,
            target: target(client),
        },
    );
    assert!(
        matches!(received(client, connection), Reply::Refused { .. }),
        "over-budget recording capture was accepted"
    );
    let epoch = client.snapshot().unwrap().persistence_epoch;
    let ids: Vec<_> = client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .map(|track| track.id)
        .collect();
    let mut padded = (*small).clone();
    let Manifest::Session(session) = &mut padded.manifest else {
        panic!("session")
    };
    session
        .tracks
        .iter_mut()
        .find(|track| track.name == "Zynq exact media")
        .unwrap()
        .loops[0]
        .frames = 9_000_000;
    let staged = stage(client, connection, 501, Arc::new(padded));
    assert!(
        matches!(
            command(client, connection, Command::CommitPersistence(staged)),
            Outcome::Refused { .. }
        ),
        "replacement exceeded the normalized media budget"
    );
    assert_eq!(client.snapshot().unwrap().persistence_epoch, epoch);
    assert_eq!(
        client
            .snapshot()
            .unwrap()
            .tracks
            .iter()
            .map(|track| track.id)
            .collect::<Vec<_>>(),
        ids
    );
    assert_eq!(
        drive("S 256")[1],
        stats[1],
        "refused replacement changed live playback"
    );
    confirmed(command(
        client,
        connection,
        Command::Track {
            track: recording_track,
            action: TrackAction::Remove,
        },
    ));
    println!("Full recording pool stopped capture, refused over-budget export, preserved playback and reclaimed track resources; stats={stats:?}");
}

fn soak(client: &mut Client, connection: &mut Connection, image: Arc<Image>) {
    let profile = shoop_host::remote::RemoteProfile::read(std::path::Path::new(
        &std::env::args().nth(1).unwrap(),
    ))
    .unwrap();
    let mut baseline = [0; 11];
    let cycles = std::env::var("SHOOP_ZYNQ_SOAK_CYCLES")
        .map(|s| s.parse::<u64>().unwrap())
        .unwrap_or(100);
    assert!((100..=100_000).contains(&cycles));
    let before = client.snapshot().unwrap().tracks[1].loops[0].clone();
    for cycle in 0..cycles + 10 {
        let key = stage(client, connection, 100 + cycle, image.clone());
        release(client, connection, key);
        let detach = client.detach().unwrap();
        connection.send(detach).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while connection.port.pending_records() > 0 {
            connection.port.flush().unwrap();
            assert!(Instant::now() < deadline, "detach flush deadline");
            std::thread::sleep(Duration::from_millis(2));
        }
        let shoop_host::connection::Port::Socket(port) = &connection.port else {
            panic!("TCP expected")
        };
        port.stream().shutdown(std::net::Shutdown::Write).unwrap();
        let mut bytes = [0; shoop_transport::MAX_RECORD_BYTES];
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            for _ in 0..MAX_MESSAGES_PER_TURN {
                if connection.port.take(&mut bytes).is_none() {
                    break;
                }
            }
            if let Some(error) = connection.port.failure() {
                assert_eq!(error, shoop_transport::CarrierError::Disconnected);
                break;
            }
            assert!(Instant::now() < deadline, "detached peer did not close");
            advance();
            std::thread::sleep(Duration::from_millis(2));
        }
        (*client, *connection) = super::attach(&profile);
        for _ in 0..10 {
            pump(client, connection);
        }
        if cycle == 9 {
            baseline = retained_stats(client, connection);
        }
    }
    let final_stats = retained_stats(client, connection);
    assert!(
        final_stats[3] == baseline[3],
        "retained Rust heap growth: {baseline:?} -> {final_stats:?}"
    );
    assert!(final_stats[5] >= baseline[5], "retained C heap growth");
    assert_eq!(final_stats[6], baseline[6], "retained socket slots");
    let after = &client.snapshot().unwrap().tracks[1].loops[0];
    assert_eq!(after.content_revision, before.content_revision);
    assert_eq!(after.length_frames, before.length_frames);
    assert!(
        final_stats[4] < 768 * 1024 * 1024,
        "Rust arena peak exceeded"
    );
    println!(
        "{cycles} warmed attach/stage/cancel/detach cycles passed: {baseline:?} -> {final_stats:?}"
    );
}

fn retained_stats(client: &mut Client, connection: &mut Connection) -> [u32; 11] {
    // The fixture task can preempt temporary snapshot serialization. Compare
    // bounded lower watermarks after pumping, keeping exact retained equality.
    for _ in 0..20 {
        pump(client, connection);
    }
    let mut minimum = drive("Q");
    let mut maximum = minimum[3];
    let mut peak = minimum[4];
    for _ in 0..32 {
        pump(client, connection);
        let stats = drive("Q");
        maximum = maximum.max(stats[3]);
        peak = peak.max(stats[4]);
        if stats[3] < minimum[3] {
            minimum = stats;
        }
    }
    minimum[4] = peak;
    println!(
        "Retained Rust heap window: minimum={} maximum={maximum}",
        minimum[3]
    );
    minimum
}
