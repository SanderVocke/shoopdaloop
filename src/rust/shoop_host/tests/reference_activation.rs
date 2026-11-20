#![cfg(all(feature = "fpga-reference", not(target_arch = "wasm32")))]
use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_app_api::*;
use shoop_app_client::Client;
use shoop_app_protocol::{immediate, ClientMessage, Command, OperationId, Outcome};
use shoop_host::fpga_reference::{self, Driver, Endpoint};
use shoop_rt_backend::{RtBackend, Sleep};
use shoop_rt_protocol::{immediate::Batch, ClientMessage as RtMessage, EngineMessage};
use shoop_rt_transport::{
    carrier::{Carrier, CarrierError, InProcess},
    client::ProtocolClient,
};
use shoop_wasm_test_support::shoop_test;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Gate {
    hold: AtomicBool,
    waiting: AtomicBool,
    refuse: AtomicBool,
    cycle: AtomicBool,
    tail_busy: AtomicBool,
    sent: Mutex<Vec<Batch>>,
}
struct Gated {
    inner: InProcess<Endpoint>,
    gate: Arc<Gate>,
    held: Option<EngineMessage>,
}
fn activation(batch: &Batch) -> bool {
    batch
        .commands()
        .contains(&shoop_rt_protocol::immediate::Command::ActivateGraph)
}
impl Carrier for Gated {
    fn send(&mut self, message: RtMessage) -> Result<(), CarrierError> {
        if let RtMessage::Immediate(batch) = &message {
            let header = batch.commands().iter().any(|c| {
                matches!(
                    c,
                    shoop_rt_protocol::immediate::Command::BeginGraphImage { .. }
                )
            });
            if header && self.gate.refuse.swap(false, Ordering::Relaxed) {
                return Err(CarrierError::Full);
            }
            if activation(batch) && self.gate.tail_busy.swap(false, Ordering::Relaxed) {
                return Err(CarrierError::Full);
            }
            if header && self.gate.cycle.swap(false, Ordering::Relaxed) {
                let mut commands = batch.commands().to_vec();
                for command in &mut commands {
                    if let shoop_rt_protocol::immediate::Command::BeginGraphImage {
                        quantum, ..
                    } = command
                    {
                        *quantum = 3;
                    }
                }
                return self.inner.send(RtMessage::Immediate(
                    Batch::new(batch.graph, batch.topology, &commands).unwrap(),
                ));
            }
            self.gate.sent.lock().unwrap().push(batch.clone());
        }
        self.inner.send(message)
    }
    fn receive(&mut self) -> Option<EngineMessage> {
        if self.held.is_some() {
            return (!self.gate.hold.load(Ordering::Relaxed)).then(|| self.held.take().unwrap());
        }
        let message = self.inner.receive()?;
        if self.gate.hold.load(Ordering::Relaxed)
            && matches!(&message,
            EngineMessage::ImmediateResponse(r) if r.records().iter().any(|r|
                r.command() == shoop_rt_protocol::immediate::Command::ActivateGraph))
        {
            self.held = Some(message);
            self.gate.waiting.store(true, Ordering::Relaxed);
            None
        } else {
            Some(message)
        }
    }
}
struct Fixture {
    service: Service,
    client: Client,
    gate: Arc<Gate>,
    _driver: Driver,
}
impl Fixture {
    fn new() -> Self {
        Self::with_host(fpga_reference::start_with_memory(16, 1000, 1).unwrap())
    }
    fn with_host(started: fpga_reference::Started) -> Self {
        let gate = Arc::new(Gate::default());
        let client = ProtocolClient::new(Gated {
            inner: started.client.into_carrier(),
            gate: gate.clone(),
            held: None,
        });
        let backend = RtBackend::connect(client, Sleep::default()).unwrap();
        let service = Service::new(
            Box::new(backend),
            ServiceOptions {
                loop_smoothing_ms: 0,
                master_auto_connect: true,
            },
        )
        .unwrap();
        let mut f = Self {
            service,
            client: Client::new(),
            gate,
            _driver: started.driver,
        };
        f.until(|f| f.service.is_ready());
        f.service
            .handle(ConnectionId(1), f.client.attach().unwrap())
            .unwrap();
        f.pump();
        let ready = f.client.ready_message().unwrap();
        f.service.handle(ConnectionId(1), ready).unwrap();
        f.client.ready_sent().unwrap();
        f.gate.sent.lock().unwrap().clear();
        f
    }
    fn pump(&mut self) {
        self.service.tick(Duration::from_millis(1));
        while let Some(message) = self.service.take_update(ConnectionId(1)) {
            self.client.receive(message).unwrap();
        }
    }
    fn until(&mut self, done: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !done(self) {
            self.pump();
            assert!(
                Instant::now() < deadline,
                "reference application operation timed out"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn submit(&mut self, command: Command) -> OperationId {
        let message = self.client.command(command, None).unwrap();
        let ClientMessage::Submit { operation, .. } = message else {
            unreachable!()
        };
        self.service.handle(ConnectionId(1), message).unwrap();
        operation
    }
    fn add_track(&mut self, bus: &str) -> OperationId {
        self.add_audio_track(bus, 1)
    }
    fn add_audio_track(&mut self, bus: &str, audio_channels: u32) -> OperationId {
        self.submit(Command::AddTrack(TrackSpec {
            name: "Reference".into(),
            topology: TrackSpecTopology::Direct {
                audio_channels,
                midi: false,
            },
            initial_output_bus_names: if bus.is_empty() {
                vec![]
            } else {
                vec![bus.into()]
            },
            latency: Default::default(),
            creation_request_id: None,
        }))
    }
}
#[shoop_test]
fn activation_is_one_operation_and_orders_following_commands_after_the_fence() {
    let mut f = Fixture::new();
    f.gate.hold.store(true, Ordering::Relaxed);
    let added = f.add_track("Master");
    f.until(|f| f.gate.waiting.load(Ordering::Relaxed));
    assert!(!f.client.results().contains_key(&added));
    let state = f.service.snapshot();
    let track = &state.tracks[1];
    let voice = track.loops[0].id;
    let rename = f.submit(Command::Loop {
        track: track.id,
        loop_id: voice,
        action: LoopAction::NameChanged("Queued behind switch".into()),
    });
    let record = f
        .client
        .immediate(vec![immediate::Command::Transition(
            voice,
            TransitionPlan {
                now: Some(TransitionMode::Recording),
                ..Default::default()
            },
        )])
        .unwrap();
    f.service.handle(ConnectionId(1), record).unwrap();
    for _ in 0..5 {
        f.pump();
    }
    assert!(!f.client.results().contains_key(&rename));
    assert_eq!(
        f.gate
            .sent
            .lock()
            .unwrap()
            .iter()
            .filter(|b| activation(b))
            .count(),
        1
    );
    f.gate.hold.store(false, Ordering::Relaxed);
    f.until(|f| {
        f.client.results().contains_key(&added)
            && f.client.results().contains_key(&rename)
            && f.service.snapshot().tracks[1].loops[0].mode == LoopMode::Recording
    });
    assert!(matches!(
        f.client.results()[&added],
        Outcome::TrackCreated { .. }
    ));
    assert_eq!(
        f.service.snapshot().tracks[1].loops[0].name,
        "Queued behind switch"
    );
    assert_eq!(
        f.gate
            .sent
            .lock()
            .unwrap()
            .iter()
            .filter(|b| activation(b))
            .count(),
        1
    );
}
#[shoop_test]
fn refused_installation_restores_the_previous_graph_and_allows_retry() {
    for fail_transport in [false, true] {
        let mut f = Fixture::new();
        f.gate.refuse.store(fail_transport, Ordering::Relaxed);
        f.gate.cycle.store(!fail_transport, Ordering::Relaxed);
        let failed = f.add_track("Master");
        f.until(|f| f.client.results().contains_key(&failed));
        assert!(matches!(
            f.client.results()[&failed],
            Outcome::Refused { .. }
        ));
        assert_eq!(f.service.snapshot().tracks.len(), 1);
        let added = f.add_track("Master");
        f.until(|f| f.client.results().contains_key(&added));
        assert!(matches!(
            f.client.results()[&added],
            Outcome::TrackCreated { .. }
        ));
        assert_eq!(f.service.snapshot().tracks.len(), 2);
    }
}
#[shoop_test]
fn partial_routing_activates_and_allows_later_edits() {
    let mut f = Fixture::new();
    let added = f.add_track("Master");
    f.until(|f| f.client.results().contains_key(&added));
    let state = f.service.snapshot();
    let output = state.connections.application_ports.iter().find(|p|
        matches!(p.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == state.tracks[1].id)
        && p.role == PortRole::AudioOutput).unwrap().id;
    let channel = state.buses[0].channels[0].id;
    let before = state.connections.mixer_links.len();
    let changed = f.submit(Command::SetMixerRoute {
        source: output,
        destination: channel,
        connected: false,
    });
    f.until(|f| f.client.results().contains_key(&changed));
    assert!(!matches!(
        f.client.results()[&changed],
        Outcome::Refused { .. }
    ));
    assert_eq!(
        f.service.snapshot().connections.mixer_links.len(),
        before - 1
    );
    let added = f.submit(Command::AddLoop {
        track: state.tracks[1].id,
    });
    f.until(|f| f.client.results().contains_key(&added));
    assert!(matches!(
        f.client.results()[&added],
        Outcome::LoopCreated { .. }
    ));
}

#[shoop_test]
fn default_profile_admits_64_stereo_loops_and_rolls_back_capacity_refusals() {
    let mut f = Fixture::with_host(fpga_reference::start(16, 1000).unwrap());
    let mut accepted = 0;
    let mut refused = None;
    for _ in 0..32 {
        let operation = f.add_audio_track("Master", 2);
        f.until(|f| f.client.results().contains_key(&operation));
        if matches!(f.client.results()[&operation], Outcome::Refused { .. }) {
            refused = Some(format!("{:?}", f.client.results()[&operation]));
            break;
        }
        accepted += 1;
    }
    assert!(
        refused.is_some(),
        "the test must exhaust a program resource"
    );
    assert_eq!(f.service.snapshot().tracks.len(), accepted + 1);
    assert!(
        f.service
            .snapshot()
            .tracks
            .iter()
            .map(|t| t.loops.len())
            .sum::<usize>()
            >= 65,
        "eight stereo tracks plus the sync loop must fit",
    );
    // A non-structural operation still works after the hardware admission refusal.
    let track = &f.service.snapshot().tracks[1];
    let renamed = f.submit(Command::Loop {
        track: track.id,
        loop_id: track.loops[0].id,
        action: LoopAction::NameChanged("Still alive".into()),
    });
    f.until(|f| f.client.results().contains_key(&renamed));
    assert_eq!(f.service.snapshot().tracks[1].loops[0].name, "Still alive");
}

#[shoop_test]
fn application_streams_large_graphs_and_retries_a_busy_activation_tail() {
    let mut f = Fixture::new();
    for _ in 0..3 {
        let added = f.add_track("");
        f.until(|f| f.client.results().contains_key(&added));
        assert!(matches!(
            f.client.results()[&added],
            Outcome::TrackCreated { .. }
        ));
    }
    while f
        .service
        .snapshot()
        .tracks
        .iter()
        .map(|t| t.loops.len())
        .sum::<usize>()
        < 31
    {
        let before = f.service.snapshot();
        let loops = before.tracks.iter().map(|t| t.loops.len()).sum::<usize>();
        if loops == 30 {
            f.gate.sent.lock().unwrap().clear();
            f.gate.tail_busy.store(true, Ordering::Relaxed);
        }
        let added = f.submit(Command::AddLoop {
            track: before.tracks[1].id,
        });
        f.until(|f| f.client.results().contains_key(&added));
        assert!(matches!(
            f.client.results()[&added],
            Outcome::LoopCreated { .. }
        ));
    }
    let batches = f.gate.sent.lock().unwrap();
    assert!(batches.len() > 2);
    assert!(batches
        .iter()
        .all(|b| b.commands().len() <= shoop_rt_protocol::immediate::COMMANDS));
    assert!(matches!(
        batches[0].commands()[0],
        shoop_rt_protocol::immediate::Command::BeginGraphImage { .. }
    ));
    assert!(activation(batches.last().unwrap()));
    assert_eq!(batches.iter().filter(|b| activation(b)).count(), 1);
    drop(batches);
    let state = f.service.snapshot();
    let added = f.submit(Command::AddLoop {
        track: state.tracks[1].id,
    });
    f.until(|f| f.client.results().contains_key(&added));
    assert!(matches!(
        f.client.results()[&added],
        Outcome::LoopCreated { .. }
    ));
    assert_eq!(
        f.service
            .snapshot()
            .tracks
            .iter()
            .map(|t| t.loops.len())
            .sum::<usize>(),
        32
    );
}

#[shoop_test]
fn stereo_activates_and_midi_refusal_restores_the_ui_layout() {
    let mut f = Fixture::new();
    let accepted = f.submit(Command::AddTrack(TrackSpec {
        name: "stereo".into(),
        topology: TrackSpecTopology::Direct {
            audio_channels: 2,
            midi: false,
        },
        initial_output_bus_names: vec!["Master".into()],
        latency: Default::default(),
        creation_request_id: None,
    }));
    f.until(|f| f.client.results().contains_key(&accepted));
    assert!(
        matches!(f.client.results()[&accepted], Outcome::TrackCreated { .. }),
        "{:?}",
        f.client.results()[&accepted]
    );
    for (audio_channels, midi, reason) in [(1, true, "MIDI")] {
        f.gate.sent.lock().unwrap().clear();
        let failed = f.submit(Command::AddTrack(TrackSpec {
            name: "unsupported".into(),
            topology: TrackSpecTopology::Direct {
                audio_channels,
                midi,
            },
            initial_output_bus_names: vec![],
            latency: Default::default(),
            creation_request_id: None,
        }));
        f.until(|f| f.client.results().contains_key(&failed));
        assert!(matches!(
            f.client.results()[&failed],
            Outcome::Refused { .. }
        ));
        assert!(format!("{:?}", f.client.results()[&failed]).contains(reason));
        assert!(f.gate.sent.lock().unwrap().iter().any(activation));
        assert_eq!(f.service.snapshot().tracks.len(), 2);
    }
    let track = f.service.snapshot().tracks[1].id;
    let accepted = f.submit(Command::AddLoop { track });
    f.until(|f| f.client.results().contains_key(&accepted));
    assert!(
        matches!(f.client.results()[&accepted], Outcome::LoopCreated { .. }),
        "{:?}",
        f.client.results()[&accepted]
    );
}
