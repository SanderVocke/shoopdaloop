use super::*;
use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_app_client::Client;
use shoop_app_protocol::{audio_driver::Command as DriverCommand, *};
use shoop_backend::{Backend, FakeBackend};
use shoop_wasm_test_support::shoop_test;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Control {
    starts: usize,
    failures: Vec<usize>,
    restore_failure: bool,
    mapping_failure: bool,
    save_failure: bool,
    saves: usize,
    pending: VecDeque<(u64, DriverWork)>,
    hold: bool,
    preflights: usize,
    changed_rate: bool,
    changed_open: bool,
    reopened_buffer: Option<u32>,
}
struct FakeController(Arc<Mutex<Control>>);
impl AudioDriverController for FakeController {
    fn submit(&mut self, ticket: u64, work: DriverWork) -> Result<(), String> {
        self.0.lock().unwrap().pending.push_back((ticket, work));
        Ok(())
    }
    fn poll(&mut self) -> Option<DriverCompletion> {
        let mut control = self.0.lock().unwrap();
        if control.hold {
            return None;
        }
        let (ticket, work) = control.pending.pop_front()?;
        let result = (|| -> Result<DriverReply> {
            match work {
                DriverWork::Discover(_) => Ok(DriverReply::Discovered(Arc::from([]))),
                DriverWork::Preflight(config) => {
                    control.preflights += 1;
                    let mut resolved = FakeBackend::default().preflight_audio_driver(&config)?;
                    if matches!(&config, AudioDriverConfig::Cpal(cpal) if cpal.buffer_size == 0) {
                        resolved.buffer_size = 0;
                    }
                    if control.changed_rate && control.preflights > 1 {
                        resolved.sample_rate = 32_000;
                    }
                    Ok(DriverReply::Resolved(resolved))
                }
                DriverWork::Convert {
                    image,
                    target,
                    limits,
                } => conversion::convert(image, target, limits).map(DriverReply::Converted),
                DriverWork::Start { mut profile, .. } => {
                    if profile.buffer_size == 0 {
                        profile.buffer_size = 256;
                    }
                    if control.starts > 0 && matches!(&profile.configured, AudioDriverConfig::Cpal(config) if config.buffer_size == 0) {
                        if let Some(buffer) = control.reopened_buffer { profile.buffer_size = buffer; }
                    }
                    if control.changed_open && control.starts == 0 {
                        profile.buffer_size = 128;
                    }
                    control.starts += 1;
                    if control.failures.contains(&control.starts) {
                        return Err(anyhow!("injected start failure {}", control.starts));
                    }
                    let mut backend = FakeBackend::default();
                    let mut empty = backend.capture_session()?;
                    empty.sample_rate = profile.sample_rate;
                    backend.switch_audio_driver(
                        &profile.configured,
                        profile.sample_rate,
                        &empty,
                    )?;
                    if control.restore_failure && control.starts == 1 {
                        backend.fail_next_session_replace("injected restoration failure");
                    }
                    if control.mapping_failure && control.starts == 1 {
                        backend
                            .audio_driver_control()
                            .corrupt_next_replacement_mapping();
                    }
                    Ok(DriverReply::Started {
                        backend: Box::new(backend),
                        graph: None,
                        profile,
                    })
                }
                DriverWork::Persist(_) => {
                    control.saves += 1;
                    if control.save_failure {
                        Err(anyhow!("injected preference failure"))
                    } else {
                        Ok(DriverReply::Persisted)
                    }
                }
            }
        })()
        .map_err(|e| e.to_string());
        Some(DriverCompletion { ticket, result })
    }
}
struct Fixture {
    service: Service,
    client: Client,
    control: Arc<Mutex<Control>>,
    connection: ConnectionId,
    owner: Option<Owner>,
    #[cfg(all(feature = "builtin-fx", feature = "oxisynth"))]
    transfer: u64,
}
impl Fixture {
    fn new() -> Self {
        let mut service = Service::new(
            Box::new(FakeBackend::default()),
            ServiceOptions {
                master_auto_connect: true,
                loop_smoothing_ms: 3,
            },
        )
        .unwrap();
        for _ in 0..5 {
            service.tick(Duration::from_millis(10));
        }
        let control = Arc::new(Mutex::new(Control::default()));
        service
            .set_audio_driver_controller(Box::new(FakeController(control.clone())))
            .unwrap();
        for _ in 0..5 {
            service.tick(Duration::from_millis(10));
        }
        let mut f = Self {
            service,
            client: Client::new(),
            control,
            connection: ConnectionId(1),
            owner: None,
            #[cfg(all(feature = "builtin-fx", feature = "oxisynth"))]
            transfer: 0,
        };
        f.attach();
        f
    }
    fn attach(&mut self) {
        self.service
            .handle(self.connection, self.client.attach().unwrap())
            .unwrap();
        self.drain();
        self.service
            .handle(self.connection, self.client.ready_message().unwrap())
            .unwrap();
        self.client.ready_sent().unwrap();
    }
    fn drain(&mut self) {
        while let Some(update) = self.service.take_update(self.connection) {
            self.client.receive(update).unwrap();
        }
    }
    fn tick(&mut self) {
        self.service.tick(Duration::from_millis(10));
        self.drain();
        if self.owner.is_some() {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn queue(&mut self, command: Command) -> OperationId {
        let message = self.client.command(command, None).unwrap();
        let ClientMessage::Submit { operation, .. } = message else {
            unreachable!()
        };
        self.service.handle(self.connection, message).unwrap();
        self.drain();
        operation
    }
    fn run(&mut self, command: Command) -> Outcome {
        let operation = self.queue(command);
        for _ in 0..30_000 {
            self.tick();
            if let Some(outcome) = self.client.results().get(&operation) {
                return outcome.clone();
            }
        }
        panic!(
            "operation did not finish: {:?}",
            self.service.snapshot().audio_drivers
        );
    }
    fn request(&mut self) -> u64 {
        let config = AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 48_000,
            buffer_size: 64,
        });
        assert!(matches!(
            self.run(Command::AudioDriver(DriverCommand::Request(config))),
            Outcome::Confirmed { .. }
        ));
        assert_eq!(
            self.service.snapshot().audio_drivers.switch.status,
            AudioDriverSwitchStatus::AwaitingConfirmation
        );
        self.service.snapshot().audio_drivers.switch.request_id
    }
    fn confirm(&mut self, request_id: u64) -> Outcome {
        self.run(Command::AudioDriver(DriverCommand::Confirm {
            request_id,
            accept: true,
        }))
    }
}

