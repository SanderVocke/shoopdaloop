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
                    command: Command::Global(GlobalControlAction::SetSolo(true))
                        | Command::Preview(PreviewCommand::Chunk { .. }),
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
async fn pending_composite_capture_notice_survives_encoding_and_platform_file_completion() {
    let fixture = Fixture::new();
    let state = fixture.runtime.client().snapshot().unwrap().clone();
    let authority = fixture.runtime.client().authority().unwrap();
    let mut client = Client::new();
    client.attach().unwrap();
    client
        .receive(ServiceMessage::Attached { authority })
        .unwrap();
    client
        .receive(ServiceMessage::Snapshot {
            authority,
            state: state.clone(),
        })
        .unwrap();
    client.ready_sent().unwrap();
    let mut provider = Provider::new();
    provider
        .start(&state, IoTaskKind::SaveSession, None)
        .unwrap();
    provider.phase = Phase::CaptureSent;
    let key = provider.task.as_ref().unwrap().key;
    let notice = "Newer composite edits were pending; the confirmed arrangement was captured.";
    client
        .receive(ServiceMessage::Persistence {
            authority,
            reply: transfer::Reply::CaptureNotice {
                key,
                message: notice.into(),
            },
        })
        .unwrap();
    let (mut delivery, _port) = local::channel();
    provider.pump(&mut client, &mut delivery).unwrap();
    let Value::Image(image) = (Work::Prepare {
        decoded: Decoded::Session(shoop_session::SessionBundle::new(
            shoop_session::SessionDocument::empty(state.status.sample_rate),
        )),
        mapping: vec![],
        update_length: false,
    })
    .run()
    .unwrap() else {
        panic!()
    };
    provider.captured(image).unwrap();
    let mut output = None;
    for _ in 0..10_000 {
        provider.pump(&mut client, &mut delivery).unwrap();
        if let Some(ready) = provider.output.take() {
            output = Some(ready);
            break;
        }
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::sleep(Duration::from_millis(1));
        idle().await;
    }
    let output = output.expect("file encoding did not finish");
    shoop_session::decode_session(&output.bytes).unwrap();
    provider
        .apply(
            AppIntent::CompleteIoOutput {
                task_id: output.task_id,
                outcome: FileOutputOutcome::Saved,
            },
            &state,
            false,
        )
        .unwrap();
    let task = provider.task().unwrap();
    assert_eq!(task.status, IoTaskStatus::Completed);
    assert!(task.message.contains(notice), "{}", task.message);
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
async fn public_core_file_capabilities_include_copying_but_sync_clone_stays_refused() {
    let mut fixture = Fixture::new();
    let state = fixture.runtime.snapshot();
    for capability in [
        BackendCapability::SessionCapture,
        BackendCapability::SessionContentLoad,
        BackendCapability::LoopContentImport,
        BackendCapability::ClickTrackInsertion,
        BackendCapability::LoopDuplication,
    ] {
        assert!(state.capabilities.supports(capability));
    }
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
        state
            .notifications
            .iter()
            .any(|notice| notice.message.contains("the sync loop cannot be cloned")),
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
    let expected_left = vec![0.25; 64];
    let expected_right = vec![-0.5; 64];
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
                .is_some_and(|t| t.status == IoTaskStatus::AwaitingChannelMapping)
        })
        .await;
    f.dispatch(AppIntent::ConfirmAudioChannelMapping {
        task_id: state.io_task.as_ref().unwrap().id,
        source_for_destination: vec![1, 0],
    });
    let state = f
        .until(|s| {
            s.io_task
                .as_ref()
                .is_some_and(|t| t.status == IoTaskStatus::AwaitingFormatConversionConfirmation)
        })
        .await;
    assert_eq!(state.tracks[1].loops[0].length_frames, 0);
    f.dispatch(AppIntent::ConfirmFormatConversion {
        task_id: state.io_task.as_ref().unwrap().id,
        accept: true,
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
    assert_eq!(f.runtime.snapshot().tracks[1].loops[0].length_frames, 64);
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
    assert_eq!(audio, [&expected_right, &expected_left]);
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
        Self::with_quantum(1)
    }
    fn with_quantum(quantum: usize) -> Self {
        let (render, client) = OfflineRender::new(&EngineDescription {
            quantum,
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
async fn session_retiming_warns_once_preserves_sync_cycles_and_reloads_without_conversion() {
    use shoop_session::*;
    for source_rate in [1000, 2000] {
        let mut f = Fixture::with_quantum(32);
        f.dispatch(AppIntent::Global(GlobalControlAction::SetSync(true)));
        f.dispatch(AppIntent::AddTrack(DirectTrackSpec {
            name: "Retime".into(),
            audio_channels: 1,
            midi: true,
        }));
        f.until(|state| state.tracks.len() == 2 && state.tracks[1].loops.len() == 8)
            .await;
        f.dispatch(AppIntent::RequestSaveSession);
        let baseline = f.output().await;
        let mut bundle = decode_session(&baseline.bytes).unwrap();
        f.dispatch(AppIntent::CompleteIoOutput {
            task_id: baseline.task_id,
            outcome: FileOutputOutcome::Saved,
        });
        bundle.document.sample_rate = source_rate;
        bundle.document.global.sync = true;
        for track in bundle
            .document
            .track_groups
            .iter_mut()
            .flat_map(|group| &mut group.tracks)
        {
            let loop_ = &mut track.loops[0];
            let frames = u64::from(source_rate / 1000) * if track.is_sync { 1008 } else { 4032 };
            loop_.length_frames = frames;
            for channel in &mut loop_.channels {
                channel.data_length_frames = frames;
                channel.start_offset_frames = 0;
                channel.capture_alignment_frames = 0;
                let id = format!("retime_{}", channel.id);
                let payload = match channel.data_type {
                    DataTypeDocument::Audio => MediaPayload::Audio(AudioPayload {
                        samples: vec![0.25; frames as usize],
                    }),
                    DataTypeDocument::Midi => MediaPayload::Midi(ExactMidi {
                        sample_rate: source_rate,
                        length_frames: frames,
                        start_state: vec![vec![0xb0, 7, 99]],
                        events: vec![
                            ExactMidiEvent {
                                frame: 0,
                                order: 0,
                                data: vec![0x90, 60, 100],
                            },
                            ExactMidiEvent {
                                frame: frames - 1,
                                order: 1,
                                data: vec![0x80, 60, 0],
                            },
                        ],
                    }),
                };
                channel.media_id = Some(id.clone());
                bundle.media.insert(id, payload);
            }
        }
        let bytes: Arc<Vec<u8>> = encode_session(&bundle, "retime-workflow").unwrap().into();
        let before = f.runtime.snapshot();
        for accept in [false, true] {
            f.dispatch(AppIntent::LoadSessionBytes {
                name: "old-quantum.shoop".into(),
                bytes: Arc::clone(&bytes),
            });
            let warning = f
                .until(|state| {
                    state.io_task.as_ref().is_some_and(|task| {
                        task.status == IoTaskStatus::AwaitingFormatConversionConfirmation
                    })
                })
                .await;
            assert!(warning
                .io_task
                .as_ref()
                .unwrap()
                .format_conversion_warning
                .is_some());
            assert_eq!(warning.tracks[0].loops[0].length_frames, 0);
            f.dispatch(AppIntent::ConfirmFormatConversion {
                task_id: warning.io_task.as_ref().unwrap().id,
                accept,
            });
            if !accept {
                assert_eq!(
                    f.runtime.snapshot().io_task.as_ref().unwrap().status,
                    IoTaskStatus::Cancelled
                );
                assert_eq!(
                    f.runtime.snapshot().tracks[0].loops[0].id,
                    before.tracks[0].loops[0].id
                );
            } else {
                let loaded = f
                    .until(|state| {
                        state
                            .io_task
                            .as_ref()
                            .is_some_and(|task| task.status == IoTaskStatus::Completed)
                    })
                    .await;
                assert_eq!(loaded.status.quantum_frames, 32);
                assert_eq!(loaded.tracks[0].loops[0].length_frames, 1024);
                assert_eq!(loaded.tracks[1].loops[0].length_frames, 4096);
                assert!(loaded
                    .tracks
                    .iter()
                    .flat_map(|track| &track.loops)
                    .all(|loop_| loop_.mode == LoopMode::Stopped));
            }
        }
        f.dispatch(AppIntent::RequestSaveSession);
        let converted = f.output().await;
        f.dispatch(AppIntent::CompleteIoOutput {
            task_id: converted.task_id,
            outcome: FileOutputOutcome::Saved,
        });
        let saved = decode_session(&converted.bytes).unwrap();
        assert!(saved
            .media
            .values()
            .filter_map(|payload| match payload {
                MediaPayload::Audio(audio) => Some(&audio.samples),
                _ => None,
            })
            .flatten()
            .all(|sample| *sample == 0.25));
        f.dispatch(AppIntent::LoadSessionBytes {
            name: "converted.shoop".into(),
            bytes: Arc::clone(&converted.bytes),
        });
        f.until(|state| {
            state
                .io_task
                .as_ref()
                .is_some_and(|task| task.status == IoTaskStatus::Completed)
        })
        .await;
        assert_eq!(bundle.document.sample_rate, source_rate);
    }
}

async fn live_family_fixture(quantum: usize) -> Fixture {
    use shoop_session::*;
    let mut f = Fixture::with_quantum(quantum);
    f.dispatch(AppIntent::Global(GlobalControlAction::SetSync(true)));
    f.dispatch(AppIntent::AddTrack(DirectTrackSpec {
        name: "Family".into(),
        audio_channels: 1,
        midi: true,
    }));
    f.until(|state| state.tracks.len() == 2 && state.tracks[1].loops.len() == 8)
        .await;
    f.dispatch(AppIntent::RequestSaveSession);
    let saved = f.output().await;
    let mut bundle = decode_session(&saved.bytes).unwrap();
    f.dispatch(AppIntent::CompleteIoOutput {
        task_id: saved.task_id,
        outcome: FileOutputOutcome::Saved,
    });
    bundle.document.global.sync = true;
    for track in bundle
        .document
        .track_groups
        .iter_mut()
        .flat_map(|group| &mut group.tracks)
    {
        let content = &mut track.loops[0];
        let frames = quantum as u64 * if track.is_sync { 3 } else { 12 };
        content.length_frames = frames;
        for channel in &mut content.channels {
            channel.data_length_frames = frames + 8;
            channel.start_offset_frames = 2;
            channel.capture_alignment_frames = 2;
            let id = format!("family_{}", channel.id);
            channel.media_id = Some(id.clone());
            bundle.media.insert(
                id,
                match channel.data_type {
                    DataTypeDocument::Audio => MediaPayload::Audio(AudioPayload {
                        samples: vec![0.25; frames as usize + 8],
                    }),
                    DataTypeDocument::Midi => MediaPayload::Midi(ExactMidi {
                        sample_rate: 1000,
                        length_frames: frames + 8,
                        start_state: vec![vec![0xb0, 7, 99]],
                        events: vec![
                            ExactMidiEvent {
                                frame: 5,
                                order: 0,
                                data: vec![0x90, 60, 100],
                            },
                            ExactMidiEvent {
                                frame: frames + 5,
                                order: 1,
                                data: vec![0x80, 60, 0],
                            },
                        ],
                    }),
                },
            );
        }
    }
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "aligned-family.shoop".into(),
        bytes: encode_session(&bundle, "family").unwrap().into(),
    });
    let state = f
        .until(|state| {
            state
                .io_task
                .as_ref()
                .is_some_and(|task| task.status == IoTaskStatus::Completed)
        })
        .await;
    for track in &state.tracks {
        f.dispatch(AppIntent::Loop {
            track_id: track.id,
            loop_id: track.loops[0].id,
            action: LoopAction::PlayClicked,
        });
    }
    f.until(|state| {
        state
            .tracks
            .iter()
            .all(|track| track.loops[0].mode == LoopMode::Playing)
    })
    .await;
    f
}

#[shoop_test]
async fn live_audio_and_midi_retiming_preserves_history_sync_dependents_and_current_controls() {
    use shoop_session::*;
    for audio in [true, false] {
        for rate in [1000, 2000] {
            let mut f = live_family_fixture(32).await;
            let before = f.runtime.snapshot();
            let target_track = usize::from(!audio);
            let target = before.tracks[target_track].loops[0].id;
            let child = before.tracks[1].loops[0].id;
            let bytes: Arc<Vec<u8>> = if audio {
                encode_float_wav(&LoopAudio {
                    sample_rate: rate,
                    channels: vec![LoopAudioChannel {
                        label: "Incoming".into(),
                        role: "direct".into(),
                        samples: vec![0.5; (112 * rate / 1000) as usize],
                    }],
                })
                .unwrap()
                .into()
            } else {
                encode_exact_midi(&ExactMidi {
                    sample_rate: rate,
                    length_frames: u64::from(112 * rate / 1000),
                    start_state: vec![vec![0xb0, 7, 75]],
                    events: vec![ExactMidiEvent {
                        frame: u64::from(9 * rate / 1000),
                        order: 0,
                        data: vec![0x90, 62, 100],
                    }],
                })
                .unwrap()
                .into()
            };
            for accept in [false, true] {
                f.dispatch(if audio {
                    AppIntent::ImportLoopAudioBytes {
                        loop_id: target,
                        name: "unaligned.wav".into(),
                        bytes: Arc::clone(&bytes),
                        update_loop_length: true,
                    }
                } else {
                    AppIntent::ImportLoopMidiBytes {
                        loop_id: target,
                        name: "unaligned.shoop-midi".into(),
                        bytes: Arc::clone(&bytes),
                        update_loop_length: true,
                    }
                });
                if audio {
                    let mapping = f
                        .until(|state| {
                            state.io_task.as_ref().is_some_and(|task| {
                                task.status == IoTaskStatus::AwaitingChannelMapping
                            })
                        })
                        .await;
                    f.dispatch(AppIntent::ConfirmAudioChannelMapping {
                        task_id: mapping.io_task.as_ref().unwrap().id,
                        source_for_destination: vec![0],
                    });
                }
                let warning = f
                    .until(|state| {
                        state.io_task.as_ref().is_some_and(|task| {
                            task.status == IoTaskStatus::AwaitingFormatConversionConfirmation
                        })
                    })
                    .await;
                assert_eq!(warning.tracks[0].loops[0].length_frames, 96);
                assert_eq!(warning.tracks[1].loops[0].length_frames, 384);
                f.dispatch(AppIntent::ConfirmFormatConversion {
                    task_id: warning.io_task.as_ref().unwrap().id,
                    accept,
                });
                if !accept {
                    assert_eq!(
                        f.runtime.snapshot().io_task.as_ref().unwrap().status,
                        IoTaskStatus::Cancelled
                    );
                }
            }
            f.dispatch(AppIntent::Loop {
                track_id: before.tracks[1].id,
                loop_id: child,
                action: LoopAction::GainChanged(0.625),
            });
            let state = f
                .until(|state| {
                    state
                        .io_task
                        .as_ref()
                        .is_some_and(|task| task.status == IoTaskStatus::Completed)
                })
                .await;
            assert_eq!(state.tracks[target_track].loops[0].length_frames, 128);
            assert_eq!(state.tracks[1].loops[0].gain, 0.625);
            assert!(state
                .tracks
                .iter()
                .all(|track| track.loops[0].mode == LoopMode::Playing));
            assert_eq!(state.tracks[0].loops[0].id, before.tracks[0].loops[0].id);
            if audio {
                assert_eq!(state.tracks[1].loops[0].length_frames, 448);
            } else {
                assert_eq!(state.tracks[0].loops[0].length_frames, 96);
            }
            f.dispatch(AppIntent::RequestSaveSession);
            let saved = f.output().await;
            let bundle = decode_session(&saved.bytes).unwrap();
            let track = bundle
                .document
                .track_groups
                .iter()
                .flat_map(|group| &group.tracks)
                .find(|track| !track.is_sync)
                .unwrap();
            let content = &track.loops[0];
            for channel in &content.channels {
                let payload = &bundle.media[channel.media_id.as_ref().unwrap()];
                match payload {
                    MediaPayload::Audio(samples) => {
                        assert_eq!(samples.samples.len(), if audio { 458 } else { 449 });
                        assert_eq!(
                            channel.start_offset_frames + channel.capture_alignment_frames,
                            5
                        );
                    }
                    MediaPayload::Midi(midi) if audio => {
                        assert_eq!(midi.length_frames, 458);
                        assert_eq!(midi.events[0].frame, 6);
                        assert_eq!(midi.start_state, vec![vec![0xb0, 7, 99]]);
                        assert_eq!(midi.events[1].data, vec![0x80, 60, 0]);
                    }
                    MediaPayload::Midi(midi) => {
                        assert_eq!(midi.length_frames, 128);
                        assert_eq!(midi.events[0].frame, 10);
                        assert_eq!(midi.start_state, vec![vec![0xb0, 7, 75]]);
                    }
                }
            }
        }
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
        .length_frames = u64::MAX;
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "invalid-period.shoop".into(),
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
            .contains("Invalid"),
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
                .is_some_and(|t| t.status == IoTaskStatus::AwaitingFormatConversionConfirmation)
        })
        .await;
    f.dispatch(AppIntent::ConfirmFormatConversion {
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

async fn unaligned_audio(f: &mut Fixture, quantum: usize, update_length: bool) -> TaskId {
    let root = f.runtime.snapshot().tracks[0].loops[0].id;
    let audio = shoop_session::LoopAudio {
        sample_rate: 1000,
        channels: vec![shoop_session::LoopAudioChannel {
            label: "New".into(),
            role: "direct".into(),
            samples: vec![0.5; quantum * 7 / 2],
        }],
    };
    f.dispatch(AppIntent::ImportLoopAudioBytes {
        loop_id: root,
        name: "new.wav".into(),
        bytes: shoop_session::encode_float_wav(&audio).unwrap().into(),
        update_loop_length: update_length,
    });
    let state = f
        .until(|state| {
            state
                .io_task
                .as_ref()
                .is_some_and(|task| task.status == IoTaskStatus::AwaitingChannelMapping)
        })
        .await;
    let id = state.io_task.as_ref().unwrap().id;
    f.dispatch(AppIntent::ConfirmAudioChannelMapping {
        task_id: id,
        source_for_destination: vec![0],
    });
    id
}

#[shoop_test]
async fn live_family_import_cancels_during_capture_and_refuses_stale_consent() {
    for stale in [false, true] {
        let mut f = live_family_fixture(32).await;
        let id = unaligned_audio(&mut f, 32, true).await;
        let state = f
            .until(|state| {
                state.io_task.as_ref().is_some_and(|task| {
                    task.status == IoTaskStatus::AwaitingFormatConversionConfirmation
                })
            })
            .await;
        if stale {
            f.dispatch(AppIntent::Loop {
                track_id: state.tracks[1].id,
                loop_id: state.tracks[1].loops[0].id,
                action: LoopAction::RecordClicked,
            });
            f.until(|state| state.tracks[1].loops[0].mode == LoopMode::Recording)
                .await;
            f.dispatch(AppIntent::ConfirmFormatConversion {
                task_id: id,
                accept: true,
            });
            let state = f
                .until(|state| {
                    state
                        .io_task
                        .as_ref()
                        .is_some_and(|task| task.status == IoTaskStatus::Failed)
                })
                .await;
            assert!(state.io_task.as_ref().unwrap().message.contains("changed"));
            assert_eq!(state.tracks[1].loops[0].mode, LoopMode::Recording);
        } else {
            f.dispatch(AppIntent::ConfirmFormatConversion {
                task_id: id,
                accept: true,
            });
            let state = f
                .until(|state| {
                    state
                        .io_task
                        .as_ref()
                        .is_some_and(|task| task.message.contains("Capturing"))
                })
                .await;
            assert_eq!(state.tracks[1].loops[0].length_frames, 384);
            f.dispatch(AppIntent::CancelIoTask { task_id: id });
            assert_eq!(
                f.runtime.snapshot().io_task.as_ref().unwrap().status,
                IoTaskStatus::Cancelled
            );
            for _ in 0..16 {
                f.step();
                idle().await;
            }
            assert_eq!(f.runtime.snapshot().tracks[1].loops[0].length_frames, 384);
            let id = unaligned_audio(&mut f, 32, true).await;
            f.until(|state| {
                state.io_task.as_ref().is_some_and(|task| {
                    task.status == IoTaskStatus::AwaitingFormatConversionConfirmation
                })
            })
            .await;
            f.dispatch(AppIntent::ConfirmFormatConversion {
                task_id: id,
                accept: false,
            });
        }
        assert_eq!(f.runtime.snapshot().tracks[0].loops[0].length_frames, 96);
    }
}

#[shoop_test]
async fn live_import_at_16_and_64_frames_keeps_raw_lengths_when_the_window_is_unchanged() {
    for quantum in [16, 64] {
        let mut f = live_family_fixture(quantum).await;
        unaligned_audio(&mut f, quantum, false).await;
        let state = f
            .until(|state| {
                state
                    .io_task
                    .as_ref()
                    .is_some_and(|task| task.status == IoTaskStatus::Completed)
            })
            .await;
        assert_eq!(state.tracks[0].loops[0].length_frames, quantum as u64 * 3);
        assert_eq!(state.tracks[1].loops[0].length_frames, quantum as u64 * 12);
        let id = unaligned_audio(&mut f, quantum, true).await;
        f.until(|state| {
            state.io_task.as_ref().is_some_and(|task| {
                task.status == IoTaskStatus::AwaitingFormatConversionConfirmation
            })
        })
        .await;
        f.dispatch(AppIntent::ConfirmFormatConversion {
            task_id: id,
            accept: true,
        });
        let state = f
            .until(|state| {
                state
                    .io_task
                    .as_ref()
                    .is_some_and(|task| task.status == IoTaskStatus::Completed)
            })
            .await;
        assert_eq!(state.tracks[0].loops[0].length_frames, quantum as u64 * 4);
        assert_eq!(state.tracks[1].loops[0].length_frames, quantum as u64 * 14);
        assert!(state
            .tracks
            .iter()
            .all(|track| track.loops[0].mode == LoopMode::Playing));
    }
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

#[shoop_test]
async fn system_preview_is_local_and_stale_completion_cannot_restart_it() {
    let mut f = Fixture::new();
    let loop_id = f.runtime.snapshot().tracks[0].loops[0].id;
    let before: Vec<_> = f
        .runtime
        .snapshot()
        .tracks
        .iter()
        .flat_map(|t| &t.loops)
        .map(|l| (l.id, l.length_frames, l.content_revision))
        .collect();
    f.dispatch(AppIntent::PreviewClickTrack {
        output: ClickTrackPreviewOutput::System,
        loop_id,
        request: ClickTrackRequest::default(),
    });
    let mut output = None;
    for _ in 0..10_000 {
        f.step();
        idle().await;
        output = f.runtime.take_audio_preview();
        if output.is_some() {
            break;
        }
    }
    let output = output.expect("system player must receive PCM");
    assert_eq!(output.output, ClickTrackPreviewOutput::System);
    assert!(output.samples.iter().any(|v| v.abs() > 0.));
    assert_eq!(
        f.runtime
            .snapshot()
            .tracks
            .iter()
            .flat_map(|t| &t.loops)
            .map(|l| (l.id, l.length_frames, l.content_revision))
            .collect::<Vec<_>>(),
        before
    );
    f.dispatch(AppIntent::ClickTrackPreviewStarted {
        request_id: output.request_id,
    });
    assert_eq!(
        f.runtime.snapshot().click_track.preview_status,
        ClickTrackPreviewStatus::Playing
    );
    f.dispatch(AppIntent::StopClickTrackPreview);
    f.dispatch(AppIntent::CompleteClickTrackPreview {
        request_id: output.request_id,
        success: true,
        message: "late callback".into(),
    });
    assert_eq!(
        f.runtime.snapshot().click_track.preview_status,
        ClickTrackPreviewStatus::Cancelled
    );
    assert!(f.runtime.take_audio_preview().is_none());
    f.dispatch(AppIntent::PreviewClickTrack {
        output: ClickTrackPreviewOutput::System,
        loop_id,
        request: ClickTrackRequest::default(),
    });
    f.dispatch(AppIntent::StopClickTrackPreview);
    for _ in 0..100 {
        f.step();
        idle().await;
        assert!(f.runtime.take_audio_preview().is_none());
    }
}

#[shoop_test]
async fn master_preview_uses_service_rt_path_and_stops_on_cancel_or_lease_loss() {
    let mut f = Fixture::new();
    let loop_id = f.runtime.snapshot().tracks[0].loops[0].id;
    let content_before = f.runtime.snapshot().tracks[0].loops[0].content_revision;
    assert!(f.runtime.snapshot().click_track.master_preview_available);
    for detach in [false, true] {
        f.dispatch(AppIntent::PreviewClickTrack {
            output: ClickTrackPreviewOutput::MasterBus,
            loop_id,
            request: ClickTrackRequest::default(),
        });
        f.until(|s| s.click_track.preview_status == ClickTrackPreviewStatus::Playing)
            .await;
        assert!(
            f.runtime.take_audio_preview().is_none(),
            "Master must never reach the client system player"
        );
        assert_eq!(
            f.runtime.snapshot().tracks[0].loops[0].content_revision,
            content_before
        );
        if detach {
            f.service.detach(ConnectionId(1));
            for _ in 0..200 {
                f.service.tick(STEP);
            }
            assert_eq!(
                f.service.snapshot().preview.preview_status,
                ClickTrackPreviewStatus::Cancelled
            );
        } else {
            f.dispatch(AppIntent::StopClickTrackPreview);
            f.until(|s| s.click_track.preview_status == ClickTrackPreviewStatus::Cancelled)
                .await;
            // A second preview must start after cancellation releases the prepared upload.
            f.dispatch(AppIntent::PreviewClickTrack {
                output: ClickTrackPreviewOutput::MasterBus,
                loop_id,
                request: ClickTrackRequest::default(),
            });
            f.until(|s| s.click_track.preview_status == ClickTrackPreviewStatus::Completed)
                .await;
        }
    }
}

#[shoop_test]
async fn preview_cancellation_discards_unsent_chunks_and_session_replacement_stops_playback() {
    let mut f = Fixture::new();
    let loop_id = f.runtime.snapshot().tracks[0].loops[0].id;
    f.hold.set(true);
    f.dispatch(AppIntent::PreviewClickTrack {
        output: ClickTrackPreviewOutput::MasterBus,
        loop_id,
        request: ClickTrackRequest::default(),
    });
    for _ in 0..10_000 {
        f.step();
        idle().await;
        if f.service.snapshot().preview.preview_request_id != 0 {
            break;
        }
    }
    assert_ne!(f.service.snapshot().preview.preview_request_id, 0);
    f.dispatch(AppIntent::StopClickTrackPreview);
    for _ in 0..100 {
        f.step();
        idle().await;
    }
    assert_eq!(
        f.service.snapshot().preview.preview_status,
        ClickTrackPreviewStatus::Cancelled
    );
    f.hold.set(false);
    f.dispatch(AppIntent::PreviewClickTrack {
        output: ClickTrackPreviewOutput::MasterBus,
        loop_id,
        request: ClickTrackRequest::default(),
    });
    f.until(|s| s.click_track.preview_status == ClickTrackPreviewStatus::Playing)
        .await;
    f.dispatch(AppIntent::RequestNewSession);
    f.until(|s| {
        s.io_task
            .as_ref()
            .is_some_and(|t| t.status == IoTaskStatus::Completed)
    })
    .await;
    assert_eq!(
        f.runtime.snapshot().click_track.preview_status,
        ClickTrackPreviewStatus::Cancelled
    );
    assert!(f.runtime.take_audio_preview().is_none());
}

#[shoop_test]
async fn master_preview_identity_survives_renamed_session_and_clears_when_removed() {
    let mut f = Fixture::new();
    let original = f.runtime.snapshot().buses[0].id;
    f.dispatch(AppIntent::AddBus(BusSpec {
        name: "Master".into(),
        channel_count: 2,
        fx: None,
        creation_request_id: None,
    }));
    let state = f
        .until(|s| {
            s.buses.len() == 2
                && s.buses
                    .iter()
                    .all(|b| b.structural_state == StructuralState::Confirmed)
        })
        .await;
    f.dispatch(AppIntent::Bus {
        bus_id: state.buses[1].id,
        action: BusAction::MoveBefore(Some(original)),
    });
    f.until(|s| s.buses[0].id != original).await;
    f.dispatch(AppIntent::RequestSaveSession);
    let saved = f.output().await;
    f.dispatch(AppIntent::CompleteIoOutput {
        task_id: saved.task_id,
        outcome: FileOutputOutcome::Saved,
    });
    let mut bundle = shoop_session::decode_session(&saved.bytes).unwrap();
    assert!(matches!(
        bundle.document.preview_bus,
        shoop_session::PreviewBusDocument::Bus(_)
    ));
    bundle.document.buses[0].name = "Renamed output".into();
    let bytes = shoop_session::encode_session(&bundle, "preview").unwrap();
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "renamed.shoop".into(),
        bytes: bytes.into(),
    });
    let state = f
        .until(|s| {
            s.io_task
                .as_ref()
                .is_some_and(|t| t.status == IoTaskStatus::Completed)
        })
        .await;
    assert!(state.click_track.master_preview_available);
    assert_eq!(state.buses[0].name, "Master");
    let bus_id = state
        .buses
        .iter()
        .find(|b| b.name == "Renamed output")
        .unwrap()
        .id;
    let loop_id = state.tracks[0].loops[0].id;
    assert_eq!(f.service.snapshot().preview_bus, Some(bus_id));
    f.dispatch(AppIntent::PreviewClickTrack {
        loop_id,
        request: Default::default(),
        output: ClickTrackPreviewOutput::MasterBus,
    });
    f.until(|s| s.click_track.preview_status == ClickTrackPreviewStatus::Playing)
        .await;
    f.dispatch(AppIntent::Bus {
        bus_id,
        action: BusAction::Remove,
    });
    f.until(|s| s.buses.len() == 1 && !s.click_track.master_preview_available)
        .await;
    assert_eq!(f.service.snapshot().preview_bus, None);
    assert_eq!(
        f.service.snapshot().preview.preview_status,
        ClickTrackPreviewStatus::Cancelled
    );
    f.dispatch(AppIntent::RequestNewSession);
    f.until(|s| s.buses.len() == 1 && s.click_track.master_preview_available)
        .await;
    assert_eq!(
        f.service.snapshot().preview_bus,
        Some(f.runtime.snapshot().buses[0].id)
    );
}
