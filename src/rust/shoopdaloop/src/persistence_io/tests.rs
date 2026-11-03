use super::*;
use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_rt_backend::{EngineClock, RtBackend};
use shoop_rt_engine::{testing::OfflineRender, EngineDescription, HostPorts, Segment};
use shoop_wasm_test_support::shoop_test;
use std::{cell::Cell, rc::Rc, time::Duration};

struct GateDelivery {
    port: local::ClientPort,
    hold: Rc<Cell<bool>>,
}
impl Delivery for GateDelivery {
    fn send(&mut self, message: ClientMessage) -> Result<(), Error> {
        if self.hold.get()
            && matches!(
                &message,
                ClientMessage::Submit {
                    command: Command::Global(GlobalControlAction::SetSolo(true)),
                    ..
                }
            )
        {
            return Err(Error::Full);
        }
        self.port.send(message)
    }
    fn poll(&mut self, client: &mut Client) -> Result<usize, Error> {
        self.port.poll(client)
    }
}

const STEP: Duration = Duration::from_millis(16);
struct Clock(OfflineRender);
impl EngineClock for Clock {
    fn advance(&mut self, elapsed: Duration) {
        self.0.run_for(elapsed, |_, _| 0.25).unwrap();
    }
    fn wait(&mut self) -> bool {
        self.0.run(16, |_, _| 0.25).unwrap();
        true
    }
}
struct Fixture {
    service: Service,
    runtime: Runtime,
    port: local::ServicePort,
    hold: Rc<Cell<bool>>,
}

#[shoop_test]
fn committed_file_tasks_wait_for_the_confirmed_projection_and_keep_known_loss_outcomes() {
    let fixture = Fixture::new();
    let state = fixture.runtime.client().snapshot().unwrap().clone();
    let authority = fixture.runtime.client().authority().unwrap();
    for lose_connection in [false, true] {
        let mut client = Client::new();
        client.attach().unwrap();
        client
            .receive(ServiceMessage::Attached { authority })
            .unwrap();
        client
            .receive(ServiceMessage::Snapshot {
                authority,
                state: Arc::clone(&state),
            })
            .unwrap();
        client.ready_sent().unwrap();
        let mut provider = Provider::new();
        provider
            .start(&state, IoTaskKind::LoadSession, None)
            .unwrap();
        let message = client
            .command(
                Command::CommitPersistence(provider.task.as_ref().unwrap().key),
                None,
            )
            .unwrap();
        let ClientMessage::Submit { operation, .. } = message else {
            unreachable!()
        };
        provider.phase = Phase::Committed(operation);
        let revision = Revision(state.revision.0 + 1);
        client
            .receive(ServiceMessage::Finished {
                authority,
                result: OperationResult {
                    id: operation,
                    outcome: Outcome::Confirmed { revision },
                },
            })
            .unwrap();
        let (mut delivery, _service) = local::channel();
        provider.pump(&mut client, &mut delivery).unwrap();
        assert_eq!(provider.task().unwrap().status, IoTaskStatus::Running);
        if lose_connection {
            client.disconnect();
        } else {
            let mut published = (*state).clone();
            published.revision = revision;
            published.persistence_epoch += 1;
            client
                .receive(ServiceMessage::Snapshot {
                    authority,
                    state: Arc::new(published),
                })
                .unwrap();
        }
        provider.pump(&mut client, &mut delivery).unwrap();
        let task = provider.task().unwrap();
        assert_eq!(task.status, IoTaskStatus::Completed);
        if lose_connection {
            assert!(task.message.contains("reconnect"));
            assert!(provider.pending_cancel.is_none());
        } else {
            assert_eq!(client.snapshot().unwrap().revision, revision);
            assert_eq!(
                client.snapshot().unwrap().persistence_epoch,
                state.persistence_epoch + 1
            );
        }
    }
}

#[shoop_test]
async fn client_queue_holds_one_raw_file_input_without_blocking_controls() {
    let mut fixture = Fixture::new();
    fixture.until(|_| true).await;
    let input = || AppIntent::LoadSessionBytes {
        name: "invalid.shoop".into(),
        bytes: vec![0].into(),
    };
    fixture.runtime.dispatch(input()).unwrap();
    assert_eq!(
        fixture.runtime.dispatch(input()),
        Err(shoop_app_client::Error::Full)
    );
    fixture
        .runtime
        .dispatch(AppIntent::Global(GlobalControlAction::SetSolo(true)))
        .unwrap();
    for _ in 0..10_000 {
        fixture.step();
        idle().await;
        let state = fixture.runtime.snapshot();
        if state.global_controls.solo
            && state
                .io_task
                .as_ref()
                .is_some_and(|task| task.status == IoTaskStatus::Failed)
        {
            break;
        }
    }
    assert!(fixture.runtime.snapshot().global_controls.solo);
    assert_eq!(
        fixture.runtime.snapshot().io_task.as_ref().unwrap().status,
        IoTaskStatus::Failed
    );
    fixture.runtime.dispatch(input()).unwrap();
}

