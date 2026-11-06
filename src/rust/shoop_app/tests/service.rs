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
    sync::{Arc, Mutex},
    time::Duration,
};

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

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
    refuse_next_bus: Rc<Cell<bool>>,
    refuse_next_lifecycle: Rc<Cell<bool>>,
    refuse_composite_after: Rc<Cell<Option<u32>>>,
    refuse_next_monitoring: Rc<Cell<bool>>,
}
fn fixture() -> Fixture {
    fixture_with_inventory(None)
}
fn fixture_with_inventory(
    inventory: Option<Box<dyn shoop_rt_backend::HostPortInventory>>,
) -> Fixture {
    fixture_with_options(2, inventory)
}
fn fixture_with_outputs(outputs: usize) -> Fixture {
    fixture_with_options(outputs, None)
}
fn fixture_with_dry_wet(outputs: usize, dry_wet: bool) -> Fixture {
    fixture_configured(outputs, dry_wet, None)
}
fn fixture_with_options(
    outputs: usize,
    inventory: Option<Box<dyn shoop_rt_backend::HostPortInventory>>,
) -> Fixture {
    fixture_configured(outputs, false, inventory)
}
fn fixture_configured(
    outputs: usize,
    dry_wet: bool,
    inventory: Option<Box<dyn shoop_rt_backend::HostPortInventory>>,
) -> Fixture {
    let description = EngineDescription {
        external_dry_wet: dry_wet,
        quantum: 1,
        sample_rate: 1000,
        max_frames: 16,
        host: HostPorts::numbered(2, outputs, 1),
        segments: vec![Segment {
            strips: 8,
            loops: 80,
        }],
        strip_channels: 2,
        master_channels: 2,
        buses: 8,
        bus_channels: 8,
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
    let refuse_next_bus = Rc::new(Cell::new(false));
    let refuse_next_lifecycle = Rc::new(Cell::new(false));
    let refuse_composite_after = Rc::new(Cell::new(None));
    let refuse_next_monitoring = Rc::new(Cell::new(false));
    let client = shoop_rt_transport::client::ProtocolClient::new(RefuseLoop {
        inner: client.into_carrier(),
        refuse: Rc::clone(&refuse_next_loop),
        refuse_bus: Rc::clone(&refuse_next_bus),
        refuse_lifecycle: Rc::clone(&refuse_next_lifecycle),
        refuse_composite_after: Rc::clone(&refuse_composite_after),
        refuse_monitoring: Rc::clone(&refuse_next_monitoring),
        reply: None,
    });
    let mut backend = RtBackend::connect(client, clock).unwrap();
    if let Some(inventory) = inventory {
        backend = backend.with_host_port_inventory(inventory);
    }
    let initial_waits = waits.get();
    let mut service = Service::new(
        Box::new(backend),
        ServiceOptions {
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
        refuse_next_bus,
        refuse_next_lifecycle,
        refuse_composite_after,
        refuse_next_monitoring,
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
    let mut stream = stream::StreamEncoder::new(message, TransferId(1)).unwrap();
    while let Some(message) = stream.next_message() {
        wire_message(client, message);
    }
}

fn drain(service: &mut Service, client: &mut Client, connection: ConnectionId, encoded: bool) {
    while let Some(message) = service.take_update(connection) {
        deliver(client, message, encoded);
    }
}
fn attach(service: &mut Service, _id: u128, connection: ConnectionId, encoded: bool) -> Client {
    let mut client = Client::new();
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

#[shoop_test]
fn fixed_cycle_recording_resolves_each_tracks_default_post_record_playback() {
    let mut f = fixture_with_dry_wet(2, true);
    let connection = ConnectionId(1);
    let mut client = attach(&mut f.service, 1, connection, true);
    let run = |f: &mut Fixture, client: &mut Client, value| {
        let id = submit(&mut f.service, client, connection, value);
        complete(&mut f.service, client, connection, id, true)
    };
    run(
        &mut f,
        &mut client,
        Command::Global(GlobalControlAction::SetSync(false)),
    );
    let sync = client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .find(|t| t.is_sync)
        .unwrap()
        .clone();
    let sync_action = |action| Command::Loop {
        track: sync.id,
        loop_id: sync.loops[0].id,
        action,
    };
    run(&mut f, &mut client, sync_action(LoopAction::RecordClicked));
    for _ in 0..8 {
        f.service.tick(STEP);
    }
    run(&mut f, &mut client, sync_action(LoopAction::StopClicked));
    run(&mut f, &mut client, sync_action(LoopAction::PlayClicked));
    let (track, loops) = created(run(
        &mut f,
        &mut client,
        Command::AddTrack(TrackSpec {
            name: "Post-record defaults".into(),
            topology: TrackSpecTopology::DryWet {
                dry_audio_channels: 1,
                wet_audio_channels: 2,
                dry_midi: true,
                processor_type: TrackProcessorTypeId::new(TrackProcessorTypeId::EXTERNAL),
                default_playback_mode: DefaultPlaybackMode::Regular,
            },
            latency: Default::default(),
            initial_output_bus_names: vec!["Master".into()],
            creation_request_id: None,
        }),
    ));
    run(
        &mut f,
        &mut client,
        Command::Global(GlobalControlAction::SetSync(true)),
    );
    run(
        &mut f,
        &mut client,
        Command::Global(GlobalControlAction::SetApplyNCycles(1)),
    );
    for (index, (default, autoplay, expected)) in [
        (DefaultPlaybackMode::Regular, true, LoopMode::Playing),
        (
            DefaultPlaybackMode::DryThroughWet,
            true,
            LoopMode::PlayingDryThroughWet,
        ),
        (DefaultPlaybackMode::DryThroughWet, false, LoopMode::Stopped),
    ]
    .into_iter()
    .enumerate()
    {
        run(
            &mut f,
            &mut client,
            Command::Track {
                track,
                action: TrackAction::DefaultPlaybackModeChanged(default),
            },
        );
        run(
            &mut f,
            &mut client,
            Command::Global(GlobalControlAction::SetPlayAfterRecord(autoplay)),
        );
        assert!(matches!(
            run(
                &mut f,
                &mut client,
                Command::Loop {
                    track,
                    loop_id: loops[index],
                    action: LoopAction::DefaultClicked
                }
            ),
            Outcome::Confirmed { .. }
        ));
        let mut finished = false;
        for _ in 0..100 {
            f.service.tick(STEP);
            drain(&mut f.service, &mut client, connection, true);
            let state = client
                .snapshot()
                .unwrap()
                .tracks
                .iter()
                .find(|t| t.id == track)
                .unwrap();
            let state = &state.loops[index];
            if !state.empty && state.mode == expected && state.next_transition_delay.is_none() {
                finished = true;
                break;
            }
        }
        assert!(finished, "default={default:?} autoplay={autoplay}");
        assert!(matches!(
            run(
                &mut f,
                &mut client,
                Command::Loop {
                    track,
                    loop_id: loops[index],
                    action: LoopAction::PlayClicked
                }
            ),
            Outcome::Confirmed { .. }
        ));
        let mut explicit_wet = false;
        for _ in 0..100 {
            f.service.tick(STEP);
            drain(&mut f.service, &mut client, connection, true);
            let state = client
                .snapshot()
                .unwrap()
                .tracks
                .iter()
                .find(|t| t.id == track)
                .unwrap();
            if state.loops[index].mode == LoopMode::Playing {
                explicit_wet = true;
                break;
            }
        }
        assert!(explicit_wet);
    }
}

#[shoop_test]
fn dry_wet_service_orders_defaults_and_keeps_explicit_play_wet_over_the_wire() {
    for encoded in [false, true] {
        let mut f = fixture_with_dry_wet(2, true);
        let connection = ConnectionId(1);
        let mut client = attach(&mut f.service, 1, connection, encoded);
        let run = |f: &mut Fixture, client: &mut Client, command| {
            let id = submit(&mut f.service, client, connection, command);
            complete(&mut f.service, client, connection, id, encoded)
        };
        assert!(client
            .snapshot()
            .unwrap()
            .track_processors
            .iter()
            .any(|p| p.id.as_str() == TrackProcessorTypeId::EXTERNAL && p.available));
        run(
            &mut f,
            &mut client,
            Command::Global(GlobalControlAction::SetSync(false)),
        );
        let (track, loops) = created(run(
            &mut f,
            &mut client,
            Command::AddTrack(TrackSpec {
                name: "External".into(),
                topology: TrackSpecTopology::DryWet {
                    dry_audio_channels: 1,
                    wet_audio_channels: 2,
                    dry_midi: true,
                    processor_type: TrackProcessorTypeId::new(TrackProcessorTypeId::EXTERNAL),
                    default_playback_mode: DefaultPlaybackMode::Regular,
                },
                latency: Default::default(),
                initial_output_bus_names: vec!["Master".into()],
                creation_request_id: None,
            }),
        ));
        let action = |action| Command::Loop {
            track,
            loop_id: loops[0],
            action,
        };
        assert!(matches!(
            run(&mut f, &mut client, action(LoopAction::RecordClicked)),
            Outcome::Confirmed { .. }
        ));
        f.service.tick(STEP);
        assert!(matches!(
            run(&mut f, &mut client, action(LoopAction::StopClicked)),
            Outcome::Confirmed { .. }
        ));
        let mode = |client: &Client| {
            client
                .snapshot()
                .unwrap()
                .tracks
                .iter()
                .find(|t| t.id == track)
                .unwrap()
                .loops[0]
                .mode
        };
        for final_mode in [
            DefaultPlaybackMode::Regular,
            DefaultPlaybackMode::DryThroughWet,
        ] {
            let mut pending = Vec::new();
            for choice in [
                DefaultPlaybackMode::DryThroughWet,
                DefaultPlaybackMode::Regular,
                final_mode,
            ] {
                pending.push(submit(
                    &mut f.service,
                    &mut client,
                    connection,
                    Command::Track {
                        track,
                        action: TrackAction::DefaultPlaybackModeChanged(choice),
                    },
                ));
            }
            let playback = submit(
                &mut f.service,
                &mut client,
                connection,
                Command::Control(ControlOperation::TriggerDefaultPlayback {
                    loops: vec![loops[0]],
                }),
            );
            for id in pending.into_iter().chain([playback]) {
                assert!(matches!(
                    complete(&mut f.service, &mut client, connection, id, encoded),
                    Outcome::Confirmed { .. }
                ));
            }
            assert_eq!(
                mode(&client),
                if final_mode == DefaultPlaybackMode::Regular {
                    LoopMode::Playing
                } else {
                    LoopMode::PlayingDryThroughWet
                }
            );
            assert!(matches!(
                run(&mut f, &mut client, action(LoopAction::StopClicked)),
                Outcome::Confirmed { .. }
            ));
        }
        assert!(matches!(
            run(&mut f, &mut client, action(LoopAction::PlayClicked)),
            Outcome::Confirmed { .. }
        ));
        assert_eq!(mode(&client), LoopMode::Playing);
        assert!(matches!(
            run(&mut f, &mut client, action(LoopAction::StopClicked)),
            Outcome::Confirmed { .. }
        ));
        assert!(matches!(
            run(&mut f, &mut client, action(LoopAction::DefaultClicked)),
            Outcome::Confirmed { .. }
        ));
        assert_eq!(mode(&client), LoopMode::PlayingDryThroughWet);
        let projected = client
            .snapshot()
            .unwrap()
            .tracks
            .iter()
            .find(|t| t.id == track)
            .unwrap()
            .clone();
        assert!(matches!(
            projected.topology,
            TrackTopology::DryWet {
                dry_audio_channels: 1,
                wet_audio_channels: 2,
                ..
            }
        ));
        assert!(!projected.controls.input_stereo);
        assert!(projected.controls.output_stereo);
        let snapshot = client.snapshot().unwrap();
        let wet = snapshot
            .connections
            .application_ports
            .iter()
            .filter(|port| {
                projected.port_ids.contains(&port.id) && port.role == PortRole::AudioOutput
            })
            .map(|port| port.id)
            .collect::<Vec<_>>();
        let routed = snapshot
            .connections
            .mixer_links
            .iter()
            .filter(|link| projected.port_ids.contains(&link.source_port_id))
            .collect::<Vec<_>>();
        assert_eq!(routed.len(), 2);
        assert!(routed.iter().all(|link| wet.contains(&link.source_port_id)));
    }
}

#[shoop_test]
fn buses_round_trip_order_routes_zero_and_reconcile_admitted_mutations_after_ui_loss() {
    for encoded in [false, true] {
        let mut f = fixture();
        let mut client = attach(&mut f.service, 6001, ConnectionId(1), encoded);
        let run = |f: &mut Fixture, client: &mut Client, value| {
            let id = submit(&mut f.service, client, ConnectionId(1), value);
            complete(&mut f.service, client, ConnectionId(1), id, encoded)
        };
        for (name, channel_count) in [("Cue", 2), ("Wide", 8)] {
            assert!(matches!(
                run(
                    &mut f,
                    &mut client,
                    Command::AddBus(BusSpec {
                        name: name.into(),
                        channel_count,
                        creation_request_id: None,
                        fx: None
                    })
                ),
                Outcome::Confirmed { .. }
            ));
        }
        let buses = client.snapshot().unwrap().buses.clone();
        let master = buses[0].id;
        let cue = buses[1].id;
        let wide = buses[2].id;
        assert!(matches!(
            run(
                &mut f,
                &mut client,
                Command::Control(ControlOperation::SetBusBalance {
                    buses: vec![cue, wide],
                    balance: 0.5,
                })
            ),
            Outcome::Refused { .. }
        ));
        for _ in 0..20 {
            f.service.tick(STEP);
        }
        drain(&mut f.service, &mut client, ConnectionId(1), encoded);
        assert!(client
            .snapshot()
            .unwrap()
            .buses
            .iter()
            .all(|bus| bus.balance == 0.));
        for operation in [
            ControlOperation::SetBusGain {
                buses: vec![cue, wide],
                gain_db: -6.,
            },
            ControlOperation::SetBusMuted {
                buses: vec![cue, wide],
                muted: true,
            },
        ] {
            assert!(matches!(
                run(&mut f, &mut client, Command::Control(operation)),
                Outcome::Confirmed { .. }
            ));
        }
        assert!(client.snapshot().unwrap().buses[1..]
            .iter()
            .all(|bus| bus.gain_db == -6. && bus.muted));
        assert!(matches!(
            run(
                &mut f,
                &mut client,
                Command::Bus {
                    bus: wide,
                    action: BusAction::MoveBefore(Some(master))
                }
            ),
            Outcome::Confirmed { .. }
        ));
        for action in [
            BusAction::GainChanged(-6.),
            BusAction::BalanceChanged(0.25),
            BusAction::MuteChanged(true),
        ] {
            assert!(matches!(
                run(&mut f, &mut client, Command::Bus { bus: cue, action }),
                Outcome::Confirmed { .. }
            ));
        }
        let Command::AddTrack(mut spec) = new_track("Fanout", false) else {
            unreachable!()
        };
        spec.initial_output_bus_names = vec![
            "Master".into(),
            "Cue".into(),
            "Wide".into(),
            "Missing".into(),
        ];
        created(run(&mut f, &mut client, Command::AddTrack(spec)));
        assert_eq!(client.snapshot().unwrap().connections.mixer_links.len(), 12);
        let old_ids = client
            .snapshot()
            .unwrap()
            .buses
            .iter()
            .map(|b| b.id)
            .collect::<Vec<_>>();
        let image = capture_image(&mut f.service, &mut client, 1, encoded);
        release_image(&mut f.service, &mut client, 1, 1, encoded);
        let key = stage_image(&mut f.service, &mut client, 2, image, encoded);
        assert!(matches!(
            run(&mut f, &mut client, Command::CommitPersistence(key)),
            Outcome::Confirmed { .. }
        ));
        let loaded = client.snapshot().unwrap();
        let mut logical_order = loaded.buses.iter().collect::<Vec<_>>();
        logical_order.sort_by_key(|bus| bus.id);
        assert_eq!(
            logical_order
                .iter()
                .map(|bus| bus.name.as_str())
                .collect::<Vec<_>>(),
            ["Master", "Cue", "Wide"]
        );
        assert_eq!(
            loaded
                .buses
                .iter()
                .map(|b| b.name.as_str())
                .collect::<Vec<_>>(),
            ["Wide", "Master", "Cue"]
        );
        assert!(loaded.buses.iter().all(|b| !old_ids.contains(&b.id)));
        assert_eq!(loaded.connections.mixer_links.len(), 12);
        assert_eq!(loaded.buses[2].gain_db, -6.);
        assert_eq!(loaded.buses[2].balance, 0.25);
        assert!(loaded.buses[2].muted);
        assert_eq!(loaded.buses[0].gain_db, -6.);
        assert!(loaded.buses[0].muted);
        let ids = loaded.buses.iter().map(|b| b.id).collect::<Vec<_>>();
        for bus in ids {
            assert!(matches!(
                run(
                    &mut f,
                    &mut client,
                    Command::Bus {
                        bus,
                        action: BusAction::Remove
                    }
                ),
                Outcome::Confirmed { .. }
            ));
        }
        assert!(client.snapshot().unwrap().buses.is_empty());
        assert!(client
            .snapshot()
            .unwrap()
            .connections
            .mixer_links
            .is_empty());
        let image = capture_image(&mut f.service, &mut client, 3, encoded);
        release_image(&mut f.service, &mut client, 3, 2, encoded);
        let persistence_image::Manifest::Session(session) = &image.manifest else {
            panic!()
        };
        assert!(session.buses.is_empty() && session.bus_display_order.is_empty());
        let key = stage_image(&mut f.service, &mut client, 4, image, encoded);
        assert!(matches!(
            run(&mut f, &mut client, Command::CommitPersistence(key)),
            Outcome::Confirmed { .. }
        ));
        assert!(client.snapshot().unwrap().buses.is_empty());
        let accepted = submit(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::AddBus(BusSpec {
                name: "After loss".into(),
                channel_count: 1,
                creation_request_id: None,
                fx: None,
            }),
        );
        f.service.detach(ConnectionId(1));
        for _ in 0..20 {
            f.service.tick(STEP);
        }
        assert!(f
            .service
            .snapshot()
            .results
            .iter()
            .any(|r| r.id == accepted && matches!(r.outcome, Outcome::Confirmed { .. })));
        client = attach(&mut f.service, 6002, ConnectionId(1), encoded);
        assert_eq!(client.snapshot().unwrap().buses.len(), 1);
        assert_eq!(client.snapshot().unwrap().buses[0].name, "After loss");
        let bus = client.snapshot().unwrap().buses[0].id;
        for (index, action) in [
            BusAction::GainChanged(-9.),
            BusAction::MuteChanged(true),
            BusAction::Remove,
        ]
        .into_iter()
        .enumerate()
        {
            f.advance.set(false);
            let accepted = submit(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                Command::Bus { bus, action },
            );
            f.service.detach(ConnectionId(1));
            client = attach(
                &mut f.service,
                6003 + index as u128,
                ConnectionId(1),
                encoded,
            );
            assert!(client
                .snapshot()
                .unwrap()
                .pending
                .iter()
                .any(|op| op.id == accepted));
            f.advance.set(true);
            for _ in 0..100 {
                f.service.tick(STEP);
                drain(&mut f.service, &mut client, ConnectionId(1), encoded);
                if client.snapshot().unwrap().results.iter().any(|result| {
                    result.id == accepted && matches!(result.outcome, Outcome::Confirmed { .. })
                }) {
                    break;
                }
            }
            assert!(client.snapshot().unwrap().results.iter().any(|result| {
                result.id == accepted && matches!(result.outcome, Outcome::Confirmed { .. })
            }));
            if index < 2 {
                assert_eq!(client.snapshot().unwrap().buses[0].gain_db, -9.);
                assert_eq!(client.snapshot().unwrap().buses[0].muted, index == 1);
            }
        }
        assert!(client.snapshot().unwrap().buses.is_empty());
        assert!(matches!(
            run(&mut f, &mut client, Command::NewSession),
            Outcome::Confirmed { .. }
        ));
        assert_eq!(client.snapshot().unwrap().buses.len(), 1);
        assert_eq!(client.snapshot().unwrap().buses[0].name, "Master");
        assert_eq!(f.waits.get(), f.initial_waits);
    }
}

#[shoop_test]
fn refused_bus_structure_reconciles_ports_and_retains_a_removable_survivor() {
    let mut f = fixture();
    let mut client = attach(&mut f.service, 6100, ConnectionId(1), false);
    let before = client
        .snapshot()
        .unwrap()
        .connections
        .application_ports
        .len();
    let add = || {
        Command::AddBus(BusSpec {
            name: "Cue".into(),
            channel_count: 2,
            creation_request_id: None,
            fx: None,
        })
    };
    f.refuse_next_bus.set(true);
    let operation = submit(&mut f.service, &mut client, ConnectionId(1), add());
    let reserved = f.service.snapshot().buses[1].id;
    assert!(matches!(
        complete(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            operation,
            false
        ),
        Outcome::Refused { .. }
    ));
    assert_eq!(client.snapshot().unwrap().buses.len(), 1);
    assert_eq!(
        client
            .snapshot()
            .unwrap()
            .connections
            .application_ports
            .len(),
        before
    );
    assert!(matches!(
        command(&mut f.service, &mut client, ConnectionId(1), add()),
        Outcome::Confirmed { .. }
    ));
    let bus = client.snapshot().unwrap().buses[1].id;
    assert_ne!(bus, reserved);
    f.refuse_next_bus.set(true);
    let removal = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Bus {
            bus,
            action: BusAction::Remove,
        },
    );
    let addition = submit(&mut f.service, &mut client, ConnectionId(1), add());
    assert!(matches!(
        complete(&mut f.service, &mut client, ConnectionId(1), removal, false,),
        Outcome::Refused { .. }
    ));
    assert!(matches!(
        complete(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            addition,
            false
        ),
        Outcome::Confirmed { .. }
    ));
    let state = client.snapshot().unwrap();
    assert_eq!(state.buses[1].id, bus);
    assert_eq!(state.buses[1].structural_state, StructuralState::Confirmed);
    assert!(state.buses[1].structural_error.is_some());
    assert_eq!(state.connections.application_ports.len(), before + 4);
    let added = state.buses[2].id;
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::Bus {
                bus,
                action: BusAction::Remove
            }
        ),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::Bus {
                bus: added,
                action: BusAction::Remove,
            }
        ),
        Outcome::Confirmed { .. }
    ));
    assert_eq!(
        client
            .snapshot()
            .unwrap()
            .connections
            .application_ports
            .len(),
        before
    );
}

