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
#[cfg(all(test, not(target_arch = "wasm32")))]
use shoop_egui::register_settings;
#[cfg(not(target_arch = "wasm32"))]
use shoop_egui::AudioDriverConfig;
use shoop_egui::{
    register_audio_settings, register_settings_with_appearance_defaults, AppIntent, AppSnapshot,
    AppWidget, ScriptKind, SettingsAction, SettingsRegistryBuilder, UI_SCALE_FACTOR,
};
use shoop_egui::{TracingStatus, TracingStopped};

#[cfg(target_arch = "wasm32")]
use shoop_app::CooperativeApplicationRuntime;
mod app_args;
#[cfg(target_arch = "wasm32")]
mod browser_audio;
#[cfg(any(target_arch = "wasm32", test))]
mod browser_midi;
#[cfg(target_arch = "wasm32")]
mod browser_preview;
#[cfg(target_arch = "wasm32")]
mod browser_trace;
#[cfg(target_arch = "wasm32")]
mod browser_worker;
#[cfg(not(target_arch = "wasm32"))]
mod native_preview;
mod settings;
use app_args::AppArgs;
#[cfg(not(target_arch = "wasm32"))]
use shoop_app::StartupScript;
#[cfg(not(target_arch = "wasm32"))]
use shoop_app::{ApplicationHandle, ApplicationRuntime};

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

#[derive(Clone, Copy)]
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
        let global = js_sys::global();
        let isolated = js_sys::Reflect::get(&global, &"crossOriginIsolated".into())
            .ok()
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let has_shared_buffer = js_sys::Reflect::get(&global, &"SharedArrayBuffer".into())
            .is_ok_and(|value| value.is_function());
        let unavailable_reason = (!isolated || !has_shared_buffer)
            .then_some("Multirealm tracing requires COOP/COEP cross-origin isolation");
        Self {
            capture: None,
            window_calibrations: Vec::new(),
            unavailable_reason,
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

#[cfg(not(target_arch = "wasm32"))]
struct PendingAudioSettings {
    request_id: Option<u64>,
    config: AudioDriverConfig,
    draft: shoop_settings::SettingsDraft,
    saving: bool,
    retry_requested: bool,
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

fn show_about_dialog(context: &egui::Context, open: &mut bool, identity: BuildIdentity) {
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
        });
}

struct UnifiedApp {
    runtime: Runtime,
    widget: AppWidget,
    about_open: bool,
    settings: SettingsManager,
    #[cfg(not(target_arch = "wasm32"))]
    pending_audio_settings: Option<PendingAudioSettings>,
    #[cfg(not(target_arch = "wasm32"))]
    tracing: Option<SharedNativeTracing>,
    #[cfg(target_arch = "wasm32")]
    tracing: BrowserTracing,
    #[cfg(target_arch = "wasm32")]
    tracing_smoke_started: Option<Instant>,
    pending_tracing_action: Option<PendingTracingAction>,
    last_update: Instant,
    startup_session: Option<(String, bool)>,
    session_url_input: String,
    session_url_error: Option<String>,
    session_url_prompt_open: bool,
    session_url_confirmation: Option<String>,
    #[cfg(target_arch = "wasm32")]
    browser_self_test: BrowserSelfTest,
    #[cfg(target_arch = "wasm32")]
    browser_settings_test: BrowserSettingsSelfTest,
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

impl UnifiedApp {
    fn new(
        ui_scale_default: f64,
        touch_mode_default: bool,
        args: &AppArgs,
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
        register_audio_settings(&mut settings_builder)?;
        #[cfg(all(not(target_arch = "wasm32"), feature = "native-fx"))]
        register_carla_settings(&mut settings_builder)?;
        #[cfg(not(target_arch = "wasm32"))]
        register_script_settings(&mut settings_builder)?;
        #[cfg(target_arch = "wasm32")]
        register_bundled_script_settings(&mut settings_builder)?;
        let settings_registry = settings_builder.finish();
        let settings = load_settings_manager(settings_registry.clone(), args);
        let mut widget = AppWidget::new(std::sync::Arc::new(settings_registry));
        #[cfg(not(target_arch = "wasm32"))]
        let runtime = Runtime::new(&settings.active())?;
        #[cfg(target_arch = "wasm32")]
        let runtime = Runtime::new(&settings.active(), args)?;
        widget.set_click_track_preview_available(runtime.audio_preview_available());
        Ok(Self {
            runtime,
            widget,
            about_open: false,
            settings,
            #[cfg(not(target_arch = "wasm32"))]
            pending_audio_settings: None,
            #[cfg(not(target_arch = "wasm32"))]
            tracing: None,
            #[cfg(target_arch = "wasm32")]
            tracing: BrowserTracing::new(),
            #[cfg(target_arch = "wasm32")]
            tracing_smoke_started: None,
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
            session_url_confirmation: None,
            #[cfg(target_arch = "wasm32")]
            browser_self_test: BrowserSelfTest::from_args(args),
            #[cfg(target_arch = "wasm32")]
            browser_settings_test: BrowserSettingsSelfTest::from_args(args),
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
                load_session_path(source, self.pending_file_intent_tx.clone());
                #[cfg(target_arch = "wasm32")]
                tracing::warn!(source = %source, "frontend.session.filesystem_path_unavailable");
            }
        }
    }