#[shoop_test]
fn new_session_completion_waits_for_its_projection_and_preserves_confirmed_loss() {
    let fixture = Fixture::new();
    let state = fixture.runtime.client().snapshot().unwrap().clone();
    let authority = fixture.runtime.client().authority().unwrap();
    for lose_connection in [false, true] {
        let client = Client::new();
        let (delivery, service) = local::channel();
        let mut runtime = Runtime::new(
            client,
            Box::new(delivery),
            Box::new(shoop_scripting::NullMidiService),
            Startup::Fresh(Vec::new()),
        )
        .unwrap();
        service
            .updates
            .send(ServiceMessage::Attached { authority })
            .unwrap();
        service
            .updates
            .send(ServiceMessage::Snapshot {
                authority,
                state: Arc::clone(&state),
            })
            .unwrap();
        runtime.tick(STEP);
        runtime.dispatch(AppIntent::RequestNewSession).unwrap();
        runtime.tick(STEP);
        let operation = service
            .commands
            .try_iter()
            .find_map(|message| match message {
                ClientMessage::Submit {
                    command: Command::NewSession,
                    operation,
                    ..
                } => Some(operation),
                _ => None,
            })
            .unwrap();
        let revision = Revision(state.revision.0 + 1);
        service
            .updates
            .send(ServiceMessage::Finished {
                authority,
                result: OperationResult {
                    id: operation,
                    outcome: Outcome::Confirmed { revision },
                },
            })
            .unwrap();
        runtime.tick(STEP);
        assert_eq!(
            runtime.snapshot().io_task.as_ref().unwrap().status,
            IoTaskStatus::Running
        );
        if lose_connection {
            drop(service);
        } else {
            let mut published = (*state).clone();
            published.revision = revision;
            published.persistence_epoch += 1;
            service
                .updates
                .send(ServiceMessage::Snapshot {
                    authority,
                    state: Arc::new(published),
                })
                .unwrap();
        }
        runtime.tick(STEP);
        let task = runtime.snapshot().io_task.as_ref().unwrap().clone();
        assert_eq!(task.status, IoTaskStatus::Completed);
        if lose_connection {
            assert!(task.message.contains("reconnect"));
        } else {
            assert_eq!(runtime.client().snapshot().unwrap().revision, revision);
        }
    }
}

#[shoop_test]
async fn public_core_file_capabilities_keep_duplication_refused() {
    let mut fixture = Fixture::new();
    let state = fixture.runtime.snapshot();
    for capability in [
        BackendCapability::SessionCapture,
        BackendCapability::SessionContentLoad,
        BackendCapability::LoopContentImport,
        BackendCapability::ClickTrackInsertion,
    ] {
        assert!(state.capabilities.supports(capability));
    }
    assert!(!state
        .capabilities
        .supports(BackendCapability::LoopDuplication));
    let track = &state.tracks[0];
    fixture.dispatch(AppIntent::Loop {
        track_id: track.id,
        loop_id: track.loops[0].id,
        action: LoopAction::Duplicate,
    });
    for _ in 0..8 {
        fixture.step();
        idle().await;
    }
    let state = fixture.runtime.snapshot();
    assert!(
        state.notifications.iter().any(|notice| notice
            .message
            .contains(BackendCapability::LoopDuplication.unavailable_reason())),
        "{:?}",
        state.notifications
    );
    assert!(fixture.runtime.take_file_request().is_none());
    assert!(fixture.runtime.take_file_output().is_none());
}

