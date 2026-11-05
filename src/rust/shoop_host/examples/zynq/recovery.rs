use super::{attach, command, pump};
use shoop_app_api::*;
use shoop_app_client::{Client, State};
use shoop_app_protocol::*;
use shoop_host::{
    connection::{self, Connection},
    remote::RemoteProfile,
};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

struct Relay {
    endpoint: String,
    blocked: Arc<AtomicU8>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Relay {
    fn new(target: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let target = target.to_owned();
        let blocked = Arc::new(AtomicU8::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let flags = blocked.clone();
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            let mut buffers = [Vec::new(), Vec::new()];
            let (mut client, _) = loop {
                if stopped.load(Ordering::Acquire) {
                    return;
                }
                if let Ok(peer) = listener.accept() {
                    break peer;
                }
                thread::sleep(Duration::from_millis(1));
            };
            let mut server = TcpStream::connect(target).unwrap();
            client.set_nonblocking(true).unwrap();
            server.set_nonblocking(true).unwrap();
            client.set_nodelay(true).unwrap();
            server.set_nodelay(true).unwrap();
            while !stopped.load(Ordering::Acquire) {
                let blocked = flags.load(Ordering::Acquire);
                for index in 0..2 {
                    let (source, destination) = if index == 0 {
                        (&mut client, &mut server)
                    } else {
                        (&mut server, &mut client)
                    };
                    let buffer = &mut buffers[index];
                    let mut bytes = [0; 4096];
                    match source.read(&mut bytes) {
                        Ok(0) => return,
                        Ok(n) if blocked & (1 << index) == 0 => {
                            buffer.extend_from_slice(&bytes[..n])
                        }
                        Ok(_) => buffer.clear(),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(_) => return,
                    }
                    if blocked & (1 << index) != 0 {
                        buffer.clear();
                        continue;
                    }
                    if !buffer.is_empty() {
                        match destination.write(buffer) {
                            Ok(n) => {
                                buffer.drain(..n);
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(_) => return,
                        }
                    }
                    assert!(buffer.len() < 1024 * 1024, "relay queue bound");
                }
                thread::sleep(Duration::from_millis(1));
            }
        });
        Self {
            endpoint,
            blocked,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn confirmed(outcome: Outcome) {
    assert!(matches!(outcome, Outcome::Confirmed { .. }), "{outcome:?}");
}
fn status(profile: &RemoteProfile) -> ManagementReply {
    connection::management(
        profile,
        ManagementRequest::Status { version: VERSION },
        Duration::from_secs(5),
    )
    .unwrap()
}
fn retired(profile: &RemoteProfile) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if matches!(status(profile), ManagementReply::Status { busy: false, .. }) {
            return;
        }
        assert!(Instant::now() < deadline, "controller did not expire");
        thread::sleep(Duration::from_millis(100));
    }
}
fn replace(profile: &RemoteProfile, client: &mut Client, socket: &mut Connection) {
    let connection::Port::Socket(port) = &socket.port else {
        panic!("TCP expected")
    };
    port.stream().shutdown(std::net::Shutdown::Both).unwrap();
    client.disconnect();
    retired(profile);
    (*client, *socket) = attach(profile);
}
fn relayed(profile: &RemoteProfile, client: &mut Client, socket: &mut Connection) -> Relay {
    let connection::Port::Socket(port) = &socket.port else {
        panic!("TCP expected")
    };
    port.stream().shutdown(std::net::Shutdown::Both).unwrap();
    retired(profile);
    let relay = Relay::new(&profile.endpoint);
    let mut forwarded = profile.clone();
    forwarded.endpoint = relay.endpoint.clone();
    (*client, *socket) = attach(&forwarded);
    relay
}
fn lose(
    profile: &RemoteProfile,
    client: &mut Client,
    socket: &mut Connection,
    relay: &Relay,
    mask: u8,
) {
    relay.blocked.store(mask, Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !matches!(status(profile), ManagementReply::Status { busy: false, .. }) {
        let _ = socket.poll(client);
        assert!(
            Instant::now() < deadline,
            "transfer blackhole did not expire"
        );
        thread::sleep(Duration::from_millis(100));
    }
    replace(profile, client, socket);
}
pub fn run(profile: &RemoteProfile) {
    let (mut client, mut socket) = attach(profile);
    let first = client.authority().unwrap();
    let wrong_key = profile
        .credential
        .as_ref()
        .unwrap()
        .with_file_name("wrong.key");
    shoop_host::remote::create_credential(&wrong_key).unwrap();
    let mut wrong = profile.clone();
    wrong.credential = Some(wrong_key.clone());
    assert!(connection::connect(&wrong, Duration::from_secs(2)).is_err());
    std::fs::remove_file(wrong_key).unwrap();
    let mut wrong = connection::connect(profile, Duration::from_secs(5)).unwrap();
    let mut version = Client::new();
    version.attach().unwrap();
    wrong
        .send(ClientMessage::Attach {
            version: VERSION + 1,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while version.state() != State::Lost {
        let _ = wrong.poll(&mut version);
        pump(&mut client, &mut socket);
        assert!(Instant::now() < deadline);
    }
    assert_eq!(version.refusal(), Some(Refusal::Version));
    drop(wrong);
    let mut busy = connection::connect(profile, Duration::from_secs(5)).unwrap();
    let mut competitor = Client::new();
    busy.send(competitor.attach().unwrap()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while competitor.state() != State::Lost {
        let _ = busy.poll(&mut competitor);
        pump(&mut client, &mut socket);
        assert!(Instant::now() < deadline);
    }
    assert_eq!(competitor.refusal(), Some(Refusal::Busy));
    drop(busy);
    let mut slow: Vec<_> = (0..12)
        .map(|_| TcpStream::connect(&profile.endpoint).unwrap())
        .collect();
    let start = client.snapshot().unwrap().status.processed_frames;
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        pump(&mut client, &mut socket);
    }
    assert!(client.snapshot().unwrap().status.processed_frames > start);
    confirmed(command(
        &mut client,
        &mut socket,
        Command::SetLoopSmoothingMs(0),
    ));
    assert_eq!(client.authority(), Some(first));
    drop(slow.drain(4..).collect::<Vec<_>>());
    let deadline = Instant::now() + Duration::from_secs(120);
    while client.snapshot().unwrap().status.processed_frames - start < 6 * 48000 {
        pump(&mut client, &mut socket);
        assert!(
            Instant::now() < deadline,
            "slow handshake did not reach its guest-time expiry"
        );
    }
    let mut expired_slot = connection::connect(profile, Duration::from_secs(20))
        .expect("expired handshake slots were not released");
    let mut challenger = Client::new();
    expired_slot.send(challenger.attach().unwrap()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while challenger.state() != State::Lost {
        let _ = expired_slot.poll(&mut challenger);
        pump(&mut client, &mut socket);
        assert!(Instant::now() < deadline);
    }
    assert_eq!(challenger.refusal(), Some(Refusal::Busy));
    drop(expired_slot);
    drop(slow);
    println!("Wrong credentials/version, competing controller and 12 slow handshakes preserved the healthy owner");
    confirmed(command(
        &mut client,
        &mut socket,
        Command::Global(GlobalControlAction::SetSync(false)),
    ));
    let Outcome::TrackCreated { track, loops, .. } = command(
        &mut client,
        &mut socket,
        Command::AddTrack(TrackSpec {
            name: "Surviving recording".into(),
            topology: TrackSpecTopology::Direct {
                audio_channels: 1,
                midi: true,
            },
            latency: Default::default(),
            initial_output_bus_names: vec!["Master".into()],
            creation_request_id: None,
        }),
    ) else {
        panic!("track")
    };
    let loop_id = loops[0];
    confirmed(command(
        &mut client,
        &mut socket,
        Command::Loop {
            track,
            loop_id,
            action: LoopAction::RecordClicked,
        },
    ));
    for directions in [1, 2, 3] {
        let detach = client.detach().unwrap();
        socket.send(detach).unwrap();
        retired(profile);
        let relay = Relay::new(&profile.endpoint);
        let mut relayed = profile.clone();
        relayed.endpoint = relay.endpoint.clone();
        (client, socket) = attach(&relayed);
        let old = client.authority().unwrap();
        let before = client.snapshot().unwrap().status.processed_frames;
        relay.blocked.store(directions, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(30);
        while !matches!(status(profile), ManagementReply::Status { busy: false, .. }) {
            let _ = socket.poll(&mut client);
            assert!(Instant::now() < deadline, "blackhole lease did not expire");
            thread::sleep(Duration::from_millis(100));
        }
        (client, socket) = attach(profile);
        let state = client.snapshot().unwrap();
        assert!(state.status.processed_frames > before);
        assert_eq!(state.tracks[1].loops[0].mode, LoopMode::Recording);
        assert!(state.tracks[1].loops[0].length_frames > 0);
        assert!(client.authority().unwrap().generation > old.generation);
        assert!(matches!(
            connection::management(
                profile,
                ManagementRequest::Shutdown {
                    authority: old,
                    operation: OperationId {
                        client: old.client,
                        sequence: 900
                    }
                },
                Duration::from_secs(5)
            )
            .unwrap(),
            ManagementReply::Refused(Refusal::StaleAuthority)
        ));
        confirmed(command(
            &mut client,
            &mut socket,
            Command::SetLoopSmoothingMs(0),
        ));
        drop(relay);
        println!("TCP blackhole direction mask {directions}: recording continued, heartbeat expired, fresh state attached, stale authority refused");
    }
    confirmed(command(
        &mut client,
        &mut socket,
        Command::Loop {
            track,
            loop_id,
            action: LoopAction::StopClicked,
        },
    ));
    let deadline = Instant::now() + Duration::from_secs(10);
    while client.snapshot().unwrap().tracks[1].loops[0].mode != LoopMode::Stopped
        || client.snapshot().unwrap().tracks[1].loops[0]
            .next_transition_delay
            .is_some()
    {
        pump(&mut client, &mut socket);
        assert!(Instant::now() < deadline, "recording stop was not observed");
    }
    replace(profile, &mut client, &mut socket);
    persistence_recovery(profile, &mut client, &mut socket, loop_id);
    drop(socket);
    retired(profile);
    ownerless_lost_reply(profile);
    println!("Recovery and ownerless shutdown passed");
}

fn ownerless_lost_reply(profile: &RemoteProfile) {
    use shoop_host::remote::ConnectionTarget;
    use shoop_transport::{RecordPort, MAX_RECORD_BYTES};
    let token = profile.target().token().unwrap();
    let mut port = shoop_transport_socket::setup::connect_authenticated(
        profile.management.parse().unwrap(),
        &token,
        shoop_transport_socket::Config {
            reliable: 8,
            telemetry: 1,
            bulk: 8,
        },
        Duration::from_secs(10),
    )
    .unwrap();
    let mut bytes = [0; MAX_RECORD_BYTES];
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(n) = port.take(&mut bytes) {
            assert!(matches!(
                codec::decode(&bytes[..n]).unwrap(),
                WireMessage::CarrierReady
            ));
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    let operation = OperationId {
        client: ClientId(0xabc_0000_0000_0000_0001),
        sequence: 1,
    };
    let mut request = ManagementRequest::ClaimShutdown {
        version: VERSION,
        operation,
    };
    let n = codec::encode(&WireMessage::ManagementRequest(request.clone()), &mut bytes).unwrap();
    port.post(&bytes[..n]).unwrap();
    while !port.outgoing_empty() {
        port.flush().unwrap();
        assert!(Instant::now() < deadline);
    }
    drop(port);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match connection::management(profile, request.clone(), Duration::from_secs(5)).unwrap() {
            ManagementReply::Accepted {
                authority,
                operation: pending,
            } => {
                assert_eq!(pending.id, operation);
                request = ManagementRequest::Result {
                    authority,
                    operation,
                };
            }
            ManagementReply::Finished { result, .. } => {
                assert_eq!(result.id, operation);
                confirmed(result.outcome);
                break;
            }
            reply => panic!("lost shutdown reply did not reconcile: {reply:?}"),
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    println!("Lost ownerless shutdown reply reconciled the same operation after confirmed cleanup");
}

fn persistence_recovery(
    profile: &RemoteProfile,
    client: &mut Client,
    socket: &mut Connection,
    loop_id: LoopId,
) {
    use persistence::*;
    use persistence_image::*;
    let samples = Arc::new(vec![0.125; 256 * 1024]);
    let image = Arc::new(Image {
        manifest: Manifest::Update {
            sample_rate: 48000,
            frames: Some(samples.len() as u64),
            audio: vec![AudioUpdate {
                channel: 0,
                content: Channel {
                    object: Some(1),
                    frames: samples.len() as u64,
                    gain: 1.,
                    start_offset: 0,
                    capture_alignment: 0,
                    preplay: 0,
                    recording_started_at: None,
                },
            }],
            midi: Vec::new(),
        },
        payloads: vec![Payload::Audio(samples.clone())],
    });
    for phase in [0, 1, 2] {
        let relay = relayed(profile, client, socket);
        let state = client.snapshot().unwrap();
        let key = Key {
            transfer: TransferId(100 + phase),
            epoch: state.persistence_epoch,
        };
        let target = Target::Loop {
            loop_id,
            content_revision: state.tracks[1].loops[0].content_revision,
        };
        socket
            .send(client.stage_image(key, target, image.clone()).unwrap())
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut acks = 0;
        loop {
            pump(client, socket);
            let mut ready = false;
            while let Some(reply) = client.take_persistence_reply() {
                assert!(!matches!(reply, Reply::Refused { .. }), "{reply:?}");
                acks += 1;
                ready |= matches!(reply, Reply::Ready { .. });
            }
            if phase == 0 && acks > 8 || ready {
                break;
            }
            assert!(Instant::now() < deadline, "recovery upload timeout");
        }
        if phase == 2 {
            let message = client
                .command(Command::CommitPersistence(key), None)
                .unwrap();
            let ClientMessage::Submit { operation, .. } = message else {
                unreachable!()
            };
            socket.send(message).unwrap();
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                pump(client, socket);
                if client
                    .snapshot()
                    .unwrap()
                    .pending
                    .iter()
                    .any(|p| p.id == operation)
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "commit acceptance not observed before losing the confirmation"
                );
            }
            lose(profile, client, socket, &relay, 3);
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                pump(client, socket);
                if client
                    .snapshot()
                    .unwrap()
                    .results
                    .iter()
                    .any(|r| r.id == operation && matches!(r.outcome, Outcome::Confirmed { .. }))
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "host-owned accepted commit did not complete"
                );
            }
            let download_relay = relayed(profile, client, socket);
            socket.send(client.read_media(loop_id).unwrap()).unwrap();
            for _ in 0..10 {
                pump(client, socket);
            }
            assert!(
                client.media().is_none(),
                "download completed before fault injection"
            );
            lose(profile, client, socket, &download_relay, 2);
            socket.send(client.read_media(loop_id).unwrap()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(60);
            loop {
                pump(client, socket);
                if let Some(media) = client.media() {
                    assert_eq!(
                        media.as_ref().unwrap().audio[0].samples.as_ref(),
                        samples.as_slice()
                    );
                    break;
                }
                assert!(Instant::now() < deadline, "fresh download did not complete");
            }
        } else {
            lose(
                profile,
                client,
                socket,
                &relay,
                if phase == 0 { 1 } else { 2 },
            );
            assert!(matches!(
                command(client, socket, Command::CommitPersistence(key)),
                Outcome::Refused { .. }
            ));
        }
        println!("Persistence loss phase {phase}: staging reclaimed or accepted commit completed; fresh download exact");
    }
}