#[shoop_test]
fn default_bus_bootstrap_and_reset_from_zero_fan_out_to_a_mono_host() {
    let mut f = fixture_with_outputs(1);
    let mut client = attach(&mut f.service, 6200, ConnectionId(1), false);
    let initial = client.snapshot().unwrap();
    assert_eq!(initial.connections.confirmed_links.len(), 2);
    assert_eq!(
        initial.connections.confirmed_links[0].host_port_id,
        initial.connections.confirmed_links[1].host_port_id
    );
    let bus = initial.buses[0].id;
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::Bus {
                bus,
                action: BusAction::Remove
            }
        ),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::NewSession
        ),
        Outcome::Confirmed { .. }
    ));
    let reset = client.snapshot().unwrap();
    assert_eq!(reset.buses.len(), 1);
    assert_eq!(reset.connections.confirmed_links.len(), 2);
    assert_eq!(
        reset.connections.confirmed_links[0].host_port_id,
        reset.connections.confirmed_links[1].host_port_id
    );
}
fn created(outcome: Outcome) -> (TrackId, Vec<LoopId>) {
    let Outcome::TrackCreated { track, loops, .. } = outcome else {
        panic!("{outcome:?}")
    };
    (track, loops)
}

#[shoop_test]
fn connection_revision_ignores_unrelated_commands_and_backend_updates() {
    for encoded in [false, true] {
        let mut f = fixture();
        let connection = ConnectionId(1);
        let mut client = attach(&mut f.service, 1, connection, encoded);
        let before = f.service.snapshot();
        let bus = before.buses[0].id;
        for value in [
            Command::Global(GlobalControlAction::SetSync(false)),
            Command::Bus {
                bus,
                action: BusAction::GainChanged(-6.),
            },
            Command::Bus {
                bus,
                action: BusAction::MuteChanged(true),
            },
        ] {
            let operation = submit(&mut f.service, &mut client, connection, value);
            assert!(matches!(
                complete(&mut f.service, &mut client, connection, operation, encoded),
                Outcome::Confirmed { .. }
            ));
            let after = client.snapshot().unwrap();
            assert!(after.revision > before.revision);
            assert_eq!(after.connections, before.connections);
        }
        f.service.detach(connection);
        for _ in 0..20 {
            f.service.tick(STEP);
            assert_eq!(f.service.snapshot().connections, before.connections);
        }
    }
}

#[shoop_test]
fn connection_revision_tracks_port_and_route_changes() {
    let mut f = fixture();
    let connection = ConnectionId(1);
    let mut client = attach(&mut f.service, 1, connection, false);
    let initial = f.service.snapshot().connections.clone();
    let (track, _) = created(command(
        &mut f.service,
        &mut client,
        connection,
        new_track("Route", false),
    ));
    let added = f.service.snapshot().connections.clone();
    assert!(added.revision > initial.revision);
    assert!(added.application_ports.len() > initial.application_ports.len());
    let route = added.mixer_links[0];
    f.advance.set(false);
    let operation = submit(
        &mut f.service,
        &mut client,
        connection,
        Command::SetMixerRoute {
            source: route.source_port_id,
            destination: route.destination_channel_id,
            connected: false,
        },
    );
    let pending = f.service.snapshot().connections.clone();
    assert!(pending.revision > added.revision);
    assert!(pending.mixer_links.contains(&route));
    assert_eq!(
        pending.pending_mixer_links.as_ref(),
        [PendingMixerRouteState {
            route,
            desired_connected: false,
        }]
    );
    f.advance.set(true);
    assert!(matches!(
        complete(&mut f.service, &mut client, connection, operation, false),
        Outcome::Confirmed { .. }
    ));
    let confirmed = f.service.snapshot().connections.clone();
    assert!(confirmed.revision > pending.revision);
    assert!(!confirmed.mixer_links.contains(&route));
    assert!(confirmed.pending_mixer_links.is_empty());
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            connection,
            Command::Track {
                track,
                action: TrackAction::Remove,
            }
        ),
        Outcome::Confirmed { .. }
    ));
    let removed = f.service.snapshot().connections.clone();
    assert!(removed.revision > confirmed.revision);
    assert_eq!(removed.application_ports, initial.application_ports);
}