#[shoop_test]
async fn generated_clicks_commit_and_export_through_the_public_route() {
    let mut f = Fixture::new();
    f.dispatch(AppIntent::AddTrack(DirectTrackSpec {
        name: "Clicks".into(),
        audio_channels: 2,
        midi: true,
    }));
    let state = f
        .until(|s| s.tracks.len() == 2 && s.tracks[1].loops.len() == 8)
        .await;
    assert!(!state.click_track.sounds.is_empty());
    assert!(state.click_track.max_click_count >= 4);
    let loop_id = state.tracks[1].loops[0].id;
    for kind in [ClickTrackKind::Audio, ClickTrackKind::Midi] {
        f.dispatch(AppIntent::GenerateClickTrack {
            loop_id,
            request: ClickTrackRequest {
                kind,
                bpm: 120.0,
                ..Default::default()
            },
        });
        let state = f
            .until(|s| {
                s.io_task
                    .as_ref()
                    .is_some_and(|t| t.status == IoTaskStatus::Completed)
            })
            .await;
        assert_eq!(
            state.io_task.as_ref().unwrap().kind,
            IoTaskKind::GenerateClickTrack
        );
        assert_eq!(state.tracks[1].loops[0].length_frames, 2000);
        match kind {
            ClickTrackKind::Audio => f.dispatch(AppIntent::RequestLoopAudioExport {
                loop_id,
                format: LoopAudioExportFormat::Exact,
            }),
            ClickTrackKind::Midi => f.dispatch(AppIntent::RequestLoopMidiExport {
                loop_id,
                standard: false,
            }),
        }
        if kind == ClickTrackKind::Audio {
            let state = f
                .until(|s| {
                    s.io_task
                        .as_ref()
                        .is_some_and(|t| t.status == IoTaskStatus::AwaitingChannelSelection)
                })
                .await;
            f.dispatch(AppIntent::ConfirmAudioChannelSelection {
                task_id: state.io_task.as_ref().unwrap().id,
                channels: vec![0, 1],
            });
        }
        let output = f.output().await;
        match kind {
            ClickTrackKind::Audio => {
                let audio = shoop_session::decode_loop_audio(&output.bytes).unwrap();
                assert_eq!(audio.channels.len(), 2);
                assert_eq!(audio.channels[0].samples, audio.channels[1].samples);
                assert!(audio.channels[0].samples.iter().any(|s| *s != 0.0));
            }
            ClickTrackKind::Midi => {
                let midi = shoop_session::decode_exact_midi(&output.bytes).unwrap();
                assert_eq!(midi.events.len(), 8);
                assert_eq!(midi.events[0].frame, 0);
                assert_eq!(midi.events[2].frame, 500);
            }
        }
        f.dispatch(AppIntent::CompleteIoOutput {
            task_id: output.task_id,
            outcome: FileOutputOutcome::Saved,
        });
        f.step();
    }
}

