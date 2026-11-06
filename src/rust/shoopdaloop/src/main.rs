#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

use std::time::Duration;

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::collections::VecDeque;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

#[cfg(not(target_arch = "wasm32"))]
use clap::Parser;
#[cfg(not(target_arch = "wasm32"))]
use std::{
    io::Write,
    path::Path,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    time::Instant,
};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use eframe::egui;
use settings::SettingsManager;
#[cfg(test)]
use shoop_egui::register_audio_settings;
#[cfg(target_arch = "wasm32")]
use shoop_egui::register_bundled_script_settings;
#[cfg(not(target_arch = "wasm32"))]
use shoop_egui::register_script_settings;
#[cfg(test)]
use shoop_egui::register_settings;
#[cfg(not(target_arch = "wasm32"))]
use shoop_egui::AudioDriverConfig;
use shoop_egui::{
    register_audio_settings_with_default_driver, register_settings_with_appearance_defaults,
    AppIntent, AppSnapshot, AppWidget, AudioDriverKind, ScriptKind, SettingsAction,
    SettingsRegistryBuilder, TrackDefaultSaveResult, UI_SCALE_FACTOR,
};
use shoop_egui::{TracingStatus, TracingStopped};

#[cfg(target_arch = "wasm32")]
use application::BrowserApplicationRuntime as CooperativeApplicationRuntime;
mod app_args;
mod application;
#[cfg(any(target_arch = "wasm32", test))]
mod browser_midi;
#[cfg(target_arch = "wasm32")]
mod browser_preview;
#[cfg(any(target_arch = "wasm32", test))]
mod browser_rt_audio;
#[cfg(target_arch = "wasm32")]
mod browser_rt_self_test;
#[cfg(not(target_arch = "wasm32"))]
mod engine_backend;
#[cfg(not(target_arch = "wasm32"))]
mod native_preview;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod native_recovery_tests;
mod persistence_io;
mod platform_input;
#[cfg(not(target_arch = "wasm32"))]
mod recovery;
#[cfg(not(target_arch = "wasm32"))]
mod session_probe;
mod settings;
use app_args::AppArgs;
#[cfg(not(target_arch = "wasm32"))]
use application::StartupScript;
#[cfg(not(target_arch = "wasm32"))]
use application::{ApplicationHandle, ApplicationRuntime};

#[cfg(any(target_arch = "wasm32", test))]
const WEB_CANVAS_ID: &str = "shoop_canvas";
#[cfg(not(target_arch = "wasm32"))]
const APPLICATION_ICON_PNG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../resources/iconset/icon.png"
));
const UPDATE_INTERVAL: Duration = Duration::from_millis(16);

#[cfg(not(target_arch = "wasm32"))]
fn application_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(APPLICATION_ICON_PNG)
        .expect("embedded application icon must be valid PNG")
}

