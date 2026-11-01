use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_app_api::*;
use shoop_app_client::Client;
use shoop_app_protocol::*;
use shoop_rt_backend::{EngineClock, RtBackend};
use shoop_rt_engine::{testing::OfflineRender, EngineDescription, HostPorts, Segment};
use shoop_wasm_test_support::shoop_test;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

const HOST: HostId = HostId(100);
const SESSION: SessionId = SessionId(200);
const STEP: Duration = Duration::from_millis(16);

struct Clock {
    render: Rc<RefCell<OfflineRender>>,
    advance: Rc<Cell<bool>>,
    waits: Rc<Cell<usize>>,
}
impl EngineClock for Clock {
    fn advance(&mut self, elapsed: Duration) {
        if self.advance.get() {
            self.render
                .borrow_mut()
                .run_for(elapsed, |_, _| 0.25)
                .unwrap();
        }
    }
    fn wait(&mut self) -> bool {
        self.waits.set(self.waits.get() + 1);
        self.render.borrow_mut().run(16, |_, _| 0.25).unwrap();
        true
    }
}
struct Fixture {
    service: Service,
    render: Rc<RefCell<OfflineRender>>,
    advance: Rc<Cell<bool>>,
    waits: Rc<Cell<usize>>,
    initial_waits: usize,
    refuse_next_loop: Rc<Cell<bool>>,
}
fn fixture() -> Fixture {
    let description = EngineDescription {
        sample_rate: 1000,
        max_frames: 16,
        host: HostPorts::numbered(2, 2, 1),
        segments: vec![Segment {
            strips: 8,
            loops: 80,
        }],
        strip_channels: 2,
        master_channels: 2,
        master_connections: Vec::new(),
        ramp_frames: 4,
        resource_pages: 4096,
        page_bytes: 256,
        midi_events: 64,
        midi_bytes: 512,
        pending_transitions: 128,
        reliable_credits: 32,
        bulk_credits: 2,
        telemetry_interval_frames: 0,
        ..Default::default()
    };
    let (render, client) = OfflineRender::new(&description).unwrap();
    let render = Rc::new(RefCell::new(render));
    let advance = Rc::new(Cell::new(true));
    let waits = Rc::new(Cell::new(0));
    let clock = Clock {
        render: Rc::clone(&render),
        advance: Rc::clone(&advance),
        waits: Rc::clone(&waits),
    };
    let refuse_next_loop = Rc::new(Cell::new(false));
    let client = shoop_rt_transport::client::ProtocolClient::new(RefuseLoop {
        inner: client.into_carrier(),
        refuse: Rc::clone(&refuse_next_loop),
        reply: None,
    });
    let backend = RtBackend::connect(client, clock).unwrap();
    let initial_waits = waits.get();
    let mut service = Service::new(
        Box::new(backend),
        ServiceOptions {
            host: HOST,
            session: SESSION,
            master_auto_connect: true,
            loop_smoothing_ms: 4,
        },
    )
    .unwrap();
    for _ in 0..100 {
        service.tick(STEP);
        if service.is_ready() {
            break;
        }
    }
    assert!(service.is_ready());
    assert_eq!(
        waits.get(),
        initial_waits,
        "the app service must not synchronously wait for engine progress"
    );
    Fixture {
        service,
        render,
        advance,
        waits,
        initial_waits,
        refuse_next_loop,
    }
}
fn wire_message(client: &mut Client, message: WireMessage) {
    let mut bytes = [0; shoop_transport::MAX_RECORD_BYTES];
    let used = codec::encode(&message, &mut bytes).unwrap();
    client
        .receive_wire(codec::decode(&bytes[..used]).unwrap())
        .unwrap();
}
fn deliver(client: &mut Client, message: ServiceMessage, encoded: bool) {
    if !encoded {
        client.receive(message).unwrap();
        return;
    }
    let wire = match message {
        ServiceMessage::Attached { authority, host } => WireMessage::Attached { authority, host },
        ServiceMessage::Rejected(reason) => WireMessage::Rejected(reason),
        ServiceMessage::Accepted {
            authority,
            operation,
        } => WireMessage::Accepted {
            authority,
            operation,
        },
        ServiceMessage::Finished { authority, result } => {
            WireMessage::Finished { authority, result }
        }
        ServiceMessage::Detached { authority } => WireMessage::Detached { authority },
        ServiceMessage::ShutdownComplete { authority, result } => {
            WireMessage::ShutdownComplete { authority, result }
        }
        ServiceMessage::Snapshot { authority, state } => {
            let bytes = codec::encode_snapshot(&state).unwrap();
            wire_message(
                client,
                WireMessage::SnapshotBegin {
                    authority,
                    transfer: TransferId(1),
                    revision: state.revision,
                    bytes: bytes.len() as u32,
                },
            );
            for (index, bytes) in bytes.chunks(CHUNK_BYTES).enumerate() {
                wire_message(
                    client,
                    WireMessage::SnapshotChunk {
                        transfer: TransferId(1),
                        index: index as u32,
                        bytes: bytes.to_vec(),
                    },
                );
            }
            wire_message(
                client,
                WireMessage::SnapshotEnd {
                    transfer: TransferId(1),
                },
            );
            return;
        }
    };
    wire_message(client, wire);
}
fn drain(service: &mut Service, client: &mut Client, connection: ConnectionId, encoded: bool) {
    while let Some(message) = service.take_update(connection) {
        deliver(client, message, encoded);
    }
}
fn attach(service: &mut Service, id: u128, connection: ConnectionId, encoded: bool) -> Client {
    let mut client = Client::new(ClientId(id), HOST, SESSION).unwrap();
    service
        .handle(connection, client.attach().unwrap())
        .unwrap();
    drain(service, &mut client, connection, encoded);
    service
        .handle(connection, client.ready_message().unwrap())
        .unwrap();
    client.ready_sent().unwrap();
    client
}
fn submit(
    service: &mut Service,
    client: &mut Client,
    connection: ConnectionId,
    command: Command,
) -> OperationId {
    let message = client.command(command, None).unwrap();
    let ClientMessage::Submit { operation, .. } = &message else {
        unreachable!()
    };
    let id = *operation;
    service.handle(connection, message).unwrap();
    id
}
fn complete(
    service: &mut Service,
    client: &mut Client,
    connection: ConnectionId,
    id: OperationId,
    encoded: bool,
) -> Outcome {
    for _ in 0..100 {
        drain(service, client, connection, encoded);
        if let Some(outcome) = client.results().get(&id) {
            return outcome.clone();
        }
        service.tick(STEP);
    }
    panic!("operation did not complete: {id:?}");
}
fn command(
    service: &mut Service,
    client: &mut Client,
    connection: ConnectionId,
    value: Command,
) -> Outcome {
    let id = submit(service, client, connection, value);
    complete(service, client, connection, id, false)
}
fn new_track(name: &str, midi: bool) -> Command {
    Command::AddTrack(TrackSpec {
        name: name.to_owned(),
        topology: TrackSpecTopology::Direct {
            audio_channels: 1,
            midi,
        },
        latency: Default::default(),
        initial_output_bus_names: vec!["Master".to_owned()],
        creation_request_id: None,
    })
}
fn created(outcome: Outcome) -> (TrackId, Vec<LoopId>) {
    let Outcome::TrackCreated { track, loops, .. } = outcome else {
        panic!("{outcome:?}")
    };
    (track, loops)
}