#[shoop_test]
fn live_host_availability_reaches_encoded_clients_and_pins_disconnected_routes() {
    struct Inventory(
        std::sync::Arc<std::sync::Mutex<Option<Vec<shoop_rt_backend::HostPortAlias>>>>,
    );
    impl shoop_rt_backend::HostPortInventory for Inventory {
        fn poll(&mut self) -> Option<Vec<shoop_rt_backend::HostPortAlias>> {
            self.0.lock().unwrap().take()
        }
    }
    let alias = |available| {
        vec![shoop_rt_backend::HostPortAlias {
            slot: "midi_in_1".into(),
            id: "usb0/1:synth/in/0".into(),
            name: "Synth input 1".into(),
            available,
        }]
    };
    let inventory = std::sync::Arc::new(std::sync::Mutex::new(Some(alias(true))));
    let mut f = fixture_with_inventory(Some(Box::new(Inventory(inventory.clone()))));
    let connection = ConnectionId(1);
    let mut client = attach(&mut f.service, 1, connection, true);
    let id = HostPortId::new("usb0/1:synth/in/0");
    assert!(client
        .snapshot()
        .unwrap()
        .connections
        .host_ports
        .iter()
        .any(|p| p.id == id && p.available));
    assert!(!f.service.release_host_port_identity(id.as_str()));
    *inventory.lock().unwrap() = Some(alias(false));
    assert!(
        !f.service.release_host_port_identity(id.as_str()),
        "unpublished device loss released an online identity"
    );
    for _ in 0..3 {
        f.service.tick(STEP);
        drain(&mut f.service, &mut client, connection, true);
    }
    assert!(f.service.release_host_port_identity(id.as_str()));
    *inventory.lock().unwrap() = Some(alias(true));
    for _ in 0..3 {
        f.service.tick(STEP);
        drain(&mut f.service, &mut client, connection, true);
    }
    let (track, _) = created(command(
        &mut f.service,
        &mut client,
        connection,
        new_track("MIDI", true),
    ));
    let port = f
        .service
        .snapshot()
        .connections
        .application_ports
        .iter()
        .find(|p| {
            matches!(p.owner, ApplicationPortOwner::Track {track_id, ..} if track_id == track)
                && p.data_type == PortDataType::Midi
                && p.direction == PortDirection::Input
        })
        .unwrap()
        .id;
    let operation = submit(
        &mut f.service,
        &mut client,
        connection,
        Command::SetPortConnected {
            port,
            host_port: id.clone(),
            connected: true,
        },
    );
    assert!(
        f.service.host_port_in_use(id.as_str()),
        "pending route lost its identity pin"
    );
    assert!(matches!(
        complete(&mut f.service, &mut client, connection, operation, true),
        Outcome::Confirmed { .. }
    ));
    let revision = client.snapshot().unwrap().revision;
    let connection_revision = client.snapshot().unwrap().connections.revision;
    *inventory.lock().unwrap() = Some(alias(false));
    for _ in 0..3 {
        f.service.tick(STEP);
        drain(&mut f.service, &mut client, connection, true);
    }
    assert!(client.snapshot().unwrap().revision > revision);
    assert!(client.snapshot().unwrap().connections.revision > connection_revision);
    assert!(client
        .snapshot()
        .unwrap()
        .connections
        .host_ports
        .iter()
        .any(|p| p.id == id && !p.available));
    assert!(
        f.service.host_port_in_use(id.as_str()),
        "offline route lost its identity pin"
    );
    let (offline_track, _) = created(command(
        &mut f.service,
        &mut client,
        connection,
        new_track("Offline MIDI", true),
    ));
    for _ in 0..3 {
        f.service.tick(STEP);
    }
    let snapshot = f.service.snapshot();
    assert!(snapshot.connections.application_ports.iter().filter(|p|
        matches!(p.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == offline_track) &&
        p.data_type == PortDataType::Midi).all(|p| !snapshot.connections.confirmed_links.iter()
            .any(|link| link.application_port_id == p.id)));
    assert!(matches!(
        command(
            &mut f.service,
            &mut client,
            connection,
            Command::SetPortConnected {
                port,
                host_port: id.clone(),
                connected: false,
            }
        ),
        Outcome::Confirmed { .. }
    ));
    assert!(!f.service.host_port_in_use(id.as_str()));
    assert!(f.service.release_host_port_identity(id.as_str()));
}

