use std::time::Duration;

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::collections::VecDeque;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

#[cfg(not(target_arch = "wasm32"))]
use std::{
    io::Write,
    path::Path,
    sync::mpsc::{self, Receiver, Sender},
    time::Instant,
};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use eframe::egui;
#[cfg(not(target_arch = "wasm32"))]
use shoop_backend::EngineBackend;
use shoop_egui::{AppIntent, AppSnapshot, AppWidget};

#[cfg(target_arch = "wasm32")]
use shoop_app::CooperativeApplicationRuntime;
#[cfg(target_arch = "wasm32")]
mod browser_audio;
#[cfg(not(target_arch = "wasm32"))]
use shoop_app::{ApplicationHandle, ApplicationRuntime};

#[cfg(any(target_arch = "wasm32", test))]
const WEB_CANVAS_ID: &str = "shoop_canvas";
const UPDATE_INTERVAL: Duration = Duration::from_millis(16);

struct UnifiedApp {
    runtime: Runtime,
    widget: AppWidget,
    last_update: Instant,
    #[cfg(target_arch = "wasm32")]
    browser_self_test: BrowserSelfTest,
    #[cfg(target_arch = "wasm32")]
    pending_file_intents: Rc<RefCell<VecDeque<AppIntent>>>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_file_intent_tx: Sender<AppIntent>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_file_intent_rx: Receiver<AppIntent>,
}

impl UnifiedApp {
    fn new() -> anyhow::Result<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        let (pending_file_intent_tx, pending_file_intent_rx) = mpsc::channel();
        Ok(Self {
            runtime: Runtime::new()?,
            widget: AppWidget::default(),
            last_update: Instant::now(),
            #[cfg(target_arch = "wasm32")]
            browser_self_test: BrowserSelfTest::from_location(),
            #[cfg(target_arch = "wasm32")]
            pending_file_intents: Rc::new(RefCell::new(VecDeque::new())),
            #[cfg(not(target_arch = "wasm32"))]
            pending_file_intent_tx,
            #[cfg(not(target_arch = "wasm32"))]
            pending_file_intent_rx,
        })
    }
}

