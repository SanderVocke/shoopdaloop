#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use js_sys::{Array, Function, Promise};
use shoop_app::CooperativeApplicationRuntime;
use shoop_app_api::{
    AppIntent, AppSnapshot, ClickTrackRequest, DefaultPlaybackMode, DirectTrackSpec,
    GlobalControlAction, IoTaskKind, IoTaskStatus, LoopAction, LoopMode, OxiSynthControl,
    OxiSynthMidiCcAssignment, OxiSynthParameter, TrackAction, TrackProcessorEditorState,
    TrackProcessorTypeId, TrackSpec, TrackSpecTopology,
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
        harness
            .drive_until("published engine sample rate", |snapshot| {
                snapshot.status.sample_rate == 48_000
            })
            .await;
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
            "remote application did not reach {description}; readiness={:?}, io_task={:?}",
            self.control.readiness(),
            self.snapshot().io_task
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

async fn save_session(harness: &mut RemoteAppHarness) -> shoop_app::ApplicationFileOutput {
    let previous_task = harness.snapshot().io_task.as_ref().map(|task| task.id);
    harness.dispatch(AppIntent::RequestSaveSession);
    harness
        .drive_until("completed session save", |snapshot| {
            snapshot.io_task.as_ref().is_some_and(|task| {
                task.kind == IoTaskKind::SaveSession
                    && task.status == IoTaskStatus::Completed
                    && Some(task.id) != previous_task
            })
        })
        .await;
    harness
        .runtime()
        .take_file_output()
        .expect("saved session output")
}

async fn load_session(harness: &mut RemoteAppHarness, output: shoop_app::ApplicationFileOutput) {
    let previous_task = harness.snapshot().io_task.as_ref().map(|task| task.id);
    harness.dispatch(AppIntent::LoadSessionBytes {
        name: output.suggested_name,
        bytes: output.bytes,
    });
    harness
        .drive_until("completed session load", |snapshot| {
            snapshot.io_task.as_ref().is_some_and(|task| {
                task.kind == IoTaskKind::LoadSession
                    && task.status == IoTaskStatus::Completed
                    && Some(task.id) != previous_task
            })
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
        harness.snapshot().status.audio_driver,
        shoop_app_api::AudioDriverState::Dummy
    );
    harness.shutdown().await;
}

#[shoop_wasm_test_support::shoop_test(
    wasm_only = "requires the production WebAssembly Worker runtime"
)]
async fn remote_loop_duplication_copies_content_and_controls() {
    let mut harness = RemoteAppHarness::start().await;
    let track = add_audio_track(&mut harness).await;
    let source = track.loops[0].id;
    let target = track.loops[1].id;
    generate_click(&mut harness, source).await;
    harness.dispatch(AppIntent::Loop {
        track_id: track.id,
        loop_id: source,
        action: LoopAction::GainChanged(0.42),
    });
    harness.dispatch(AppIntent::Loop {
        track_id: track.id,
        loop_id: source,
        action: LoopAction::BalanceChanged(-0.25),
    });
    harness.drive_steps(2).await;
    harness
        .drive_until("published source loop controls", |snapshot| {
            let source = &snapshot.tracks[1].loops[0];
            (source.gain - 0.42).abs() < f32::EPSILON
                && (source.balance + 0.25).abs() < f32::EPSILON
        })
        .await;
    harness.dispatch(AppIntent::Loop {
        track_id: track.id,
        loop_id: source,
        action: LoopAction::DuplicateTo(target),
    });
    harness
        .drive_until("completed asynchronous loop duplication", |snapshot| {
            let target = &snapshot.tracks[1].loops[1];
            !target.empty
                && (target.gain - 0.42).abs() < f32::EPSILON
                && (target.balance + 0.25).abs() < f32::EPSILON
        })
        .await;
    let snapshot = harness.snapshot();
    let source_state = &snapshot.tracks[1].loops[0];
    let target_state = &snapshot.tracks[1].loops[1];
    assert_eq!(source_state.id, source);
    assert_eq!(target_state.id, target);
    assert_eq!(target_state.length_frames, source_state.length_frames);
    assert_eq!(target_state.gain, source_state.gain);
    assert_eq!(target_state.balance, source_state.balance);
    harness.shutdown().await;
}