#[shoop_test]
fn shared_state_and_ordered_selection_survive_attachment_in_typed_and_encoded_delivery() {
    for encoded in [false, true] {
        #[cfg(not(target_arch = "wasm32"))]
        let started = std::time::Instant::now();
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
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!(
            "portable app contract: encoded={encoded}, elapsed_us={}, snapshot_bytes={}",
            started.elapsed().as_micros(),
            codec::encode_snapshot(after).unwrap().len()
        );
        #[cfg(not(target_arch = "wasm32"))]
        if !encoded && std::env::var_os("SHOOP_MEASURE_SESSION_SNAPSHOTS").is_some() {
            measure_snapshots(after, b.authority().unwrap());
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn measure_snapshots(snapshot: &std::sync::Arc<SessionSnapshot>, authority: Authority) {
    use std::{hint::black_box, sync::Arc, time::Instant};
    let mut expanded = snapshot.as_ref().clone();
    expanded.tracks = (0..64)
        .map(|index| {
            let mut track = snapshot.tracks[1].clone();
            track.id = TrackId::from_raw(index + 1);
            track.is_sync = index == 0;
            track.loops.truncate(4);
            for (slot, state) in track.loops.iter_mut().enumerate() {
                state.id = LoopId::from_raw(index * 4 + slot as u64 + 1);
            }
            track
        })
        .collect();
    for state in [Arc::clone(snapshot), Arc::new(expanded)] {
        let (client, service) = local::channel();
        let started = Instant::now();
        for _ in 0..128 {
            service
                .updates
                .try_send(ServiceMessage::Snapshot {
                    authority,
                    state: Arc::clone(&state),
                })
                .unwrap();
            black_box(client.updates.try_recv().unwrap());
        }
        let typed_us = started.elapsed().as_micros();
        let bytes = codec::encode_snapshot(&state).unwrap().len();
        let started = Instant::now();
        for _ in 0..128 {
            let encoded = codec::encode_snapshot(&state).unwrap();
            black_box(codec::decode_snapshot(&encoded).unwrap());
        }
        eprintln!("snapshot measurement: tracks={}, loops={}, bytes={bytes}, iterations=128, typed_queue_us={typed_us}, codec_roundtrip_us={}", state.tracks.len(), state.tracks.iter().map(|track| track.loops.len()).sum::<usize>(), started.elapsed().as_micros());
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
    assert_eq!(
        f.service.handle(
            ConnectionId(3),
            ClientMessage::Submit {
                session_epoch: 1,
                authority,
                operation: id,
                expected_revision: None,
                command: new_track("Must not duplicate", false),
            },
        ),
        Err(Refusal::Identity)
    );
    drain(&mut f.service, &mut resumed, ConnectionId(3), false);
    f.service.tick(STEP);
    assert_eq!(f.service.snapshot().tracks.len(), 2);
    assert!(resumed.results().is_empty());
    assert!(resumed
        .snapshot()
        .unwrap()
        .results
        .iter()
        .any(|result| result.id == id));
}

#[shoop_test]
fn replacing_a_client_fences_old_mutations_detach_and_shutdown_without_disturbing_the_owner() {
    let mut f = fixture();
    let first = ConnectionId(1);
    let mut a = attach(&mut f.service, 10, first, false);
    let stale = a.authority().unwrap();
    assert_eq!(
        f.service
            .handle(ConnectionId(9), ClientMessage::Attach { version: VERSION }),
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
        .any(|event| event.data == [0x90, 60, 100].into()));
    assert!(midi.channels[0]
        .events
        .iter()
        .any(|event| event.data == [0x80, 60, 0].into()));
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
    let mut b = Client::new();
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
    for sequence in 1000..1000 + QUEUE_CAPACITY as u64 {
        if !f.service.has_connection(ConnectionId(1)) {
            break;
        }
        f.service
            .handle(
                ConnectionId(1),
                ClientMessage::Submit {
                    session_epoch: 1,
                    authority,
                    operation: OperationId {
                        client: authority.client,
                        sequence,
                    },
                    expected_revision: None,
                    command: Command::ResetXruns,
                },
            )
            .unwrap();
    }
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
    refuse_bus: Rc<Cell<bool>>,
    refuse_lifecycle: Rc<Cell<bool>>,
    refuse_composite_after: Rc<Cell<Option<u32>>>,
    refuse_monitoring: Rc<Cell<bool>>,
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
            request:
                shoop_rt_protocol::Request::StripControl {
                    control: shoop_rt_protocol::StripControl::AutomaticMonitoring(_),
                    ..
                },
        } = message
        {
            if self.refuse_monitoring.replace(false) {
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
        if let shoop_rt_protocol::ClientMessage::Request {
            correlation,
            request:
                shoop_rt_protocol::Request::Composite(shoop_rt_protocol::CompositeCommand::Begin(
                    description,
                )),
        } = message
        {
            if let Some(remaining) = self
                .refuse_composite_after
                .get()
                .filter(|_| description.entries > 0)
            {
                self.refuse_composite_after.set(remaining.checked_sub(1));
                if remaining == 0 {
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
        }
        if let shoop_rt_protocol::ClientMessage::Request {
            correlation,
            request:
                shoop_rt_protocol::Request::ResetComposite { .. }
                | shoop_rt_protocol::Request::RemoveStrip { .. },
        } = message
        {
            if self.refuse_lifecycle.replace(false) {
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
        if let shoop_rt_protocol::ClientMessage::Request {
            correlation,
            request:
                shoop_rt_protocol::Request::CreateBus { .. }
                | shoop_rt_protocol::Request::RemoveBus { .. },
        } = message
        {
            if self.refuse_bus.replace(false) {
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

fn runtime(
    service: &mut Service,
    startup: shoop_app_client::runtime::Startup,
) -> (shoop_app_client::runtime::Runtime, local::ServicePort) {
    runtime_with_midi(service, startup, Box::new(shoop_scripting::NullMidiService))
}
fn runtime_with_midi(
    service: &mut Service,
    startup: shoop_app_client::runtime::Startup,
    midi: Box<dyn shoop_scripting::MidiControlService>,
) -> (shoop_app_client::runtime::Runtime, local::ServicePort) {
    let (delivery, port) = local::channel();
    let client = Client::new();
    let mut runtime =
        shoop_app_client::runtime::Runtime::new(client, Box::new(delivery), midi, startup).unwrap();
    for _ in 0..4 {
        service.pump_local(ConnectionId(99), &port, STEP);
        runtime.tick(STEP);
    }
    assert_eq!(runtime.state(), shoop_app_client::State::Ready);
    (runtime, port)
}

fn pump_runtime(
    service: &mut Service,
    runtime: &mut shoop_app_client::runtime::Runtime,
    port: &local::ServicePort,
    count: usize,
) {
    for _ in 0..count {
        service.pump_local(ConnectionId(99), port, STEP);
        runtime.tick(STEP);
    }
}

#[shoop_test]
fn ui_and_lua_use_ordered_service_operations_and_lua_reads_confirmed_snapshots() {
    use std::sync::Arc;
    let mut f = fixture();
    let (mut runtime, port) = runtime(
        &mut f.service,
        shoop_app_client::runtime::Startup::Fresh(Vec::new()),
    );
    runtime
        .dispatch(AppIntent::AddTrack(DirectTrackSpec {
            name: "Lua".into(),
            audio_channels: 1,
            midi: false,
        }))
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 20);
    assert_eq!(runtime.snapshot().tracks.len(), 2);
    runtime
        .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 4);
    runtime
        .dispatch(AppIntent::AddScriptSource {
            name: "confirmed.lua".into(),
            kind: ScriptKind::User,
            enabled: true,
            source: Arc::from(
                r#"
shoop_announce_api_version(1,0)
local c = require('shoop_control')
if c.loop_get_gain({0,0})[1] ~= 1 then error('unconfirmed read') end
c.loop_set_gain({0,0}, 0.25)
if c.loop_get_gain({0,0})[1] ~= 1 then error('unconfirmed read') end
c.register_one_shot_timer_cb(100, function()
    if c.loop_get_gain({0,0})[1] ~= 0.25 then error('confirmation missing') end
    c.loop_select({0,0},true)
    c.loop_trigger({0,0}, 3)
    print('confirmed-read')
end)
c.register_loop_event_cb(function(event) print('loop-event') end)
"#,
            ),
        })
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 30);
    let state = runtime.snapshot();
    let script = &state.scripting.scripts[0];
    assert!(script.latest_error.is_none(), "{:?}", script.latest_error);
    assert!(script
        .logs
        .iter()
        .any(|log| log.message.contains("confirmed-read")));
    assert!(script
        .logs
        .iter()
        .any(|log| log.message.contains("loop-event")));
    assert_eq!(state.tracks[1].loops[0].gain, 0.25);
    assert!(state.tracks[1].loops[0].selected);
    assert_eq!(state.tracks[1].loops[0].mode, LoopMode::Recording);
    let details = state
        .details
        .as_ref()
        .expect("recording metadata remains visible");
    assert_eq!(details.channels.len(), 1);
    let loop_id = state.tracks[1].loops[0].id;
    runtime
        .dispatch(AppIntent::SetLoopChannelTimings {
            loop_id,
            updates: vec![ChannelTimelineUpdate {
                channel_id: details.channels[0].id,
                start_offset: Some(-7),
                preplay_samples: Some(5),
            }],
        })
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 4);
    let state = runtime.snapshot();
    let details = state.details.as_ref().unwrap();
    assert_eq!(
        (
            details.channels[0].start_offset,
            details.channels[0].preplay_samples
        ),
        (-7, 5)
    );
    assert_eq!(
        runtime.snapshot().tracks[1].loops[0].mode,
        LoopMode::Recording
    );
    runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 2);
    assert!(runtime
        .snapshot()
        .notifications
        .iter()
        .any(|notice| notice.message == BackendCapability::SessionCapture.unavailable_reason()));
    assert!(runtime.take_file_output().is_none());
}

#[shoop_test]
fn recovered_clients_do_not_execute_catalog_scripts_until_explicit_user_start() {
    use std::sync::Arc;
    let mut f = fixture();
    let (mut runtime, port) = runtime(
        &mut f.service,
        shoop_app_client::runtime::Startup::Connected(Vec::new()),
    );
    runtime
        .dispatch(AppIntent::ReconcileCatalogScripts {
            scripts: vec![CatalogScriptSource {
                identity: "recovered-script".into(),
                name: "recovered.lua".into(),
                source: Arc::from(
                    "shoop_announce_api_version(1,0); require('shoop_control').set_solo(true)",
                ),
                source_path: None,
                resource_bundle: None,
                kind: ScriptKind::Bundled,
                enabled: true,
            }]
            .into(),
            preserve_identities: Arc::from([]),
        })
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 4);
    assert!(!f.service.snapshot().global.solo);
    let script = runtime.snapshot().scripting.scripts[0].clone();
    assert!(!script.enabled);
    assert_eq!(script.lifecycle, ScriptLifecycle::Inactive);
    runtime
        .dispatch(AppIntent::SetScriptEnabled {
            script_id: script.id,
            enabled: true,
        })
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 4);
    assert!(f.service.snapshot().global.solo);
}

#[shoop_test]
fn temporary_parity_gaps_refuse_file_preview_and_driver_work_without_mutating_the_session() {
    use shoop_app_client::runtime::Startup;
    let mut f = fixture();
    let (mut runtime, port) = runtime(&mut f.service, Startup::Connected(Vec::new()));
    let before = f.service.snapshot();
    let prior_notifications = runtime.snapshot().notifications.len();
    for intent in [
        AppIntent::RequestSaveSession,
        AppIntent::LoadSessionBytes {
            name: "unavailable.shoop".into(),
            bytes: std::sync::Arc::new(Vec::new()),
        },
        AppIntent::PreviewClickTrack {
            output: ClickTrackPreviewOutput::System,
            loop_id: before.tracks[0].loops[0].id,
            request: Default::default(),
        },
        AppIntent::RequestAudioDriverSwitch {
            config: AudioDriverConfig::Dummy(Default::default()),
        },
    ] {
        runtime.dispatch(intent).unwrap();
    }
    pump_runtime(&mut f.service, &mut runtime, &port, 4);
    let state = runtime.snapshot();
    assert_eq!(state.notifications.len(), prior_notifications + 4);
    for capability in BackendCapability::ALL {
        assert_eq!(
            state.capabilities.supports(capability),
            matches!(
                capability,
                BackendCapability::MidiInjection
                    | BackendCapability::Grab
                    | BackendCapability::ReplacingMode
                    | BackendCapability::BusManagement
                    | BackendCapability::CompositeLoops
                    | BackendCapability::LoopTimingEdits
            )
        );
    }
    assert!(runtime.take_file_output().is_none());
    assert_eq!(
        state.click_track.preview_status,
        ClickTrackPreviewStatus::Idle
    );
    let after = f.service.snapshot();
    assert_eq!(after.tracks[0].id, before.tracks[0].id);
    assert_eq!(after.tracks[0].loops[0].id, before.tracks[0].loops[0].id);
    assert!(after.pending.is_empty());
}

#[shoop_test]
fn management_shutdown_survives_data_loss_and_a_replacement_fences_it() {
    let mut f = fixture();
    let (mut runtime, port) = runtime(
        &mut f.service,
        shoop_app_client::runtime::Startup::Connected(Vec::new()),
    );
    runtime.dispatch(AppIntent::SetLoopSmoothingMs(7)).unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 4);
    runtime.fail("injected data loss".into());
    f.service.detach(ConnectionId(99));
    assert!(runtime.dispatch(AppIntent::SetLoopSmoothingMs(8)).is_err());
    let ClientMessage::Shutdown {
        authority,
        operation,
    } = runtime.prepare_shutdown().unwrap()
    else {
        unreachable!()
    };
    let replacement = attach(&mut f.service, 501, ConnectionId(100), false);
    assert_ne!(replacement.authority(), Some(authority));
    assert_eq!(
        f.service.management(ManagementRequest::Shutdown {
            authority,
            operation
        }),
        ManagementReply::Refused(Refusal::StaleAuthority)
    );
    assert!(f.service.is_ready());
}

#[shoop_test]
fn explicit_ownerless_shutdown_cannot_evict_a_live_ui_and_fences_previous_owners() {
    let mut f = fixture();
    let mut client = attach(&mut f.service, 500, ConnectionId(1), false);
    let authority = client.authority().unwrap();
    let operation = OperationId {
        client: ClientId(501),
        sequence: 1,
    };
    let claim = ManagementRequest::ClaimShutdown {
        version: VERSION,

        operation,
    };
    assert_eq!(
        f.service.management(claim.clone()),
        ManagementReply::Refused(Refusal::Busy)
    );
    f.service
        .handle(ConnectionId(1), client.detach().unwrap())
        .unwrap();
    let reply = f.service.management(claim.clone());
    let ManagementReply::Accepted {
        authority: shutdown_authority,
        ..
    } = reply
    else {
        panic!("{reply:?}")
    };
    assert_ne!(shutdown_authority, authority);
    assert_eq!(f.service.management(claim), reply);
    assert_eq!(
        f.service.management(ManagementRequest::Result {
            authority,
            operation
        }),
        ManagementReply::Refused(Refusal::StaleAuthority)
    );
    for _ in 0..100 {
        f.service.tick(STEP);
        if f.service.shutdown_ready() {
            break;
        }
    }
    assert!(f.service.shutdown_ready());
}

#[shoop_test]
fn deployment_is_boot_bound_exclusive_idempotent_and_preserves_shutdown_kind() {
    use shoop_deployment_protocol as d;
    let mut f = fixture();
    assert_eq!(
        f.service.deployment(d::Request::Status),
        d::Reply::Refused(d::Refusal::Unsupported)
    );
    f.service
        .set_deployment_info(d::Info {
            profile: "shoop-zynq7000-1g-v1".into(),
            build: "a".into(),
            hash: [1; 32],
            slot: Some(0),
            confirmed: true,
            trial: false,
            boot_nonce: 123,
            last_operation: None,
            session_version: VERSION,
            busy: false,
            ready: false,
        })
        .unwrap();
    let operation = d::Operation {
        client: 800,
        sequence: 1,
    };
    let enter = d::Request::Enter {
        boot_nonce: 123,
        operation,
    };
    assert_eq!(
        f.service.deployment(d::Request::Enter {
            boot_nonce: 122,
            operation
        }),
        d::Reply::Refused(d::Refusal::StaleBoot)
    );
    let mut ui = attach(&mut f.service, 799, ConnectionId(1), false);
    assert_eq!(
        f.service.deployment(enter),
        d::Reply::Refused(d::Refusal::Busy)
    );
    f.service
        .handle(ConnectionId(1), ui.detach().unwrap())
        .unwrap();
    assert_eq!(f.service.deployment(enter), d::Reply::Accepted(operation));
    assert_eq!(f.service.deployment(enter), d::Reply::Accepted(operation));
    assert_eq!(f.service.updater_request(), Some(operation));
    for _ in 0..100 {
        f.service.tick(STEP);
        if f.service.shutdown_ready() {
            break;
        }
    }
    assert!(f.service.shutdown_ready());
    f.service.begin_host_cleanup().unwrap();
    f.service
        .complete_shutdown(Err("injected cleanup failure".into()));
    assert_eq!(f.service.updater_request(), None);
    assert_eq!(
        f.service.deployment(enter),
        d::Reply::Finished {
            operation,
            result: Err(d::Refusal::CleanupFailed)
        }
    );
    let ordinary = OperationId {
        client: ClientId(801),
        sequence: 1,
    };
    assert!(matches!(
        f.service.management(ManagementRequest::ClaimShutdown {
            version: VERSION,
            operation: ordinary
        }),
        ManagementReply::Accepted { .. }
    ));
    assert_eq!(
        f.service.updater_request(),
        None,
        "failed deployment must not convert ordinary shutdown into reset"
    );
    assert_eq!(
        f.service.deployment(d::Request::Enter {
            boot_nonce: 123,
            operation: d::Operation {
                client: 801,
                sequence: 1
            }
        }),
        d::Reply::Refused(d::Refusal::Conflict)
    );
}

#[shoop_test]
fn deployment_status_and_success_remain_reconcilable_after_cleanup() {
    use shoop_deployment_protocol as d;
    let mut f = fixture();
    let identity = d::Info {
        profile: "shoop-zynq7000-1g-v1".into(),
        build: "firmware-a".into(),
        hash: [1; 32],
        slot: Some(0),
        confirmed: true,
        trial: false,
        boot_nonce: 123,
        last_operation: None,
        session_version: VERSION,
        busy: false,
        ready: true,
    };
    f.service.set_deployment_info(identity.clone()).unwrap();
    assert_eq!(
        f.service.deployment(d::Request::Status),
        d::Reply::Info(identity.clone())
    );
    let operation = d::Operation {
        client: 900,
        sequence: 1,
    };
    let enter = d::Request::Enter {
        boot_nonce: 123,
        operation,
    };
    let result = d::Request::Result {
        boot_nonce: 123,
        operation,
    };
    assert_eq!(
        f.service.deployment(result),
        d::Reply::Refused(d::Refusal::Conflict)
    );
    assert_eq!(f.service.deployment(enter), d::Reply::Accepted(operation));
    assert_eq!(f.service.deployment(result), d::Reply::Accepted(operation));
    let d::Reply::Info(pending) = f.service.deployment(d::Request::Status) else {
        panic!()
    };
    assert!(pending.busy);
    for _ in 0..100 {
        f.service.tick(STEP);
        if f.service.shutdown_ready() {
            break;
        }
    }
    assert!(f.service.shutdown_ready());
    f.service.begin_host_cleanup().unwrap();
    f.service.complete_shutdown(Ok(()));
    assert_eq!(f.service.updater_request(), None);
    let finished = d::Reply::Finished {
        operation,
        result: Ok(()),
    };
    assert_eq!(f.service.deployment(result), finished);
    assert_eq!(f.service.deployment(enter), finished);
    let d::Reply::Info(closed) = f.service.deployment(d::Request::Status) else {
        panic!()
    };
    assert!(!closed.ready && !closed.busy);
    assert_eq!(closed.boot_nonce, identity.boot_nonce);
    assert_eq!(closed.hash, identity.hash);
}

#[shoop_test]
fn recovered_startup_catalog_is_visible_but_does_not_run_and_shutdown_discards_unsent_commands() {
    use shoop_app_client::runtime::{Runtime, Startup, StartupScript};
    let mut f = fixture();
    let mut first = attach(&mut f.service, 499, ConnectionId(1), false);
    f.service
        .handle(ConnectionId(1), first.detach().unwrap())
        .unwrap();
    let (delivery, port) = local::channel();
    let mut runtime = Runtime::new(
        Client::new(),
        Box::new(delivery),
        Box::new(shoop_scripting::NullMidiService),
        Startup::Connected(vec![StartupScript {
            name: "saved.lua".into(),
            identity: None,
            source: "shoop_announce_api_version(1,0); require('shoop_control').set_solo(true)"
                .into(),
            source_path: None,
            kind: ScriptKind::User,
            enabled: true,
        }]),
    )
    .unwrap();
    assert!(runtime.dispatch(AppIntent::SetLoopSmoothingMs(5)).is_err());
    pump_runtime(&mut f.service, &mut runtime, &port, 5);
    assert_eq!(runtime.snapshot().scripting.scripts.len(), 1);
    assert!(!runtime.snapshot().scripting.scripts[0].enabled);
    assert!(!f.service.snapshot().global.solo);
    runtime
        .dispatch(AppIntent::Global(GlobalControlAction::SetSolo(true)))
        .unwrap();
    let ClientMessage::Shutdown {
        authority,
        operation,
    } = runtime.prepare_shutdown().unwrap()
    else {
        unreachable!()
    };
    pump_runtime(&mut f.service, &mut runtime, &port, 2);
    assert!(!f.service.snapshot().global.solo);
    assert!(matches!(
        f.service.management(ManagementRequest::Shutdown {
            authority,
            operation
        }),
        ManagementReply::Accepted { .. }
    ));
    for _ in 0..100 {
        f.service.tick(STEP);
        if f.service.shutdown_ready() {
            break;
        }
    }
    assert!(f.service.shutdown_ready());
}

#[shoop_test]
fn script_command_bursts_report_capacity_and_do_not_mutate_the_confirmed_read_view() {
    use std::sync::Arc;
    let mut f = fixture();
    let (mut runtime, port) = runtime(
        &mut f.service,
        shoop_app_client::runtime::Startup::Fresh(Vec::new()),
    );
    runtime.dispatch(AppIntent::AddScriptSource {name: "bounded.lua".into(), kind: ScriptKind::User, enabled: true, source: Arc::from("shoop_announce_api_version(1,0); local c=require('shoop_control'); for i=1,129 do c.set_solo(true) end")}).unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 20);
    let script = runtime.snapshot().scripting.scripts[0].clone();
    assert!(script.latest_error.unwrap().contains("queue is full"));
    assert!(f.service.snapshot().global.solo);
    assert!(f.service.snapshot().pending.len() <= MAX_PENDING_OPERATIONS);
}

#[shoop_test]
fn waveform_and_midi_views_use_bounded_media_delivery_and_release_it_on_selection_change() {
    let mut f = fixture();
    let (mut runtime, port) = runtime(
        &mut f.service,
        shoop_app_client::runtime::Startup::Fresh(Vec::new()),
    );
    runtime
        .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
        .unwrap();
    runtime
        .dispatch(AppIntent::AddTrack(DirectTrackSpec {
            name: "Media".into(),
            audio_channels: 1,
            midi: true,
        }))
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 20);
    let track = runtime.snapshot().tracks[1].id;
    let loop_id = runtime.snapshot().tracks[1].loops[0].id;
    runtime
        .dispatch(AppIntent::Loop {
            track_id: track,
            loop_id,
            action: LoopAction::IconClicked(SelectionModifiers::default()),
        })
        .unwrap();
    runtime
        .dispatch(AppIntent::Loop {
            track_id: track,
            loop_id,
            action: LoopAction::RecordClicked,
        })
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 4);
    runtime
        .dispatch(AppIntent::Track {
            track_id: track,
            action: TrackAction::InputMonitoringChanged {
                enabled: true,
                respect_auto_mute: false,
            },
        })
        .unwrap();
    runtime
        .dispatch(AppIntent::Piano(PianoAction::Press(
            MidiNote::new(60).unwrap(),
        )))
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 4);
    runtime
        .dispatch(AppIntent::Piano(PianoAction::Release(
            MidiNote::new(60).unwrap(),
        )))
        .unwrap();
    runtime
        .dispatch(AppIntent::Loop {
            track_id: track,
            loop_id,
            action: LoopAction::StopClicked,
        })
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 100);
    let details = runtime.snapshot().details.clone().unwrap();
    assert!(!details.loading);
    assert_eq!(details.channels.len(), 1);
    assert!(details.channels[0]
        .samples
        .iter()
        .any(|sample| *sample == 0.25));
    assert!(details.midi_channels[0]
        .events
        .iter()
        .any(|event| event.data.as_ref() == [0x90, 60, 100]));
    runtime
        .dispatch(AppIntent::Global(GlobalControlAction::DeselectAll))
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 4);
    assert!(runtime.client().media().is_none());
    assert!(runtime.snapshot().details.is_none());
}