#[cfg(not(target_arch = "wasm32"))]
struct NativeTracing {
    capture: Option<shoop_common::tracing_capture::ReusableCaptureSession>,
    start_on_app_init: Option<bool>,
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeTracing {
    fn initialize(cli: &AppArgs) -> anyhow::Result<Self> {
        shoop_common::tracing_helpers::set_engine_detail_enabled(false);
        shoop_common::tracing_helpers::set_tracing_output_enabled(true);
        shoop_common::tracing_helpers::set_tracing_enabled(false);
        shoop_common::init()?;
        Ok(Self {
            capture: None,
            start_on_app_init: cli.tracing.then_some(cli.tracing_engine_detail),
        })
    }

    fn start_requested_capture(&mut self) -> anyhow::Result<()> {
        if let Some(engine_detail) = self.start_on_app_init.take() {
            self.start_capture(engine_detail)?;
        }
        Ok(())
    }

    fn start_capture(&mut self, engine_detail: bool) -> anyhow::Result<()> {
        if self.capture.is_some() {
            return Ok(());
        }
        let capture = shoop_common::tracing_capture::ReusableCaptureSession::start(
            Path::new("traces"),
            "application",
        )?;
        capture.wait_until_capturing()?;
        shoop_common::tracing_helpers::set_engine_detail_enabled(engine_detail);
        shoop_common::tracing_helpers::set_tracing_output_enabled(true);
        shoop_common::tracing_helpers::set_tracing_enabled(true);
        tracing::info!(
            target: "Frontend.Egui",
            capture = true,
            engine_detail,
            "frontend.egui.tracing_started"
        );
        self.capture = Some(capture);
        Ok(())
    }

    fn quiesce_capture(&self) -> anyhow::Result<()> {
        if self.capture.is_none() {
            anyhow::bail!("tracing is not active");
        }
        shoop_common::tracing_helpers::set_tracing_output_enabled(false);
        shoop_common::tracing_helpers::set_tracing_enabled(false);
        Ok(())
    }

    fn stop_capture(&mut self, save: bool) -> anyhow::Result<TracingStopped> {
        use shoop_common::tracing_capture::CaptureDisposition;

        let Some(mut capture) = self.capture.take() else {
            anyhow::bail!("tracing is not active");
        };
        shoop_common::tracing_helpers::set_engine_detail_enabled(false);
        let disposition = if save {
            CaptureDisposition::Save
        } else {
            CaptureDisposition::Discard
        };
        let path = capture.path().display().to_string();
        capture.stop(disposition)?;
        Ok(if save {
            TracingStopped::Saved(path)
        } else {
            TracingStopped::Discarded
        })
    }

    fn status(&self) -> TracingStatus {
        let status = shoop_common::tracing_capture::CaptureStatus::current();
        TracingStatus {
            available: true,
            unavailable_reason: None,
            active: status.active,
            buffer_capacity_bytes: status.event_storage_bytes,
        }
    }

    fn shutdown(&mut self) -> anyhow::Result<()> {
        if self.capture.is_some() {
            self.quiesce_capture()?;
            self.stop_capture(true)?;
        }
        shoop_common::tracing_capture::shutdown_reusable_profiler()?;
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
type SharedNativeTracing = Arc<Mutex<NativeTracing>>;

#[derive(Clone, Copy, Eq, PartialEq)]
enum PendingTracingAction {
    Start { engine_detail: bool },
    Stop { save: bool },
    FinishStop { save: bool },
}

#[cfg(target_arch = "wasm32")]
struct BrowserTracing {
    capture: Option<shoop_tracing::BrowserCapture>,
    window_calibrations: Vec<shoop_tracing::BrowserCalibration>,
    unavailable_reason: Option<&'static str>,
}

#[cfg(target_arch = "wasm32")]
impl BrowserTracing {
    fn new() -> Self {
        Self {
            capture: None,
            window_calibrations: Vec::new(),
            unavailable_reason: None,
        }
    }

    fn start_capture(&mut self, engine_detail: bool) -> anyhow::Result<()> {
        if let Some(reason) = self.unavailable_reason {
            anyhow::bail!(reason);
        }
        if self.capture.is_none() {
            self.capture = Some(
                shoop_tracing::BrowserCapture::start(engine_detail).map_err(anyhow::Error::msg)?,
            );
            self.window_calibrations = vec![browser_window_calibration()?];
            tracing::info!(engine_detail, "frontend.egui.tracing_started");
        }
        Ok(())
    }

    fn poll(&self) -> anyhow::Result<()> {
        if let Some(capture) = &self.capture {
            capture.poll().map_err(anyhow::Error::msg)?;
        }
        Ok(())
    }

    fn discard_active(&mut self) -> anyhow::Result<()> {
        if let Some(capture) = self.capture.take() {
            capture.discard().map_err(anyhow::Error::msg)?;
        }
        self.window_calibrations.clear();
        Ok(())
    }

    fn stop_capture(
        &mut self,
        save: bool,
        realms: Vec<shoop_tracing::BrowserRealmData>,
    ) -> anyhow::Result<TracingStopped> {
        let capture = self
            .capture
            .take()
            .ok_or_else(|| anyhow::anyhow!("tracing is not active"))?;
        if !save {
            capture.discard().map_err(anyhow::Error::msg)?;
            self.window_calibrations.clear();
            return Ok(TracingStopped::Discarded);
        }
        self.window_calibrations.push(browser_window_calibration()?);
        let calibrations = std::mem::take(&mut self.window_calibrations);
        let bytes = capture
            .finish(calibrations, realms)
            .map_err(anyhow::Error::msg)?;
        let filename = "shoopdaloop-browser.pftrace";
        download_browser_trace(&bytes, filename)?;
        Ok(TracingStopped::Saved(filename.to_owned()))
    }

    fn status(&self) -> TracingStatus {
        TracingStatus {
            available: self.unavailable_reason.is_none(),
            unavailable_reason: self.unavailable_reason,
            active: self.capture.is_some(),
            buffer_capacity_bytes: 0,
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_window_calibration() -> anyhow::Result<shoop_tracing::BrowserCalibration> {
    let performance = web_sys::window()
        .ok_or_else(|| anyhow::anyhow!("browser window is unavailable"))?
        .performance()
        .ok_or_else(|| anyhow::anyhow!("browser performance clock is unavailable"))?;
    let before = performance.now();
    let after = performance.now();
    let source_ms = (before + after) * 0.5;
    Ok(shoop_tracing::BrowserCalibration {
        realm_id: 1,
        clock_id: 101,
        source_ticks: (source_ms * 1_000_000.0).round() as u64,
        reference_time_ns: ((performance.time_origin() + source_ms) * 1_000_000.0).round() as u64,
        uncertainty_ns: (((after - before) * 500_000.0).round() as u64).max(1),
    })
}

#[cfg(target_arch = "wasm32")]
fn download_browser_trace(bytes: &[u8], filename: &str) -> anyhow::Result<()> {
    use wasm_bindgen::JsCast as _;

    let data = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&data.buffer());
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)
        .map_err(|error| anyhow::anyhow!("could not create trace Blob: {error:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|error| anyhow::anyhow!("could not create trace URL: {error:?}"))?;
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| anyhow::anyhow!("browser document is unavailable"))?;
    let anchor = document
        .create_element("a")
        .map_err(|error| anyhow::anyhow!("could not create trace download: {error:?}"))?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| anyhow::anyhow!("trace download element is not an anchor"))?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();
    let revoke_url = url.clone();
    let revoke = wasm_bindgen::closure::Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&revoke_url);
    });
    web_sys::window()
        .ok_or_else(|| anyhow::anyhow!("browser window disappeared before trace URL cleanup"))?
        .set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 0)
        .map_err(|error| anyhow::anyhow!("could not schedule trace URL cleanup: {error:?}"))?;
    if let Some(body) = document.body() {
        body.set_attribute("data-perfetto-trace-saved", filename)
            .map_err(|error| anyhow::anyhow!("could not publish trace status: {error:?}"))?;
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
struct BrowserEphemeralFile {
    name: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
enum StartupSessionAction {
    LoadPath(String),
    ConfirmUrl(String),
    FetchUrl(String),
}

#[derive(Clone, Copy)]
struct BuildIdentity {
    kind: &'static str,
    version: &'static str,
    branch: &'static str,
    revision: &'static str,
    date: &'static str,
}

impl BuildIdentity {
    const CURRENT: Self = Self {
        kind: env!("SHOOP_BUILD_KIND"),
        version: env!("SHOOP_BUILD_VERSION"),
        branch: env!("SHOOP_BUILD_BRANCH"),
        revision: env!("SHOOP_BUILD_REVISION"),
        date: env!("SHOOP_BUILD_DATE"),
    };
}

fn show_about_dialog(
    context: &egui::Context,
    open: &mut bool,
    identity: BuildIdentity,
    settings_storage_location: &str,
) {
    if !*open {
        return;
    }
    egui::Window::new("About ShoopDaLoop")
        .open(open)
        .resizable(false)
        .show(context, |ui| {
            ui.heading("ShoopDaLoop by Sander Vocke");
            ui.separator();
            ui.label(format!("Build: {}", identity.kind));
            if identity.kind == "release" {
                ui.label(format!("Version: {}", identity.version));
            } else {
                ui.label(format!("Branch: {}", identity.branch));
                ui.label(format!("Commit: {}", identity.revision));
            }
            ui.label(format!("Built: {}", identity.date));
            ui.label(format!("Settings storage: {settings_storage_location}"));
        });
}

struct UnifiedApp {
    #[cfg(not(target_arch = "wasm32"))]
    exit: recovery::ExitController,
    #[cfg(not(target_arch = "wasm32"))]
    host_panel: recovery::HostPanel,
    runtime: Runtime,
    widget: AppWidget,
    about_open: bool,
    settings: SettingsManager,
    pending_profiling_preference: Option<bool>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_audio_settings: Option<u64>,
    #[cfg(not(target_arch = "wasm32"))]
    tracing: Option<SharedNativeTracing>,
    #[cfg(target_arch = "wasm32")]
    tracing: BrowserTracing,
    #[cfg(target_arch = "wasm32")]
    tracing_smoke_started: Option<Instant>,
    #[cfg(target_arch = "wasm32")]
    tracing_stop_started: Option<Instant>,
    pending_tracing_action: Option<PendingTracingAction>,
    last_update: Instant,
    startup_session: Option<(String, bool)>,
    session_url_input: String,
    session_url_error: Option<String>,
    session_url_prompt_open: bool,
    session_url_task: Option<shoop_egui::TaskId>,
    file_input: platform_input::InputSlot,
    file_output: platform_input::OutputSlot,
    session_url_confirmation: Option<String>,
    #[cfg(target_arch = "wasm32")]
    browser_settings_test: BrowserSettingsSelfTest,
    #[cfg(target_arch = "wasm32")]
    new_engine_self_test: browser_rt_self_test::NewEngineSelfTest,
    #[cfg(target_arch = "wasm32")]
    args: AppArgs,
    #[cfg(target_arch = "wasm32")]
    pending_file_intents: Rc<RefCell<VecDeque<AppIntent>>>,
    #[cfg(target_arch = "wasm32")]
    pending_ephemeral_files: Rc<RefCell<VecDeque<BrowserEphemeralFile>>>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_file_intent_tx: Sender<AppIntent>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_file_intent_rx: Receiver<AppIntent>,
}

#[cfg(all(not(target_arch = "wasm32"), not(test)))]
fn load_settings_manager(
    registry: shoop_egui::SettingsRegistry,
    _args: &AppArgs,
) -> SettingsManager {
    SettingsManager::load(registry, env!("CARGO_PKG_VERSION"))
}

#[cfg(all(not(target_arch = "wasm32"), test))]
fn load_settings_manager(
    registry: shoop_egui::SettingsRegistry,
    _args: &AppArgs,
) -> SettingsManager {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_TEST_SETTINGS: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_TEST_SETTINGS.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "shoopdaloop-test-settings-{}-{id}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    SettingsManager::load_from_path(registry, env!("CARGO_PKG_VERSION"), path)
}

#[cfg(target_arch = "wasm32")]
fn load_settings_manager(
    registry: shoop_egui::SettingsRegistry,
    args: &AppArgs,
) -> SettingsManager {
    SettingsManager::load(
        registry,
        env!("CARGO_PKG_VERSION"),
        args.settings_save_failure,
    )
}

#[cfg(all(not(target_arch = "wasm32"), not(test)))]
fn default_audio_driver_kind() -> AudioDriverKind {
    if cfg!(feature = "cpal") {
        AudioDriverKind::Cpal
    } else if cfg!(feature = "jack") {
        AudioDriverKind::Jack
    } else {
        AudioDriverKind::Dummy
    }
}

#[cfg(all(not(target_arch = "wasm32"), test))]
fn default_audio_driver_kind() -> AudioDriverKind {
    AudioDriverKind::Dummy
}

#[cfg(target_arch = "wasm32")]
fn default_audio_driver_kind() -> AudioDriverKind {
    AudioDriverKind::WebAudio
}

impl UnifiedApp {
    fn new(
        ui_scale_default: f64,
        touch_mode_default: bool,
        args: &AppArgs,
    ) -> anyhow::Result<Self> {
        Self::new_with_runtime(ui_scale_default, touch_mode_default, args, Runtime::new)
    }

    fn new_with_runtime(
        ui_scale_default: f64,
        touch_mode_default: bool,
        args: &AppArgs,
        make_runtime: impl FnOnce(
            &shoop_settings::SettingsSnapshot,
            &AppArgs,
        ) -> anyhow::Result<Runtime>,
    ) -> anyhow::Result<Self> {
        let _span = tracing::info_span!("frontend.egui.initialize").entered();
        #[cfg(not(target_arch = "wasm32"))]
        let (pending_file_intent_tx, pending_file_intent_rx) = mpsc::channel();
        let mut settings_builder = SettingsRegistryBuilder::default();
        register_settings_with_appearance_defaults(
            &mut settings_builder,
            ui_scale_default,
            touch_mode_default,
        )?;
        register_audio_settings_with_default_driver(
            &mut settings_builder,
            default_audio_driver_kind(),
        )?;
        shoop_egui::register_audio_backend_settings(&mut settings_builder)?;
        #[cfg(not(target_arch = "wasm32"))]
        register_script_settings(&mut settings_builder)?;
        #[cfg(target_arch = "wasm32")]
        register_bundled_script_settings(&mut settings_builder)?;
        let settings_registry = settings_builder.finish();
        let settings = load_settings_manager(settings_registry.clone(), args);
        let mut widget = AppWidget::new(std::sync::Arc::new(settings_registry));
        let runtime = make_runtime(&settings.active(), args)?;
        widget.set_click_track_preview_available(runtime.audio_preview_available());
        Ok(Self {
            #[cfg(not(target_arch = "wasm32"))]
            exit: Default::default(),
            #[cfg(not(target_arch = "wasm32"))]
            host_panel: Default::default(),
            runtime,
            widget,
            about_open: false,
            settings,
            pending_profiling_preference: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_audio_settings: None,
            #[cfg(not(target_arch = "wasm32"))]
            tracing: None,
            #[cfg(target_arch = "wasm32")]
            tracing: BrowserTracing::new(),
            #[cfg(target_arch = "wasm32")]
            tracing_smoke_started: None,
            #[cfg(target_arch = "wasm32")]
            tracing_stop_started: None,
            pending_tracing_action: if cfg!(target_arch = "wasm32") && args.tracing {
                Some(PendingTracingAction::Start {
                    engine_detail: args.tracing_engine_detail,
                })
            } else {
                None
            },
            last_update: Instant::now(),
            startup_session: args
                .session
                .clone()
                .map(|source| (source, args.force_url_session)),
            session_url_input: String::new(),
            session_url_error: None,
            session_url_prompt_open: false,
            session_url_task: None,
            file_input: platform_input::InputSlot::default(),
            file_output: platform_input::OutputSlot::default(),
            session_url_confirmation: None,
            #[cfg(target_arch = "wasm32")]
            browser_settings_test: BrowserSettingsSelfTest::from_args(args),
            #[cfg(target_arch = "wasm32")]
            new_engine_self_test: browser_rt_self_test::NewEngineSelfTest::new(
                args.new_engine_self_test || args.self_test,
                args.stress,
                args.persistence_large,
            ),
            #[cfg(target_arch = "wasm32")]
            args: args.clone(),
            #[cfg(target_arch = "wasm32")]
            pending_file_intents: Rc::new(RefCell::new(VecDeque::new())),
            #[cfg(target_arch = "wasm32")]
            pending_ephemeral_files: Rc::new(RefCell::new(VecDeque::new())),
            #[cfg(not(target_arch = "wasm32"))]
            pending_file_intent_tx,
            #[cfg(not(target_arch = "wasm32"))]
            pending_file_intent_rx,
        })
    }
}

fn startup_session_action(source: String, force_url_session: bool) -> StartupSessionAction {
    if !is_http_url(&source) {
        StartupSessionAction::LoadPath(source)
    } else if force_url_session {
        StartupSessionAction::FetchUrl(source)
    } else {
        StartupSessionAction::ConfirmUrl(source)
    }
}

impl UnifiedApp {
    fn process_startup_session_source(&mut self, source: String, force_url_session: bool) {
        match startup_session_action(source, force_url_session) {
            StartupSessionAction::ConfirmUrl(source) => {
                self.session_url_confirmation = Some(source);
            }
            StartupSessionAction::FetchUrl(source) => self.fetch_session_url(source),
            StartupSessionAction::LoadPath(source) => {
                #[cfg(not(target_arch = "wasm32"))]
                self.handle_ui_intent(AppIntent::RequestLoadSessionSource { source });
                #[cfg(target_arch = "wasm32")]
                tracing::warn!(source = %source, "frontend.session.filesystem_path_unavailable");
            }
        }
    }

    fn show_session_url_dialogs(&mut self, context: &egui::Context) {
        if self.session_url_prompt_open {
            let mut submit = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::unique("load_session_url_prompt")).show(context, |ui| {
                ui.heading("Load session from URL");
                ui.add_space(8.0);
                ui.label("Session URL:");
                let response = ui.text_edit_singleline(&mut self.session_url_input);
                if let Some(error) = &self.session_url_error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.horizontal(|ui| {
                    submit = (response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter)))
                        || ui.button("Continue").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
            if cancel {
                self.session_url_prompt_open = false;
                self.session_url_error = None;
                self.cancel_session_url_task();
            } else if submit {
                let source = self.session_url_input.trim().to_owned();
                if is_http_url(&source) {
                    self.session_url_prompt_open = false;
                    self.session_url_error = None;
                    self.session_url_confirmation = Some(source);
                } else {
                    self.session_url_error =
                        Some("Please enter an http:// or https:// URL".to_owned());
                }
            }
        }

        if let Some(url) = self.session_url_confirmation.clone() {
            let mut fetch = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::unique("confirm_session_url_fetch")).show(context, |ui| {
                ui.heading("Fetch session from URL?");
                ui.add_space(8.0);
                ui.label("ShoopDaLoop will download and open this session:");
                ui.monospace(&url);
                ui.horizontal(|ui| {
                    fetch = ui.button("Fetch and open").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
            if fetch {
                self.session_url_confirmation = None;
                self.fetch_session_url(url);
            } else if cancel {
                self.session_url_confirmation = None;
                self.cancel_session_url_task();
            }
        }
    }

    fn cancel_session_url_task(&mut self) {
        if let Some(task_id) = self.session_url_task.take() {
            self.handle_ui_intent(AppIntent::CancelIoTask { task_id });
        }
    }
    fn fetch_session_url(&mut self, url: String) {
        if let Some(task_id) = self.session_url_task.take() {
            self.fetch_session_url_for_task(url, task_id);
        } else {
            self.handle_ui_intent(AppIntent::RequestLoadSessionSource { source: url });
        }
    }
    fn reserve_file_input(
        &mut self,
        task_id: shoop_egui::TaskId,
    ) -> Option<platform_input::InputLease> {
        match self.file_input.reserve(task_id) {
            Ok(lease) => Some(lease),
            Err(error) => {
                self.handle_ui_intent(AppIntent::FailIoTask {
                    task_id,
                    message: error.to_string(),
                });
                None
            }
        }
    }
    fn fetch_session_url_for_task(&mut self, url: String, task_id: shoop_egui::TaskId) {
        let Some(lease) = self.reserve_file_input(task_id) else {
            return;
        };
        let download = SessionDownload {
            task_id,
            name: session_source_name(&url),
            bytes: Vec::new(),
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let pending = self.pending_file_intent_tx.clone();
            let download = std::sync::Mutex::new(download);
            ehttp::streaming::fetch(ehttp::Request::get(&url), move |part| {
                if lease.check().is_err() {
                    return std::ops::ControlFlow::Break(());
                }
                match download
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .part(part)
                {
                    Ok(None) => std::ops::ControlFlow::Continue(()),
                    Ok(Some(intent)) => {
                        let _ = pending.send(intent);
                        std::ops::ControlFlow::Break(())
                    }
                    Err(error) => {
                        let _ = pending.send(AppIntent::FailIoTask {
                            task_id,
                            message: error.to_string(),
                        });
                        std::ops::ControlFlow::Break(())
                    }
                }
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let pending = Rc::clone(&self.pending_file_intents);
            let registration = lease.registration();
            wasm_bindgen_futures::spawn_local(async move {
                let result = futures_util::future::Abortable::new(
                    read_browser_session_url(&url, download, &lease),
                    registration,
                )
                .await;
                match result {
                    Ok(Ok(intent)) => pending.borrow_mut().push_back(intent),
                    Ok(Err(error)) => pending.borrow_mut().push_back(AppIntent::FailIoTask {
                        task_id,
                        message: error.to_string(),
                    }),
                    Err(_) => {}
                }
            });
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn process_tracing(&mut self) {
        let Some(tracing) = self.tracing.as_ref().cloned() else {
            self.widget.set_tracing_status(TracingStatus::default());
            self.pending_tracing_action = None;
            return;
        };
        if let Some(action) = self.pending_tracing_action.take() {
            let result = tracing
                .lock()
                .map_err(|_| anyhow::anyhow!("tracing controller lock is poisoned"))
                .and_then(|mut tracing| match action {
                    PendingTracingAction::Start { engine_detail } => {
                        tracing.start_capture(engine_detail).map(|()| None)
                    }
                    PendingTracingAction::Stop { save } => {
                        tracing.quiesce_capture()?;
                        self.pending_tracing_action =
                            Some(PendingTracingAction::FinishStop { save });
                        Ok(None)
                    }
                    PendingTracingAction::FinishStop { save } => {
                        tracing.stop_capture(save).map(Some)
                    }
                });
            match result {
                Ok(Some(stopped)) => self.widget.notify_tracing_stopped(stopped),
                Ok(None) => {}
                Err(error) => self.settings.report_action_error(error.to_string()),
            }
        }
        let status = tracing
            .lock()
            .map(|tracing| tracing.status())
            .unwrap_or_default();
        self.widget.set_tracing_status(status);
    }

    #[cfg(target_arch = "wasm32")]
    fn process_tracing(&mut self) {
        if self
            .pending_tracing_action
            .is_some_and(|action| matches!(action, PendingTracingAction::FinishStop { .. }))
            && self
                .tracing_stop_started
                .is_some_and(|started| started.elapsed() >= Duration::from_secs(5))
            && !self.runtime.trace_stopped()
        {
            self.runtime.cancel_tracing_request();
            let _ = self.tracing.discard_active();
            self.pending_tracing_action = None;
            self.tracing_stop_started = None;
            tracing::error!("frontend.browser_trace.stop_timeout");
            self.settings
                .report_action_error("Browser trace stop timed out; capture is incomplete");
        }
        if let Some(action) = self.pending_tracing_action.take() {
            let result: anyhow::Result<Option<TracingStopped>> = (|| match action {
                PendingTracingAction::Start { engine_detail } => {
                    self.tracing.start_capture(engine_detail)?;
                    if let Err(error) = self.runtime.start_tracing(engine_detail) {
                        self.runtime.cancel_tracing_request();
                        if let Err(discard_error) = self.tracing.discard_active() {
                            tracing::error!(
                                error = %discard_error,
                                "frontend.egui.tracing_start_rollback_failed"
                            );
                        }
                        return Err(error);
                    }
                    Ok(None)
                }
                PendingTracingAction::Stop { save } => {
                    self.runtime.request_stop_tracing()?;
                    self.tracing_stop_started = Some(Instant::now());
                    self.pending_tracing_action = Some(PendingTracingAction::FinishStop { save });
                    Ok(None)
                }
                PendingTracingAction::FinishStop { save } => {
                    match self.runtime.take_trace_realms()? {
                        Some(realms) => self.tracing.stop_capture(save, realms).map(Some),
                        None => {
                            self.pending_tracing_action =
                                Some(PendingTracingAction::FinishStop { save });
                            Ok(None)
                        }
                    }
                }
            })();
            match result {
                Ok(Some(stopped)) => self.widget.notify_tracing_stopped(stopped),
                Ok(None) => {}
                Err(error) => {
                    tracing::error!(error = %error, "frontend.egui.tracing_action_failed");
                    self.settings.report_action_error(error.to_string());
                }
            }
        }
        if !self
            .pending_tracing_action
            .is_some_and(|action| matches!(action, PendingTracingAction::FinishStop { .. }))
        {
            self.tracing_stop_started = None;
        }
        if let Err(error) = self.tracing.poll() {
            self.runtime.cancel_tracing_request();
            if let Err(discard_error) = self.tracing.discard_active() {
                tracing::error!(
                    error = %discard_error,
                    "frontend.browser_trace.window_storage_failure_cleanup_failed"
                );
            }
            self.settings.report_action_error(error.to_string());
        }
        if let Err(error) = self.runtime.poll_tracing() {
            tracing::error!(error = %error, "frontend.egui.tracing_realm_poll_failed");
            self.settings.report_action_error(error.to_string());
        }
        let mut status = self.tracing.status();
        if status.active {
            if let Err(error) = self.runtime.ensure_tracing_realm() {
                self.runtime.cancel_tracing_request();
                if let Err(discard_error) = self.tracing.discard_active() {
                    tracing::error!(
                        error = %discard_error,
                        "frontend.egui.tracing_start_rollback_failed"
                    );
                }
                tracing::error!(error = %error, "frontend.egui.tracing_realm_attach_failed");
                self.settings.report_action_error(error.to_string());
                status = self.tracing.status();
            }
        }
        if self.args.tracing_smoke_test && status.active {
            let started = self.tracing_smoke_started.get_or_insert_with(Instant::now);
            let received_records = self
                .tracing
                .capture
                .as_ref()
                .map_or(0, |capture| capture.emitted_records());
            let ready = self.runtime.tracing_realm_ready() && received_records > 8192;
            let timed_out = started.elapsed() >= Duration::from_secs(60);
            if let Some(body) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.body())
            {
                let _ = body
                    .set_attribute("data-perfetto-trace-records", &received_records.to_string());
                if timed_out && !ready {
                    let _ = body.set_attribute(
                        "data-perfetto-trace-error",
                        &format!(
                            "Timed out waiting for the local UI trace to emit more than 8192 records; received {received_records}"
                        ),
                    );
                }
            }
            if (ready || timed_out) && self.pending_tracing_action.is_none() {
                tracing::info!("frontend.egui.tracing_smoke_stop_requested");
                self.pending_tracing_action = Some(PendingTracingAction::Stop { save: true });
                self.args.tracing_smoke_test = false;
            }
        }
        self.widget.set_tracing_status(status);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_settings_action(&mut self, action: SettingsAction) {
        let action = match action {
            SettingsAction::StartTracing { engine_detail } => {
                if self.tracing.is_some() {
                    self.pending_tracing_action =
                        Some(PendingTracingAction::Start { engine_detail });
                } else {
                    self.settings
                        .report_action_error("Tracing is unavailable in this build");
                }
                return;
            }
            SettingsAction::StopTracing { save } => {
                if self.tracing.is_some() {
                    self.pending_tracing_action = Some(PendingTracingAction::Stop { save });
                } else {
                    self.settings
                        .report_action_error("Tracing is unavailable in this build");
                }
                return;
            }
            action => action,
        };
        if matches!(
            &action,
            SettingsAction::Save(_)
                | SettingsAction::SaveTrackDefaults { .. }
                | SettingsAction::RecoverWithDefaults
        ) {
            if self.audio_settings_busy() {
                if let SettingsAction::SaveTrackDefaults { request_id, .. } = &action {
                    self.widget.notify_track_default_save_result(
                        *request_id,
                        TrackDefaultSaveResult::Retry,
                    );
                }
                self.settings.report_action_error(
                    "Wait for the audio-driver operation to finish before saving settings",
                );
                return;
            }
            if !matches!(&action, SettingsAction::RecoverWithDefaults)
                && !self.reconcile_audio_settings(&self.runtime.snapshot())
            {
                return;
            }
        }
        if matches!(
            &action,
            SettingsAction::RequestAudioDriverSwitch { .. }
                | SettingsAction::RetryAudioDriverPersistence { .. }
        ) && self.settings.view().persistence == shoop_settings::SettingsPersistenceState::Saving
        {
            self.settings.report_action_error(
                "Wait for the settings save to finish before updating audio preferences",
            );
            return;
        }
        let kind = action.kind();
        let _span = tracing::debug_span!("frontend.egui.settings_action", action = kind).entered();
        let action = self.preserve_profiling_preference(action);
        let result = match action {
            SettingsAction::SetProfiling(enabled) => {
                self.request_profiling(enabled);
                return;
            }
            SettingsAction::Save(draft) => validate_script_draft(&draft)
                .and_then(|()| self.settings.request_save(draft).map_err(Into::into)),
            SettingsAction::SaveTrackDefaults { request_id, draft } => {
                match validate_script_draft(&draft) {
                    Err(error) => {
                        self.widget.notify_track_default_save_result(
                            request_id,
                            TrackDefaultSaveResult::Failed,
                        );
                        Err(error)
                    }
                    Ok(()) => match self.settings.request_save(draft) {
                        Ok(()) => {
                            self.widget.notify_track_default_save_result(
                                request_id,
                                TrackDefaultSaveResult::Accepted,
                            );
                            Ok(())
                        }
                        Err(
                            settings::SettingsManagerError::Saving
                            | settings::SettingsManagerError::StaleRevision { .. },
                        ) => {
                            self.widget.notify_track_default_save_result(
                                request_id,
                                TrackDefaultSaveResult::Retry,
                            );
                            Ok(())
                        }
                        Err(error) => {
                            self.widget.notify_track_default_save_result(
                                request_id,
                                TrackDefaultSaveResult::Failed,
                            );
                            Err(error.into())
                        }
                    },
                }
            }
            SettingsAction::RequestAudioDriverSwitch { config, draft } => {
                validate_script_draft(&draft).and_then(|()| {
                    self.runtime
                        .dispatch(AppIntent::RequestAudioDriverSwitch { config })
                        .map_err(Into::into)
                })
            }
            SettingsAction::RetryAudioDriverPersistence { request_id } => self
                .runtime
                .dispatch(AppIntent::RetryAudioDriverPersistence { request_id })
                .map_err(Into::into),
            SettingsAction::RequestBrowserPermissions => {
                self.settings
                    .report_action_error("Browser permissions are unavailable in native builds");
                return;
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
            SettingsAction::RescanBuiltinScripts => {
                self.runtime.request_builtin_rescan(&self.settings.active());
                Ok(())
            }
            SettingsAction::RequestEphemeralScriptPicker => {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("Lua script", &["lua"])
                    .pick_file()
                else {
                    return;
                };
                load_ephemeral_script_path(&path).map(|(name, source, source_path)| {
                    self.widget
                        .queue_ephemeral_script_from_path(name, source, Some(source_path));
                })
            }
            SettingsAction::RequestReloadUserScript { script_id } => {
                self.runtime.reload_user_script(script_id)
            }
            SettingsAction::StartTracing { .. } | SettingsAction::StopTracing { .. } => {
                unreachable!("tracing actions are handled before traced settings actions")
            }
        };
        if let Err(error) = result {
            self.settings.report_action_error(error.to_string());
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn handle_settings_action(&mut self, action: SettingsAction) {
        let action = match action {
            SettingsAction::StartTracing { engine_detail } => {
                self.pending_tracing_action = Some(PendingTracingAction::Start { engine_detail });
                return;
            }
            SettingsAction::StopTracing { save } => {
                self.pending_tracing_action = Some(PendingTracingAction::Stop { save });
                return;
            }
            action => action,
        };
        let kind = action.kind();
        let _span = tracing::debug_span!("frontend.egui.settings_action", action = kind).entered();
        let action = self.preserve_profiling_preference(action);
        let result = match action {
            SettingsAction::SetProfiling(enabled) => {
                self.request_profiling(enabled);
                return;
            }
            SettingsAction::Save(draft) => self.settings.request_save(draft),
            SettingsAction::SaveTrackDefaults { request_id, draft } => {
                match self.settings.request_save(draft) {
                    Ok(()) => {
                        self.widget.notify_track_default_save_result(
                            request_id,
                            TrackDefaultSaveResult::Accepted,
                        );
                        Ok(())
                    }
                    Err(
                        settings::SettingsManagerError::Saving
                        | settings::SettingsManagerError::StaleRevision { .. },
                    ) => {
                        self.widget.notify_track_default_save_result(
                            request_id,
                            TrackDefaultSaveResult::Retry,
                        );
                        Ok(())
                    }
                    Err(error) => {
                        self.widget.notify_track_default_save_result(
                            request_id,
                            TrackDefaultSaveResult::Failed,
                        );
                        Err(error)
                    }
                }
            }
            SettingsAction::RequestAudioDriverSwitch { .. }
            | SettingsAction::RetryAudioDriverPersistence { .. } => {
                self.settings.report_action_error(
                    "Native audio-driver switching is unavailable in browser builds",
                );
                return;
            }
            SettingsAction::RequestBrowserPermissions => {
                if let Err(error) = open_browser_permissions_dialog() {
                    self.settings.report_action_error(error.to_string());
                }
                return;
            }
            SettingsAction::RecoverWithDefaults => self.settings.request_recovery(),
            SettingsAction::RequestEphemeralScriptPicker => {
                let pending = Rc::clone(&self.pending_ephemeral_files);
                wasm_bindgen_futures::spawn_local(async move {
                    if let Some(file) = rfd::AsyncFileDialog::new()
                        .add_filter("Lua script", &["lua"])
                        .pick_file()
                        .await
                    {
                        pending.borrow_mut().push_back(BrowserEphemeralFile {
                            name: file.file_name(),
                            bytes: file.read().await,
                        });
                    }
                });
                return;
            }
            SettingsAction::RescanBuiltinScripts => {
                self.runtime.request_builtin_rescan(&self.settings.active());
                return;
            }
            SettingsAction::RequestAddUserScript
            | SettingsAction::RequestReloadUserScript { .. } => {
                self.settings.report_action_error(
                    "Path-based user scripts are unavailable in browser builds",
                );
                return;
            }
            SettingsAction::StartTracing { .. } | SettingsAction::StopTracing { .. } => {
                unreachable!("tracing actions are handled before traced settings actions")
            }
        };
        if let Err(error) = result {
            self.settings.report_action_error(error.to_string());
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn audio_settings_busy(&self) -> bool {
        self.runtime.handle.has_pending_audio_driver_commands()
            || matches!(
                self.runtime.snapshot().audio_drivers.switch.status,
                shoop_egui::AudioDriverSwitchStatus::Switching
                    | shoop_egui::AudioDriverSwitchStatus::Resampling
                    | shoop_egui::AudioDriverSwitchStatus::Restoring
                    | shoop_egui::AudioDriverSwitchStatus::Persisting
            )
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn reconcile_audio_settings(&mut self, snapshot: &AppSnapshot) -> bool {
        let state = &snapshot.audio_drivers.switch;
        if self.runtime.remote_entry.is_some()
            || state.status != shoop_egui::AudioDriverSwitchStatus::Completed
            || self.pending_audio_settings == Some(state.request_id)
            || !state
                .target
                .as_ref()
                .zip(snapshot.audio_drivers.active.as_ref())
                .is_some_and(|(target, active)| target.configured == active.configured)
        {
            return true;
        }
        if self.settings.view().persistence == shoop_settings::SettingsPersistenceState::Saving {
            return false;
        }
        if let Err(error) = self.settings.reload_driver_preferences() {
            self.settings.report_action_error(error.to_string());
            return false;
        }
        self.pending_audio_settings = Some(state.request_id);
        true
    }

    #[cfg(target_arch = "wasm32")]
    fn queue_ephemeral_script_bytes(&mut self, name: String, bytes: &[u8]) {
        match load_ephemeral_script_bytes(name, bytes) {
            Ok((name, source)) => self.widget.queue_ephemeral_script(name, source),
            Err(error) => {
                tracing::error!(error = %error, "frontend.scripting.ephemeral_load_failed")
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_dropped_files(&mut self, context: &egui::Context) {
        let files = context.input(|input| input.raw.dropped_files.clone());
        for file in files {
            let path = file.path();
            if !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_lua_file_name)
            {
                continue;
            }
            match load_ephemeral_script_path(path) {
                Ok((name, source, source_path)) => {
                    self.widget
                        .queue_ephemeral_script_from_path(name, source, Some(source_path))
                }
                Err(error) => {
                    tracing::error!(path = %path.display(), error = %error, "frontend.scripting.ephemeral_load_failed")
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn handle_dropped_files(&mut self, context: &egui::Context) {
        let files = context.input(|input| input.raw.dropped_files.clone());
        for file in files {
            let Some(name) = file
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
            else {
                continue;
            };
            if !is_lua_file_name(&name) {
                continue;
            }
            let pending = Rc::clone(&self.pending_ephemeral_files);
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(bytes) = file.bytes_async().await {
                    pending
                        .borrow_mut()
                        .push_back(BrowserEphemeralFile { name, bytes });
                }
            });
        }
    }

    fn show_file_drop_overlay(&self, context: &egui::Context) {
        let hovering = context.input(|input| {
            #[cfg(not(target_arch = "wasm32"))]
            {
                input.raw.hovered_files.iter().any(|file| {
                    file.path
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .and_then(|name| name.to_str())
                        .is_some_and(is_lua_file_name)
                })
            }
            #[cfg(target_arch = "wasm32")]
            {
                !input.raw.hovered_files.is_empty()
            }
        });
        if hovering {
            egui::Area::new(egui::Id::unique("lua_file_drop_overlay"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(context, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.heading("Drop Lua script to load it for this app run");
                    });
                });
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn clear_connection_state(&mut self, context: &egui::Context) {
        context.memory_mut(|memory| {
            memory.data.clear();
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
        self.file_input.reconcile(None);
        let (sender, receiver) = mpsc::channel();
        self.pending_file_intent_tx = sender;
        self.pending_file_intent_rx = receiver;
        self.startup_session = None;
        self.session_url_task = None;
        self.session_url_confirmation = None;
        self.session_url_prompt_open = false;
        self.session_url_input.clear();
        self.session_url_error = None;
        self.pending_audio_settings = None;
        self.widget.reset_connection();
        self.exit = Default::default();
        self.host_panel = Default::default();
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn use_connection(&mut self, runtime: Runtime, context: &egui::Context) {
        self.clear_connection_state(context);
        self.runtime = runtime;
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn open_remote_profile(&mut self, context: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Remote host profile", &["json"])
            .pick_file()
        {
            match shoop_host::remote::RemoteProfile::read(&path)
                .and_then(|profile| Runtime::attach(&self.settings.active(), profile))
            {
                Ok(runtime) => {
                    self.use_connection(runtime, context);
                }
                Err(error) => self.settings.report_action_error(error.to_string()),
            }
        }
    }
    fn show(&mut self, ui: &mut egui::Ui) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(entry) = self.runtime.remote_entry.clone() {
                self.exit.intercept_close(ui.ctx());
                self.exit.show(ui.ctx(), &entry, &self.runtime.handle);
                self.host_panel.show(ui.ctx(), &entry);
            }
            let status = self.runtime.handle.status();
            self.widget.set_host_state(recovery::host_ui_state(
                self.runtime.remote_entry.as_ref(),
                &status,
                &self.exit,
            ));
            if self.runtime.remote_entry.is_some() && status.state != shoop_app_client::State::Ready
            {
                let settings_state = self.settings.view();
                let response =
                    self.widget
                        .show(ui, &self.runtime.snapshot(), &settings_state, None);
                self.about_open |= response.about_requested;
                show_about_dialog(
                    ui.ctx(),
                    &mut self.about_open,
                    BuildIdentity::CURRENT,
                    &settings_state.storage_location,
                );
                self.handle_host_actions(ui.ctx(), response.host_actions);
                ui.ctx().request_repaint_after(Duration::from_millis(50));
                return;
            }
        }
        if let Some((source, force_url_session)) = self.startup_session.take() {
            self.process_startup_session_source(source, force_url_session);
        }
        let _span = tracing::trace_span!("frontend.egui.update").entered();
        self.settings.poll();
        self.save_profiling_preference();
        if let Err(error) = self
            .runtime
            .reconcile_audio_settings(&self.settings.active())
        {
            self.settings.report_action_error(error.to_string());
        }
        if let Err(error) = self
            .runtime
            .reconcile_script_settings(&self.settings.active())
        {
            self.settings.report_action_error(error.to_string());
        }
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_update);
        self.last_update = now;
        #[cfg(target_arch = "wasm32")]
        self.runtime.set_repaint_context(ui.ctx().clone());
        self.runtime.tick(elapsed);
        self.widget
            .set_click_track_preview_available(self.runtime.audio_preview_available());
        self.runtime.process_audio_previews();

        #[cfg(target_arch = "wasm32")]
        let ephemeral_files: Vec<_> = self
            .pending_ephemeral_files
            .borrow_mut()
            .drain(..)
            .collect();
        #[cfg(target_arch = "wasm32")]
        for file in ephemeral_files {
            self.queue_ephemeral_script_bytes(file.name, &file.bytes);
        }
        self.handle_dropped_files(ui.ctx());

        #[cfg(target_arch = "wasm32")]
        let pending: Vec<_> = self.pending_file_intents.borrow_mut().drain(..).collect();
        #[cfg(not(target_arch = "wasm32"))]
        let pending: Vec<_> = self.pending_file_intent_rx.try_iter().collect();
        for intent in pending {
            match self.runtime.dispatch(intent.clone()) {
                Ok(()) => {}
                Err(application::DispatchError::Full) => {
                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = self.pending_file_intent_tx.send(intent);
                    #[cfg(target_arch = "wasm32")]
                    self.pending_file_intents.borrow_mut().push_back(intent);
                }
                Err(error) => {
                    tracing::error!(error = %error, "frontend.egui.file_intent_dispatch_failed")
                }
            }
        }
        let snapshot = self.runtime.snapshot();
        self.file_input.reconcile(snapshot.io_task.as_ref());
        #[cfg(not(target_arch = "wasm32"))]
        self.reconcile_audio_settings(&snapshot);
        #[cfg(target_arch = "wasm32")]
        self.browser_settings_test
            .update(&mut self.settings, &mut self.widget, &self.runtime);
        #[cfg(target_arch = "wasm32")]
        self.new_engine_self_test
            .update(&mut self.runtime, &snapshot);
        let settings_state = self.settings.view();
        #[cfg(not(target_arch = "wasm32"))]
        let script_paths = Some(self.runtime.script_paths());
        #[cfg(target_arch = "wasm32")]
        let script_paths = None;
        let response = self
            .widget
            .show(ui, &snapshot, &settings_state, script_paths);
        self.about_open |= response.about_requested;
        for intent in response.app_actions {
            self.handle_ui_intent(intent);
        }
        for action in response.settings_actions {
            self.handle_settings_action(action);
        }
        show_about_dialog(
            ui.ctx(),
            &mut self.about_open,
            BuildIdentity::CURRENT,
            &settings_state.storage_location,
        );
        self.show_file_drop_overlay(ui.ctx());
        self.show_session_url_dialogs(ui.ctx());
        while let Some(request) = self.runtime.take_file_request() {
            self.handle_file_request(request);
        }
        while let Some(output) = self.runtime.take_file_output() {
            #[cfg(target_arch = "wasm32")]
            self.new_engine_self_test.file_output(&output);
            let lease = match self.file_output.reserve() {
                Ok(lease) => lease,
                Err(error) => {
                    let intent = AppIntent::FailIoTask {
                        task_id: output.task_id,
                        message: error.to_string(),
                    };
                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = self.pending_file_intent_tx.send(intent);
                    #[cfg(target_arch = "wasm32")]
                    self.pending_file_intents.borrow_mut().push_back(intent);
                    continue;
                }
            };
            #[cfg(not(target_arch = "wasm32"))]
            save_file_output(output, self.pending_file_intent_tx.clone(), lease);
            #[cfg(target_arch = "wasm32")]
            save_file_output(output, Rc::clone(&self.pending_file_intents), lease);
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.handle_host_actions(ui.ctx(), response.host_actions);
        ui.ctx().request_repaint_after(UPDATE_INTERVAL);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_host_actions(
        &mut self,
        context: &egui::Context,
        actions: Vec<shoop_egui::HostAction>,
    ) {
        for action in actions {
            match action {
                shoop_egui::HostAction::OpenProfiling => self.widget.open_profiling(),
                shoop_egui::HostAction::OpenManager => {
                    if let Err(error) = shoop_host::launch::launch_manager() {
                        self.settings.report_action_error(error.to_string());
                    }
                }
                shoop_egui::HostAction::ImportProfile => self.open_remote_profile(context),
                shoop_egui::HostAction::OpenSettings => self.host_panel.open = true,
                shoop_egui::HostAction::CloseUi => self.exit.close_ui(context),
                shoop_egui::HostAction::CloseUiAndHost => self.exit.open = true,
                shoop_egui::HostAction::Reconnect => {
                    if let Some(entry) = self.runtime.remote_entry.clone() {
                        match Runtime::attach(&self.settings.active(), entry) {
                            Ok(runtime) => self.use_connection(runtime, context),
                            Err(error) => self.settings.report_action_error(error.to_string()),
                        }
                    }
                }
            }
        }
    }

    fn preserve_profiling_preference(&self, mut action: SettingsAction) -> SettingsAction {
        let enabled = self
            .runtime
            .profiling_requested_override
            .or(self.runtime.applied_profiling_preference)
            .unwrap_or_else(|| {
                self.settings
                    .active()
                    .get(shoop_egui::RUNTIME_PROFILING)
                    .unwrap_or(false)
            });
        match &mut action {
            SettingsAction::Save(draft)
            | SettingsAction::SaveTrackDefaults { draft, .. }
            | SettingsAction::RequestAudioDriverSwitch { draft, .. } => {
                draft.set(shoop_egui::RUNTIME_PROFILING, enabled);
            }
            _ => {}
        }
        action
    }

    fn request_profiling(&mut self, enabled: bool) {
        if let Err(error) = self.runtime.dispatch(AppIntent::SetProfiling(enabled)) {
            self.settings.report_action_error(error.to_string());
            return;
        }
        self.runtime.profiling_requested_override = Some(enabled);
        self.runtime.applied_profiling_preference = Some(enabled);
        self.pending_profiling_preference = Some(enabled);
    }

    fn save_profiling_preference(&mut self) {
        if self.settings.view().persistence == shoop_settings::SettingsPersistenceState::Saving {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.audio_settings_busy() || !self.reconcile_audio_settings(&self.runtime.snapshot()) {
            return;
        }
        let Some(enabled) = self.pending_profiling_preference.take() else {
            return;
        };
        let active = self.settings.active();
        if active.get(shoop_egui::RUNTIME_PROFILING).unwrap_or(false) == enabled {
            return;
        }
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&active);
        draft.set(shoop_egui::RUNTIME_PROFILING, enabled);
        if let Err(error) = self.settings.request_save(draft) {
            self.settings.report_action_error(error.to_string());
        }
    }

    fn handle_ui_intent(&mut self, intent: AppIntent) {
        #[cfg(not(target_arch = "wasm32"))]
        if shoop_app_client::runtime::writes_audio_preferences(&intent)
            && self.settings.view().persistence == shoop_settings::SettingsPersistenceState::Saving
        {
            self.settings.report_action_error(
                "Wait for the settings save to finish before updating audio preferences",
            );
            return;
        }
        let kind = intent.kind();
        let _span = tracing::debug_span!("frontend.egui.intent_dispatch", intent = kind).entered();
        if let Err(error) = self.runtime.dispatch(intent) {
            tracing::error!(error = %error, "frontend.egui.intent_dispatch_failed");
        }
    }

    fn handle_file_request(&mut self, request: application::ApplicationFileRequest) {
        use application::ApplicationFileRequestKind as Kind;
        if self.runtime.snapshot().io_task.as_ref().is_none_or(|task| {
            task.id != request.task_id || task.status != shoop_egui::IoTaskStatus::Running
        }) {
            return;
        }
        if request.kind == Kind::SessionUrl {
            self.session_url_task = Some(request.task_id);
            self.session_url_prompt_open = true;
            return;
        }
        if let Some(source) = request.source.as_ref().filter(|source| is_http_url(source)) {
            self.fetch_session_url_for_task(source.clone(), request.task_id);
            return;
        }
        let Some(lease) = self.reserve_file_input(request.task_id) else {
            return;
        };
        #[cfg(not(target_arch = "wasm32"))]
        request_file_input(request, self.pending_file_intent_tx.clone(), lease);
        #[cfg(target_arch = "wasm32")]
        request_file_input(request, Rc::clone(&self.pending_file_intents), lease);
    }
}

fn is_lua_file_name(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("lua"))
}

fn load_ephemeral_script_bytes(
    name: String,
    bytes: &[u8],
) -> anyhow::Result<(String, std::sync::Arc<str>)> {
    if !is_lua_file_name(&name) {
        anyhow::bail!("{name} is not a Lua file");
    }
    let source = std::str::from_utf8(bytes)
        .map_err(|error| anyhow::anyhow!("could not decode {name} as UTF-8: {error}"))?;
    shoop_scripting::LuaRuntime::new()?.check_syntax(&name, source)?;
    Ok((name, std::sync::Arc::from(source)))
}

#[cfg(not(target_arch = "wasm32"))]
fn load_ephemeral_script_path(
    path: &Path,
) -> anyhow::Result<(String, std::sync::Arc<str>, String)> {
    let name = file_name(path);
    let bytes = std::fs::read(path)
        .map_err(|error| anyhow::anyhow!("could not read {}: {error}", path.display()))?;
    let (name, source) = load_ephemeral_script_bytes(name, &bytes)?;
    Ok((name, source, path.to_string_lossy().into_owned()))
}

#[cfg(not(target_arch = "wasm32"))]
fn save_file_output(
    output: application::ApplicationFileOutput,
    sender: Sender<AppIntent>,
    lease: std::sync::Arc<()>,
) {
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(&output.suggested_name)
        .save_file()
    else {
        let _ = sender.send(AppIntent::CompleteIoOutput {
            task_id: output.task_id,
            outcome: shoop_egui::FileOutputOutcome::Cancelled,
        });
        return;
    };
    std::thread::spawn(move || {
        let _lease = lease;
        let result = match atomic_replace(&path, &output.bytes, output.task_id.raw()) {
            Ok(()) => AppIntent::CompleteIoOutput {
                task_id: output.task_id,
                outcome: shoop_egui::FileOutputOutcome::Saved,
            },
            Err(error) => AppIntent::FailIoTask {
                task_id: output.task_id,
                message: format!("Could not save {}: {error}", path.display()),
            },
        };
        let _ = sender.send(result);
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn is_http_url(source: &str) -> bool {
    ["https://", "http://"].iter().any(|scheme| {
        source
            .get(..scheme.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(scheme))
            && source[scheme.len()..]
                .split(['/', '?', '#'])
                .next()
                .is_some_and(|authority| !authority.is_empty())
    })
}

fn session_source_name(source: &str) -> String {
    source
        .split(['?', '#'])
        .next()
        .and_then(|path| path.rsplit('/').next())
        .filter(|name| !name.is_empty())
        .unwrap_or("downloaded-session.shoop")
        .to_owned()
}

fn reserve_file_bytes(bytes: &mut Vec<u8>, additional: usize, limit: u64) -> anyhow::Result<()> {
    let required = bytes
        .len()
        .checked_add(additional)
        .filter(|n| *n as u64 <= limit)
        .ok_or_else(|| anyhow::anyhow!("file input exceeds its byte budget"))?;
    if required > bytes.capacity() {
        let capacity = required
            .max(bytes.capacity().saturating_mul(2))
            .min(usize::try_from(limit).unwrap_or(usize::MAX));
        bytes.try_reserve_exact(capacity - bytes.len())?;
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn read_session_input(path: &Path, lease: &platform_input::InputLease) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let limit = shoop_app_protocol::persistence::MAX_SESSION_FILE_BYTES;
    let metadata = file.metadata()?;
    anyhow::ensure!(metadata.is_file(), "session input must be a regular file");
    if metadata.len() > limit {
        anyhow::bail!("file exceeds the 512 MiB input limit");
    }
    let mut bytes = Vec::new();
    let mut reader = file.take(limit + 1);
    let mut chunk = [0; 64 * 1024];
    loop {
        lease.check()?;
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        anyhow::ensure!(
            bytes.len() as u64 + n as u64 <= limit,
            "file exceeds the 512 MiB input limit"
        );
        reserve_file_bytes(&mut bytes, n, limit)?;
        bytes.extend_from_slice(&chunk[..n]);
    }
    if bytes.len() as u64 > limit {
        anyhow::bail!("file exceeds the 512 MiB input limit");
    }
    Ok(bytes)
}
#[cfg(not(target_arch = "wasm32"))]
fn request_file_input(
    request: application::ApplicationFileRequest,
    sender: Sender<AppIntent>,
    lease: platform_input::InputLease,
) {
    use application::ApplicationFileRequestKind as Kind;
    let path = if let Some(source) = request.source {
        Some(std::path::PathBuf::from(source))
    } else {
        let dialog = rfd::FileDialog::new();
        match request.kind {
            Kind::Audio => dialog.add_filter("Loop audio", &["shoop-audio", "wav"]),
            Kind::Midi => dialog.add_filter("Loop MIDI", &["shoop-midi", "mid"]),
            _ => dialog.add_filter("Shoop session", &["shoop"]),
        }
        .pick_file()
    };
    let Some(path) = path else {
        let _ = sender.send(AppIntent::CancelIoTask {
            task_id: request.task_id,
        });
        return;
    };
    std::thread::spawn(move || {
        let intent = match read_session_input(&path, &lease) {
            Ok(bytes) => AppIntent::CompleteIoInput {
                task_id: request.task_id,
                name: file_name(&path),
                bytes: bytes.into(),
            },
            Err(error) => AppIntent::FailIoTask {
                task_id: request.task_id,
                message: format!("Could not read {}: {error}", path.display()),
            },
        };
        let _ = sender.send(intent);
    });
}
#[cfg(target_arch = "wasm32")]
fn request_file_input(
    request: application::ApplicationFileRequest,
    pending: Rc<RefCell<VecDeque<AppIntent>>>,
    lease: platform_input::InputLease,
) {
    use application::ApplicationFileRequestKind as Kind;
    wasm_bindgen_futures::spawn_local(async move {
        let result: anyhow::Result<Option<(String, Vec<u8>)>> = async {
            anyhow::ensure!(
                request.source.is_none(),
                "filesystem paths are unavailable in the browser; choose a file instead"
            );
            let dialog = rfd::AsyncFileDialog::new();
            let file = match request.kind {
                Kind::Audio => dialog.add_filter("Loop audio", &["shoop-audio", "wav"]),
                Kind::Midi => dialog.add_filter("Loop MIDI", &["shoop-midi", "mid"]),
                _ => dialog.add_filter("Shoop session", &["shoop"]),
            }
            .pick_file()
            .await;
            let Some(file) = file else { return Ok(None) };
            lease.check()?;
            let size = file.inner().size() as u64;
            anyhow::ensure!(
                size <= shoop_app_protocol::persistence::MAX_SESSION_FILE_BYTES,
                "file exceeds the 512 MiB input limit"
            );
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(size as usize)?;
            for offset in (0..size).step_by(64 * 1024) {
                lease.check()?;
                let blob = file
                    .inner()
                    .slice_with_f64_and_f64(offset as f64, (offset + 64 * 1024).min(size) as f64)
                    .map_err(|e| anyhow::anyhow!("could not read file slice: {e:?}"))?;
                let buffer = wasm_bindgen_futures::JsFuture::from(blob.array_buffer())
                    .await
                    .map_err(|e| anyhow::anyhow!("could not read file: {e:?}"))?;
                let chunk = js_sys::Uint8Array::new(&buffer);
                let start = bytes.len();
                bytes.resize(start + chunk.length() as usize, 0);
                chunk.copy_to(&mut bytes[start..]);
            }
            Ok(Some((file.file_name(), bytes)))
        }
        .await;
        let intent = match result {
            Ok(Some((name, bytes))) => AppIntent::CompleteIoInput {
                task_id: request.task_id,
                name,
                bytes: bytes.into(),
            },
            Ok(None) => AppIntent::CancelIoTask {
                task_id: request.task_id,
            },
            Err(error) => AppIntent::FailIoTask {
                task_id: request.task_id,
                message: error.to_string(),
            },
        };
        pending.borrow_mut().push_back(intent);
    });
}

#[cfg(target_arch = "wasm32")]
async fn read_browser_session_url(
    url: &str,
    mut download: SessionDownload,
    lease: &platform_input::InputLease,
) -> anyhow::Result<AppIntent> {
    use ehttp::streaming::Part;
    use futures_util::StreamExt;
    lease.check()?;
    let request = ehttp::Request::get(url);
    let stream = ehttp::streaming::fetch_async_streaming(&request)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    futures_util::pin_mut!(stream);
    while let Some(part) = stream.next().await {
        lease.check()?;
        if matches!(&part, Ok(Part::Chunk(chunk)) if chunk.is_empty()) {
            continue;
        }
        if let Some(intent) = download.part(part)? {
            return Ok(intent);
        }
    }
    lease.check()?;
    download
        .part(Ok(Part::Chunk(Vec::new())))?
        .ok_or_else(|| anyhow::anyhow!("session download did not complete"))
}

struct SessionDownload {
    task_id: shoop_egui::TaskId,
    name: String,
    bytes: Vec<u8>,
}
impl SessionDownload {
    fn part(
        &mut self,
        part: ehttp::Result<ehttp::streaming::Part>,
    ) -> anyhow::Result<Option<AppIntent>> {
        use ehttp::streaming::Part;
        let limit = shoop_app_protocol::persistence::MAX_SESSION_FILE_BYTES;
        match part.map_err(|e| anyhow::anyhow!("Could not fetch session: {e}"))? {
            Part::Response(response) => {
                anyhow::ensure!(
                    response.ok,
                    "Could not fetch session: HTTP {}",
                    response.status
                );
                if let Some(length) = response
                    .headers
                    .get("content-length")
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    anyhow::ensure!(length <= limit, "download exceeds the 512 MiB input limit");
                }
                Ok(None)
            }
            Part::Chunk(chunk) if chunk.is_empty() => Ok(Some(AppIntent::CompleteIoInput {
                task_id: self.task_id,
                name: std::mem::take(&mut self.name),
                bytes: std::mem::take(&mut self.bytes).into(),
            })),
            Part::Chunk(chunk) => {
                anyhow::ensure!(
                    (self.bytes.len() as u64).saturating_add(chunk.len() as u64) <= limit,
                    "download exceeds the 512 MiB input limit"
                );
                reserve_file_bytes(&mut self.bytes, chunk.len(), limit)?;
                self.bytes.extend_from_slice(&chunk);
                Ok(None)
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn atomic_replace(path: &Path, bytes: &[u8], task_id: u64) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(1);
    let extension = path.extension().unwrap_or_default().to_string_lossy();
    let mut opened = None;
    for _ in 0..16 {
        let serial = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let temporary = path.with_extension(format!(
            "{extension}.tmp-{task_id}-{}-{serial}",
            std::process::id()
        ));
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
        {
            Ok(file) => {
                opened = Some((temporary, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let (temporary, mut file) = opened.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not reserve a unique temporary output file",
        )
    })?;
    let mut renamed = false;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        renamed = true;
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            let parent = if parent.as_os_str().is_empty() {
                Path::new(".")
            } else {
                parent
            };
            std::fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| {
                    std::io::Error::other(format!(
                        "file replacement completed, but directory synchronization failed: {error}"
                    ))
                })?;
        }
        Ok(())
    })();
    if result.is_err() && !renamed {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(target_arch = "wasm32")]
async fn write_browser_file(
    file: &rfd::FileHandle,
    bytes: &[u8],
) -> anyhow::Result<shoop_egui::FileOutputOutcome> {
    use wasm_bindgen::{closure::Closure, JsCast};
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| anyhow::anyhow!("browser document is unavailable"))?;
    let handed_off = Rc::new(std::cell::Cell::new(false));
    let clicked = Rc::clone(&handed_off);
    let download_url = Rc::new(RefCell::new(None));
    let observed_url = Rc::clone(&download_url);
    let listener = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        let Some(target) = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        else {
            return;
        };
        let Some(card) = target.closest("#rfd-card").ok().flatten() else {
            return;
        };
        if let Some(output) = card.query_selector("#rfd-output").ok().flatten() {
            *observed_url.borrow_mut() = output.get_attribute("href");
            clicked.set(target.closest("#rfd-output").ok().flatten().is_some());
        }
    });
    document
        .add_event_listener_with_callback_and_bool("click", listener.as_ref().unchecked_ref(), true)
        .map_err(|error| anyhow::anyhow!("could not observe browser download: {error:?}"))?;
    let result = file.write(bytes).await;
    let removed = document.remove_event_listener_with_callback_and_bool(
        "click",
        listener.as_ref().unchecked_ref(),
        true,
    );
    if let Err(error) = removed {
        listener.forget();
        anyhow::bail!("could not release browser download listener: {error:?}");
    }
    if handed_off.get() {
        let window = web_sys::window().unwrap();
        let turn = js_sys::Promise::new(&mut |resolve, reject| {
            if let Err(error) =
                window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 0)
            {
                let _ = reject.call1(&wasm_bindgen::JsValue::UNDEFINED, &error);
            }
        });
        wasm_bindgen_futures::JsFuture::from(turn)
            .await
            .map_err(|error| {
                anyhow::anyhow!("download was handed off but cleanup could not finish: {error:?}")
            })?;
    }
    if let Some(url) = download_url.borrow().as_ref() {
        web_sys::Url::revoke_object_url(url)
            .map_err(|error| anyhow::anyhow!("could not release browser download: {error:?}"))?;
    }
    result?;
    Ok(if handed_off.get() {
        shoop_egui::FileOutputOutcome::DownloadHandedOff
    } else {
        shoop_egui::FileOutputOutcome::Cancelled
    })
}

#[cfg(target_arch = "wasm32")]
fn save_file_output(
    output: application::ApplicationFileOutput,
    pending: Rc<RefCell<VecDeque<AppIntent>>>,
    lease: std::sync::Arc<()>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let _lease = lease;
        let result = if let Some(file) = rfd::AsyncFileDialog::new()
            .set_file_name(&output.suggested_name)
            .save_file()
            .await
        {
            match write_browser_file(&file, &output.bytes).await {
                Ok(outcome) => AppIntent::CompleteIoOutput {
                    task_id: output.task_id,
                    outcome,
                },
                Err(error) => AppIntent::FailIoTask {
                    task_id: output.task_id,
                    message: format!("Could not save browser file: {error}"),
                },
            }
        } else {
            AppIntent::CompleteIoOutput {
                task_id: output.task_id,
                outcome: shoop_egui::FileOutputOutcome::Cancelled,
            }
        };
        pending.borrow_mut().push_back(result);
    });
}

impl eframe::App for UnifiedApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.process_tracing();
        #[cfg(target_arch = "wasm32")]
        if self.tracing.status().active || self.pending_tracing_action.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
        self.show(ui);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
const TEST_KEYBOARD_SCRIPT: &str = unsafe {
    std::str::from_utf8_unchecked(include_bytes!(
        "../../../../resources/builtins/keyboard.lua"
    ))
};
#[cfg(all(test, not(target_arch = "wasm32")))]
const TEST_DIALOG_SCRIPT: &str = unsafe {
    std::str::from_utf8_unchecked(include_bytes!(
        "../../../../resources/builtins/examples/dialogs.lua"
    ))
};

#[cfg(any(test, target_arch = "wasm32"))]
const KEYBOARD_SCRIPT_FILENAME: &str = "keyboard.lua";
#[cfg(any(test, target_arch = "wasm32"))]
const APC_MINI_SCRIPT_FILENAME: &str = "akai_apc_mini_mk1.lua";

#[cfg(not(target_arch = "wasm32"))]
fn configured_catalog_scripts(
    settings: &shoop_settings::SettingsSnapshot,
    generation: u64,
) -> Result<(Vec<StartupScript>, Vec<String>, Vec<String>, bool), String> {
    let root = settings
        .get(shoop_egui::BUILTINS_LOCATION)
        .map_err(|error| error.to_string())?;
    if root.trim().is_empty() {
        return Err("built-ins location must not be empty".to_owned());
    }
    let enabled = settings
        .get(shoop_egui::BUILTIN_SCRIPTS)
        .map_err(|error| error.to_string())?
        .0
        .into_iter()
        .map(|entry| (entry.value, entry.enabled))
        .collect::<std::collections::BTreeMap<_, _>>();
    let catalog = shoop_script_resources::scan_builtin_directory(
        std::path::Path::new(&root),
        generation,
        shoop_script_resources::ResourceLimits::default(),
    )
    .map_err(|error| error.to_string())?;
    let syntax = shoop_scripting::LuaRuntime::new().map_err(|error| error.to_string())?;
    let deletions_safe = catalog.deletions_safe;
    let mut preserve_identities = catalog
        .diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.path.as_deref())
        .filter_map(|path| shoop_script_resources::NormalizedRelativePath::parse(path).ok())
        .filter(|path| {
            shoop_script_resources::classify_resource(path)
                == Some(shoop_script_resources::ResourceKind::Lua)
        })
        .map(|path| path.to_string())
        .collect::<Vec<_>>();
    let mut warnings = catalog
        .diagnostics
        .into_iter()
        .map(|diagnostic| match diagnostic.path {
            Some(path) => format!("{path}: {}", diagnostic.message),
            None => diagnostic.message,
        })
        .collect::<Vec<_>>();
    let mut scripts = Vec::new();
    for entry in catalog.entries {
        let identity = entry.identity.to_string();
        let name = entry.identity.file_name().to_owned();
        if let Err(error) = syntax.check_syntax(&name, &entry.source) {
            warnings.push(format!("{identity}: {error}"));
            preserve_identities.push(identity);
            continue;
        }
        let kind = if identity.starts_with("examples/") {
            ScriptKind::Example
        } else {
            ScriptKind::Bundled
        };
        let enabled =
            kind == ScriptKind::Bundled && enabled.get(&identity).copied().unwrap_or(false);
        scripts.push(StartupScript {
            name,
            identity: Some(identity.clone()),
            source: entry.source.to_string(),
            source_path: Some(
                std::path::Path::new(&root)
                    .join(entry.identity.as_str())
                    .to_string_lossy()
                    .into_owned(),
            ),
            kind,
            enabled,
        });
    }
    let valid = scripts
        .iter()
        .filter_map(|script| script.identity.as_deref())
        .map(str::to_lowercase)
        .collect::<std::collections::BTreeSet<_>>();
    preserve_identities.retain(|identity| !valid.contains(&identity.to_lowercase()));
    preserve_identities.sort();
    preserve_identities.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    Ok((scripts, warnings, preserve_identities, deletions_safe))
}

#[cfg(not(target_arch = "wasm32"))]
fn configured_startup_scripts(
    settings: &shoop_settings::SettingsSnapshot,
) -> anyhow::Result<(Vec<StartupScript>, Vec<String>, Vec<String>)> {
    let (mut scripts, mut warnings, _, _) = match configured_catalog_scripts(settings, 1) {
        Ok(result) => result,
        Err(error) => (
            Vec::new(),
            vec![format!("could not scan built-in scripts: {error}")],
            Vec::new(),
            false,
        ),
    };
    let mut identities = scripts
        .iter()
        .filter_map(|script| script.identity.clone())
        .collect::<Vec<_>>();
    for configured in settings.get(shoop_egui::USER_SCRIPTS)?.0 {
        match read_user_script(&configured.value) {
            Ok((name, source)) => {
                identities.push(configured.value.clone());
                scripts.push(StartupScript {
                    name,
                    identity: None,
                    source,
                    source_path: Some(configured.value.clone()),
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
fn script_kind_has_file_identity(kind: ScriptKind) -> bool {
    matches!(
        kind,
        ScriptKind::Bundled | ScriptKind::Example | ScriptKind::User
    )
}

fn builtin_script_settings(
    settings: &shoop_settings::SettingsSnapshot,
) -> (String, shoop_settings::StringToggleList) {
    (
        settings
            .get(shoop_egui::BUILTINS_LOCATION)
            .unwrap_or_default(),
        settings
            .get(shoop_egui::BUILTIN_SCRIPTS)
            .unwrap_or_default(),
    )
}

fn reconcile_profiling_settings(
    settings: &shoop_settings::SettingsSnapshot,
    applied: &mut Option<bool>,
    requested_override: &mut Option<bool>,
    mut dispatch: impl FnMut(AppIntent) -> Result<(), application::DispatchError>,
) -> anyhow::Result<()> {
    let stored = settings.get(shoop_egui::RUNTIME_PROFILING).unwrap_or(false);
    if *requested_override == Some(stored) {
        *requested_override = None;
    }
    let enabled = requested_override.unwrap_or(stored);
    if *applied != Some(enabled) {
        dispatch(AppIntent::SetProfiling(enabled))?;
        *applied = Some(enabled);
    }
    Ok(())
}

fn reconcile_loop_smoothing_settings(
    settings: &shoop_settings::SettingsSnapshot,
    applied_revision: &mut u64,
    mut dispatch: impl FnMut(AppIntent) -> Result<(), application::DispatchError>,
) -> anyhow::Result<()> {
    if settings.revision() == *applied_revision {
        return Ok(());
    }
    let milliseconds = match shoop_egui::loop_edge_smoothing_ms(settings) {
        Ok(milliseconds) => milliseconds,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "frontend.audio.loop_smoothing_settings_fallback"
            );
            3
        }
    };
    let master_auto_connect = match shoop_egui::master_auto_connect(settings) {
        Ok(enabled) => enabled,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "frontend.audio.master_auto_connect_settings_fallback"
            );
            true
        }
    };
    dispatch(AppIntent::SetLoopSmoothingMs(milliseconds))?;
    dispatch(AppIntent::SetMasterAutoConnect(master_auto_connect))?;
    *applied_revision = settings.revision();
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
struct Runtime {
    _runtime: ApplicationRuntime,
    handle: ApplicationHandle,
    script_paths: std::collections::BTreeMap<shoop_egui::ScriptId, String>,
    pending_script_paths: std::collections::VecDeque<(String, ScriptKind, String)>,
    catalog_scan_generation: u64,
    catalog_scan_tx: std::sync::mpsc::Sender<CatalogScanCompletion>,
    catalog_scan_rx: std::sync::mpsc::Receiver<CatalogScanCompletion>,
    preview_player: native_preview::NativePreviewPlayer,
    system_preview_request: Option<u64>,
    pending_preview: Option<application::ApplicationAudioPreview>,
    preview_availability: std::cell::Cell<Option<(Instant, bool)>>,
    applied_audio_settings_revision: u64,
    applied_profiling_preference: Option<bool>,
    profiling_requested_override: Option<bool>,
    applied_settings_revision: u64,
    applied_builtin_settings: (String, shoop_settings::StringToggleList),
    applied_user_scripts: shoop_settings::StringToggleList,
    applied_audio_preferences: (u32, bool),
    /// The new engine backend's host. It is declared after the application runtime so that
    /// it stops after the backend is gone.
    _engine_host: Option<engine_backend::EngineHost>,
    remote_entry: Option<shoop_host::remote::Target>,
}

#[cfg(not(target_arch = "wasm32"))]
type CatalogScanCompletion = (
    u64,
    Result<(Vec<StartupScript>, Vec<String>, Vec<String>, bool), String>,
);

#[cfg(not(target_arch = "wasm32"))]
impl Runtime {
    fn new(settings: &shoop_settings::SettingsSnapshot, args: &AppArgs) -> anyhow::Result<Self> {
        Self::new_with_engine_start(
            settings,
            args,
            |driver, pools, transport| {
                engine_backend::start_selected_with_processors(
                    driver,
                    pools,
                    transport,
                    args.configured_processors(settings)?,
                )
            },
            None,
        )
    }

    fn new_with_engine_start(
        settings: &shoop_settings::SettingsSnapshot,
        args: &AppArgs,
        start_engine: impl FnOnce(
            &AudioDriverConfig,
            &shoop_egui::NewEnginePools,
            shoop_egui::EngineTransportKind,
        ) -> anyhow::Result<engine_backend::NewEngine>,
        preferences: Option<shoop_host::audio_driver::PreferenceWriter>,
    ) -> anyhow::Result<Self> {
        if let Some(target) = args.host_target()? {
            return Self::attach(settings, target);
        }
        let mut startup_warnings = Vec::new();
        let configured_driver = match args.configured_audio_driver(settings) {
            Ok(driver) => driver,
            Err(error) if args.audio.audio_driver.is_some() => anyhow::bail!(error),
            Err(error) => {
                startup_warnings.push(format!(
                    "Could not use preferred audio settings: {error}; using dummy / offline"
                ));
                AudioDriverConfig::default()
            }
        };
        let pools = shoop_egui::new_engine_pools_from_snapshot(settings).unwrap_or_else(|error| {
            startup_warnings.push(format!(
                "Could not use engine capacities: {error}; using defaults"
            ));
            shoop_egui::NewEnginePools::default()
        });
        let transport = args.selected_engine_transport(settings)?;
        let started = start_engine(&configured_driver, &pools, transport)?;
        startup_warnings.extend(started.warnings);
        let mut engine_host = Some(started.host);
        let backend = started.backend;
        let (loop_smoothing_ms, loop_smoothing_warning) =
            match shoop_egui::loop_edge_smoothing_ms(settings) {
                Ok(milliseconds) => (milliseconds, None),
                Err(error) => (
                    3,
                    Some(format!(
                        "Could not use loop edge smoothing setting: {error}; using 3 ms"
                    )),
                ),
            };
        let (startup_scripts, script_paths, mut warnings) = configured_startup_scripts(settings)?;
        warnings.extend(loop_smoothing_warning);
        let master_auto_connect = match shoop_egui::master_auto_connect(settings) {
            Ok(enabled) => enabled,
            Err(error) => {
                let warning = format!(
                    "Could not use Master auto-connect setting: {error}; using automatic connection"
                );
                warnings.push(warning);
                true
            }
        };
        let graph = engine_host
            .as_ref()
            .and_then(|host| host.graph_controller());
        #[cfg(not(test))]
        let preferences = match preferences {
            Some(preferences) => preferences,
            None => {
                shoop_settings::default_settings_path().map(settings::driver_preference_writer)?
            }
        };
        #[cfg(test)]
        let preferences = preferences
            .unwrap_or_else(|| Box::new(|_| anyhow::bail!("test runtime has no preference store")));
        let driver = engine_host
            .as_mut()
            .unwrap()
            .enable_audio_driver_switching(
                shoop_app_api::HostSettings {
                    driver: configured_driver,
                    pools,
                    processors: args.configured_processors(settings)?,
                    master_auto_connect,
                    loop_smoothing_ms,
                },
                preferences,
            )?;
        let runtime = ApplicationRuntime::start_with_options(
            backend,
            application::ApplicationStartOptions {
                startup_scripts,
                loop_smoothing_ms,
                master_auto_connect,
                startup_warnings,
            },
            graph,
            Some(driver),
        )?;
        let handle = runtime.handle();
        let preview_player = native_preview::NativePreviewPlayer::new(handle.clone())?;
        for warning in warnings {
            tracing::warn!(warning = %warning, "frontend.startup.fallback");
        }
        let script_paths =
            associate_startup_script_paths(runtime.startup_script_ids(), script_paths);
        let (catalog_scan_tx, catalog_scan_rx) = std::sync::mpsc::channel();
        Ok(Self {
            _runtime: runtime,
            handle,
            script_paths,
            pending_script_paths: std::collections::VecDeque::new(),
            catalog_scan_generation: 1,
            catalog_scan_tx,
            catalog_scan_rx,
            preview_player,
            system_preview_request: None,
            pending_preview: None,
            preview_availability: std::cell::Cell::new(None),
            applied_audio_settings_revision: settings.revision(),
            applied_profiling_preference: None,
            profiling_requested_override: None,
            applied_settings_revision: settings.revision(),
            applied_builtin_settings: builtin_script_settings(settings),
            applied_user_scripts: settings.get(shoop_egui::USER_SCRIPTS).unwrap_or_default(),
            applied_audio_preferences: (
                shoop_egui::loop_edge_smoothing_ms(settings).unwrap_or(3),
                shoop_egui::master_auto_connect(settings).unwrap_or(true),
            ),
            _engine_host: engine_host,
            remote_entry: None,
        })
    }

    fn attach(
        settings: &shoop_settings::SettingsSnapshot,
        entry: impl Into<shoop_host::remote::Target>,
    ) -> anyhow::Result<Self> {
        let entry = entry.into();
        let entry = if entry.process.is_none() {
            shoop_host::connection::resolve_target(&entry, Duration::from_secs(3))?
        } else {
            entry
        };
        let (startup_scripts, _paths, warnings) = configured_startup_scripts(settings)?;
        let pending_script_paths = startup_scripts
            .iter()
            .filter_map(|script| {
                script
                    .source_path
                    .clone()
                    .map(|path| (script.name.clone(), script.kind, path))
            })
            .collect();
        let runtime = ApplicationRuntime::attach(
            entry.clone(),
            application::ApplicationStartOptions {
                startup_scripts,
                startup_warnings: warnings,
                ..Default::default()
            },
        )?;
        let handle = runtime.handle();
        let preview_player = native_preview::NativePreviewPlayer::new(handle.clone())?;
        let (catalog_scan_tx, catalog_scan_rx) = std::sync::mpsc::channel();
        Ok(Self {
            _runtime: runtime,
            handle,
            script_paths: Default::default(),
            pending_script_paths,
            catalog_scan_generation: 1,
            catalog_scan_tx,
            catalog_scan_rx,
            preview_player,
            system_preview_request: None,
            pending_preview: None,
            preview_availability: std::cell::Cell::new(None),
            applied_audio_settings_revision: settings.revision(),
            applied_profiling_preference: None,
            profiling_requested_override: None,
            applied_settings_revision: settings.revision(),
            applied_builtin_settings: builtin_script_settings(settings),
            applied_user_scripts: settings.get(shoop_egui::USER_SCRIPTS).unwrap_or_default(),
            applied_audio_preferences: (
                shoop_egui::loop_edge_smoothing_ms(settings).unwrap_or(3),
                shoop_egui::master_auto_connect(settings).unwrap_or(true),
            ),
            _engine_host: None,
            remote_entry: Some(entry),
        })
    }

    fn tick(&mut self, _elapsed: Duration) {
        while let Ok((generation, result)) = self.catalog_scan_rx.try_recv() {
            if generation != self.catalog_scan_generation {
                continue;
            }
            match result {
                Ok((scripts, warnings, preserve_identities, deletions_safe)) => {
                    for warning in warnings {
                        tracing::warn!(warning = %warning, "frontend.scripting.catalog_scan_warning");
                    }
                    if !deletions_safe {
                        tracing::warn!("frontend.scripting.catalog_scan_incomplete");
                        continue;
                    }
                    let descriptors = scripts
                        .iter()
                        .filter_map(|script| {
                            script.identity.clone().map(|identity| {
                                shoop_egui::CatalogScriptSource {
                                    identity,
                                    name: script.name.clone(),
                                    source: std::sync::Arc::from(script.source.as_str()),
                                    source_path: script.source_path.clone(),
                                    resource_bundle: None,
                                    kind: script.kind,
                                    enabled: script.enabled,
                                }
                            })
                        })
                        .collect::<Vec<_>>();
                    if self
                        .handle
                        .dispatch(AppIntent::ReconcileCatalogScripts {
                            scripts: descriptors.into(),
                            preserve_identities: preserve_identities.into(),
                        })
                        .is_ok()
                    {
                        for script in scripts {
                            if let Some(identity) = script.identity {
                                self.pending_script_paths.push_back((
                                    script.name,
                                    script.kind,
                                    identity,
                                ));
                            }
                        }
                    }
                }
                Err(error) => {
                    tracing::error!(error = %error, "frontend.scripting.catalog_scan_failed");
                }
            }
        }
        let snapshot = self.handle.snapshot();
        self.script_paths.retain(|script_id, _| {
            snapshot
                .scripting
                .scripts
                .iter()
                .any(|script| script.id == *script_id && script_kind_has_file_identity(script.kind))
        });
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

    fn reconcile_audio_settings(
        &mut self,
        settings: &shoop_settings::SettingsSnapshot,
    ) -> anyhow::Result<()> {
        let handle = self.handle.clone();
        reconcile_profiling_settings(
            settings,
            &mut self.applied_profiling_preference,
            &mut self.profiling_requested_override,
            move |intent| handle.dispatch(intent),
        )?;
        if self.remote_entry.is_some() {
            self.applied_audio_settings_revision = settings.revision();
            return Ok(());
        }
        let preferences = (
            shoop_egui::loop_edge_smoothing_ms(settings).unwrap_or(3),
            shoop_egui::master_auto_connect(settings).unwrap_or(true),
        );
        if preferences == self.applied_audio_preferences {
            self.applied_audio_settings_revision = settings.revision();
            return Ok(());
        }
        let handle = self.handle.clone();
        reconcile_loop_smoothing_settings(
            settings,
            &mut self.applied_audio_settings_revision,
            move |intent| handle.dispatch(intent),
        )?;
        self.applied_audio_preferences = preferences;
        Ok(())
    }

    fn reconcile_script_settings(
        &mut self,
        settings: &shoop_settings::SettingsSnapshot,
    ) -> anyhow::Result<()> {
        if settings.revision() == self.applied_settings_revision {
            return Ok(());
        }
        let builtins = builtin_script_settings(settings);
        if builtins != self.applied_builtin_settings {
            self.request_builtin_rescan(settings);
            self.applied_builtin_settings = builtins;
        }
        let user_scripts = settings.get(shoop_egui::USER_SCRIPTS)?;
        if user_scripts == self.applied_user_scripts {
            self.applied_settings_revision = settings.revision();
            return Ok(());
        }
        let mut desired = std::collections::BTreeMap::new();
        let mut warnings = Vec::new();
        for configured in settings.get(shoop_egui::USER_SCRIPTS)?.0 {
            match read_user_script(&configured.value) {
                Ok((name, source)) => {
                    desired.insert(
                        configured.value.clone(),
                        StartupScript {
                            name,
                            identity: None,
                            source,
                            source_path: Some(configured.value),
                            kind: ScriptKind::User,
                            enabled: configured.enabled,
                        },
                    );
                }
                Err(error) => warnings.push(error.to_string()),
            }
        }
        for warning in warnings {
            tracing::warn!(warning = %warning, "frontend.scripting.user_script_warning");
        }
        let snapshot = self.handle.snapshot();
        for script in snapshot
            .scripting
            .scripts
            .iter()
            .filter(|script| script.kind == ScriptKind::User)
        {
            if self
                .script_paths
                .get(&script.id)
                .is_some_and(|path| !desired.contains_key(path))
            {
                self.handle.dispatch(AppIntent::ForgetScript {
                    script_id: script.id,
                })?;
                self.script_paths.remove(&script.id);
            }
        }
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
                self.handle.dispatch(AppIntent::AddScriptFileSource {
                    name: script.name.clone(),
                    source: script.source.into(),
                    source_path: identity.clone(),
                    kind: script.kind,
                    enabled: script.enabled,
                })?;
                self.pending_script_paths
                    .push_back((script.name, script.kind, identity));
            }
        }
        self.applied_settings_revision = settings.revision();
        self.applied_user_scripts = user_scripts;
        Ok(())
    }

    fn request_builtin_rescan(&mut self, settings: &shoop_settings::SettingsSnapshot) {
        self.catalog_scan_generation = self.catalog_scan_generation.saturating_add(1);
        let generation = self.catalog_scan_generation;
        let settings = settings.clone();
        let sender = self.catalog_scan_tx.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("shoop-builtins-scan".to_owned())
            .spawn(move || {
                let result = configured_catalog_scripts(&settings, generation);
                let _ = sender.send((generation, result));
            })
        {
            tracing::error!(error = %error, "frontend.scripting.catalog_scan_start_failed");
        }
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

    fn dispatch(&mut self, intent: AppIntent) -> Result<(), application::DispatchError> {
        self.handle.dispatch(intent)
    }

    fn take_file_output(&self) -> Option<application::ApplicationFileOutput> {
        self.handle.take_file_output()
    }
    fn take_file_request(&self) -> Option<application::ApplicationFileRequest> {
        self.handle.take_file_request()
    }

    fn process_audio_previews(&mut self) {
        let state = self.handle.snapshot();
        if self
            .system_preview_request
            .is_some_and(|id| id != state.click_track.preview_request_id)
            || state.click_track.preview_output != Some(shoop_egui::ClickTrackPreviewOutput::System)
            || !matches!(
                state.click_track.preview_status,
                shoop_egui::ClickTrackPreviewStatus::Queued
                    | shoop_egui::ClickTrackPreviewStatus::Playing
            )
        {
            if let Err(error) = self.preview_player.stop() {
                tracing::warn!(%error, "frontend.preview.stop_failed");
            }
        }
        while let Some(preview) = self
            .pending_preview
            .take()
            .or_else(|| self.handle.take_audio_preview())
        {
            let request_id = preview.request_id;
            let current = self.handle.snapshot();
            // Preview samples can arrive before the actor publishes their queued state.
            if current.click_track.preview_request_id < request_id {
                self.pending_preview = Some(preview);
                break;
            }
            if current.click_track.preview_request_id != request_id
                || current.click_track.preview_status != shoop_egui::ClickTrackPreviewStatus::Queued
            {
                continue;
            }
            if let Err(error) = self.preview_player.stop() {
                let _ = self.handle.dispatch(AppIntent::CompleteClickTrackPreview {
                    request_id,
                    success: false,
                    message: error.to_string(),
                });
                continue;
            }
            match preview.output {
                shoop_egui::ClickTrackPreviewOutput::System => {
                    self.system_preview_request = Some(request_id);
                    self.preview_player.play(preview);
                }
                shoop_egui::ClickTrackPreviewOutput::MasterBus => {
                    if let Err(error) =
                        self.handle
                            .dispatch(AppIntent::StartMasterClickTrackPreview {
                                request_id,
                                sample_rate: preview.sample_rate,
                                samples: Arc::clone(&preview.samples),
                            })
                    {
                        if error == application::DispatchError::Full {
                            self.pending_preview = Some(preview);
                        } else {
                            tracing::warn!(%error, "frontend.preview.actor_disconnected");
                        }
                        break;
                    }
                }
            }
        }
    }

    fn audio_preview_available(&self) -> bool {
        if let Some((checked, available)) = self.preview_availability.get() {
            if checked.elapsed() < Duration::from_secs(1) {
                return available;
            }
        }
        let available = native_preview::is_available();
        self.preview_availability
            .set(Some((Instant::now(), available)));
        available
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(serde::Deserialize)]
struct BrowserBuiltinCatalog {
    format: String,
    version: u16,
    files: Vec<BrowserBuiltinFile>,
}

#[cfg(target_arch = "wasm32")]
#[derive(serde::Deserialize)]
struct BrowserBuiltinFile {
    path: String,
    kind: shoop_script_resources::ResourceKind,
    bytes: u64,
    sha256: String,
}

#[cfg(any(target_arch = "wasm32", test))]
fn embedded_builtin_key(url: &str) -> &str {
    url.trim_start_matches("./")
}

#[cfg(target_arch = "wasm32")]
async fn fetch_browser_bytes(url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast as _;
    use wasm_bindgen_futures::JsFuture;

    let embedded = js_sys::Reflect::get(
        &js_sys::global(),
        &wasm_bindgen::JsValue::from_str("shoopEmbeddedBuiltins"),
    )
    .map_err(|error| format!("could not inspect embedded built-ins: {error:?}"))?;
    if !embedded.is_undefined() {
        let key = embedded_builtin_key(url);
        let value = js_sys::Reflect::get(&embedded, &wasm_bindgen::JsValue::from_str(key))
            .map_err(|error| format!("could not inspect embedded built-in {key}: {error:?}"))?;
        if !value.is_undefined() {
            let bytes = js_sys::Uint8Array::new(&value);
            if bytes.length() as u64 > max_bytes {
                return Err(format!(
                    "embedded built-in {url} is {} bytes; limit is {max_bytes}",
                    bytes.length()
                ));
            }
            return Ok(bytes.to_vec());
        }
    }

    let window = web_sys::window().ok_or_else(|| "browser window is unavailable".to_owned())?;
    let response = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|error| format!("could not fetch {url}: {error:?}"))?
        .dyn_into::<web_sys::Response>()
        .map_err(|_| format!("fetch for {url} returned no response"))?;
    if !response.ok() {
        return Err(format!(
            "fetch for {url} returned HTTP {}",
            response.status()
        ));
    }
    let stream = response
        .body()
        .ok_or_else(|| format!("fetch for {url} returned no body"))?;
    let reader = stream
        .get_reader()
        .dyn_into::<web_sys::ReadableStreamDefaultReader>()
        .map_err(|_| format!("could not create a bounded reader for {url}"))?;
    let mut bytes = Vec::new();
    loop {
        let result = JsFuture::from(reader.read())
            .await
            .map_err(|error| format!("could not read {url}: {error:?}"))?
            .unchecked_into::<web_sys::ReadableStreamReadResult>();
        if result.get_done().unwrap_or(false) {
            break;
        }
        let chunk = js_sys::Uint8Array::new(&result.get_value());
        let next = (bytes.len() as u64)
            .checked_add(chunk.length() as u64)
            .ok_or_else(|| format!("fetch for {url} exceeded its byte limit"))?;
        if next > max_bytes {
            return Err(format!(
                "fetch for {url} is {next} bytes; limit is {max_bytes}"
            ));
        }
        let start = bytes.len();
        bytes.resize(next as usize, 0);
        chunk.copy_to(&mut bytes[start..]);
    }
    Ok(bytes)
}

#[cfg(target_arch = "wasm32")]
async fn fetch_browser_catalog(
    root: String,
    enabled: std::collections::BTreeMap<String, bool>,
) -> Result<
    (
        Vec<shoop_egui::CatalogScriptSource>,
        Vec<String>,
        Vec<String>,
    ),
    String,
> {
    use shoop_script_resources::{
        classify_resource, NormalizedRelativePath, ResourceKind, ResourceLimits, ScriptResource,
        ScriptResourceBundle,
    };

    let root = root.trim_end_matches('/');
    if root.is_empty() {
        return Err("built-ins catalog root must not be empty".to_owned());
    }
    let catalog_bytes = fetch_browser_bytes(
        &format!("{root}/catalog.json"),
        shoop_script_resources::ResourceLimits::default().max_file_bytes,
    )
    .await?;
    let catalog: BrowserBuiltinCatalog = serde_json::from_slice(&catalog_bytes)
        .map_err(|error| format!("built-ins catalog is malformed: {error}"))?;
    if catalog.format != "shoop-builtins-catalog" || catalog.version != 1 {
        return Err("built-ins catalog has an unsupported format or version".to_owned());
    }
    let limits = ResourceLimits::default();
    if catalog.files.len() > limits.max_scan_entries {
        return Err(format!(
            "built-ins catalog has {} entries; limit is {}",
            catalog.files.len(),
            limits.max_scan_entries
        ));
    }
    let mut declarations = Vec::with_capacity(catalog.files.len());
    let mut identities = std::collections::BTreeSet::new();
    let mut total = 0_u64;
    for file in catalog.files {
        let path = NormalizedRelativePath::parse(file.path).map_err(|error| error.to_string())?;
        if classify_resource(&path) != Some(file.kind) {
            return Err(format!("catalog kind does not match {path}"));
        }
        if !identities.insert(path.case_folded()) {
            return Err(format!(
                "catalog has a duplicate/case-colliding path {path}"
            ));
        }
        if file.bytes > limits.max_file_bytes {
            return Err(format!("catalog file {path} exceeds the per-file limit"));
        }
        total = total.saturating_add(file.bytes);
        if total > limits.max_aggregate_bytes {
            return Err("built-ins catalog exceeds the aggregate resource limit".to_owned());
        }
        declarations.push((path, file.kind, file.bytes, file.sha256));
    }
    let mut fetched = std::collections::BTreeMap::new();
    for (path, kind, declared_bytes, declared_hash) in declarations {
        let bytes =
            fetch_browser_bytes(&format!("{root}/{}", path.as_str()), declared_bytes).await?;
        if bytes.len() as u64 != declared_bytes
            || shoop_script_resources::sha256(&bytes) != declared_hash
        {
            return Err(format!("catalog checksum/size mismatch for {path}"));
        }
        fetched.insert(
            path,
            ScriptResource::new(kind, std::sync::Arc::<[u8]>::from(bytes)),
        );
    }

    let syntax = shoop_scripting::LuaRuntime::new().map_err(|error| error.to_string())?;
    let mut scripts = Vec::new();
    let mut warnings = Vec::new();
    let mut preserve_identities = Vec::new();
    for (identity, source_resource) in fetched.iter().filter(|(path, resource)| {
        classify_resource(path) == Some(ResourceKind::Lua) && resource.kind == ResourceKind::Lua
    }) {
        let source = match std::str::from_utf8(&source_resource.bytes) {
            Ok(source) => source,
            Err(error) => {
                warnings.push(format!("{identity}: script is not UTF-8: {error}"));
                preserve_identities.push(identity.to_string());
                continue;
            }
        };
        if let Err(error) = syntax.check_syntax(identity.file_name(), source) {
            warnings.push(format!("{identity}: {error}"));
            preserve_identities.push(identity.to_string());
            continue;
        }
        let prefix = identity
            .parent()
            .map(|parent| format!("{parent}/"))
            .unwrap_or_default();
        let entrypoint = NormalizedRelativePath::parse(identity.file_name())
            .map_err(|error| error.to_string())?;
        let mut resources = std::collections::BTreeMap::new();
        resources.insert(entrypoint.clone(), source_resource.clone());
        for (path, resource) in &fetched {
            if resource.kind == ResourceKind::Lua {
                continue;
            }
            let Some(relative) = path.as_str().strip_prefix(&prefix) else {
                continue;
            };
            let relative = NormalizedRelativePath::parse(relative)
                .map_err(|error| format!("{path}: {error}"))?;
            resources.insert(relative, resource.clone());
        }
        let bundle = ScriptResourceBundle::new(entrypoint, resources, limits)
            .map_err(|error| format!("{identity}: {error}"))?;
        let identity_text = identity.to_string();
        let kind = if identity_text.starts_with("examples/") {
            ScriptKind::Example
        } else {
            ScriptKind::Bundled
        };
        scripts.push(shoop_egui::CatalogScriptSource {
            identity: identity_text.clone(),
            name: identity.file_name().to_owned(),
            source: std::sync::Arc::from(source),
            source_path: None,
            resource_bundle: Some(std::sync::Arc::new(bundle)),
            kind,
            enabled: kind == ScriptKind::Bundled
                && enabled.get(&identity_text).copied().unwrap_or(false),
        });
    }
    Ok((scripts, warnings, preserve_identities))
}

/// What the application needs to start once the new engine is connected.
#[cfg(target_arch = "wasm32")]
struct PendingNewEngineStart {
    midi_service: Box<dyn shoop_scripting::MidiControlService>,
    master_auto_connect: bool,
    loop_smoothing_ms: u32,
    warnings: Vec<String>,
}

#[cfg(target_arch = "wasm32")]
struct Runtime {
    /// Absent until the new engine backend is connected.
    runtime: Option<CooperativeApplicationRuntime>,
    pending_start: Option<PendingNewEngineStart>,
    /// The snapshot shown while the application has not started.
    waiting_snapshot: std::sync::Arc<AppSnapshot>,
    audio: browser_rt_audio::BrowserRtAudio,
    midi: browser_midi::BrowserMidiController,
    preview_player: browser_preview::BrowserPreviewPlayer,
    system_preview_request: Option<u64>,
    catalog_scan_generation: u64,
    catalog_completions: Rc<RefCell<VecDeque<BrowserCatalogCompletion>>>,
    applied_audio_settings_revision: u64,
    applied_profiling_preference: Option<bool>,
    profiling_requested_override: Option<bool>,
    applied_settings_revision: u64,
    applied_builtin_settings: (String, shoop_settings::StringToggleList),
}

#[cfg(target_arch = "wasm32")]
type BrowserCatalogCompletion = (
    u64,
    Result<
        (
            Vec<shoop_egui::CatalogScriptSource>,
            Vec<String>,
            Vec<String>,
        ),
        String,
    >,
);

#[cfg(target_arch = "wasm32")]
impl Runtime {
    fn new(settings: &shoop_settings::SettingsSnapshot, args: &AppArgs) -> anyhow::Result<Self> {
        args.selected_engine_transport(settings)?;
        anyhow::ensure!(!args.worker, "Worker mode was retired. The browser session service runs on the main thread and renders in an AudioWorklet.");
        let (midi, midi_service) = browser_midi::BrowserMidiController::new()?;
        Self::new_engine(settings, args.offline, midi, midi_service)
    }

    /// The new engine backend: the application starts once browser audio is enabled and
    /// the engine is connected.
    fn new_engine(
        settings: &shoop_settings::SettingsSnapshot,
        offline: bool,
        midi: browser_midi::BrowserMidiController,
        midi_service: Box<dyn shoop_scripting::MidiControlService>,
    ) -> anyhow::Result<Self> {
        let mut warnings = Vec::new();
        let pools = shoop_egui::new_engine_pools_from_snapshot(settings).unwrap_or_else(|error| {
            warnings.push(format!(
                "Could not use the new engine's capacity settings: {error}; using the defaults"
            ));
            shoop_egui::NewEnginePools::default()
        });
        let loop_smoothing_ms =
            shoop_egui::loop_edge_smoothing_ms(settings).unwrap_or_else(|error| {
                warnings.push(format!(
                    "Could not use loop edge smoothing setting: {error}; using 3 ms"
                ));
                3
            });
        let master_auto_connect =
            shoop_egui::master_auto_connect(settings).unwrap_or_else(|error| {
                warnings.push(format!(
                "Could not use Master auto-connect setting: {error}; using automatic connection"
            ));
                true
            });
        let controller = browser_rt_audio::BrowserRtAudio::new(
            midi.hub(),
            pools,
            shoop_egui::processor_settings_from_snapshot(settings).map_err(anyhow::Error::msg)?,
        )?;
        if offline {
            set_offline_audio_permission_presentation();
            controller.refuse(
                "Audio is disabled in offline mode. Reload without offline to enable the session engine.".to_owned(),
            );
        }
        let mut runtime = Self {
            runtime: None,
            pending_start: Some(PendingNewEngineStart {
                midi_service,
                master_auto_connect,
                loop_smoothing_ms,
                warnings,
            }),
            waiting_snapshot: Default::default(),
            audio: controller,
            midi,
            preview_player: browser_preview::BrowserPreviewPlayer::default(),
            system_preview_request: None,
            catalog_scan_generation: 0,
            catalog_completions: Rc::new(RefCell::new(VecDeque::new())),
            applied_audio_settings_revision: settings.revision(),
            applied_profiling_preference: None,
            profiling_requested_override: None,
            applied_settings_revision: settings.revision(),
            applied_builtin_settings: builtin_script_settings(settings),
        };
        runtime.update_waiting_snapshot();
        runtime.request_builtin_rescan(settings);
        Ok(runtime)
    }

    /// Starts the application on the new engine backend once it is connected.
    fn start_new_engine(&mut self) {
        let controller = &self.audio;
        if self.runtime.is_some() {
            return;
        }
        let Some((backend, host_warnings)) = controller.take_backend() else {
            return;
        };
        let Some(pending) = self.pending_start.take() else {
            return;
        };
        let mut warnings = pending.warnings;
        warnings.extend(host_warnings);
        for warning in &warnings {
            tracing::warn!(warning = %warning, "frontend.startup.fallback");
        }
        match CooperativeApplicationRuntime::start_with_options_and_midi(
            backend,
            application::ApplicationStartOptions {
                startup_scripts: Vec::new(),
                master_auto_connect: pending.master_auto_connect,
                loop_smoothing_ms: pending.loop_smoothing_ms,
                startup_warnings: warnings,
            },
            pending.midi_service,
        ) {
            Ok(runtime) => {
                controller.set_pump_wake(runtime.wake());
                self.runtime = Some(runtime);
            }
            Err(error) => controller.refuse(format!(
                "The application could not start on the new engine backend: {error:#}"
            )),
        }
    }

    fn update_waiting_snapshot(&mut self) {
        let controller = &self.audio;
        let audio_driver = controller.driver_state();
        let (error, message) = controller.notice();
        let current = &self.waiting_snapshot;
        if current.status.audio_driver == audio_driver
            && current
                .notifications
                .first()
                .is_some_and(|notification| notification.message == message)
        {
            return;
        }
        self.waiting_snapshot = std::sync::Arc::new(AppSnapshot {
            revision: current.revision + 1,
            status: shoop_egui::StatusState {
                version: env!("CARGO_PKG_VERSION").to_owned(),
                audio_driver,
                ..Default::default()
            },
            capabilities: shoop_egui::BackendCapabilities::NONE,
            notifications: std::sync::Arc::from([shoop_egui::AppNotification {
                id: current.revision + 1,
                level: if error {
                    shoop_egui::NotificationLevel::Error
                } else {
                    shoop_egui::NotificationLevel::Warning
                },
                message,
            }]),
            ..Default::default()
        });
    }

    fn new_engine_microphone(&self) -> bool {
        self.audio.microphone_connected()
    }

    fn set_repaint_context(&self, context: egui::Context) {
        self.audio.set_repaint_context(context);
    }

    fn start_tracing(&mut self, engine_detail: bool) -> anyhow::Result<()> {
        let _ = engine_detail;
        Ok(())
    }

    fn ensure_tracing_realm(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn poll_tracing(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn cancel_tracing_request(&mut self) {}

    fn tracing_realm_ready(&self) -> bool {
        self.audio_running()
    }

    fn request_stop_tracing(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn trace_stopped(&self) -> bool {
        true
    }

    fn take_trace_realms(
        &mut self,
    ) -> anyhow::Result<Option<Vec<shoop_tracing::BrowserRealmData>>> {
        Ok(Some(Vec::new()))
    }

    fn tick(&mut self, elapsed: Duration) {
        self.start_new_engine();
        if self.runtime.is_none() {
            self.midi.update_presentation();
            self.audio.update_presentation();
            self.update_waiting_snapshot();
            let message = format!(
                "Browser audio (new engine): {:?}",
                self.waiting_snapshot.status.audio_driver
            );
            set_browser_status(&message, Some(&self.waiting_snapshot));
            self.publish_midi_status();
            return;
        }
        let completions = if self
            .runtime
            .as_ref()
            .is_some_and(|runtime| runtime.is_ready())
        {
            self.catalog_completions
                .borrow_mut()
                .drain(..)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for (generation, result) in completions {
            if generation != self.catalog_scan_generation {
                continue;
            }
            match result {
                Ok((scripts, warnings, preserve_identities)) => {
                    for warning in warnings {
                        tracing::warn!(warning = %warning, "frontend.scripting.catalog_scan_warning");
                    }
                    let _ = self.dispatch(AppIntent::ReconcileCatalogScripts {
                        scripts: scripts.into(),
                        preserve_identities: preserve_identities.into(),
                    });
                }
                Err(error) => {
                    tracing::error!(error = %error, "frontend.scripting.catalog_scan_failed");
                }
            }
        }
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.tick(elapsed);
        }
        self.midi.update_presentation();
        let snapshot = self.snapshot();
        self.audio.update_presentation();
        let message = format!("Browser audio: {:?}", snapshot.status.audio_driver);
        set_browser_status(&message, Some(&snapshot));
        self.publish_midi_status();
    }

    fn publish_midi_status(&self) {
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

    fn reconcile_audio_settings(
        &mut self,
        settings: &shoop_settings::SettingsSnapshot,
    ) -> anyhow::Result<()> {
        let Some(runtime) = self.runtime.as_mut() else {
            return Ok(());
        };
        reconcile_profiling_settings(
            settings,
            &mut self.applied_profiling_preference,
            &mut self.profiling_requested_override,
            |intent| runtime.dispatch(intent),
        )?;
        reconcile_loop_smoothing_settings(
            settings,
            &mut self.applied_audio_settings_revision,
            |intent| runtime.dispatch(intent),
        )
    }

    fn reconcile_script_settings(
        &mut self,
        settings: &shoop_settings::SettingsSnapshot,
    ) -> anyhow::Result<()> {
        if settings.revision() == self.applied_settings_revision {
            return Ok(());
        }
        let builtins = builtin_script_settings(settings);
        if builtins != self.applied_builtin_settings {
            self.request_builtin_rescan(settings);
            self.applied_builtin_settings = builtins;
        }
        self.applied_settings_revision = settings.revision();
        Ok(())
    }

    fn request_builtin_rescan(&mut self, settings: &shoop_settings::SettingsSnapshot) {
        self.catalog_scan_generation = self.catalog_scan_generation.saturating_add(1);
        let generation = self.catalog_scan_generation;
        let root = settings
            .get(shoop_egui::BUILTINS_LOCATION)
            .unwrap_or_else(|_| "builtins".to_owned());
        let enabled = settings
            .get(shoop_egui::BUILTIN_SCRIPTS)
            .unwrap_or_default()
            .0
            .into_iter()
            .map(|entry| (entry.value, entry.enabled))
            .collect();
        let completions = Rc::clone(&self.catalog_completions);
        wasm_bindgen_futures::spawn_local(async move {
            let result = fetch_browser_catalog(root, enabled).await;
            completions.borrow_mut().push_back((generation, result));
        });
    }

    fn snapshot(&self) -> std::sync::Arc<AppSnapshot> {
        match &self.runtime {
            Some(runtime) => runtime.snapshot(),
            None => std::sync::Arc::clone(&self.waiting_snapshot),
        }
    }

    fn dispatch(&mut self, intent: AppIntent) -> Result<(), application::DispatchError> {
        match &mut self.runtime {
            Some(runtime) => runtime.dispatch(intent),
            None => Err(application::DispatchError::Disconnected),
        }
    }

    fn take_file_output(&self) -> Option<application::ApplicationFileOutput> {
        self.runtime.as_ref()?.take_file_output()
    }
    fn take_file_request(&self) -> Option<application::ApplicationFileRequest> {
        self.runtime.as_ref()?.take_file_request()
    }

    fn audio_preview_available(&self) -> bool {
        browser_preview::is_available()
    }

    fn process_audio_previews(&mut self) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        let state = runtime.snapshot();
        if self
            .system_preview_request
            .is_some_and(|id| id != state.click_track.preview_request_id)
            || state.click_track.preview_output != Some(shoop_egui::ClickTrackPreviewOutput::System)
            || !matches!(
                state.click_track.preview_status,
                shoop_egui::ClickTrackPreviewStatus::Queued
                    | shoop_egui::ClickTrackPreviewStatus::Playing
            )
        {
            self.preview_player.stop_active();
        }
        if let Some(intent) = self.preview_player.update() {
            let _ = runtime.dispatch(intent);
        }
        while let Some(preview) = runtime.take_audio_preview() {
            let request_id = preview.request_id;
            let state = runtime.snapshot();
            if state.click_track.preview_request_id != request_id
                || state.click_track.preview_status != shoop_egui::ClickTrackPreviewStatus::Queued
            {
                continue;
            }
            self.system_preview_request = Some(request_id);
            let context = self.audio.audio_context();
            match self.preview_player.play(context, preview) {
                Ok(()) => {}
                Err(message) => {
                    let _ = runtime.dispatch(AppIntent::CompleteClickTrackPreview {
                        request_id,
                        success: false,
                        message,
                    });
                }
            }
        }
    }

    fn audio_running(&self) -> bool {
        self.audio.running() && self.runtime.is_some()
    }
    fn session_operations_pending(&self) -> bool {
        self.runtime
            .as_ref()
            .is_none_or(CooperativeApplicationRuntime::has_pending_operations)
    }
}

const SMALL_SCREEN_MAX_SHORT_SIDE: f32 = 800.0;
const SMALL_SCREEN_UI_SCALE: f64 = 1.25;

fn default_ui_scale_for_screen(screen_size: Option<egui::Vec2>) -> f64 {
    if screen_size.is_some_and(|size| {
        size.x > 0.0 && size.y > 0.0 && size.x.min(size.y) <= SMALL_SCREEN_MAX_SHORT_SIDE
    }) {
        SMALL_SCREEN_UI_SCALE
    } else {
        1.3
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn default_touch_mode() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
fn default_touch_mode() -> bool {
    web_sys::window()
        .and_then(|window| window.match_media("(any-hover: hover)").ok().flatten())
        .is_some_and(|query| !query.matches())
}

#[cfg(not(target_arch = "wasm32"))]
fn detected_screen_size(context: &eframe::CreationContext<'_>) -> Option<egui::Vec2> {
    context
        .egui_ctx
        .input(|input| input.viewport().monitor_size)
}

#[cfg(target_arch = "wasm32")]
fn detected_screen_size(_context: &eframe::CreationContext<'_>) -> Option<egui::Vec2> {
    let screen = web_sys::window()?.screen().ok()?;
    let width = screen.width().ok()? as f32;
    let height = screen.height().ok()? as f32;
    Some(egui::vec2(width, height))
}

fn create_app(
    context: &eframe::CreationContext<'_>,
    args: &AppArgs,
    #[cfg(not(target_arch = "wasm32"))] tracing: Option<SharedNativeTracing>,
) -> Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>> {
    shoop_egui::initialize(&context.egui_ctx);
    let ui_scale_default = default_ui_scale_for_screen(detected_screen_size(context));
    let app = UnifiedApp::new(ui_scale_default, default_touch_mode(), args)?;
    #[cfg(not(target_arch = "wasm32"))]
    let mut app = app;
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(tracing) = tracing {
        tracing
            .lock()
            .map_err(|_| anyhow::anyhow!("tracing controller lock is poisoned"))?
            .start_requested_capture()?;
        app.tracing = Some(tracing);
    }
    if let Ok(scale) = app.settings.active().get(UI_SCALE_FACTOR) {
        context.egui_ctx.set_zoom_factor(scale as f32);
    }
    Ok(Box::new(app) as Box<dyn eframe::App>)
}

#[cfg(not(target_arch = "wasm32"))]
fn independent_roles(args: &AppArgs) -> anyhow::Result<bool> {
    use anyhow::Context;
    use shoop_host::{config::AppTransport, serve::StartOptions};
    if args.shutdown_host {
        let target = args.host_target()?.context("Host target required")?;
        shoop_host::connection::shutdown_ownerless(&target, Duration::from_secs(15))?;
        println!("Host cleanup confirmed");
        return Ok(true);
    }
    if args.ui_only || args.remote_profile.is_some() || args.host_address.is_some() {
        return Ok(false);
    }
    let executable = std::env::current_exe()?;
    let companion = shoop_host::launch::companion_path(&executable)?;
    let mut builder = SettingsRegistryBuilder::default();
    register_settings_with_appearance_defaults(&mut builder, 1.0, false)?;
    register_audio_settings_with_default_driver(&mut builder, default_audio_driver_kind())?;
    shoop_egui::register_audio_backend_settings(&mut builder)?;
    let settings = load_settings_manager(builder.finish(), args);
    let settings = settings.active();
    let transport = args.selected_engine_transport(&settings)?;
    if transport == shoop_egui::EngineTransportKind::InProcess {
        return Ok(false);
    }
    let config = shoop_host::config::HostConfig {
        settings: shoop_host::config::HostSettings {
            processors: args.configured_processors(&settings)?,
            driver: args
                .configured_audio_driver(&settings)
                .map_err(anyhow::Error::msg)?,
            pools: shoop_egui::new_engine_pools_from_snapshot(&settings)
                .map_err(anyhow::Error::msg)?,
            master_auto_connect: shoop_egui::master_auto_connect(&settings)
                .map_err(anyhow::Error::msg)?,
            loop_smoothing_ms: shoop_egui::loop_edge_smoothing_ms(&settings)
                .map_err(anyhow::Error::msg)?,
            ..Default::default()
        },
        transport: if transport == shoop_egui::EngineTransportKind::Socket {
            AppTransport::Socket
        } else {
            AppTransport::SharedMemory
        },
        ..Default::default()
    };
    let mut options = StartOptions::new()?;
    if let Some(endpoint) = args.endpoint {
        options.endpoint = endpoint;
    }
    options.export_profile = args.export_profile.clone();
    options.log_file = args.log_file.clone();
    let original =
        shoop_host::audio_args::AudioArgs::without_overrides(std::env::args_os().skip(1));
    shoop_host::launch::pair(&config, options, &companion, |_target| {
        let mut command = std::process::Command::new(&executable);
        command.args(&original).arg("--ui-only").arg("--host-stdin");
        command
    })?;
    Ok(true)
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let cli = AppArgs::parse();
    if !cli.probe_engine_transport
        && !cli.probe_persistence
        && !cli.probe_hosted_processors
        && !cli.probe_composites
        && !cli.probe_input
        && !cli.probe_builtins
        && !cli.tracing_smoke_test
    {
        match independent_roles(&cli) {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => {
                eprintln!("Session launch failed: {error:#}");
                std::process::exit(1);
            }
        }
    }

    if cli.probe_engine_transport
        || cli.probe_persistence
        || cli.probe_hosted_processors
        || cli.probe_composites
        || cli.probe_input
    {
        let result = if cli.probe_input {
            session_probe::input(cli.engine_transport.unwrap())
        } else if cli.probe_hosted_processors {
            cli.audio
                .apply_processors(Default::default())
                .map_err(anyhow::Error::msg)
                .and_then(|settings| session_probe::hosted(cli.engine_transport.unwrap(), settings))
        } else if cli.probe_composites {
            session_probe::composites(cli.engine_transport.unwrap())
        } else if cli.probe_persistence {
            session_probe::files(cli.engine_transport.unwrap())
        } else {
            session_probe::run(cli.engine_transport.unwrap())
        };
        match result {
            Ok(()) => {
                println!(
                    "Session {} {}: workflow and graceful shutdown passed",
                    if cli.probe_input {
                        "input"
                    } else if cli.probe_composites {
                        "composites"
                    } else if cli.probe_persistence {
                        "persistence"
                    } else {
                        "transport"
                    },
                    cli.engine_transport.unwrap().id()
                );
                return;
            }
            Err(error) => {
                eprintln!("Engine transport probe failed: {error:#}");
                std::process::exit(5);
            }
        }
    }
    if cli.probe_builtins {
        let root = shoop_egui::default_builtins_location();
        let result = shoop_script_resources::scan_builtin_directory(
            std::path::Path::new(&root),
            1,
            shoop_script_resources::ResourceLimits::default(),
        )
        .map_err(anyhow::Error::from)
        .and_then(|catalog| {
            if catalog.entries.is_empty() || !catalog.diagnostics.is_empty() {
                anyhow::bail!(
                    "discovered {} scripts with diagnostics: {:?}",
                    catalog.entries.len(),
                    catalog.diagnostics
                );
            }
            let runtime = shoop_scripting::LuaRuntime::new()?;
            for entry in &catalog.entries {
                runtime.check_syntax(entry.identity.as_str(), &entry.source)?;
            }
            Ok(catalog.entries.len())
        });
        match result {
            Ok(count) => {
                println!("Discovered {count} built-in scripts in {root}");
                return;
            }
            Err(error) => {
                eprintln!("Built-in script probe failed: {error:#}");
                std::process::exit(4);
            }
        }
    }

    let tracing_runtime = match NativeTracing::initialize(&cli) {
        Ok(tracing) => Arc::new(Mutex::new(tracing)),
        Err(error) => {
            eprintln!("Could not initialize tracing: {error:#}");
            std::process::exit(1);
        }
    };
    if cli.tracing_smoke_test {
        let result = tracing_runtime
            .lock()
            .map_err(|_| anyhow::anyhow!("tracing controller lock is poisoned"))
            .and_then(|mut tracing| {
                tracing.start_requested_capture()?;
                tracing::info!(target: "Frontend.Egui", "frontend.egui.tracing_smoke_test");
                tracing.shutdown()
            });
        if let Err(error) = result {
            eprintln!("Failed to run Perfetto capture smoke test: {error:#}");
            std::process::exit(1);
        }
        return;
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ShoopDaLoop")
            .with_icon(application_icon())
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([360.0, 200.0]),
        ..Default::default()
    };
    let app_tracing = Arc::clone(&tracing_runtime);
    let result = eframe::run_native(
        "ShoopDaLoop",
        options,
        Box::new(move |context| create_app(context, &cli, Some(Arc::clone(&app_tracing)))),
    );
    let shutdown = tracing_runtime
        .lock()
        .map_err(|_| anyhow::anyhow!("tracing controller lock is poisoned"))
        .and_then(|mut tracing| tracing.shutdown());
    if let Err(error) = shutdown {
        eprintln!("Failed to shut down Perfetto capture: {error:#}");
        std::process::exit(1);
    }
    if let Err(error) = result {
        eprintln!("ShoopDaLoop failed: {error}");
        std::process::exit(1);
    }
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
    fn from_args(args: &AppArgs) -> Self {
        match args.settings_test {
            Some(app_args::SettingsTest::Write) => Self::Write,
            Some(app_args::SettingsTest::Verify) => Self::Verify,
            Some(app_args::SettingsTest::Rejected) => Self::Rejected,
            Some(app_args::SettingsTest::Invalid) => Self::Invalid,
            Some(app_args::SettingsTest::SaveFailure) => Self::SaveFailure,
            Some(app_args::SettingsTest::Unavailable) => Self::Unavailable,
            None => Self::Disabled,
        }
    }

    fn update(
        &mut self,
        settings: &mut SettingsManager,
        widget: &mut AppWidget,
        runtime: &Runtime,
    ) {
        if runtime.snapshot().scripting.scripts.is_empty() {
            return;
        }
        let result = match *self {
            Self::Disabled | Self::Complete | Self::Failed => return,
            Self::Write => {
                let active = settings.view().active;
                let mut draft = shoop_egui::SettingsDraft::from_snapshot(&active);
                draft.set(shoop_egui::DEFAULT_NEW_TRACK_AUDIO_CHANNELS, 6);
                draft.set(shoop_egui::DEFAULT_NEW_TRACK_MIDI, true);
                draft.set(
                    shoop_egui::BUILTIN_SCRIPTS,
                    shoop_settings::StringToggleList(vec![
                        shoop_settings::StringToggle {
                            value: KEYBOARD_SCRIPT_FILENAME.to_owned(),
                            enabled: false,
                        },
                        shoop_settings::StringToggle {
                            value: APC_MINI_SCRIPT_FILENAME.to_owned(),
                            enabled: true,
                        },
                    ]),
                );
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
                    verify_browser_settings(settings, widget, runtime, 2, false, false, false)
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
                    verify_browser_settings(settings, widget, runtime, 2, false, false, false)
                        .map(|()| "invalid")
                }
            }
            Self::SaveFailure => {
                let active = settings.view().active;
                let mut draft = shoop_egui::SettingsDraft::from_snapshot(&active);
                draft.set(shoop_egui::DEFAULT_NEW_TRACK_AUDIO_CHANNELS, 8);
                draft.set(
                    shoop_egui::BUILTIN_SCRIPTS,
                    shoop_settings::StringToggleList(vec![
                        shoop_settings::StringToggle {
                            value: KEYBOARD_SCRIPT_FILENAME.to_owned(),
                            enabled: false,
                        },
                        shoop_settings::StringToggle {
                            value: APC_MINI_SCRIPT_FILENAME.to_owned(),
                            enabled: true,
                        },
                    ]),
                );
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
                    verify_browser_settings(settings, widget, runtime, 2, false, false, false)
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
    let scripts = view
        .active
        .get(shoop_egui::BUILTIN_SCRIPTS)
        .map_err(|error| settings::SettingsManagerError::Storage(error.to_string()))?;
    let enabled = |identity: &str| {
        scripts
            .0
            .iter()
            .find(|entry| entry.value == identity)
            .is_some_and(|entry| entry.enabled)
    };
    let keyboard = enabled(KEYBOARD_SCRIPT_FILENAME);
    let apc = enabled(APC_MINI_SCRIPT_FILENAME);
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

#[cfg(test)]
fn click_request_was_rejected(
    snapshot_revision: u64,
    io_task: Option<&shoop_egui::IoTaskState>,
    previous_task: shoop_egui::TaskId,
    request_revision: u64,
) -> bool {
    snapshot_revision > request_revision
        && io_task.is_none_or(|task| {
            task.id == previous_task || task.kind != shoop_egui::IoTaskKind::GenerateClickTrack
        })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
fn lua_version_rejection_is_expected(script: &shoop_egui::ScriptState) -> bool {
    let lifecycle = if script.name == "lua-api-unannounced.lua" {
        shoop_egui::ScriptLifecycle::Error
    } else {
        shoop_egui::ScriptLifecycle::Incompatible
    };
    script.lifecycle == lifecycle && script.latest_error.is_some()
}

#[cfg(target_arch = "wasm32")]
fn open_browser_permissions_dialog() -> anyhow::Result<()> {
    let dialog = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("browser_permissions_dialog"))
        .ok_or_else(|| anyhow::anyhow!("browser permissions dialog is unavailable"))?;
    dialog
        .remove_attribute("hidden")
        .map_err(|error| anyhow::anyhow!("could not open browser permissions: {error:?}"))
}

#[cfg(target_arch = "wasm32")]
fn set_offline_audio_permission_presentation() {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    for id in ["enable_audio", "enable_output_audio"] {
        if let Some(button) = document.get_element_by_id(id) {
            let _ = button.set_attribute("hidden", "");
        }
    }
    for id in [
        "audio_output_permission_status",
        "microphone_permission_status",
    ] {
        if let Some(status) = document.get_element_by_id(id) {
            status.set_text_content(Some("Unavailable in offline mode"));
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_status_element() -> Option<web_sys::Element> {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("runtime_status"))
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
            "data-render-discontinuities",
            &status.render_discontinuities.to_string(),
        );
        let _ = element.set_attribute("data-memory-growths", &status.memory_growths.to_string());
        let _ = element.set_attribute(
            "data-render-memory-growths",
            &status.render_memory_growths.to_string(),
        );
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
        if let Some(task) = &snapshot.io_task {
            let _ = element.set_attribute("data-io-task-id", &task.id.raw().to_string());
            let _ = element.set_attribute("data-io-task-kind", &format!("{:?}", task.kind));
            let _ = element.set_attribute("data-io-task-status", &format!("{:?}", task.status));
        } else {
            let _ = element.remove_attribute("data-io-task-id");
            let _ = element.remove_attribute("data-io-task-kind");
            let _ = element.remove_attribute("data-io-task-status");
        }
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
            let _ = element.set_attribute(
                "data-midi-detail-channels",
                &details.midi_channels.len().to_string(),
            );
            let _ = element.set_attribute(
                "data-midi-detail-events",
                &details
                    .midi_channels
                    .iter()
                    .map(|channel| channel.events.len())
                    .sum::<usize>()
                    .to_string(),
            );
            let _ = element.set_attribute(
                "data-midi-detail-loading",
                &details.midi_loading.to_string(),
            );
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast as _;

    if let Err(error) = shoop_tracing::initialize_browser_tracing() {
        web_sys::console::error_1(&format!("Could not initialize browser tracing: {error}").into());
        return;
    }
    wasm_bindgen_futures::spawn_local(async {
        let window = web_sys::window().expect("browser window is unavailable");
        let document = window.document().expect("browser document is unavailable");
        let canvas = document
            .get_element_by_id(WEB_CANVAS_ID)
            .expect("missing #shoop_canvas element")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("#shoop_canvas is not a canvas");

        let query = window.location().search().unwrap_or_default();
        let args = match app_args::parse_web_query(&query) {
            Ok(args) => args,
            Err(error) => {
                let message = format!("Invalid application arguments: {error}");
                set_browser_status(&message, None);
                log::error!("{message}");
                return;
            }
        };
        match eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |context| create_app(context, &args)),
            )
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
    #[cfg(not(target_arch = "wasm32"))]
    use std::thread;

    #[cfg(not(target_arch = "wasm32"))]
    use shoop_egui::{
        ApplicationPortOwner, DirectTrackSpec, HostPortId, LoopAction, LoopMode, MidiNote,
        PianoAction, PortRole, SelectionModifiers, TrackAction,
    };

    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn about_dialog_renders_release_and_development_identities() {
        let context = egui::Context::default();
        for identity in [
            BuildIdentity {
                kind: "release",
                version: "1.2.3",
                branch: "unused",
                revision: "unused",
                date: "2026-08-31T12:00:00Z",
            },
            BuildIdentity {
                kind: "development",
                version: "unused",
                branch: "topic/build-identity",
                revision: "12345678",
                date: "2026-08-31T12:00:00Z",
            },
        ] {
            let mut open = true;
            let mut output = context.run_ui(Default::default(), |ui| {
                show_about_dialog(ui.ctx(), &mut open, identity, "fixture settings path");
            });
            output.textures_delta.clear();
            assert!(open);
        }

        let mut open = false;
        let mut output = context.run_ui(Default::default(), |ui| {
            show_about_dialog(
                ui.ctx(),
                &mut open,
                BuildIdentity::CURRENT,
                "fixture settings path",
            );
        });
        output.textures_delta.clear();
        assert!(!open);
    }

    #[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
    #[shoop_wasm_test_support::shoop_test(
        wasm_only = "requires browser performance APIs",
        no_trace = "manages the browser capture lifecycle directly"
    )]
    fn browser_window_perfetto_capture_is_nonempty() {
        shoop_tracing::initialize_browser_tracing().unwrap();
        let capture = shoop_tracing::BrowserCapture::start(false).unwrap();
        {
            let _span = tracing::info_span!("shoop.browser.capture.test").entered();
            tracing::info!(answer = 42_u64, "shoop.browser.capture.event");
        }
        capture.poll().unwrap();
        let bytes = capture
            .finish(vec![browser_window_calibration().unwrap()], Vec::new())
            .unwrap();
        assert!(bytes.len() > 128);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn session_sources_recognize_http_urls_and_names() {
        assert!(is_http_url("https://example.com/session.shoop"));
        assert!(is_http_url("HTTP://EXAMPLE.COM/session.shoop"));
        assert!(!is_http_url("https://"));
        assert!(!is_http_url("session.shoop"));
        assert_eq!(
            session_source_name("https://example.com/sessions/demo.shoop?download=1#top"),
            "demo.shoop"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn forced_startup_url_skips_only_its_confirmation() {
        let url = "https://example.com/session.shoop";
        assert_eq!(
            startup_session_action(url.to_owned(), false),
            StartupSessionAction::ConfirmUrl(url.to_owned())
        );
        assert_eq!(
            startup_session_action(url.to_owned(), true),
            StartupSessionAction::FetchUrl(url.to_owned())
        );
        assert_eq!(
            startup_session_action("session.shoop".to_owned(), true),
            StartupSessionAction::LoadPath("session.shoop".to_owned())
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn browser_click_rejection_is_detected_after_the_request_revision() {
        let previous_task = shoop_egui::TaskId::from_raw(7);
        assert!(!click_request_was_rejected(12, None, previous_task, 12));
        assert!(click_request_was_rejected(13, None, previous_task, 12));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn embedded_builtin_keys_accept_relative_root_variants() {
        assert_eq!(
            embedded_builtin_key("builtins/catalog.json"),
            "builtins/catalog.json"
        );
        assert_eq!(
            embedded_builtin_key("./builtins/catalog.json"),
            "builtins/catalog.json"
        );
        assert_eq!(
            embedded_builtin_key("././builtins/catalog.json"),
            "builtins/catalog.json"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn profiling_settings_persist_default_off_and_retry_failed_dispatch() {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let defaults = registry.defaults(1);
        assert_eq!(defaults.get(shoop_egui::RUNTIME_PROFILING), Ok(false));
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&defaults);
        draft.set(shoop_egui::RUNTIME_PROFILING, true);
        let document = registry
            .document_from_draft(
                &shoop_settings::SettingsDocument::empty("test"),
                &draft,
                "test",
            )
            .unwrap();
        let saved = shoop_settings::encode_settings(&document).unwrap();
        let restored = registry
            .resolve(&shoop_settings::decode_settings(&saved).unwrap(), 2)
            .snapshot;
        assert_eq!(restored.get(shoop_egui::RUNTIME_PROFILING), Ok(true));
        let mut applied = None;
        assert!(
            reconcile_profiling_settings(&restored, &mut applied, &mut None, |_| {
                Err(application::DispatchError::Full)
            })
            .is_err()
        );
        assert_eq!(applied, None);
        reconcile_profiling_settings(&restored, &mut applied, &mut None, |intent| {
            assert_eq!(intent, AppIntent::SetProfiling(true));
            Ok(())
        })
        .unwrap();
        reconcile_profiling_settings(&restored, &mut applied, &mut None, |_| {
            panic!("duplicate control")
        })
        .unwrap();
        reconcile_profiling_settings(&defaults, &mut applied, &mut None, |intent| {
            assert_eq!(intent, AppIntent::SetProfiling(false));
            Ok(())
        })
        .unwrap();
        assert_eq!(applied, Some(false));
        let mut requested = Some(true);
        applied = Some(true);
        reconcile_profiling_settings(&defaults, &mut applied, &mut requested, |_| {
            panic!("pending preference must not undo an immediate toggle")
        })
        .unwrap();
        assert_eq!(requested, Some(true));
        reconcile_profiling_settings(&restored, &mut applied, &mut requested, |_| {
            panic!("saving must not dispatch a second toggle")
        })
        .unwrap();
        assert_eq!(requested, None);
        requested = Some(false);
        applied = Some(false);
        reconcile_profiling_settings(&restored, &mut applied, &mut requested, |_| {
            panic!("latest rapid toggle wins over an earlier save")
        })
        .unwrap();
        assert_eq!(requested, Some(false));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn loop_smoothing_settings_retry_failed_dispatch_and_apply_zero() {
        let mut builder = SettingsRegistryBuilder::default();
        register_audio_settings(&mut builder).unwrap();
        shoop_egui::register_audio_backend_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut document = shoop_settings::SettingsDocument::empty("test");
        document.values.insert(
            shoop_egui::LOOP_EDGE_SMOOTHING_MS.id().to_owned(),
            serde_json::json!(0),
        );
        let settings = registry.resolve(&document, 42).snapshot;
        let mut applied_revision = 0;

        assert!(
            reconcile_loop_smoothing_settings(&settings, &mut applied_revision, |_| Err(
                application::DispatchError::Full
            ),)
            .is_err()
        );
        assert_eq!(applied_revision, 0);

        let mut received = Vec::new();
        reconcile_loop_smoothing_settings(&settings, &mut applied_revision, |intent| {
            received.push(intent);
            Ok(())
        })
        .unwrap();
        assert_eq!(
            received,
            vec![
                AppIntent::SetLoopSmoothingMs(0),
                AppIntent::SetMasterAutoConnect(true),
            ]
        );
        assert_eq!(applied_revision, 42);

        reconcile_loop_smoothing_settings(&settings, &mut applied_revision, |_| {
            panic!("an applied revision must not dispatch again")
        })
        .unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn application_icon_is_embedded() {
        let icon = application_icon();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        assert!(icon.rgba.chunks_exact(4).any(|pixel| pixel[3] == 0));
        assert!(icon.rgba.chunks_exact(4).any(|pixel| pixel[3] == 255));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn native_cli_parses_tracing_mode() {
        let tracing = AppArgs::try_parse_from(["shoopdaloop", "--tracing"]).unwrap();
        assert!(tracing.tracing);
        assert!(!tracing.tracing_engine_detail);
        assert!(!tracing.tracing_smoke_test);

        let detailed =
            AppArgs::try_parse_from(["shoopdaloop", "--tracing", "--tracing-engine-detail"])
                .unwrap();
        assert!(detailed.tracing);
        assert!(detailed.tracing_engine_detail);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn native_cli_rejects_removed_capture_option() {
        assert!(AppArgs::try_parse_from(["shoopdaloop", "--tracing-capture"]).is_err());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn native_cli_rejects_engine_detail_without_tracing_mode() {
        assert!(AppArgs::try_parse_from(["shoopdaloop", "--tracing-engine-detail"]).is_err());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn web_shell_targets_the_application_canvas() {
        let html = include_str!("../index.html");
        assert!(html.contains("data-trunk"));
        assert!(html.contains(&format!("id=\"{WEB_CANVAS_ID}\"")));
        assert!(html.contains("id=\"browser_permissions_dialog\""));
        assert!(html.contains("id=\"runtime_status\" data-driver-state=\"starting\" hidden"));
        assert!(html.contains("Browser audio and MIDI permissions"));
        assert!(html.contains("Enable microphone audio"));
        assert!(html.contains("Enable output-only audio"));
        assert!(html.contains("data-permission-granted"));
        assert!(!html.contains(">Not granted</span>"));
        assert!(!html.contains(">Not enabled</span>"));
        assert!(html.contains("audio_worklet.js"));
        assert!(html.contains("Roboto-Regular.ttf"));
        assert!(html.contains("Roboto-BoldItalic.ttf"));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn missing_setting_default_accounts_for_screen_size() {
        assert_eq!(
            default_ui_scale_for_screen(Some(egui::vec2(1280.0, 800.0))),
            1.25
        );
        assert_eq!(
            default_ui_scale_for_screen(Some(egui::vec2(1280.0, 801.0))),
            1.3
        );
        assert_eq!(default_ui_scale_for_screen(None), 1.3);

        let mut builder = SettingsRegistryBuilder::default();
        register_settings_with_appearance_defaults(&mut builder, 1.25, false).unwrap();
        let registry = builder.finish();
        assert_eq!(registry.defaults(1).get(UI_SCALE_FACTOR).unwrap(), 1.25);

        let stored = shoop_settings::SettingsDocument {
            writer_version: "test".to_owned(),
            values: std::collections::BTreeMap::from([(
                UI_SCALE_FACTOR.id().to_owned(),
                serde_json::json!(1.0),
            )]),
        };
        assert_eq!(
            registry
                .resolve(&stored, 1)
                .snapshot
                .get(UI_SCALE_FACTOR)
                .unwrap(),
            1.0
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn persisted_dummy_configuration_is_used_on_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_audio_settings(&mut builder).unwrap();
        shoop_egui::register_audio_backend_settings(&mut builder).unwrap();
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
        let runtime = Runtime::new(&restarted.active(), &AppArgs::default()).unwrap();
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

    #[cfg(not(target_arch = "wasm32"))]
    fn new_engine_args() -> AppArgs {
        AppArgs {
            ..AppArgs::default()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Waits until `done`; prints how long each step took, to compare structural latency
    /// across changes (`--no-capture`).
    fn wait_for_snapshot(
        runtime: &Runtime,
        what: &str,
        done: impl Fn(&AppSnapshot) -> bool,
    ) -> std::sync::Arc<AppSnapshot> {
        let start = Instant::now();
        let deadline = start + Duration::from_secs(20);
        loop {
            let snapshot = runtime.snapshot();
            if done(&snapshot) {
                eprintln!("new engine smoke: {what} after {:?}", start.elapsed());
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "{what} timed out; notifications: {:?}",
                snapshot.notifications
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn first_loop(snapshot: &AppSnapshot, track_id: shoop_egui::TrackId) -> &shoop_egui::LoopState {
        &snapshot
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .expect("the track exists")
            .loops[0]
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn track_ports(
        snapshot: &AppSnapshot,
        track_id: shoop_egui::TrackId,
        role: PortRole,
    ) -> Vec<shoop_egui::PortId> {
        snapshot
            .connections
            .application_ports
            .iter()
            .filter(|port| {
                matches!(port.owner, ApplicationPortOwner::Track { track_id: owner, .. } if owner == track_id)
                    && port.role == role
            })
            .map(|port| port.id)
            .collect()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn loop_action(runtime: &mut Runtime, track: &shoop_egui::TrackState, action: LoopAction) {
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id: track.loops[0].id,
                action,
            })
            .unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn new_engine_backend_runs_the_core_workflow_in_the_application() {
        application_core_workflow(UnifiedApp::new(1.0, false, &new_engine_args()).unwrap());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn driver_preference_retry_serializes_all_native_settings_writers() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let location = Arc::new(Mutex::new(None::<std::path::PathBuf>));
        let writer_location = location.clone();
        let attempts = Arc::new(AtomicUsize::new(0));
        let writer_attempts = attempts.clone();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let preferences: shoop_host::audio_driver::PreferenceWriter = Box::new(move |config| {
            if writer_attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                anyhow::bail!("injected initial preference failure");
            }
            entered_tx.send(())?;
            release_rx.recv_timeout(Duration::from_secs(30))?;
            let path = writer_location.lock().unwrap().clone().unwrap();
            settings::driver_preference_writer(path)(config)
        });
        let mut app =
            UnifiedApp::new_with_runtime(1.0, false, &new_engine_args(), |settings, args| {
                Runtime::new_with_engine_start(
                    settings,
                    args,
                    |driver, pools, transport| {
                        engine_backend::start_selected_with_processors(
                            driver,
                            pools,
                            transport,
                            Default::default(),
                        )
                    },
                    Some(preferences),
                )
            })
            .unwrap();
        let path = std::path::PathBuf::from(app.settings.view().storage_location);
        *location.lock().unwrap() = Some(path.clone());
        wait_for_snapshot(&app.runtime, "driver discovery", |s| {
            s.audio_drivers.catalog.len() == 3
        });
        app.handle_settings_action(SettingsAction::RequestAudioDriverSwitch {
            config: AudioDriverConfig::Dummy(shoop_egui::DummyAudioDriverConfig {
                sample_rate: 32_000,
                buffer_size: 128,
            }),
            draft: shoop_settings::SettingsDraft::from_snapshot(&app.settings.active()),
        });
        let confirmation = wait_for_snapshot(&app.runtime, "driver confirmation", |s| {
            s.audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::AwaitingConfirmation
        });
        let request_id = confirmation.audio_drivers.switch.request_id;
        app.handle_ui_intent(AppIntent::ConfirmAudioDriverSwitch {
            request_id,
            accept: true,
        });
        wait_for_snapshot(&app.runtime, "failed preference write", |s| {
            s.audio_drivers.switch.persistence_retry_available && s.status.sample_rate == 32_000
        });
        let wait_idle = |app: &UnifiedApp| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while app.audio_settings_busy() {
                assert!(
                    Instant::now() < deadline,
                    "audio preference command did not finish"
                );
                thread::sleep(Duration::from_millis(2));
            }
        };
        wait_idle(&app);
        for recover in [false, true] {
            let action = if recover {
                SettingsAction::RecoverWithDefaults
            } else {
                SettingsAction::Save(shoop_settings::SettingsDraft::from_snapshot(
                    &app.settings.active(),
                ))
            };
            app.handle_settings_action(action);
            assert_eq!(
                app.settings.view().persistence,
                shoop_settings::SettingsPersistenceState::Saving
            );
            for _ in 0..2 {
                app.handle_settings_action(SettingsAction::RetryAudioDriverPersistence {
                    request_id,
                });
                assert_eq!(
                    app.settings.view().persistence,
                    shoop_settings::SettingsPersistenceState::Saving
                );
                assert!(!app.runtime.handle.has_pending_audio_driver_commands());
            }
            app.handle_ui_intent(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: true,
            });
            assert!(!app.runtime.handle.has_pending_audio_driver_commands());
            wait_for_settings_save(&mut app.settings);
        }
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&app.settings.active());
        draft.set(shoop_egui::TOUCH_MODE, true);
        app.handle_settings_action(SettingsAction::Save(draft));
        wait_for_settings_save(&mut app.settings);
        let stale = shoop_settings::SettingsDraft::from_snapshot(&app.settings.active());
        let before = std::fs::read(&path).unwrap();
        app.handle_settings_action(SettingsAction::RetryAudioDriverPersistence { request_id });
        assert!(app.runtime.handle.has_pending_audio_driver_commands());
        app.handle_settings_action(SettingsAction::Save(stale.clone()));
        app.handle_settings_action(SettingsAction::RecoverWithDefaults);
        app.pending_profiling_preference = Some(true);
        app.save_profiling_preference();
        assert_eq!(app.pending_profiling_preference, Some(true));
        assert_ne!(
            app.settings.view().persistence,
            shoop_settings::SettingsPersistenceState::Saving
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        release_tx.send(()).unwrap();
        wait_for_snapshot(&app.runtime, "saved driver preference", |s| {
            s.audio_drivers.switch.status == shoop_egui::AudioDriverSwitchStatus::Completed
        });
        wait_idle(&app);
        let saved_driver = std::fs::read(&path).unwrap();
        app.handle_settings_action(SettingsAction::Save(stale));
        assert_eq!(std::fs::read(&path).unwrap(), saved_driver);
        assert_eq!(
            app.settings
                .active()
                .get(shoop_egui::DUMMY_SAMPLE_RATE)
                .unwrap(),
            32_000
        );
        app.save_profiling_preference();
        wait_for_settings_save(&mut app.settings);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(
            app.settings
                .active()
                .get(shoop_egui::DUMMY_SAMPLE_RATE)
                .unwrap(),
            32_000
        );
        assert!(app.settings.active().get(shoop_egui::TOUCH_MODE).unwrap());
        assert!(app
            .settings
            .active()
            .get(shoop_egui::RUNTIME_PROFILING)
            .unwrap());
        let stored =
            shoop_settings::decode_settings(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(
            stored.values["audio.dummy.sample_rate"],
            serde_json::json!(32_000)
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn embedded_driver_switch_keeps_the_live_lua_vm_and_application_ids() {
        let mut app = UnifiedApp::new(1.0, false, &new_engine_args()).unwrap();
        let runtime = &mut app.runtime;
        wait_for_snapshot(runtime, "driver discovery", |s| {
            s.audio_drivers.catalog.len() == 3
        });
        runtime.dispatch(AppIntent::AddScriptSource {
            name: "switch-survivor.lua".into(),
            source: std::sync::Arc::from("shoop_announce_api_version(1, 0); print('started once'); local d=require('shoop_dialog'); d.simple('Switch survivor', {d.rich_text('VM still running')})"),
            kind: ScriptKind::User,
            enabled: true,
        }).unwrap();
        let before = wait_for_snapshot(runtime, "Lua survivor dialog", |s| {
            s.scripting
                .dialogs
                .iter()
                .any(|d| d.name == "Switch survivor")
        });
        let script = before
            .scripting
            .scripts
            .iter()
            .find(|s| s.name == "switch-survivor.lua")
            .unwrap()
            .clone();
        let dialogs = before.scripting.dialogs.clone();
        runtime
            .dispatch(AppIntent::RequestLoadSessionPicker)
            .unwrap();
        let file = wait_for_snapshot(runtime, "active file picker", |s| s.io_task.is_some());
        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Dummy(shoop_egui::DummyAudioDriverConfig {
                    sample_rate: 32_000,
                    buffer_size: 128,
                }),
            })
            .unwrap();
        let busy = wait_for_snapshot(runtime, "file workflow refusal", |s| {
            s.notifications.iter().any(|n| {
                n.message
                    .contains("Finish the active file workflow before switching")
            })
        });
        assert_eq!(busy.status.sample_rate, 48_000);
        assert_ne!(
            busy.audio_drivers.switch.status,
            shoop_egui::AudioDriverSwitchStatus::AwaitingConfirmation
        );
        runtime
            .dispatch(AppIntent::CancelIoTask {
                task_id: file.io_task.as_ref().unwrap().id,
            })
            .unwrap();
        wait_for_snapshot(runtime, "cancelled file picker", |s| {
            s.io_task
                .as_ref()
                .is_some_and(|t| t.status == shoop_egui::IoTaskStatus::Cancelled)
        });

        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Dummy(shoop_egui::DummyAudioDriverConfig {
                    sample_rate: 32_000,
                    buffer_size: 128,
                }),
            })
            .unwrap();
        let prepared = wait_for_snapshot(runtime, "switch confirmation", |s| {
            s.audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::AwaitingConfirmation
        });
        runtime
            .dispatch(AppIntent::RequestLoadSessionPicker)
            .unwrap();
        let file = wait_for_snapshot(runtime, "file workflow during confirmation", |s| {
            s.io_task
                .as_ref()
                .is_some_and(|t| t.status == shoop_egui::IoTaskStatus::Running)
        });
        let notifications = file.notifications.len();
        runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id: prepared.audio_drivers.switch.request_id,
                accept: true,
            })
            .unwrap();
        let busy = wait_for_snapshot(runtime, "confirmation file workflow refusal", |s| {
            s.notifications.len() > notifications
        });
        assert_eq!(
            busy.audio_drivers.switch.status,
            shoop_egui::AudioDriverSwitchStatus::AwaitingConfirmation
        );
        assert_eq!(busy.status.sample_rate, 48_000);
        runtime
            .dispatch(AppIntent::CancelIoTask {
                task_id: file.io_task.as_ref().unwrap().id,
            })
            .unwrap();
        wait_for_snapshot(runtime, "cancelled second file picker", |s| {
            s.io_task
                .as_ref()
                .is_some_and(|t| t.status == shoop_egui::IoTaskStatus::Cancelled)
        });
        runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id: prepared.audio_drivers.switch.request_id,
                accept: true,
            })
            .unwrap();
        let after = wait_for_snapshot(
            runtime,
            "runtime switch and unsaved test preferences",
            |s| {
                s.status.sample_rate == 32_000 && s.audio_drivers.switch.persistence_retry_available
            },
        );
        let survivor = after
            .scripting
            .scripts
            .iter()
            .find(|s| s.id == script.id)
            .unwrap();
        assert_eq!(survivor.lifecycle, script.lifecycle);
        assert_eq!(survivor.logs, script.logs);
        assert_eq!(after.scripting.dialogs, dialogs);
        assert_eq!(after.tracks[0].id, before.tracks[0].id);
        assert_eq!(after.tracks[0].loops[0].id, before.tracks[0].loops[0].id);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn application_core_workflow(mut app: UnifiedApp) {
        use shoop_egui::{
            BackendCapabilities, BackendCapability, BusAction, ConfirmedConnectionState,
            GlobalControlAction, IoTaskStatus, MixerRouteState, StructuralState,
        };

        let runtime = &mut app.runtime;
        let snapshot = wait_for_snapshot(runtime, "startup", |snapshot| {
            snapshot.tracks.len() == 1 && snapshot.connections.backend_available
        });
        assert_eq!(
            snapshot.capabilities,
            BackendCapabilities::NONE
                .with(BackendCapability::MidiInjection, true)
                .with(BackendCapability::AudioDriverSwitching, true)
                .with(BackendCapability::SessionCapture, true)
                .with(BackendCapability::SessionContentLoad, true)
                .with(BackendCapability::LoopContentImport, true)
                .with(BackendCapability::LoopTimingEdits, true)
                .with(BackendCapability::BusManagement, true)
                .with(BackendCapability::DryWetModes, true)
                .with(BackendCapability::DryWetTracks, true)
                .with(
                    BackendCapability::TrackFx,
                    snapshot
                        .track_processors
                        .iter()
                        .any(|processor| processor.available),
                )
                .with(BackendCapability::CompositeLoops, true)
                .with(BackendCapability::Grab, true)
                .with(BackendCapability::ReplacingMode, true)
                .with(BackendCapability::ClickTrackInsertion, true)
        );
        assert_eq!(
            runtime.audio_preview_available(),
            native_preview::is_available()
        );
        assert_eq!(snapshot.status.sample_rate, 48_000);
        assert!(
            snapshot.notifications.is_empty(),
            "{:?}",
            snapshot.notifications
        );

        runtime.dispatch(AppIntent::RequestNewSession).unwrap();
        let snapshot = wait_for_snapshot(runtime, "a new session", |snapshot| {
            snapshot
                .io_task
                .as_ref()
                .is_some_and(|task| task.status == IoTaskStatus::Completed)
        });
        let sync = snapshot.tracks[0].clone();
        assert!(sync.is_sync);
        let master = snapshot.buses[0].clone();
        assert_eq!(master.name, "Master");
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(true)))
            .unwrap();
        for (name, audio_channels, midi) in [("Audio", 2, false), ("Keys", 0, true)] {
            runtime
                .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                    name: name.to_owned(),
                    audio_channels,
                    midi,
                }))
                .unwrap();
        }
        let snapshot = wait_for_snapshot(runtime, "two new tracks", |snapshot| {
            snapshot.tracks.len() == 3
                && snapshot.tracks.iter().all(|track| {
                    track.structural_state == StructuralState::Confirmed
                        && track
                            .loops
                            .iter()
                            .all(|state| state.structural_state == StructuralState::Confirmed)
                })
        });
        let audio = snapshot.tracks[1].clone();
        let keys = snapshot.tracks[2].clone();

        let audio_inputs = track_ports(&snapshot, audio.id, PortRole::AudioInput);
        let audio_outputs = track_ports(&snapshot, audio.id, PortRole::AudioOutput);
        let keys_input = track_ports(&snapshot, keys.id, PortRole::MidiInput);
        assert_eq!((audio_inputs.len(), audio_outputs.len()), (2, 2));
        let mut links = Vec::new();
        for (port, host) in audio_inputs
            .iter()
            .map(|port| (*port, "audio_in_1"))
            .chain(keys_input.iter().map(|port| (*port, "midi_in_1")))
        {
            runtime
                .dispatch(AppIntent::SetPortConnected {
                    port_id: port,
                    host_port_id: HostPortId::new(host),
                    connected: true,
                })
                .unwrap();
            links.push(ConfirmedConnectionState {
                application_port_id: port,
                host_port_id: HostPortId::new(host),
            });
        }
        let routes = audio_outputs
            .iter()
            .zip(master.channels.iter())
            .map(|(port, channel)| MixerRouteState {
                source_port_id: *port,
                destination_channel_id: channel.id,
            })
            .collect::<Vec<_>>();
        for route in &routes {
            runtime
                .dispatch(AppIntent::SetMixerRouteConnected {
                    source_port_id: route.source_port_id,
                    destination_channel_id: route.destination_channel_id,
                    connected: true,
                })
                .unwrap();
        }
        wait_for_snapshot(runtime, "connections and routes", |snapshot| {
            let connections = &snapshot.connections;
            links
                .iter()
                .all(|link| connections.confirmed_links.contains(link))
                && routes
                    .iter()
                    .all(|route| connections.mixer_links.contains(route))
                && connections
                    .confirmed_links
                    .iter()
                    .any(|link| link.host_port_id == HostPortId::new("audio_out_1"))
        });

        loop_action(runtime, &sync, LoopAction::RecordClicked);
        wait_for_snapshot(runtime, "sync recording", |snapshot| {
            first_loop(snapshot, sync.id).mode == LoopMode::Recording
        });
        wait_for_snapshot(runtime, "sync captured frames", |snapshot| {
            let state = first_loop(snapshot, sync.id);
            state.mode == LoopMode::Recording && state.length_frames >= 4_800
        });
        loop_action(runtime, &sync, LoopAction::PlayClicked);
        let snapshot = wait_for_snapshot(runtime, "sync playback", |snapshot| {
            first_loop(snapshot, sync.id).mode == LoopMode::Playing
        });
        let sync_length = first_loop(&snapshot, sync.id).length_frames;
        assert!(sync_length >= 4_800, "sync length {sync_length}");
        let position = first_loop(&snapshot, sync.id).position;
        wait_for_snapshot(runtime, "loop progress", |snapshot| {
            first_loop(snapshot, sync.id).position != position
        });

        loop_action(runtime, &audio, LoopAction::RecordClicked);
        wait_for_snapshot(runtime, "a synchronized recording request", |snapshot| {
            let state = first_loop(snapshot, audio.id);
            (state.mode == LoopMode::Stopped && state.next_mode == LoopMode::Recording)
                || state.mode == LoopMode::Recording
        });
        wait_for_snapshot(runtime, "synchronized recording", |snapshot| {
            first_loop(snapshot, audio.id).mode == LoopMode::Recording
        });
        loop_action(runtime, &audio, LoopAction::PlayClicked);
        let snapshot = wait_for_snapshot(runtime, "synchronized playback", |snapshot| {
            first_loop(snapshot, audio.id).mode == LoopMode::Playing
        });
        let audio_length = first_loop(&snapshot, audio.id).length_frames;
        assert!(
            audio_length > 0 && audio_length % sync_length == 0,
            "{audio_length} frames for a sync loop of {sync_length}"
        );

        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        runtime
            .dispatch(AppIntent::Track {
                track_id: keys.id,
                action: TrackAction::InputMonitoringChanged {
                    enabled: true,
                    respect_auto_mute: false,
                },
            })
            .unwrap();
        loop_action(runtime, &keys, LoopAction::RecordClicked);
        wait_for_snapshot(runtime, "immediate MIDI recording", |snapshot| {
            first_loop(snapshot, keys.id).mode == LoopMode::Recording
        });
        let note = MidiNote::new(60).unwrap();
        runtime
            .dispatch(AppIntent::Piano(PianoAction::Press(note)))
            .unwrap();
        thread::sleep(Duration::from_millis(30));
        runtime
            .dispatch(AppIntent::Piano(PianoAction::Release(note)))
            .unwrap();
        thread::sleep(Duration::from_millis(30));
        loop_action(runtime, &keys, LoopAction::PlayClicked);
        wait_for_snapshot(runtime, "immediate MIDI playback", |snapshot| {
            let state = first_loop(snapshot, keys.id);
            state.mode == LoopMode::Playing && state.length_frames > 0
        });

        for action in [
            TrackAction::OutputGainChanged(-6.0),
            TrackAction::OutputBalanceChanged(0.5),
            TrackAction::InputMonitoringChanged {
                enabled: true,
                respect_auto_mute: false,
            },
        ] {
            runtime
                .dispatch(AppIntent::Track {
                    track_id: audio.id,
                    action,
                })
                .unwrap();
        }
        for action in [
            BusAction::GainChanged(-3.0),
            BusAction::BalanceChanged(-0.25),
        ] {
            runtime
                .dispatch(AppIntent::Bus {
                    bus_id: master.id,
                    action,
                })
                .unwrap();
        }
        wait_for_snapshot(runtime, "controls and meters", |snapshot| {
            let track = snapshot
                .tracks
                .iter()
                .find(|track| track.id == audio.id)
                .unwrap();
            let controls = &track.controls;
            let master = &snapshot.buses[0];
            controls.output_gain_db == -6.0
                && controls.output_balance == 0.5
                && controls.input_monitoring
                && controls.input_peak_left_db > -20.0
                && controls.output_peak_left_db > -100.0
                && first_loop(snapshot, audio.id).peak_left_db > -100.0
                && master.gain_db == -3.0
                && master.balance == -0.25
                && !master.control_pending
                && master.output_peaks_db.iter().all(|peak| *peak > -100.0)
        });

        loop_action(
            runtime,
            &audio,
            LoopAction::IconClicked(SelectionModifiers::default()),
        );
        let snapshot = wait_for_snapshot(runtime, "the waveform", |snapshot| {
            snapshot.details.as_ref().is_some_and(|details| {
                details.loop_id == audio.loops[0].id
                    && details.channels.len() == 2
                    && details
                        .channels
                        .iter()
                        .all(|channel| channel.samples.len() as u64 == audio_length)
            })
        });
        let details = snapshot.details.as_ref().unwrap();
        assert!(details.channels[0]
            .samples
            .iter()
            .all(|sample| { (*sample - engine_backend::tests::DUMMY_SIGNAL).abs() < 1e-6 }));
        loop_action(
            runtime,
            &keys,
            LoopAction::IconClicked(SelectionModifiers::default()),
        );
        wait_for_snapshot(runtime, "the MIDI detail", |snapshot| {
            snapshot.details.as_ref().is_some_and(|details| {
                details.loop_id == keys.loops[0].id
                    && details
                        .midi_channels
                        .first()
                        .is_some_and(|channel| channel.events.len() == 2)
            })
        });

        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Dummy(shoop_egui::DummyAudioDriverConfig {
                    sample_rate: 44_100,
                    buffer_size: 128,
                }),
            })
            .unwrap();
        let pending = wait_for_snapshot(runtime, "driver-switch confirmation", |snapshot| {
            snapshot.audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::AwaitingConfirmation
        });
        let request_id = pending.audio_drivers.switch.request_id;
        runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: false,
            })
            .unwrap();
        wait_for_snapshot(runtime, "cancelled driver switch", |snapshot| {
            snapshot.audio_drivers.switch.status == shoop_egui::AudioDriverSwitchStatus::Completed
        });

        loop_action(runtime, &audio, LoopAction::StopClicked);
        wait_for_snapshot(runtime, "immediate stop", |snapshot| {
            first_loop(snapshot, audio.id).mode == LoopMode::Stopped
        });
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::ClearRecordings {
                include_sync: false,
            }))
            .unwrap();
        let snapshot = wait_for_snapshot(runtime, "cleared loops", |snapshot| {
            [audio.id, keys.id].iter().all(|id| {
                let state = first_loop(snapshot, *id);
                state.mode == LoopMode::Stopped && state.length_frames == 0
            })
        });
        assert_eq!(first_loop(&snapshot, sync.id).mode, LoopMode::Playing);
        assert!(
            snapshot.notifications.is_empty(),
            "{:?}",
            snapshot.notifications
        );

        let host = app.runtime._engine_host.take().unwrap();
        drop(app);
        let problems = host.stop();
        assert!(
            !problems
                .iter()
                .any(|problem| problem.contains("MIDI") || problem.contains("fault")),
            "{problems:?}"
        );
    }

    /// Opt-in, because it opens the default audio devices: set `SHOOP_TEST_CPAL_DEVICE=1`.
    #[cfg(all(not(target_arch = "wasm32"), feature = "cpal"))]
    #[shoop_wasm_test_support::shoop_test]
    fn new_engine_backend_runs_the_core_workflow_on_a_cpal_device() {
        cpal_core_workflow(shoop_egui::EngineTransportKind::InProcess);
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "cpal"))]
    fn cpal_core_workflow(transport: shoop_egui::EngineTransportKind) {
        use shoop_egui::{
            BusAction, GlobalControlAction, IoTaskStatus, NotificationLevel, StructuralState,
        };

        if std::env::var_os("SHOOP_TEST_CPAL_DEVICE").is_none() {
            eprintln!("skipping the CPAL device workflow; set SHOOP_TEST_CPAL_DEVICE=1");
            return;
        }
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_audio_settings(&mut builder).unwrap();
        shoop_egui::register_audio_backend_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&registry.defaults(1));
        shoop_egui::set_selected_audio_driver(&mut draft, shoop_egui::AudioDriverKind::Cpal);
        if let Ok(host) = std::env::var("SHOOP_TEST_CPAL_HOST") {
            draft.set(shoop_egui::CPAL_HOST, host);
        }
        let document = registry
            .document_from_draft(
                &shoop_settings::SettingsDocument::empty("test"),
                &draft,
                "test",
            )
            .unwrap();
        let settings = registry.resolve(&document, 2).snapshot;
        let args = AppArgs {
            engine_transport: Some(transport),
            ..new_engine_args()
        };
        let mut runtime = Runtime::new_with_engine_start(
            &settings,
            &args,
            |driver, pools, kind| {
                engine_backend::start_selected_with_processors(
                    driver,
                    pools,
                    kind,
                    Default::default(),
                )
            },
            None,
        )
        .unwrap();
        let snapshot = wait_for_snapshot(&runtime, "startup", |snapshot| {
            snapshot.tracks.len() == 1 && snapshot.connections.backend_available
        });
        assert_eq!(
            snapshot
                .audio_drivers
                .active
                .as_ref()
                .unwrap()
                .configured
                .kind(),
            shoop_egui::AudioDriverKind::Cpal,
            "{:?}",
            snapshot.notifications
        );
        eprintln!(
            "CPAL device: {} Hz, {} frames, host ports {:?}",
            snapshot.status.sample_rate,
            snapshot.status.buffer_size,
            snapshot
                .connections
                .host_ports
                .iter()
                .map(|port| port.name.as_str())
                .collect::<Vec<_>>()
        );

        runtime.dispatch(AppIntent::RequestNewSession).unwrap();
        let snapshot = wait_for_snapshot(&runtime, "a new session", |snapshot| {
            snapshot
                .io_task
                .as_ref()
                .is_some_and(|task| task.status == IoTaskStatus::Completed)
        });
        let sync = snapshot.tracks[0].clone();
        let master = snapshot.buses[0].clone();
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Audio".to_owned(),
                audio_channels: 2,
                midi: false,
            }))
            .unwrap();
        let snapshot = wait_for_snapshot(&runtime, "a new track", |snapshot| {
            snapshot.tracks.len() == 2
                && snapshot
                    .tracks
                    .iter()
                    .all(|track| track.structural_state == StructuralState::Confirmed)
        });
        let audio = snapshot.tracks[1].clone();
        let captures = snapshot
            .connections
            .host_ports
            .iter()
            .filter(|port| port.name.contains(":capture_"))
            .map(|port| port.id.clone())
            .collect::<Vec<_>>();
        for (port, capture) in track_ports(&snapshot, audio.id, PortRole::AudioInput)
            .into_iter()
            .zip(captures.iter().cycle())
        {
            runtime
                .dispatch(AppIntent::SetPortConnected {
                    port_id: port,
                    host_port_id: capture.clone(),
                    connected: true,
                })
                .unwrap();
        }
        for (port, channel) in track_ports(&snapshot, audio.id, PortRole::AudioOutput)
            .into_iter()
            .zip(master.channels.iter())
        {
            runtime
                .dispatch(AppIntent::SetMixerRouteConnected {
                    source_port_id: port,
                    destination_channel_id: channel.id,
                    connected: true,
                })
                .unwrap();
        }

        loop_action(&mut runtime, &sync, LoopAction::RecordClicked);
        wait_for_snapshot(&runtime, "sync recording", |snapshot| {
            first_loop(snapshot, sync.id).mode == LoopMode::Recording
        });
        thread::sleep(Duration::from_millis(300));
        loop_action(&mut runtime, &sync, LoopAction::PlayClicked);
        let snapshot = wait_for_snapshot(&runtime, "sync playback", |snapshot| {
            first_loop(snapshot, sync.id).mode == LoopMode::Playing
        });
        let sync_length = first_loop(&snapshot, sync.id).length_frames;
        assert!(sync_length > 0);
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(true)))
            .unwrap();
        loop_action(&mut runtime, &audio, LoopAction::RecordClicked);
        wait_for_snapshot(&runtime, "synchronized recording", |snapshot| {
            first_loop(snapshot, audio.id).mode == LoopMode::Recording
        });
        loop_action(&mut runtime, &audio, LoopAction::PlayClicked);
        let snapshot = wait_for_snapshot(&runtime, "synchronized playback", |snapshot| {
            first_loop(snapshot, audio.id).mode == LoopMode::Playing
        });
        let audio_length = first_loop(&snapshot, audio.id).length_frames;
        assert!(
            audio_length > 0 && audio_length % sync_length == 0,
            "{audio_length} frames for a sync loop of {sync_length}"
        );
        runtime
            .dispatch(AppIntent::Bus {
                bus_id: master.id,
                action: BusAction::GainChanged(-3.0),
            })
            .unwrap();
        loop_action(
            &mut runtime,
            &audio,
            LoopAction::IconClicked(SelectionModifiers::default()),
        );
        wait_for_snapshot(&runtime, "the waveform", |snapshot| {
            snapshot.details.as_ref().is_some_and(|details| {
                details.loop_id == audio.loops[0].id
                    && details.channels.len() == 2
                    && details
                        .channels
                        .iter()
                        .all(|channel| channel.samples.len() as u64 == audio_length)
            })
        });
        let before = runtime.snapshot().status.clone();
        thread::sleep(Duration::from_millis(1000));
        let after = runtime.snapshot().status.clone();
        eprintln!(
            "CPAL device: {} frames and {} xruns in one second, load {:.1}%",
            after
                .processed_frames
                .saturating_sub(before.processed_frames),
            after.xruns,
            after.dsp_load_percent
        );
        assert!(after.processed_frames > before.processed_frames);

        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        loop_action(&mut runtime, &audio, LoopAction::StopClicked);
        wait_for_snapshot(&runtime, "immediate stop", |snapshot| {
            first_loop(snapshot, audio.id).mode == LoopMode::Stopped
        });
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::ClearRecordings {
                include_sync: false,
            }))
            .unwrap();
        let snapshot = wait_for_snapshot(&runtime, "a cleared loop", |snapshot| {
            first_loop(snapshot, audio.id).length_frames == 0
        });
        assert!(
            snapshot
                .notifications
                .iter()
                .all(|notification| notification.level != NotificationLevel::Error),
            "{:?}",
            snapshot.notifications
        );
        let host = runtime._engine_host.take().unwrap();
        drop(runtime);
        let problems = host.stop();
        assert!(
            !problems.iter().any(|problem| problem.contains("fault")),
            "{problems:?}"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn new_engine_capacity_settings_bound_the_application_visibly() {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_audio_settings(&mut builder).unwrap();
        shoop_egui::register_audio_backend_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&registry.defaults(1));
        draft.set(shoop_egui::NEW_ENGINE_TRACKS, 2);
        draft.set(shoop_egui::NEW_ENGINE_LOOPS, 9);
        draft.set(shoop_egui::NEW_ENGINE_CHANNELS, 1);
        draft.set(shoop_egui::NEW_ENGINE_LOOP_MEMORY_MB, 16);
        let document = registry
            .document_from_draft(
                &shoop_settings::SettingsDocument::empty("test"),
                &draft,
                "test",
            )
            .unwrap();
        let settings = registry.resolve(&document, 2).snapshot;
        let mut runtime = Runtime::new(&settings, &AppArgs::default()).unwrap();
        wait_for_snapshot(&runtime, "startup", |snapshot| {
            snapshot.tracks.len() == 1 && snapshot.connections.backend_available
        });
        for (name, audio_channels) in [("Wide", 2), ("One", 1), ("Two", 1)] {
            runtime
                .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                    name: name.to_owned(),
                    audio_channels,
                    midi: false,
                }))
                .unwrap();
        }
        let refused = |snapshot: &AppSnapshot, text: &str| {
            snapshot.notifications.iter().any(|notification| {
                notification.level == shoop_egui::NotificationLevel::Error
                    && notification.message.contains(text)
            })
        };
        let snapshot = wait_for_snapshot(&runtime, "capacity refusals", |snapshot| {
            refused(snapshot, "at most 1 audio channels")
                && refused(snapshot, "all 2 prepared track slots")
        });
        assert_eq!(snapshot.tracks.len(), 2);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn new_engine_driver_settings_saved_without_switching_apply_on_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_audio_settings(&mut builder).unwrap();
        shoop_egui::register_audio_backend_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut manager = SettingsManager::load_from_path(registry.clone(), "test", path.clone());
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&manager.active());
        draft.set(shoop_egui::NEW_ENGINE_TRACKS, 2);
        draft.set(shoop_egui::NEW_ENGINE_LOOPS, 9);
        draft.set(shoop_egui::NEW_ENGINE_LOOP_MEMORY_MB, 16);
        shoop_egui::set_selected_audio_driver(&mut draft, shoop_egui::AudioDriverKind::Dummy);
        draft.set(shoop_egui::DUMMY_SAMPLE_RATE, 32_000);
        draft.set(shoop_egui::DUMMY_BUFFER_SIZE, 64);
        manager.request_save(draft).unwrap();
        wait_for_settings_save(&mut manager);
        drop(manager);

        let restarted = SettingsManager::load_from_path(registry, "test-2", path);
        let runtime = Runtime::new(&restarted.active(), &AppArgs::default()).unwrap();
        let snapshot = wait_for_snapshot(&runtime, "the saved driver", |snapshot| {
            snapshot.status.sample_rate == 32_000 && snapshot.status.buffer_size == 64
        });
        assert!(snapshot
            .capabilities
            .supports(shoop_egui::BackendCapability::AudioDriverSwitching));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn new_engine_startup_fallbacks_are_shown_to_the_user() {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_audio_settings(&mut builder).unwrap();
        shoop_egui::register_audio_backend_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&registry.defaults(1));
        shoop_egui::set_selected_audio_driver(&mut draft, shoop_egui::AudioDriverKind::Cpal);
        draft.set(shoop_egui::CPAL_OUTPUT_DEVICE, "none".to_owned());
        draft.set(shoop_egui::CPAL_MIDI_INPUTS, "none".to_owned());
        draft.set(shoop_egui::CPAL_MIDI_OUTPUTS, "none".to_owned());
        let document = registry
            .document_from_draft(
                &shoop_settings::SettingsDocument::empty("test"),
                &draft,
                "test",
            )
            .unwrap();
        let settings = registry.resolve(&document, 2).snapshot;
        let runtime = Runtime::new(&settings, &new_engine_args()).unwrap();
        let snapshot = wait_for_snapshot(&runtime, "startup", |snapshot| {
            snapshot.tracks.len() == 1 && snapshot.connections.backend_available
        });
        assert!(
            snapshot.notifications.iter().any(|notification| {
                notification.level == shoop_egui::NotificationLevel::Warning
                    && notification.message.contains("using the dummy driver")
            }),
            "{:?}",
            snapshot.notifications
        );
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
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn unavailable_saved_preference_falls_back_without_overwriting_settings() {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_audio_settings(&mut builder).unwrap();
        shoop_egui::register_audio_backend_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&registry.defaults(1));
        shoop_egui::set_selected_audio_driver(&mut draft, shoop_egui::AudioDriverKind::Jack);
        draft.set(shoop_egui::JACK_CLIENT_NAME, String::new());
        let document = registry
            .document_from_draft(
                &shoop_settings::SettingsDocument::empty("test"),
                &draft,
                "test",
            )
            .unwrap();
        let settings = registry.resolve(&document, 2).snapshot;
        let runtime = Runtime::new(&settings, &AppArgs::default()).unwrap();
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn ephemeral_script_files_require_lua_utf8_and_valid_syntax() {
        assert!(is_lua_file_name("controller.lua"));
        assert!(is_lua_file_name("controller.LUA"));
        assert!(!is_lua_file_name("controller.txt"));
        let (name, source) = load_ephemeral_script_bytes(
            "controller.lua".to_owned(),
            b"shoop_announce_api_version(1, 0); print('loaded')",
        )
        .unwrap();
        assert_eq!(name, "controller.lua");
        assert!(source.contains("loaded"));
        assert!(load_ephemeral_script_bytes("controller.txt".to_owned(), b"return").is_err());
        assert!(load_ephemeral_script_bytes("controller.lua".to_owned(), &[0xff]).is_err());
        assert!(load_ephemeral_script_bytes("controller.lua".to_owned(), b"function(").is_err());

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("controller.lua");
        std::fs::write(&path, "shoop_announce_api_version(1, 0)").unwrap();
        let (name, _, source_path) = load_ephemeral_script_path(&path).unwrap();
        assert_eq!(name, "controller.lua");
        assert_eq!(source_path, path.to_string_lossy());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn startup_script_adapter_resolves_typed_bundles_files_and_missing_paths() {
        let directory = tempfile::tempdir().unwrap();
        let user_script = directory.path().join("user.lua");
        std::fs::write(
            &user_script,
            "shoop_announce_api_version(1, 0); print('user')",
        )
        .unwrap();
        let missing = directory.path().join("missing.lua");
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_audio_settings(&mut builder).unwrap();
        shoop_egui::register_audio_backend_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&registry.defaults(1));
        draft.set(
            shoop_egui::BUILTINS_LOCATION,
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../resources/builtins")
                .to_string_lossy()
                .into_owned(),
        );
        draft.set(
            shoop_egui::BUILTIN_SCRIPTS,
            shoop_settings::StringToggleList(vec![shoop_settings::StringToggle {
                value: "keyboard.lua".to_owned(),
                enabled: true,
            }]),
        );
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
                &shoop_settings::SettingsDocument::empty("test"),
                &draft,
                "test",
            )
            .unwrap();
        let settings = registry.resolve(&document, 2).snapshot;
        let (scripts, paths, warnings) = configured_startup_scripts(&settings).unwrap();
        assert_eq!(scripts.len(), 5);
        assert_eq!(paths.len(), 5);
        let keyboard = scripts
            .iter()
            .find(|script| script.identity.as_deref() == Some("keyboard.lua"))
            .unwrap();
        assert_eq!(keyboard.kind, ScriptKind::Bundled);
        assert_eq!(keyboard.source, TEST_KEYBOARD_SCRIPT);
        assert!(keyboard.enabled);
        let dialogs = scripts
            .iter()
            .find(|script| script.identity.as_deref() == Some("examples/dialogs.lua"))
            .unwrap();
        assert_eq!(dialogs.kind, ScriptKind::Example);
        assert_eq!(dialogs.source, TEST_DIALOG_SCRIPT);
        assert!(!dialogs.enabled);
        let user = scripts
            .iter()
            .find(|script| script.kind == ScriptKind::User)
            .unwrap();
        assert_eq!(
            user.source_path.as_deref(),
            Some(user_script.to_str().unwrap())
        );
        assert!(!user.enabled);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("missing.lua"));
        assert!(validate_script_draft(&draft).is_err());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn catalog_adapter_marks_invalid_existing_identities_for_preservation() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("valid.lua"),
            "shoop_announce_api_version(1, 0)",
        )
        .unwrap();
        std::fs::write(directory.path().join("invalid.lua"), "function(").unwrap();
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_audio_settings(&mut builder).unwrap();
        shoop_egui::register_audio_backend_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&registry.defaults(1));
        draft.set(
            shoop_egui::BUILTINS_LOCATION,
            directory.path().to_string_lossy().into_owned(),
        );
        let document = registry
            .document_from_draft(
                &shoop_settings::SettingsDocument::empty("test"),
                &draft,
                "test",
            )
            .unwrap();
        let settings = registry.resolve(&document, 2).snapshot;

        let (scripts, warnings, preserve, deletions_safe) =
            configured_catalog_scripts(&settings, 4).unwrap();
        assert_eq!(
            scripts
                .iter()
                .filter_map(|script| script.identity.as_deref())
                .collect::<Vec<_>>(),
            ["valid.lua"]
        );
        assert_eq!(preserve, ["invalid.lua"]);
        assert!(deletions_safe);
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("invalid.lua")));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn committed_settings_reconcile_scripts_and_failed_save_leaves_runtime_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let script_path = directory.path().join("controller.lua");
        std::fs::write(
            &script_path,
            "shoop_announce_api_version(1, 0); print('controller')",
        )
        .unwrap();
        let settings_directory = directory.path().join("configuration");
        let settings_path = settings_directory.join("settings.json");
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_audio_settings(&mut builder).unwrap();
        shoop_egui::register_audio_backend_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut manager = SettingsManager::load_from_path(registry, "test", settings_path.clone());
        let mut runtime = Runtime::new(&manager.active(), &AppArgs::default()).unwrap();

        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&manager.active());
        draft.set(
            shoop_egui::BUILTINS_LOCATION,
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../resources/builtins")
                .to_string_lossy()
                .into_owned(),
        );
        draft.set(
            shoop_egui::BUILTIN_SCRIPTS,
            shoop_settings::StringToggleList(vec![
                shoop_settings::StringToggle {
                    value: "keyboard.lua".to_owned(),
                    enabled: false,
                },
                shoop_settings::StringToggle {
                    value: "akai_apc_mini_mk1.lua".to_owned(),
                    enabled: true,
                },
            ]),
        );
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
        assert_eq!(snapshot.scripting.scripts.len(), 5);

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
        let after_removal = wait_for_script_count(&mut runtime, 4);
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
        let mut toggles = failing.get(shoop_egui::BUILTIN_SCRIPTS).unwrap();
        toggles
            .0
            .iter_mut()
            .find(|entry| entry.value == "keyboard.lua")
            .unwrap()
            .enabled = true;
        failing.set(shoop_egui::BUILTIN_SCRIPTS, toggles);
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

    #[cfg(not(target_arch = "wasm32"))]
    fn wait_for_settings_save(manager: &mut SettingsManager) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while manager.view().persistence == shoop_settings::SettingsPersistenceState::Saving {
            manager.poll();
            assert!(Instant::now() < deadline, "settings save timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
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

    #[cfg(not(target_arch = "wasm32"))]
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
            if snapshot.scripting.scripts.len() == 5
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn file_identity_kinds_exclude_session_and_ephemeral_scripts() {
        assert!(script_kind_has_file_identity(ScriptKind::Bundled));
        assert!(script_kind_has_file_identity(ScriptKind::Example));
        assert!(script_kind_has_file_identity(ScriptKind::User));
        assert!(!script_kind_has_file_identity(ScriptKind::Session));
        assert!(!script_kind_has_file_identity(ScriptKind::Ephemeral));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn file_input_capacity_stays_within_the_limit_when_growth_would_double_it() {
        let mut bytes = vec![1; 45];
        reserve_file_bytes(&mut bytes, 20, 70).unwrap();
        bytes.extend_from_slice(&[2; 20]);
        assert!(bytes.capacity() <= 70);
        assert!(reserve_file_bytes(&mut bytes, 6, 70).is_err());
        assert_eq!(bytes.len(), 65);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn native_atomic_replace_overwrites_and_cleans_up_failed_temporary_files() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("session.shoop");
        std::fs::write(&target, b"old").unwrap();
        let unrelated = directory.path().join("session.shoop.tmp-7");
        std::fs::write(&unrelated, b"unrelated").unwrap();
        atomic_replace(&target, b"new", 7).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert_eq!(std::fs::read(&unrelated).unwrap(), b"unrelated");

        let missing_target = directory.path().join("missing").join("session.shoop");
        assert!(atomic_replace(&missing_target, b"partial", 8).is_err());
        assert!(!missing_target.exists());
        assert!(!directory
            .path()
            .join("missing")
            .join("session.shoop.tmp-8")
            .exists());
        let blocked = directory.path().join("blocked.shoop");
        std::fs::create_dir(&blocked).unwrap();
        std::fs::write(blocked.join("keep"), b"keep").unwrap();
        assert!(atomic_replace(&blocked, b"partial", 9).is_err());
        assert_eq!(std::fs::read(blocked.join("keep")).unwrap(), b"keep");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 3);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn native_file_input_enforces_cancellation_and_the_file_ceiling() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input.shoop");
        std::fs::write(&path, b"input").unwrap();
        let mut slot = platform_input::InputSlot::default();
        let lease = slot.reserve(shoop_egui::TaskId::from_raw(1)).unwrap();
        assert_eq!(read_session_input(&path, &lease).unwrap(), b"input");
        slot.reconcile(None);
        assert!(read_session_input(&path, &lease)
            .unwrap_err()
            .to_string()
            .contains("cancelled"));
        drop(lease);
        let lease = slot.reserve(shoop_egui::TaskId::from_raw(2)).unwrap();
        std::fs::File::create(&path)
            .unwrap()
            .set_len(shoop_app_protocol::persistence::MAX_SESSION_FILE_BYTES + 1)
            .unwrap();
        assert!(read_session_input(&path, &lease)
            .unwrap_err()
            .to_string()
            .contains("512 MiB"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn unified_application_paints_at_minimum_and_common_sizes() {
        for size in [egui::vec2(360.0, 200.0), egui::vec2(900.0, 600.0)] {
            let context = egui::Context::default();
            shoop_egui::initialize(&context);
            let mut app = UnifiedApp::new(1.0, false, &AppArgs::default()).unwrap();
            let snapshot = app.runtime.snapshot();
            assert_eq!(snapshot.tracks.len(), 1);
            assert!(snapshot.tracks[0].is_sync);
            assert_eq!(snapshot.tracks[0].loops.len(), 1);
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| app.show(ui),
            );
            assert!(!output.shapes.is_empty());
            output.textures_delta.clear();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn unified_native_app_runs_paints_invokes_and_removes_lua_dialogs() {
        let context = egui::Context::default();
        shoop_egui::initialize(&context);
        let mut app = UnifiedApp::new(1.0, false, &AppArgs::default()).unwrap();
        for (name, source) in [
            (
                "lua-api-higher-minor.lua",
                "shoop_announce_api_version(1, 7); require('shoop_control').set_solo(true)",
            ),
            (
                "lua-api-lower-major.lua",
                "shoop_announce_api_version(0, 0); require('shoop_control').set_solo(true)",
            ),
            (
                "lua-api-higher-major.lua",
                "shoop_announce_api_version(2, 0); require('shoop_control').set_solo(true)",
            ),
            (
                "lua-api-unannounced.lua",
                "require('shoop_control').set_solo(true)",
            ),
        ] {
            app.runtime
                .dispatch(AppIntent::AddScriptSource {
                    name: name.to_owned(),
                    source: std::sync::Arc::from(source),
                    kind: ScriptKind::User,
                    enabled: true,
                })
                .unwrap();
        }
        let started = Instant::now();
        loop {
            let snapshot = app.runtime.snapshot();
            let rejected = snapshot
                .scripting
                .scripts
                .iter()
                .filter(|script| script.name.starts_with("lua-api-"))
                .collect::<Vec<_>>();
            if rejected.len() == 4
                && rejected
                    .iter()
                    .all(|script| lua_version_rejection_is_expected(script))
            {
                assert!(!snapshot.global_controls.solo);
                assert!(snapshot.scripting.dialogs.is_empty());
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        }
        app.runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "dialogs.lua".to_owned(),
                source: std::sync::Arc::from(TEST_DIALOG_SCRIPT),
                kind: ScriptKind::User,
                enabled: true,
            })
            .unwrap();
        app.runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "dialog-survivor.lua".to_owned(),
                source: std::sync::Arc::from(
                    "shoop_announce_api_version(1, 0); local d=require('shoop_dialog'); d.simple('Survivor', {d.rich_text('Still active')})",
                ),
                kind: ScriptKind::User,
                enabled: true,
            })
            .unwrap();
        let snapshot = loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.scripting.dialogs.len() == 3 {
                break snapshot;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(snapshot.scripting.dialogs[0].open_request, 1);
        let owner = snapshot.scripting.dialogs[0].owner_script_id;
        let dialog_id = snapshot.scripting.dialogs[0].id;
        let shoop_egui::ScriptDialogKind::Simple(content) = &snapshot.scripting.dialogs[0].kind
        else {
            panic!("expected simple dialog");
        };
        let shoop_egui::ScriptDialogElement::Button {
            id: Some(button_id),
            ..
        } = &content.elements[1]
        else {
            panic!("expected callback button");
        };
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                ..Default::default()
            },
            |ui| app.show(ui),
        );
        assert!(!output.shapes.is_empty());
        output.textures_delta.clear();
        app.runtime
            .dispatch(AppIntent::InvokeScriptDialogButton {
                script_id: owner,
                dialog_id,
                button_id: *button_id,
            })
            .unwrap();
        let updated = loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.global_controls.solo && snapshot.scripting.dialogs[1].open_request == 1 {
                break snapshot;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(
            updated.scripting.dialogs[0].owner_script_name,
            "dialogs.lua"
        );
        app.runtime
            .dispatch(AppIntent::StopScript { script_id: owner })
            .unwrap();
        loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.scripting.dialogs.len() == 1
                && snapshot.scripting.dialogs[0].name == "Survivor"
            {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn native_preview_waits_for_actor_snapshot_publication() {
        let mut app = UnifiedApp::new(1.0, false, &AppArgs::default()).unwrap();
        let request_id = app.runtime.snapshot().click_track.preview_request_id + 1;
        app.runtime.pending_preview = Some(application::ApplicationAudioPreview {
            output: shoop_egui::ClickTrackPreviewOutput::MasterBus,
            request_id,
            sample_rate: 48_000,
            samples: Arc::from([0.25; 4]),
        });
        app.runtime.process_audio_previews();
        assert_eq!(
            app.runtime
                .pending_preview
                .as_ref()
                .map(|preview| preview.request_id),
            Some(request_id)
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn native_actor_delivers_system_preview_pcm_without_using_the_host_driver() {
        let mut app = UnifiedApp::new(1.0, false, &new_engine_args()).unwrap();
        let state = wait_for_snapshot(&app.runtime, "preview target", |s| !s.tracks.is_empty());
        let loop_id = state.tracks[0].loops[0].id;
        app.runtime
            .dispatch(AppIntent::PreviewClickTrack {
                loop_id,
                request: Default::default(),
                output: shoop_egui::ClickTrackPreviewOutput::System,
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let preview = loop {
            if let Some(preview) = app.runtime.handle.take_audio_preview() {
                break preview;
            }
            assert!(
                Instant::now() < deadline,
                "preview mailbox timed out: {:?}",
                app.runtime.snapshot().notifications
            );
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(preview.output, shoop_egui::ClickTrackPreviewOutput::System);
        assert!(preview.samples.iter().any(|v| v.abs() > 0.));
        app.runtime
            .dispatch(AppIntent::StopClickTrackPreview)
            .unwrap();
        wait_for_snapshot(&app.runtime, "preview cancellation", |s| {
            s.click_track.preview_status == shoop_egui::ClickTrackPreviewStatus::Cancelled
        });
    }
    #[cfg(all(not(target_arch = "wasm32"), feature = "system-preview"))]
    #[shoop_wasm_test_support::shoop_test]
    #[ignore = "requires an isolated system audio device and external signal capture"]
    fn native_system_preview_device_acceptance() {
        let mut app = UnifiedApp::new(1.0, false, &new_engine_args()).unwrap();
        let state = wait_for_snapshot(&app.runtime, "preview target", |s| !s.tracks.is_empty());
        let loop_id = state.tracks[0].loops[0].id;
        let before = state.tracks[0].loops[0].content_revision;
        for stop in [false, true] {
            app.runtime
                .dispatch(AppIntent::PreviewClickTrack {
                    loop_id,
                    request: shoop_egui::ClickTrackRequest {
                        click_count: if stop { 16 } else { 2 },
                        ..Default::default()
                    },
                    output: shoop_egui::ClickTrackPreviewOutput::System,
                })
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                app.runtime.process_audio_previews();
                let state = app.runtime.snapshot();
                assert_ne!(
                    state.click_track.preview_status,
                    shoop_egui::ClickTrackPreviewStatus::Failed,
                    "{:?}",
                    state.click_track
                );
                if state.click_track.preview_status == shoop_egui::ClickTrackPreviewStatus::Playing
                {
                    break;
                }
                assert!(Instant::now() < deadline, "preview did not start");
                thread::sleep(Duration::from_millis(5));
            }
            if stop {
                thread::sleep(Duration::from_millis(250));
                app.runtime
                    .dispatch(AppIntent::StopClickTrackPreview)
                    .unwrap();
                wait_for_snapshot(&app.runtime, "preview stop", |s| {
                    s.click_track.preview_status == shoop_egui::ClickTrackPreviewStatus::Cancelled
                });
                app.runtime.process_audio_previews();
                thread::sleep(Duration::from_millis(750));
            } else {
                loop {
                    app.runtime.process_audio_previews();
                    if app.runtime.snapshot().click_track.preview_status
                        == shoop_egui::ClickTrackPreviewStatus::Completed
                    {
                        break;
                    }
                    assert!(Instant::now() < deadline, "preview did not finish");
                    thread::sleep(Duration::from_millis(5));
                }
            }
            assert_eq!(
                app.runtime.snapshot().tracks[0].loops[0].content_revision,
                before
            );
        }
    }
}