#[shoop_wasm_test_support::shoop_test(
    wasm_only = "requires the production WebAssembly Worker runtime"
)]
async fn remote_peak_publication_resets_after_silence() {
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
        harness
            .process_quantum(&[vec![0.25; QUANTUM], vec![0.5; QUANTUM]], 2)
            .await;
    }
    harness
        .drive_until(
            "published loud loop and track peaks on both channels",
            |snapshot| {
                snapshot.tracks[1].loops[0].peak_left_db > -100.0
                    && snapshot.tracks[1].loops[0].peak_right_db > -100.0
                    && snapshot.tracks[1].controls.output_peak_left_db > -100.0
                    && snapshot.tracks[1].controls.output_peak_right_db > -100.0
                    && snapshot.tracks[1].controls.input_peak_left_db > -100.0
                    && snapshot.tracks[1].controls.input_peak_right_db > -100.0
            },
        )
        .await;
    let loud = harness.snapshot();
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
    harness
        .drive_until("published silent loop and track peaks", |snapshot| {
            snapshot.tracks[1].loops[0].peak_left_db <= -100.0
                && snapshot.tracks[1].controls.output_peak_left_db <= -100.0
        })
        .await;
    let silent = harness.snapshot();
    let loud_loop = &loud.tracks[1].loops[0];
    let loud_track = &loud.tracks[1].controls;
    let silent_loop = &silent.tracks[1].loops[0];
    let silent_track = &silent.tracks[1].controls;
    for (name, loud_peak, silent_peak) in [
        (
            "loop left",
            loud_loop.peak_left_db,
            silent_loop.peak_left_db,
        ),
        (
            "loop right",
            loud_loop.peak_right_db,
            silent_loop.peak_right_db,
        ),
        (
            "track output left",
            loud_track.output_peak_left_db,
            silent_track.output_peak_left_db,
        ),
        (
            "track output right",
            loud_track.output_peak_right_db,
            silent_track.output_peak_right_db,
        ),
        (
            "track input left",
            loud_track.input_peak_left_db,
            silent_track.input_peak_left_db,
        ),
        (
            "track input right",
            loud_track.input_peak_right_db,
            silent_track.input_peak_right_db,
        ),
    ] {
        assert!(
            loud_peak > -100.0,
            "expected loud {name} peak, got {loud_peak}"
        );
        assert!(
            silent_peak <= -100.0,
            "{name} peak did not reset after silence: loud={loud_peak}, silent={silent_peak}"
        );
    }
    harness.shutdown().await;
}

#[shoop_wasm_test_support::shoop_test(
    wasm_only = "requires the production WebAssembly Worker runtime"
)]
async fn remote_session_round_trips_track_controls() {
    let mut harness = RemoteAppHarness::start().await;
    let track = add_audio_track(&mut harness).await;
    harness.dispatch(AppIntent::Track {
        track_id: track.id,
        action: TrackAction::NameChanged("Saved remote track".to_owned()),
    });
    harness.dispatch(AppIntent::Track {
        track_id: track.id,
        action: TrackAction::OutputGainChanged(-7.0),
    });
    harness
        .drive_until("published session controls", |snapshot| {
            snapshot.tracks[1].name == "Saved remote track"
                && (snapshot.tracks[1].controls.output_gain_db + 7.0).abs() < f32::EPSILON
        })
        .await;
    let saved = save_session(&mut harness).await;
    harness.dispatch(AppIntent::Track {
        track_id: track.id,
        action: TrackAction::NameChanged("Mutated track".to_owned()),
    });
    harness.dispatch(AppIntent::Track {
        track_id: track.id,
        action: TrackAction::OutputGainChanged(3.0),
    });
    load_session(&mut harness, saved).await;
    let loaded = harness.snapshot();
    assert_eq!(loaded.tracks[1].name, "Saved remote track");
    assert!((loaded.tracks[1].controls.output_gain_db + 7.0).abs() < f32::EPSILON);
    harness.shutdown().await;
}