#[shoop_test]
fn ui_bursts_apply_in_order_without_disconnecting_a_pumping_client_and_reset_is_confirmed() {
    let mut f = fixture();
    let sync = f.service.snapshot().tracks[0].id;
    let (mut runtime, port) = runtime(
        &mut f.service,
        shoop_app_client::runtime::Startup::Fresh(Vec::new()),
    );
    for index in 0..QUEUE_CAPACITY {
        runtime
            .dispatch(AppIntent::Track {
                track_id: sync,
                action: TrackAction::NameChanged(format!("Sync {index}")),
            })
            .unwrap();
    }
    for _ in 0..100 {
        pump_runtime(&mut f.service, &mut runtime, &port, 1);
        assert_eq!(runtime.state(), shoop_app_client::State::Ready);
        if runtime.snapshot().tracks[0].name == format!("Sync {}", QUEUE_CAPACITY - 1) {
            break;
        }
    }
    assert_eq!(
        runtime.snapshot().tracks[0].name,
        format!("Sync {}", QUEUE_CAPACITY - 1)
    );
    assert!(runtime.snapshot().notifications.is_empty());
    f.advance.set(false);
    runtime.dispatch(AppIntent::RequestNewSession).unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 4);
    assert_eq!(
        runtime.snapshot().io_task.as_ref().unwrap().status,
        IoTaskStatus::Running
    );
    f.advance.set(true);
    pump_runtime(&mut f.service, &mut runtime, &port, 20);
    assert_eq!(
        runtime.snapshot().io_task.as_ref().unwrap().status,
        IoTaskStatus::Completed
    );
    assert_ne!(runtime.snapshot().tracks[0].id, sync);
    assert_eq!(runtime.snapshot().tracks[0].name, "Sync");
}

#[shoop_test]
fn losing_the_client_releases_only_held_piano_notes_while_recording_continues() {
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
        new_track("Held MIDI", true),
    ));
    command(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        Command::Track {
            track,
            action: TrackAction::InputMonitoringChanged {
                enabled: true,
                respect_auto_mute: false,
            },
        },
    );
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
    command(
        &mut f.service,
        &mut a,
        ConnectionId(1),
        Command::Piano(PianoAction::Press(MidiNote::new(60).unwrap())),
    );
    let before = f.render.borrow().next_frame();
    f.service.detach(ConnectionId(1));
    for _ in 0..4 {
        f.service.tick(STEP);
    }
    let mut b = attach(&mut f.service, 11, ConnectionId(2), true);
    assert_eq!(
        b.snapshot().unwrap().tracks[1].loops[0].mode,
        LoopMode::Recording
    );
    assert!(f.render.borrow().next_frame() > before);
    command(
        &mut f.service,
        &mut b,
        ConnectionId(2),
        Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::StopClicked,
        },
    );
    let mut midi = None;
    for _ in 0..100 {
        midi = f
            .service
            .read_midi(ConnectionId(2), b.authority().unwrap(), loops[0])
            .unwrap();
        if midi.is_some() {
            break;
        }
        f.service.tick(STEP);
    }
    let midi = midi.unwrap();
    assert_eq!(
        midi.channels[0].events.len(),
        2,
        "detach sends a held note-off without a channel panic or capture reset"
    );
    assert_eq!(midi.channels[0].events[0].data, [0x90, 60, 100].into());
    assert_eq!(midi.channels[0].events[1].data, [0x80, 60, 0].into());
}

#[shoop_test]
fn lua_midi_ports_and_callbacks_live_in_the_client_and_release_on_detach() {
    use shoop_scripting::{FakeMidiService, MidiEndpoint, MidiEndpointDirection};
    use std::sync::Arc;
    let mut f = fixture();
    let (midi, control) = FakeMidiService::new();
    control.set_endpoints(vec![MidiEndpoint {
        id: "control-source".into(),
        name: "Controller".into(),
        direction: MidiEndpointDirection::Output,
    }]);
    let (mut runtime, port) = runtime_with_midi(
        &mut f.service,
        shoop_app_client::runtime::Startup::Fresh(Vec::new()),
        Box::new(midi),
    );
    runtime.dispatch(AppIntent::AddScriptSource {name: "controller.lua".into(), kind: ScriptKind::User, enabled: true, source: Arc::from("shoop_announce_api_version(1,0); local c=require('shoop_control'); c.auto_open_device_specific_midi_control_input('Controller',function() c.set_solo(true) end)")}).unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 4);
    assert_eq!(control.active_connections(), 1);
    let snapshot = runtime.snapshot();
    assert!(snapshot
        .connections
        .application_ports
        .iter()
        .any(
            |port| matches!(port.owner, ApplicationPortOwner::LuaControl { .. })
                && port.connection_policy == ConnectionPolicy::OwnerManaged
        ));
    assert!(!f
        .service
        .snapshot()
        .connections
        .application_ports
        .iter()
        .any(|port| matches!(port.owner, ApplicationPortOwner::LuaControl { .. })));
    control.push_input("control-source", vec![0x90, 60, 100]);
    pump_runtime(&mut f.service, &mut runtime, &port, 4);
    assert!(f.service.snapshot().global.solo);
    runtime.detach().unwrap();
    assert_eq!(control.active_connections(), 0);
    f.service.pump_local(ConnectionId(99), &port, STEP);
    assert!(f.service.is_ready());
    assert_eq!(f.service.snapshot().tracks.len(), 1);
}

#[shoop_test]
fn another_session_epoch_cannot_apply_even_global_commands_and_refusal_is_reconciled() {
    let mut f = fixture();
    let mut client = attach(&mut f.service, 900, ConnectionId(1), false);
    let mut message = client
        .command(Command::SetMasterAutoConnect(true), None)
        .unwrap();
    let ClientMessage::Submit {
        operation,
        session_epoch,
        ..
    } = &mut message
    else {
        panic!()
    };
    let operation = *operation;
    *session_epoch += 1;
    let before = f.service.snapshot();
    f.service.handle(ConnectionId(1), message.clone()).unwrap();
    let result = complete(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        operation,
        false,
    );
    assert!(matches!(
        result,
        Outcome::Refused {
            reason: Refusal::Identity,
            ..
        }
    ));
    assert_eq!(
        f.service.snapshot().master_auto_connect,
        before.master_auto_connect
    );
    f.service.handle(ConnectionId(1), message).unwrap();
    drain(&mut f.service, &mut client, ConnectionId(1), false);
    assert_eq!(client.results()[&operation], result);
}

fn persistence_message(
    service: &mut Service,
    client: &mut Client,
    request: persistence::Request,
    encoded: bool,
) {
    let message = client.persistence(request).unwrap();
    let message = if encoded {
        let mut bytes = [0; shoop_transport::MAX_RECORD_BYTES];
        let length = codec::encode(&WireMessage::Client(message), &mut bytes).unwrap();
        let WireMessage::Client(message) = codec::decode(&bytes[..length]).unwrap() else {
            panic!()
        };
        message
    } else {
        message
    };
    service.handle(ConnectionId(1), message).unwrap();
    drain(service, client, ConnectionId(1), encoded);
}
fn capture_image(
    service: &mut Service,
    client: &mut Client,
    transfer: u64,
    encoded: bool,
) -> std::sync::Arc<persistence_image::Image> {
    let key = persistence::Key {
        transfer: TransferId(transfer),
        epoch: client.snapshot().unwrap().persistence_epoch,
    };
    let target = persistence::Target::Session {
        revision: client.snapshot().unwrap().revision,
    };
    if encoded {
        persistence_message(
            service,
            client,
            persistence::Request::Capture { key, target },
            true,
        );
    } else {
        service
            .handle(ConnectionId(1), client.capture_image(key, target).unwrap())
            .unwrap();
    }
    let mut description = None;
    for _ in 0..500 {
        service.tick(STEP);
        drain(service, client, ConnectionId(1), encoded);
        if let Some((received, image)) = client.take_captured_image() {
            assert_eq!(received, key);
            return image;
        }
        while let Some(reply) = client.take_persistence_reply() {
            match reply {
                persistence::Reply::Captured(captured) => description = Some(captured),
                persistence::Reply::Refused { reason, .. } => panic!("capture refused: {reason:?}"),
                _ => {}
            }
        }
        if description.is_some() {
            break;
        }
    }
    let description = description.expect("capture did not complete");
    let mut objects = Vec::new();
    for object in 0..description.objects {
        persistence_message(
            service,
            client,
            persistence::Request::Describe { key, object },
            true,
        );
        let persistence::Reply::Object {
            object: descriptor, ..
        } = client.take_persistence_reply().unwrap()
        else {
            panic!()
        };
        let mut bytes = Vec::new();
        while (bytes.len() as u64) < descriptor.bytes {
            persistence_message(
                service,
                client,
                persistence::Request::Read {
                    key,
                    object,
                    offset: bytes.len() as u64,
                },
                true,
            );
            let persistence::Reply::Chunk { bytes: chunk, .. } =
                client.take_persistence_reply().unwrap()
            else {
                panic!()
            };
            bytes.extend_from_slice(&chunk);
        }
        objects.push(persistence::Object {
            description: descriptor,
            bytes,
        });
    }
    persistence_transfer::assemble(objects).unwrap()
}
fn release_image(
    service: &mut Service,
    client: &mut Client,
    transfer: u64,
    epoch: u64,
    encoded: bool,
) {
    persistence_message(
        service,
        client,
        persistence::Request::Cancel {
            key: persistence::Key {
                transfer: TransferId(transfer),
                epoch,
            },
        },
        encoded,
    );
    assert!(matches!(
        client.take_persistence_reply(),
        Some(persistence::Reply::Closed { .. })
    ));
}
fn stage_image(
    service: &mut Service,
    client: &mut Client,
    transfer: u64,
    image: std::sync::Arc<persistence_image::Image>,
    encoded: bool,
) -> persistence::Key {
    let key = persistence::Key {
        transfer: TransferId(transfer),
        epoch: client.snapshot().unwrap().persistence_epoch,
    };
    let target = persistence::Target::Session {
        revision: client.snapshot().unwrap().revision,
    };
    if encoded {
        let mut stream = persistence_transfer::UploadStream::new(image, key, target).unwrap();
        while let Some(request) = stream.next_request().unwrap() {
            persistence_message(service, client, request, true);
            if let Some(reply) = client.take_persistence_reply() {
                assert!(
                    !matches!(reply, persistence::Reply::Refused { .. }),
                    "stage refused: {reply:?}"
                );
            }
        }
    } else {
        service
            .handle(
                ConnectionId(1),
                client.stage_image(key, target, image).unwrap(),
            )
            .unwrap();
        drain(service, client, ConnectionId(1), false);
    }
    for _ in 0..20 {
        service.tick(STEP);
        drain(service, client, ConnectionId(1), encoded);
        while let Some(reply) = client.take_persistence_reply() {
            match reply {
                persistence::Reply::Ready { key: received } => {
                    assert_eq!(received, key);
                    return key;
                }
                persistence::Reply::Refused { reason, .. } => panic!("stage refused: {reason:?}"),
                _ => {}
            }
        }
    }
    panic!("persistence preparation did not complete")
}
#[shoop_test]
fn service_captures_and_reloads_core_images_through_typed_and_wire_paths() {
    for encoded in [false, true] {
        let mut f = fixture();
        let mut client = attach(&mut f.service, 999, ConnectionId(1), encoded);
        let image = capture_image(&mut f.service, &mut client, 1, encoded);
        let epoch = client.snapshot().unwrap().persistence_epoch;
        release_image(&mut f.service, &mut client, 1, epoch, encoded);
        let old_track = client.snapshot().unwrap().tracks[0].id;
        let old_loop = client.snapshot().unwrap().tracks[0].loops[0].id;
        let key = stage_image(&mut f.service, &mut client, 2, image, encoded);
        let operation = submit(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::CommitPersistence(key),
        );
        let result = complete(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            operation,
            encoded,
        );
        assert!(matches!(result, Outcome::Confirmed { .. }), "{result:?}");
        assert!(f.service.is_ready());
        assert_eq!(client.snapshot().unwrap().persistence_epoch, epoch + 1);
        assert_ne!(client.snapshot().unwrap().tracks[0].id, old_track);
        assert_ne!(client.snapshot().unwrap().tracks[0].loops[0].id, old_loop);
        assert_eq!(
            client.snapshot().unwrap().tracks[0].loops[0].mode,
            LoopMode::Stopped
        );
        let saved = capture_image(&mut f.service, &mut client, 3, encoded);
        assert!(matches!(
            saved.manifest,
            persistence_image::Manifest::Session(_)
        ));
        assert!(saved.payloads.is_empty());
        assert_eq!(f.waits.get(), f.initial_waits);
    }
}

