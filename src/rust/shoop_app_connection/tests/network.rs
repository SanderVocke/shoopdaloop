mod support;
use shoop_app::{ConnectionId, Service};
use shoop_app_client::{Client, State};
use shoop_app_connection::{Heartbeat, HeartbeatConfig, Session, Upload};
use shoop_app_protocol::{
    persistence::{Key, Reply, Target},
    persistence_image::{AudioUpdate, Channel, Image, Manifest, Payload},
    *,
};
use shoop_transport::{CarrierError, RecordPort};
use shoop_wasm_test_support::shoop_test;
use std::{cell::RefCell, collections::VecDeque, rc::Rc, sync::Arc, time::Duration};

#[derive(Default)]
struct Link {
    time: u64,
    budgets: [usize; 2],
    queues: [VecDeque<(u64, Vec<u8>)>; 2],
    maximum: usize,
}
struct Peer {
    link: Rc<RefCell<Link>>,
    side: usize,
}
impl RecordPort for Peer {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        let mut link = self.link.borrow_mut();
        let side = self.side;
        if link.queues[side].len() >= 8 || link.budgets[side] < record.len() {
            return Err(CarrierError::Full);
        }
        link.budgets[side] -= record.len();
        let at = link.time + 10;
        link.queues[side].push_back((at, record.to_vec()));
        link.maximum = link.maximum.max(link.queues[side].len());
        Ok(())
    }
    fn take(&mut self, output: &mut [u8]) -> Option<usize> {
        let mut link = self.link.borrow_mut();
        let side = 1 - self.side;
        if link.queues[side].front()?.0 > link.time {
            return None;
        }
        let (_, record) = link.queues[side].pop_front()?;
        output[..record.len()].copy_from_slice(&record);
        Some(record.len())
    }
    fn pending_records(&self) -> usize {
        self.link.borrow().queues[self.side].len()
    }
}
struct Rig {
    link: Rc<RefCell<Link>>,
    peer: Peer,
    session: Session<Peer>,
    service: Service,
    client: Client,
    upload: Upload,
    heartbeat: Heartbeat,
    replies: Vec<Reply>,
    snapshots: usize,
}
impl Rig {
    fn new() -> Self {
        let link = Rc::new(RefCell::new(Link::default()));
        let session = Session::new(
            ConnectionId(1),
            Peer {
                link: link.clone(),
                side: 1,
            },
        )
        .with_network(HeartbeatConfig::default(), 100, 30)
        .unwrap();
        Self {
            peer: Peer {
                link: link.clone(),
                side: 0,
            },
            link,
            session,
            service: support::service(),
            client: Client::new(),
            upload: Upload::new(),
            heartbeat: Heartbeat::new(HeartbeatConfig::default(), 200).unwrap(),
            replies: Vec::new(),
            snapshots: 0,
        }
    }
    fn step(&mut self) {
        {
            let mut link = self.link.borrow_mut();
            link.time += 10;
            link.budgets = [1_048_576 / 100; 2];
        }
        let elapsed = Duration::from_millis(10);
        self.heartbeat.advance(elapsed);
        self.heartbeat.pump(&mut self.peer).unwrap();
        self.upload.pump(&mut self.peer).unwrap();
        self.session.advance(elapsed);
        self.session.receive(&mut self.service);
        self.service.tick(elapsed);
        self.session.write(&mut self.service);
        let mut bytes = [0; 1280];
        for _ in 0..64 {
            let Some(n) = self.peer.take(&mut bytes) else {
                break;
            };
            let message = codec::decode(&bytes[..n]).unwrap();
            if self.heartbeat.receive(&message) {
                continue;
            }
            if matches!(message, WireMessage::SnapshotBegin { .. }) {
                self.snapshots += 1;
            }
            self.upload.received(&message);
            self.client.receive_wire(message).unwrap();
        }
        if let Some(message) = self.client.ready_message() {
            if self.upload.send(&mut self.peer, message).is_ok() {
                self.client.ready_sent().unwrap();
            }
        }
        while let Some(reply) = self.client.take_persistence_reply() {
            assert!(!matches!(reply, Reply::Refused { .. }), "{reply:?}");
            self.replies.push(reply);
        }
        assert!(self.session.alive(&self.service));
        assert!(!self.heartbeat.expired());
    }
    fn send(&mut self, message: ClientMessage) {
        for _ in 0..128 {
            match self.upload.send(&mut self.peer, message.clone()) {
                Ok(()) => return,
                Err(shoop_app_connection::Error::Full) => self.step(),
                Err(e) => panic!("{e:?}"),
            }
        }
        panic!("send starved");
    }
    fn command(&mut self, command: Command) -> Outcome {
        let measure = matches!(command, Command::SetLoopSmoothingMs(_));
        let start = self.link.borrow().time;
        let message = self.client.command(command, None).unwrap();
        let ClientMessage::Submit { operation, .. } = message else {
            panic!()
        };
        self.send(message);
        for _ in 0..128 {
            self.step();
            if let Some(outcome) = self.client.results().get(&operation) {
                if measure {
                    let elapsed = self.link.borrow().time - start;
                    assert!(
                        elapsed <= 250,
                        "control exceeded simulated 250 ms budget: {elapsed}"
                    );
                    eprintln!("throttled control round-trip: {elapsed} simulated ms");
                }
                return outcome.clone();
            }
        }
        panic!("control did not complete");
    }
}

