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
                host: HostId(100),
                session: SessionId(200),
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
        let client = Client::new(ClientId(300), HostId(100), SessionId(200)).unwrap();
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
    let epoch = f.runtime.client().snapshot().unwrap().persistence_epoch;
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
    let saved = shoop_session::decode_session(&f.output().await.bytes).unwrap();
    assert_eq!(
        before.media.values().collect::<Vec<_>>(),
        saved.media.values().collect::<Vec<_>>()
    );
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