impl UnifiedApp {
    fn show(&mut self, ui: &mut egui::Ui) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_update);
        self.last_update = now;
        self.runtime.tick(elapsed);

        #[cfg(target_arch = "wasm32")]
        let pending: Vec<_> = self.pending_file_intents.borrow_mut().drain(..).collect();
        #[cfg(not(target_arch = "wasm32"))]
        let pending: Vec<_> = self.pending_file_intent_rx.try_iter().collect();
        for intent in pending {
            if let Err(error) = self.runtime.dispatch(intent) {
                eprintln!("could not dispatch file intent: {error}");
            }
        }
        let snapshot = self.runtime.snapshot();
        #[cfg(target_arch = "wasm32")]
        self.browser_self_test.update(&mut self.runtime, &snapshot);
        for intent in self.widget.show(ui, &snapshot) {
            self.handle_ui_intent(intent);
        }
        while let Some(output) = self.runtime.take_file_output() {
            #[cfg(not(target_arch = "wasm32"))]
            save_file_output(output, self.pending_file_intent_tx.clone());
            #[cfg(target_arch = "wasm32")]
            save_file_output(output, Rc::clone(&self.pending_file_intents));
        }
        if let Some(notification) = snapshot.notifications.last() {
            egui::Area::new(egui::Id::new("latest_notification"))
                .anchor(egui::Align2::CENTER_TOP, [0.0, 8.0])
                .show(ui.ctx(), |ui| {
                    ui.label(&notification.message);
                });
        }
        ui.ctx().request_repaint_after(UPDATE_INTERVAL);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_ui_intent(&mut self, intent: AppIntent) {
        let intent = match intent {
            AppIntent::RequestLoadSessionPicker => {
                let path = rfd::FileDialog::new()
                    .add_filter("Shoop session", &["shoop"])
                    .pick_file();
                if let Some(path) = path {
                    let sender = self.pending_file_intent_tx.clone();
                    std::thread::spawn(move || match std::fs::read(&path) {
                        Ok(bytes) => {
                            let _ = sender.send(AppIntent::LoadSessionBytes {
                                name: file_name(&path),
                                bytes: std::sync::Arc::from(bytes),
                            });
                        }
                        Err(error) => {
                            let _ = sender.send(AppIntent::ReportFileIoError {
                                task_id: None,
                                message: format!("Could not read {}: {error}", path.display()),
                            });
                        }
                    });
                }
                None
            }
            AppIntent::RequestLoopAudioImportPicker { loop_id } => {
                let path = rfd::FileDialog::new()
                    .add_filter("Loop audio", &["shoop-audio", "wav"])
                    .pick_file();
                if let Some(path) = path {
                    let sender = self.pending_file_intent_tx.clone();
                    std::thread::spawn(move || match std::fs::read(&path) {
                        Ok(bytes) => {
                            let _ = sender.send(AppIntent::ImportLoopAudioBytes {
                                loop_id,
                                name: file_name(&path),
                                bytes: std::sync::Arc::from(bytes),
                                update_loop_length: true,
                            });
                        }
                        Err(error) => {
                            let _ = sender.send(AppIntent::ReportFileIoError {
                                task_id: None,
                                message: format!("Could not read {}: {error}", path.display()),
                            });
                        }
                    });
                }
                None
            }
            AppIntent::RequestLoopMidiImportPicker { loop_id } => {
                let path = rfd::FileDialog::new()
                    .add_filter("Loop MIDI", &["shoop-midi", "mid"])
                    .pick_file();
                if let Some(path) = path {
                    let sender = self.pending_file_intent_tx.clone();
                    std::thread::spawn(move || match std::fs::read(&path) {
                        Ok(bytes) => {
                            let _ = sender.send(AppIntent::ImportLoopMidiBytes {
                                loop_id,
                                name: file_name(&path),
                                bytes: std::sync::Arc::from(bytes),
                                update_loop_length: true,
                            });
                        }
                        Err(error) => {
                            let _ = sender.send(AppIntent::ReportFileIoError {
                                task_id: None,
                                message: format!("Could not read {}: {error}", path.display()),
                            });
                        }
                    });
                }
                None
            }
            other => Some(other),
        };
        if let Some(intent) = intent {
            if let Err(error) = self.runtime.dispatch(intent) {
                eprintln!("could not dispatch GUI intent: {error}");
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn handle_ui_intent(&mut self, intent: AppIntent) {
        match intent {
            AppIntent::RequestLoadSessionPicker => {
                let pending = Rc::clone(&self.pending_file_intents);
                wasm_bindgen_futures::spawn_local(async move {
                    if let Some(file) = rfd::AsyncFileDialog::new()
                        .add_filter("Shoop session", &["shoop"])
                        .pick_file()
                        .await
                    {
                        let name = file.file_name();
                        let bytes = file.read().await;
                        pending.borrow_mut().push_back(AppIntent::LoadSessionBytes {
                            name,
                            bytes: std::sync::Arc::from(bytes),
                        });
                    }
                });
            }
            AppIntent::RequestLoopAudioImportPicker { loop_id } => {
                let pending = Rc::clone(&self.pending_file_intents);
                wasm_bindgen_futures::spawn_local(async move {
                    if let Some(file) = rfd::AsyncFileDialog::new()
                        .add_filter("Loop audio", &["shoop-audio", "wav"])
                        .pick_file()
                        .await
                    {
                        let name = file.file_name();
                        let bytes = file.read().await;
                        pending
                            .borrow_mut()
                            .push_back(AppIntent::ImportLoopAudioBytes {
                                loop_id,
                                name,
                                bytes: std::sync::Arc::from(bytes),
                                update_loop_length: true,
                            });
                    }
                });
            }
            AppIntent::RequestLoopMidiImportPicker { loop_id } => {
                let pending = Rc::clone(&self.pending_file_intents);
                wasm_bindgen_futures::spawn_local(async move {
                    if let Some(file) = rfd::AsyncFileDialog::new()
                        .add_filter("Loop MIDI", &["shoop-midi", "mid"])
                        .pick_file()
                        .await
                    {
                        let name = file.file_name();
                        let bytes = file.read().await;
                        pending
                            .borrow_mut()
                            .push_back(AppIntent::ImportLoopMidiBytes {
                                loop_id,
                                name,
                                bytes: std::sync::Arc::from(bytes),
                                update_loop_length: true,
                            });
                    }
                });
            }
            other => {
                if let Err(error) = self.runtime.dispatch(other) {
                    eprintln!("could not dispatch GUI intent: {error}");
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_file_output(output: shoop_app::ApplicationFileOutput, sender: Sender<AppIntent>) {
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(&output.suggested_name)
        .save_file()
    else {
        return;
    };
    std::thread::spawn(move || {
        if let Err(error) = atomic_replace(&path, &output.bytes, output.task_id.raw()) {
            let _ = sender.send(AppIntent::ReportFileIoError {
                task_id: Some(output.task_id),
                message: format!("Could not save {}: {error}", path.display()),
            });
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

#[cfg(not(target_arch = "wasm32"))]
fn atomic_replace(path: &Path, bytes: &[u8], task_id: u64) -> std::io::Result<()> {
    let extension = path.extension().unwrap_or_default().to_string_lossy();
    let temporary = path.with_extension(format!("{extension}.tmp-{task_id}"));
    let _ = std::fs::remove_file(&temporary);
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            std::fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(target_arch = "wasm32")]
fn save_file_output(
    output: shoop_app::ApplicationFileOutput,
    pending: Rc<RefCell<VecDeque<AppIntent>>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Some(file) = rfd::AsyncFileDialog::new()
            .set_file_name(&output.suggested_name)
            .save_file()
            .await
        {
            if let Err(error) = file.write(&output.bytes).await {
                pending
                    .borrow_mut()
                    .push_back(AppIntent::ReportFileIoError {
                        task_id: Some(output.task_id),
                        message: format!("Could not save browser file: {error}"),
                    });
            }
        }
    });
}

impl eframe::App for UnifiedApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct Runtime {
    _runtime: ApplicationRuntime,
    handle: ApplicationHandle,
}

#[cfg(not(target_arch = "wasm32"))]
impl Runtime {
    fn new() -> anyhow::Result<Self> {
        let backend = EngineBackend::new_dummy(48_000, 256)?;
        let runtime = ApplicationRuntime::start(Box::new(backend))?;
        let handle = runtime.handle();
        Ok(Self {
            _runtime: runtime,
            handle,
        })
    }

    fn tick(&mut self, _elapsed: Duration) {}

    fn snapshot(&self) -> std::sync::Arc<AppSnapshot> {
        self.handle.snapshot()
    }

    fn dispatch(&mut self, intent: AppIntent) -> Result<(), shoop_app::DispatchError> {
        self.handle.dispatch(intent)
    }

    fn take_file_output(&self) -> Option<shoop_app::ApplicationFileOutput> {
        self.handle.take_file_output()
    }
}

#[cfg(target_arch = "wasm32")]
enum BrowserRuntimeMode {
    WebAudio(browser_audio::BrowserAudioController),
    OfflineDummy,
}

#[cfg(target_arch = "wasm32")]
struct Runtime {
    runtime: CooperativeApplicationRuntime,
    mode: BrowserRuntimeMode,
}

#[cfg(target_arch = "wasm32")]
impl Runtime {
    fn new() -> anyhow::Result<Self> {
        let offline = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .is_some_and(|search| search.contains("offline=1"));
        if offline {
            let backend = shoop_backend::EngineBackend::new_dummy(48_000, 256)?;
            return Ok(Self {
                runtime: CooperativeApplicationRuntime::start(Box::new(backend))?,
                mode: BrowserRuntimeMode::OfflineDummy,
            });
        }
        let (backend, transport) = browser_audio::WebAudioBackend::new();
        let controller = browser_audio::BrowserAudioController::new(transport)?;
        Ok(Self {
            runtime: CooperativeApplicationRuntime::start(Box::new(backend))?,
            mode: BrowserRuntimeMode::WebAudio(controller),
        })
    }

    fn tick(&mut self, elapsed: Duration) {
        self.runtime.tick(elapsed);
        let snapshot = self.runtime.snapshot();
        let mut message = match &self.mode {
            BrowserRuntimeMode::WebAudio(controller) => {
                controller.update_presentation();
                format!("Browser audio: {:?}", controller.state())
            }
            BrowserRuntimeMode::OfflineDummy => "Explicit offline dummy engine".to_owned(),
        };
        if let Some(notification) = snapshot.notifications.first() {
            message.push_str(": ");
            message.push_str(&notification.message);
        }
        set_browser_status(&message, Some(&snapshot));
    }

    fn snapshot(&self) -> std::sync::Arc<AppSnapshot> {
        self.runtime.snapshot()
    }

    fn dispatch(&mut self, intent: AppIntent) -> Result<(), shoop_app::DispatchError> {
        self.runtime.dispatch(intent)
    }

    fn take_file_output(&self) -> Option<shoop_app::ApplicationFileOutput> {
        self.runtime.take_file_output()
    }

    fn audio_running(&self) -> bool {
        match &self.mode {
            BrowserRuntimeMode::WebAudio(controller) => {
                controller.state() == shoop_backend::BackendDriverState::Running
            }
            BrowserRuntimeMode::OfflineDummy => true,
        }
    }
}

fn create_app(
    context: &eframe::CreationContext<'_>,
) -> Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>> {
    shoop_egui::initialize(&context.egui_ctx);
    UnifiedApp::new()
        .map(|app| Box::new(app) as Box<dyn eframe::App>)
        .map_err(|error| error.into())
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ShoopDaLoop egui (dummy engine)")
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([360.0, 200.0]),
        ..Default::default()
    };
    eframe::run_native("ShoopDaLoop", options, Box::new(create_app))
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BrowserSelfTest {
    Disabled,
    WaitForAudio,
    AddTrack,
    WaitForTrack,
    WaitForRecording,
    WaitForStopped,
    WaitForDetails,
    WaitForPlaying,
    SaveSession { callbacks_before: u64 },
    WaitForSessionSave { callbacks_before: u64 },
    WaitForSessionLoad { callbacks_before: u64 },
    PlayLoadedLoop,
    WaitForLoadedPlayback,
    ExportLoopAudio,
    WaitForLoopAudioSelection,
    WaitForLoopAudioExport,
    WaitForLoopAudioMapping,
    WaitForLoopAudioImport,
    ExportLoopMidi,
    WaitForLoopMidiExport,
    WaitForLoopMidiImport,
    Complete,
    Failed,
}

#[cfg(target_arch = "wasm32")]
impl BrowserSelfTest {
    fn from_location() -> Self {
        let enabled = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .is_some_and(|search| search.contains("self-test=1"));
        if enabled {
            set_browser_self_test_status("awaiting-audio");
            Self::WaitForAudio
        } else {
            Self::Disabled
        }
    }

    fn update(&mut self, runtime: &mut Runtime, snapshot: &AppSnapshot) {
        let result = match *self {
            Self::Disabled | Self::Complete | Self::Failed => return,
            Self::WaitForAudio => {
                if !runtime.audio_running() {
                    return;
                }
                Ok(Self::AddTrack)
            }
            Self::AddTrack => runtime
                .dispatch(AppIntent::Global(shoop_egui::GlobalControlAction::SetSync(
                    false,
                )))
                .and_then(|()| {
                    runtime.dispatch(AppIntent::AddTrack(shoop_egui::DirectTrackSpec {
                        name: "Browser self-test stereo".to_owned(),
                        audio_channels: 2,
                        midi: true,
                    }))
                })
                .and_then(|()| {
                    runtime.dispatch(AppIntent::AddTrack(shoop_egui::DirectTrackSpec {
                        name: "Browser self-test mono".to_owned(),
                        audio_channels: 1,
                        midi: false,
                    }))
                })
                .map(|()| Self::WaitForTrack),
            Self::WaitForTrack => {
                let Some(track) = snapshot.tracks.iter().find(|track| !track.is_sync) else {
                    return;
                };
                let Some(loop_state) = track.loops.first() else {
                    return;
                };
                if snapshot.status.audio_driver == shoop_egui::AudioDriverState::Dummy {
                    Ok(Self::SaveSession {
                        callbacks_before: snapshot.status.callback_count,
                    })
                } else {
                    runtime
                        .dispatch(AppIntent::Track {
                            track_id: track.id,
                            action: shoop_egui::TrackAction::InputMonitoringChanged(true),
                        })
                        .and_then(|()| {
                            runtime.dispatch(AppIntent::Loop {
                                track_id: track.id,
                                loop_id: loop_state.id,
                                action: shoop_egui::LoopAction::IconClicked(Default::default()),
                            })
                        })
                        .and_then(|()| {
                            runtime.dispatch(AppIntent::Loop {
                                track_id: track.id,
                                loop_id: loop_state.id,
                                action: shoop_egui::LoopAction::RecordClicked,
                            })
                        })
                        .map(|()| Self::WaitForRecording)
                }
            }
            Self::WaitForRecording => {
                let Some((track, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Recording
                    || loop_state.empty
                    || browser_stress_enabled() && snapshot.status.callback_count < 1_500
                {
                    return;
                }
                runtime
                    .dispatch(AppIntent::Loop {
                        track_id: track.id,
                        loop_id: loop_state.id,
                        action: shoop_egui::LoopAction::StopClicked,
                    })
                    .map(|()| Self::WaitForStopped)
            }
            Self::WaitForStopped => {
                let Some((track, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Stopped {
                    return;
                }
                runtime
                    .dispatch(AppIntent::Loop {
                        track_id: track.id,
                        loop_id: loop_state.id,
                        action: shoop_egui::LoopAction::IconClicked(Default::default()),
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::Loop {
                            track_id: track.id,
                            loop_id: loop_state.id,
                            action: shoop_egui::LoopAction::IconClicked(Default::default()),
                        })
                    })
                    .map(|()| Self::WaitForDetails)
            }
            Self::WaitForDetails => {
                let Some((track, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                let waveform_ready = snapshot.details.as_ref().is_some_and(|details| {
                    details.loop_id == loop_state.id
                        && details.channels.first().is_some_and(|channel| {
                            !channel.samples.is_empty()
                                && channel
                                    .samples
                                    .iter()
                                    .any(|sample| sample.abs() > 0.000_001)
                        })
                });
                if !waveform_ready {
                    return;
                }
                runtime
                    .dispatch(AppIntent::Loop {
                        track_id: track.id,
                        loop_id: loop_state.id,
                        action: shoop_egui::LoopAction::PlayClicked,
                    })
                    .map(|()| Self::WaitForPlaying)
            }
            Self::WaitForPlaying => {
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Playing
                    || snapshot.status.output_peak <= 0.000_001
                    || snapshot.status.callback_count == 0
                    || snapshot
                        .details
                        .as_ref()
                        .is_none_or(|details| details.loop_id != loop_state.id)
                {
                    return;
                }
                Ok(Self::SaveSession {
                    callbacks_before: snapshot.status.callback_count,
                })
            }
            Self::SaveSession { callbacks_before } => runtime
                .dispatch(AppIntent::RequestSaveSession)
                .map(|()| Self::WaitForSessionSave { callbacks_before }),
            Self::WaitForSessionSave { callbacks_before } => {
                if snapshot.status.audio_driver != shoop_egui::AudioDriverState::Dummy
                    && snapshot.status.callback_count <= callbacks_before
                {
                    return;
                }
                let Some(output) = runtime.take_file_output() else {
                    return;
                };
                if !output.suggested_name.ends_with(".shoop") || output.bytes.is_empty() {
                    return self.fail("browser session output is invalid");
                }
                runtime
                    .dispatch(AppIntent::LoadSessionBytes {
                        name: output.suggested_name,
                        bytes: output.bytes,
                    })
                    .map(|()| Self::WaitForSessionLoad { callbacks_before })
            }
            Self::WaitForSessionLoad { callbacks_before } => {
                if snapshot.io_task.as_ref().is_none_or(|task| {
                    task.kind != shoop_egui::IoTaskKind::LoadSession
                        || task.status != shoop_egui::IoTaskStatus::Completed
                }) {
                    return;
                }
                if snapshot
                    .tracks
                    .iter()
                    .filter(|track| !track.is_sync)
                    .count()
                    != 2
                {
                    return self.fail("loaded browser session lost tracks");
                }
                if snapshot.status.audio_driver != shoop_egui::AudioDriverState::Dummy
                    && snapshot.status.callback_count <= callbacks_before
                {
                    return self.fail("audio callbacks did not advance through session reload");
                }
                if snapshot.status.audio_driver == shoop_egui::AudioDriverState::Dummy {
                    Ok(Self::ExportLoopAudio)
                } else {
                    Ok(Self::PlayLoadedLoop)
                }
            }
            Self::PlayLoadedLoop => {
                let Some((track, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::Loop {
                        track_id: track.id,
                        loop_id: loop_state.id,
                        action: shoop_egui::LoopAction::PlayClicked,
                    })
                    .map(|()| Self::WaitForLoadedPlayback)
            }
            Self::WaitForLoadedPlayback => {
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Playing
                    || snapshot.status.audio_driver != shoop_egui::AudioDriverState::Dummy
                        && snapshot.status.output_peak <= 0.000_001
                {
                    return;
                }
                if browser_stress_enabled() || browser_session_only_enabled() {
                    Ok(Self::Complete)
                } else {
                    Ok(Self::ExportLoopAudio)
                }
            }
            Self::ExportLoopAudio => {
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::RequestLoopAudioExport {
                        loop_id: loop_state.id,
                        format: shoop_egui::LoopAudioExportFormat::Exact,
                    })
                    .map(|()| Self::WaitForLoopAudioSelection)
            }
            Self::WaitForLoopAudioSelection => {
                let Some(task) = &snapshot.io_task else {
                    return;
                };
                let Some(selection) = &task.audio_channel_selection else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::ConfirmAudioChannelSelection {
                        task_id: task.id,
                        channels: selection.default_selection.clone(),
                    })
                    .map(|()| Self::WaitForLoopAudioExport)
            }
            Self::WaitForLoopAudioExport => {
                let Some(output) = runtime.take_file_output() else {
                    return;
                };
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::ImportLoopAudioBytes {
                        loop_id: loop_state.id,
                        name: output.suggested_name,
                        bytes: output.bytes,
                        update_loop_length: true,
                    })
                    .map(|()| Self::WaitForLoopAudioMapping)
            }
            Self::WaitForLoopAudioMapping => {
                let Some(task) = &snapshot.io_task else {
                    return;
                };
                let Some(mapping) = &task.audio_channel_mapping else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::ConfirmAudioChannelMapping {
                        task_id: task.id,
                        source_for_destination: mapping.default_mapping.clone(),
                    })
                    .map(|()| Self::WaitForLoopAudioImport)
            }
            Self::WaitForLoopAudioImport => {
                if snapshot.io_task.as_ref().is_none_or(|task| {
                    task.kind != shoop_egui::IoTaskKind::ImportLoopAudio
                        || task.status != shoop_egui::IoTaskStatus::Completed
                }) {
                    return;
                }
                Ok(Self::ExportLoopMidi)
            }
            Self::ExportLoopMidi => {
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::RequestLoopMidiExport {
                        loop_id: loop_state.id,
                        standard: false,
                    })
                    .map(|()| Self::WaitForLoopMidiExport)
            }
            Self::WaitForLoopMidiExport => {
                let Some(output) = runtime.take_file_output() else {
                    return;
                };
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::ImportLoopMidiBytes {
                        loop_id: loop_state.id,
                        name: output.suggested_name,
                        bytes: output.bytes,
                        update_loop_length: true,
                    })
                    .map(|()| Self::WaitForLoopMidiImport)
            }
            Self::WaitForLoopMidiImport => {
                if snapshot.io_task.as_ref().is_none_or(|task| {
                    task.kind != shoop_egui::IoTaskKind::ImportLoopMidi
                        || task.status != shoop_egui::IoTaskStatus::Completed
                }) {
                    return;
                }
                Ok(Self::Complete)
            }
        };

        match result {
            Ok(next) => {
                *self = next;
                if next == Self::Complete {
                    set_browser_self_test_status("passed");
                } else {
                    set_browser_self_test_status(&format!("{next:?}"));
                }
            }
            Err(error) => {
                *self = Self::Failed;
                set_browser_self_test_status("failed");
                log::error!("browser dummy-engine self-test dispatch failed: {error}");
            }
        }
    }

    fn fail(&mut self, message: &str) {
        *self = Self::Failed;
        set_browser_self_test_status("failed");
        if let Some(element) = browser_status_element() {
            let _ = element.set_attribute("data-self-test-error", message);
        }
        log::error!("browser self-test failed: {message}");
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_stress_enabled() -> bool {
    browser_location_has("stress=1")
}

#[cfg(target_arch = "wasm32")]
fn browser_session_only_enabled() -> bool {
    browser_location_has("session-only=1")
}

#[cfg(target_arch = "wasm32")]
fn browser_location_has(query: &str) -> bool {
    web_sys::window()
        .and_then(|window| window.location().search().ok())
        .is_some_and(|search| search.contains(query))
}

#[cfg(target_arch = "wasm32")]
fn first_main_loop(
    snapshot: &AppSnapshot,
) -> Option<(&shoop_egui::TrackState, &shoop_egui::LoopState)> {
    let track = snapshot.tracks.iter().find(|track| !track.is_sync)?;
    Some((track, track.loops.first()?))
}

#[cfg(target_arch = "wasm32")]
fn browser_status_element() -> Option<web_sys::Element> {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("runtime_status"))
}

#[cfg(target_arch = "wasm32")]
fn set_browser_self_test_status(status: &str) {
    if let Some(element) = browser_status_element() {
        let _ = element.set_attribute("data-self-test", status);
        if status == "passed" {
            element.set_text_content(Some("Web Audio non-zero I/O self-test passed"));
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn set_browser_status(message: &str, snapshot: Option<&AppSnapshot>) {
    let Some(element) = browser_status_element() else {
        return;
    };
    element.set_text_content(Some(message));
    if let Some(snapshot) = snapshot {
        let status = &snapshot.status;
        let _ = element.set_attribute("data-engine-revision", &snapshot.revision.to_string());
        let _ = element.set_attribute("data-driver-state", &format!("{:?}", status.audio_driver));
        let _ = element.set_attribute("data-callback-count", &status.callback_count.to_string());
        let _ = element.set_attribute(
            "data-processed-frames",
            &status.processed_frames.to_string(),
        );
        let _ = element.set_attribute("data-input-peak", &status.input_peak.to_string());
        let _ = element.set_attribute("data-output-peak", &status.output_peak.to_string());
        let _ = element.set_attribute("data-sample-rate", &status.sample_rate.to_string());
        let _ = element.set_attribute("data-render-quantum", &status.buffer_size.to_string());
        let _ = element.set_attribute("data-xruns", &status.xruns.to_string());
        let _ = element.set_attribute(
            "data-callback-budget-overruns",
            &status.callback_budget_overruns.to_string(),
        );
        let _ = element.set_attribute(
            "data-render-discontinuities",
            &status.render_discontinuities.to_string(),
        );
        let _ = element.set_attribute("data-memory-growths", &status.memory_growths.to_string());
        let _ = element.set_attribute(
            "data-command-overflows",
            &status.command_overflows.to_string(),
        );
        let _ = element.set_attribute(
            "data-storage-low-channels",
            &status.storage_low_channels.to_string(),
        );
        let _ = element.set_attribute(
            "data-storage-exhaustions",
            &status.storage_exhaustions.to_string(),
        );
        let _ = element.set_attribute("data-web-midi", "unavailable");
        if let Some(details) = &snapshot.details {
            let samples = details
                .channels
                .first()
                .map(|channel| channel.samples.as_ref())
                .unwrap_or(&[]);
            let peak = samples
                .iter()
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
            let _ = element.set_attribute("data-waveform-samples", &samples.len().to_string());
            let _ = element.set_attribute("data-waveform-peak", &peak.to_string());
            let _ = element.set_attribute("data-waveform-loading", &details.loading.to_string());
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast as _;

    eframe::WebLogger::init(log::LevelFilter::Debug).ok();
    wasm_bindgen_futures::spawn_local(async {
        let window = web_sys::window().expect("browser window is unavailable");
        let document = window.document().expect("browser document is unavailable");
        let canvas = document
            .get_element_by_id(WEB_CANVAS_ID)
            .expect("missing #shoop_canvas element")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("#shoop_canvas is not a canvas");

        match eframe::WebRunner::new()
            .start(canvas, eframe::WebOptions::default(), Box::new(create_app))
            .await
        {
            Ok(()) => set_browser_status("Awaiting browser audio enable action", None),
            Err(error) => {
                let message = format!("Browser application failed: {error:?}");
                set_browser_status(&message, None);
                log::error!("{message}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::thread;

    use shoop_egui::{
        DirectTrackSpec, LoopAction, LoopMode, PortRole, SelectionModifiers, TrackAction,
    };

    use super::*;

    #[test]
    fn web_shell_targets_the_application_canvas() {
        let html = include_str!("../index.html");
        assert!(html.contains("data-trunk"));
        assert!(html.contains(&format!("id=\"{WEB_CANVAS_ID}\"")));
        assert!(html.contains("Enable microphone audio"));
        assert!(html.contains("Enable output-only audio"));
        assert!(html.contains("audio_worklet.js"));
        assert!(html.contains("Roboto-Regular.ttf"));
        assert!(html.contains("Roboto-BoldItalic.ttf"));
    }

    #[test]
    fn native_atomic_replace_overwrites_and_cleans_up_failed_temporary_files() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("session.shoop");
        std::fs::write(&target, b"old").unwrap();
        atomic_replace(&target, b"new", 7).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert!(!directory.path().join("session.shoop.tmp-7").exists());

        let missing_target = directory.path().join("missing").join("session.shoop");
        assert!(atomic_replace(&missing_target, b"partial", 8).is_err());
        assert!(!missing_target.exists());
        assert!(!directory
            .path()
            .join("missing")
            .join("session.shoop.tmp-8")
            .exists());
    }

    #[test]
    fn unified_application_paints_at_minimum_and_common_sizes() {
        for size in [egui::vec2(360.0, 200.0), egui::vec2(900.0, 600.0)] {
            let context = egui::Context::default();
            shoop_egui::initialize(&context);
            let mut app = UnifiedApp::new().unwrap();
            let snapshot = app.runtime.snapshot();
            assert_eq!(snapshot.tracks.len(), 1);
            assert!(snapshot.tracks[0].is_sync);
            assert_eq!(snapshot.tracks[0].loops.len(), 1);
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| app.show(ui),
            );
            assert!(!output.shapes.is_empty());
        }
    }

    #[test]
    fn native_dummy_workflow_creates_records_and_controls_tracks_and_loops() {
        let mut app = UnifiedApp::new().unwrap();
        let track_specs = [
            ("Native stereo + MIDI", 2, true),
            ("Native mono", 1, false),
            ("Native MIDI", 0, true),
            ("Native disabled", 0, false),
            ("Native custom", 4, false),
        ];
        for (name, audio_channels, midi) in track_specs {
            app.runtime
                .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                    name: name.to_owned(),
                    audio_channels,
                    midi,
                }))
                .unwrap();
        }
        let started = Instant::now();
        let snapshot = loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.tracks.len() == track_specs.len() + 1 {
                break snapshot;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        };
        assert!(snapshot.tracks[1..]
            .iter()
            .all(|track| track.loops.len() == 8));
        assert!(snapshot.tracks[1].controls.output_stereo);
        assert!(!snapshot.tracks[3].controls.has_output_audio);
        assert!(!snapshot.tracks[4].controls.has_output);

        let snapshot = loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.connections.ports.iter().any(|port| {
                port.track_id == snapshot.tracks[1].id
                    && port.role == PortRole::MidiInput
                    && !port.candidates.is_empty()
            }) {
                break snapshot;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        };
        let connection_targets: Vec<_> = [
            (
                snapshot.tracks[0].id,
                PortRole::AudioInput,
                "system:capture_1",
            ),
            (
                snapshot.tracks[1].id,
                PortRole::AudioInput,
                "system:capture_2",
            ),
            (
                snapshot.tracks[1].id,
                PortRole::MidiInput,
                "controller:midi_out",
            ),
        ]
        .into_iter()
        .map(|(track_id, role, endpoint)| {
            let port_id = snapshot
                .connections
                .ports
                .iter()
                .find(|port| port.track_id == track_id && port.role == role)
                .unwrap()
                .id;
            (port_id, endpoint)
        })
        .collect();
        for (port_id, endpoint) in &connection_targets {
            app.runtime
                .dispatch(AppIntent::SetPortConnected {
                    port_id: *port_id,
                    external_port: (*endpoint).to_owned(),
                    connected: true,
                })
                .unwrap();
        }
        let started_connections = Instant::now();
        loop {
            let snapshot = app.runtime.snapshot();
            let all_connected = connection_targets.iter().all(|(port_id, endpoint)| {
                snapshot
                    .connections
                    .ports
                    .iter()
                    .find(|port| port.id == *port_id)
                    .and_then(|port| {
                        port.candidates
                            .iter()
                            .find(|candidate| candidate.full_name == *endpoint)
                    })
                    .is_some_and(|candidate| candidate.connected && candidate.pending.is_none())
            });
            if all_connected {
                break;
            }
            assert!(started_connections.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        }
        app.runtime
            .dispatch(AppIntent::SetPortConnected {
                port_id: connection_targets[1].0,
                external_port: connection_targets[1].1.to_owned(),
                connected: false,
            })
            .unwrap();

        let track_id = snapshot.tracks[1].id;
        let loop_id = snapshot.tracks[1].loops[0].id;
        app.runtime
            .dispatch(AppIntent::Global(shoop_egui::GlobalControlAction::SetSync(
                false,
            )))
            .unwrap();
        app.runtime
            .dispatch(AppIntent::Track {
                track_id,
                action: TrackAction::OutputGainChanged(-3.0),
            })
            .unwrap();
        app.runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::IconClicked(SelectionModifiers::default()),
            })
            .unwrap();
        app.runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::BalanceChanged(0.25),
            })
            .unwrap();
        app.runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::RecordClicked,
            })
            .unwrap();
        let started = Instant::now();
        loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.tracks[1].controls.output_gain_db == -3.0
                && snapshot.tracks[1].loops[0].balance == 0.25
                && snapshot.tracks[1].loops[0].mode == LoopMode::Recording
                && snapshot.details.is_some()
            {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        }

        thread::sleep(Duration::from_millis(50));
        app.runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::StopClicked,
            })
            .unwrap();
        wait_for_loop_mode(&app, track_id, loop_id, LoopMode::Stopped);
        app.runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        wait_for_loop_mode(&app, track_id, loop_id, LoopMode::Playing);
        let frames_before_save = app.runtime.snapshot().status.processed_frames;
        app.runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        let started = Instant::now();
        let output = loop {
            if let Some(output) = app.runtime.take_file_output() {
                break output;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        };
        let started = Instant::now();
        let after_save = loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.status.processed_frames > frames_before_save {
                break snapshot;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        };
        let saved_loop = after_save
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .and_then(|track| track.loops.iter().find(|loop_| loop_.id == loop_id))
            .unwrap();
        assert_eq!(saved_loop.mode, LoopMode::Playing);
        assert!(output.suggested_name.ends_with(".shoop"));

        app.runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: output.suggested_name,
                bytes: output.bytes,
            })
            .unwrap();
        let started = Instant::now();
        loop {
            let loaded = app.runtime.snapshot();
            if loaded.io_task.as_ref().is_some_and(|task| {
                task.kind == shoop_egui::IoTaskKind::LoadSession
                    && task.status == shoop_egui::IoTaskStatus::Completed
            }) {
                assert_eq!(loaded.tracks.len(), track_specs.len() + 1);
                assert!(loaded
                    .tracks
                    .iter()
                    .flat_map(|track| &track.loops)
                    .all(|loop_| {
                        loop_.mode == LoopMode::Stopped || loop_.mode == LoopMode::Unknown
                    }));
                assert!(loaded.tracks.iter().any(|track| {
                    track.name == "Native stereo + MIDI"
                        && (track.controls.output_gain_db + 3.0).abs() < 0.001
                }));
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait_for_loop_mode(
        app: &UnifiedApp,
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
        expected: LoopMode,
    ) {
        let started = Instant::now();
        loop {
            let snapshot = app.runtime.snapshot();
            if snapshot
                .tracks
                .iter()
                .find(|track| track.id == track_id)
                .and_then(|track| track.loops.iter().find(|loop_| loop_.id == loop_id))
                .is_some_and(|loop_| loop_.mode == expected)
            {
                return;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        }
    }
}