#[shoop_test]
fn cancel_expiry_and_stale_confirmation_do_not_start_a_driver() {
    let mut f = Fixture::new();
    let before = f.service.snapshot();
    let id = f.request();
    assert!(matches!(
        f.run(Command::AudioDriver(DriverCommand::Confirm {
            request_id: id + 1,
            accept: true
        })),
        Outcome::Refused { .. }
    ));
    assert!(matches!(
        f.run(Command::AudioDriver(DriverCommand::Confirm {
            request_id: id,
            accept: false
        })),
        Outcome::Confirmed { .. }
    ));
    let id = f.request();
    f.service.tick(Duration::from_secs(61));
    assert!(matches!(f.confirm(id), Outcome::Refused { .. }));
    assert_eq!(f.control.lock().unwrap().starts, 0);
    assert_eq!(f.service.snapshot().tracks[0].id, before.tracks[0].id);
}

#[shoop_test]
fn detached_preflight_cannot_become_a_confirmation_for_a_replacement_owner() {
    let mut f = Fixture::new();
    f.control.lock().unwrap().hold = true;
    f.queue(Command::AudioDriver(DriverCommand::Request(
        AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 32_000,
            buffer_size: 128,
        }),
    )));
    f.service.detach(f.connection);
    f.connection = ConnectionId(2);
    f.client = Client::new();
    f.attach();
    f.control.lock().unwrap().hold = false;
    for _ in 0..5 {
        f.tick();
    }
    assert_eq!(
        f.service.snapshot().audio_drivers.switch.status,
        AudioDriverSwitchStatus::Completed
    );
    assert_eq!(f.control.lock().unwrap().starts, 0);
}

#[shoop_test]
fn target_and_restoration_failures_restore_the_original_session() {
    for failure in 0..3 {
        let mut f = Fixture::new();
        let before = f.service.snapshot();
        if failure == 2 {
            f.control.lock().unwrap().mapping_failure = true;
        } else if failure == 1 {
            f.control.lock().unwrap().restore_failure = true;
        } else {
            f.control.lock().unwrap().failures.push(1);
        }
        let id = f.request();
        assert!(matches!(f.confirm(id), Outcome::Refused { .. }));
        let after = f.service.snapshot();
        assert_eq!(
            after.audio_drivers.switch.status,
            AudioDriverSwitchStatus::Failed
        );
        assert!(
            after
                .audio_drivers
                .switch
                .message
                .contains("prior driver and session were restored"),
            "{}",
            after.audio_drivers.switch.message
        );
        assert_eq!(before.tracks[0].id, after.tracks[0].id);
        assert_eq!(before.buses[0].id, after.buses[0].id);
        assert_eq!(after.audio_drivers.active, before.audio_drivers.active);
        assert_eq!(f.control.lock().unwrap().starts, 2);
    }
}

#[shoop_test]
fn failed_rollback_reports_both_errors_and_a_fatal_runtime() {
    let mut f = Fixture::new();
    f.control.lock().unwrap().failures = vec![1, 2];
    let id = f.request();
    assert!(matches!(f.confirm(id), Outcome::Refused { .. }));
    let state = f.service.snapshot();
    assert_eq!(
        state.audio_drivers.switch.status,
        AudioDriverSwitchStatus::Fatal
    );
    assert!(state.audio_drivers.active.is_none());
    assert!(state
        .audio_drivers
        .switch
        .message
        .contains("start failure 1"));
    assert!(state
        .audio_drivers
        .switch
        .message
        .contains("start failure 2"));
    f.service.detach(f.connection);
    f.connection = ConnectionId(2);
    f.client = Client::new();
    f.attach();
    assert_eq!(
        f.client.snapshot().unwrap().audio_drivers.switch.status,
        AudioDriverSwitchStatus::Fatal
    );
    let message = f.client.shutdown().unwrap();
    let ClientMessage::Shutdown { operation, .. } = message else {
        unreachable!()
    };
    f.service.handle(f.connection, message).unwrap();
    f.tick();
    assert!(f.service.shutdown_ready());
    f.service.complete_shutdown(Ok(()));
    f.drain();
    assert!(matches!(
        f.client.results().get(&operation),
        Some(Outcome::Confirmed { .. })
    ));
}