#[shoop_wasm_test_support::shoop_test(
    wasm_only = "requires the production WebAssembly Worker runtime"
)]
async fn remote_loop_content_get_and_set_round_trips_through_session() {
    let mut harness = RemoteAppHarness::start().await;
    let track = add_audio_track(&mut harness).await;
    let loop_id = track.loops[0].id;
    generate_click(&mut harness, loop_id).await;
    let before = harness.snapshot().tracks[1].loops[0].clone();
    assert!(!before.empty);
    assert!(before.length_frames > 0);
    let saved = save_session(&mut harness).await;
    harness.dispatch(AppIntent::Global(GlobalControlAction::ClearRecordings {
        include_sync: false,
    }));
    harness
        .drive_until("cleared generated loop content", |snapshot| {
            snapshot.tracks[1].loops[0].empty
        })
        .await;
    load_session(&mut harness, saved).await;
    let loaded = &harness.snapshot().tracks[1].loops[0];
    assert!(!loaded.empty);
    assert_eq!(loaded.length_frames, before.length_frames);
    harness.shutdown().await;
}

#[shoop_wasm_test_support::shoop_test(
    wasm_only = "requires the production WebAssembly Worker runtime"
)]
async fn remote_builtin_synth_state_round_trips_through_session() {
    let mut harness = RemoteAppHarness::start().await;
    harness.dispatch(AppIntent::AddTrackWithTopology(TrackSpec {
        name: "Remote Built-in Synth".to_owned(),
        topology: TrackSpecTopology::DryWet {
            dry_audio_channels: 2,
            wet_audio_channels: 2,
            dry_midi: true,
            processor_type: TrackProcessorTypeId::new(TrackProcessorTypeId::OXISYNTH),
            default_playback_mode: DefaultPlaybackMode::DryThroughWet,
        },
        latency: Default::default(),
        creation_request_id: None,
    }));
    harness
        .drive_until("created Built-in Synth track", |snapshot| {
            snapshot.tracks.len() == 2 && snapshot.tracks[1].fx.is_some()
        })
        .await;
    let track_id = harness.snapshot().tracks[1].id;
    for control in [
        OxiSynthControl::SetReverbSend(0.4),
        OxiSynthControl::SetChorusSend(0.6),
        OxiSynthControl::AssignMidiCc(OxiSynthMidiCcAssignment {
            parameter: OxiSynthParameter::ReverbSend,
            channel: 3,
            controller: 74,
        }),
    ] {
        harness.dispatch(AppIntent::Track {
            track_id,
            action: TrackAction::OxiSynth(control),
        });
    }
    harness.dispatch(AppIntent::Track {
        track_id,
        action: TrackAction::FxVisibilityChanged(true),
    });
    harness
        .drive_until("published Built-in Synth state", |snapshot| {
            snapshot.tracks[1].fx.as_ref().is_some_and(|fx| {
                fx.visible
                    && matches!(
                        fx.editor.as_ref(),
                        Some(TrackProcessorEditorState::OxiSynth(editor))
                            if (editor.reverb_send - 0.4).abs() < f32::EPSILON
                                && (editor.chorus_send - 0.6).abs() < f32::EPSILON
                                && editor.midi_cc_assignments.len() == 1
                    )
            })
        })
        .await;
    let saved = save_session(&mut harness).await;
    load_session(&mut harness, saved).await;
    harness
        .drive_until("published loaded Built-in Synth state", |snapshot| {
            snapshot.tracks.get(1).is_some_and(|track| {
                track.default_playback_mode == DefaultPlaybackMode::DryThroughWet
                    && track.fx.as_ref().is_some_and(|fx| {
                        matches!(
                            fx.editor.as_ref(),
                            Some(TrackProcessorEditorState::OxiSynth(editor))
                                if (editor.reverb_send - 0.4).abs() < f32::EPSILON
                                    && (editor.chorus_send - 0.6).abs() < f32::EPSILON
                                    && editor.midi_cc_assignments.len() == 1
                        )
                    })
            })
        })
        .await;
    harness.shutdown().await;
}
