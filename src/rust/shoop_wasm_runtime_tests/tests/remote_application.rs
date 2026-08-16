#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use js_sys::{Array, Function, Promise};
use shoop_app::CooperativeApplicationRuntime;
use shoop_app_api::{
    AppIntent, AppSnapshot, ClickTrackRequest, DirectTrackSpec, GlobalControlAction, IoTaskStatus,
    LoopAction, LoopMode,
};
use shoop_backend::BackendDriverState;
use shoop_worklet_client::{MessageEndpoint, NullHostMidiBridge, RemoteBackendControl};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[cfg(feature = "wasm-test-browser")]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

const GENERATION: u64 = 1;
const QUANTUM: usize = 128;
const MAX_DRIVE_STEPS: usize = 200;

#[wasm_bindgen(module = "/js/worker_fixture.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = spawnRemoteApplicationFixture)]
    async fn spawn_remote_application_fixture(
        runtime: &str,
        asset_location: &str,
        protocol_version: u16,
        command_max_bytes: u32,
        on_message: &Function,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = remoteApplicationPostMessage)]
    fn remote_application_post_message(fixture: &JsValue, message: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(catch, js_name = remoteApplicationProcessQuantum)]
    fn remote_application_process_quantum(
        fixture: &JsValue,
        inputs: &Array,
        output_channels: u32,
    ) -> Result<Promise, JsValue>;

    #[wasm_bindgen(js_name = remoteApplicationTurn)]
    fn remote_application_turn() -> Promise;

    #[wasm_bindgen(catch, js_name = shutdownRemoteApplicationFixture)]
    fn shutdown_remote_application_fixture(fixture: &JsValue) -> Result<Promise, JsValue>;
}

fn runtime_and_assets() -> (&'static str, &'static str) {
    let browser = cfg!(feature = "wasm-test-browser");
    let runtime = if browser { "chrome" } else { "node" };
    let asset_location = if browser {
        option_env!("SHOOP_WASM_TEST_ASSET_BASE")
    } else {
        option_env!("SHOOP_WASM_TEST_ASSET_DIR")
    }
    .expect("run_wasm_tests.py must provide the staged asset location");
    (runtime, asset_location)
}

#[derive(Clone)]
struct FixtureEndpoint {
    fixture: JsValue,
}

impl MessageEndpoint for FixtureEndpoint {
    fn post_message(&self, message: &str) -> anyhow::Result<()> {
        remote_application_post_message(&self.fixture, message)
            .map_err(|error| anyhow::anyhow!("could not post fixture message: {error:?}"))
    }
}

struct RemoteAppHarness {
    fixture: JsValue,
    runtime: Option<CooperativeApplicationRuntime>,
    control: RemoteBackendControl,
    callback_errors: Rc<RefCell<Vec<String>>>,
    _on_message: Closure<dyn FnMut(JsValue)>,
}

impl RemoteAppHarness {
    async fn start() -> Self {
        let (backend, control) =
            shoop_worklet_client::RemoteWorkletBackend::new(NullHostMidiBridge);
        let callback_errors = Rc::new(RefCell::new(Vec::new()));
        let callback_control = control.clone();
        let callback_error_sink = Rc::clone(&callback_errors);
        let on_message = Closure::wrap(Box::new(move |message: JsValue| {
            let Some(message) = message.as_string() else {
                callback_error_sink.borrow_mut().push(format!(
                    "fixture returned a non-string message: {message:?}"
                ));
                return;
            };
            if let Err(error) = callback_control.receive(GENERATION, &message) {
                callback_error_sink.borrow_mut().push(error.to_string());
            }
        }) as Box<dyn FnMut(JsValue)>);
        let (runtime_name, assets) = runtime_and_assets();
        let fixture = spawn_remote_application_fixture(
            runtime_name,
            assets,
            shoop_audio_protocol::PROTOCOL_VERSION,
            shoop_audio_protocol::COMMAND_MAX_BYTES as u32,
            on_message.as_ref().unchecked_ref(),
        )
        .await
        .unwrap_or_else(|error| panic!("could not start remote application fixture: {error:?}"));
        control
            .attach(
                Box::new(FixtureEndpoint {
                    fixture: fixture.clone(),
                }),
                GENERATION,
                2,
                2,
            )
            .expect("attach remote application endpoint");
        control.set_driver_state(BackendDriverState::Dummy);
        let runtime = CooperativeApplicationRuntime::start(Box::new(backend))
            .expect("start cooperative application runtime");
        let mut harness = Self {
            fixture,
            runtime: Some(runtime),
            control,
            callback_errors,
            _on_message: on_message,
        };
        harness.drive_steps(4).await;
        harness
    }