#[shoop_test]
fn shared_state_and_ordered_selection_survive_attachment_in_typed_and_encoded_delivery() {
    for encoded in [false, true] {
        let mut f = fixture();
        let connection = ConnectionId(1);
        let mut a = attach(&mut f.service, 10, connection, encoded);
        let id = submit(
            &mut f.service,
            &mut a,
            connection,
            new_track("First", false),
        );
        let (first, loops) = created(complete(&mut f.service, &mut a, connection, id, encoded));
        let id = submit(
            &mut f.service,
            &mut a,
            connection,
            new_track("Second", false),
        );
        let (second, _) = created(complete(&mut f.service, &mut a, connection, id, encoded));
        command(
            &mut f.service,
            &mut a,
            connection,
            Command::Track {
                track: first,
                action: TrackAction::NameChanged("Kept name".to_owned()),
            },
        );
        command(
            &mut f.service,
            &mut a,
            connection,
            Command::Track {
                track: second,
                action: TrackAction::MoveBefore(Some(first)),
            },
        );
        command(
            &mut f.service,
            &mut a,
            connection,
            Command::Global(GlobalControlAction::SetSync(false)),
        );
        let selected = submit(
            &mut f.service,
            &mut a,
            connection,
            Command::SelectLoops {
                loops: vec![loops[0]],
            },
        );
        let play = submit(&mut f.service, &mut a, connection, Command::PlaySelected);
        complete(&mut f.service, &mut a, connection, selected, encoded);
        complete(&mut f.service, &mut a, connection, play, encoded);
        command(
            &mut f.service,
            &mut a,
            connection,
            Command::SetTarget(Some(loops[1])),
        );
        command(
            &mut f.service,
            &mut a,
            connection,
            Command::Global(GlobalControlAction::SetSolo(true)),
        );
        command(
            &mut f.service,
            &mut a,
            connection,
            Command::Global(GlobalControlAction::SetAutoMuteOtherTrackInputs(true)),
        );
        let before = f.service.snapshot();
        f.service.detach(connection);
        let b = attach(&mut f.service, 11, ConnectionId(2), encoded);
        let after = b.snapshot().unwrap();
        assert_eq!(
            after
                .tracks
                .iter()
                .map(|track| track.id)
                .collect::<Vec<_>>(),
            [TrackId::from_raw(1), second, first]
        );
        assert_eq!(after.tracks[2].name, "Kept name");
        assert!(after.tracks[2].loops[0].selected);
        assert!(after.tracks[2].loops[1].targeted);
        assert!(after.global.solo && after.global.auto_mute_other_track_inputs);
        assert!(!after.global.sync);
        assert_eq!(before.tracks[2].loops[0].id, after.tracks[2].loops[0].id);
        assert_eq!(after.tracks.iter().filter(|track| track.is_sync).count(), 1);
        assert_eq!(f.waits.get(), f.initial_waits);
    }
}