#[shoop_test]
fn persistence_retry_keeps_the_new_driver_and_never_starts_it_again() {
    let mut f = Fixture::new();
    f.control.lock().unwrap().save_failure = true;
    let id = f.request();
    assert!(matches!(f.confirm(id), Outcome::Confirmed { .. }));
    for _ in 0..5 {
        f.tick();
    }
    assert!(
        f.service
            .snapshot()
            .audio_drivers
            .switch
            .persistence_retry_available
    );
    assert_eq!(
        f.service
            .snapshot()
            .audio_drivers
            .active
            .as_ref()
            .unwrap()
            .buffer_size,
        64
    );
    f.service.detach(f.connection);
    f.connection = ConnectionId(2);
    f.client = Client::new();
    f.attach();
    f.control.lock().unwrap().save_failure = false;
    assert!(matches!(
        f.run(Command::AudioDriver(DriverCommand::RetryPersistence {
            request_id: id
        })),
        Outcome::Confirmed { .. }
    ));
    assert_eq!(
        f.service.snapshot().audio_drivers.switch.status,
        AudioDriverSwitchStatus::Completed
    );
    assert_eq!(f.control.lock().unwrap().starts, 1);
    assert_eq!(f.control.lock().unwrap().saves, 2);
}

#[shoop_test]
fn changed_preflight_rate_requires_new_consent_without_teardown() {
    let mut f = Fixture::new();
    f.control.lock().unwrap().changed_rate = true;
    let id = f.request();
    assert!(matches!(f.confirm(id), Outcome::Confirmed { .. }));
    let state = f.service.snapshot();
    assert_eq!(
        state.audio_drivers.switch.status,
        AudioDriverSwitchStatus::AwaitingConfirmation
    );
    assert_eq!(
        state
            .audio_drivers
            .switch
            .target
            .as_ref()
            .unwrap()
            .sample_rate,
        32_000
    );
    assert_eq!(state.status.sample_rate, 48_000);
    assert_eq!(f.control.lock().unwrap().starts, 0);
}

#[shoop_test]
fn automatic_buffer_consent_accepts_the_opened_size_without_reconfirmation() {
    let mut f = Fixture::new();
    assert!(matches!(
        f.run(Command::AudioDriver(DriverCommand::Request(
            AudioDriverConfig::Cpal(CpalAudioDriverConfig::default())
        ))),
        Outcome::Confirmed { .. }
    ));
    let state = f.service.snapshot();
    assert_eq!(
        state
            .audio_drivers
            .switch
            .target
            .as_ref()
            .unwrap()
            .buffer_size,
        0
    );
    assert!(state
        .audio_drivers
        .switch
        .message
        .contains("device default (negotiated on open)"));
    let id = state.audio_drivers.switch.request_id;
    assert!(matches!(f.confirm(id), Outcome::Confirmed { .. }));
    for _ in 0..5 {
        f.tick();
    }
    assert_eq!(
        f.service.snapshot().audio_drivers.switch.status,
        AudioDriverSwitchStatus::Completed
    );
    assert_eq!(
        f.service
            .snapshot()
            .audio_drivers
            .active
            .as_ref()
            .unwrap()
            .buffer_size,
        256
    );
    assert_eq!(f.control.lock().unwrap().starts, 1);
}

#[shoop_test]
fn rollback_to_automatic_cpal_accepts_a_different_observed_callback_size() {
    let mut f = Fixture::new();
    assert!(matches!(
        f.run(Command::AudioDriver(DriverCommand::Request(
            AudioDriverConfig::Cpal(CpalAudioDriverConfig::default())
        ))),
        Outcome::Confirmed { .. }
    ));
    let id = f.service.snapshot().audio_drivers.switch.request_id;
    assert!(matches!(f.confirm(id), Outcome::Confirmed { .. }));
    for _ in 0..5 {
        f.tick();
    }
    {
        let mut control = f.control.lock().unwrap();
        control.failures.push(2);
        control.reopened_buffer = Some(512);
    }
    let id = f.request();
    assert!(matches!(f.confirm(id), Outcome::Refused { .. }));
    let state = f.service.snapshot();
    assert_eq!(
        state.audio_drivers.switch.status,
        AudioDriverSwitchStatus::Failed
    );
    assert!(state
        .audio_drivers
        .switch
        .message
        .contains("prior driver and session were restored"));
    assert_eq!(
        state
            .audio_drivers
            .active
            .as_ref()
            .unwrap()
            .configured
            .kind(),
        AudioDriverKind::Cpal
    );
    assert_eq!(f.control.lock().unwrap().starts, 3);
}