#[shoop_test]
fn accepted_media_bearing_load_survives_ui_loss_and_reconciles_without_replay() {
    use std::sync::Arc;
    let mut f = fixture();
    let mut client = attach(&mut f.service, 1001, ConnectionId(1), false);
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        new_track("Music", true),
    );
    let (_, loops) = created(complete(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        operation,
        false,
    ));
    let mut image = (*capture_image(&mut f.service, &mut client, 1, false)).clone();
    release_image(&mut f.service, &mut client, 1, 1, false);
    let persistence_image::Manifest::Session(session) = &mut image.manifest else {
        panic!()
    };
    session.global.sync = false;
    session.selected = vec![loops[0]];
    session.target = Some(loops[1]);
    let content = &mut session.tracks[1].loops[0];
    content.frames = 4;
    content.audio[0].frames = 6;
    content.audio[0].object = Some(1);
    content.audio[0].start_offset = 1;
    content.audio[0].gain = 0.5;
    content.audio[0].recording_started_at = Some("2026-11-01T00:00:00Z".to_owned());
    content.midi[0].channel.frames = 8;
    content.midi[0].channel.object = Some(2);
    content.midi[0].initial = vec![vec![0xb0, 7, 12], vec![0xb0, 7, 99]];
    image.payloads = vec![
        persistence_image::Payload::Audio(Arc::new(vec![0., 0.25, 0.5, 0.75, 1., 0.125])),
        persistence_image::Payload::Midi(Arc::new(vec![
            MidiEventState {
                frame: 0,
                data: Arc::from([0x90, 60, 100]),
            },
            MidiEventState {
                frame: 5,
                data: Arc::from([0x80, 60, 0]),
            },
        ])),
    ];
    let expected = image.payloads.clone();
    let key = stage_image(&mut f.service, &mut client, 2, Arc::new(image), false);
    f.advance.set(false);
    let message = client
        .command(Command::CommitPersistence(key), None)
        .unwrap();
    let ClientMessage::Submit { operation, .. } = &message else {
        panic!()
    };
    let operation = *operation;
    f.service.handle(ConnectionId(1), message).unwrap();
    persistence_message(
        &mut f.service,
        &mut client,
        persistence::Request::Cancel { key },
        false,
    );
    assert!(matches!(
        client.take_persistence_reply(),
        Some(persistence::Reply::Refused {
            reason: Refusal::Busy,
            ..
        })
    ));
    f.service.detach(ConnectionId(1));
    client.disconnect();
    let mut recovered = attach(&mut f.service, 1001, ConnectionId(2), false);
    assert!(recovered.pending().is_empty());
    assert!(recovered
        .snapshot()
        .unwrap()
        .pending
        .iter()
        .any(|pending| pending.id == operation));
    f.advance.set(true);
    let outcome = observe_host_result(
        &mut f.service,
        &mut recovered,
        ConnectionId(2),
        operation,
        false,
    );
    assert!(matches!(outcome, Outcome::Confirmed { .. }), "{outcome:?}");
    let state = recovered.snapshot().unwrap();
    assert_eq!(state.persistence_epoch, 2);
    assert!(state.tracks[1].loops[0].selected);
    assert!(state.tracks[1].loops[1].targeted);
    let ids = state
        .tracks
        .iter()
        .map(|track| track.id)
        .collect::<Vec<_>>();
    assert_eq!(
        f.service.handle(
            ConnectionId(2),
            ClientMessage::Submit {
                authority: recovered.authority().unwrap(),
                session_epoch: 1,
                operation,
                expected_revision: None,
                command: Command::CommitPersistence(key),
            },
        ),
        Err(Refusal::Identity)
    );
    drain(&mut f.service, &mut recovered, ConnectionId(2), false);
    assert!(!recovered.results().contains_key(&operation));
    assert!(recovered
        .snapshot()
        .unwrap()
        .results
        .iter()
        .any(|result| result.id == operation && result.outcome == outcome));
    assert_eq!(
        f.service
            .snapshot()
            .tracks
            .iter()
            .map(|track| track.id)
            .collect::<Vec<_>>(),
        ids
    );
    let authority = recovered.authority().unwrap();
    let capture_key = persistence::Key {
        transfer: TransferId(3),
        epoch: 2,
    };
    let target = persistence::Target::Session {
        revision: recovered.snapshot().unwrap().revision,
    };
    f.service
        .handle(
            ConnectionId(2),
            recovered.capture_image(capture_key, target).unwrap(),
        )
        .unwrap();
    let captured = (0..100)
        .find_map(|_| {
            f.service.tick(STEP);
            drain(&mut f.service, &mut recovered, ConnectionId(2), false);
            recovered.take_captured_image().map(|(_, image)| image)
        })
        .expect("capture completion");
    assert_eq!(captured.payloads, expected);
    for (captured, expected) in captured.payloads.iter().zip(&expected) {
        match (captured, expected) {
            (persistence_image::Payload::Audio(a), persistence_image::Payload::Audio(b)) => {
                assert!(Arc::ptr_eq(a, b))
            }
            (persistence_image::Payload::Midi(a), persistence_image::Payload::Midi(b)) => {
                assert!(Arc::ptr_eq(a, b))
            }
            _ => panic!("capture changed media types"),
        }
    }
    let persistence_image::Manifest::Session(session) = &captured.manifest else {
        panic!()
    };
    let content = &session.tracks[1].loops[0];
    assert_eq!(
        content.audio[0].recording_started_at.as_deref(),
        Some("2026-11-01T00:00:00Z")
    );
    assert_eq!(content.audio[0].start_offset, 1);
    assert_eq!(content.audio[0].gain, 0.5);
    assert_eq!(
        content.midi[0].initial,
        [vec![0xb0, 7, 12], vec![0xb0, 7, 99]]
    );
    assert_eq!(f.waits.get(), f.initial_waits);
    assert!(f.service.has_connection(ConnectionId(2)));
    assert_ne!(
        authority.generation,
        client
            .authority()
            .map_or(ConnectionGeneration(0), |authority| authority.generation)
    );
}

#[shoop_test]
fn capture_keeps_one_metadata_cut_while_controls_continue_and_view_cancellation_is_independent() {
    let mut f = fixture();
    let mut client = attach(&mut f.service, 1002, ConnectionId(1), false);
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Global(GlobalControlAction::SetSync(false)),
    );
    complete(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        operation,
        false,
    );
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        new_track("Music", true),
    );
    let (track, loops) = created(complete(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        operation,
        false,
    ));
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::RecordClicked,
        },
    );
    complete(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        operation,
        false,
    );
    for _ in 0..20 {
        f.service.tick(STEP);
        drain(&mut f.service, &mut client, ConnectionId(1), false);
    }
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::StopClicked,
        },
    );
    complete(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        operation,
        false,
    );
    let old_name = client.snapshot().unwrap().tracks[1].loops[0].name.clone();
    let key = persistence::Key {
        transfer: TransferId(1),
        epoch: 1,
    };
    let target = persistence::Target::Session {
        revision: client.snapshot().unwrap().revision,
    };
    f.service
        .handle(ConnectionId(1), client.capture_image(key, target).unwrap())
        .unwrap();
    f.service.tick(STEP);
    drain(&mut f.service, &mut client, ConnectionId(1), false);
    assert!(client.take_captured_image().is_none());
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::NameChanged("Changed after cut".to_owned()),
        },
    );
    complete(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        operation,
        false,
    );
    f.service
        .handle(
            ConnectionId(1),
            ClientMessage::CancelMedia {
                authority: client.authority().unwrap(),
            },
        )
        .unwrap();
    let view = client.read_media(loops[0]).unwrap();
    f.service.handle(ConnectionId(1), view).unwrap();
    let image = (0..100)
        .find_map(|index| {
            let _ = submit(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                Command::Track {
                    track,
                    action: TrackAction::OutputGainChanged(-6. - index as f32 * 0.01),
                },
            );
            f.service.tick(STEP);
            drain(&mut f.service, &mut client, ConnectionId(1), false);
            client.take_captured_image().map(|(_, image)| image)
        })
        .expect("capture must progress while gain controls continue");
    let persistence_image::Manifest::Session(session) = &image.manifest else {
        panic!()
    };
    assert_eq!(session.tracks[1].loops[0].name, old_name);
    assert_eq!(session.tracks[1].controls.output_gain_db, 0.);
    assert_eq!(
        client.snapshot().unwrap().tracks[1].loops[0].name,
        "Changed after cut"
    );
    assert!(client.snapshot().unwrap().tracks[1].controls.output_gain_db < 0.);
    let persistence_image::Payload::Audio(samples) = &image.payloads[0] else {
        panic!()
    };
    assert!(samples.iter().all(|sample| *sample == 0.25));
    for _ in 0..100 {
        if matches!(client.media(), Some(Ok(_))) {
            break;
        }
        f.service.tick(STEP);
        drain(&mut f.service, &mut client, ConnectionId(1), false);
    }
    assert!(
        matches!(client.media(), Some(Ok(_))),
        "the deferred media view must resume after capture"
    );
    assert_eq!(f.waits.get(), f.initial_waits);
}
#[shoop_test]
fn empty_session_load_and_new_session_use_fresh_epochs_and_remain_usable() {
    use std::sync::Arc;
    let mut f = fixture();
    let mut client = attach(&mut f.service, 1003, ConnectionId(1), false);
    let mut image = (*capture_image(&mut f.service, &mut client, 1, false)).clone();
    release_image(&mut f.service, &mut client, 1, 1, false);
    let persistence_image::Manifest::Session(session) = &mut image.manifest else {
        panic!()
    };
    session.tracks.clear();
    session.routes.clear();
    session.selected.clear();
    session.target = None;
    let key = stage_image(&mut f.service, &mut client, 2, Arc::new(image), false);
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::CommitPersistence(key),
    );
    assert!(matches!(
        complete(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            operation,
            false
        ),
        Outcome::Confirmed { .. }
    ));
    assert!(f.service.is_ready());
    assert!(client.snapshot().unwrap().tracks.is_empty());
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        new_track("No sync source", false),
    );
    created(complete(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        operation,
        false,
    ));
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::NewSession,
    );
    assert!(matches!(
        complete(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            operation,
            false
        ),
        Outcome::Confirmed { .. }
    ));
    assert_eq!(client.snapshot().unwrap().persistence_epoch, 3);
    assert_eq!(client.snapshot().unwrap().tracks.len(), 1);
    assert!(client.snapshot().unwrap().tracks[0].is_sync);
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        new_track("After reset", false),
    );
    created(complete(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        operation,
        false,
    ));
    assert_eq!(f.waits.get(), f.initial_waits);
}
#[shoop_test]
fn selected_loop_capture_continues_while_another_loop_records() {
    let mut f = fixture();
    let mut client = attach(&mut f.service, 1004, ConnectionId(1), false);
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Global(GlobalControlAction::SetSync(false)),
    );
    complete(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        operation,
        false,
    );
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        new_track("Music", true),
    );
    let (track, loops) = created(complete(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        operation,
        false,
    ));
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::RecordClicked,
        },
    );
    complete(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        operation,
        false,
    );
    let key = persistence::Key {
        transfer: TransferId(1),
        epoch: 1,
    };
    let revision = client.snapshot().unwrap().tracks[1].loops[1].content_revision;
    let target = persistence::Target::Loop {
        loop_id: loops[1],
        content_revision: revision,
    };
    f.service
        .handle(ConnectionId(1), client.capture_image(key, target).unwrap())
        .unwrap();
    let image = (0..100)
        .find_map(|_| {
            f.service.tick(STEP);
            drain(&mut f.service, &mut client, ConnectionId(1), false);
            client.take_captured_image().map(|(_, image)| image)
        })
        .expect("selected capture completion");
    assert!(
        matches!(&image.manifest, persistence_image::Manifest::Loop { content, .. } if content.id == loops[1])
    );
    assert_eq!(
        client.snapshot().unwrap().tracks[1].loops[0].mode,
        LoopMode::Recording
    );
}