#[shoop_test]
fn accepted_creation_and_lost_replies_remain_owned_until_confirmation_without_replay() {
    let mut f = fixture();
    let connection = ConnectionId(1);
    let mut a = attach(&mut f.service, 10, connection, false);
    f.advance.set(false);
    let id = submit(
        &mut f.service,
        &mut a,
        connection,
        new_track("Admitted", false),
    );
    assert!(f
        .service
        .snapshot()
        .pending
        .iter()
        .any(|pending| pending.id == id));
    let stable = f.service.snapshot().tracks[1].id;
    f.service.detach(connection);
    drop(a);
    let mut b = attach(&mut f.service, 11, ConnectionId(2), false);
    assert_eq!(b.snapshot().unwrap().tracks[1].id, stable);
    assert!(b
        .snapshot()
        .unwrap()
        .pending
        .iter()
        .any(|pending| pending.id == id));
    command(
        &mut f.service,
        &mut b,
        ConnectionId(2),
        Command::Track {
            track: stable,
            action: TrackAction::NameChanged("Renamed before engine confirmation".to_owned()),
        },
    );
    f.advance.set(true);
    for _ in 0..100 {
        f.service.tick(STEP);
        if f.service.snapshot().pending.is_empty() {
            break;
        }
    }
    let snapshot = f.service.snapshot();
    assert!(snapshot.pending.is_empty());
    assert_eq!(snapshot.tracks.len(), 2);
    assert_eq!(snapshot.tracks[1].id, stable);
    assert_eq!(
        snapshot.tracks[1].name,
        "Renamed before engine confirmation"
    );
    assert!(
        matches!(snapshot.results.iter().find(|result| result.id == id).unwrap().outcome, Outcome::TrackCreated { track, .. } if track == stable)
    );
    f.service.detach(ConnectionId(2));
    let mut resumed = attach(&mut f.service, 10, ConnectionId(3), false);
    let authority = resumed.authority().unwrap();
    f.service
        .handle(
            ConnectionId(3),
            ClientMessage::Submit {
                authority,
                operation: id,
                expected_revision: None,
                command: new_track("Must not duplicate", false),
            },
        )
        .unwrap();
    drain(&mut f.service, &mut resumed, ConnectionId(3), false);
    f.service.tick(STEP);
    assert_eq!(f.service.snapshot().tracks.len(), 2);
    assert!(resumed.results().contains_key(&id));
}

