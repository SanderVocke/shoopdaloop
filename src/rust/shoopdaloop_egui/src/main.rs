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
use settings::SettingsManager;
#[cfg(all(not(target_arch = "wasm32"), feature = "native-fx"))]
use shoop_backend::configure_carla_hosting_mode;
#[cfg(not(target_arch = "wasm32"))]
use shoop_backend::NativeBackend;
#[cfg(target_arch = "wasm32")]
use shoop_egui::register_bundled_script_settings;
#[cfg(all(not(target_arch = "wasm32"), any(feature = "native-fx", test)))]
use shoop_egui::register_carla_settings;
#[cfg(not(target_arch = "wasm32"))]
use shoop_egui::register_script_settings;
#[cfg(not(target_arch = "wasm32"))]
use shoop_egui::{register_audio_settings, AudioDriverConfig};
use shoop_egui::{
    register_settings, AppIntent, AppSnapshot, AppWidget, ScriptKind, SettingsAction,
    SettingsRegistryBuilder,
};

#[cfg(target_arch = "wasm32")]
use shoop_app::CooperativeApplicationRuntime;
#[cfg(target_arch = "wasm32")]
mod browser_audio;
#[cfg(any(target_arch = "wasm32", test))]
mod browser_midi;
#[cfg(target_arch = "wasm32")]
mod browser_preview;
#[cfg(not(target_arch = "wasm32"))]
mod native_preview;
mod settings;
use shoop_app::StartupScript;
#[cfg(not(target_arch = "wasm32"))]
use shoop_app::{ApplicationHandle, ApplicationRuntime};

#[cfg(any(target_arch = "wasm32", test))]
const WEB_CANVAS_ID: &str = "shoop_canvas";
const UPDATE_INTERVAL: Duration = Duration::from_millis(16);

#[cfg(not(target_arch = "wasm32"))]
struct PendingAudioSettings {
    request_id: Option<u64>,
    config: AudioDriverConfig,
    draft: shoop_settings::SettingsDraft,
    saving: bool,
}

struct UnifiedApp {
    runtime: Runtime,
    widget: AppWidget,
    settings: SettingsManager,
    #[cfg(not(target_arch = "wasm32"))]
    pending_audio_settings: Option<PendingAudioSettings>,
    last_update: Instant,
    #[cfg(target_arch = "wasm32")]
    browser_self_test: BrowserSelfTest,
    #[cfg(target_arch = "wasm32")]
    browser_settings_test: BrowserSettingsSelfTest,
    #[cfg(target_arch = "wasm32")]
    pending_file_intents: Rc<RefCell<VecDeque<AppIntent>>>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_file_intent_tx: Sender<AppIntent>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_file_intent_rx: Receiver<AppIntent>,
}

#[cfg(all(not(target_arch = "wasm32"), not(test)))]
fn load_settings_manager(registry: shoop_egui::SettingsRegistry) -> SettingsManager {
    SettingsManager::load(registry, env!("CARGO_PKG_VERSION"))
}

#[cfg(all(not(target_arch = "wasm32"), test))]
fn load_settings_manager(registry: shoop_egui::SettingsRegistry) -> SettingsManager {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_TEST_SETTINGS: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_TEST_SETTINGS.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "shoopdaloop-egui-test-settings-{}-{id}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    SettingsManager::load_from_path(registry, env!("CARGO_PKG_VERSION"), path)
}

#[cfg(target_arch = "wasm32")]
fn load_settings_manager(registry: shoop_egui::SettingsRegistry) -> SettingsManager {
    SettingsManager::load(registry, env!("CARGO_PKG_VERSION"))
}