#[shoop_test]
async fn archive_resampling_channel_reorder_and_standard_midi_import_use_the_public_route() {
    use shoop_session::*;
    let mut f = Fixture::new();
    f.dispatch(AppIntent::AddTrack(DirectTrackSpec {
        name: "Mapped".into(),
        audio_channels: 2,
        midi: true,
    }));
    let state = f
        .until(|s| s.tracks.len() == 2 && s.tracks[1].loops.len() == 8)
        .await;
    let loop_id = state.tracks[1].loops[0].id;
    let source = LoopAudio {
        sample_rate: 2000,
        channels: vec![
            LoopAudioChannel {
                label: "Left".into(),
                role: "direct".into(),
                samples: vec![0.25; 128],
            },
            LoopAudioChannel {
                label: "Right".into(),
                role: "direct".into(),
                samples: vec![-0.5; 128],
            },
        ],
    };
    let converted = resample_loop_audio(&source, 1000).unwrap();
    f.dispatch(AppIntent::ImportLoopAudioBytes {
        loop_id,
        name: "source.shoop-audio".into(),
        bytes: encode_loop_audio(&source).unwrap().into(),
        update_loop_length: true,
    });
    let state = f
        .until(|s| {
            s.io_task
                .as_ref()
                .is_some_and(|t| t.status == IoTaskStatus::AwaitingSampleRateConfirmation)
        })
        .await;
    assert_eq!(state.tracks[1].loops[0].length_frames, 0);
    f.dispatch(AppIntent::ConfirmSampleRateConversion {
        task_id: state.io_task.as_ref().unwrap().id,
        accept: true,
    });
    let state = f
        .until(|s| {
            s.io_task
                .as_ref()
                .is_some_and(|t| t.status == IoTaskStatus::AwaitingChannelMapping)
        })
        .await;
    f.dispatch(AppIntent::ConfirmAudioChannelMapping {
        task_id: state.io_task.as_ref().unwrap().id,
        source_for_destination: vec![1, 0],
    });
    f.until(|s| {
        s.io_task
            .as_ref()
            .is_some_and(|t| t.status == IoTaskStatus::Completed)
    })
    .await;
    let midi = ExactMidi {
        sample_rate: 2000,
        length_frames: 128,
        start_state: Vec::new(),
        events: vec![
            ExactMidiEvent {
                frame: 32,
                order: 0,
                data: vec![0x90, 60, 100],
            },
            ExactMidiEvent {
                frame: 96,
                order: 1,
                data: vec![0xf0, 0x7d, 1, 2, 3, 4, 0xf7],
            },
        ],
    };
    let smf = encode_standard_midi(&midi).unwrap();
    let expected_midi = decode_standard_midi(&smf.bytes, 1000).unwrap();
    f.dispatch(AppIntent::ImportLoopMidiBytes {
        loop_id,
        name: "source.mid".into(),
        bytes: smf.bytes.into(),
        update_loop_length: false,
    });
    f.until(|s| {
        s.io_task.as_ref().is_some_and(|t| {
            t.kind == IoTaskKind::ImportLoopMidi && t.status == IoTaskStatus::Completed
        })
    })
    .await;
    assert_eq!(
        f.runtime.snapshot().tracks[1].loops[0].length_frames,
        converted.channels[0].samples.len() as u64
    );
    f.dispatch(AppIntent::RequestSaveSession);
    let file = f.output().await;
    let saved = decode_session(&file.bytes).unwrap();
    assert_eq!(saved.document.sample_rate, 1000);
    let audio = saved
        .media
        .values()
        .filter_map(|p| match p {
            MediaPayload::Audio(a) => Some(&a.samples),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        audio,
        [
            &converted.channels[1].samples,
            &converted.channels[0].samples
        ]
    );
    assert!(saved
        .media
        .values()
        .any(|p| matches!(p, MediaPayload::Midi(m) if m == &expected_midi)));
    assert_eq!(f.runtime.snapshot().tracks[1].loops[0].id, loop_id);
}

#[shoop_test]
async fn callbacks_from_a_replaced_client_runtime_cannot_load_into_the_new_runtime() {
    let mut old = Fixture::new();
    old.dispatch(AppIntent::RequestLoadSessionPicker);
    let old_id = old.runtime.take_file_request().unwrap().task_id;
    drop(old);
    let mut current = Fixture::new();
    current.dispatch(AppIntent::RequestLoadSessionPicker);
    let current_id = current.runtime.take_file_request().unwrap().task_id;
    assert_ne!(old_id, current_id);
    let epoch = current
        .runtime
        .client()
        .snapshot()
        .unwrap()
        .persistence_epoch;
    let bytes: Arc<Vec<u8>> = shoop_session::encode_session(
        &shoop_session::SessionBundle::new(shoop_session::SessionDocument::empty(1000)),
        "test",
    )
    .unwrap()
    .into();
    current.dispatch(AppIntent::CompleteIoInput {
        task_id: old_id,
        name: "late.shoop".into(),
        bytes: Arc::clone(&bytes),
    });
    for _ in 0..8 {
        current.step();
        idle().await;
    }
    assert_eq!(
        current
            .runtime
            .client()
            .snapshot()
            .unwrap()
            .persistence_epoch,
        epoch
    );
    assert_eq!(
        current.runtime.snapshot().io_task.as_ref().unwrap().status,
        IoTaskStatus::Running
    );
    current.dispatch(AppIntent::CompleteIoInput {
        task_id: current_id,
        name: "current.shoop".into(),
        bytes,
    });
    current
        .until(|s| {
            s.io_task
                .as_ref()
                .is_some_and(|t| t.status == IoTaskStatus::Completed)
        })
        .await;
    assert_eq!(
        current
            .runtime
            .client()
            .snapshot()
            .unwrap()
            .persistence_epoch,
        epoch + 1
    );
}
#[shoop_test]
async fn platform_output_reports_its_real_completion_after_the_backend_connection_is_lost() {
    let mut f = Fixture::new();
    f.dispatch(AppIntent::RequestSaveSession);
    let output = f.output().await;
    f.service.detach(ConnectionId(1));
    f.runtime.fail("backend connection lost".into());
    f.runtime.tick(STEP);
    assert_eq!(
        f.runtime.snapshot().io_task.as_ref().unwrap().status,
        IoTaskStatus::Running
    );
    f.runtime
        .dispatch(AppIntent::CompleteIoOutput {
            task_id: output.task_id,
            outcome: FileOutputOutcome::Saved,
        })
        .unwrap();
    let state = f.runtime.snapshot();
    assert_eq!(
        state.io_task.as_ref().unwrap().status,
        IoTaskStatus::Completed
    );
    assert!(state
        .io_task
        .as_ref()
        .unwrap()
        .message
        .to_ascii_lowercase()
        .contains("saved"));
    assert_eq!(f.runtime.client().state(), shoop_app_client::State::Lost);
}

impl Fixture {
    fn new() -> Self {
        let (render, client) = OfflineRender::new(&EngineDescription {
            sample_rate: 1000,
            max_frames: 16,
            host: HostPorts::numbered(2, 2, 1),
            segments: vec![Segment {
                strips: 8,
                loops: 80,
            }],
            strip_channels: 2,
            resource_pages: 4096,
            page_bytes: 256,
            midi_events: 64,
            midi_bytes: 512,
            telemetry_interval_frames: 0,
            ..Default::default()
        })
        .unwrap();
        let backend = RtBackend::connect(client, Clock(render)).unwrap();
        let mut service = Service::new(
            Box::new(backend),
            ServiceOptions {
                master_auto_connect: false,
                loop_smoothing_ms: 0,
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
        let (delivery, port) = local::channel();
        let hold = Rc::new(Cell::new(false));
        let client = Client::new();
        let runtime = Runtime::new(
            client,
            Box::new(GateDelivery {
                port: delivery,
                hold: Rc::clone(&hold),
            }),
            Box::new(shoop_scripting::NullMidiService),
            Startup::Fresh(Vec::new()),
        )
        .unwrap()
        .with_persistence_io(Box::new(Provider::new()));
        let mut f = Self {
            service,
            runtime,
            port,
            hold,
        };
        for _ in 0..4 {
            f.step();
        }
        assert!(!f.runtime.snapshot().tracks.is_empty());
        f
    }
    fn step(&mut self) {
        self.service.pump_local(ConnectionId(1), &self.port, STEP);
        self.runtime.tick(STEP);
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::yield_now();
    }
    async fn until(&mut self, predicate: impl Fn(&AppSnapshot) -> bool) -> Arc<AppSnapshot> {
        for _ in 0..10_000 {
            self.step();
            idle().await;
            let s = self.runtime.snapshot();
            if predicate(&s) {
                return s;
            }
            if s.io_task
                .as_ref()
                .is_some_and(|t| t.status == IoTaskStatus::Failed)
            {
                panic!("file task failed: {:?}", s.io_task);
            }
        }
        panic!(
            "file workflow timed out: {:?}",
            self.runtime.snapshot().io_task
        )
    }
    fn dispatch(&mut self, intent: AppIntent) {
        self.runtime.dispatch(intent).unwrap();
        self.step();
    }
    async fn output(&mut self) -> ApplicationFileOutput {
        for _ in 0..10_000 {
            self.step();
            idle().await;
            if let Some(output) = self.runtime.take_file_output() {
                return output;
            }
            assert!(
                self.runtime
                    .snapshot()
                    .io_task
                    .as_ref()
                    .is_none_or(|t| t.status != IoTaskStatus::Failed),
                "{:?}",
                self.runtime.snapshot().io_task
            );
        }
        panic!("file output timed out")
    }
}
#[shoop_test]
async fn production_files_roundtrip_audio_midi_and_fresh_session_identities() {
    let mut f = Fixture::new();
    f.dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)));
    f.dispatch(AppIntent::AddTrack(DirectTrackSpec {
        name: "Files".into(),
        audio_channels: 1,
        midi: true,
    }));
    let state = f
        .until(|s| s.tracks.len() == 2 && s.tracks[1].loops.len() == 8)
        .await;
    let loop_id = state.tracks[1].loops[0].id;
    let audio = shoop_session::LoopAudio {
        sample_rate: 1000,
        channels: vec![shoop_session::LoopAudioChannel {
            label: "Mono".into(),
            role: "direct".into(),
            samples: vec![0., 0.25, -0.5, 0.75],
        }],
    };
    let wav = shoop_session::encode_float_wav(&audio).unwrap();
    f.dispatch(AppIntent::ImportLoopAudioBytes {
        loop_id,
        name: "take.wav".into(),
        bytes: wav.into(),
        update_loop_length: true,
    });
    let state = f
        .until(|s| {
            s.io_task
                .as_ref()
                .is_some_and(|t| t.status == IoTaskStatus::AwaitingChannelMapping)
        })
        .await;
    f.dispatch(AppIntent::ConfirmAudioChannelMapping {
        task_id: state.io_task.as_ref().unwrap().id,
        source_for_destination: vec![0],
    });
    f.until(|s| {
        s.io_task
            .as_ref()
            .is_some_and(|t| t.status == IoTaskStatus::Completed)
    })
    .await;
    let midi = shoop_session::ExactMidi {
        sample_rate: 1000,
        length_frames: 4,
        start_state: vec![vec![0xb0, 7, 99]],
        events: vec![
            shoop_session::ExactMidiEvent {
                frame: 1,
                order: 0,
                data: vec![0x90, 60, 100],
            },
            shoop_session::ExactMidiEvent {
                frame: 3,
                order: 1,
                data: vec![0x80, 60, 0],
            },
        ],
    };
    f.dispatch(AppIntent::ImportLoopMidiBytes {
        loop_id,
        name: "take.shoop-midi".into(),
        bytes: shoop_session::encode_exact_midi(&midi).unwrap().into(),
        update_loop_length: false,
    });
    f.until(|s| {
        s.io_task.as_ref().is_some_and(|t| {
            t.kind == IoTaskKind::ImportLoopMidi && t.status == IoTaskStatus::Completed
        })
    })
    .await;
    f.dispatch(AppIntent::RequestSaveSession);
    let file = f.output().await;
    let before = shoop_session::decode_session(&file.bytes).unwrap();
    let mut shared = before.clone();
    let track = shared
        .document
        .track_groups
        .iter_mut()
        .flat_map(|group| &mut group.tracks)
        .find(|track| track.name == "Files")
        .unwrap();
    let mut repeated = track.loops[0].clone();
    repeated.id = track.loops[1].id;
    for (channel, empty) in repeated.channels.iter_mut().zip(&track.loops[1].channels) {
        channel.id = empty.id;
    }
    track.loops[1] = repeated;
    let shoop_persistence_codec::codec::Value::Image(shared) =
        (shoop_persistence_codec::codec::Work::Prepare {
            decoded: shoop_persistence_codec::codec::Decoded::Session(shared),
            mapping: Vec::new(),
            update_length: false,
        })
        .run()
        .unwrap()
    else {
        panic!()
    };
    let audio_payloads = shared
        .payloads
        .iter()
        .filter_map(|p| match p {
            shoop_app_protocol::persistence_image::Payload::Audio(samples) => Some(samples),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(audio_payloads.len(), 2);
    assert!(std::sync::Arc::ptr_eq(audio_payloads[0], audio_payloads[1]));
    assert!(before.media.values().any(|p| matches!(p, shoop_session::MediaPayload::Audio(a) if a.samples == audio.channels[0].samples)));
    assert!(before
        .media
        .values()
        .any(|p| matches!(p, shoop_session::MediaPayload::Midi(m) if m == &midi)));
    assert_eq!(
        f.runtime.snapshot().io_task.as_ref().unwrap().status,
        IoTaskStatus::Running
    );
    f.dispatch(AppIntent::CompleteIoOutput {
        task_id: file.task_id,
        outcome: FileOutputOutcome::Saved,
    });
    f.until(|s| {
        s.io_task
            .as_ref()
            .is_some_and(|t| t.status == IoTaskStatus::Completed)
    })
    .await;
    f.dispatch(AppIntent::Loop {
        track_id: f.runtime.snapshot().tracks[1].id,
        loop_id,
        action: LoopAction::PlayClicked,
    });
    f.until(|s| s.tracks[1].loops[0].mode == LoopMode::Playing)
        .await;
    let epoch = f.runtime.client().snapshot().unwrap().persistence_epoch;
    let mut oversized = before.clone();
    oversized
        .document
        .track_groups
        .iter_mut()
        .flat_map(|group| &mut group.tracks)
        .find(|track| track.name == "Files")
        .unwrap()
        .loops[0]
        .length_frames = shoop_app_protocol::persistence::MAX_SESSION_MEDIA_BYTES / 8 + 1;
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "one-frame-over-budget.shoop".into(),
        bytes: shoop_session::encode_session(&oversized, "test")
            .unwrap()
            .into(),
    });
    let refused = f
        .until(|s| {
            s.io_task
                .as_ref()
                .is_some_and(|t| t.status == IoTaskStatus::Failed)
        })
        .await;
    assert!(
        refused
            .io_task
            .as_ref()
            .unwrap()
            .message
            .contains("256 MiB"),
        "{:?}",
        refused.io_task
    );
    assert_eq!(refused.tracks[1].loops[0].id, loop_id);
    assert_eq!(refused.tracks[1].loops[0].mode, LoopMode::Playing);
    assert_eq!(
        f.runtime.client().snapshot().unwrap().persistence_epoch,
        epoch
    );
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "saved.shoop".into(),
        bytes: file.bytes,
    });
    f.until(|s| {
        s.io_task.as_ref().is_some_and(|t| {
            t.kind == IoTaskKind::LoadSession && t.status == IoTaskStatus::Completed
        })
    })
    .await;
    let state = f.runtime.snapshot();
    assert_ne!(state.tracks[1].loops[0].id, loop_id);
    assert!(state
        .tracks
        .iter()
        .flat_map(|t| &t.loops)
        .all(|l| l.mode == LoopMode::Stopped));
    assert_eq!(
        f.runtime.client().snapshot().unwrap().persistence_epoch,
        epoch + 1
    );
    f.dispatch(AppIntent::RequestSaveSession);
    let saved_file = f.output().await;
    let saved = shoop_session::decode_session(&saved_file.bytes).unwrap();
    assert_eq!(
        before.media.values().collect::<Vec<_>>(),
        saved.media.values().collect::<Vec<_>>()
    );
    f.dispatch(AppIntent::CompleteIoOutput {
        task_id: saved_file.task_id,
        outcome: FileOutputOutcome::Saved,
    });
    let state = f.runtime.snapshot();
    let loop_id = state.tracks[1].loops[0].id;
    f.dispatch(AppIntent::Loop {
        track_id: state.tracks[1].id,
        loop_id,
        action: LoopAction::PlayClicked,
    });
    f.until(|s| s.tracks[1].loops[0].mode == LoopMode::Playing)
        .await;
    f.dispatch(AppIntent::RequestLoopAudioExport {
        loop_id,
        format: LoopAudioExportFormat::Exact,
    });
    let state = f
        .until(|s| {
            s.io_task
                .as_ref()
                .is_some_and(|t| t.status == IoTaskStatus::AwaitingChannelSelection)
        })
        .await;
    let task_id = state.io_task.as_ref().unwrap().id;
    f.dispatch(AppIntent::ConfirmAudioChannelSelection {
        task_id,
        channels: vec![0, 0],
    });
    assert_eq!(
        f.runtime.snapshot().io_task.as_ref().unwrap().status,
        IoTaskStatus::AwaitingChannelSelection
    );
    f.dispatch(AppIntent::ConfirmAudioChannelSelection {
        task_id,
        channels: vec![0],
    });
    let output = f.output().await;
    assert_eq!(
        shoop_session::decode_loop_audio(&output.bytes)
            .unwrap()
            .channels[0]
            .samples,
        audio.channels[0].samples
    );
    f.dispatch(AppIntent::CompleteIoOutput {
        task_id: output.task_id,
        outcome: FileOutputOutcome::Saved,
    });
    f.dispatch(AppIntent::RequestLoopMidiExport {
        loop_id,
        standard: false,
    });
    let output = f.output().await;
    assert_eq!(
        shoop_session::decode_exact_midi(&output.bytes).unwrap(),
        midi
    );
    f.dispatch(AppIntent::CompleteIoOutput {
        task_id: output.task_id,
        outcome: FileOutputOutcome::Saved,
    });
    f.dispatch(AppIntent::RequestLoopMidiExport {
        loop_id,
        standard: true,
    });
    let output = f.output().await;
    assert!(output.bytes.starts_with(b"MThd"));
    f.dispatch(AppIntent::CompleteIoOutput {
        task_id: output.task_id,
        outcome: FileOutputOutcome::DownloadHandedOff,
    });
    let state = f.runtime.snapshot();
    assert!(state
        .io_task
        .as_ref()
        .unwrap()
        .message
        .contains("Standard MIDI quantizes timing"));
    assert!(state
        .io_task
        .as_ref()
        .unwrap()
        .message
        .contains("disk completion"));
    assert_eq!(state.tracks[1].loops[0].id, loop_id);
    assert_eq!(state.tracks[1].loops[0].mode, LoopMode::Playing);
}
#[shoop_test]
async fn task_callbacks_are_fenced_and_rate_decline_preserves_the_session() {
    let mut f = Fixture::new();
    f.dispatch(AppIntent::RequestLoadSessionPicker);
    let request = f.runtime.take_file_request().unwrap();
    f.dispatch(AppIntent::CancelIoTask {
        task_id: request.task_id,
    });
    let epoch = f.runtime.client().snapshot().unwrap().persistence_epoch;
    let bundle = shoop_session::SessionBundle::new(shoop_session::SessionDocument::empty(2000));
    let bytes: Arc<Vec<u8>> = shoop_session::encode_session(&bundle, "test")
        .unwrap()
        .into();
    f.dispatch(AppIntent::CompleteIoInput {
        task_id: request.task_id,
        name: "late.shoop".into(),
        bytes: Arc::clone(&bytes),
    });
    for _ in 0..8 {
        f.step();
    }
    assert_eq!(
        f.runtime.snapshot().io_task.as_ref().unwrap().status,
        IoTaskStatus::Cancelled
    );
    assert_eq!(
        f.runtime.client().snapshot().unwrap().persistence_epoch,
        epoch
    );
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "convert.shoop".into(),
        bytes,
    });
    let state = f
        .until(|s| {
            s.io_task
                .as_ref()
                .is_some_and(|t| t.status == IoTaskStatus::AwaitingSampleRateConfirmation)
        })
        .await;
    f.dispatch(AppIntent::ConfirmSampleRateConversion {
        task_id: state.io_task.as_ref().unwrap().id,
        accept: false,
    });
    assert_eq!(
        f.runtime.snapshot().io_task.as_ref().unwrap().status,
        IoTaskStatus::Cancelled
    );
    assert_eq!(
        f.runtime.client().snapshot().unwrap().persistence_epoch,
        epoch
    );
}
#[shoop_test]
async fn queued_global_commands_keep_their_original_session_epoch() {
    let mut f = Fixture::new();
    f.hold.set(true);
    f.dispatch(AppIntent::Global(GlobalControlAction::SetSolo(true)));
    let bundle = shoop_session::SessionBundle::new(shoop_session::SessionDocument::empty(1000));
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "empty.shoop".into(),
        bytes: shoop_session::encode_session(&bundle, "test")
            .unwrap()
            .into(),
    });
    f.until(|s| {
        s.io_task
            .as_ref()
            .is_some_and(|t| t.status == IoTaskStatus::Completed)
    })
    .await;
    f.hold.set(false);
    for _ in 0..12 {
        f.step();
    }
    assert!(!f.runtime.snapshot().global_controls.solo);
    assert!(f
        .runtime
        .client()
        .results()
        .values()
        .any(|outcome| matches!(
            outcome,
            Outcome::Refused {
                reason: Refusal::Identity,
                ..
            }
        )));
}