#[shoop_test]
fn replacing_a_client_fences_old_mutations_detach_and_shutdown_without_disturbing_the_owner() {
    let mut f = fixture();
    let first = ConnectionId(1);
    let mut a = attach(&mut f.service, 10, first, false);
    let stale = a.authority().unwrap();
    assert_eq!(
        f.service.handle(
            ConnectionId(9),
            ClientMessage::Attach {
                version: VERSION,
                host: HOST,
                session: SESSION,
                client: ClientId(99)
            }
        ),
        Err(Refusal::Busy)
    );
    f.service.detach(first);
    let mut b = attach(&mut f.service, 11, ConnectionId(2), false);
    let before = f.service.snapshot();
    let message = a
        .command(Command::Global(GlobalControlAction::SetSolo(true)), None)
        .unwrap();
    assert_eq!(
        f.service.handle(first, message),
        Err(Refusal::StaleAuthority)
    );
    assert_eq!(
        f.service.handle(
            first,
            ClientMessage::Shutdown {
                authority: stale,
                operation: OperationId {
                    client: stale.client,
                    sequence: 100
                }
            }
        ),
        Err(Refusal::StaleAuthority)
    );
    f.service.detach(first);
    assert!(f.service.has_connection(ConnectionId(2)));
    assert_eq!(f.service.snapshot().revision, before.revision);
    assert!(!f.service.snapshot().global.solo);
    assert!(matches!(
        command(
            &mut f.service,
            &mut b,
            ConnectionId(2),
            Command::Global(GlobalControlAction::SetSolo(true))
        ),
        Outcome::Confirmed { .. }
    ));
    assert!(f.service.snapshot().global.solo);
}

#[shoop_test]
fn active_audio_and_midi_recording_continues_through_client_loss_and_media_is_not_reuploaded() {
    let mut f = fixture();
    let mut a = attach(&mut f.service, 10, ConnectionId(1), false);
    command(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        Command::Global(GlobalControlAction::SetSync(false)),
    );
    let (track, loops) = created(command(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        new_track("Audio and MIDI", true),
    ));
    command(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::RecordClicked,
        },
    );
    let before = f.render.borrow().next_frame();
    let identity = f.service.snapshot().tracks[1].loops[0].id;
    f.service.detach(ConnectionId(1));
    f.render
        .borrow_mut()
        .midi_input_mut(0)
        .push(0, 16, &[0x90, 60, 100])
        .unwrap();
    for _ in 0..4 {
        f.service.tick(STEP);
    }
    f.render
        .borrow_mut()
        .midi_input_mut(0)
        .push(0, 16, &[0x80, 60, 0])
        .unwrap();
    for _ in 0..4 {
        f.service.tick(STEP);
    }
    assert!(f.render.borrow().next_frame() > before + 100);
    let mut b = attach(&mut f.service, 11, ConnectionId(2), false);
    assert_eq!(
        b.snapshot().unwrap().tracks[1].loops[0].mode,
        LoopMode::Recording
    );
    assert_eq!(b.snapshot().unwrap().tracks[1].loops[0].id, identity);
    command(
        &mut f.service,
        &mut b,
        ConnectionId(2),
        Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::PlayClicked,
        },
    );
    for _ in 0..4 {
        f.service.tick(STEP);
    }
    assert!(f
        .render
        .borrow()
        .output(0)
        .iter()
        .any(|sample| sample.abs() > 0.1));
    let authority = b.authority().unwrap();
    let mut audio = None;
    let mut midi = None;
    for _ in 0..100 {
        audio = f
            .service
            .read_audio(ConnectionId(2), authority, loops[0])
            .unwrap()
            .or(audio);
        midi = f
            .service
            .read_midi(ConnectionId(2), authority, loops[0])
            .unwrap()
            .or(midi);
        if audio.is_some() && midi.is_some() {
            break;
        }
        f.service.tick(STEP);
    }
    let audio = audio.expect("recorded audio");
    assert!(audio.channels[0].samples.len() > 100);
    assert!(audio.channels[0]
        .samples
        .iter()
        .any(|sample| (*sample - 0.25).abs() < 1e-6));
    let midi = midi.expect("recorded MIDI");
    assert!(midi.channels[0]
        .events
        .iter()
        .any(|event| event.data == [0x90, 60, 100]));
    assert!(midi.channels[0]
        .events
        .iter()
        .any(|event| event.data == [0x80, 60, 0]));
    assert_eq!(f.waits.get(), f.initial_waits);
}