impl UnifiedApp {
    fn new() -> anyhow::Result<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        let (pending_file_intent_tx, pending_file_intent_rx) = mpsc::channel();
        let mut settings_builder = SettingsRegistryBuilder::default();
        register_settings(&mut settings_builder)?;
        #[cfg(not(target_arch = "wasm32"))]
        register_audio_settings(&mut settings_builder)?;
        #[cfg(all(not(target_arch = "wasm32"), feature = "native-fx"))]
        register_carla_settings(&mut settings_builder)?;
        #[cfg(not(target_arch = "wasm32"))]
        register_script_settings(&mut settings_builder)?;
        #[cfg(target_arch = "wasm32")]
        register_bundled_script_settings(&mut settings_builder)?;
        let settings_registry = settings_builder.finish();
        let settings = load_settings_manager(settings_registry.clone());
        let mut widget = AppWidget::new(std::sync::Arc::new(settings_registry));
        #[cfg(not(target_arch = "wasm32"))]
        let runtime = Runtime::new(&settings.active())?;
        #[cfg(target_arch = "wasm32")]
        let runtime = Runtime::new(&settings.active())?;
        widget.set_click_track_preview_available(runtime.audio_preview_available());
        Ok(Self {
            runtime,
            widget,
            settings,
            #[cfg(not(target_arch = "wasm32"))]
            pending_audio_settings: None,
            last_update: Instant::now(),
            #[cfg(target_arch = "wasm32")]
            browser_self_test: BrowserSelfTest::from_location(),
            #[cfg(target_arch = "wasm32")]
            browser_settings_test: BrowserSettingsSelfTest::from_location(),
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
    #[cfg(not(target_arch = "wasm32"))]
    fn handle_settings_action(&mut self, action: SettingsAction) {
        let result = match action {
            SettingsAction::Save(draft) => validate_script_draft(&draft)
                .and_then(|()| self.settings.request_save(draft).map_err(Into::into)),
            SettingsAction::RequestAudioDriverSwitch { config, draft } => {
                validate_script_draft(&draft).and_then(|()| {
                    self.runtime
                        .dispatch(AppIntent::RequestAudioDriverSwitch {
                            config: config.clone(),
                        })
                        .map_err(Into::into)
                        .map(|()| {
                            self.pending_audio_settings = Some(PendingAudioSettings {
                                request_id: None,
                                config,
                                draft,
                                saving: false,
                            });
                        })
                })
            }
            SettingsAction::RetryAudioDriverPersistence { request_id } => {
                let pending = self
                    .pending_audio_settings
                    .as_mut()
                    .filter(|pending| pending.request_id == Some(request_id))
                    .ok_or_else(|| anyhow::anyhow!("stale audio settings retry {request_id}"));
                pending.and_then(|pending| {
                    self.settings
                        .request_save(pending.draft.clone())
                        .map_err(Into::into)
                        .map(|()| pending.saving = true)
                })
            }
            SettingsAction::RecoverWithDefaults => {
                self.settings.request_recovery().map_err(Into::into)
            }
            SettingsAction::RequestAddUserScript => {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("Lua script", &["lua"])
                    .pick_file()
                else {
                    return;
                };
                let path = path.to_string_lossy().into_owned();
                read_user_script(&path).and_then(|_| {
                    self.widget
                        .add_user_script_path(path)
                        .map_err(anyhow::Error::msg)
                })
            }
            SettingsAction::RequestReloadUserScript { script_id } => {
                self.runtime.reload_user_script(script_id)
            }
        };
        if let Err(error) = result {
            self.settings.report_action_error(error.to_string());
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn handle_settings_action(&mut self, action: SettingsAction) {
        let result = match action {
            SettingsAction::Save(draft) => self.settings.request_save(draft),
            SettingsAction::RequestAudioDriverSwitch { .. }
            | SettingsAction::RetryAudioDriverPersistence { .. } => {
                self.settings.report_action_error(
                    "Native audio-driver switching is unavailable in browser builds",
                );
                return;
            }
            SettingsAction::RecoverWithDefaults => self.settings.request_recovery(),
            SettingsAction::RequestAddUserScript
            | SettingsAction::RequestReloadUserScript { .. } => {
                self.settings.report_action_error(
                    "Path-based user scripts are unavailable in browser builds",
                );
                return;
            }
        };
        if let Err(error) = result {
            self.settings.report_action_error(error.to_string());
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn reconcile_audio_settings(&mut self, snapshot: &AppSnapshot) {
        let Some(mut pending) = self.pending_audio_settings.take() else {
            return;
        };
        let switch = &snapshot.audio_drivers.switch;
        if pending.request_id.is_none()
            && switch
                .target
                .as_ref()
                .is_some_and(|target| target.configured == pending.config)
        {
            pending.request_id = Some(switch.request_id);
        }
        let Some(request_id) = pending.request_id else {
            self.pending_audio_settings = Some(pending);
            return;
        };
        if switch.request_id != request_id {
            self.pending_audio_settings = Some(pending);
            return;
        }
        if switch.status == shoop_egui::AudioDriverSwitchStatus::Completed {
            return;
        }
        if switch.status == shoop_egui::AudioDriverSwitchStatus::Persisting && !pending.saving {
            shoop_egui::set_selected_audio_driver(&mut pending.draft, pending.config.kind());
            match self.settings.request_save(pending.draft.clone()) {
                Ok(()) => pending.saving = true,
                Err(error) => {
                    let _ =
                        self.runtime
                            .dispatch(AppIntent::CompleteAudioDriverSwitchPersistence {
                                request_id,
                                success: false,
                                message: format!(
                            "The new audio driver is active, but settings were not saved: {error}"
                        ),
                            });
                }
            }
        }
        if pending.saving {
            let view = self.settings.view();
            if view.persistence == shoop_settings::SettingsPersistenceState::Saved
                && view.active.revision() > pending.draft.base_revision()
            {
                let _ = self
                    .runtime
                    .dispatch(AppIntent::CompleteAudioDriverSwitchPersistence {
                        request_id,
                        success: true,
                        message: "Audio driver switched and saved for the next launch".to_owned(),
                    });
                return;
            } else if view.persistence == shoop_settings::SettingsPersistenceState::Failed {
                let _ = self.runtime.dispatch(
                    AppIntent::CompleteAudioDriverSwitchPersistence {
                        request_id,
                        success: false,
                        message: "The new audio driver is active, but settings were not saved. Retry saving without switching again."
                            .to_owned(),
                    },
                );
                pending.saving = false;
            }
        }
        self.pending_audio_settings = Some(pending);
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        self.settings.poll();
        if let Err(error) = self
            .runtime
            .reconcile_script_settings(&self.settings.active())
        {
            self.settings.report_action_error(error.to_string());
        }
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_update);
        self.last_update = now;
        self.runtime.tick(elapsed);
        self.runtime.process_audio_previews();

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
        #[cfg(not(target_arch = "wasm32"))]
        self.reconcile_audio_settings(&snapshot);
        #[cfg(target_arch = "wasm32")]
        self.browser_self_test
            .update(&mut self.runtime, &snapshot, &mut self.widget);
        #[cfg(target_arch = "wasm32")]
        self.browser_settings_test
            .update(&mut self.settings, &mut self.widget, &self.runtime);
        let settings_state = self.settings.view();
        #[cfg(not(target_arch = "wasm32"))]
        let script_paths = Some(self.runtime.script_paths());
        #[cfg(target_arch = "wasm32")]
        let script_paths = None;
        let response = self
            .widget
            .show(ui, &snapshot, &settings_state, script_paths);
        for intent in response.app_actions {
            self.handle_ui_intent(intent);
        }
        for action in response.settings_actions {
            self.handle_settings_action(action);
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

const KEYBOARD_SCRIPT_FILENAME: &str = "keyboard.lua";
const APC_MINI_SCRIPT_FILENAME: &str = "akai_apc_mini_mk1.lua";

#[cfg(not(target_arch = "wasm32"))]
fn configured_startup_scripts(
    settings: &shoop_settings::SettingsSnapshot,
) -> anyhow::Result<(Vec<StartupScript>, Vec<String>, Vec<String>)> {
    let mut scripts = vec![
        StartupScript {
            name: KEYBOARD_SCRIPT_FILENAME.to_owned(),
            source: shoop_scripting::KEYBOARD_SCRIPT.to_owned(),
            kind: ScriptKind::Bundled,
            enabled: settings.get(shoop_egui::KEYBOARD_SCRIPT_ENABLED)?,
        },
        StartupScript {
            name: APC_MINI_SCRIPT_FILENAME.to_owned(),
            source: shoop_scripting::AKAI_APC_MINI_MK1_SCRIPT.to_owned(),
            kind: ScriptKind::Bundled,
            enabled: settings.get(shoop_egui::APC_MINI_SCRIPT_ENABLED)?,
        },
    ];
    let mut identities = vec![
        KEYBOARD_SCRIPT_FILENAME.to_owned(),
        APC_MINI_SCRIPT_FILENAME.to_owned(),
    ];
    let mut warnings = Vec::new();
    for configured in settings.get(shoop_egui::USER_SCRIPTS)?.0 {
        match read_user_script(&configured.value) {
            Ok((name, source)) => {
                identities.push(configured.value);
                scripts.push(StartupScript {
                    name,
                    source,
                    kind: ScriptKind::User,
                    enabled: configured.enabled,
                });
            }
            Err(error) => warnings.push(error.to_string()),
        }
    }
    Ok((scripts, identities, warnings))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_user_script(path: &str) -> anyhow::Result<(String, String)> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| anyhow::anyhow!("could not read {path}: {error}"))?;
    let name = Path::new(path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    shoop_scripting::LuaRuntime::new()?.check_syntax(&name, &source)?;
    Ok((name, source))
}

#[cfg(not(target_arch = "wasm32"))]
fn validate_script_draft(draft: &shoop_settings::SettingsDraft) -> anyhow::Result<()> {
    let mut paths = std::collections::BTreeSet::new();
    for configured in draft.get(shoop_egui::USER_SCRIPTS)?.0 {
        if configured.value.trim().is_empty() {
            anyhow::bail!("user script paths may not be empty");
        }
        if !paths.insert(configured.value.clone()) {
            anyhow::bail!("duplicate user script path {}", configured.value);
        }
        read_user_script(&configured.value)?;
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn associate_startup_script_paths(
    ids: &[Option<shoop_egui::ScriptId>],
    paths: Vec<String>,
) -> std::collections::BTreeMap<shoop_egui::ScriptId, String> {
    ids.iter()
        .copied()
        .zip(paths)
        .filter_map(|(script_id, path)| script_id.map(|script_id| (script_id, path)))
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
struct Runtime {
    _runtime: ApplicationRuntime,
    handle: ApplicationHandle,
    script_paths: std::collections::BTreeMap<shoop_egui::ScriptId, String>,
    pending_script_paths: std::collections::VecDeque<(String, ScriptKind, String)>,
    preview_player: native_preview::NativePreviewPlayer,
    applied_settings_revision: u64,
}

#[cfg(not(target_arch = "wasm32"))]
impl Runtime {
    fn new(settings: &shoop_settings::SettingsSnapshot) -> anyhow::Result<Self> {
        #[cfg(feature = "native-fx")]
        let carla_configuration_warning = {
            let (carla_hosting_mode, warning) =
                match shoop_egui::carla_hosting_mode_from_snapshot(settings) {
                    Ok(mode) => (mode, None),
                    Err(error) => (
                        shoop_settings::CarlaHostingMode::InProcess,
                        Some(format!(
                            "Could not use Carla hosting setting: {error}; using in_process"
                        )),
                    ),
                };
            configure_carla_hosting_mode(carla_hosting_mode);
            warning
        };
        let configured_driver = shoop_egui::selected_audio_driver(settings)
            .and_then(|kind| shoop_egui::audio_driver_config_from_snapshot(settings, kind));
        let (configured_driver, configuration_warning) = match configured_driver {
            Ok(configured) => (configured, None),
            Err(error) => (
                AudioDriverConfig::default(),
                Some(format!(
                    "Could not use preferred audio settings: {error}; using dummy / offline"
                )),
            ),
        };
        let (backend, backend_warning) = NativeBackend::new_with_fallback(configured_driver)?;
        let (startup_scripts, script_paths, mut warnings) = configured_startup_scripts(settings)?;
        warnings.extend(configuration_warning);
        #[cfg(feature = "native-fx")]
        warnings.extend(carla_configuration_warning);
        warnings.extend(backend_warning);
        let runtime = ApplicationRuntime::start_with_scripts(Box::new(backend), startup_scripts)?;
        let handle = runtime.handle();
        let preview_player = native_preview::NativePreviewPlayer::new(handle.clone())?;
        for warning in warnings {
            eprintln!("ShoopDaLoop settings: {warning}");
            handle.dispatch(AppIntent::ReportFileIoError {
                task_id: None,
                message: warning,
            })?;
        }
        let script_paths =
            associate_startup_script_paths(runtime.startup_script_ids(), script_paths);
        Ok(Self {
            _runtime: runtime,
            handle,
            script_paths,
            pending_script_paths: std::collections::VecDeque::new(),
            preview_player,
            applied_settings_revision: settings.revision(),
        })
    }

    fn tick(&mut self, _elapsed: Duration) {
        let snapshot = self.handle.snapshot();
        let mut mapped = self
            .script_paths
            .keys()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let mut retained = std::collections::VecDeque::new();
        while let Some((name, kind, path)) = self.pending_script_paths.pop_front() {
            if let Some(script) = snapshot.scripting.scripts.iter().find(|script| {
                script.kind == kind && script.name == name && !mapped.contains(&script.id)
            }) {
                self.script_paths.insert(script.id, path);
                mapped.insert(script.id);
            } else {
                retained.push_back((name, kind, path));
            }
        }
        self.pending_script_paths = retained;
    }

    fn reconcile_script_settings(
        &mut self,
        settings: &shoop_settings::SettingsSnapshot,
    ) -> anyhow::Result<()> {
        if settings.revision() == self.applied_settings_revision {
            return Ok(());
        }
        let (scripts, identities, warnings) = configured_startup_scripts(settings)?;
        for warning in warnings {
            self.handle.dispatch(AppIntent::ReportFileIoError {
                task_id: None,
                message: warning,
            })?;
        }
        let desired = identities
            .into_iter()
            .zip(scripts)
            .collect::<std::collections::BTreeMap<_, _>>();
        let current = self.script_paths.clone();
        for (script_id, identity) in current {
            if !desired.contains_key(&identity) {
                self.handle
                    .dispatch(AppIntent::ForgetScript { script_id })?;
                self.script_paths.remove(&script_id);
            }
        }
        let snapshot = self.handle.snapshot();
        for (identity, script) in desired {
            if let Some(script_id) = self
                .script_paths
                .iter()
                .find_map(|(id, path)| (path == &identity).then_some(*id))
            {
                if snapshot
                    .scripting
                    .scripts
                    .iter()
                    .find(|current| current.id == script_id)
                    .is_some_and(|current| current.enabled != script.enabled)
                {
                    self.handle.dispatch(AppIntent::SetScriptEnabled {
                        script_id,
                        enabled: script.enabled,
                    })?;
                }
            } else {
                self.handle.dispatch(AppIntent::AddScriptSource {
                    name: script.name.clone(),
                    source: script.source.into(),
                    kind: script.kind,
                    enabled: script.enabled,
                })?;
                self.pending_script_paths
                    .push_back((script.name, script.kind, identity));
            }
        }
        self.applied_settings_revision = settings.revision();
        Ok(())
    }

    fn reload_user_script(&mut self, script_id: shoop_egui::ScriptId) -> anyhow::Result<()> {
        let path = self
            .script_paths
            .get(&script_id)
            .ok_or_else(|| anyhow::anyhow!("no file path for script {script_id}"))?;
        let (_, source) = read_user_script(path)?;
        self.handle.dispatch(AppIntent::ReplaceScriptSource {
            script_id,
            source: source.into(),
        })?;
        Ok(())
    }

    fn script_paths(&self) -> &std::collections::BTreeMap<shoop_egui::ScriptId, String> {
        &self.script_paths
    }

    fn snapshot(&self) -> std::sync::Arc<AppSnapshot> {
        self.handle.snapshot()
    }

    fn dispatch(&mut self, intent: AppIntent) -> Result<(), shoop_app::DispatchError> {
        self.handle.dispatch(intent)
    }

    fn take_file_output(&self) -> Option<shoop_app::ApplicationFileOutput> {
        self.handle.take_file_output()
    }

    fn process_audio_previews(&mut self) {
        while let Some(preview) = self.handle.take_audio_preview() {
            self.preview_player.play(preview);
        }
    }

    fn audio_preview_available(&self) -> bool {
        native_preview::is_available()
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_startup_scripts(
    settings: &shoop_settings::SettingsSnapshot,
) -> anyhow::Result<Vec<StartupScript>> {
    Ok(vec![
        StartupScript {
            name: KEYBOARD_SCRIPT_FILENAME.to_owned(),
            source: shoop_scripting::KEYBOARD_SCRIPT.to_owned(),
            kind: ScriptKind::Bundled,
            enabled: settings.get(shoop_egui::KEYBOARD_SCRIPT_ENABLED)?,
        },
        StartupScript {
            name: APC_MINI_SCRIPT_FILENAME.to_owned(),
            source: shoop_scripting::AKAI_APC_MINI_MK1_SCRIPT.to_owned(),
            kind: ScriptKind::Bundled,
            enabled: settings.get(shoop_egui::APC_MINI_SCRIPT_ENABLED)?,
        },
    ])
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
    midi: browser_midi::BrowserMidiController,
    preview_player: browser_preview::BrowserPreviewPlayer,
    applied_settings_revision: u64,
}

#[cfg(target_arch = "wasm32")]
impl Runtime {
    fn new(settings: &shoop_settings::SettingsSnapshot) -> anyhow::Result<Self> {
        let offline = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .is_some_and(|search| search.contains("offline=1"));
        let startup_scripts = browser_startup_scripts(settings)?;
        let (midi, midi_service) = browser_midi::BrowserMidiController::new()?;
        if offline {
            let mut backend = shoop_backend::EngineBackend::new_dummy(48_000, 256)?;
            backend.remove_all_external_mock_ports();
            return Ok(Self {
                runtime: CooperativeApplicationRuntime::start_with_scripts_and_midi(
                    Box::new(backend),
                    startup_scripts,
                    midi_service,
                )?,
                mode: BrowserRuntimeMode::OfflineDummy,
                midi,
                preview_player: browser_preview::BrowserPreviewPlayer::default(),
                applied_settings_revision: settings.revision(),
            });
        }
        let (backend, transport) = browser_audio::WebAudioBackend::new(midi.hub());
        let controller = browser_audio::BrowserAudioController::new(transport)?;
        Ok(Self {
            runtime: CooperativeApplicationRuntime::start_with_scripts_and_midi(
                Box::new(backend),
                startup_scripts,
                midi_service,
            )?,
            mode: BrowserRuntimeMode::WebAudio(controller),
            midi,
            preview_player: browser_preview::BrowserPreviewPlayer::default(),
            applied_settings_revision: settings.revision(),
        })
    }

    fn tick(&mut self, elapsed: Duration) {
        self.runtime.tick(elapsed);
        self.midi.update_presentation();
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
        if let Some(element) = browser_status_element() {
            let _ = element.set_attribute("data-web-midi", &format!("{:?}", self.midi.state()));
            let _ = element.set_attribute(
                "data-web-midi-endpoints",
                &self.midi.endpoint_count().to_string(),
            );
            let (dropped, refused_track, refused_control) = self.midi.diagnostics();
            let _ = element.set_attribute("data-web-midi-track-drops", &dropped.to_string());
            let _ =
                element.set_attribute("data-web-midi-track-refusals", &refused_track.to_string());
            let _ = element.set_attribute(
                "data-web-midi-control-refusals",
                &refused_control.to_string(),
            );
        }
    }

    fn reconcile_script_settings(
        &mut self,
        settings: &shoop_settings::SettingsSnapshot,
    ) -> anyhow::Result<()> {
        if settings.revision() == self.applied_settings_revision {
            return Ok(());
        }
        let desired = [
            (
                KEYBOARD_SCRIPT_FILENAME,
                settings.get(shoop_egui::KEYBOARD_SCRIPT_ENABLED)?,
            ),
            (
                APC_MINI_SCRIPT_FILENAME,
                settings.get(shoop_egui::APC_MINI_SCRIPT_ENABLED)?,
            ),
        ];
        let snapshot = self.runtime.snapshot();
        for (name, enabled) in desired {
            let script = snapshot
                .scripting
                .scripts
                .iter()
                .find(|script| script.kind == ScriptKind::Bundled && script.name == name)
                .ok_or_else(|| anyhow::anyhow!("bundled browser script is missing: {name}"))?;
            if script.enabled != enabled {
                self.runtime.dispatch(AppIntent::SetScriptEnabled {
                    script_id: script.id,
                    enabled,
                })?;
            }
        }
        self.applied_settings_revision = settings.revision();
        Ok(())
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

    fn audio_preview_available(&self) -> bool {
        browser_preview::is_available()
    }

    fn process_audio_previews(&mut self) {
        if let Some(intent) = self.preview_player.update() {
            let _ = self.runtime.dispatch(intent);
        }
        while let Some(preview) = self.runtime.take_audio_preview() {
            let request_id = preview.request_id;
            let context = match &self.mode {
                BrowserRuntimeMode::WebAudio(controller) => controller.audio_context(),
                BrowserRuntimeMode::OfflineDummy => None,
            };
            if let Err(message) = self.preview_player.play(context, preview) {
                let _ = self.runtime.dispatch(AppIntent::CompleteClickTrackPreview {
                    request_id,
                    success: false,
                    message,
                });
            }
        }
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
    #[cfg(feature = "native-fx")]
    match shoop_backend::run_carla_worker_if_requested(std::env::args_os()) {
        Ok(true) => return Ok(()),
        Ok(false) => {}
        Err(error) => {
            eprintln!("Carla worker failed: {error:#}");
            std::process::exit(2);
        }
    }
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
enum BrowserSettingsSelfTest {
    Disabled,
    Write,
    Verify,
    Rejected,
    Invalid,
    SaveFailure,
    Unavailable,
    Complete,
    Failed,
}

#[cfg(target_arch = "wasm32")]
impl BrowserSettingsSelfTest {
    fn from_location() -> Self {
        let search = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .unwrap_or_default();
        if search.contains("settings-test=write") {
            Self::Write
        } else if search.contains("settings-test=verify") {
            Self::Verify
        } else if search.contains("settings-test=rejected") {
            Self::Rejected
        } else if search.contains("settings-test=invalid") {
            Self::Invalid
        } else if search.contains("settings-test=save-failure") {
            Self::SaveFailure
        } else if search.contains("settings-test=unavailable") {
            Self::Unavailable
        } else {
            Self::Disabled
        }
    }

    fn update(
        &mut self,
        settings: &mut SettingsManager,
        widget: &mut AppWidget,
        runtime: &Runtime,
    ) {
        let result = match *self {
            Self::Disabled | Self::Complete | Self::Failed => return,
            Self::Write => {
                let active = settings.view().active;
                let mut draft = shoop_egui::SettingsDraft::from_snapshot(&active);
                draft.set(shoop_egui::DEFAULT_NEW_TRACK_AUDIO_CHANNELS, 6);
                draft.set(shoop_egui::DEFAULT_NEW_TRACK_MIDI, true);
                draft.set(shoop_egui::KEYBOARD_SCRIPT_ENABLED, false);
                draft.set(shoop_egui::APC_MINI_SCRIPT_ENABLED, true);
                settings.request_save(draft).map(|()| "written")
            }
            Self::Verify => {
                verify_browser_settings(settings, widget, runtime, 6, true, false, true)
                    .map(|()| "passed")
            }
            Self::Rejected => {
                let view = settings.view();
                if !view.recovery_required {
                    Err(settings::SettingsManagerError::Storage(
                        "rejected settings did not require recovery".to_owned(),
                    ))
                } else {
                    verify_browser_settings(settings, widget, runtime, 2, false, true, false)
                        .map(|()| "rejected")
                }
            }
            Self::Invalid => {
                let view = settings.view();
                if view.recovery_required || view.diagnostics.is_empty() {
                    Err(settings::SettingsManagerError::Storage(
                        "invalid known value did not fall back with a diagnostic".to_owned(),
                    ))
                } else {
                    verify_browser_settings(settings, widget, runtime, 2, false, true, false)
                        .map(|()| "invalid")
                }
            }
            Self::SaveFailure => {
                let active = settings.view().active;
                let mut draft = shoop_egui::SettingsDraft::from_snapshot(&active);
                draft.set(shoop_egui::DEFAULT_NEW_TRACK_AUDIO_CHANNELS, 8);
                draft.set(shoop_egui::KEYBOARD_SCRIPT_ENABLED, false);
                draft.set(shoop_egui::APC_MINI_SCRIPT_ENABLED, true);
                match settings.request_save(draft) {
                    Err(settings::SettingsManagerError::Storage(_)) => {
                        let view = settings.view();
                        if view.active.revision() == active.revision()
                            && view.persistence == shoop_egui::SettingsPersistenceState::Failed
                        {
                            Ok("save-failed")
                        } else {
                            Err(settings::SettingsManagerError::Storage(
                                "failed browser save published settings".to_owned(),
                            ))
                        }
                    }
                    Err(error) => Err(error),
                    Ok(()) => Err(settings::SettingsManagerError::Storage(
                        "injected browser save unexpectedly succeeded".to_owned(),
                    )),
                }
            }
            Self::Unavailable => {
                let view = settings.view();
                if !view.recovery_required || view.diagnostics.is_empty() {
                    Err(settings::SettingsManagerError::Storage(
                        "unavailable browser storage was not observable".to_owned(),
                    ))
                } else {
                    verify_browser_settings(settings, widget, runtime, 2, false, true, false)
                        .map(|()| "unavailable")
                }
            }
        };
        match result {
            Ok(status) => {
                let view = settings.view();
                let channels = view
                    .active
                    .get(shoop_egui::DEFAULT_NEW_TRACK_AUDIO_CHANNELS)
                    .unwrap_or_default();
                let midi = view
                    .active
                    .get(shoop_egui::DEFAULT_NEW_TRACK_MIDI)
                    .unwrap_or_default();
                set_browser_settings_test_status(status, channels, midi, view.recovery_required);
                *self = Self::Complete;
            }
            Err(error) => {
                set_browser_settings_test_status("failed", 0, false, false);
                settings.report_action_error(error.to_string());
                *self = Self::Failed;
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn verify_browser_settings(
    settings: &SettingsManager,
    widget: &mut AppWidget,
    runtime: &Runtime,
    expected_channels: u32,
    expected_midi: bool,
    expected_keyboard: bool,
    expected_apc: bool,
) -> Result<(), settings::SettingsManagerError> {
    let view = settings.view();
    let channels = view
        .active
        .get(shoop_egui::DEFAULT_NEW_TRACK_AUDIO_CHANNELS)
        .map_err(|error| settings::SettingsManagerError::Storage(error.to_string()))?;
    let midi = view
        .active
        .get(shoop_egui::DEFAULT_NEW_TRACK_MIDI)
        .map_err(|error| settings::SettingsManagerError::Storage(error.to_string()))?;
    let keyboard = view
        .active
        .get(shoop_egui::KEYBOARD_SCRIPT_ENABLED)
        .map_err(|error| settings::SettingsManagerError::Storage(error.to_string()))?;
    let apc = view
        .active
        .get(shoop_egui::APC_MINI_SCRIPT_ENABLED)
        .map_err(|error| settings::SettingsManagerError::Storage(error.to_string()))?;
    let snapshot = runtime.snapshot();
    let runtime_keyboard = snapshot
        .scripting
        .scripts
        .iter()
        .find(|script| script.name == KEYBOARD_SCRIPT_FILENAME)
        .is_some_and(|script| script.enabled);
    let runtime_apc = snapshot
        .scripting
        .scripts
        .iter()
        .find(|script| script.name == APC_MINI_SCRIPT_FILENAME)
        .is_some_and(|script| script.enabled);
    let dialog_defaults = widget.browser_settings_test_open_add_track(&view);
    let scripts_tab_opened = widget.browser_settings_test_open_scripts(&view);
    if !scripts_tab_opened
        || (channels, midi) != (expected_channels, expected_midi)
        || (keyboard, apc) != (expected_keyboard, expected_apc)
        || (runtime_keyboard, runtime_apc) != (expected_keyboard, expected_apc)
        || dialog_defaults != (expected_channels, expected_midi)
    {
        return Err(settings::SettingsManagerError::Storage(format!(
            "settings consumer mismatch: active ({channels}, {midi}, keyboard={keyboard}, apc={apc}), runtime scripts ({runtime_keyboard}, {runtime_apc}), dialog {dialog_defaults:?}"
        )));
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn set_browser_settings_test_status(status: &str, channels: u32, midi: bool, recovery: bool) {
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("runtime_status"))
    {
        let _ = element.set_attribute("data-settings-self-test", status);
        let _ = element.set_attribute("data-settings-channels", &channels.to_string());
        let _ = element.set_attribute("data-settings-midi", if midi { "true" } else { "false" });
        let _ = element.set_attribute(
            "data-settings-recovery",
            if recovery { "true" } else { "false" },
        );
    }
}

#[cfg(target_arch = "wasm32")]
const BROWSER_SESSION_SCRIPT_NAME: &str = "browser-self-test-session.lua";
#[cfg(target_arch = "wasm32")]
const BROWSER_SESSION_SCRIPT_SOURCE: &str =
    "local shoop_control = require('shoop_control'); shoop_control.register_keyboard_event_cb(function(_) end)";

#[cfg(target_arch = "wasm32")]
fn browser_unsupported_session_bytes(
    sample_rate: u32,
    carla: bool,
) -> Result<std::sync::Arc<[u8]>, String> {
    use shoop_session::{
        encode_session, FxChainDocument, FxChainTypeDocument, SessionBundle, SessionDocument,
        TrackControlsDocument, TrackDocument, TrackGroupDocument, TrackTopologyDocument,
    };

    let mut document = SessionDocument::empty(sample_rate);
    let (name, port_name_base, topology, fx_chain) = if carla {
        (
            "Unsupported browser Carla",
            "unsupported_browser_carla",
            TrackTopologyDocument::Carla {
                chain_type: FxChainTypeDocument::CarlaRack,
                audio_channels: 1,
                midi: true,
                dry_audio_channels: Some(1),
                wet_audio_channels: Some(1),
            },
            Some(FxChainDocument {
                id: 100,
                title: "Unavailable".to_owned(),
                chain_type: FxChainTypeDocument::CarlaRack,
                ports: Vec::new(),
                internal_state: "opaque browser rejection state".to_owned(),
            }),
        )
    } else {
        (
            "Unsupported browser External",
            "unsupported_browser_external",
            TrackTopologyDocument::DryWetExternal {
                dry_audio_channels: 1,
                wet_audio_channels: 1,
                dry_midi: true,
            },
            None,
        )
    };
    document.track_groups.push(TrackGroupDocument {
        name: "main".to_owned(),
        tracks: vec![TrackDocument {
            id: 99,
            name: name.to_owned(),
            port_name_base: port_name_base.to_owned(),
            is_sync: false,
            width: None,
            topology,
            controls: TrackControlsDocument::default(),
            loops: Vec::new(),
            ports: Vec::new(),
            fx_chain,
        }],
    });
    encode_session(&SessionBundle::new(document), "browser-capability-test")
        .map(std::sync::Arc::from)
        .map_err(|error| format!("could not encode browser capability fixture: {error}"))
}

#[cfg(target_arch = "wasm32")]
fn set_browser_web_midi_test_status(status: &str) {
    if let Some(element) = browser_status_element() {
        let _ = element.set_attribute("data-web-midi-self-test", status);
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_click_request(kind: shoop_egui::ClickTrackKind) -> shoop_egui::ClickTrackRequest {
    shoop_egui::ClickTrackRequest {
        kind,
        bpm: 1_000.0,
        click_count: 1,
        midi_note: 67,
        ..Default::default()
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BrowserSelfTest {
    Disabled,
    WaitForAudio,
    WaitForDryWetForm,
    WaitForWebMidi,
    WaitForWebMidiControlBeforeAudio,
    AddWebMidiTrack,
    WaitForWebMidiTrack,
    WaitForWebMidiTrackReady {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
        input_port: shoop_egui::PortId,
        output_port: shoop_egui::PortId,
        callbacks_before: u64,
    },
    WaitForWebMidiConnections {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
        input_port: shoop_egui::PortId,
        output_port: shoop_egui::PortId,
    },
    WaitForWebMidiControl {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
    },
    WaitForWebMidiInput {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
    },
    WaitForWebMidiStopped {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
    },
    WaitForWebMidiSessionLoad {
        callbacks_before: u64,
    },
    WaitForWebMidiSave {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
        callbacks_before: u64,
    },
    WaitForWebMidiPlayback {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
        callbacks_before: u64,
    },
    AddTrack,
    WaitForTrack,
    WaitForRecording,
    WaitForPianoPress {
        callbacks_before: u64,
    },
    WaitForPianoRelease {
        callbacks_before: u64,
    },
    WaitForStopped,
    WaitForDetails,
    WaitForPlaying,
    WaitForDisconnectedOutput {
        left: shoop_egui::PortId,
        right: shoop_egui::PortId,
        callbacks_before: u64,
    },
    WaitForReconnectedOutput {
        left: shoop_egui::PortId,
        right: shoop_egui::PortId,
        callbacks_before: u64,
    },
    SaveSession {
        callbacks_before: u64,
    },
    WaitForSessionSave {
        callbacks_before: u64,
    },
    WaitForSessionLoad {
        callbacks_before: u64,
    },
    SaveLoadedScriptSession {
        callbacks_before: u64,
    },
    WaitForLoadedScriptSave {
        callbacks_before: u64,
    },
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
    GenerateClickAudio,
    WaitForClickAudio {
        previous_task: shoop_egui::TaskId,
    },
    WaitForClickAudioSelection,
    WaitForClickAudioExport,
    PreviewClickAudio,
    WaitForClickPreview,
    GenerateClickMidi,
    WaitForClickMidi {
        previous_task: shoop_egui::TaskId,
    },
    WaitForClickMidiExport,
    RejectProcessedSession,
    RejectExternalSession,
    WaitForProcessedSessionRejection {
        audio_progress_before: u64,
        external: bool,
    },
    Complete,
    Failed,
}

#[cfg(target_arch = "wasm32")]
impl BrowserSelfTest {
    fn from_location() -> Self {
        let search = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .unwrap_or_default();
        if search.contains("web-midi-test=1") {
            set_browser_self_test_status("web-midi");
            set_browser_web_midi_test_status("awaiting-permission");
            Self::WaitForWebMidi
        } else if search.contains("self-test=1") {
            set_browser_self_test_status("awaiting-audio");
            Self::WaitForAudio
        } else {
            Self::Disabled
        }
    }

    fn update(&mut self, runtime: &mut Runtime, snapshot: &AppSnapshot, widget: &mut AppWidget) {
        let result = match *self {
            Self::Disabled | Self::Complete | Self::Failed => return,
            Self::WaitForAudio => {
                if !runtime.audio_running() {
                    return;
                }
                if !widget.browser_test_open_tiny_dry_wet_form(&snapshot.track_processors) {
                    return self.fail(
                        "browser dry/wet form did not expose the Tiny Synth/FX processor contract",
                    );
                }
                mark_browser_dry_wet_capability_check();
                Ok(Self::WaitForDryWetForm)
            }
            Self::WaitForDryWetForm => {
                if snapshot.track_processors.len() != 1
                    || snapshot.track_processors[0].id.as_str()
                        != shoop_egui::TrackProcessorTypeId::TINY_SYNTH_FX
                {
                    return self.fail("browser Tiny Synth/FX catalog changed unexpectedly");
                }
                widget.browser_test_close_add_track();
                widget.browser_test_open_global_connections();
                Ok(Self::AddTrack)
            }
            Self::WaitForWebMidi => {
                if runtime.midi.state() != browser_midi::BrowserMidiState::Running {
                    return;
                }
                let Some(apc) = snapshot
                    .scripting
                    .scripts
                    .iter()
                    .find(|script| script.name == APC_MINI_SCRIPT_FILENAME)
                else {
                    return self.fail("bundled APC script is missing");
                };
                runtime
                    .dispatch(AppIntent::SetScriptEnabled {
                        script_id: apc.id,
                        enabled: true,
                    })
                    .map(|()| Self::WaitForWebMidiControlBeforeAudio)
            }
            Self::WaitForWebMidiControlBeforeAudio => {
                let ready = snapshot
                    .scripting
                    .scripts
                    .iter()
                    .find(|script| script.name == APC_MINI_SCRIPT_FILENAME)
                    .is_some_and(|script| {
                        script.lifecycle == shoop_egui::ScriptLifecycle::Listening
                            && script.midi.connections == 2
                    });
                if !ready {
                    return;
                }
                set_browser_web_midi_test_status("control-ready-without-audio");
                if !runtime.audio_running() {
                    return;
                }
                widget.browser_test_open_global_connections();
                Ok(Self::AddWebMidiTrack)
            }
            Self::AddWebMidiTrack => runtime
                .dispatch(AppIntent::Global(shoop_egui::GlobalControlAction::SetSync(
                    false,
                )))
                .and_then(|()| {
                    runtime.dispatch(AppIntent::AddTrack(shoop_egui::DirectTrackSpec {
                        name: "Browser Web MIDI test".to_owned(),
                        audio_channels: 0,
                        midi: true,
                    }))
                })
                .and_then(|()| {
                    runtime.dispatch(AppIntent::AddTrackWithTopology(shoop_egui::TrackSpec {
                        name: "Browser Web MIDI Tiny".to_owned(),
                        topology: shoop_egui::TrackSpecTopology::DryWet {
                            dry_audio_channels: 0,
                            wet_audio_channels: 0,
                            dry_midi: true,
                            processor_type: shoop_egui::TrackProcessorTypeId::new(
                                shoop_egui::TrackProcessorTypeId::TINY_SYNTH_FX,
                            ),
                        },
                    }))
                })
                .map(|()| Self::WaitForWebMidiTrack),
            Self::WaitForWebMidiTrack => {
                let Some(track) = snapshot.tracks.iter().find(|track| !track.is_sync) else {
                    return;
                };
                let Some(loop_state) = track.loops.first() else {
                    return;
                };
                let input_port = snapshot.connections.application_ports.iter().find(|port| {
                    matches!(
                        port.owner,
                        shoop_egui::ApplicationPortOwner::Track { track_id, .. }
                            if track_id == track.id
                    ) && port.role == shoop_egui::PortRole::MidiInput
                });
                let output_port = snapshot.connections.application_ports.iter().find(|port| {
                    matches!(
                        port.owner,
                        shoop_egui::ApplicationPortOwner::Track { track_id, .. }
                            if track_id == track.id
                    ) && port.role == shoop_egui::PortRole::MidiOutput
                });
                let (Some(input_port), Some(output_port)) = (input_port, output_port) else {
                    return;
                };
                let Some(tiny) = snapshot.tracks.iter().find(|candidate| {
                    candidate.fx.as_ref().is_some_and(|fx| {
                        fx.processor_type.as_str()
                            == shoop_egui::TrackProcessorTypeId::TINY_SYNTH_FX
                    })
                }) else {
                    return;
                };
                let Some(tiny_midi_input) =
                    snapshot.connections.application_ports.iter().find(|port| {
                        matches!(
                            port.owner,
                            shoop_egui::ApplicationPortOwner::Track { track_id, .. }
                                if track_id == tiny.id
                        ) && port.role == shoop_egui::PortRole::MidiInput
                    })
                else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::SetPortConnected {
                        port_id: tiny_midi_input.id,
                        host_port_id: shoop_egui::HostPortId::new("webmidi:source:test-input"),
                        connected: true,
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::Track {
                            track_id: tiny.id,
                            action: shoop_egui::TrackAction::InputMonitoringChanged(true),
                        })
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::Track {
                            track_id: tiny.id,
                            action: shoop_egui::TrackAction::TinySynthFx(
                                shoop_egui::TinySynthFxControl::SelectPreset("pad".to_owned()),
                            ),
                        })
                    })
                    .map(|()| Self::WaitForWebMidiTrackReady {
                        track_id: track.id,
                        loop_id: loop_state.id,
                        input_port: input_port.id,
                        output_port: output_port.id,
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::WaitForWebMidiTrackReady {
                track_id,
                loop_id,
                input_port,
                output_port,
                callbacks_before,
            } => {
                if snapshot.status.callback_count <= callbacks_before.saturating_add(10) {
                    return;
                }
                runtime
                    .dispatch(AppIntent::SetPortConnected {
                        port_id: input_port,
                        host_port_id: shoop_egui::HostPortId::new("webmidi:source:test-input"),
                        connected: true,
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::SetPortConnected {
                            port_id: output_port,
                            host_port_id: shoop_egui::HostPortId::new("webmidi:sink:test-output"),
                            connected: true,
                        })
                    })
                    .map(|()| Self::WaitForWebMidiConnections {
                        track_id,
                        loop_id,
                        input_port,
                        output_port,
                    })
            }
            Self::WaitForWebMidiConnections {
                track_id,
                loop_id,
                input_port,
                output_port,
            } => {
                let track_input_connected =
                    snapshot.connections.confirmed_links.iter().any(|link| {
                        link.application_port_id == input_port
                            && link.host_port_id.as_str() == "webmidi:source:test-input"
                    });
                let track_output_connected =
                    snapshot.connections.confirmed_links.iter().any(|link| {
                        link.application_port_id == output_port
                            && link.host_port_id.as_str() == "webmidi:sink:test-output"
                    });
                let Some(apc) = snapshot
                    .scripting
                    .scripts
                    .iter()
                    .find(|script| script.name == APC_MINI_SCRIPT_FILENAME)
                else {
                    return;
                };
                let control_ports = snapshot
                    .connections
                    .application_ports
                    .iter()
                    .filter(|port| {
                        matches!(
                            port.owner,
                            shoop_egui::ApplicationPortOwner::LuaControl { script_id, .. }
                                if script_id == apc.id
                        )
                    })
                    .map(|port| port.id)
                    .collect::<Vec<_>>();
                let control_links = snapshot
                    .connections
                    .confirmed_links
                    .iter()
                    .filter(|link| control_ports.contains(&link.application_port_id))
                    .count();
                if !track_input_connected {
                    let _ = runtime.dispatch(AppIntent::SetPortConnected {
                        port_id: input_port,
                        host_port_id: shoop_egui::HostPortId::new("webmidi:source:test-input"),
                        connected: true,
                    });
                }
                if !track_output_connected {
                    let _ = runtime.dispatch(AppIntent::SetPortConnected {
                        port_id: output_port,
                        host_port_id: shoop_egui::HostPortId::new("webmidi:sink:test-output"),
                        connected: true,
                    });
                }
                if !track_input_connected
                    || !track_output_connected
                    || apc.lifecycle != shoop_egui::ScriptLifecycle::Listening
                    || control_ports.len() != 2
                    || control_links != 2
                {
                    return;
                }
                runtime
                    .dispatch(AppIntent::Track {
                        track_id,
                        action: shoop_egui::TrackAction::InputMonitoringChanged(true),
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::Loop {
                            track_id,
                            loop_id,
                            action: shoop_egui::LoopAction::IconClicked(Default::default()),
                        })
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::Loop {
                            track_id,
                            loop_id,
                            action: shoop_egui::LoopAction::RecordClicked,
                        })
                    })
                    .map(|()| {
                        set_browser_web_midi_test_status("awaiting-input");
                        Self::WaitForWebMidiControl { track_id, loop_id }
                    })
            }
            Self::WaitForWebMidiControl { track_id, loop_id } => {
                let Some(track) = snapshot.tracks.iter().find(|track| track.id == track_id) else {
                    return;
                };
                let Some(loop_state) = track.loops.iter().find(|loop_| loop_.id == loop_id) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Recording
                    || !snapshot.global_controls.solo
                {
                    return;
                }
                runtime
                    .dispatch(AppIntent::Loop {
                        track_id,
                        loop_id,
                        action: shoop_egui::LoopAction::StopClicked,
                    })
                    .map(|()| Self::WaitForWebMidiInput { track_id, loop_id })
            }
            Self::WaitForWebMidiInput { track_id, loop_id } => {
                let Some(track) = snapshot.tracks.iter().find(|track| track.id == track_id) else {
                    return;
                };
                let Some(loop_state) = track.loops.iter().find(|loop_| loop_.id == loop_id) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Stopped {
                    return;
                }
                runtime
                    .dispatch(AppIntent::RequestSaveSession)
                    .map(|()| Self::WaitForWebMidiStopped { track_id, loop_id })
            }
            Self::WaitForWebMidiStopped {
                track_id: _,
                loop_id: _,
            } => {
                let Some(output) = runtime.take_file_output() else {
                    return;
                };
                let bundle = match shoop_session::decode_session(&output.bytes) {
                    Ok(bundle) => bundle,
                    Err(error) => {
                        return self.fail(&format!("could not decode Web MIDI session: {error}"));
                    }
                };
                let recorded = bundle.media.values().any(|payload| {
                    matches!(
                        payload,
                        shoop_session::MediaPayload::Midi(midi)
                            if midi.events.iter().any(|event| event.data == [0x90, 83, 0x7f])
                    )
                });
                let routes = bundle
                    .document
                    .track_groups
                    .iter()
                    .flat_map(|group| &group.tracks)
                    .flat_map(|track| &track.ports)
                    .flat_map(|port| &port.external_connections)
                    .filter(|endpoint| endpoint.starts_with("webmidi:"))
                    .count();
                if !recorded || routes != 2 {
                    return self.fail("Web MIDI recording or persisted routes are missing");
                }
                runtime
                    .dispatch(AppIntent::LoadSessionBytes {
                        name: output.suggested_name,
                        bytes: output.bytes,
                    })
                    .map(|()| Self::WaitForWebMidiSessionLoad {
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::WaitForWebMidiSessionLoad { callbacks_before } => {
                if snapshot.io_task.as_ref().is_none_or(|task| {
                    task.kind != shoop_egui::IoTaskKind::LoadSession
                        || task.status != shoop_egui::IoTaskStatus::Completed
                }) {
                    return;
                }
                let Some(track) = snapshot
                    .tracks
                    .iter()
                    .find(|track| track.name == "Browser Web MIDI test")
                else {
                    return self.fail("loaded Web MIDI session lost its track");
                };
                let Some(loop_state) = track.loops.first() else {
                    return self.fail("loaded Web MIDI session lost its loop");
                };
                let track_ports = snapshot
                    .connections
                    .application_ports
                    .iter()
                    .filter(|port| {
                        matches!(
                            port.owner,
                            shoop_egui::ApplicationPortOwner::Track { track_id, .. }
                                if track_id == track.id
                        )
                    })
                    .map(|port| port.id)
                    .collect::<Vec<_>>();
                let restored_routes = snapshot
                    .connections
                    .confirmed_links
                    .iter()
                    .filter(|link| {
                        track_ports.contains(&link.application_port_id)
                            && link.host_port_id.as_str().starts_with("webmidi:")
                    })
                    .count();
                if restored_routes < 2 {
                    return;
                }
                if restored_routes > 2 {
                    return self.fail("loaded Web MIDI session restored duplicate routes");
                }
                if snapshot.status.callback_count <= callbacks_before {
                    return;
                }
                set_browser_web_midi_test_status("ready-for-playback");
                Ok(Self::WaitForWebMidiSave {
                    track_id: track.id,
                    loop_id: loop_state.id,
                    callbacks_before: snapshot.status.callback_count,
                })
            }
            Self::WaitForWebMidiSave {
                track_id,
                loop_id,
                callbacks_before: _,
            } => {
                let ready = browser_status_element().is_some_and(|element| {
                    element
                        .get_attribute("data-web-midi-playback-ready")
                        .as_deref()
                        == Some("true")
                });
                if !ready {
                    return;
                }
                runtime
                    .dispatch(AppIntent::Loop {
                        track_id,
                        loop_id,
                        action: shoop_egui::LoopAction::PlayClicked,
                    })
                    .map(|()| {
                        set_browser_web_midi_test_status("awaiting-playback-output");
                        Self::WaitForWebMidiPlayback {
                            track_id,
                            loop_id,
                            callbacks_before: snapshot.status.callback_count,
                        }
                    })
            }
            Self::WaitForWebMidiPlayback {
                track_id,
                loop_id,
                callbacks_before,
            } => {
                let playing = snapshot
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .and_then(|track| track.loops.iter().find(|loop_| loop_.id == loop_id))
                    .is_some_and(|loop_| loop_.mode == shoop_egui::LoopMode::Playing);
                if !playing
                    || snapshot.status.callback_count <= callbacks_before.saturating_add(100)
                {
                    return;
                }
                set_browser_web_midi_test_status("passed");
                mark_browser_self_test_nonzero_io();
                Ok(Self::Complete)
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
                .and_then(|()| {
                    runtime.dispatch(AppIntent::AddTrackWithTopology(shoop_egui::TrackSpec {
                        name: "Browser Tiny Synth/FX".to_owned(),
                        topology: shoop_egui::TrackSpecTopology::DryWet {
                            dry_audio_channels: 2,
                            wet_audio_channels: 2,
                            dry_midi: true,
                            processor_type: shoop_egui::TrackProcessorTypeId::new(
                                shoop_egui::TrackProcessorTypeId::TINY_SYNTH_FX,
                            ),
                        },
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
                let Some(tiny) = snapshot.tracks.iter().find(|track| {
                    track.fx.as_ref().is_some_and(|fx| {
                        fx.processor_type.as_str()
                            == shoop_egui::TrackProcessorTypeId::TINY_SYNTH_FX
                    })
                }) else {
                    return;
                };
                let tiny_controls = [
                    shoop_egui::TrackAction::TinySynthFx(
                        shoop_egui::TinySynthFxControl::SelectPreset("pad".to_owned()),
                    ),
                    shoop_egui::TrackAction::TinySynthFx(
                        shoop_egui::TinySynthFxControl::SetMasterGainDb(-12.0),
                    ),
                    shoop_egui::TrackAction::TinySynthFx(
                        shoop_egui::TinySynthFxControl::SetReverbEnabled(true),
                    ),
                    shoop_egui::TrackAction::TinySynthFx(
                        shoop_egui::TinySynthFxControl::SetReverbAmount(0.4),
                    ),
                    shoop_egui::TrackAction::TinySynthFx(
                        shoop_egui::TinySynthFxControl::SetDistortionEnabled(true),
                    ),
                    shoop_egui::TrackAction::TinySynthFx(
                        shoop_egui::TinySynthFxControl::SetDistortionDrive(7.0),
                    ),
                    shoop_egui::TrackAction::TinySynthFx(shoop_egui::TinySynthFxControl::Panic),
                    shoop_egui::TrackAction::FxVisibilityChanged(true),
                    shoop_egui::TrackAction::FxVisibilityChanged(false),
                ]
                .into_iter()
                .try_for_each(|action| {
                    runtime.dispatch(AppIntent::Track {
                        track_id: tiny.id,
                        action,
                    })
                });
                tiny_controls.and_then(|()| {
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
                })
            }
            Self::WaitForRecording => {
                let Some((_track, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Recording
                    || loop_state.empty
                    || browser_stress_enabled() && snapshot.status.callback_count < 1_500
                {
                    return;
                }
                runtime
                    .dispatch(AppIntent::Piano(shoop_egui::PianoAction::Press(
                        shoop_egui::MidiNote::new(65).unwrap(),
                    )))
                    .map(|()| Self::WaitForPianoPress {
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::WaitForPianoPress { callbacks_before } => {
                if snapshot.status.callback_count <= callbacks_before {
                    return;
                }
                runtime
                    .dispatch(AppIntent::Piano(shoop_egui::PianoAction::Release(
                        shoop_egui::MidiNote::new(65).unwrap(),
                    )))
                    .map(|()| Self::WaitForPianoRelease {
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::WaitForPianoRelease { callbacks_before } => {
                if snapshot.status.callback_count <= callbacks_before {
                    return;
                }
                let Some((track, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
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
                let Some((track, loop_state)) = first_main_loop(snapshot) else {
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
                mark_browser_self_test_nonzero_io();
                let mut outputs: Vec<_> = snapshot
                    .connections
                    .application_ports
                    .iter()
                    .filter(|port| {
                        matches!(
                            port.owner,
                            shoop_egui::ApplicationPortOwner::Track { track_id, .. }
                                if track_id == track.id
                        ) && port.role == shoop_egui::PortRole::AudioOutput
                    })
                    .map(|port| port.id)
                    .collect();
                outputs.sort();
                if outputs.len() < 2 {
                    return self.fail("browser stereo route ports are missing");
                }
                runtime
                    .dispatch(AppIntent::SetPortConnected {
                        port_id: outputs[0],
                        host_port_id: shoop_egui::HostPortId::new("webaudio:destination_1"),
                        connected: false,
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::SetPortConnected {
                            port_id: outputs[1],
                            host_port_id: shoop_egui::HostPortId::new("webaudio:destination_2"),
                            connected: false,
                        })
                    })
                    .map(|()| Self::WaitForDisconnectedOutput {
                        left: outputs[0],
                        right: outputs[1],
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::WaitForDisconnectedOutput {
                left,
                right,
                callbacks_before,
            } => {
                let disconnected = !snapshot.connections.confirmed_links.iter().any(|link| {
                    (link.application_port_id == left
                        && link.host_port_id.as_str() == "webaudio:destination_1")
                        || (link.application_port_id == right
                            && link.host_port_id.as_str() == "webaudio:destination_2")
                });
                if !disconnected
                    || snapshot.status.callback_count <= callbacks_before.saturating_add(5)
                    || snapshot.status.output_peak > 0.000_001
                {
                    return;
                }
                runtime
                    .dispatch(AppIntent::SetPortConnected {
                        port_id: left,
                        host_port_id: shoop_egui::HostPortId::new("webaudio:destination_1"),
                        connected: true,
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::SetPortConnected {
                            port_id: right,
                            host_port_id: shoop_egui::HostPortId::new("webaudio:destination_2"),
                            connected: true,
                        })
                    })
                    .map(|()| Self::WaitForReconnectedOutput {
                        left,
                        right,
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::WaitForReconnectedOutput {
                left,
                right,
                callbacks_before,
            } => {
                let left_connected = snapshot.connections.confirmed_links.iter().any(|link| {
                    link.application_port_id == left
                        && link.host_port_id.as_str() == "webaudio:destination_1"
                });
                let right_connected = snapshot.connections.confirmed_links.iter().any(|link| {
                    link.application_port_id == right
                        && link.host_port_id.as_str() == "webaudio:destination_2"
                });
                if !left_connected
                    || !right_connected
                    || snapshot.status.callback_count <= callbacks_before
                    || snapshot.status.output_peak <= 0.000_001
                {
                    return;
                }
                let Some(tiny) = snapshot.tracks.iter().find(|track| {
                    track.fx.as_ref().is_some_and(|fx| {
                        fx.processor_type.as_str()
                            == shoop_egui::TrackProcessorTypeId::TINY_SYNTH_FX
                    })
                }) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::Track {
                        track_id: tiny.id,
                        action: shoop_egui::TrackAction::InputMonitoringChanged(true),
                    })
                    .map(|()| Self::SaveSession {
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
                let mut bundle = match shoop_session::decode_session(&output.bytes) {
                    Ok(bundle) => bundle,
                    Err(error) => {
                        return self.fail(&format!("could not decode browser session: {error}"));
                    }
                };
                bundle.document.scripts.push(shoop_session::ScriptDocument {
                    id: 9_000_001,
                    name: BROWSER_SESSION_SCRIPT_NAME.to_owned(),
                    source: BROWSER_SESSION_SCRIPT_SOURCE.to_owned(),
                    enabled: true,
                });
                let bytes = match shoop_session::encode_session(&bundle, env!("CARGO_PKG_VERSION"))
                {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        return self.fail(&format!("could not encode browser session: {error}"));
                    }
                };
                runtime
                    .dispatch(AppIntent::LoadSessionBytes {
                        name: output.suggested_name,
                        bytes: bytes.into(),
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
                    != 3
                {
                    return self.fail("loaded browser session lost tracks");
                }
                let tiny_state = snapshot.tracks.iter().find_map(|track| {
                    let fx = track.fx.as_ref()?;
                    if fx.processor_type.as_str() != shoop_egui::TrackProcessorTypeId::TINY_SYNTH_FX
                    {
                        return None;
                    }
                    match fx.editor.as_ref()? {
                        shoop_egui::TrackProcessorEditorState::TinySynthFx(editor) => Some(editor),
                    }
                });
                let Some(tiny_state) = tiny_state else {
                    return;
                };
                if tiny_state.selected_preset_id.as_deref() != Some("pad")
                    || tiny_state.master_gain_db != -12.0
                    || !tiny_state.reverb_enabled
                    || tiny_state.reverb_amount != 0.4
                    || !tiny_state.distortion_enabled
                    || tiny_state.distortion_drive != 7.0
                {
                    return self.fail(&format!(
                        "loaded browser Tiny Synth/FX state changed: {tiny_state:?}"
                    ));
                }
                if !snapshot.scripting.scripts.iter().any(|script| {
                    script.kind == ScriptKind::Session
                        && script.name == BROWSER_SESSION_SCRIPT_NAME
                        && script.enabled
                        && script.lifecycle == shoop_egui::ScriptLifecycle::Listening
                }) {
                    return self.fail(&format!(
                        "loaded browser session lost its active Lua script: {:?}",
                        snapshot.scripting.scripts
                    ));
                }
                if snapshot.status.audio_driver != shoop_egui::AudioDriverState::Dummy
                    && snapshot.status.callback_count <= callbacks_before
                {
                    return self.fail("audio callbacks did not advance through session reload");
                }
                if browser_stress_enabled() {
                    // Ordinary hosted/direct-file workflows verify exact script resave.
                    // Avoid a duplicate capture here so the stress case remains focused on
                    // sustained render/capture and reload.
                    Ok(Self::PlayLoadedLoop)
                } else {
                    Ok(Self::SaveLoadedScriptSession {
                        callbacks_before: snapshot.status.callback_count,
                    })
                }
            }
            Self::SaveLoadedScriptSession { callbacks_before } => runtime
                .dispatch(AppIntent::RequestSaveSession)
                .map(|()| Self::WaitForLoadedScriptSave { callbacks_before }),
            Self::WaitForLoadedScriptSave { callbacks_before } => {
                if snapshot.status.audio_driver != shoop_egui::AudioDriverState::Dummy
                    && snapshot.status.callback_count <= callbacks_before
                {
                    return;
                }
                let Some(output) = runtime.take_file_output() else {
                    return;
                };
                let bundle = match shoop_session::decode_session(&output.bytes) {
                    Ok(bundle) => bundle,
                    Err(error) => {
                        return self.fail(&format!(
                            "could not decode browser script round trip: {error}"
                        ));
                    }
                };
                if !bundle.document.scripts.iter().any(|script| {
                    script.name == BROWSER_SESSION_SCRIPT_NAME
                        && script.source == BROWSER_SESSION_SCRIPT_SOURCE
                        && script.enabled
                }) {
                    return self.fail("browser session Lua source did not round trip exactly");
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
                    Ok(Self::RejectProcessedSession)
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
                if snapshot.io_task.as_ref().is_some_and(|task| {
                    task.kind == shoop_egui::IoTaskKind::ImportLoopMidi
                        && task.status == shoop_egui::IoTaskStatus::Failed
                }) {
                    return self.fail(&format!(
                        "browser loop MIDI import failed: {:?}",
                        snapshot.io_task
                    ));
                }
                if snapshot.io_task.as_ref().is_none_or(|task| {
                    task.kind != shoop_egui::IoTaskKind::ImportLoopMidi
                        || task.status != shoop_egui::IoTaskStatus::Completed
                }) {
                    return;
                }
                Ok(Self::GenerateClickAudio)
            }
            Self::GenerateClickAudio => {
                let Some((_, loop_state)) = first_mixed_main_loop(snapshot) else {
                    return;
                };
                if !widget.browser_test_open_click_track(snapshot, loop_state.id) {
                    self.fail("production click-track dialog did not open");
                    return;
                }
                let previous_task = snapshot
                    .io_task
                    .as_ref()
                    .map(|task| task.id)
                    .unwrap_or_default();
                runtime
                    .dispatch(AppIntent::GenerateClickTrack {
                        loop_id: loop_state.id,
                        request: browser_click_request(shoop_egui::ClickTrackKind::Audio),
                    })
                    .map(|()| Self::WaitForClickAudio { previous_task })
            }
            Self::WaitForClickAudio { previous_task } => {
                let Some(task) = &snapshot.io_task else {
                    return;
                };
                if task.id == previous_task
                    || task.kind != shoop_egui::IoTaskKind::GenerateClickTrack
                    || task.status != shoop_egui::IoTaskStatus::Completed
                {
                    return;
                }
                let Some((_, loop_state)) = first_mixed_main_loop(snapshot) else {
                    return;
                };
                let expected = u64::from(snapshot.status.sample_rate) * 3 / 50;
                if loop_state.length_frames != expected || loop_state.empty {
                    self.fail("generated browser audio click length/state is incorrect");
                    return;
                }
                runtime
                    .dispatch(AppIntent::RequestLoopAudioExport {
                        loop_id: loop_state.id,
                        format: shoop_egui::LoopAudioExportFormat::Exact,
                    })
                    .map(|()| Self::WaitForClickAudioSelection)
            }
            Self::WaitForClickAudioSelection => {
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
                    .map(|()| Self::WaitForClickAudioExport)
            }
            Self::WaitForClickAudioExport => {
                let Some(output) = runtime.take_file_output() else {
                    return;
                };
                let Ok(audio) = shoop_session::decode_loop_audio(&output.bytes) else {
                    self.fail("generated browser click audio did not decode");
                    return;
                };
                let expected = (u64::from(snapshot.status.sample_rate) * 3 / 50) as usize;
                if audio.channels.len() != 2
                    || audio
                        .channels
                        .iter()
                        .any(|channel| channel.samples.len() != expected)
                    || audio
                        .channels
                        .iter()
                        .any(|channel| !channel.samples.iter().any(|sample| *sample != 0.0))
                {
                    self.fail("generated browser click audio payload is incorrect");
                    return;
                }
                Ok(Self::PreviewClickAudio)
            }
            Self::PreviewClickAudio => {
                let Some((_, loop_state)) = first_mixed_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::PreviewClickTrack {
                        loop_id: loop_state.id,
                        request: browser_click_request(shoop_egui::ClickTrackKind::Audio),
                    })
                    .map(|()| Self::WaitForClickPreview)
            }
            Self::WaitForClickPreview => match snapshot.click_track.preview_status {
                shoop_egui::ClickTrackPreviewStatus::Completed => Ok(Self::GenerateClickMidi),
                shoop_egui::ClickTrackPreviewStatus::Failed => {
                    self.fail("browser click preview failed");
                    return;
                }
                _ => return,
            },
            Self::GenerateClickMidi => {
                let Some((_, loop_state)) = first_mixed_main_loop(snapshot) else {
                    return;
                };
                let previous_task = snapshot
                    .io_task
                    .as_ref()
                    .map(|task| task.id)
                    .unwrap_or_default();
                runtime
                    .dispatch(AppIntent::GenerateClickTrack {
                        loop_id: loop_state.id,
                        request: browser_click_request(shoop_egui::ClickTrackKind::Midi),
                    })
                    .map(|()| Self::WaitForClickMidi { previous_task })
            }
            Self::WaitForClickMidi { previous_task } => {
                let Some(task) = &snapshot.io_task else {
                    return;
                };
                if task.id == previous_task
                    || task.kind != shoop_egui::IoTaskKind::GenerateClickTrack
                    || task.status != shoop_egui::IoTaskStatus::Completed
                {
                    return;
                }
                let Some((_, loop_state)) = first_mixed_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::RequestLoopMidiExport {
                        loop_id: loop_state.id,
                        standard: false,
                    })
                    .map(|()| Self::WaitForClickMidiExport)
            }
            Self::WaitForClickMidiExport => {
                let Some(output) = runtime.take_file_output() else {
                    return;
                };
                let Ok(midi) = shoop_session::decode_exact_midi(&output.bytes) else {
                    self.fail("generated browser click MIDI did not decode");
                    return;
                };
                if midi.length_frames != u64::from(snapshot.status.sample_rate) * 3 / 50
                    || midi.events.len() != 2
                    || midi.events[0].data != [0x90, 67, 127]
                {
                    self.fail("generated browser click MIDI payload is incorrect");
                    return;
                }
                Ok(Self::RejectProcessedSession)
            }
            Self::RejectProcessedSession | Self::RejectExternalSession => {
                let external = matches!(*self, Self::RejectExternalSession);
                match browser_unsupported_session_bytes(snapshot.status.sample_rate, !external) {
                    Ok(bytes) => runtime
                        .dispatch(AppIntent::LoadSessionBytes {
                            name: "unsupported-processed.shoop".to_owned(),
                            bytes,
                        })
                        .map(|()| Self::WaitForProcessedSessionRejection {
                            audio_progress_before: snapshot
                                .status
                                .callback_count
                                .max(snapshot.status.processed_frames),
                            external,
                        }),
                    Err(error) => return self.fail(&error),
                }
            }
            Self::WaitForProcessedSessionRejection {
                audio_progress_before,
                external,
            } => {
                if snapshot.io_task.as_ref().is_none_or(|task| {
                    task.kind != shoop_egui::IoTaskKind::LoadSession
                        || task.status != shoop_egui::IoTaskStatus::Failed
                }) {
                    return;
                }
                if snapshot
                    .status
                    .callback_count
                    .max(snapshot.status.processed_frames)
                    <= audio_progress_before
                {
                    return;
                }
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return self.fail("processed-session rejection removed direct tracks");
                };
                if snapshot.status.audio_driver != shoop_egui::AudioDriverState::Dummy
                    && loop_state.empty
                {
                    return self.fail("processed-session rejection removed direct loop media");
                }
                if external {
                    Ok(Self::Complete)
                } else {
                    Ok(Self::RejectExternalSession)
                }
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
fn first_mixed_main_loop(
    snapshot: &AppSnapshot,
) -> Option<(&shoop_egui::TrackState, &shoop_egui::LoopState)> {
    snapshot
        .tracks
        .iter()
        .filter(|track| !track.is_sync)
        .find_map(|track| {
            track
                .loops
                .iter()
                .find(|loop_| loop_.has_audio && loop_.has_midi)
                .map(|loop_| (track, loop_))
        })
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
        if status == "awaiting-audio" {
            let _ = element.set_attribute("data-self-test-nonzero-io", "false");
        }
        if status == "passed" {
            element.set_text_content(Some("Web Audio non-zero I/O self-test passed"));
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn mark_browser_dry_wet_capability_check() {
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("runtime_status"))
    {
        let _ = element.set_attribute("data-dry-wet-form", "tiny-synth-fx");
    }
}

#[cfg(target_arch = "wasm32")]
fn mark_browser_self_test_nonzero_io() {
    if let Some(element) = browser_status_element() {
        let _ = element.set_attribute("data-self-test-nonzero-io", "true");
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
        let _ = element.set_attribute("data-web-midi", "AwaitingGesture");
        let _ = element.set_attribute(
            "data-application-ports",
            &snapshot.connections.application_ports.len().to_string(),
        );
        let _ = element.set_attribute(
            "data-host-ports",
            &snapshot.connections.host_ports.len().to_string(),
        );
        let _ = element.set_attribute(
            "data-confirmed-links",
            &snapshot.connections.confirmed_links.len().to_string(),
        );
        let _ = element.set_attribute(
            "data-selected-loops",
            &snapshot
                .tracks
                .iter()
                .flat_map(|track| &track.loops)
                .filter(|loop_| loop_.selected)
                .count()
                .to_string(),
        );
        let _ = element.set_attribute(
            "data-lua-control-ports",
            &snapshot
                .connections
                .application_ports
                .iter()
                .filter(|port| {
                    matches!(
                        port.owner,
                        shoop_egui::ApplicationPortOwner::LuaControl { .. }
                    )
                })
                .count()
                .to_string(),
        );
        let _ = element.set_attribute(
            "data-midi-host-ports",
            &snapshot
                .connections
                .host_ports
                .iter()
                .filter(|port| port.data_type == shoop_egui::PortDataType::Midi)
                .count()
                .to_string(),
        );
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
        ApplicationPortOwner, DirectTrackSpec, HostPortId, LoopAction, LoopMode, MidiNote,
        PianoAction, PortRole, SelectionModifiers, TrackAction,
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
    fn confirmed_driver_switch_is_saved_once_and_completed_after_persistence() {
        let mut app = UnifiedApp::new().unwrap();
        app.runtime.tick(Duration::ZERO);
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&app.settings.active());
        draft.set(shoop_egui::DUMMY_SAMPLE_RATE, 32_000);
        draft.set(shoop_egui::DUMMY_BUFFER_SIZE, 128);
        let config =
            shoop_egui::audio_driver_config_from_draft(&draft, shoop_egui::AudioDriverKind::Dummy)
                .unwrap();
        app.handle_settings_action(SettingsAction::RequestAudioDriverSwitch { config, draft });
        let deadline = Instant::now() + Duration::from_secs(3);
        let prepared = loop {
            app.runtime.tick(Duration::ZERO);
            let snapshot = app.runtime.snapshot();
            if snapshot.audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::AwaitingConfirmation
            {
                break snapshot;
            }
            assert!(Instant::now() < deadline, "driver preflight timed out");
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(
            prepared.audio_drivers.switch.status,
            shoop_egui::AudioDriverSwitchStatus::AwaitingConfirmation
        );
        let request_id = prepared.audio_drivers.switch.request_id;
        app.runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: true,
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            app.runtime.tick(Duration::ZERO);
            if app.runtime.snapshot().audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::Persisting
            {
                break;
            }
            assert!(Instant::now() < deadline, "driver switch timed out");
            thread::sleep(Duration::from_millis(5));
        }

        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            app.settings.poll();
            let snapshot = app.runtime.snapshot();
            app.reconcile_audio_settings(&snapshot);
            app.runtime.tick(Duration::ZERO);
            if app.runtime.snapshot().audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::Completed
            {
                break;
            }
            assert!(Instant::now() < deadline, "driver settings save timed out");
            thread::sleep(Duration::from_millis(5));
        }
        let active = app.settings.active();
        assert_eq!(active.get(shoop_egui::DUMMY_SAMPLE_RATE).unwrap(), 32_000);
        assert_eq!(
            active.get(shoop_egui::SELECTED_AUDIO_DRIVER).unwrap(),
            "dummy"
        );
        assert!(app.pending_audio_settings.is_none());
    }

    #[test]
    fn failed_driver_settings_save_retries_without_switching_backend_again() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("blocked");
        let path = blocker.join("settings.json");
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_audio_settings(&mut builder).unwrap();
        register_carla_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let manager = SettingsManager::load_from_path(builder.finish(), "test", path);
        std::fs::write(&blocker, b"not a directory").unwrap();

        let mut app = UnifiedApp::new().unwrap();
        app.settings = manager;
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&app.settings.active());
        draft.set(shoop_egui::DUMMY_SAMPLE_RATE, 32_000);
        draft.set(shoop_egui::DUMMY_BUFFER_SIZE, 128);
        let config =
            shoop_egui::audio_driver_config_from_draft(&draft, shoop_egui::AudioDriverKind::Dummy)
                .unwrap();
        app.handle_settings_action(SettingsAction::RequestAudioDriverSwitch { config, draft });
        let deadline = Instant::now() + Duration::from_secs(10);
        let request_id = loop {
            app.runtime.tick(Duration::ZERO);
            let snapshot = app.runtime.snapshot();
            if snapshot.audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::AwaitingConfirmation
            {
                break snapshot.audio_drivers.switch.request_id;
            }
            assert!(Instant::now() < deadline, "driver preflight timed out");
            thread::sleep(Duration::from_millis(5));
        };
        app.runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: true,
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            app.settings.poll();
            app.runtime.tick(Duration::ZERO);
            let snapshot = app.runtime.snapshot();
            app.reconcile_audio_settings(&snapshot);
            if app.runtime.snapshot().audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::Failed
            {
                break;
            }
            assert!(Instant::now() < deadline, "injected save failure timed out");
            thread::sleep(Duration::from_millis(5));
        }
        let failed = app.runtime.snapshot();
        assert_eq!(
            failed.audio_drivers.active.as_ref().unwrap().sample_rate,
            32_000
        );
        assert!(failed.audio_drivers.switch.persistence_retry_available);
        assert_eq!(failed.audio_drivers.switch.request_id, request_id);

        std::fs::remove_file(&blocker).unwrap();
        app.handle_settings_action(SettingsAction::RetryAudioDriverPersistence { request_id });
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            app.settings.poll();
            app.runtime.tick(Duration::ZERO);
            let snapshot = app.runtime.snapshot();
            app.reconcile_audio_settings(&snapshot);
            if app.runtime.snapshot().audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::Completed
            {
                break;
            }
            if Instant::now() >= deadline {
                let view = app.settings.view();
                let current = app.runtime.snapshot();
                let pending = app
                    .pending_audio_settings
                    .as_ref()
                    .map(|pending| (pending.request_id, pending.saving));
                panic!(
                    "driver save retry timed out: persistence={:?}, switch={:?}, pending={pending:?}",
                    view.persistence, current.audio_drivers.switch.status
                );
            }
            thread::sleep(Duration::from_millis(5));
        }
        let completed = app.runtime.snapshot();
        assert_eq!(completed.audio_drivers.switch.request_id, request_id);
        assert_eq!(
            completed.audio_drivers.active.as_ref().unwrap().sample_rate,
            32_000
        );
        assert_eq!(
            app.settings
                .active()
                .get(shoop_egui::DUMMY_SAMPLE_RATE)
                .unwrap(),
            32_000
        );
    }

    #[test]
    fn persisted_dummy_configuration_is_used_on_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_audio_settings(&mut builder).unwrap();
        register_carla_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut manager = SettingsManager::load_from_path(registry.clone(), "test", path.clone());
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&manager.active());
        draft.set(shoop_egui::DUMMY_SAMPLE_RATE, 32_000);
        draft.set(shoop_egui::DUMMY_BUFFER_SIZE, 64);
        shoop_egui::set_selected_audio_driver(&mut draft, shoop_egui::AudioDriverKind::Dummy);
        manager.request_save(draft).unwrap();
        wait_for_settings_save(&mut manager);
        assert_eq!(manager.active().revision(), 2);
        drop(manager);

        let restarted = SettingsManager::load_from_path(registry, "test-2", path);
        assert_eq!(
            restarted
                .active()
                .get(shoop_egui::DUMMY_SAMPLE_RATE)
                .unwrap(),
            32_000
        );
        let runtime = Runtime::new(&restarted.active()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot.status.sample_rate == 32_000 {
                assert_eq!(snapshot.status.buffer_size, 64);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "restarted driver state timed out"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[cfg(feature = "native-fx")]
    #[test]
    fn runtime_applies_carla_hosting_setting_before_backend_start() {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_audio_settings(&mut builder).unwrap();
        register_carla_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let settings = builder.finish().defaults(1);
        configure_carla_hosting_mode(shoop_settings::CarlaHostingMode::Subprocess);
        let runtime = Runtime::new(&settings).unwrap();
        assert_eq!(
            shoop_backend::configured_carla_hosting_mode(),
            shoop_settings::CarlaHostingMode::InProcess
        );
        drop(runtime);
    }

    #[cfg(feature = "native-fx")]
    #[test]
    fn carla_hosting_mode_persists_but_does_not_change_the_running_backend() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut builder = SettingsRegistryBuilder::default();
        register_carla_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut manager = SettingsManager::load_from_path(registry.clone(), "test", path.clone());
        configure_carla_hosting_mode(shoop_settings::CarlaHostingMode::InProcess);
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&manager.active());
        draft.set(shoop_egui::CARLA_HOSTING_MODE, "subprocess".to_owned());
        manager.request_save(draft).unwrap();
        wait_for_settings_save(&mut manager);
        assert_eq!(
            shoop_backend::configured_carla_hosting_mode(),
            shoop_settings::CarlaHostingMode::InProcess
        );
        drop(manager);

        let restarted = SettingsManager::load_from_path(registry, "test-2", path);
        assert_eq!(
            shoop_egui::carla_hosting_mode_from_snapshot(&restarted.active()).unwrap(),
            shoop_settings::CarlaHostingMode::Subprocess
        );
    }

    #[test]
    fn unavailable_saved_preference_falls_back_without_overwriting_settings() {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_audio_settings(&mut builder).unwrap();
        register_carla_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&registry.defaults(1));
        shoop_egui::set_selected_audio_driver(&mut draft, shoop_egui::AudioDriverKind::Jack);
        draft.set(shoop_egui::JACK_CLIENT_NAME, String::new());
        let document = registry
            .document_from_draft(
                &shoop_settings::EgSettingsDocument::empty("test"),
                &draft,
                "test",
            )
            .unwrap();
        let settings = registry.resolve(&document, 2).snapshot;
        let runtime = Runtime::new(&settings).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot.audio_drivers.active.is_some() {
                assert_eq!(
                    snapshot
                        .audio_drivers
                        .active
                        .as_ref()
                        .unwrap()
                        .configured
                        .kind(),
                    shoop_egui::AudioDriverKind::Dummy
                );
                break;
            }
            assert!(Instant::now() < deadline, "fallback driver state timed out");
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            settings.get(shoop_egui::SELECTED_AUDIO_DRIVER).unwrap(),
            "jack"
        );
    }

    #[test]
    fn startup_script_adapter_resolves_typed_bundles_files_and_missing_paths() {
        let directory = tempfile::tempdir().unwrap();
        let user_script = directory.path().join("user.lua");
        std::fs::write(&user_script, "print('user')").unwrap();
        let missing = directory.path().join("missing.lua");
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&registry.defaults(1));
        draft.set(
            shoop_egui::USER_SCRIPTS,
            shoop_settings::StringToggleList(vec![
                shoop_settings::StringToggle {
                    value: user_script.to_string_lossy().into_owned(),
                    enabled: false,
                },
                shoop_settings::StringToggle {
                    value: missing.to_string_lossy().into_owned(),
                    enabled: true,
                },
            ]),
        );
        let document = registry
            .document_from_draft(
                &shoop_settings::EgSettingsDocument::empty("test"),
                &draft,
                "test",
            )
            .unwrap();
        let settings = registry.resolve(&document, 2).snapshot;
        let (scripts, paths, warnings) = configured_startup_scripts(&settings).unwrap();
        assert_eq!(scripts.len(), 3);
        assert_eq!(paths.len(), 3);
        assert_eq!(scripts[0].kind, ScriptKind::Bundled);
        assert_eq!(scripts[0].source, shoop_scripting::KEYBOARD_SCRIPT);
        assert!(scripts[0].enabled);
        assert_eq!(scripts[1].kind, ScriptKind::Bundled);
        assert!(!scripts[1].enabled);
        assert_eq!(scripts[2].kind, ScriptKind::User);
        assert!(!scripts[2].enabled);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("missing.lua"));
        assert!(validate_script_draft(&draft).is_err());
    }

    #[test]
    fn committed_settings_reconcile_scripts_and_failed_save_leaves_runtime_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let script_path = directory.path().join("controller.lua");
        std::fs::write(&script_path, "print('controller')").unwrap();
        let settings_directory = directory.path().join("configuration");
        let settings_path = settings_directory.join("settings.json");
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut manager = SettingsManager::load_from_path(registry, "test", settings_path.clone());
        let mut runtime = Runtime::new(&manager.active()).unwrap();

        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&manager.active());
        draft.set(shoop_egui::KEYBOARD_SCRIPT_ENABLED, false);
        draft.set(shoop_egui::APC_MINI_SCRIPT_ENABLED, true);
        draft.set(
            shoop_egui::USER_SCRIPTS,
            shoop_settings::StringToggleList(vec![shoop_settings::StringToggle {
                value: script_path.to_string_lossy().into_owned(),
                enabled: true,
            }]),
        );
        validate_script_draft(&draft).unwrap();
        manager.request_save(draft).unwrap();
        wait_for_settings_save(&mut manager);
        runtime
            .reconcile_script_settings(&manager.active())
            .unwrap();
        let snapshot = wait_for_script_configuration(&mut runtime);
        assert_eq!(snapshot.scripting.scripts.len(), 3);

        let mut removal = shoop_settings::SettingsDraft::from_snapshot(&manager.active());
        removal.set(
            shoop_egui::USER_SCRIPTS,
            shoop_settings::StringToggleList::default(),
        );
        manager.request_save(removal).unwrap();
        wait_for_settings_save(&mut manager);
        runtime
            .reconcile_script_settings(&manager.active())
            .unwrap();
        let after_removal = wait_for_script_count(&mut runtime, 2);
        assert!(!after_removal
            .scripting
            .scripts
            .iter()
            .any(|script| script.name == "controller.lua"));

        let committed_revision = manager.active().revision();
        std::fs::remove_file(&settings_path).unwrap();
        std::fs::remove_dir(&settings_directory).unwrap();
        std::fs::write(&settings_directory, b"not a directory").unwrap();
        let mut failing = shoop_settings::SettingsDraft::from_snapshot(&manager.active());
        failing.set(shoop_egui::KEYBOARD_SCRIPT_ENABLED, true);
        manager.request_save(failing).unwrap();
        wait_for_settings_save(&mut manager);
        assert_eq!(manager.active().revision(), committed_revision);
        runtime
            .reconcile_script_settings(&manager.active())
            .unwrap();
        assert!(
            !runtime
                .snapshot()
                .scripting
                .scripts
                .iter()
                .find(|script| script.name == KEYBOARD_SCRIPT_FILENAME)
                .unwrap()
                .enabled
        );
    }

    fn wait_for_settings_save(manager: &mut SettingsManager) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while manager.view().persistence == shoop_settings::SettingsPersistenceState::Saving {
            manager.poll();
            assert!(Instant::now() < deadline, "settings save timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait_for_script_count(
        runtime: &mut Runtime,
        expected: usize,
    ) -> std::sync::Arc<AppSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            runtime.tick(Duration::from_millis(5));
            let snapshot = runtime.snapshot();
            if snapshot.scripting.scripts.len() == expected {
                return snapshot;
            }
            assert!(Instant::now() < deadline, "script reconciliation timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait_for_script_configuration(runtime: &mut Runtime) -> std::sync::Arc<AppSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            runtime.tick(Duration::from_millis(5));
            let snapshot = runtime.snapshot();
            let keyboard_disabled = snapshot
                .scripting
                .scripts
                .iter()
                .find(|script| script.name == KEYBOARD_SCRIPT_FILENAME)
                .is_some_and(|script| !script.enabled);
            let apc_enabled = snapshot
                .scripting
                .scripts
                .iter()
                .find(|script| script.name == APC_MINI_SCRIPT_FILENAME)
                .is_some_and(|script| script.enabled);
            let user_enabled = snapshot
                .scripting
                .scripts
                .iter()
                .any(|script| script.name == "controller.lua" && script.enabled);
            if snapshot.scripting.scripts.len() == 3
                && keyboard_disabled
                && apc_enabled
                && user_enabled
            {
                return snapshot;
            }
            assert!(Instant::now() < deadline, "script reconciliation timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn startup_path_association_preserves_rejected_slots_and_duplicate_names() {
        let first = shoop_egui::ScriptId::from_raw(11);
        let second = shoop_egui::ScriptId::from_raw(12);
        let paths = associate_startup_script_paths(
            &[None, Some(first), Some(second)],
            vec![
                "invalid.lua".to_owned(),
                "first.lua".to_owned(),
                "second.lua".to_owned(),
            ],
        );
        assert_eq!(paths.len(), 2);
        assert_eq!(paths.get(&first).map(String::as_str), Some("first.lua"));
        assert_eq!(paths.get(&second).map(String::as_str), Some("second.lua"));
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
            if snapshot.connections.application_ports.iter().any(|port| {
                matches!(
                    port.owner,
                    ApplicationPortOwner::Track { track_id, .. }
                        if track_id == snapshot.tracks[1].id
                ) && port.role == PortRole::MidiInput
                    && snapshot.connections.host_ports.iter().any(|host| {
                        host.data_type == port.data_type && host.direction != port.direction
                    })
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
                .application_ports
                .iter()
                .find(|port| {
                    matches!(
                        port.owner,
                        ApplicationPortOwner::Track { track_id: owner, .. }
                            if owner == track_id
                    ) && port.role == role
                })
                .unwrap()
                .id;
            (port_id, endpoint)
        })
        .collect();
        for (port_id, endpoint) in &connection_targets {
            app.runtime
                .dispatch(AppIntent::SetPortConnected {
                    port_id: *port_id,
                    host_port_id: HostPortId::new(*endpoint),
                    connected: true,
                })
                .unwrap();
        }
        let started_connections = Instant::now();
        loop {
            let snapshot = app.runtime.snapshot();
            let all_connected = connection_targets.iter().all(|(port_id, endpoint)| {
                snapshot.connections.confirmed_links.iter().any(|link| {
                    link.application_port_id == *port_id && link.host_port_id.as_str() == *endpoint
                }) && !snapshot.connections.pending_links.iter().any(|link| {
                    link.application_port_id == *port_id && link.host_port_id.as_str() == *endpoint
                })
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
                host_port_id: HostPortId::new(connection_targets[1].1),
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
        for monitored_track in [snapshot.tracks[1].id, snapshot.tracks[3].id] {
            app.runtime
                .dispatch(AppIntent::Track {
                    track_id: monitored_track,
                    action: TrackAction::InputMonitoringChanged(true),
                })
                .unwrap();
        }
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

        let piano_note = MidiNote::new(65).unwrap();
        app.runtime
            .dispatch(AppIntent::Piano(PianoAction::Press(piano_note)))
            .unwrap();
        thread::sleep(Duration::from_millis(20));
        app.runtime
            .dispatch(AppIntent::Piano(PianoAction::Release(piano_note)))
            .unwrap();
        thread::sleep(Duration::from_millis(30));
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
        let saved = shoop_session::decode_session(&output.bytes).unwrap();
        let saved_piano_loop = saved
            .document
            .track_groups
            .iter()
            .flat_map(|group| &group.tracks)
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.id == loop_id.raw())
            .unwrap();
        let recorded_midi = saved_piano_loop
            .channels
            .iter()
            .filter_map(|channel| channel.media_id.as_ref())
            .find_map(|id| match &saved.media[id] {
                shoop_session::MediaPayload::Midi(midi) => Some(midi),
                shoop_session::MediaPayload::Audio(_) => None,
            })
            .unwrap();
        assert!(recorded_midi
            .events
            .iter()
            .any(|event| event.data == [0x90, 65, 100]));
        assert!(recorded_midi
            .events
            .iter()
            .any(|event| event.data == [0x80, 65, 0]));

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

        let loaded = app.runtime.snapshot();
        let target = loaded
            .tracks
            .iter()
            .find(|track| track.name == "Native stereo + MIDI")
            .unwrap();
        let generated_track = target.id;
        let generated_loop = target.loops[1].id;
        let mut request = shoop_egui::ClickTrackRequest::default();
        request.bpm = 600.0;
        request.click_count = 2;
        let before_preview = target.loops[1].clone();
        app.runtime
            .dispatch(AppIntent::PreviewClickTrack {
                loop_id: generated_loop,
                request: request.clone(),
            })
            .unwrap();
        let preview_started = Instant::now();
        loop {
            app.runtime.process_audio_previews();
            let snapshot = app.runtime.snapshot();
            if matches!(
                snapshot.click_track.preview_status,
                shoop_egui::ClickTrackPreviewStatus::Completed
                    | shoop_egui::ClickTrackPreviewStatus::Failed
            ) {
                let after_preview = snapshot
                    .tracks
                    .iter()
                    .flat_map(|track| &track.loops)
                    .find(|loop_| loop_.id == generated_loop)
                    .unwrap();
                assert_eq!(after_preview.id, before_preview.id);
                assert_eq!(after_preview.length_frames, before_preview.length_frames);
                assert_eq!(after_preview.empty, before_preview.empty);
                break;
            }
            assert!(preview_started.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        }
        app.runtime
            .dispatch(AppIntent::GenerateClickTrack {
                loop_id: generated_loop,
                request: request.clone(),
            })
            .unwrap();
        wait_for_click_generation(&app, generated_loop, 9_600, None);
        let first_task = app.runtime.snapshot().io_task.as_ref().unwrap().id;
        request.kind = shoop_egui::ClickTrackKind::Midi;
        request.midi_note = 67;
        app.runtime
            .dispatch(AppIntent::GenerateClickTrack {
                loop_id: generated_loop,
                request,
            })
            .unwrap();
        wait_for_click_generation(&app, generated_loop, 9_600, Some(first_task));
        app.runtime
            .dispatch(AppIntent::Loop {
                track_id: generated_track,
                loop_id: generated_loop,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        wait_for_loop_mode(&app, generated_track, generated_loop, LoopMode::Playing);
        app.runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        let save_started = Instant::now();
        let generated_session = loop {
            if let Some(output) = app.runtime.take_file_output() {
                break output;
            }
            assert!(save_started.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        };
        let decoded = shoop_session::decode_session(&generated_session.bytes).unwrap();
        let saved_generated = decoded
            .document
            .track_groups
            .iter()
            .flat_map(|group| &group.tracks)
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.id == generated_loop.raw())
            .unwrap();
        assert_eq!(saved_generated.length_frames, 9_600);
        assert_eq!(
            saved_generated
                .channels
                .iter()
                .filter(|channel| channel.media_id.is_some())
                .count(),
            3
        );
        let generated_media = saved_generated
            .channels
            .iter()
            .filter_map(|channel| channel.media_id.as_ref())
            .map(|id| &decoded.media[id])
            .collect::<Vec<_>>();
        let audio = generated_media
            .iter()
            .filter_map(|payload| match payload {
                shoop_session::MediaPayload::Audio(audio) => Some(&audio.samples),
                shoop_session::MediaPayload::Midi(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(audio.len(), 2);
        assert_eq!(audio[0], audio[1]);
        assert!(audio[0].iter().any(|sample| *sample != 0.0));
        let midi = generated_media
            .iter()
            .find_map(|payload| match payload {
                shoop_session::MediaPayload::Midi(midi) => Some(midi),
                shoop_session::MediaPayload::Audio(_) => None,
            })
            .unwrap();
        assert_eq!(midi.length_frames, 9_600);
        assert_eq!(
            midi.events
                .iter()
                .map(|event| (event.frame, event.data.as_slice()))
                .collect::<Vec<_>>(),
            vec![
                (0, [0x90, 67, 127].as_slice()),
                (4_800, [0x80, 67, 127].as_slice()),
                (4_800, [0x90, 67, 127].as_slice()),
                (9_599, [0x80, 67, 127].as_slice()),
            ]
        );
        app.runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: generated_session.suggested_name,
                bytes: generated_session.bytes,
            })
            .unwrap();
        let reload_started = Instant::now();
        loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.io_task.as_ref().is_some_and(|task| {
                task.kind == shoop_egui::IoTaskKind::LoadSession
                    && task.status == shoop_egui::IoTaskStatus::Completed
            }) && snapshot
                .tracks
                .iter()
                .flat_map(|track| &track.loops)
                .find(|loop_| loop_.id == generated_loop)
                .is_some_and(|loop_| loop_.length_frames == 9_600 && !loop_.empty)
            {
                break;
            }
            assert!(reload_started.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait_for_click_generation(
        app: &UnifiedApp,
        loop_id: shoop_egui::LoopId,
        length: u64,
        previous_task: Option<shoop_egui::TaskId>,
    ) {
        let started = Instant::now();
        loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.io_task.as_ref().is_some_and(|task| {
                task.kind == shoop_egui::IoTaskKind::GenerateClickTrack
                    && task.status == shoop_egui::IoTaskStatus::Completed
                    && Some(task.id) != previous_task
            }) && snapshot
                .tracks
                .iter()
                .flat_map(|track| &track.loops)
                .find(|loop_| loop_.id == loop_id)
                .is_some_and(|loop_| loop_.length_frames == length && !loop_.empty)
            {
                return;
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