#[shoop_test]
fn large_private_validation_yields_to_controls_and_cancel_reclaims_the_job() {
    use std::sync::Arc;
    let mut f = fixture();
    let mut client = attach(&mut f.service, 10, ConnectionId(1), false);
    let original = capture_image(&mut f.service, &mut client, 1, false);
    let epoch = client.snapshot().unwrap().persistence_epoch;
    release_image(&mut f.service, &mut client, 1, epoch, false);
    let identities = client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .flat_map(|t| &t.loops)
        .map(|l| l.id)
        .collect::<Vec<_>>();
    let mut large = (*original).clone();
    let persistence_image::Manifest::Session(session) = &mut large.manifest else {
        panic!()
    };
    let frames = 8 * 1024 * 1024 + 1;
    let content = &mut session.tracks[0].loops[0];
    content.frames = frames;
    content.audio[0].frames = frames;
    content.audio[0].object = Some(1);
    large.payloads = vec![persistence_image::Payload::Audio(Arc::new(
        vec![0.25; frames as usize],
    ))];
    let key = persistence::Key {
        transfer: TransferId(2),
        epoch,
    };
    let target = persistence::Target::Session {
        revision: client.snapshot().unwrap().revision,
    };
    f.service
        .handle(
            ConnectionId(1),
            client.stage_image(key, target, Arc::new(large)).unwrap(),
        )
        .unwrap();
    f.service.tick(STEP);
    drain(&mut f.service, &mut client, ConnectionId(1), false);
    assert!(
        client.take_persistence_reply().is_none(),
        "large validation must yield before ready"
    );
    let operation = submit(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Global(GlobalControlAction::SetSolo(true)),
    );
    for _ in 0..4 {
        f.service.tick(STEP);
        drain(&mut f.service, &mut client, ConnectionId(1), false);
    }
    assert!(matches!(
        client.results().get(&operation),
        Some(Outcome::Confirmed { .. })
    ));
    assert!(client.snapshot().unwrap().global.solo);
    assert!(client.take_persistence_reply().is_none());
    persistence_message(
        &mut f.service,
        &mut client,
        persistence::Request::Cancel { key },
        false,
    );
    assert!(
        matches!(client.take_persistence_reply(), Some(persistence::Reply::Closed { key: closed }) if closed == key)
    );
    let next = stage_image(&mut f.service, &mut client, 3, original, false);
    persistence_message(
        &mut f.service,
        &mut client,
        persistence::Request::Cancel { key: next },
        false,
    );
    assert_eq!(client.snapshot().unwrap().persistence_epoch, epoch);
    assert_eq!(
        client
            .snapshot()
            .unwrap()
            .tracks
            .iter()
            .flat_map(|t| &t.loops)
            .map(|l| l.id)
            .collect::<Vec<_>>(),
        identities
    );
    assert_eq!(f.waits.get(), f.initial_waits);
}

fn observe_host_result(
    service: &mut Service,
    client: &mut Client,
    connection: ConnectionId,
    operation: OperationId,
    encoded: bool,
) -> Outcome {
    for _ in 0..100 {
        service.tick(STEP);
        drain(service, client, connection, encoded);
        if let Some(result) = client
            .snapshot()
            .unwrap()
            .results
            .iter()
            .find(|result| result.id == operation)
        {
            return result.outcome.clone();
        }
    }
    panic!("host operation did not finish");
}

#[shoop_test]
fn graph_commands_wait_for_host_observation_and_reconcile_after_ui_detach() {
    use shoop_port_graph::{
        self as graph,
        testing::{perform, FakeProvider},
    };
    for encoded in [false, true] {
        let mut f = fixture();
        let mut provider = FakeProvider::new(4);
        let mut control = provider.control();
        let mut client = attach(&mut f.service, 1, ConnectionId(1), encoded);
        let logical = client
            .snapshot()
            .unwrap()
            .connections
            .application_ports
            .iter()
            .find(|port| {
                port.data_type == PortDataType::Audio && port.direction == PortDirection::Output
            })
            .unwrap()
            .id;
        let key = graph::LocalPortKey(1001);
        let graph::Outcome::Published { port: local, .. } = perform(
            &mut control,
            graph::Operation::Publish {
                key,
                spec: graph::PortSpec {
                    name: "shared label".into(),
                    data_type: PortDataType::Audio,
                    direction: PortDirection::Output,
                },
            },
            || provider.advance(),
        )
        .unwrap() else {
            panic!()
        };
        let peer = provider.peer(graph::PortSpec {
            name: "shared label".into(),
            data_type: PortDataType::Audio,
            direction: PortDirection::Input,
        });
        f.service
            .set_host_graph_controller(Box::new(control))
            .unwrap();
        f.service.bind_host_publication(logical, Some(key)).unwrap();
        assert!(f
            .service
            .set_host_graph_controller(Box::new(provider.control()))
            .is_err());
        let other = client
            .snapshot()
            .unwrap()
            .connections
            .application_ports
            .iter()
            .find(|port| {
                port.id != logical
                    && port.data_type == PortDataType::Audio
                    && port.direction == PortDirection::Output
            })
            .unwrap()
            .id;
        assert!(f.service.bind_host_publication(other, Some(key)).is_err());

        let request = HostGraphRequest {
            epoch: 4,
            local,
            peer,
            connected: true,
        };
        let id = submit(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::SetHostGraphConnected(request.clone()),
        );
        for _ in 0..4 {
            f.service.tick(STEP);
        }
        drain(&mut f.service, &mut client, ConnectionId(1), encoded);
        assert!(client
            .snapshot()
            .unwrap()
            .pending
            .iter()
            .any(|p| p.id == id));
        assert!(!client.results().contains_key(&id));
        assert!(client.snapshot().unwrap().host_graph.links.is_empty());
        let stale_message = client
            .command(Command::SetHostGraphConnected(request.clone()), None)
            .unwrap();
        f.service.detach(ConnectionId(1));
        provider.advance();
        assert!(!provider.control().snapshot().connected(local, peer));
        provider.advance();
        f.service.tick(STEP);
        let mut client = attach(&mut f.service, 2, ConnectionId(2), encoded);
        let state = client.snapshot().unwrap();
        assert!(state.host_graph.valid());
        assert_eq!(state.host_graph.links.len(), 1);
        assert!(state.pending.is_empty());
        assert!(state
            .results
            .iter()
            .any(|r| r.id == id && matches!(r.outcome, Outcome::Confirmed { .. })));
        let binding = state
            .connections
            .application_ports
            .iter()
            .find(|port| port.id == logical)
            .unwrap()
            .publication
            .unwrap();
        assert!(
            matches!(state.host_graph.publication(binding).unwrap().state, HostGraphPublicationState::Published(port) if port == local)
        );
        assert_eq!(
            f.service.handle(ConnectionId(1), stale_message),
            Err(Refusal::StaleAuthority)
        );
        provider.fail_next(graph::Error::new(
            graph::ErrorKind::Backend,
            "connection refused by provider",
        ));
        let id = submit(
            &mut f.service,
            &mut client,
            ConnectionId(2),
            Command::SetHostGraphConnected(HostGraphRequest {
                connected: false,
                ..request
            }),
        );
        provider.advance();
        provider.advance();
        assert!(matches!(
            complete(&mut f.service, &mut client, ConnectionId(2), id, encoded),
            Outcome::Refused {
                reason: Refusal::Engine,
                ..
            }
        ));
        assert_eq!(client.snapshot().unwrap().host_graph.links.len(), 1);
        f.service.bind_host_publication(logical, None).unwrap();
    }
}
#[shoop_test]
fn graph_control_is_explicitly_unsupported_without_a_host_adapter() {
    let mut f = fixture();
    let mut client = attach(&mut f.service, 1, ConnectionId(1), false);
    assert_eq!(
        client.snapshot().unwrap().host_graph.status,
        HostGraphStatus::Unsupported
    );
    let outcome = command(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::SetHostGraphConnected(HostGraphRequest {
            epoch: 1,
            local: HostGraphPortId(1),
            peer: HostGraphPortId(2),
            connected: true,
        }),
    );
    assert!(matches!(
        outcome,
        Outcome::Refused {
            reason: Refusal::Unsupported,
            ..
        }
    ));
}

struct BoundProvider {
    control: shoop_port_graph::Control,
    refuse_unpublish: Arc<Mutex<Option<(String, shoop_port_graph::ErrorKind)>>>,
}
impl shoop_port_graph::Controller for BoundProvider {
    fn supports_io_binding(&self) -> bool {
        true
    }
    fn io_binding(
        &self,
        key: shoop_port_graph::LocalPortKey,
    ) -> Option<shoop_port_graph::IoBinding> {
        use shoop_port_graph::{DataType, Direction, PublicationState};
        let snapshot = self.control.snapshot();
        let publication = snapshot.publication(key)?;
        if !matches!(publication.state, PublicationState::Published(_)) {
            return None;
        }
        let port = match (publication.spec.data_type, publication.spec.direction) {
            (DataType::Audio, Direction::Input) => "audio_in_1",
            (DataType::Audio, Direction::Output) => "audio_out_1",
            (DataType::Midi, Direction::Input) => "midi_in_1",
            (DataType::Midi, Direction::Output) => "midi_out_1",
        };
        Some(shoop_port_graph::IoBinding { port: port.into() })
    }
    fn snapshot(&mut self) -> std::sync::Arc<shoop_port_graph::Snapshot> {
        self.control.snapshot()
    }
    fn submit(
        &mut self,
        request: shoop_port_graph::Request,
    ) -> Result<shoop_port_graph::OperationId, shoop_port_graph::Error> {
        if let shoop_port_graph::Operation::Unpublish { key } = &request.operation {
            let mut refusal = self.refuse_unpublish.lock().unwrap();
            if refusal.as_ref().is_some_and(|(name, _)| {
                self.control
                    .snapshot()
                    .publication(*key)
                    .is_some_and(|port| port.spec.name == *name)
            }) {
                let (_, kind) = refusal.take().unwrap();
                return Err(shoop_port_graph::Error::new(
                    kind,
                    "unpublish admission refused",
                ));
            }
        }
        if let shoop_port_graph::Operation::Publish { key, spec } = &request.operation {
            assert!(
                !self
                    .control
                    .snapshot()
                    .ports
                    .iter()
                    .any(|port| { port.name == spec.name && port.local != Some(*key) }),
                "publication reused a physical name before retirement"
            );
        }
        self.control.submit(request)
    }
    fn take_result(&mut self) -> Option<shoop_port_graph::Completion> {
        self.control.take_result()
    }
}
#[shoop_test]
fn application_publication_is_provider_independent_and_failed_creation_rolls_back() {
    use shoop_port_graph::{self as graph, testing::FakeProvider};
    for encoded in [false, true] {
        let mut f = fixture();
        let mut provider = FakeProvider::new(9);
        f.service
            .set_host_graph_controller(Box::new(BoundProvider {
                control: provider.control(),
                refuse_unpublish: Arc::new(Mutex::new(None)),
            }))
            .unwrap();
        for _ in 0..32 {
            provider.advance();
            f.service.tick(STEP);
        }
        assert!(f.service.is_ready());
        let mut client = attach(&mut f.service, 1, ConnectionId(1), encoded);
        let initial = client.snapshot().unwrap().host_graph.publications.len();
        assert_eq!(initial, 4);
        let spec = TrackSpec {
            name: "Published".into(),
            topology: TrackSpecTopology::Direct {
                audio_channels: 1,
                midi: true,
            },
            latency: Default::default(),
            initial_output_bus_names: Vec::new(),
            creation_request_id: None,
        };
        let id = submit(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::AddTrack(spec.clone()),
        );
        for _ in 0..8 {
            f.service.tick(STEP);
        }
        drain(&mut f.service, &mut client, ConnectionId(1), encoded);
        assert!(
            !client.results().contains_key(&id),
            "engine acknowledgement alone cannot finish creation"
        );
        provider.fail_next(graph::Error::new(
            graph::ErrorKind::Backend,
            "publication failed",
        ));
        for _ in 0..64 {
            provider.advance();
            f.service.tick(STEP);
            drain(&mut f.service, &mut client, ConnectionId(1), encoded);
        }
        assert!(matches!(
            client.results()[&id],
            Outcome::Refused {
                reason: Refusal::Engine,
                ..
            }
        ));
        assert_eq!(client.snapshot().unwrap().tracks.len(), 1);
        assert_eq!(
            client.snapshot().unwrap().host_graph.publications.len(),
            initial
        );
        let id = submit(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::AddTrack(spec),
        );
        for _ in 0..32 {
            provider.advance();
            f.service.tick(STEP);
            drain(&mut f.service, &mut client, ConnectionId(1), encoded);
        }
        let Outcome::TrackCreated { track, .. } = client.results()[&id] else {
            panic!("{:?}", client.results()[&id])
        };
        assert_eq!(
            client.snapshot().unwrap().host_graph.publications.len(),
            initial + 4
        );
        let id = submit(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::Track {
                track,
                action: TrackAction::Remove,
            },
        );
        for _ in 0..8 {
            f.service.tick(STEP);
        }
        drain(&mut f.service, &mut client, ConnectionId(1), encoded);
        assert!(
            !client.results().contains_key(&id),
            "removal waits for provider reclamation"
        );
        if encoded {
            provider.fail_next(graph::Error::new(
                graph::ErrorKind::Backend,
                "retirement failed",
            ));
        }
        for _ in 0..32 {
            provider.advance();
            f.service.tick(STEP);
            drain(&mut f.service, &mut client, ConnectionId(1), encoded);
        }
        if encoded {
            assert!(matches!(
                client.results()[&id],
                Outcome::Refused {
                    reason: Refusal::Engine,
                    ..
                }
            ));
        } else {
            assert!(matches!(client.results()[&id], Outcome::Confirmed { .. }));
        }
        assert_eq!(
            client.snapshot().unwrap().host_graph.publications.len(),
            initial
        );
        assert_eq!(f.waits.get(), f.initial_waits);
    }
}