#[shoop_test]
fn snapshots_order_pending_work_and_engine_refusals_across_the_attach_boundary() {
    let mut f = fixture();
    let mut a = attach(&mut f.service, 10, ConnectionId(1), false);
    command(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        Command::Global(GlobalControlAction::SetSync(false)),
    );
    let (_, loops) = created(command(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        new_track("Refusal", false),
    ));
    command(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        Command::SetTarget(Some(loops[0])),
    );
    command(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        Command::RecordWithTarget {
            loops: vec![loops[1]],
        },
    );
    command(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        Command::SetTarget(Some(loops[1])),
    );
    f.advance.set(false);
    let refused = submit(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        Command::RecordWithTarget {
            loops: vec![loops[0]],
        },
    );
    f.service.detach(ConnectionId(1));
    let mut b = Client::new(ClientId(11), HOST, SESSION).unwrap();
    f.service
        .handle(ConnectionId(2), b.attach().unwrap())
        .unwrap();
    f.advance.set(true);
    for _ in 0..10 {
        f.service.tick(STEP);
    }
    drain(&mut f.service, &mut b, ConnectionId(2), true);
    f.service
        .handle(ConnectionId(2), b.ready_message().unwrap())
        .unwrap();
    b.ready_sent().unwrap();
    let state = b.snapshot().unwrap();
    assert!(state.pending.is_empty());
    assert!(matches!(
        state
            .results
            .iter()
            .find(|result| result.id == refused)
            .unwrap()
            .outcome,
        Outcome::Refused {
            reason: Refusal::Engine,
            ..
        }
    ));
    assert_eq!(state.tracks.len(), 2);
}

#[shoop_test]
fn a_slow_client_is_reclaimed_while_accepted_work_and_outcomes_remain_bounded() {
    let mut f = fixture();
    let mut a = attach(&mut f.service, 10, ConnectionId(1), false);
    let (track, loops) = created(command(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        new_track("Bounded", false),
    ));
    let authority = a.authority().unwrap();
    f.advance.set(false);
    for index in 0..MAX_PENDING_OPERATIONS {
        submit(
            &mut f.service,
            &mut a,
            ConnectionId(1),
            Command::Loop {
                track,
                loop_id: loops[0],
                action: LoopAction::GainChanged(index as f32 / MAX_PENDING_OPERATIONS as f32),
            },
        );
    }
    let pending = f.service.snapshot().pending.clone();
    assert_eq!(pending.len(), MAX_PENDING_OPERATIONS);
    f.service
        .handle(
            ConnectionId(1),
            ClientMessage::Submit {
                authority,
                operation: OperationId {
                    client: authority.client,
                    sequence: 1000,
                },
                expected_revision: None,
                command: Command::ResetXruns,
            },
        )
        .unwrap();
    assert!(!f.service.has_connection(ConnectionId(1)));
    assert_eq!(f.service.snapshot().pending.len(), MAX_PENDING_OPERATIONS);
    f.advance.set(true);
    for _ in 0..100 {
        f.service.tick(STEP);
        if f.service.snapshot().pending.is_empty() {
            break;
        }
    }
    assert!(f.service.snapshot().pending.is_empty());
    assert!(f.service.snapshot().results.len() <= MAX_RETAINED_RESULTS);
    let b = attach(&mut f.service, 11, ConnectionId(2), false);
    assert_eq!(b.snapshot().unwrap().tracks.len(), 2);
    assert!(b.snapshot().unwrap().results.iter().any(|result| matches!(
        result.outcome,
        Outcome::Refused {
            reason: Refusal::Overloaded,
            ..
        }
    )));
}