#[shoop_test]
fn throttled_upload_and_download_preserve_control_latency_heartbeat_and_snapshot_budgets() {
    let mut rig = Rig::new();
    rig.step();
    let attach = rig.client.attach().unwrap();
    rig.send(attach);
    for _ in 0..128 {
        rig.step();
        if rig.client.state() == State::Ready {
            break;
        }
    }
    assert_eq!(rig.client.state(), State::Ready);
    use shoop_app_api::{TrackSpec, TrackSpecTopology};
    rig.command(Command::AddTrack(TrackSpec {
        name: "Network".into(),
        topology: TrackSpecTopology::Direct {
            audio_channels: 1,
            midi: false,
        },
        latency: Default::default(),
        initial_output_bus_names: vec!["Master".into()],
        creation_request_id: None,
    }));
    for _ in 0..128 {
        rig.step();
        if rig.client.snapshot().unwrap().tracks.len() > 1 {
            break;
        }
    }
    let state = rig.client.snapshot().unwrap();
    let loop_id = state.tracks[1].loops[0].id;
    let key = Key {
        transfer: TransferId(1),
        epoch: state.persistence_epoch,
    };
    let target = Target::Loop {
        loop_id,
        content_revision: state.tracks[1].loops[0].content_revision,
    };
    let frames = 256 * 1024;
    let samples = Arc::new(vec![0.25; frames]);
    let image = Arc::new(Image {
        manifest: Manifest::Update {
            sample_rate: 48000,
            frames: Some(frames as u64),
            audio: vec![AudioUpdate {
                channel: 0,
                content: Channel {
                    mode: Default::default(),
                    object: Some(1),
                    frames: frames as u64,
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
    let stage = rig.client.stage_image(key, target, image).unwrap();
    rig.send(stage);
    for _ in 0..10 {
        rig.step();
    }
    assert!(rig.upload.active());
    rig.command(Command::SetLoopSmoothingMs(5));
    assert!(
        rig.upload.active(),
        "control was delayed until upload ended"
    );
    for _ in 0..1000 {
        rig.step();
        if rig
            .replies
            .iter()
            .any(|r| matches!(r,Reply::Ready {key:k} if *k==key))
        {
            break;
        }
    }
    assert!(rig
        .replies
        .iter()
        .any(|r| matches!(r,Reply::Ready {key:k} if *k==key)));
    rig.command(Command::CommitPersistence(key));
    let read = rig.client.read_media(loop_id).unwrap();
    rig.send(read);
    for _ in 0..10 {
        rig.step();
    }
    assert!(rig.client.media().is_none());
    rig.command(Command::SetLoopSmoothingMs(6));
    assert!(
        rig.client.media().is_none(),
        "control was delayed until download ended"
    );
    for _ in 0..1000 {
        rig.step();
        if rig.client.media().is_some() {
            break;
        }
    }
    assert_eq!(
        rig.client.media().unwrap().as_ref().unwrap().audio[0]
            .samples
            .as_ref(),
        samples.as_slice()
    );
    let snapshots = rig.snapshots;
    for _ in 0..100 {
        rig.step();
    }
    assert!(rig.snapshots - snapshots <= 30);
    assert!(rig.link.borrow().maximum <= 8);
}

#[shoop_test]
fn master_preview_stages_over_bounded_wire_records_and_cancels_without_session_media_changes() {
    use shoop_app_api::ClickTrackPreviewStatus;
    let mut rig = Rig::new();
    let attach = rig.client.attach().unwrap();
    rig.send(attach);
    for _ in 0..128 {
        rig.step();
        if rig.client.state() == State::Ready {
            break;
        }
    }
    assert_eq!(rig.client.state(), State::Ready);
    let snapshot = rig.client.snapshot().unwrap();
    let bus = snapshot
        .buses
        .iter()
        .find(|b| b.name == "Master")
        .unwrap()
        .id;
    let rate = snapshot.status.sample_rate;
    let tracks: Vec<_> = snapshot
        .tracks
        .iter()
        .flat_map(|t| &t.loops)
        .map(|l| (l.id, l.length_frames, l.content_revision))
        .collect();
    let command = |value| Command::Preview(value);
    assert!(matches!(
        rig.command(command(PreviewCommand::Begin {
            request_id: 1,
            bus,
            sample_rate: rate,
            frames: 12_000
        })),
        Outcome::Confirmed { .. }
    ));
    for offset in (0..12_000).step_by(PREVIEW_CHUNK_FRAMES) {
        let count = PREVIEW_CHUNK_FRAMES.min(12_000 - offset);
        assert!(matches!(
            rig.command(command(PreviewCommand::Chunk {
                request_id: 1,
                offset: offset as u32,
                samples: vec![0.25; count]
            })),
            Outcome::Confirmed { .. }
        ));
    }
    assert!(matches!(
        rig.command(command(PreviewCommand::Play { request_id: 1 })),
        Outcome::Confirmed { .. }
    ));
    let mut playing = false;
    for _ in 0..5000 {
        rig.step();
        if rig.client.snapshot().unwrap().preview.preview_status == ClickTrackPreviewStatus::Playing
        {
            playing = true;
            break;
        }
        assert_ne!(
            rig.client.snapshot().unwrap().preview.preview_status,
            ClickTrackPreviewStatus::Failed,
            "{:?}",
            rig.client.snapshot().unwrap().preview
        );
    }
    assert!(playing, "host never started preview");
    assert!(matches!(
        rig.command(command(PreviewCommand::Stop)),
        Outcome::Confirmed { .. }
    ));
    for _ in 0..100 {
        rig.step();
    }
    assert_eq!(
        rig.client.snapshot().unwrap().preview.preview_status,
        ClickTrackPreviewStatus::Cancelled
    );
    assert!(matches!(
        rig.command(command(PreviewCommand::Play { request_id: 1 })),
        Outcome::Refused { .. }
    ));
    let after = rig.client.snapshot().unwrap();
    assert_eq!(
        after
            .tracks
            .iter()
            .flat_map(|t| &t.loops)
            .map(|l| (l.id, l.length_frames, l.content_revision))
            .collect::<Vec<_>>(),
        tracks
    );
    assert_eq!(after.persistence_epoch, 1);
}