    fn runtime(&self) -> &CooperativeApplicationRuntime {
        self.runtime.as_ref().expect("remote runtime is active")
    }

    fn runtime_mut(&mut self) -> &mut CooperativeApplicationRuntime {
        self.runtime.as_mut().expect("remote runtime is active")
    }

    fn snapshot(&self) -> Arc<AppSnapshot> {
        self.runtime().snapshot()
    }

    fn dispatch(&mut self, intent: AppIntent) {
        self.runtime_mut()
            .dispatch(intent)
            .expect("dispatch remote application intent");
    }

    fn check_callback_errors(&self) {
        let errors = self.callback_errors.borrow();
        assert!(errors.is_empty(), "remote callback errors: {errors:?}");
    }

    async fn turn(&self) {
        JsFuture::from(remote_application_turn())
            .await
            .expect("wait for remote Worker turn");
        self.check_callback_errors();
    }

    async fn drive_step(&mut self) {
        self.runtime_mut().tick(Duration::from_millis(16));
        self.turn().await;
        self.runtime_mut().tick(Duration::ZERO);
        self.turn().await;
    }

    async fn drive_steps(&mut self, count: usize) {
        for _ in 0..count {
            self.drive_step().await;
        }
    }

    async fn drive_until(&mut self, description: &str, predicate: impl Fn(&AppSnapshot) -> bool) {
        for _ in 0..MAX_DRIVE_STEPS {
            if predicate(&self.snapshot()) {
                return;
            }
            self.drive_step().await;
        }
        panic!(
            "remote application did not reach {description}; readiness={:?}, notifications={:?}",
            self.control.readiness(),
            self.snapshot().notifications
        );
    }

    async fn process_quantum(&mut self, inputs: &[Vec<f32>], output_channels: u32) {
        let js_inputs = Array::new();
        for input in inputs {
            let channel = Array::new();
            for sample in input {
                channel.push(&JsValue::from_f64(f64::from(*sample)));
            }
            js_inputs.push(&channel);
        }
        let promise =
            remote_application_process_quantum(&self.fixture, &js_inputs, output_channels)
                .expect("request explicit remote quantum");
        JsFuture::from(promise)
            .await
            .expect("process explicit remote quantum");
        self.drive_steps(2).await;
    }

    async fn shutdown(mut self) {
        self.runtime.take();
        self.control.detach(false);
        let promise = shutdown_remote_application_fixture(&self.fixture)
            .expect("request remote application fixture shutdown");
        JsFuture::from(promise)
            .await
            .expect("shutdown remote application fixture");
        self.check_callback_errors();
    }
}

fn has_notification(snapshot: &AppSnapshot, needle: &str) -> bool {
    snapshot
        .notifications
        .iter()
        .any(|notification| notification.message.contains(needle))
}

async fn add_audio_track(harness: &mut RemoteAppHarness) -> shoop_app_api::TrackState {
    harness.dispatch(AppIntent::AddTrack(DirectTrackSpec {
        name: "Remote fixture".to_owned(),
        audio_channels: 2,
        midi: false,
    }));
    harness
        .drive_until("created audio track", |snapshot| snapshot.tracks.len() == 2)
        .await;
    harness.snapshot().tracks[1].clone()
}