#[shoop_test]
fn shutdown_waits_for_engine_outcomes_and_host_cleanup_and_allows_failure_retry() {
    let mut f = fixture();
    let mut a = attach(&mut f.service, 10, ConnectionId(1), false);
    let request = a.shutdown().unwrap();
    let ClientMessage::Shutdown { operation, .. } = request else {
        unreachable!()
    };
    f.service.handle(ConnectionId(1), request).unwrap();
    assert!(!f.service.shutdown_ready());
    for _ in 0..100 {
        f.service.tick(STEP);
        if f.service.shutdown_ready() {
            break;
        }
    }
    assert!(f.service.shutdown_ready());
    assert!(f
        .service
        .snapshot()
        .pending
        .iter()
        .any(|pending| pending.id == operation));
    assert!(!f
        .service
        .snapshot()
        .results
        .iter()
        .any(|result| result.id == operation));
    f.service
        .complete_shutdown(Err("mock device stop failure".to_owned()));
    drain(&mut f.service, &mut a, ConnectionId(1), false);
    assert!(matches!(
        a.results().get(&operation),
        Some(Outcome::Refused {
            reason: Refusal::ShutdownFailed,
            ..
        })
    ));
    assert!(f.service.is_ready());
    let request = a.shutdown().unwrap();
    f.service.handle(ConnectionId(1), request).unwrap();
    for _ in 0..100 {
        f.service.tick(STEP);
        if f.service.shutdown_ready() {
            break;
        }
    }
    f.service.complete_shutdown(Ok(()));
    assert!(!f.service.is_ready());
    assert_eq!(
        f.service.snapshot().status.audio_driver,
        AudioDriverState::Stopped
    );
}

struct RefuseLoop<C> {
    inner: C,
    refuse: Rc<Cell<bool>>,
    reply: Option<shoop_rt_protocol::EngineMessage>,
}
impl<C: shoop_rt_transport::carrier::Carrier> shoop_rt_transport::carrier::Carrier
    for RefuseLoop<C>
{
    fn send(
        &mut self,
        message: shoop_rt_protocol::ClientMessage,
    ) -> Result<(), shoop_transport::CarrierError> {
        if let shoop_rt_protocol::ClientMessage::Request {
            correlation,
            request: shoop_rt_protocol::Request::CreateLoop { .. },
        } = message
        {
            if self.refuse.replace(false) {
                assert!(self.reply.is_none());
                self.reply = Some(shoop_rt_protocol::EngineMessage::Result {
                    correlation,
                    outcome: shoop_rt_protocol::Outcome::Refused(
                        shoop_rt_protocol::Refusal::Capacity,
                    ),
                });
                return Ok(());
            }
        }
        self.inner.send(message)
    }
    fn receive(&mut self) -> Option<shoop_rt_protocol::EngineMessage> {
        self.reply.take().or_else(|| self.inner.receive())
    }
}

#[shoop_test]
fn partial_creation_refusal_retains_manageable_survivors_and_reconciles_every_outcome() {
    let mut f = fixture();
    let mut a = attach(&mut f.service, 10, ConnectionId(1), false);
    f.refuse_next_loop.set(true);
    let op = submit(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        new_track("Partial", false),
    );
    f.service.detach(ConnectionId(1));
    for _ in 0..100 {
        f.service.tick(STEP);
    }
    let mut b = attach(&mut f.service, 11, ConnectionId(2), true);
    let state = b.snapshot().unwrap();
    assert!(state.pending.is_empty());
    assert!(matches!(
        state
            .results
            .iter()
            .find(|result| result.id == op)
            .unwrap()
            .outcome,
        Outcome::Refused {
            reason: Refusal::Engine,
            ..
        }
    ));
    assert_eq!(state.tracks.len(), 2);
    assert_eq!(state.tracks[1].loops.len(), 7);
    assert!(state.tracks[1]
        .loops
        .iter()
        .all(|entry| entry.structural_state == StructuralState::Confirmed));
    let track = state.tracks[1].id;
    assert!(matches!(
        command(
            &mut f.service,
            &mut b,
            ConnectionId(2),
            Command::Track {
                track,
                action: TrackAction::NameChanged("Managed".into())
            }
        ),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        command(
            &mut f.service,
            &mut b,
            ConnectionId(2),
            Command::Track {
                track,
                action: TrackAction::Remove
            }
        ),
        Outcome::Confirmed { .. }
    ));
    assert_eq!(f.service.snapshot().tracks.len(), 1);
}