async fn idle() {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            window
                .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 0)
                .unwrap();
        });
        wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
    }
}

#[shoop_test]
async fn a_second_file_request_refuses_without_cancelling_the_active_task() {
    let mut f = Fixture::new();
    f.dispatch(AppIntent::RequestLoadSessionPicker);
    let request = f.runtime.take_file_request().unwrap();
    f.dispatch(AppIntent::RequestSaveSession);
    let state = f.runtime.snapshot();
    assert_eq!(state.io_task.as_ref().unwrap().id, request.task_id);
    assert_eq!(
        state.io_task.as_ref().unwrap().status,
        IoTaskStatus::Running
    );
    assert!(state
        .notifications
        .iter()
        .any(|n| n.message.contains("another file workflow is still active")));
    f.dispatch(AppIntent::CancelIoTask {
        task_id: request.task_id,
    });
    assert_eq!(
        f.runtime.snapshot().io_task.as_ref().unwrap().status,
        IoTaskStatus::Cancelled
    );
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
#[shoop_test]
async fn large_browser_worker_maps_and_returns_media_over_the_old_view_limit() {
    let frames = 33 * 1024 * 1024 / 4;
    let audio = shoop_session::LoopAudio {
        sample_rate: 44100,
        channels: vec![shoop_session::LoopAudioChannel {
            label: "Mono".into(),
            role: "direct".into(),
            samples: vec![0.25; frames],
        }],
    };
    let mut codec = CodecExecutor::new();
    let mut job = codec
        .start(Work::Decode {
            kind: IoTaskKind::ImportLoopAudio,
            bytes: shoop_session::encode_float_wav(&audio).unwrap().into(),
            rate: 44100,
        })
        .unwrap();
    let mut decoded = None;
    for _ in 0..10_000 {
        if let Some(result) = job.poll() {
            let Value::Decoded(value) = result.expect("large worker decode") else {
                panic!("expected decoded audio")
            };
            decoded = Some(value);
            break;
        }
        idle().await;
    }
    let mut job = codec
        .start(Work::Prepare {
            decoded: decoded.expect("large decode deadline"),
            mapping: vec![0, 0],
            update_length: true,
        })
        .unwrap();
    let mut image = None;
    for _ in 0..10_000 {
        if let Some(result) = job.poll() {
            let Value::Image(value) = result.expect("large worker image") else {
                panic!("expected mapped image")
            };
            image = Some(value);
            break;
        }
        idle().await;
    }
    let image = image.expect("large image deadline");
    assert_eq!(image.payloads.len(), 2);
    for payload in &image.payloads {
        let image::Payload::Audio(samples) = payload else {
            panic!("expected PCM")
        };
        assert_eq!(samples.len(), frames);
        assert!(samples.iter().all(|sample| *sample == 0.25));
    }
    codec.reset();
}