#[shoop_test]
fn changed_opened_buffer_restores_source_before_renewed_confirmation() {
    let mut f = Fixture::new();
    let before = f.service.snapshot();
    f.control.lock().unwrap().changed_open = true;
    let id = f.request();
    assert!(matches!(f.confirm(id), Outcome::Confirmed { .. }));
    let state = f.service.snapshot();
    assert_eq!(
        state.audio_drivers.switch.status,
        AudioDriverSwitchStatus::AwaitingConfirmation
    );
    assert_eq!(state.audio_drivers.active, before.audio_drivers.active);
    assert_eq!(
        state
            .audio_drivers
            .switch
            .target
            .as_ref()
            .unwrap()
            .buffer_size,
        128
    );
    assert_eq!(f.control.lock().unwrap().starts, 2);
}

#[shoop_test]
fn admitted_switch_refuses_concurrent_mutations_and_duplicate_confirmation() {
    let mut f = Fixture::new();
    let id = f.request();
    f.control.lock().unwrap().hold = true;
    f.queue(Command::AudioDriver(DriverCommand::Confirm {
        request_id: id,
        accept: true,
    }));
    for command in [
        Command::AudioDriver(DriverCommand::Confirm {
            request_id: id,
            accept: true,
        }),
        Command::NewSession,
        Command::Global(GlobalControlAction::StopAll),
    ] {
        assert!(matches!(
            f.run(command),
            Outcome::Refused {
                reason: Refusal::Busy,
                ..
            }
        ));
    }
    f.control.lock().unwrap().hold = false;
    for _ in 0..50 {
        f.tick();
    }
    assert_eq!(
        f.service.snapshot().audio_drivers.switch.status,
        AudioDriverSwitchStatus::Completed
    );
    assert_eq!(f.control.lock().unwrap().starts, 1);
}

#[shoop_test]
fn a_confirmed_switch_finishes_after_detach_and_reconnect_does_not_replay_it() {
    let mut f = Fixture::new();
    let before = f.service.snapshot();
    let id = f.request();
    f.control.lock().unwrap().hold = true;
    let operation = f.queue(Command::AudioDriver(DriverCommand::Confirm {
        request_id: id,
        accept: true,
    }));
    assert!(f
        .service
        .snapshot()
        .pending
        .iter()
        .any(|p| p.id == operation));
    f.service.detach(f.connection);
    f.control.lock().unwrap().hold = false;
    for _ in 0..30 {
        f.service.tick(Duration::from_millis(10));
    }
    assert_eq!(
        f.service.snapshot().audio_drivers.switch.status,
        AudioDriverSwitchStatus::Completed
    );
    assert!(f
        .service
        .snapshot()
        .results
        .iter()
        .any(|r| r.id == operation && matches!(r.outcome, Outcome::Confirmed { .. })));
    f.connection = ConnectionId(2);
    f.client = Client::new();
    f.attach();
    assert_eq!(
        f.client.snapshot().unwrap().tracks[0].id,
        before.tracks[0].id
    );
    assert_eq!(f.control.lock().unwrap().starts, 1);
}

#[shoop_test]
fn confirmation_refuses_recording_without_stopping_the_take() {
    let mut f = Fixture::new();
    let state = f.service.snapshot();
    let track = state.tracks[0].id;
    let loop_id = state.tracks[0].loops[0].id;
    assert!(matches!(
        f.run(Command::Loop {
            track,
            loop_id,
            action: LoopAction::RecordClicked
        }),
        Outcome::Confirmed { .. }
    ));
    let id = f.request();
    assert!(matches!(f.confirm(id), Outcome::Refused { .. }));
    assert_eq!(
        f.service.snapshot().tracks[0].loops[0].mode,
        LoopMode::Recording
    );
    assert_eq!(f.control.lock().unwrap().starts, 0);
}