#[shoop_test]
fn profiling_labels_resolve_backend_slots_and_follow_ui_renames_over_the_wire() {
    for encoded in [false, true] {
        let mut f = fixture();
        let connection = ConnectionId(1);
        let mut client = attach(&mut f.service, 9001, connection, encoded);
        let id = submit(
            &mut f.service,
            &mut client,
            connection,
            new_track("Drums", false),
        );
        let (track, loops) = created(complete(
            &mut f.service,
            &mut client,
            connection,
            id,
            encoded,
        ));
        let mut request = shoop_app_api::profiling_data::wire::subscribe(true);
        request.epoch = f.service.audio_profiling_view().connection.max(1);
        let id = submit(
            &mut f.service,
            &mut client,
            connection,
            Command::Diagnostic(request),
        );
        complete(&mut f.service, &mut client, connection, id, encoded);
        for _ in 0..32 {
            f.service.tick(STEP);
            drain(&mut f.service, &mut client, connection, encoded);
        }
        let view = f.service.audio_profiling_view();
        let profiling = &view;
        assert!(profiling
            .groups
            .iter()
            .any(|g| g.kind == 1 && g.name == "Drums"));
        assert!(profiling
            .groups
            .iter()
            .any(|g| g.kind == 2 && g.name.starts_with("Drums / ")));
        let groups = profiling.groups.clone();
        let topology = profiling.topology.clone().unwrap();
        command(
            &mut f.service,
            &mut client,
            connection,
            Command::Track {
                track,
                action: TrackAction::NameChanged("Percussion".into()),
            },
        );
        command(
            &mut f.service,
            &mut client,
            connection,
            Command::Loop {
                track,
                loop_id: loops[0],
                action: LoopAction::NameChanged("Verse".into()),
            },
        );
        f.service.tick(STEP);
        drain(&mut f.service, &mut client, connection, encoded);
        let renamed = f.service.audio_profiling_view();
        assert_eq!(renamed.topology.as_deref(), Some(topology.as_ref()));
        assert!(renamed
            .groups
            .iter()
            .any(|g| g.kind == 2 && g.name == "Percussion / Verse"));
        assert_eq!(
            groups
                .iter()
                .map(|g| (g.kind, g.slot, g.entity))
                .collect::<Vec<_>>(),
            renamed
                .groups
                .iter()
                .map(|g| (g.kind, g.slot, g.entity))
                .collect::<Vec<_>>()
        );
        for _ in 0..16 {
            f.service.tick(STEP);
            drain(&mut f.service, &mut client, connection, encoded);
        }
        assert_eq!(client.profiling().groups, renamed.groups);
    }
}

#[shoop_test]
fn session_replacement_retains_publications_until_unpublish_admission_and_completion() {
    use shoop_port_graph::{self as graph, testing::FakeProvider};
    for encoded in [false, true] {
        for kind in [graph::ErrorKind::Unavailable, graph::ErrorKind::Backend] {
            let mut f = fixture();
            let mut provider = FakeProvider::new(9);
            let refusal = Arc::new(Mutex::new(None));
            f.service
                .set_host_graph_controller(Box::new(BoundProvider {
                    control: provider.control(),
                    refuse_unpublish: Arc::clone(&refusal),
                }))
                .unwrap();
            for _ in 0..32 {
                provider.advance();
                f.service.tick(STEP);
            }
            assert!(f.service.is_ready());
            let mut client = attach(&mut f.service, 1, ConnectionId(1), encoded);
            let old: Vec<_> = client.snapshot().unwrap().host_graph.publications.clone();
            assert_eq!(old.len(), 4);
            let image = capture_image(&mut f.service, &mut client, 1, encoded);
            release_image(&mut f.service, &mut client, 1, 1, encoded);
            let key = stage_image(&mut f.service, &mut client, 2, image, encoded);
            *refusal.lock().unwrap() = Some(("sync_loop_direct_in".into(), kind));
            let id = submit(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                Command::CommitPersistence(key),
            );
            for _ in 0..16 {
                f.service.tick(STEP);
                drain(&mut f.service, &mut client, ConnectionId(1), encoded);
            }
            assert!(refusal.lock().unwrap().is_none());
            if kind == graph::ErrorKind::Unavailable {
                assert!(!client.results().contains_key(&id));
            } else {
                assert!(matches!(
                    client.results()[&id],
                    Outcome::Refused {
                        reason: Refusal::Engine,
                        ..
                    }
                ));
            }
            assert_eq!(provider.control().snapshot().publications, old);
            for _ in 0..64 {
                provider.advance();
                f.service.tick(STEP);
                drain(&mut f.service, &mut client, ConnectionId(1), encoded);
            }
            assert_eq!(
                matches!(client.results()[&id], Outcome::Confirmed { .. }),
                kind == graph::ErrorKind::Unavailable
            );
            let snapshot = client.snapshot().unwrap();
            assert_eq!(snapshot.host_graph.publications.len(), old.len());
            for previous in old {
                assert!(snapshot.host_graph.publication(previous.key).is_none());
                if previous.spec.name.starts_with("sync_loop_") {
                    assert_eq!(
                        snapshot
                            .host_graph
                            .publications
                            .iter()
                            .filter(|current| current.spec.name == previous.spec.name)
                            .count(),
                        1
                    );
                }
            }
            assert_eq!(f.waits.get(), f.initial_waits);
        }
    }
}

#[path = "support/composite_cases.rs"]
mod composite_cases;
#[shoop_test]
fn timing_commands_are_atomic_while_recording_and_reconcile_once_after_ui_loss() {
    for encoded in [false, true] {
        let mut f = fixture();
        let connection = ConnectionId(1);
        let mut client = attach(&mut f.service, 12000, connection, encoded);
        let (track, loops) = created(command(
            &mut f.service,
            &mut client,
            connection,
            new_track("Timing", true),
        ));
        command(
            &mut f.service,
            &mut client,
            connection,
            Command::Global(GlobalControlAction::SetSync(false)),
        );
        let target = loops[0];
        assert!(client
            .snapshot()
            .unwrap()
            .capabilities
            .supports(BackendCapability::LoopTimingEdits));
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                connection,
                Command::Loop {
                    track,
                    loop_id: target,
                    action: LoopAction::RecordClicked
                }
            ),
            Outcome::Confirmed { .. }
        ));
        let before = f.service.snapshot().tracks[1].loops[0].clone();
        assert_eq!(before.mode, LoopMode::Recording);
        assert_eq!(before.channel_timings.len(), 2);
        let changes = before
            .channel_timings
            .iter()
            .enumerate()
            .map(|(index, channel)| ChannelTimelineUpdate {
                channel_id: channel.id,
                start_offset: Some(index as i64 - 2),
                preplay_samples: Some(index as u64 + 1),
            })
            .collect::<Vec<_>>();
        let mut message = client
            .command(
                Command::SetLoopChannelTimings {
                    loop_id: target,
                    updates: changes.clone(),
                },
                None,
            )
            .unwrap();
        if encoded {
            let mut bytes = [0; shoop_transport::MAX_RECORD_BYTES];
            let size = codec::encode(&WireMessage::Client(message), &mut bytes).unwrap();
            let WireMessage::Client(decoded) = codec::decode(&bytes[..size]).unwrap() else {
                panic!()
            };
            message = decoded;
        }
        let duplicate = message.clone();
        f.service.handle(connection, message).unwrap();
        f.service.handle(connection, duplicate.clone()).unwrap();
        f.service.detach(connection);
        for _ in 0..8 {
            f.service.tick(STEP);
        }
        let mut client = attach(&mut f.service, 12001, ConnectionId(2), encoded);
        let current = client.snapshot().unwrap().tracks[1].loops[0].clone();
        assert_eq!(current.mode, LoopMode::Recording);
        assert!(current.length_frames > before.length_frames);
        assert_eq!(current.timing_revision, before.timing_revision + 1);
        for (channel, change) in current.channel_timings.iter().zip(&changes) {
            assert_eq!(Some(channel.start_offset), change.start_offset);
            assert_eq!(Some(channel.preplay_samples), change.preplay_samples);
            assert_eq!(channel.capture_alignment_frames, 0);
        }
        assert!(f.service.handle(ConnectionId(2), duplicate).is_err());
        for _ in 0..4 {
            f.service.tick(STEP);
        }
        assert_eq!(
            f.service.snapshot().tracks[1].loops[0].timing_revision,
            current.timing_revision
        );
        let refused = command(
            &mut f.service,
            &mut client,
            ConnectionId(2),
            Command::SetLoopTimeline {
                loop_id: target,
                start_offset: Some(50),
                preplay_samples: Some(50),
                loop_length: Some(16),
            },
        );
        assert!(matches!(refused, Outcome::Refused { .. }));
        let unchanged = f.service.snapshot().tracks[1].loops[0].clone();
        assert_eq!(unchanged.channel_timings, current.channel_timings);
        assert_eq!(unchanged.timing_revision, current.timing_revision);
        assert!(matches!(
            command(
                &mut f.service,
                &mut client,
                ConnectionId(2),
                Command::SetLoopChannelTimings {
                    loop_id: target,
                    updates: vec![changes[0].clone(), changes[0].clone()]
                }
            ),
            Outcome::Refused { .. }
        ));
        command(
            &mut f.service,
            &mut client,
            ConnectionId(2),
            Command::Loop {
                track,
                loop_id: target,
                action: LoopAction::StopClicked,
            },
        );
        let revised = command(
            &mut f.service,
            &mut client,
            ConnectionId(2),
            Command::SetLoopTimeline {
                loop_id: target,
                start_offset: None,
                preplay_samples: None,
                loop_length: Some(16),
            },
        );
        assert!(matches!(revised, Outcome::Confirmed { .. }));
        assert_eq!(f.service.snapshot().tracks[1].loops[0].length_frames, 16);
        assert_eq!(
            f.waits.get(),
            f.initial_waits,
            "service timing commands must not wait for render"
        );
    }
}

#[shoop_test]
fn lua_observes_confirmed_manual_lengths_and_change_events_without_restarting() {
    let mut f = fixture();
    let (mut runtime, port) = runtime(
        &mut f.service,
        shoop_app_client::runtime::Startup::Fresh(Vec::new()),
    );
    runtime
        .dispatch(AppIntent::AddTrack(DirectTrackSpec {
            name: "Lengths".into(),
            audio_channels: 0,
            midi: true,
        }))
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 20);
    runtime.dispatch(AppIntent::AddScriptSource {
        name: "lengths.lua".into(), kind: ScriptKind::User, enabled: true,
        source: Arc::from("shoop_announce_api_version(1,0); local c=require('shoop_control'); print('started-once'); c.register_loop_event_cb(function() print('length=' .. c.loop_get_length({0,0})[1]) end)"),
    }).unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 4);
    let id = runtime.snapshot().tracks[1].loops[0].id;
    runtime
        .dispatch(AppIntent::SetLoopTimeline {
            loop_id: id,
            start_offset: None,
            preplay_samples: Some(3),
            loop_length: Some(64),
        })
        .unwrap();
    pump_runtime(&mut f.service, &mut runtime, &port, 8);
    let state = runtime.snapshot();
    assert_eq!(state.tracks[1].loops[0].length_frames, 64);
    let script = &state.scripting.scripts[0];
    assert!(script.latest_error.is_none(), "{:?}", script.latest_error);
    assert_eq!(
        script
            .logs
            .iter()
            .filter(|line| line.message.contains("started-once"))
            .count(),
        1
    );
    assert!(
        script
            .logs
            .iter()
            .any(|line| line.message.contains("length=64")),
        "{:?}",
        script.logs
    );
}

#[path = "support/input_cases.rs"]
mod input_cases;

#[path = "profiling/cases.rs"]
mod profiling_cases;