async fn generate_click(harness: &mut RemoteAppHarness, loop_id: shoop_app_api::LoopId) {
    harness.dispatch(AppIntent::GenerateClickTrack {
        loop_id,
        request: ClickTrackRequest {
            bpm: 600.0,
            click_count: 2,
            ..Default::default()
        },
    });
    harness
        .drive_until("completed click generation", |snapshot| {
            snapshot
                .io_task
                .as_ref()
                .is_some_and(|task| task.status == IoTaskStatus::Completed)
        })
        .await;
}

#[shoop_wasm_test_support::shoop_test(
    wasm_only = "requires the production WebAssembly Worker runtime"
)]
async fn remote_application_stack_processes_intents_and_engine_quanta() {
    let mut harness = RemoteAppHarness::start().await;
    let track = add_audio_track(&mut harness).await;
    harness
        .process_quantum(&[vec![0.0; QUANTUM], vec![0.0; QUANTUM]], 2)
        .await;
    harness
        .drive_until("published processed quantum", |snapshot| {
            snapshot.status.callback_count > 0
        })
        .await;
    assert_eq!(track.loops.len(), 8);
    assert_eq!(
        harness.snapshot().status.driver_state,
        shoop_app_api::AudioDriverState::Dummy
    );
    harness.shutdown().await;
}

#[shoop_wasm_test_support::shoop_test(
    wasm_only = "requires the production WebAssembly Worker runtime"
)]
async fn remote_loop_duplication_reproduces_async_capture_error() {
    let mut harness = RemoteAppHarness::start().await;
    let track = add_audio_track(&mut harness).await;
    let source = track.loops[0].id;
    let target = track.loops[1].id;
    generate_click(&mut harness, source).await;
    harness.dispatch(AppIntent::Loop {
        track_id: track.id,
        loop_id: source,
        action: LoopAction::DuplicateTo(target),
    });
    harness
        .drive_until("reported asynchronous duplication failure", |snapshot| {
            has_notification(snapshot, "asynchronous session capture is not complete")
        })
        .await;
    assert!(harness.snapshot().tracks[1].loops[1].empty);
    harness.shutdown().await;
}

#[shoop_wasm_test_support::shoop_test(
    wasm_only = "requires the production WebAssembly Worker runtime"
)]
async fn remote_peak_publication_reproduces_accumulated_maximum() {
    let mut harness = RemoteAppHarness::start().await;
    let track = add_audio_track(&mut harness).await;
    let source = track.loops[0].id;
    generate_click(&mut harness, source).await;
    harness.dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)));
    harness.dispatch(AppIntent::Loop {
        track_id: track.id,
        loop_id: source,
        action: LoopAction::PlayClicked,
    });
    for _ in 0..4 {
        harness.process_quantum(&[], 2).await;
    }
    harness
        .drive_until("published loud loop peak", |snapshot| {
            snapshot.tracks[1].loops[0].peak_left_db > -100.0
        })
        .await;
    let loud_peak = harness.snapshot().tracks[1].loops[0].peak_left_db;
    harness.dispatch(AppIntent::Loop {
        track_id: track.id,
        loop_id: source,
        action: LoopAction::StopClicked,
    });
    harness.drive_steps(2).await;
    harness.process_quantum(&[], 2).await;
    harness
        .drive_until("stopped loop", |snapshot| {
            snapshot.tracks[1].loops[0].mode == LoopMode::Stopped
        })
        .await;
    for _ in 0..3 {
        harness.process_quantum(&[], 2).await;
    }
    let retained_peak = harness.snapshot().tracks[1].loops[0].peak_left_db;
    assert!(loud_peak > -100.0, "expected a loud peak, got {loud_peak}");
    assert!(
        (retained_peak - loud_peak).abs() < f32::EPSILON,
        "peak unexpectedly reset: loud={loud_peak}, retained={retained_peak}"
    );
    harness.shutdown().await;
}