    fn show_session_url_dialogs(&mut self, context: &egui::Context) {
        if self.session_url_prompt_open {
            let mut submit = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("load_session_url_prompt")).show(context, |ui| {
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
            egui::Modal::new(egui::Id::new("confirm_session_url_fetch")).show(context, |ui| {
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
            }
        }
    }

    fn fetch_session_url(&self, url: String) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let pending = self.pending_file_intent_tx.clone();
            let name = session_source_name(&url);
            ehttp::fetch(ehttp::Request::get(&url), move |result| {
                let _ = pending.send(session_fetch_result(name, result));
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let pending = Rc::clone(&self.pending_file_intents);
            let name = session_source_name(&url);
            wasm_bindgen_futures::spawn_local(async move {
                let result = ehttp::fetch_async(ehttp::Request::get(&url)).await;
                pending
                    .borrow_mut()
                    .push_back(session_fetch_result(name, result));
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
        if let Err(error) = self.tracing.poll() {
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
        if self.args.tracing_smoke_test && status.active && self.runtime.tracing_realm_ready() {
            let started = self.tracing_smoke_started.get_or_insert_with(|| {
                tracing::info!("frontend.egui.tracing_smoke_realm_ready");
                Instant::now()
            });
            if started.elapsed() >= Duration::from_secs(3) && self.pending_tracing_action.is_none()
            {
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
        let kind = action.kind();
        let _span = tracing::debug_span!("frontend.egui.settings_action", action = kind).entered();
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
                                retry_requested: false,
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
                pending.map(|pending| pending.retry_requested = true)
            }
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
        let result = match action {
            SettingsAction::Save(draft) => self.settings.request_save(draft),
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
        let view = self.settings.view();
        let desired_settings_saved = view.persistence
            == shoop_settings::SettingsPersistenceState::Saved
            && view.active.revision() > pending.draft.base_revision()
            && shoop_egui::selected_audio_driver(&view.active).ok() == Some(pending.config.kind())
            && shoop_egui::audio_driver_config_from_snapshot(&view.active, pending.config.kind())
                .ok()
                == Some(pending.config.clone());
        if desired_settings_saved {
            let _ = self
                .runtime
                .dispatch(AppIntent::CompleteAudioDriverSwitchPersistence {
                    request_id,
                    success: true,
                    message: "Audio driver switched and saved for the next launch".to_owned(),
                });
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
            if view.persistence == shoop_settings::SettingsPersistenceState::Failed {
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
        if pending.retry_requested
            && !pending.saving
            && self.settings.view().persistence != shoop_settings::SettingsPersistenceState::Saving
        {
            pending.retry_requested = false;
            match self.settings.request_save(pending.draft.clone()) {
                Ok(()) => pending.saving = true,
                Err(error) => self.settings.report_action_error(error.to_string()),
            }
        }
        self.pending_audio_settings = Some(pending);
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
            egui::Area::new(egui::Id::new("lua_file_drop_overlay"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(context, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.heading("Drop Lua script to load it for this app run");
                    });
                });
        }
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        if let Some((source, force_url_session)) = self.startup_session.take() {
            self.process_startup_session_source(source, force_url_session);
        }
        let _span = tracing::trace_span!("frontend.egui.update").entered();
        self.settings.poll();
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
            if let Err(error) = self.runtime.dispatch(intent) {
                tracing::error!(error = %error, "frontend.egui.file_intent_dispatch_failed");
            }
        }
        let snapshot = self.runtime.snapshot();
        #[cfg(not(target_arch = "wasm32"))]
        self.reconcile_audio_settings(&snapshot);
        #[cfg(target_arch = "wasm32")]
        self.browser_self_test
            .update(&mut self.runtime, &snapshot, &mut self.widget, &self.args);
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
        self.about_open |= response.about_requested;
        for intent in response.app_actions {
            self.handle_ui_intent(intent);
        }
        for action in response.settings_actions {
            self.handle_settings_action(action);
        }
        show_about_dialog(ui.ctx(), &mut self.about_open, BuildIdentity::CURRENT);
        self.show_file_drop_overlay(ui.ctx());
        self.show_session_url_dialogs(ui.ctx());
        #[cfg(not(target_arch = "wasm32"))]
        let drain_file_outputs = true;
        #[cfg(target_arch = "wasm32")]
        let drain_file_outputs = matches!(
            self.browser_self_test,
            BrowserSelfTest::Disabled | BrowserSelfTest::Complete | BrowserSelfTest::Failed
        );
        if drain_file_outputs {
            while let Some(output) = self.runtime.take_file_output() {
                #[cfg(not(target_arch = "wasm32"))]
                save_file_output(output, self.pending_file_intent_tx.clone());
                #[cfg(target_arch = "wasm32")]
                save_file_output(output, Rc::clone(&self.pending_file_intents));
            }
        }
        ui.ctx().request_repaint_after(UPDATE_INTERVAL);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_ui_intent(&mut self, intent: AppIntent) {
        let kind = intent.kind();
        let _span = tracing::debug_span!("frontend.egui.intent_dispatch", intent = kind).entered();
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
                            let message = format!("Could not read {}: {error}", path.display());
                            let _ = sender.send(AppIntent::FailIoWorkflow {
                                kind: shoop_egui::IoTaskKind::LoadSession,
                                message,
                            });
                        }
                    });
                }
                None
            }
            AppIntent::RequestLoadSessionUrl => {
                self.session_url_prompt_open = true;
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
                            let message = format!("Could not read {}: {error}", path.display());
                            let _ = sender.send(AppIntent::FailIoWorkflow {
                                kind: shoop_egui::IoTaskKind::ImportLoopAudio,
                                message,
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
                            let message = format!("Could not read {}: {error}", path.display());
                            let _ = sender.send(AppIntent::FailIoWorkflow {
                                kind: shoop_egui::IoTaskKind::ImportLoopMidi,
                                message,
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
                tracing::error!(error = %error, "frontend.egui.intent_dispatch_failed");
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn handle_ui_intent(&mut self, intent: AppIntent) {
        let kind = intent.kind();
        let _span = tracing::debug_span!("frontend.egui.intent_dispatch", intent = kind).entered();
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
            AppIntent::RequestLoadSessionUrl => {
                self.session_url_prompt_open = true;
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
                    tracing::error!(error = %error, "frontend.egui.intent_dispatch_failed");
                }
            }
        }
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
fn save_file_output(output: shoop_app::ApplicationFileOutput, sender: Sender<AppIntent>) {
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(&output.suggested_name)
        .save_file()
    else {
        return;
    };
    std::thread::spawn(move || {
        if let Err(error) = atomic_replace(&path, &output.bytes, output.task_id.raw()) {
            let _ = sender.send(AppIntent::FailIoTask {
                task_id: output.task_id,
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

fn session_fetch_result(name: String, result: ehttp::Result<ehttp::Response>) -> AppIntent {
    match result {
        Ok(response) if response.ok => AppIntent::LoadSessionBytes {
            name,
            bytes: std::sync::Arc::from(response.bytes),
        },
        Ok(response) => AppIntent::FailIoWorkflow {
            kind: shoop_egui::IoTaskKind::LoadSession,
            message: format!("Could not fetch session: HTTP {}", response.status),
        },
        Err(error) => AppIntent::FailIoWorkflow {
            kind: shoop_egui::IoTaskKind::LoadSession,
            message: format!("Could not fetch session: {error}"),
        },
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load_session_path(source: String, sender: Sender<AppIntent>) {
    std::thread::spawn(move || {
        let path = Path::new(&source);
        match std::fs::read(path) {
            Ok(bytes) => {
                let _ = sender.send(AppIntent::LoadSessionBytes {
                    name: file_name(path),
                    bytes: std::sync::Arc::from(bytes),
                });
            }
            Err(error) => {
                let _ = sender.send(AppIntent::FailIoWorkflow {
                    kind: shoop_egui::IoTaskKind::LoadSession,
                    message: format!("Could not read {}: {error}", path.display()),
                });
            }
        }
    });
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
                pending.borrow_mut().push_back(AppIntent::FailIoTask {
                    task_id: output.task_id,
                    message: format!("Could not save browser file: {error}"),
                });
            }
        }
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

fn reconcile_loop_smoothing_settings(
    settings: &shoop_settings::SettingsSnapshot,
    applied_revision: &mut u64,
    mut dispatch: impl FnMut(AppIntent) -> Result<(), shoop_app::DispatchError>,
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
    dispatch(AppIntent::SetLoopSmoothingMs(milliseconds))?;
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
    applied_audio_settings_revision: u64,
    applied_settings_revision: u64,
}

#[cfg(not(target_arch = "wasm32"))]
type CatalogScanCompletion = (
    u64,
    Result<(Vec<StartupScript>, Vec<String>, Vec<String>, bool), String>,
);

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
        let (mut backend, backend_warning) = NativeBackend::new_with_fallback(configured_driver)?;
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
        shoop_backend::Backend::set_loop_smoothing_ms(&mut backend, loop_smoothing_ms)?;
        let (startup_scripts, script_paths, mut warnings) = configured_startup_scripts(settings)?;
        warnings.extend(configuration_warning);
        warnings.extend(loop_smoothing_warning);
        #[cfg(feature = "native-fx")]
        warnings.extend(carla_configuration_warning);
        warnings.extend(backend_warning);
        let runtime = ApplicationRuntime::start_with_scripts(Box::new(backend), startup_scripts)?;
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
            applied_audio_settings_revision: settings.revision(),
            applied_settings_revision: settings.revision(),
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
        reconcile_loop_smoothing_settings(
            settings,
            &mut self.applied_audio_settings_revision,
            move |intent| handle.dispatch(intent),
        )
    }

    fn reconcile_script_settings(
        &mut self,
        settings: &shoop_settings::SettingsSnapshot,
    ) -> anyhow::Result<()> {
        if settings.revision() == self.applied_settings_revision {
            return Ok(());
        }
        self.request_builtin_rescan(settings);
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

#[cfg(target_arch = "wasm32")]
enum BrowserRuntimeMode {
    WebAudio(browser_audio::BrowserAudioController),
    Worker(browser_worker::BrowserWorkerDriver),
}

#[cfg(target_arch = "wasm32")]
struct Runtime {
    runtime: CooperativeApplicationRuntime,
    mode: BrowserRuntimeMode,
    midi: browser_midi::BrowserMidiController,
    preview_player: browser_preview::BrowserPreviewPlayer,
    catalog_scan_generation: u64,
    catalog_completions: Rc<RefCell<VecDeque<BrowserCatalogCompletion>>>,
    applied_audio_settings_revision: u64,
    applied_settings_revision: u64,
    tracing_requested: bool,
    tracing_engine_detail: bool,
    tracing_realm_active: bool,
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
        let offline = args.offline;
        let worker = args.worker;
        let (midi, midi_service) = browser_midi::BrowserMidiController::new()?;
        let (mut backend, transport) = shoop_worklet_client::RemoteWorkletBackend::new(midi.hub());
        let loop_smoothing_ms = match shoop_egui::loop_edge_smoothing_ms(settings) {
            Ok(milliseconds) => milliseconds,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "frontend.audio.loop_smoothing_settings_fallback"
                );
                3
            }
        };
        shoop_backend::Backend::set_loop_smoothing_ms(&mut backend, loop_smoothing_ms)?;
        let mode = if worker || offline {
            set_offline_audio_permission_presentation();
            BrowserRuntimeMode::Worker(browser_worker::BrowserWorkerDriver::new(transport)?)
        } else {
            BrowserRuntimeMode::WebAudio(browser_audio::BrowserAudioController::new(transport)?)
        };
        let mut runtime = Self {
            runtime: CooperativeApplicationRuntime::start_with_scripts_and_midi(
                Box::new(backend),
                Vec::new(),
                midi_service,
            )?,
            mode,
            midi,
            preview_player: browser_preview::BrowserPreviewPlayer::default(),
            catalog_scan_generation: 0,
            catalog_completions: Rc::new(RefCell::new(VecDeque::new())),
            applied_audio_settings_revision: settings.revision(),
            applied_settings_revision: settings.revision(),
            tracing_requested: false,
            tracing_engine_detail: false,
            tracing_realm_active: false,
        };
        runtime.request_builtin_rescan(settings);
        Ok(runtime)
    }

    fn set_repaint_context(&self, context: egui::Context) {
        match &self.mode {
            BrowserRuntimeMode::WebAudio(controller) => {
                controller.set_repaint_context(context);
            }
            BrowserRuntimeMode::Worker(driver) => driver.set_repaint_context(context),
        }
    }

    fn start_tracing(&mut self, engine_detail: bool) -> anyhow::Result<()> {
        self.tracing_requested = true;
        self.tracing_engine_detail = engine_detail;
        self.ensure_tracing_realm()
    }

    fn ensure_tracing_realm(&mut self) -> anyhow::Result<()> {
        if self.tracing_realm_active {
            self.tracing_realm_active = match &self.mode {
                BrowserRuntimeMode::WebAudio(controller) => controller.has_active_trace(),
                BrowserRuntimeMode::Worker(driver) => driver.has_active_trace(),
            };
        }
        if !self.tracing_requested || self.tracing_realm_active {
            return Ok(());
        }
        self.tracing_realm_active = match &self.mode {
            BrowserRuntimeMode::WebAudio(controller) => {
                controller.start_tracing(self.tracing_engine_detail)?
            }
            BrowserRuntimeMode::Worker(driver) => {
                driver.start_tracing(self.tracing_engine_detail)?
            }
        };
        Ok(())
    }

    fn poll_tracing(&self) -> anyhow::Result<()> {
        match &self.mode {
            BrowserRuntimeMode::WebAudio(controller) => controller.poll_tracing(),
            BrowserRuntimeMode::Worker(driver) => driver.poll_tracing(),
        }
    }

    fn cancel_tracing_request(&mut self) {
        self.tracing_requested = false;
        match &self.mode {
            BrowserRuntimeMode::WebAudio(controller) => controller.discard_tracing(),
            BrowserRuntimeMode::Worker(driver) => driver.discard_tracing(),
        }
        self.tracing_realm_active = false;
    }

    fn tracing_realm_ready(&self) -> bool {
        match &self.mode {
            BrowserRuntimeMode::WebAudio(controller) => {
                controller.state() == shoop_backend::BackendDriverState::Running
            }
            BrowserRuntimeMode::Worker(driver) => {
                driver.state() == shoop_backend::BackendDriverState::Dummy
            }
        }
    }

    fn request_stop_tracing(&mut self) -> anyhow::Result<()> {
        self.tracing_requested = false;
        match &self.mode {
            BrowserRuntimeMode::WebAudio(controller) => controller.request_stop_tracing(),
            BrowserRuntimeMode::Worker(driver) => driver.request_stop_tracing(),
        }
    }

    fn take_trace_realms(
        &mut self,
    ) -> anyhow::Result<Option<Vec<shoop_tracing::BrowserRealmData>>> {
        if !self.tracing_realm_active {
            if let BrowserRuntimeMode::WebAudio(controller) = &self.mode {
                if let Some(realms) = controller.take_traces()? {
                    return Ok(Some(realms));
                }
            }
            return Ok(Some(Vec::new()));
        }
        let realms = match &self.mode {
            BrowserRuntimeMode::WebAudio(controller) => controller.take_traces()?,
            BrowserRuntimeMode::Worker(driver) => driver.take_trace()?.map(|realm| vec![realm]),
        };
        let Some(realms) = realms else {
            return Ok(None);
        };
        self.tracing_realm_active = false;
        Ok(Some(realms))
    }

    fn tick(&mut self, elapsed: Duration) {
        let completions = self
            .catalog_completions
            .borrow_mut()
            .drain(..)
            .collect::<Vec<_>>();
        for (generation, result) in completions {
            if generation != self.catalog_scan_generation {
                continue;
            }
            match result {
                Ok((scripts, warnings, preserve_identities)) => {
                    for warning in warnings {
                        tracing::warn!(warning = %warning, "frontend.scripting.catalog_scan_warning");
                    }
                    let _ = self.runtime.dispatch(AppIntent::ReconcileCatalogScripts {
                        scripts: scripts.into(),
                        preserve_identities: preserve_identities.into(),
                    });
                }
                Err(error) => {
                    tracing::error!(error = %error, "frontend.scripting.catalog_scan_failed");
                }
            }
        }
        self.runtime.tick(elapsed);
        self.midi.update_presentation();
        let snapshot = self.runtime.snapshot();
        let message = match &self.mode {
            BrowserRuntimeMode::WebAudio(controller) => {
                controller.update_presentation();
                format!("Browser audio: {:?}", controller.state())
            }
            BrowserRuntimeMode::Worker(driver) => {
                driver.update_presentation();
                format!("Browser offline Worker engine: {:?}", driver.state())
            }
        };
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

    fn reconcile_audio_settings(
        &mut self,
        settings: &shoop_settings::SettingsSnapshot,
    ) -> anyhow::Result<()> {
        let runtime = &mut self.runtime;
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
        self.request_builtin_rescan(settings);
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
                BrowserRuntimeMode::Worker(_) => None,
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
            BrowserRuntimeMode::Worker(driver) => {
                matches!(
                    driver.state(),
                    shoop_backend::BackendDriverState::Running
                        | shoop_backend::BackendDriverState::Dummy
                )
            }
        }
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
        1.0
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
fn main() {
    #[cfg(feature = "native-fx")]
    match shoop_backend::run_carla_worker_if_requested(std::env::args_os()) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("Carla worker failed: {error:#}");
            std::process::exit(2);
        }
    }

    let cli = AppArgs::parse();
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
    #[cfg(feature = "native-fx")]
    if cli.probe_carla_native || cli.probe_carla_native_ui {
        let result = if cli.probe_carla_native_ui {
            shoop_backend::smoke_test_carla_ui()
        } else {
            shoop_backend::smoke_test_carla_runtime()
        };
        match result {
            Ok(()) => {
                let path = shoop_backend::carla_runtime_path()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|_| "<unknown>".to_owned());
                println!("Carla Native runtime is available: {path}");
                return;
            }
            Err(error) => {
                eprintln!("Carla Native runtime probe failed: {error:#}");
                std::process::exit(3);
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

#[cfg(target_arch = "wasm32")]
const BROWSER_SESSION_SCRIPT_NAME: &str = "browser-self-test-session.lua";
#[cfg(target_arch = "wasm32")]
const BROWSER_SESSION_SCRIPT_SOURCE: &str =
    "shoop_announce_api_version(1, 0); local shoop_control = require('shoop_control'); local dialog = require('shoop_dialog'); local markdown = dialog.markdown_file('help.md'); shoop_control.register_keyboard_event_cb(function(_) end)";
#[cfg(target_arch = "wasm32")]
const BROWSER_SESSION_MARKDOWN: &[u8] = b"# Browser bundle\n\n![resource](image.png)";
#[cfg(target_arch = "wasm32")]
const BROWSER_SESSION_IMAGE: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[cfg(target_arch = "wasm32")]
fn browser_session_script_bundle() -> std::sync::Arc<shoop_scripting::ScriptResourceBundle> {
    use shoop_script_resources::{
        NormalizedRelativePath, ResourceKind, ResourceLimits, ScriptResource, ScriptResourceBundle,
    };

    let entrypoint = NormalizedRelativePath::parse("main.lua").unwrap();
    std::sync::Arc::new(
        ScriptResourceBundle::new(
            entrypoint.clone(),
            std::collections::BTreeMap::from([
                (
                    entrypoint,
                    ScriptResource::new(
                        ResourceKind::Lua,
                        std::sync::Arc::<[u8]>::from(BROWSER_SESSION_SCRIPT_SOURCE.as_bytes()),
                    ),
                ),
                (
                    NormalizedRelativePath::parse("help.md").unwrap(),
                    ScriptResource::new(
                        ResourceKind::Markdown,
                        std::sync::Arc::<[u8]>::from(BROWSER_SESSION_MARKDOWN),
                    ),
                ),
                (
                    NormalizedRelativePath::parse("image.png").unwrap(),
                    ScriptResource::new(
                        ResourceKind::Image,
                        std::sync::Arc::<[u8]>::from(BROWSER_SESSION_IMAGE),
                    ),
                ),
            ]),
            ResourceLimits::default(),
        )
        .expect("browser self-test resources must form a valid bundle"),
    )
}

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
                midi_cc_assignments: Vec::new(),
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
            latency: Default::default(),
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
        tiny_input_port: shoop_egui::PortId,
        callbacks_before: u64,
    },
    WaitForWebMidiConnections {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
        input_port: shoop_egui::PortId,
        output_port: shoop_egui::PortId,
        tiny_input_port: shoop_egui::PortId,
    },
    WaitForWebMidiControl {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
    },
    WaitForWebMidiRecorded {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
        callbacks_after_control: Option<u64>,
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
    WaitForMediaStopped,
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
        request_revision: u64,
    },
    WaitForClickAudioSelection,
    WaitForClickAudioExport,
    PreviewClickAudio,
    WaitForClickPreview,
    GenerateClickMidi,
    WaitForClickMidi {
        previous_task: shoop_egui::TaskId,
        request_revision: u64,
    },
    WaitForClickMidiExport,
    RejectProcessedSession,
    RejectExternalSession,
    WaitForProcessedSessionRejection {
        audio_progress_before: u64,
        external: bool,
    },
    AddLuaVersionChecks,
    WaitForLuaVersionChecks,
    WaitForLuaDialogs,
    WaitForLuaDialogCallback {
        script_id: shoop_egui::ScriptId,
        welcome: shoop_egui::ScriptDialogId,
        guide: shoop_egui::ScriptDialogId,
    },
    WaitForLuaDialogsStopped,
    Complete,
    Failed,
}

#[cfg(any(target_arch = "wasm32", test))]
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

#[cfg(any(target_arch = "wasm32", test))]
fn lua_version_rejection_is_expected(script: &shoop_egui::ScriptState) -> bool {
    let lifecycle = if script.name == "lua-api-unannounced.lua" {
        shoop_egui::ScriptLifecycle::Error
    } else {
        shoop_egui::ScriptLifecycle::Incompatible
    };
    script.lifecycle == lifecycle && script.latest_error.is_some()
}

#[cfg(target_arch = "wasm32")]
impl BrowserSelfTest {
    fn from_args(args: &AppArgs) -> Self {
        if args.web_midi_test {
            set_browser_self_test_status("web-midi");
            set_browser_web_midi_test_status("awaiting-permission");
            Self::WaitForWebMidi
        } else if args.self_test {
            set_browser_self_test_status("awaiting-audio");
            Self::WaitForAudio
        } else {
            Self::Disabled
        }
    }

    fn update(
        &mut self,
        runtime: &mut Runtime,
        snapshot: &AppSnapshot,
        widget: &mut AppWidget,
        args: &AppArgs,
    ) {
        let result = match *self {
            Self::Disabled | Self::Complete | Self::Failed => return,
            Self::WaitForAudio => {
                if !runtime.audio_running() {
                    return;
                }
                if !widget.browser_test_open_builtin_synth_form(&snapshot.track_processors) {
                    return self.fail(
                        "browser dry/wet form did not expose the Built-in Synth processor contract",
                    );
                }
                mark_browser_dry_wet_capability_check();
                Ok(Self::WaitForDryWetForm)
            }
            Self::WaitForDryWetForm => {
                if snapshot.track_processors.len() != 1
                    || snapshot.track_processors[0].id.as_str()
                        != shoop_egui::TrackProcessorTypeId::OXISYNTH
                {
                    return self.fail("browser Built-in Synth catalog changed unexpectedly");
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
                                shoop_egui::TrackProcessorTypeId::OXISYNTH,
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
                            == shoop_egui::TrackProcessorTypeId::OXISYNTH
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
                            action: shoop_egui::TrackAction::InputMonitoringChanged {
                                enabled: true,
                                respect_auto_mute: false,
                            },
                        })
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::Track {
                            track_id: tiny.id,
                            action: shoop_egui::TrackAction::OxiSynth(
                                shoop_egui::OxiSynthControl::SelectPreset("0:40".to_owned()),
                            ),
                        })
                    })
                    .map(|()| Self::WaitForWebMidiTrackReady {
                        track_id: track.id,
                        loop_id: loop_state.id,
                        input_port: input_port.id,
                        output_port: output_port.id,
                        tiny_input_port: tiny_midi_input.id,
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::WaitForWebMidiTrackReady {
                track_id,
                loop_id,
                input_port,
                output_port,
                tiny_input_port,
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
                        tiny_input_port,
                    })
            }
            Self::WaitForWebMidiConnections {
                track_id,
                loop_id,
                input_port,
                output_port,
                tiny_input_port,
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
                let tiny_input_connected =
                    snapshot.connections.confirmed_links.iter().any(|link| {
                        link.application_port_id == tiny_input_port
                            && link.host_port_id.as_str() == "webmidi:source:test-input"
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
                if !tiny_input_connected {
                    let _ = runtime.dispatch(AppIntent::SetPortConnected {
                        port_id: tiny_input_port,
                        host_port_id: shoop_egui::HostPortId::new("webmidi:source:test-input"),
                        connected: true,
                    });
                }
                if !track_input_connected
                    || !track_output_connected
                    || !tiny_input_connected
                    || apc.lifecycle != shoop_egui::ScriptLifecycle::Listening
                    || control_ports.len() != 2
                    || control_links != 2
                {
                    return;
                }
                runtime
                    .dispatch(AppIntent::Track {
                        track_id,
                        action: shoop_egui::TrackAction::InputMonitoringChanged {
                            enabled: true,
                            respect_auto_mute: false,
                        },
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
                    .map(|()| Self::WaitForWebMidiControl { track_id, loop_id })
            }
            Self::WaitForWebMidiControl { track_id, loop_id } => {
                let Some(track) = snapshot.tracks.iter().find(|track| track.id == track_id) else {
                    return;
                };
                let Some(loop_state) = track.loops.iter().find(|loop_| loop_.id == loop_id) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Recording {
                    return;
                }
                set_browser_web_midi_test_status("awaiting-input");
                Ok(Self::WaitForWebMidiRecorded {
                    track_id,
                    loop_id,
                    callbacks_after_control: None,
                })
            }
            Self::WaitForWebMidiRecorded {
                track_id,
                loop_id,
                callbacks_after_control,
            } => {
                if !snapshot.global_controls.solo {
                    return;
                }
                if let Some(callbacks_after_control) = callbacks_after_control {
                    if snapshot.status.callback_count <= callbacks_after_control.saturating_add(100)
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
                } else {
                    Ok(Self::WaitForWebMidiRecorded {
                        track_id,
                        loop_id,
                        callbacks_after_control: Some(snapshot.status.callback_count),
                    })
                }
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
                    .flat_map(|track| {
                        track.ports.iter().flat_map(|port| {
                            port.external_connections
                                .iter()
                                .filter(|endpoint| endpoint.starts_with("webmidi:"))
                                .map(|endpoint| {
                                    format!("{}:{:?}:{endpoint}", track.name, port.role)
                                })
                        })
                    })
                    .collect::<Vec<_>>();
                if !recorded || routes.len() != 3 {
                    return self.fail(&format!(
                        "Web MIDI recording or persisted routes are missing: recorded={recorded}, routes={routes:?}"
                    ));
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
                    return self
                        .fail("loaded Web MIDI session restored duplicate direct-track routes");
                }
                let all_track_ports = snapshot
                    .connections
                    .application_ports
                    .iter()
                    .filter(|port| {
                        matches!(port.owner, shoop_egui::ApplicationPortOwner::Track { .. })
                    })
                    .map(|port| port.id)
                    .collect::<Vec<_>>();
                let all_restored_routes = snapshot
                    .connections
                    .confirmed_links
                    .iter()
                    .filter(|link| {
                        all_track_ports.contains(&link.application_port_id)
                            && link.host_port_id.as_str().starts_with("webmidi:")
                    })
                    .count();
                if all_restored_routes < 3 {
                    return;
                }
                if all_restored_routes > 3 {
                    return self.fail("loaded Web MIDI session restored duplicate track routes");
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
                        name: "Browser Built-in Synth".to_owned(),
                        topology: shoop_egui::TrackSpecTopology::DryWet {
                            dry_audio_channels: 2,
                            wet_audio_channels: 2,
                            dry_midi: true,
                            processor_type: shoop_egui::TrackProcessorTypeId::new(
                                shoop_egui::TrackProcessorTypeId::OXISYNTH,
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
                            == shoop_egui::TrackProcessorTypeId::OXISYNTH
                    })
                }) else {
                    return;
                };
                let tiny_controls = [
                    shoop_egui::TrackAction::InputMonitoringChanged {
                        enabled: true,
                        respect_auto_mute: false,
                    },
                    shoop_egui::TrackAction::OxiSynth(
                        shoop_egui::OxiSynthControl::SelectPreset("0:40".to_owned()),
                    ),
                    shoop_egui::TrackAction::OxiSynth(
                        shoop_egui::OxiSynthControl::SetReverbSend(0.4),
                    ),
                    shoop_egui::TrackAction::OxiSynth(
                        shoop_egui::OxiSynthControl::SetChorusSend(0.6),
                    ),
                    shoop_egui::TrackAction::OxiSynth(shoop_egui::OxiSynthControl::Panic),
                    shoop_egui::TrackAction::FxVisibilityChanged(true),
                    shoop_egui::TrackAction::FxVisibilityChanged(false),
                    shoop_egui::TrackAction::FxVisibilityChanged(true),
                ]
                .into_iter()
                .try_for_each(|action| {
                    runtime.dispatch(AppIntent::Track {
                        track_id: tiny.id,
                        action,
                    })
                });
                let tiny_audio_inputs = snapshot
                    .connections
                    .application_ports
                    .iter()
                    .filter(|port| {
                        matches!(
                            port.owner,
                            shoop_egui::ApplicationPortOwner::Track { track_id, .. }
                                if track_id == tiny.id
                        ) && port.role == shoop_egui::PortRole::AudioInput
                    })
                    .map(|port| port.id)
                    .collect::<Vec<_>>();
                let tiny_audio_links = snapshot
                    .connections
                    .confirmed_links
                    .iter()
                    .filter(|link| tiny_audio_inputs.contains(&link.application_port_id))
                    .map(|link| (link.application_port_id, link.host_port_id.clone()))
                    .collect::<Vec<_>>();
                tiny_controls
                    .and_then(|()| {
                        tiny_audio_links
                            .into_iter()
                            .try_for_each(|(port_id, host_port_id)| {
                                runtime.dispatch(AppIntent::SetPortConnected {
                                    port_id,
                                    host_port_id,
                                    connected: false,
                                })
                            })
                    })
                    .and_then(|()| {
                        if snapshot.status.audio_driver == shoop_egui::AudioDriverState::Dummy {
                            Ok(Self::SaveSession {
                                callbacks_before: snapshot.status.callback_count,
                            })
                        } else {
                            runtime
                                .dispatch(AppIntent::Track {
                                    track_id: track.id,
                                    action: shoop_egui::TrackAction::InputMonitoringChanged {
                                        enabled: true,
                                        respect_auto_mute: false,
                                    },
                                })
                                .and_then(|()| {
                                    runtime.dispatch(AppIntent::Loop {
                                        track_id: track.id,
                                        loop_id: loop_state.id,
                                        action: shoop_egui::LoopAction::IconClicked(
                                            Default::default(),
                                        ),
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
                let Some((track, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Recording
                    || loop_state.empty
                    || args.stress && snapshot.status.callback_count < 1_500
                {
                    return;
                }
                let Some(tiny) = snapshot.tracks.iter().find(|candidate| {
                    candidate.fx.as_ref().is_some_and(|fx| {
                        fx.processor_type.as_str()
                            == shoop_egui::TrackProcessorTypeId::OXISYNTH
                    })
                }) else {
                    return;
                };
                let tiny_audio_inputs = snapshot
                    .connections
                    .application_ports
                    .iter()
                    .filter(|port| {
                        matches!(
                            port.owner,
                            shoop_egui::ApplicationPortOwner::Track { track_id, .. }
                                if track_id == tiny.id
                        ) && port.role == shoop_egui::PortRole::AudioInput
                    })
                    .map(|port| port.id)
                    .collect::<Vec<_>>();
                if let Some(link) = snapshot
                    .connections
                    .confirmed_links
                    .iter()
                    .find(|link| tiny_audio_inputs.contains(&link.application_port_id))
                {
                    if let Err(error) = runtime.dispatch(AppIntent::SetPortConnected {
                        port_id: link.application_port_id,
                        host_port_id: link.host_port_id.clone(),
                        connected: false,
                    }) {
                        return self.fail(&format!(
                            "could not isolate browser Built-in Synth audio input: {error}"
                        ));
                    }
                    return;
                }
                runtime
                    .dispatch(AppIntent::Track {
                        track_id: track.id,
                        action: shoop_egui::TrackAction::OutputMuteChanged(true),
                    })
                    .and_then(|()| {
                        (48..=65).try_for_each(|note| {
                            runtime.dispatch(AppIntent::Piano(shoop_egui::PianoAction::Press(
                                shoop_egui::MidiNote::new(note).unwrap(),
                            )))
                        })
                    })
                    .map(|()| Self::WaitForPianoPress {
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::WaitForPianoPress { callbacks_before } => {
                if snapshot.status.callback_count <= callbacks_before
                    || snapshot.status.output_peak <= 0.000_001
                {
                    return;
                }
                (48..=65)
                    .try_for_each(|note| {
                        runtime.dispatch(AppIntent::Piano(shoop_egui::PianoAction::Release(
                            shoop_egui::MidiNote::new(note).unwrap(),
                        )))
                    })
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
                    .dispatch(AppIntent::Track {
                        track_id: track.id,
                        action: shoop_egui::TrackAction::OutputMuteChanged(false),
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::Loop {
                            track_id: track.id,
                            loop_id: loop_state.id,
                            action: shoop_egui::LoopAction::StopClicked,
                        })
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
                            == shoop_egui::TrackProcessorTypeId::OXISYNTH
                    })
                }) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::Track {
                        track_id: tiny.id,
                        action: shoop_egui::TrackAction::InputMonitoringChanged {
                            enabled: true,
                            respect_auto_mute: false,
                        },
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
                    entrypoint: "main.lua".to_owned(),
                    enabled: true,
                });
                bundle
                    .scripts
                    .insert(9_000_001, browser_session_script_bundle());
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
                let synth_state = snapshot.tracks.iter().find_map(|track| {
                    let fx = track.fx.as_ref()?;
                    if fx.processor_type.as_str() != shoop_egui::TrackProcessorTypeId::OXISYNTH {
                        return None;
                    }
                    match fx.editor.as_ref()? {
                        shoop_egui::TrackProcessorEditorState::OxiSynth(editor) => {
                            Some((fx.visible, editor))
                        }
                    }
                });
                let Some((synth_visible, synth_state)) = synth_state else {
                    return;
                };
                if synth_visible {
                    return self.fail("loaded browser Built-in Synth editor visibility persisted");
                }
                if synth_state.selected_preset_id != "0:40"
                    || synth_state.reverb_send != 0.4
                    || synth_state.chorus_send != 0.6
                {
                    return self.fail(&format!(
                        "loaded browser Built-in Synth state changed: {synth_state:?}"
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
                let resource_base_uri = snapshot
                    .scripting
                    .scripts
                    .iter()
                    .find(|script| script.name == BROWSER_SESSION_SCRIPT_NAME)
                    .and_then(|script| script.resource_base_uri.as_deref());
                let Some(resource_base_uri) = resource_base_uri else {
                    return self.fail("browser session Markdown lost its bundle resource origin");
                };
                let image_uri = format!("{resource_base_uri}image.png");
                if !matches!(
                    shoop_script_resources::read_resource_uri(&image_uri),
                    Ok(Some(bytes)) if bytes.as_ref() == BROWSER_SESSION_IMAGE
                ) {
                    return self.fail("browser session image was unavailable from its bundle");
                }
                if snapshot.status.audio_driver != shoop_egui::AudioDriverState::Dummy
                    && snapshot.status.callback_count <= callbacks_before
                {
                    return self.fail("audio callbacks did not advance through session reload");
                }
                if args.stress {
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
                        && script.enabled
                        && bundle.scripts.get(&script.id).is_some_and(|resources| {
                            resources.entrypoint_resource().bytes.as_ref()
                                == BROWSER_SESSION_SCRIPT_SOURCE.as_bytes()
                                && resources
                                    .resources
                                    .get(
                                        &shoop_script_resources::NormalizedRelativePath::parse(
                                            "help.md",
                                        )
                                        .unwrap(),
                                    )
                                    .is_some_and(|resource| {
                                        resource.bytes.as_ref() == BROWSER_SESSION_MARKDOWN
                                    })
                                && resources
                                    .resources
                                    .get(
                                        &shoop_script_resources::NormalizedRelativePath::parse(
                                            "image.png",
                                        )
                                        .unwrap(),
                                    )
                                    .is_some_and(|resource| {
                                        resource.bytes.as_ref() == BROWSER_SESSION_IMAGE
                                    })
                        })
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
                if args.stress || args.session_only {
                    Ok(Self::RejectProcessedSession)
                } else {
                    let Some((track, loop_state)) = first_main_loop(snapshot) else {
                        return;
                    };
                    runtime
                        .dispatch(AppIntent::Global(
                            shoop_egui::GlobalControlAction::DeselectAll,
                        ))
                        .and_then(|()| {
                            runtime.dispatch(AppIntent::Loop {
                                track_id: track.id,
                                loop_id: loop_state.id,
                                action: shoop_egui::LoopAction::StopClicked,
                            })
                        })
                        .map(|()| Self::WaitForMediaStopped)
                }
            }
            Self::WaitForMediaStopped => {
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Stopped
                    || snapshot
                        .tracks
                        .iter()
                        .flat_map(|track| &track.loops)
                        .any(|loop_| loop_.selected)
                    || snapshot.details.is_some()
                {
                    return;
                }
                Ok(Self::ExportLoopAudio)
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
                if snapshot.io_task.as_ref().is_some_and(|task| {
                    task.kind == shoop_egui::IoTaskKind::ImportLoopAudio
                        && task.status == shoop_egui::IoTaskStatus::Failed
                }) {
                    return self.fail(&format!(
                        "browser loop audio import failed: {:?}",
                        snapshot.io_task
                    ));
                }
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
                    .map(|()| Self::WaitForClickAudio {
                        previous_task,
                        request_revision: snapshot.revision,
                    })
            }
            Self::WaitForClickAudio {
                previous_task,
                request_revision,
            } => {
                if click_request_was_rejected(
                    snapshot.revision,
                    snapshot.io_task.as_ref(),
                    previous_task,
                    request_revision,
                ) {
                    return self.fail("browser audio click request was rejected before task creation");
                }
                let Some(task) = &snapshot.io_task else {
                    return;
                };
                if task.id != previous_task
                    && task.kind == shoop_egui::IoTaskKind::GenerateClickTrack
                    && task.status == shoop_egui::IoTaskStatus::Failed
                {
                    return self.fail(&format!(
                        "browser audio click generation failed: {:?}",
                        snapshot.io_task
                    ));
                }
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
                    .map(|()| Self::WaitForClickMidi {
                        previous_task,
                        request_revision: snapshot.revision,
                    })
            }
            Self::WaitForClickMidi {
                previous_task,
                request_revision,
            } => {
                if click_request_was_rejected(
                    snapshot.revision,
                    snapshot.io_task.as_ref(),
                    previous_task,
                    request_revision,
                ) {
                    return self.fail("browser MIDI click request was rejected before task creation");
                }
                let Some(task) = &snapshot.io_task else {
                    return;
                };
                if task.id != previous_task
                    && task.kind == shoop_egui::IoTaskKind::GenerateClickTrack
                    && task.status == shoop_egui::IoTaskStatus::Failed
                {
                    return self.fail(&format!(
                        "browser MIDI click generation failed: {:?}",
                        snapshot.io_task
                    ));
                }
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
                    Ok(Self::AddLuaVersionChecks)
                } else {
                    Ok(Self::RejectExternalSession)
                }
            }
            Self::AddLuaVersionChecks => runtime
                .dispatch(AppIntent::Global(
                    shoop_egui::GlobalControlAction::SetSolo(false),
                ))
                .and_then(|()| {
                    [
                        (
                            "lua-api-higher-minor.lua",
                            "shoop_announce_api_version(1, 5); require('shoop_control').set_solo(true)",
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
                    ]
                    .into_iter()
                    .try_for_each(|(name, source)| {
                        runtime.dispatch(AppIntent::AddScriptSource {
                            name: name.to_owned(),
                            source: std::sync::Arc::from(source),
                            kind: ScriptKind::User,
                            enabled: true,
                        })
                    })
                })
                .map(|()| Self::WaitForLuaVersionChecks),
            Self::WaitForLuaVersionChecks => {
                let rejected = snapshot
                    .scripting
                    .scripts
                    .iter()
                    .filter(|script| script.name.starts_with("lua-api-"))
                    .collect::<Vec<_>>();
                if rejected.len() != 4
                    || rejected
                        .iter()
                        .any(|script| !lua_version_rejection_is_expected(script))
                    || snapshot.global_controls.solo
                    || !snapshot.scripting.dialogs.is_empty()
                {
                    return;
                }
                runtime
                    .dispatch(AppIntent::AddScriptSource {
                        name: "dialogs.lua".to_owned(),
                        source: std::sync::Arc::from(
                            "shoop_announce_api_version(1, 0); local c=require('shoop_control'); local d=require('shoop_dialog'); d.simple('Lua dialog example', {d.button('Toggle Solo and show guide', function() c.set_solo(true); d.open('Lua dialog guide') end)}); d.paged('Lua dialog guide', {{d.markdown('Page one')}, {d.markdown('Page two')}}); d.open('Lua dialog example')",
                        ),
                        kind: ScriptKind::User,
                        enabled: true,
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::AddScriptSource {
                            name: "dialog-survivor.lua".to_owned(),
                            source: std::sync::Arc::from(
                                "shoop_announce_api_version(1, 0); local d=require('shoop_dialog'); d.simple('Survivor', {d.rich_text('Still active')})",
                            ),
                            kind: ScriptKind::User,
                            enabled: true,
                        })
                    })
                    .map(|()| Self::WaitForLuaDialogs)
            }
            Self::WaitForLuaDialogs => {
                let Some(welcome) = snapshot
                    .scripting
                    .dialogs
                    .iter()
                    .find(|dialog| dialog.name == "Lua dialog example")
                else {
                    return;
                };
                let Some(guide) = snapshot
                    .scripting
                    .dialogs
                    .iter()
                    .find(|dialog| dialog.name == "Lua dialog guide")
                else {
                    return;
                };
                if snapshot.scripting.dialogs.len() != 3
                    || widget.browser_test_lua_dialog_count() != 3
                    || widget.browser_test_lua_dialog_state(welcome.id) != Some((true, 0))
                    || widget.browser_test_lua_dialog_state(guide.id) != Some((false, 0))
                {
                    return;
                }
                let shoop_egui::ScriptDialogKind::Simple(content) = &welcome.kind else {
                    return self.fail("browser Lua simple dialog has the wrong flavor");
                };
                let Some(button_id) = content.elements.iter().find_map(|element| match element {
                    shoop_egui::ScriptDialogElement::Button { id, .. } => *id,
                    _ => None,
                }) else {
                    return self.fail("browser Lua dialog callback button is missing");
                };
                widget.browser_test_close_lua_dialog(welcome.id);
                widget.browser_test_set_lua_dialog_page(guide.id, 1);
                runtime
                    .dispatch(AppIntent::InvokeScriptDialogButton {
                        script_id: welcome.owner_script_id,
                        dialog_id: welcome.id,
                        button_id,
                    })
                    .map(|()| Self::WaitForLuaDialogCallback {
                        script_id: welcome.owner_script_id,
                        welcome: welcome.id,
                        guide: guide.id,
                    })
            }
            Self::WaitForLuaDialogCallback {
                script_id,
                welcome,
                guide,
            } => {
                let guide_opened = snapshot
                    .scripting
                    .dialogs
                    .iter()
                    .find(|dialog| dialog.id == guide)
                    .is_some_and(|dialog| dialog.open_request == 1);
                if !snapshot.global_controls.solo
                    || !guide_opened
                    || widget.browser_test_lua_dialog_state(welcome) != Some((false, 0))
                    || widget.browser_test_lua_dialog_state(guide) != Some((true, 1))
                {
                    return;
                }
                widget.browser_test_close_lua_dialog(guide);
                widget.browser_test_open_lua_dialog_from_list(guide);
                if widget.browser_test_lua_dialog_state(guide) != Some((true, 1)) {
                    return self.fail("browser Lua dialog page state did not survive close/reopen");
                }
                runtime
                    .dispatch(AppIntent::StopScript { script_id })
                    .map(|()| Self::WaitForLuaDialogsStopped)
            }
            Self::WaitForLuaDialogsStopped => {
                if snapshot.scripting.dialogs.len() != 1
                    || snapshot.scripting.dialogs[0].name != "Survivor"
                    || widget.browser_test_lua_dialog_count() != 1
                {
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
        let _ = element.set_attribute("data-dry-wet-form", "built-in-synth");
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
                show_about_dialog(ui.ctx(), &mut open, identity);
            });
            output.textures_delta.clear();
            assert!(open);
        }

        let mut open = false;
        let mut output = context.run_ui(Default::default(), |ui| {
            show_about_dialog(ui.ctx(), &mut open, BuildIdentity::CURRENT);
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
    fn loop_smoothing_settings_retry_failed_dispatch_and_apply_zero() {
        let mut builder = SettingsRegistryBuilder::default();
        register_audio_settings(&mut builder).unwrap();
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
                shoop_app::DispatchError::Full
            ),)
            .is_err()
        );
        assert_eq!(applied_revision, 0);

        let mut received = None;
        reconcile_loop_smoothing_settings(&settings, &mut applied_revision, |intent| {
            received = Some(intent);
            Ok(())
        })
        .unwrap();
        assert_eq!(received, Some(AppIntent::SetLoopSmoothingMs(0)));
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
        assert!(html.contains("Browser audio and MIDI permissions"));
        assert!(html.contains("Enable microphone audio"));
        assert!(html.contains("Enable output-only audio"));
        assert!(html.contains("audio_worklet.js"));
        assert!(html.contains("Roboto-Regular.ttf"));
        assert!(html.contains("Roboto-BoldItalic.ttf"));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn small_screens_use_a_larger_missing_setting_default() {
        assert_eq!(
            default_ui_scale_for_screen(Some(egui::vec2(1280.0, 800.0))),
            1.25
        );
        assert_eq!(
            default_ui_scale_for_screen(Some(egui::vec2(1280.0, 801.0))),
            1.0
        );
        assert_eq!(default_ui_scale_for_screen(None), 1.0);

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
    fn confirmed_driver_switch_is_saved_once_and_completed_after_persistence() {
        let mut app = UnifiedApp::new(1.0, false, &AppArgs::default()).unwrap();
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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

        let mut app = UnifiedApp::new(1.0, false, &AppArgs::default()).unwrap();
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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
    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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
    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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
                &shoop_settings::SettingsDocument::empty("test"),
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
        register_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut manager = SettingsManager::load_from_path(registry, "test", settings_path.clone());
        let mut runtime = Runtime::new(&manager.active()).unwrap();

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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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
                "shoop_announce_api_version(1, 5); require('shoop_control').set_solo(true)",
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
    fn native_dummy_workflow_creates_records_and_controls_tracks_and_loops() {
        let mut app = UnifiedApp::new(1.0, false, &AppArgs::default()).unwrap();
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
                    action: TrackAction::InputMonitoringChanged {
                        enabled: true,
                        respect_auto_mute: false,
                    },
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

    #[cfg(not(target_arch = "wasm32"))]
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

    #[cfg(not(target_arch = "wasm32"))]
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