#[cfg(all(feature = "builtin-fx", feature = "oxisynth"))]
impl Fixture {
    fn native() -> Self {
        let settings = HostSettings {
            driver: AudioDriverConfig::default(),
            pools: NewEnginePools {
                tracks: 6,
                loops: 32,
                channels: 4,
                loop_memory_mb: 16,
                ..Default::default()
            },
            processors: ProcessorSettings {
                builtin_fx: ProcessorPolicy::Auto,
                oxisynth: ProcessorPolicy::Auto,
                fx_instances: 1,
                synth_instances: 1,
                memory_mb: 128,
            },
            master_auto_connect: false,
            ..Default::default()
        };
        let (mut config, warnings) = crate::engine::native_config(
            &settings.driver,
            &settings.pools,
            &crate::engine::inventory(&settings.driver),
        );
        assert!(warnings.is_empty());
        config.engine.processors = settings.processors;
        if let shoop_rt_host_native::DriverConfig::Dummy(dummy) = &mut config.driver {
            dummy.input = Some(|_, _| 0.25);
        }
        let started = shoop_rt_host_native::start(&config).unwrap();
        let status = started.host.status().driver;
        let mut backend = RtBackend::connect(started.client, Sleep::default()).unwrap();
        backend.set_driver_profile(ResolvedAudioDriverConfig {
            configured: settings.driver.clone(),
            sample_rate: status.sample_rate,
            buffer_size: status.buffer_frames,
            instance_name: started.host.instance_name().into(),
        });
        let mut service = Service::new(
            Box::new(backend),
            ServiceOptions {
                master_auto_connect: false,
                loop_smoothing_ms: 3,
            },
        )
        .unwrap();
        let (owner, controller) = controller(started.host, settings, Box::new(|_| Ok(()))).unwrap();
        service.set_audio_driver_controller(controller).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while !service.is_ready() || service.snapshot().audio_drivers.catalog.len() != 3 {
            service.tick(Duration::from_millis(2));
            assert!(
                std::time::Instant::now() < deadline,
                "native service startup: {:?}",
                service.failure()
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let mut f = Self {
            service,
            client: Client::new(),
            control: Arc::new(Mutex::new(Control::default())),
            connection: ConnectionId(1),
            owner: Some(owner),
            #[cfg(all(feature = "builtin-fx", feature = "oxisynth"))]
            transfer: 0,
        };
        f.attach();
        f
    }
    fn transfer_key(&mut self) -> persistence::Key {
        self.transfer += 1;
        persistence::Key {
            transfer: TransferId(self.transfer),
            epoch: self.service.snapshot().persistence_epoch,
        }
    }
    fn capture(&mut self) -> Arc<persistence_image::Image> {
        let key = self.transfer_key();
        self.service
            .handle(
                self.connection,
                self.client
                    .capture_image(
                        key,
                        persistence::Target::Session {
                            revision: self.service.snapshot().revision,
                        },
                    )
                    .unwrap(),
            )
            .unwrap();
        for _ in 0..30_000 {
            self.service.tick(Duration::from_millis(2));
            while let Some(update) = self.service.take_update(self.connection) {
                if let ServiceMessage::CapturedImage { image, .. } = &update {
                    let image = image.clone();
                    self.service
                        .handle(
                            self.connection,
                            ClientMessage::Persistence {
                                authority: self.client.authority().unwrap(),
                                request: persistence::Request::Cancel { key },
                            },
                        )
                        .unwrap();
                    self.drain();
                    return image;
                }
                self.client.receive(update).unwrap();
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("capture did not complete");
    }
    fn import(&mut self, target: persistence::Target, image: Arc<persistence_image::Image>) {
        let key = self.transfer_key();
        self.service
            .handle(
                self.connection,
                self.client.stage_image(key, target, image).unwrap(),
            )
            .unwrap();
        let mut ready = false;
        for _ in 0..30_000 {
            self.service.tick(Duration::from_millis(2));
            while let Some(update) = self.service.take_update(self.connection) {
                if matches!(
                    &update,
                    ServiceMessage::Persistence {
                        reply: persistence::Reply::Ready { .. },
                        ..
                    }
                ) {
                    ready = true;
                }
                if let ServiceMessage::Persistence {
                    reply: persistence::Reply::Refused { reason, .. },
                    ..
                } = &update
                {
                    panic!("import refused: {reason:?}");
                }
                self.client.receive(update).unwrap();
            }
            if ready {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(ready);
        let outcome = self.run(Command::CommitPersistence(key));
        assert!(matches!(outcome, Outcome::Confirmed { .. }), "{outcome:?}");
    }
    fn switch_to(&mut self, sample_rate: u32) {
        assert!(matches!(
            self.run(Command::AudioDriver(DriverCommand::Request(
                AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                    sample_rate,
                    buffer_size: 128
                })
            ))),
            Outcome::Confirmed { .. }
        ));
        let id = self.service.snapshot().audio_drivers.switch.request_id;
        let outcome = self.confirm(id);
        assert!(
            matches!(outcome, Outcome::Confirmed { .. }),
            "{outcome:?}; {:?}",
            self.service.snapshot().audio_drivers
        );
        for _ in 0..30_000 {
            self.tick();
            if self.service.snapshot().audio_drivers.switch.status
                == AudioDriverSwitchStatus::Completed
            {
                return;
            }
        }
        panic!("switch persistence did not finish");
    }
}

#[shoop_test]
#[cfg(all(feature = "builtin-fx", feature = "oxisynth"))]
fn native_switch_converts_exact_media_and_preserves_processed_roles_and_session_state() {
    use persistence_image as image;
    let mut f = Fixture::native();
    for (name, topology) in [
        (
            "Direct",
            TrackSpecTopology::Direct {
                audio_channels: 2,
                midi: true,
            },
        ),
        (
            "FX",
            TrackSpecTopology::DryWet {
                dry_audio_channels: 2,
                wet_audio_channels: 2,
                dry_midi: true,
                processor_type: TrackProcessorTypeId::new(TrackProcessorTypeId::BUILTIN_FX),
                default_playback_mode: DefaultPlaybackMode::DryThroughWet,
            },
        ),
        (
            "Synth",
            TrackSpecTopology::DryWet {
                dry_audio_channels: 2,
                wet_audio_channels: 2,
                dry_midi: true,
                processor_type: TrackProcessorTypeId::new(TrackProcessorTypeId::OXISYNTH),
                default_playback_mode: DefaultPlaybackMode::DryThroughWet,
            },
        ),
    ] {
        let outcome = f.run(Command::AddTrack(TrackSpec {
            name: name.into(),
            topology,
            latency: Default::default(),
            initial_output_bus_names: vec!["Master".into()],
            creation_request_id: None,
        }));
        assert!(
            matches!(outcome, Outcome::TrackCreated { .. }),
            "{name}: {outcome:?}"
        );
    }
    let state = f.service.snapshot();
    let direct = state.tracks[1].id;
    let selected_id = state.tracks[2].loops[0].id;
    let loop_id = state.tracks[1].loops[0].id;
    f.import(
        persistence::Target::Loop {
            loop_id: state.tracks[0].loops[0].id,
            content_revision: state.tracks[0].loops[0].content_revision,
        },
        Arc::new(image::Image {
            manifest: image::Manifest::Update {
                sample_rate: 48_000,
                frames: Some(96),
                audio: vec![],
                midi: vec![],
            },
            payloads: vec![],
        }),
    );
    let channel = |object| image::Channel {
        mode: image::ChannelMode::Direct,
        object: Some(object),
        frames: 144,
        gain: 1.,
        start_offset: 7,
        capture_alignment: 0,
        preplay: 3,
        recording_started_at: Some("take identity".into()),
    };
    f.import(
        persistence::Target::Loop {
            loop_id,
            content_revision: state.tracks[1].loops[0].content_revision,
        },
        Arc::new(image::Image {
            manifest: image::Manifest::Update {
                sample_rate: 48_000,
                frames: Some(96),
                audio: (0..2)
                    .map(|i| image::AudioUpdate {
                        channel: i,
                        content: channel(i + 1),
                    })
                    .collect(),
                midi: vec![image::MidiUpdate {
                    channel: 0,
                    content: image::MidiChannel {
                        channel: channel(3),
                        initial: vec![vec![0xc0, 5]],
                    },
                }],
            },
            payloads: vec![
                image::Payload::Audio(Arc::new(vec![0.25; 144])),
                image::Payload::Audio(Arc::new(vec![-0.5; 144])),
                image::Payload::Midi(Arc::new(vec![
                    MidiEventState {
                        frame: 14,
                        data: vec![0x90, 60, 100].into(),
                    },
                    MidiEventState {
                        frame: 80,
                        data: vec![0x80, 60, 0].into(),
                    },
                ])),
            ],
        }),
    );
    for index in 2..4 {
        let state = f.service.snapshot();
        let target = &state.tracks[index].loops[0];
        f.import(
            persistence::Target::Loop {
                loop_id: target.id,
                content_revision: target.content_revision,
            },
            Arc::new(image::Image {
                manifest: image::Manifest::Update {
                    sample_rate: 48_000,
                    frames: Some(96),
                    audio: (0..4)
                        .map(|i| image::AudioUpdate {
                            channel: i,
                            content: image::Channel {
                                mode: if i < 2 {
                                    image::ChannelMode::Dry
                                } else {
                                    image::ChannelMode::Wet
                                },
                                gain: 1.,
                                ..channel(i + 1)
                            },
                        })
                        .collect(),
                    midi: vec![image::MidiUpdate {
                        channel: 0,
                        content: image::MidiChannel {
                            channel: image::Channel {
                                mode: image::ChannelMode::Dry,
                                ..channel(5)
                            },
                            initial: vec![vec![0xc0, 5]],
                        },
                    }],
                },
                payloads: (0..4)
                    .map(|i| image::Payload::Audio(Arc::new(vec![0.1 * (i + 1) as f32; 144])))
                    .chain([image::Payload::Midi(Arc::new(vec![MidiEventState {
                        frame: 14,
                        data: vec![0x90, 60, 100].into(),
                    }]))])
                    .collect(),
            }),
        );
    }
    for (target, add) in [
        (state.tracks[1].loops[1].id, loop_id),
        (state.tracks[1].loops[2].id, state.tracks[1].loops[1].id),
    ] {
        let outcome = f.run(Command::Composite(composite::Edit::Append {
            target,
            add: if target == state.tracks[1].loops[2].id {
                vec![add; 10]
            } else {
                vec![add]
            },
            parallel: false,
            convert_if_empty: true,
        }));
        assert!(matches!(outcome, Outcome::Confirmed { .. }), "{outcome:?}");
    }
    assert!(matches!(
        f.run(Command::Track {
            track: direct,
            action: TrackAction::OutputGainChanged(-7.5)
        }),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        f.run(Command::SelectLoops {
            loops: vec![selected_id]
        }),
        Outcome::Confirmed { .. }
    ));
    assert!(matches!(
        f.run(Command::SetTarget(Some(loop_id))),
        Outcome::Confirmed { .. }
    ));
    let original = f.capture();
    let epoch = f.service.snapshot().persistence_epoch;
    assert!(matches!(
        f.run(Command::AudioDriver(DriverCommand::Request(
            AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                sample_rate: 100_000,
                buffer_size: 128
            })
        ))),
        Outcome::Confirmed { .. }
    ));
    let request = f.service.snapshot().audio_drivers.switch.request_id;
    let outcome = f.confirm(request);
    assert!(matches!(outcome, Outcome::Refused { .. }), "{outcome:?}");
    assert!(f
        .service
        .snapshot()
        .audio_drivers
        .switch
        .message
        .contains("cannot preserve the session's hosted processors"));
    assert_eq!(
        f.service.snapshot().persistence_epoch,
        epoch,
        "unsupported target processors must be detected before teardown"
    );
    assert_eq!(f.service.snapshot().status.sample_rate, 48_000);
    for limits in [
        HostResourceLimits {
            media_bytes: 1,
            ..Default::default()
        },
        HostResourceLimits {
            objects: 1,
            ..Default::default()
        },
        HostResourceLimits {
            metadata_bytes: 1,
            ..Default::default()
        },
    ] {
        assert!(conversion::convert(
            original.clone(),
            ConversionTarget {
                sample_rate: 32_000,
                quantum_frames: 32
            },
            limits
        )
        .is_err());
    }
    f.switch_to(48_000);
    assert_eq!(
        *original,
        *f.capture(),
        "same-format switching must preserve exact media and timing"
    );
    f.switch_to(32_000);
    let converted = f.capture();
    let image::Manifest::Session(before) = &original.manifest else {
        unreachable!()
    };
    let image::Manifest::Session(after) = &converted.manifest else {
        unreachable!()
    };
    assert_eq!(after.sample_rate, 32_000);
    assert_eq!(after.tracks[0].loops[0].frames, 64);
    assert_eq!(
        before.tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
        after.tracks.iter().map(|t| t.id).collect::<Vec<_>>()
    );
    assert_eq!(after.selected, vec![selected_id]);
    assert_eq!(after.target, Some(loop_id));
    assert_eq!(after.tracks[1].controls.output_gain_db, -7.5);
    for index in 2..4 {
        assert_eq!(before.tracks[index].topology, after.tracks[index].topology);
        assert_eq!(
            before.tracks[index].processor_state,
            after.tracks[index].processor_state
        );
        assert_eq!(before.tracks[index].ports, after.tracks[index].ports);
    }
    for index in 2..4 {
        let old = &before.tracks[index].loops[0];
        let new = &after.tracks[index].loops[0];
        assert_eq!(new.recorded_processor_state, old.recorded_processor_state);
        assert_eq!(
            new.audio
                .iter()
                .map(|c| (c.mode, c.gain))
                .collect::<Vec<_>>(),
            old.audio
                .iter()
                .map(|c| (c.mode, c.gain))
                .collect::<Vec<_>>()
        );
        for (i, channel) in new.audio.iter().enumerate() {
            assert_eq!(channel.frames, 97);
            let image::Payload::Audio(samples) =
                &converted.payloads[channel.object.unwrap() as usize - 1]
            else {
                unreachable!()
            };
            assert!((samples[30] - 0.1 * (i + 1) as f32).abs() < 1e-6);
        }
    }
    for i in 1..=2 {
        let old = before.tracks[1].loops[i].composite.as_ref().unwrap();
        let new = after.tracks[1].loops[i].composite.as_ref().unwrap();
        assert_eq!(old.document, new.document);
        assert_eq!(new.prepared.sample_rate, 32_000);
        assert_eq!(new.prepared.unit_frames, 32);
        assert_eq!(
            after.tracks[1].loops[i].frames,
            if i == 2 { 640 } else { 64 }
        );
    }
    let content = &after.tracks[1].loops[0];
    assert_eq!(content.id, loop_id);
    assert_eq!(content.frames, 64);
    assert_eq!(content.audio[0].start_offset, 5);
    assert_eq!(content.audio[0].preplay, 2);
    assert_eq!(content.audio[0].frames, 97);
    assert_eq!(
        content.audio[0].recording_started_at.as_deref(),
        Some("take identity")
    );
    let image::Payload::Audio(samples) =
        &converted.payloads[content.audio[0].object.unwrap() as usize - 1]
    else {
        unreachable!()
    };
    assert!((samples[30] - 0.25).abs() < 1e-6);
    let image::Payload::Midi(events) =
        &converted.payloads[content.midi[0].channel.object.unwrap() as usize - 1]
    else {
        unreachable!()
    };
    assert_eq!(
        events
            .iter()
            .map(|e| (e.frame, e.data.to_vec()))
            .collect::<Vec<_>>(),
        vec![(9, vec![0x90, 60, 100]), (53, vec![0x80, 60, 0])]
    );
    assert_eq!(content.midi[0].initial, vec![vec![0xc0, 5]]);
    f.switch_to(44_100);
    f.switch_to(48_000);
    assert_eq!(f.service.snapshot().tracks[1].id, direct);
    for bus in f.service.snapshot().buses.iter() {
        assert!(matches!(
            f.run(Command::Bus {
                bus: bus.id,
                action: BusAction::Remove
            }),
            Outcome::Confirmed { .. }
        ));
    }
    f.switch_to(32_000);
    assert!(f.service.snapshot().buses.is_empty());
    for (name, channel_count) in [("Cue", 1), ("Main", 4)] {
        assert!(matches!(
            f.run(Command::AddBus(BusSpec {
                name: name.into(),
                channel_count,
                creation_request_id: None,
                fx: None
            })),
            Outcome::Confirmed { .. }
        ));
    }
    let buses = f.service.snapshot().buses.clone();
    f.switch_to(44_100);
    let after = f.service.snapshot();
    assert_eq!(
        after
            .buses
            .iter()
            .map(|b| (b.id, &b.name))
            .collect::<Vec<_>>(),
        buses.iter().map(|b| (b.id, &b.name)).collect::<Vec<_>>()
    );
    use shoop_persistence_codec::codec::{Value, Work};
    let saved = f.capture();
    let Value::File { bytes, .. } = Work::Encode {
        image: saved.clone(),
        kind: IoTaskKind::SaveSession,
        wav: false,
        standard: false,
        channels: vec![],
    }
    .run()
    .unwrap() else {
        unreachable!()
    };
    let Value::Decoded(decoded) = Work::Decode {
        kind: IoTaskKind::LoadSession,
        bytes,
        rate: 44_100,
    }
    .run()
    .unwrap() else {
        unreachable!()
    };
    let Value::Image(image) = Work::Prepare {
        decoded,
        mapping: vec![],
        update_length: true,
    }
    .run()
    .unwrap() else {
        unreachable!()
    };
    f.import(
        persistence::Target::Session {
            revision: f.service.snapshot().revision,
        },
        image,
    );
    let reloaded = f.capture();
    assert_eq!(saved.payloads, reloaded.payloads);
    let image::Manifest::Session(saved) = &saved.manifest else {
        unreachable!()
    };
    let image::Manifest::Session(reloaded) = &reloaded.manifest else {
        unreachable!()
    };
    assert_eq!(saved.sample_rate, reloaded.sample_rate);
    for (old, new) in saved.tracks.iter().zip(&reloaded.tracks) {
        assert_eq!(old.name, new.name);
        assert_eq!(old.topology, new.topology);
        assert_eq!(old.processor_state, new.processor_state);
        assert_eq!(
            old.loops.iter().map(|l| l.frames).collect::<Vec<_>>(),
            new.loops.iter().map(|l| l.frames).collect::<Vec<_>>()
        );
    }
}

#[shoop_test]
fn lifecycle_controller_bounds_outstanding_work_and_reports_worker_loss() {
    let (sender, _requests) = mpsc::sync_channel(2);
    let (results, receiver) = mpsc::sync_channel(1);
    let mut controller = Controller {
        sender,
        receiver,
        ticket: None,
    };
    controller
        .submit(1, DriverWork::Discover(AudioDriverConfig::default()))
        .unwrap();
    assert!(controller
        .submit(2, DriverWork::Discover(AudioDriverConfig::default()))
        .is_err());
    assert!(controller.poll().is_none());
    drop(results);
    let completion = controller.poll().unwrap();
    assert_eq!(completion.ticket, 1);
    assert!(completion.result.err().unwrap().contains("disconnected"));
    assert!(controller.poll().is_none());
}

#[shoop_test]
fn native_owner_keeps_cleanup_pending_without_blocking_or_resubmitting() {
    let (sender, requests) = mpsc::sync_channel(2);
    let (started, receive_started) = mpsc::channel();
    let (release, receive_release) = mpsc::channel();
    let join = std::thread::spawn(move || {
        let Request::Stop(reply) = requests.recv().unwrap() else {
            panic!("expected stop")
        };
        started.send(()).unwrap();
        receive_release.recv().unwrap();
        assert!(matches!(
            requests.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        reply.send(Ok(())).unwrap();
    });
    let mut owner = Owner {
        sender,
        join: Some(join),
        stopping: None,
        stop_result: None,
    };
    assert_eq!(owner.poll_stop(), None);
    receive_started
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    for _ in 0..100 {
        assert_eq!(owner.poll_stop(), None);
    }
    release.send(()).unwrap();
    assert_eq!(owner.stop(), Ok(()));
    assert_eq!(owner.poll_stop(), Some(Ok(())));
}

#[shoop_test]
fn native_owner_retains_terminal_cleanup_failure_across_shutdown_retries() {
    for result in [Ok(()), Err("injected MIDI cleanup failure".to_owned())] {
        let expected = result.clone();
        let (sender, requests) = mpsc::sync_channel(2);
        let join = std::thread::spawn(move || {
            let Request::Stop(reply) = requests.recv().unwrap() else {
                panic!("expected stop")
            };
            reply.send(result).unwrap();
        });
        let mut owner = Owner {
            sender,
            join: Some(join),
            stopping: None,
            stop_result: None,
        };
        assert_eq!(owner.stop(), expected);
        assert_eq!(owner.stop(), expected);
    }
}
