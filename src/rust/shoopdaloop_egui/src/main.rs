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
#[cfg(not(target_arch = "wasm32"))]
use shoop_backend::EngineBackend;
#[cfg(not(target_arch = "wasm32"))]
use shoop_egui::register_script_settings;
#[cfg(not(target_arch = "wasm32"))]
use shoop_egui::ScriptKind;
use shoop_egui::{
    register_settings, AppIntent, AppSnapshot, AppWidget, SettingsAction, SettingsRegistryBuilder,
};

#[cfg(target_arch = "wasm32")]
use shoop_app::CooperativeApplicationRuntime;
#[cfg(target_arch = "wasm32")]
mod browser_audio;
mod settings;
#[cfg(not(target_arch = "wasm32"))]
use shoop_app::{ApplicationHandle, ApplicationRuntime, StartupScript};

#[cfg(any(target_arch = "wasm32", test))]
const WEB_CANVAS_ID: &str = "shoop_canvas";
const UPDATE_INTERVAL: Duration = Duration::from_millis(16);

struct UnifiedApp {
    runtime: Runtime,
    widget: AppWidget,
    settings: SettingsManager,
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
        register_script_settings(&mut settings_builder)?;
        let settings_registry = settings_builder.finish();
        let settings = load_settings_manager(settings_registry.clone());
        let widget = AppWidget::new(std::sync::Arc::new(settings_registry));
        #[cfg(not(target_arch = "wasm32"))]
        let runtime = Runtime::new(&settings.active())?;
        #[cfg(target_arch = "wasm32")]
        let runtime = Runtime::new()?;
        Ok(Self {
            runtime,
            widget,
            settings,
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
            SettingsAction::RecoverWithDefaults => self.settings.request_recovery(),
            SettingsAction::RequestAddUserScript
            | SettingsAction::RequestReloadUserScript { .. } => {
                self.settings
                    .report_action_error("Lua scripting is unavailable in browser builds");
                return;
            }
        };
        if let Err(error) = result {
            self.settings.report_action_error(error.to_string());
        }
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        self.settings.poll();
        #[cfg(not(target_arch = "wasm32"))]
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
        #[cfg(target_arch = "wasm32")]
        self.browser_settings_test
            .update(&mut self.settings, &mut self.widget);
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

#[cfg(not(target_arch = "wasm32"))]
const KEYBOARD_SCRIPT_FILENAME: &str = "keyboard.lua";
#[cfg(not(target_arch = "wasm32"))]
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
    applied_settings_revision: u64,
}

#[cfg(not(target_arch = "wasm32"))]
impl Runtime {
    fn new(settings: &shoop_settings::SettingsSnapshot) -> anyhow::Result<Self> {
        let backend = EngineBackend::new_dummy(48_000, 256)?;
        let (startup_scripts, script_paths, warnings) = configured_startup_scripts(settings)?;
        let runtime = ApplicationRuntime::start_with_scripts(Box::new(backend), startup_scripts)?;
        let handle = runtime.handle();
        for warning in warnings {
            eprintln!("ShoopDaLoop script settings: {warning}");
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

    fn update(&mut self, settings: &mut SettingsManager, widget: &mut AppWidget) {
        let result = match *self {
            Self::Disabled | Self::Complete | Self::Failed => return,
            Self::Write => {
                let active = settings.view().active;
                let mut draft = shoop_egui::SettingsDraft::from_snapshot(&active);
                draft.set(shoop_egui::DEFAULT_NEW_TRACK_AUDIO_CHANNELS, 6);
                draft.set(shoop_egui::DEFAULT_NEW_TRACK_MIDI, true);
                settings.request_save(draft).map(|()| "written")
            }
            Self::Verify => verify_browser_settings(settings, widget, 6, true).map(|()| "passed"),
            Self::Rejected => {
                let view = settings.view();
                if !view.recovery_required {
                    Err(settings::SettingsManagerError::Storage(
                        "rejected settings did not require recovery".to_owned(),
                    ))
                } else {
                    verify_browser_settings(settings, widget, 2, false).map(|()| "rejected")
                }
            }
            Self::Invalid => {
                let view = settings.view();
                if view.recovery_required || view.diagnostics.is_empty() {
                    Err(settings::SettingsManagerError::Storage(
                        "invalid known value did not fall back with a diagnostic".to_owned(),
                    ))
                } else {
                    verify_browser_settings(settings, widget, 2, false).map(|()| "invalid")
                }
            }
            Self::SaveFailure => {
                let active = settings.view().active;
                let mut draft = shoop_egui::SettingsDraft::from_snapshot(&active);
                draft.set(shoop_egui::DEFAULT_NEW_TRACK_AUDIO_CHANNELS, 8);
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
                    verify_browser_settings(settings, widget, 2, false).map(|()| "unavailable")
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
    expected_channels: u32,
    expected_midi: bool,
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
    let dialog_defaults = widget.browser_settings_test_open_add_track(&view);
    if (channels, midi) != (expected_channels, expected_midi)
        || dialog_defaults != (expected_channels, expected_midi)
    {
        return Err(settings::SettingsManagerError::Storage(format!(
            "settings consumer mismatch: active ({channels}, {midi}), dialog {dialog_defaults:?}"
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