#[shoop_test]
fn routing_and_removal_accepted_before_client_loss_settle_without_reinitialization() {
    let mut f = fixture();
    let mut a = attach(&mut f.service, 10, ConnectionId(1), false);
    let (track, _) = created(command(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        new_track("Route", false),
    ));
    let route = f.service.snapshot().connections.mixer_links[0];
    f.advance.set(false);
    let routed = submit(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        Command::SetMixerRoute {
            source: route.source_port_id,
            destination: route.destination_channel_id,
            connected: false,
        },
    );
    f.service.detach(ConnectionId(1));
    let mut b = attach(&mut f.service, 11, ConnectionId(2), true);
    assert!(b
        .snapshot()
        .unwrap()
        .pending
        .iter()
        .any(|op| op.id == routed));
    f.advance.set(true);
    for _ in 0..20 {
        f.service.tick(STEP);
    }
    drain(&mut f.service, &mut b, ConnectionId(2), false);
    assert!(!b
        .snapshot()
        .unwrap()
        .connections
        .mixer_links
        .contains(&route));
    assert!(b
        .snapshot()
        .unwrap()
        .connections
        .pending_mixer_links
        .is_empty());
    f.advance.set(false);
    let removed = submit(
        &mut f.service,
        &mut b,
        ConnectionId(2),
        Command::Track {
            track,
            action: TrackAction::Remove,
        },
    );
    f.service.detach(ConnectionId(2));
    f.advance.set(true);
    for _ in 0..20 {
        f.service.tick(STEP);
    }
    let mut c = attach(&mut f.service, 12, ConnectionId(3), false);
    assert_eq!(c.snapshot().unwrap().tracks.len(), 1);
    assert!(c
        .snapshot()
        .unwrap()
        .results
        .iter()
        .any(|result| result.id == removed && matches!(result.outcome, Outcome::Confirmed { .. })));
    let (next, _) = created(command(
        &mut f.service,
        &mut c,
        ConnectionId(3),
        new_track("Next", false),
    ));
    assert!(next.raw() > track.raw());
}

#[shoop_test]
fn revision_conflicts_and_evicted_results_cannot_replay_accepted_work() {
    let mut f = fixture();
    let mut a = attach(&mut f.service, 10, ConnectionId(1), false);
    let message = a
        .command(new_track("Conflict", false), Some(Revision(0)))
        .unwrap();
    let ClientMessage::Submit { operation, .. } = message.clone() else {
        unreachable!()
    };
    f.service.handle(ConnectionId(1), message.clone()).unwrap();
    assert!(matches!(
        complete(&mut f.service, &mut a, ConnectionId(1), operation, false),
        Outcome::Refused {
            reason: Refusal::RevisionConflict,
            ..
        }
    ));
    for _ in 0..MAX_RETAINED_RESULTS + 1 {
        command(&mut f.service, &mut a, ConnectionId(1), Command::ResetXruns);
    }
    assert_eq!(f.service.snapshot().results.len(), MAX_RETAINED_RESULTS);
    f.service.handle(ConnectionId(1), message).unwrap();
    let reply = f.service.take_update(ConnectionId(1)).unwrap();
    assert!(
        matches!(reply, ServiceMessage::Finished {result: OperationResult {id, outcome: Outcome::Refused { reason: Refusal::Duplicate, .. }}, ..} if id == operation)
    );
    assert_eq!(f.service.snapshot().tracks.len(), 1);
}
