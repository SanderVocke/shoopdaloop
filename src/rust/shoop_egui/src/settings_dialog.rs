use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use egui_commonmark::CommonMarkCache;
use egui_material_icons::{
    icons::{
        ICON_ARCHIVE, ICON_DELETE, ICON_DESCRIPTION, ICON_DOWNLOAD, ICON_INFO, ICON_MOVE,
        ICON_PLAY_ARROW, ICON_QUESTION_MARK, ICON_REFRESH, ICON_RESTART_ALT, ICON_STOP,
    },
    MaterialIcon,
};
use shoop_settings::{
    SettingEditor, SettingValue, SettingsDraft, SettingsPersistenceState, SettingsRegistry,
    SettingsViewState, StringToggle, StringToggleList,
};

use crate::{
    audio_driver_config_from_draft, colors, AppAction, AudioDriverKind, AudioDriverRuntimeState,
    ScriptId, ScriptKind, ScriptLifecycle, ScriptLogLevel, ScriptState, ScriptingState,
    BUILTINS_LOCATION, BUILTIN_SCRIPTS, TOUCH_MODE, UI_SCALE_FACTOR, USER_SCRIPTS,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TracingStatus {
    pub available: bool,
    pub active: bool,
    pub buffer_capacity_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TracingStopped {
    Saved(String),
    Discarded,
}

#[derive(Clone, Debug)]
pub enum SettingsAction {
    Save(SettingsDraft),
    RequestAudioDriverSwitch {
        config: crate::AudioDriverConfig,
        draft: SettingsDraft,
    },
    RetryAudioDriverPersistence {
        request_id: u64,
    },
    RequestBrowserPermissions,
    RecoverWithDefaults,
    RequestAddUserScript,
    RescanBuiltinScripts,
    RequestEphemeralScriptPicker,
    RequestReloadUserScript {
        script_id: ScriptId,
    },
    StartTracing {
        engine_detail: bool,
    },
    StopTracing {
        save: bool,
    },
}

impl SettingsAction {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Save(_) => "settings.save",
            Self::RequestAudioDriverSwitch { .. } => "settings.request_audio_driver_switch",
            Self::RetryAudioDriverPersistence { .. } => "settings.retry_audio_persistence",
            Self::RequestBrowserPermissions => "settings.request_browser_permissions",
            Self::RecoverWithDefaults => "settings.recover_defaults",
            Self::RequestAddUserScript => "settings.add_user_script",
            Self::RescanBuiltinScripts => "settings.rescan_builtin_scripts",
            Self::RequestEphemeralScriptPicker => "settings.pick_ephemeral_script",
            Self::RequestReloadUserScript { .. } => "settings.reload_user_script",
            Self::StartTracing { .. } => "developer.tracing.start",
            Self::StopTracing { save: true } => "developer.tracing.save",
            Self::StopTracing { save: false } => "developer.tracing.discard",
        }
    }
}

#[derive(Default)]
pub struct SettingsDialogResponse {
    pub settings_actions: Vec<SettingsAction>,
    pub app_actions: Vec<AppAction>,
}

pub struct SettingsDialog {
    registry: Arc<SettingsRegistry>,
    open: bool,
    draft: Option<SettingsDraft>,
    active_category: Option<String>,
    audio_target: Option<AudioDriverKind>,
    audio_discovery_key: Option<(AudioDriverKind, String)>,
    script_log_windows: BTreeSet<ScriptId>,
    script_documentation_windows: BTreeSet<ScriptId>,
    script_status_windows: BTreeSet<ScriptId>,
    markdown_cache: CommonMarkCache,
    tracing_status: TracingStatus,
    tracing_engine_detail: bool,
    #[cfg(test)]
    tracing_start_rect: Option<egui::Rect>,
    #[cfg(test)]
    setting_card_rects: Vec<egui::Rect>,
    #[cfg(test)]
    script_group_rects: BTreeMap<&'static str, egui::Rect>,
    #[cfg(test)]
    ephemeral_picker_rect: Option<egui::Rect>,
    #[cfg(test)]
    restart_rects: BTreeMap<ScriptId, egui::Rect>,
    #[cfg(test)]
    stop_rects: BTreeMap<ScriptId, egui::Rect>,
    #[cfg(test)]
    log_rects: BTreeMap<ScriptId, egui::Rect>,
    #[cfg(test)]
    documentation_rects: BTreeMap<ScriptId, egui::Rect>,
    #[cfg(test)]
    status_rects: BTreeMap<ScriptId, egui::Rect>,
    #[cfg(test)]
    reload_rects: BTreeMap<ScriptId, egui::Rect>,
    #[cfg(test)]
    export_rects: BTreeMap<ScriptId, egui::Rect>,
    #[cfg(test)]
    ownership_rects: BTreeMap<ScriptId, egui::Rect>,
    #[cfg(test)]
    remove_rects: BTreeMap<ScriptId, egui::Rect>,
}

impl SettingsDialog {
    pub fn new(registry: Arc<SettingsRegistry>) -> Self {
        Self {
            registry,
            open: false,
            draft: None,
            active_category: None,
            audio_target: None,
            audio_discovery_key: None,
            script_log_windows: BTreeSet::new(),
            script_documentation_windows: BTreeSet::new(),
            script_status_windows: BTreeSet::new(),
            markdown_cache: CommonMarkCache::default(),
            tracing_status: TracingStatus::default(),
            tracing_engine_detail: false,
            #[cfg(test)]
            tracing_start_rect: None,
            #[cfg(test)]
            setting_card_rects: Vec::new(),
            #[cfg(test)]
            script_group_rects: BTreeMap::new(),
            #[cfg(test)]
            ephemeral_picker_rect: None,
            #[cfg(test)]
            restart_rects: BTreeMap::new(),
            #[cfg(test)]
            stop_rects: BTreeMap::new(),
            #[cfg(test)]
            log_rects: BTreeMap::new(),
            #[cfg(test)]
            documentation_rects: BTreeMap::new(),
            #[cfg(test)]
            status_rects: BTreeMap::new(),
            #[cfg(test)]
            reload_rects: BTreeMap::new(),
            #[cfg(test)]
            export_rects: BTreeMap::new(),
            #[cfg(test)]
            ownership_rects: BTreeMap::new(),
            #[cfg(test)]
            remove_rects: BTreeMap::new(),
        }
    }

    pub fn open(&mut self, state: &SettingsViewState) {
        self.open = true;
        self.draft = Some(SettingsDraft::from_snapshot(&state.active));
        self.ensure_active_category();
    }

    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_tracing_status(&mut self, status: TracingStatus) {
        self.tracing_status = status;
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_test_open_category(
        &mut self,
        state: &SettingsViewState,
        category: &str,
    ) -> bool {
        let available = self
            .registry
            .definitions()
            .iter()
            .any(|definition| definition.category() == category);
        if available {
            self.open(state);
            self.active_category = Some(category.to_owned());
        }
        available
    }

    pub fn add_user_script_path(&mut self, path: String) -> Result<(), &'static str> {
        let draft = self.draft.as_mut().ok_or("settings are not open")?;
        let mut scripts = draft
            .get(USER_SCRIPTS)
            .map_err(|_| "user script settings are unavailable")?;
        if let Some(existing) = scripts.0.iter_mut().find(|entry| entry.value == path) {
            existing.enabled = true;
        } else {
            scripts.0.push(StringToggle {
                value: path,
                enabled: true,
            });
        }
        draft.set(USER_SCRIPTS, scripts);
        self.active_category = Some("Scripts".to_owned());
        Ok(())
    }

    fn remove_user_script_path(&mut self, path: &str) {
        let Some(draft) = self.draft.as_mut() else {
            return;
        };
        let Ok(mut scripts) = draft.get(USER_SCRIPTS) else {
            return;
        };
        scripts.0.retain(|entry| entry.value != path);
        draft.set(USER_SCRIPTS, scripts);
    }

    pub fn show(
        &mut self,
        context: &egui::Context,
        state: &SettingsViewState,
        scripting: &ScriptingState,
        audio_drivers: &AudioDriverRuntimeState,
        script_paths: Option<&BTreeMap<ScriptId, String>>,
    ) -> SettingsDialogResponse {
        if !self.open {
            return SettingsDialogResponse::default();
        }
        if self.draft.is_none() {
            self.draft = Some(SettingsDraft::from_snapshot(&state.active));
        }
        self.ensure_active_category();
        let mut response = SettingsDialogResponse::default();
        let mut open = self.open;
        let mut close_without_save = false;
        egui::Window::new("Settings")
            .id(egui::Id::new("settings_dialog"))
            .open(&mut open)
            .resizable(true)
            .default_size([660.0, 560.0])
            .min_size([320.0, 180.0])
            .show(context, |ui| {
                self.show_status(ui, state);
                ui.separator();
                if state.recovery_required {
                    ui.label(
                        "The stored document must be explicitly replaced before settings can be edited.",
                    );
                    if ui
                        .add_enabled(
                            state.persistence != SettingsPersistenceState::Saving,
                            egui::Button::new("Replace stored settings with defaults"),
                        )
                        .clicked()
                    {
                        response
                            .settings_actions
                            .push(SettingsAction::RecoverWithDefaults);
                    }
                    return;
                }

                let active_category = self.active_category.clone().unwrap_or_default();
                let footer_height = 32.0;
                let body_size = egui::vec2(
                    ui.available_width(),
                    (ui.available_height() - footer_height).max(80.0),
                );
                ui.allocate_ui_with_layout(
                    body_size,
                    egui::Layout::left_to_right(egui::Align::Min),
                    |ui| {
                        let tabs_size = egui::vec2(110.0, ui.available_height());
                        ui.allocate_ui_with_layout(
                            tabs_size,
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| self.show_category_tabs(ui),
                        );
                        ui.separator();
                        let content_size = ui.available_size();
                        ui.allocate_ui_with_layout(
                            content_size,
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                egui::ScrollArea::vertical()
                                    .id_salt("settings_values")
                                    .auto_shrink([false, false])
                                    .scroll_source(crate::control_safe_scroll_source())
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.vertical(|ui| {
                                            if active_category == "Audio" {
                                                self.show_audio(ui, audio_drivers, &mut response);
                                            } else if active_category == "Appearance" {
                                                self.show_appearance(ui);
                                            } else if active_category == "Scripts" {
                                                self.show_script_runtime(
                                                    ui,
                                                    scripting,
                                                    script_paths,
                                                    &mut response,
                                                );
                                            } else if active_category == "Developer" {
                                                self.show_developer(ui, &mut response);
                                            } else {
                                                self.show_definitions(ui, &active_category);
                                            }
                                        });
                                    });
                            },
                        );
                    },
                );
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Reset all").clicked() {
                        if let Some(draft) = &mut self.draft {
                            draft.reset_all(&self.registry);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Cancel").clicked() {
                            close_without_save = true;
                        }
                        let stale = self
                            .draft
                            .as_ref()
                            .is_some_and(|draft| draft.base_revision() != state.active.revision());
                        let save_label = if active_category == "Appearance" {
                            "Apply and save"
                        } else {
                            "Save"
                        };
                        let save = ui.add_enabled(
                            state.persistence != SettingsPersistenceState::Saving && !stale,
                            egui::Button::new(save_label),
                        );
                        if save.clicked() {
                            if let Some(action) = self.save_action() {
                                self.apply_appearance(context);
                                response.settings_actions.push(action);
                            }
                        }
                        if stale {
                            ui.colored_label(
                                colors::WARNING,
                                "Settings changed elsewhere; close and reopen this dialog.",
                            );
                        }
                    });
                });
            });
        if close_without_save || !open {
            self.open = false;
            self.draft = None;
            self.audio_target = None;
            self.audio_discovery_key = None;
            self.script_log_windows.clear();
            self.script_documentation_windows.clear();
            self.script_status_windows.clear();
        } else {
            self.open = open;
        }
        self.show_audio_confirmation(context, audio_drivers, &mut response);
        self.show_script_windows(context, scripting, script_paths);
        response
    }

    fn categories(&self) -> Vec<String> {
        let mut categories = Vec::new();
        for definition in self.registry.definitions() {
            if categories.last().map(String::as_str) != Some(definition.category()) {
                categories.push(definition.category().to_owned());
            }
        }
        #[cfg(target_arch = "wasm32")]
        if !categories.iter().any(|category| category == "Audio") {
            categories.insert(0, "Audio".to_owned());
        }
        if !categories.iter().any(|category| category == "Developer") {
            categories.push("Developer".to_owned());
        }
        categories
    }

    fn ensure_active_category(&mut self) {
        let categories = self.categories();
        if !self
            .active_category
            .as_ref()
            .is_some_and(|active| categories.contains(active))
        {
            self.active_category = categories.into_iter().next();
        }
    }

    fn show_category_tabs(&mut self, ui: &mut egui::Ui) {
        ui.set_width(110.0);
        egui::ScrollArea::vertical()
            .id_salt("settings_category_tabs")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_width(100.0);
                    for category in self.categories() {
                        let selected = self.active_category.as_deref() == Some(category.as_str());
                        if ui
                            .add_sized(
                                [ui.available_width(), 24.0],
                                egui::Button::selectable(selected, &category),
                            )
                            .clicked()
                        {
                            self.active_category = Some(category);
                        }
                    }
                });
            });
    }

    fn save_action(&self) -> Option<SettingsAction> {
        self.draft.clone().map(SettingsAction::Save)
    }

    fn show_status(&self, ui: &mut egui::Ui, state: &SettingsViewState) {
        ui.label(format!("Storage: {}", state.storage_location));
        match state.persistence {
            SettingsPersistenceState::Idle => {}
            SettingsPersistenceState::Saving => {
                ui.spinner();
                ui.label("Saving settings…");
            }
            SettingsPersistenceState::Saved => {
                ui.colored_label(colors::SUCCESS, "Settings saved");
            }
            SettingsPersistenceState::Failed => {
                ui.colored_label(colors::ERROR, "Settings were not saved");
            }
        }
        for diagnostic in state.diagnostics.iter() {
            ui.colored_label(colors::WARNING, &diagnostic.message);
        }
    }

    fn show_developer(&mut self, ui: &mut egui::Ui, response: &mut SettingsDialogResponse) {
        ui.label(
            "Capture performance data or investigate UI/audio bugs. Tracing can be stopped again, but capturing may degrade performance.",
        );
        ui.add_space(8.0);
        ui.checkbox(
            &mut self.tracing_engine_detail,
            "include detailed engine events",
        );
        let start = ui.add_enabled(
            self.tracing_status.available && !self.tracing_status.active,
            egui::Button::new("Start tracing"),
        );
        #[cfg(test)]
        {
            self.tracing_start_rect = Some(start.rect);
        }
        if start.clicked() {
            response
                .settings_actions
                .push(SettingsAction::StartTracing {
                    engine_detail: self.tracing_engine_detail,
                });
        }
        if self.tracing_status.active {
            ui.colored_label(colors::WARNING, "Tracing is already active");
        } else if !self.tracing_status.available {
            ui.colored_label(
                colors::MUTED_FOREGROUND,
                "Tracing is unavailable in this build.",
            );
        }
    }

    fn show_appearance(&mut self, ui: &mut egui::Ui) {
        let Some(definition) = self.registry.definition(UI_SCALE_FACTOR.id()).cloned() else {
            ui.colored_label(colors::ERROR, "UI scale setting is unavailable");
            return;
        };
        let card = egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(8));
        let margin = card.total_margin();
        let card_width = (ui.available_width() - margin.left - margin.right).max(0.0);
        let _card = card.show(ui, |ui| {
            ui.set_width(card_width);
            ui.horizontal(|ui| {
                ui.strong(definition.label());
                if ui.small_button("Reset").clicked() {
                    if let Some(draft) = &mut self.draft {
                        draft.reset(&definition);
                    }
                }
            });
            ui.label(definition.help());
            ui.weak("Applied only when settings are explicitly saved.");
            let (min, max) = match definition.editor() {
                SettingEditor::Number { min, max } => (*min, *max),
                _ => {
                    ui.colored_label(colors::ERROR, "UI scale editor is invalid");
                    return;
                }
            };
            let Some(draft) = &mut self.draft else {
                return;
            };
            let Ok(mut scale) = draft.get(UI_SCALE_FACTOR) else {
                ui.colored_label(colors::ERROR, "Missing UI scale draft value");
                return;
            };
            if ui
                .add(
                    egui::Slider::new(&mut scale, min..=max)
                        .fixed_decimals(2)
                        .step_by(0.05)
                        .show_value(true),
                )
                .changed()
            {
                draft.set(UI_SCALE_FACTOR, scale);
            }
        });
        #[cfg(test)]
        {
            self.setting_card_rects.clear();
            self.setting_card_rects.push(_card.response.rect);
        }
        let Some(definition) = self.registry.definition(TOUCH_MODE.id()).cloned() else {
            ui.colored_label(colors::ERROR, "Touch mode setting is unavailable");
            return;
        };
        let card = egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(8));
        let margin = card.total_margin();
        let card_width = (ui.available_width() - margin.left - margin.right).max(0.0);
        let _card = card.show(ui, |ui| {
            ui.set_width(card_width);
            ui.horizontal(|ui| {
                ui.strong(definition.label());
                if ui.small_button("Reset").clicked() {
                    if let Some(draft) = &mut self.draft {
                        draft.reset(&definition);
                    }
                }
            });
            ui.label(definition.help());
            ui.weak(definition.effect().label());
            let Some(draft) = &mut self.draft else {
                return;
            };
            let Ok(mut enabled) = draft.get(TOUCH_MODE) else {
                ui.colored_label(colors::ERROR, "Missing touch mode draft value");
                return;
            };
            if ui.checkbox(&mut enabled, definition.label()).changed() {
                draft.set(TOUCH_MODE, enabled);
            }
        });
        #[cfg(test)]
        self.setting_card_rects.push(_card.response.rect);
    }

    fn apply_appearance(&self, context: &egui::Context) {
        let Some(draft) = &self.draft else {
            return;
        };
        if let Ok(scale) = draft.get(UI_SCALE_FACTOR) {
            context.set_zoom_factor(scale as f32);
        }
    }

    fn show_definitions(&mut self, ui: &mut egui::Ui, category: &str) {
        let definitions = self
            .registry
            .definitions()
            .iter()
            .filter(|definition| definition.category() == category)
            .cloned()
            .collect::<Vec<_>>();
        self.show_definition_cards(ui, definitions, None);
    }

    fn show_audio(
        &mut self,
        ui: &mut egui::Ui,
        audio: &AudioDriverRuntimeState,
        response: &mut SettingsDialogResponse,
    ) {
        #[cfg(test)]
        self.setting_card_rects.clear();
        ui.heading("Loop audio");
        let definitions = self
            .registry
            .definitions()
            .iter()
            .filter(|definition| definition.key() == crate::LOOP_EDGE_SMOOTHING_MS.id())
            .cloned()
            .collect::<Vec<_>>();
        self.show_definition_cards(ui, definitions, None);

        if !audio.supported || cfg!(target_arch = "wasm32") {
            #[cfg(target_arch = "wasm32")]
            {
                ui.heading("Browser audio and MIDI");
                ui.label(
                    "Browser permissions control physical audio and Web MIDI access for this app run.",
                );
                if ui
                    .button("Manage browser audio and MIDI permissions…")
                    .clicked()
                {
                    response
                        .settings_actions
                        .push(SettingsAction::RequestBrowserPermissions);
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            ui.colored_label(
                colors::WARNING,
                "Native runtime driver switching is unavailable in this build.",
            );
            return;
        }
        let active_kind = audio.active.as_ref().map(|active| active.configured.kind());
        let mut selected = self
            .audio_target
            .unwrap_or_else(|| active_kind.unwrap_or(AudioDriverKind::Dummy));
        ui.heading("Audio driver");
        ui.horizontal_wrapped(|ui| {
            for driver in audio.catalog.iter() {
                let button = egui::Button::selectable(selected == driver.kind, driver.kind.label());
                if ui.add_enabled(driver.available, button).clicked() {
                    selected = driver.kind;
                }
            }
        });
        self.audio_target = Some(selected);
        let descriptor = audio.catalog.iter().find(|driver| driver.kind == selected);
        if let Some(reason) = descriptor.and_then(|driver| driver.unavailable_reason.as_ref()) {
            ui.colored_label(colors::ERROR, reason);
        }
        if let Some(active) = &audio.active {
            ui.label(format!(
                "Active: {} · {} Hz · {} frames · {}",
                active.configured.kind().label(),
                active.sample_rate,
                active.buffer_size,
                active.instance_name
            ));
        }
        let prefix = match selected {
            AudioDriverKind::Dummy => "audio.dummy.",
            AudioDriverKind::Jack => "audio.jack.",
            AudioDriverKind::Cpal => "audio.cpal.",
            AudioDriverKind::WebAudio => "audio.webaudio.",
        };
        let definitions = self
            .registry
            .definitions()
            .iter()
            .filter(|definition| definition.key().starts_with(prefix))
            .cloned()
            .collect::<Vec<_>>();
        self.show_definition_cards(ui, definitions, descriptor);

        let config = self
            .draft
            .as_ref()
            .ok_or_else(|| "settings draft is unavailable".to_owned())
            .and_then(|draft| audio_driver_config_from_draft(draft, selected));
        if let Ok(configured) = &config {
            let discovery_key = match configured {
                crate::AudioDriverConfig::Cpal(config) => {
                    (AudioDriverKind::Cpal, config.host.clone())
                }
                _ => (configured.kind(), String::new()),
            };
            if self.audio_discovery_key.as_ref() != Some(&discovery_key) {
                self.audio_discovery_key = Some(discovery_key);
                response
                    .app_actions
                    .push(AppAction::RefreshAudioDriverDiscovery {
                        config: configured.clone(),
                    });
            }
        }
        let differs = config.as_ref().is_ok_and(|config| {
            audio
                .active
                .as_ref()
                .is_none_or(|active| active.configured != *config)
        });
        if let Err(error) = &config {
            ui.colored_label(colors::ERROR, error);
        }
        let can_switch = descriptor.is_some_and(|driver| driver.available)
            && differs
            && !matches!(
                audio.switch.status,
                crate::AudioDriverSwitchStatus::AwaitingConfirmation
                    | crate::AudioDriverSwitchStatus::Switching
                    | crate::AudioDriverSwitchStatus::Resampling
                    | crate::AudioDriverSwitchStatus::Restoring
                    | crate::AudioDriverSwitchStatus::Persisting
            );
        if ui
            .add_enabled(can_switch, egui::Button::new("Switch"))
            .clicked()
        {
            if let (Ok(config), Some(draft)) = (config, self.draft.clone()) {
                response
                    .settings_actions
                    .push(SettingsAction::RequestAudioDriverSwitch { config, draft });
            }
        }
        if audio.switch.status != crate::AudioDriverSwitchStatus::Idle {
            let color = if matches!(
                audio.switch.status,
                crate::AudioDriverSwitchStatus::Failed | crate::AudioDriverSwitchStatus::Fatal
            ) {
                colors::ERROR
            } else {
                colors::WARNING
            };
            ui.colored_label(color, &audio.switch.message);
        }
        if audio.switch.persistence_retry_available
            && ui.button("Retry saving active driver settings").clicked()
        {
            response
                .settings_actions
                .push(SettingsAction::RetryAudioDriverPersistence {
                    request_id: audio.switch.request_id,
                });
        }
    }

    fn show_audio_confirmation(
        &self,
        context: &egui::Context,
        audio: &AudioDriverRuntimeState,
        response: &mut SettingsDialogResponse,
    ) {
        if audio.switch.status != crate::AudioDriverSwitchStatus::AwaitingConfirmation {
            return;
        }
        egui::Window::new("Confirm audio driver switch")
            .id(egui::Id::new(("audio_driver_confirmation", audio.switch.request_id)))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.colored_label(colors::WARNING, &audio.switch.message);
                if let (Some(source), Some(target)) = (&audio.switch.source, &audio.switch.target) {
                    if source.sample_rate != target.sample_rate {
                        ui.colored_label(
                            colors::ERROR,
                            format!(
                                "Sample rate differs: {} Hz → {} Hz. All loop contents will be resampled.",
                                source.sample_rate, target.sample_rate
                            ),
                        );
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("Confirm switch").clicked() {
                        response.app_actions.push(AppAction::ConfirmAudioDriverSwitch {
                            request_id: audio.switch.request_id,
                            accept: true,
                        });
                    }
                    if ui.button("Cancel").clicked() {
                        response.app_actions.push(AppAction::ConfirmAudioDriverSwitch {
                            request_id: audio.switch.request_id,
                            accept: false,
                        });
                    }
                });
            });
    }

    fn show_definition_cards(
        &mut self,
        ui: &mut egui::Ui,
        definitions: Vec<shoop_settings::ErasedSettingDefinition>,
        audio: Option<&crate::AudioDriverDescriptor>,
    ) {
        for definition in definitions {
            let card = egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(8));
            let margin = card.total_margin();
            let card_width = (ui.available_width() - margin.left - margin.right).max(0.0);
            let _card = card.show(ui, |ui| {
                ui.set_width(card_width);
                ui.horizontal(|ui| {
                    ui.strong(definition.label());
                    if ui.small_button("Reset").clicked() {
                        if let Some(draft) = &mut self.draft {
                            draft.reset(&definition);
                        }
                    }
                });
                ui.label(definition.help());
                ui.weak(definition.effect().label());
                if let Some(draft) = &mut self.draft {
                    let Some(value) = draft.value(definition.key()).cloned() else {
                        ui.colored_label(colors::ERROR, "Missing draft value");
                        return;
                    };
                    let mut changed = value.clone();
                    match (definition.editor(), &mut changed) {
                        (SettingEditor::Checkbox, SettingValue::Bool(value)) => {
                            ui.checkbox(value, definition.label());
                        }
                        (SettingEditor::UnsignedInteger { min, max }, SettingValue::U32(value)) => {
                            ui.add(egui::DragValue::new(value).range(*min..=*max).speed(1));
                        }
                        (SettingEditor::SignedInteger { min, max }, SettingValue::I32(value)) => {
                            ui.add(egui::DragValue::new(value).range(*min..=*max).speed(1));
                        }
                        (SettingEditor::Number { min, max }, SettingValue::F64(value)) => {
                            ui.add(egui::DragValue::new(value).range(*min..=*max));
                        }
                        (SettingEditor::StringChoice { choices }, SettingValue::String(value)) => {
                            let selected = choices
                                .iter()
                                .find(|(choice, _label)| *choice == value.as_str())
                                .map_or(value.as_str(), |(_choice, label)| *label);
                            egui::ComboBox::from_id_salt(("setting_choice", definition.key()))
                                .selected_text(selected)
                                .show_ui(ui, |ui| {
                                    for (choice, label) in *choices {
                                        ui.selectable_value(value, (*choice).to_owned(), *label);
                                    }
                                });
                        }
                        (SettingEditor::Text, SettingValue::String(value)) => {
                            let choices = audio.and_then(|audio| match definition.key() {
                                key if key == crate::CPAL_HOST.id() => Some(&audio.hosts),
                                key if key == crate::CPAL_INPUT_DEVICE.id() => {
                                    Some(&audio.input_devices)
                                }
                                key if key == crate::CPAL_OUTPUT_DEVICE.id() => {
                                    Some(&audio.output_devices)
                                }
                                _ => None,
                            });
                            if let Some(choices) = choices.filter(|choices| !choices.is_empty()) {
                                egui::ComboBox::from_id_salt(("audio_choice", definition.key()))
                                    .selected_text(value.as_str())
                                    .show_ui(ui, |ui| {
                                        if !choices.contains(value) {
                                            ui.selectable_value(
                                                value,
                                                value.clone(),
                                                format!("{} (unavailable)", value),
                                            );
                                        }
                                        for choice in choices {
                                            ui.selectable_value(value, choice.clone(), choice);
                                        }
                                    });
                            } else {
                                ui.text_edit_singleline(value);
                            }
                            if let Some(audio) = audio {
                                if definition.key() == crate::CPAL_MIDI_INPUTS.id() {
                                    ui.weak(format!(
                                        "Discovered: {}",
                                        audio.midi_inputs.join(", ")
                                    ));
                                } else if definition.key() == crate::CPAL_MIDI_OUTPUTS.id() {
                                    ui.weak(format!(
                                        "Discovered: {}",
                                        audio.midi_outputs.join(", ")
                                    ));
                                }
                            }
                        }
                        (
                            SettingEditor::StringToggleList,
                            SettingValue::StringToggleList(value),
                        ) => Self::show_string_toggle_list(ui, value),
                        _ => {
                            ui.colored_label(
                                colors::ERROR,
                                "Definition and value types do not match",
                            );
                        }
                    }
                    if changed != value {
                        draft.set_value(definition.key(), changed);
                    }
                }
            });
            #[cfg(test)]
            self.setting_card_rects.push(_card.response.rect);
        }
    }

    fn show_string_toggle_list(ui: &mut egui::Ui, value: &mut StringToggleList) {
        let mut remove = None;
        for (index, entry) in value.0.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.checkbox(&mut entry.enabled, "startup");
                ui.text_edit_singleline(&mut entry.value);
                if ui.small_button("Remove").clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            value.0.remove(index);
        }
        if ui.small_button("Add entry").clicked() {
            value.0.push(StringToggle {
                value: String::new(),
                enabled: true,
            });
        }
    }

    fn show_script_runtime(
        &mut self,
        ui: &mut egui::Ui,
        scripting: &ScriptingState,
        script_paths: Option<&BTreeMap<ScriptId, String>>,
        response: &mut SettingsDialogResponse,
    ) {
        ui.heading("Scripts");
        if !scripting.supported {
            ui.colored_label(
                colors::WARNING,
                "Lua scripting and MIDI control are unavailable in this build.",
            );
            return;
        }
        ui.horizontal(|ui| {
            let ephemeral_picker = ui.button("Load run-once Lua file…");
            #[cfg(test)]
            {
                self.ephemeral_picker_rect = Some(ephemeral_picker.rect);
            }
            if ephemeral_picker.clicked() {
                response
                    .settings_actions
                    .push(SettingsAction::RequestEphemeralScriptPicker);
            }
            if self.registry.definition(USER_SCRIPTS.id()).is_some()
                && ui.button("Add startup Lua file…").clicked()
            {
                response
                    .settings_actions
                    .push(SettingsAction::RequestAddUserScript);
            }
            if self.registry.definition(BUILTINS_LOCATION.id()).is_some()
                && ui.button("Rescan built-in scripts").clicked()
            {
                response
                    .settings_actions
                    .push(SettingsAction::RescanBuiltinScripts);
            }
        });

        for kind in [
            ScriptKind::Bundled,
            ScriptKind::Example,
            ScriptKind::User,
            ScriptKind::Session,
            ScriptKind::Ephemeral,
        ] {
            let scripts = scripting
                .scripts
                .iter()
                .filter(|script| script.kind == kind)
                .collect::<Vec<_>>();
            if scripts.is_empty() {
                continue;
            }
            ui.add_space(6.0);
            let _group = egui::CollapsingHeader::new(script_kind_heading(kind))
                .id_salt(("script_group", script_kind_heading(kind)))
                .default_open(kind != ScriptKind::Example)
                .show(ui, |ui| {
                    egui::Grid::new(("script_table", script_kind_heading(kind)))
                        .num_columns(4)
                        .striped(true)
                        .spacing([12.0, 5.0])
                        .show(ui, |ui| {
                            ui.strong("Name");
                            ui.strong("Status");
                            ui.strong("Enabled");
                            ui.strong("Controls");
                            ui.end_row();

                            for script in scripts {
                                let name = ui.add_sized(
                                    [150.0, 24.0],
                                    egui::Label::new(&script.name).truncate(),
                                );
                                if let Some(path) =
                                    script_paths.and_then(|paths| paths.get(&script.id))
                                {
                                    name.on_hover_text(path);
                                }
                                show_script_lifecycle(ui, script);
                                self.show_script_enabled(ui, script, script_paths, response);
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 3.0;
                                    let active = script_is_active(script.lifecycle);
                                    let (start_icon, start_tooltip) = if active {
                                        (ICON_RESTART_ALT, "Restart script")
                                    } else {
                                        (ICON_PLAY_ARROW, "Start script")
                                    };
                                    let compatible =
                                        script.lifecycle != ScriptLifecycle::Incompatible;
                                    let restart = script_icon_button(
                                        ui,
                                        start_icon,
                                        start_tooltip,
                                        compatible,
                                    );
                                    #[cfg(test)]
                                    self.restart_rects.insert(script.id, restart.rect);
                                    if restart.clicked() {
                                        response.app_actions.push(AppAction::RestartScript {
                                            script_id: script.id,
                                        });
                                    }

                                    let stop = ui
                                        .add_enabled(
                                            active,
                                            egui::Button::new(
                                                ICON_STOP
                                                    .rich_text()
                                                    .size(18.0)
                                                    .color(colors::MUTED_FOREGROUND),
                                            )
                                            .min_size(egui::vec2(26.0, 24.0)),
                                        )
                                        .on_hover_text("Stop script");
                                    #[cfg(test)]
                                    self.stop_rects.insert(script.id, stop.rect);
                                    if stop.clicked() {
                                        response.app_actions.push(AppAction::StopScript {
                                            script_id: script.id,
                                        });
                                    }

                                    let log = script_icon_button(
                                        ui,
                                        ICON_DESCRIPTION,
                                        "Open script log",
                                        true,
                                    );
                                    #[cfg(test)]
                                    self.log_rects.insert(script.id, log.rect);
                                    if log.clicked() {
                                        self.script_log_windows.insert(script.id);
                                    }

                                    let documentation = script_icon_button(
                                        ui,
                                        ICON_QUESTION_MARK,
                                        "Open script documentation",
                                        script.documentation.is_some(),
                                    );
                                    #[cfg(test)]
                                    self.documentation_rects
                                        .insert(script.id, documentation.rect);
                                    if documentation.clicked() {
                                        self.script_documentation_windows.insert(script.id);
                                    }

                                    let status = script_icon_button(
                                        ui,
                                        ICON_INFO,
                                        "Open script status",
                                        true,
                                    );
                                    #[cfg(test)]
                                    self.status_rects.insert(script.id, status.rect);
                                    if status.clicked() {
                                        self.script_status_windows.insert(script.id);
                                    }

                                    let export = script_icon_button(
                                        ui,
                                        ICON_DOWNLOAD,
                                        "Export script source",
                                        true,
                                    );
                                    #[cfg(test)]
                                    self.export_rects.insert(script.id, export.rect);
                                    if export.clicked() {
                                        response.app_actions.push(AppAction::ExportScript {
                                            script_id: script.id,
                                        });
                                    }

                                    if script.kind == ScriptKind::Session {
                                        let run_once = script_icon_button(
                                            ui,
                                            ICON_MOVE,
                                            "Convert session script to run once",
                                            true,
                                        );
                                        #[cfg(test)]
                                        self.ownership_rects.insert(script.id, run_once.rect);
                                        if run_once.clicked() {
                                            response.app_actions.push(
                                                AppAction::ConvertScriptKind {
                                                    script_id: script.id,
                                                    kind: ScriptKind::Ephemeral,
                                                },
                                            );
                                        }
                                        let remove = script_icon_button(
                                            ui,
                                            ICON_DELETE,
                                            "Remove script from session",
                                            true,
                                        );
                                        if remove.clicked() {
                                            response.app_actions.push(
                                                AppAction::RemoveSessionScript {
                                                    script_id: script.id,
                                                },
                                            );
                                        }
                                    } else {
                                        let include = script_icon_button(
                                            ui,
                                            ICON_ARCHIVE,
                                            "Include script in session",
                                            true,
                                        );
                                        #[cfg(test)]
                                        self.ownership_rects.insert(script.id, include.rect);
                                        if include.clicked() {
                                            response.app_actions.push(
                                                AppAction::ConvertScriptKind {
                                                    script_id: script.id,
                                                    kind: ScriptKind::Session,
                                                },
                                            );
                                        }
                                    }

                                    if script.kind == ScriptKind::User {
                                        let reload = script_icon_button(
                                            ui,
                                            ICON_REFRESH,
                                            "Reload script from file",
                                            script_paths.is_some_and(|paths| {
                                                paths.contains_key(&script.id)
                                            }),
                                        );
                                        #[cfg(test)]
                                        self.reload_rects.insert(script.id, reload.rect);
                                        if reload.clicked() {
                                            response.settings_actions.push(
                                                SettingsAction::RequestReloadUserScript {
                                                    script_id: script.id,
                                                },
                                            );
                                        }
                                        let remove = script_icon_button(
                                            ui,
                                            ICON_DELETE,
                                            "Remove user script",
                                            script_paths.is_some_and(|paths| {
                                                paths.contains_key(&script.id)
                                            }),
                                        );
                                        #[cfg(test)]
                                        self.remove_rects.insert(script.id, remove.rect);
                                        if remove.clicked() {
                                            if let Some(path) =
                                                script_paths.and_then(|paths| paths.get(&script.id))
                                            {
                                                self.remove_user_script_path(path);
                                            }
                                        }
                                    }
                                });
                                ui.end_row();
                            }
                        });
                });
            #[cfg(test)]
            self.script_group_rects
                .insert(script_kind_heading(kind), _group.header_response.rect);
        }
    }

    fn show_script_enabled(
        &mut self,
        ui: &mut egui::Ui,
        script: &ScriptState,
        script_paths: Option<&BTreeMap<ScriptId, String>>,
        response: &mut SettingsDialogResponse,
    ) {
        let compatible = script.lifecycle != ScriptLifecycle::Incompatible;
        match script.kind {
            ScriptKind::Bundled => {
                let Some(identity) = script.identity.as_deref() else {
                    ui.weak("—");
                    return;
                };
                let mut scripts = self
                    .draft
                    .as_ref()
                    .and_then(|draft| draft.get(BUILTIN_SCRIPTS).ok())
                    .unwrap_or_default();
                let mut enabled = scripts
                    .0
                    .iter()
                    .find(|entry| entry.value == identity)
                    .map_or(script.enabled, |entry| entry.enabled);
                if ui
                    .add_enabled(compatible, egui::Checkbox::without_text(&mut enabled))
                    .on_hover_text("Run this built-in script at startup")
                    .changed()
                {
                    if let Some(entry) = scripts.0.iter_mut().find(|entry| entry.value == identity)
                    {
                        entry.enabled = enabled;
                    } else {
                        scripts.0.push(StringToggle {
                            value: identity.to_owned(),
                            enabled,
                        });
                    }
                    if let Some(draft) = &mut self.draft {
                        draft.set(BUILTIN_SCRIPTS, scripts);
                    }
                }
            }
            ScriptKind::Example => {
                ui.weak("—");
            }
            ScriptKind::User => {
                let Some(path) = script_paths.and_then(|paths| paths.get(&script.id)) else {
                    ui.weak("—");
                    return;
                };
                let mut scripts = self
                    .draft
                    .as_ref()
                    .and_then(|draft| draft.get(USER_SCRIPTS).ok())
                    .unwrap_or_default();
                let mut enabled = scripts
                    .0
                    .iter()
                    .find(|entry| entry.value == *path)
                    .map_or(script.enabled, |entry| entry.enabled);
                if ui
                    .add_enabled(compatible, egui::Checkbox::without_text(&mut enabled))
                    .on_hover_text("Run this user script at startup")
                    .changed()
                {
                    if let Some(entry) = scripts.0.iter_mut().find(|entry| entry.value == *path) {
                        entry.enabled = enabled;
                    } else {
                        scripts.0.push(StringToggle {
                            value: path.clone(),
                            enabled,
                        });
                    }
                    if let Some(draft) = &mut self.draft {
                        draft.set(USER_SCRIPTS, scripts);
                    }
                }
            }
            ScriptKind::Session => {
                let mut enabled = script.enabled;
                if ui
                    .add_enabled(compatible, egui::Checkbox::without_text(&mut enabled))
                    .on_hover_text("Enable this session script")
                    .changed()
                {
                    response.app_actions.push(AppAction::SetScriptEnabled {
                        script_id: script.id,
                        enabled,
                    });
                }
            }
            ScriptKind::Ephemeral => {
                ui.weak("—");
            }
        }
    }

    fn show_script_windows(
        &mut self,
        context: &egui::Context,
        scripting: &ScriptingState,
        script_paths: Option<&BTreeMap<ScriptId, String>>,
    ) {
        for script_id in self.script_log_windows.iter().copied().collect::<Vec<_>>() {
            let Some(script) = scripting
                .scripts
                .iter()
                .find(|script| script.id == script_id)
            else {
                self.script_log_windows.remove(&script_id);
                continue;
            };
            let mut open = true;
            egui::Window::new(format!("{} — Log", script.name))
                .id(egui::Id::new(("script_log", script_id)))
                .open(&mut open)
                .default_size([520.0, 300.0])
                .show(context, |ui| show_script_log(ui, script));
            if !open {
                self.script_log_windows.remove(&script_id);
            }
        }

        for script_id in self
            .script_documentation_windows
            .iter()
            .copied()
            .collect::<Vec<_>>()
        {
            let Some(script) = scripting
                .scripts
                .iter()
                .find(|script| script.id == script_id)
            else {
                self.script_documentation_windows.remove(&script_id);
                continue;
            };
            let mut open = true;
            egui::Window::new(format!("{} — Documentation", script.name))
                .id(egui::Id::new(("script_documentation", script_id)))
                .open(&mut open)
                .default_size([640.0, 500.0])
                .show(context, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let script_path = script_paths
                            .and_then(|paths| paths.get(&script.id))
                            .map(String::as_str)
                            .unwrap_or(&script.name);
                        crate::script_markdown_viewer(
                            script_path,
                            script.resource_base_uri.as_deref(),
                        )
                        .show(
                            ui,
                            &mut self.markdown_cache,
                            script
                                .documentation
                                .as_deref()
                                .unwrap_or("*No documentation*"),
                        );
                    });
                });
            if !open {
                self.script_documentation_windows.remove(&script_id);
            }
        }

        for script_id in self
            .script_status_windows
            .iter()
            .copied()
            .collect::<Vec<_>>()
        {
            let Some(script) = scripting
                .scripts
                .iter()
                .find(|script| script.id == script_id)
            else {
                self.script_status_windows.remove(&script_id);
                continue;
            };
            let mut open = true;
            egui::Window::new(format!("{} — Status", script.name))
                .id(egui::Id::new(("script_status", script_id)))
                .open(&mut open)
                .default_width(420.0)
                .resizable(true)
                .show(context, |ui| show_script_status(ui, script));
            if !open {
                self.script_status_windows.remove(&script_id);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn draft_mut(&mut self) -> Option<&mut SettingsDraft> {
        self.draft.as_mut()
    }

    #[cfg(test)]
    pub(crate) fn select_category(&mut self, category: &str) {
        self.active_category = Some(category.to_owned());
    }

    #[cfg(test)]
    pub(crate) const fn ephemeral_picker_rect(&self) -> Option<egui::Rect> {
        self.ephemeral_picker_rect
    }

    #[cfg(test)]
    pub(crate) fn restart_rect(&self, script_id: ScriptId) -> Option<egui::Rect> {
        self.restart_rects.get(&script_id).copied()
    }

    #[cfg(test)]
    pub(crate) fn log_rect(&self, script_id: ScriptId) -> Option<egui::Rect> {
        self.log_rects.get(&script_id).copied()
    }

    #[cfg(test)]
    pub(crate) fn documentation_rect(&self, script_id: ScriptId) -> Option<egui::Rect> {
        self.documentation_rects.get(&script_id).copied()
    }

    #[cfg(test)]
    pub(crate) fn status_rect(&self, script_id: ScriptId) -> Option<egui::Rect> {
        self.status_rects.get(&script_id).copied()
    }

    #[cfg(test)]
    pub(crate) fn reload_rect(&self, script_id: ScriptId) -> Option<egui::Rect> {
        self.reload_rects.get(&script_id).copied()
    }

    #[cfg(test)]
    pub(crate) fn export_rect(&self, script_id: ScriptId) -> Option<egui::Rect> {
        self.export_rects.get(&script_id).copied()
    }

    #[cfg(test)]
    pub(crate) fn ownership_rect(&self, script_id: ScriptId) -> Option<egui::Rect> {
        self.ownership_rects.get(&script_id).copied()
    }
}

fn script_kind_heading(kind: ScriptKind) -> &'static str {
    match kind {
        ScriptKind::Bundled => "Built-in scripts",
        ScriptKind::Example => "Example scripts",
        ScriptKind::User => "User scripts",
        ScriptKind::Session => "Session scripts",
        ScriptKind::Ephemeral => "Run-once scripts",
    }
}

fn script_is_active(lifecycle: ScriptLifecycle) -> bool {
    matches!(
        lifecycle,
        ScriptLifecycle::Running | ScriptLifecycle::Listening
    )
}

fn script_lifecycle_label(lifecycle: ScriptLifecycle) -> &'static str {
    match lifecycle {
        ScriptLifecycle::Inactive => "Inactive",
        ScriptLifecycle::Running => "Running",
        ScriptLifecycle::Listening => "Listening",
        ScriptLifecycle::Finished => "Finished",
        ScriptLifecycle::Incompatible => "Incompatible",
        ScriptLifecycle::Error => "Error",
    }
}

fn show_script_lifecycle(ui: &mut egui::Ui, script: &ScriptState) {
    let color = match script.lifecycle {
        ScriptLifecycle::Running | ScriptLifecycle::Listening => colors::SUCCESS,
        ScriptLifecycle::Incompatible | ScriptLifecycle::Error => colors::ERROR,
        ScriptLifecycle::Inactive | ScriptLifecycle::Finished => colors::MUTED_FOREGROUND,
    };
    let status = ui.colored_label(color, script_lifecycle_label(script.lifecycle));
    if let Some(error) = &script.latest_error {
        status.on_hover_text(error);
    }
}

fn script_icon_button(
    ui: &mut egui::Ui,
    icon: MaterialIcon,
    tooltip: &str,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(icon.rich_text().size(18.0)).min_size(egui::vec2(26.0, 24.0)),
    )
    .on_hover_text(tooltip)
}

fn show_script_log(ui: &mut egui::Ui, script: &ScriptState) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        if script.logs.is_empty() {
            ui.weak("No messages");
        }
        for entry in script.logs.iter() {
            let color = match entry.level {
                ScriptLogLevel::Warning => colors::WARNING,
                ScriptLogLevel::Error => colors::ERROR,
                _ => ui.visuals().text_color(),
            };
            ui.colored_label(color, egui::RichText::new(&entry.message).monospace());
        }
    });
}

fn show_script_status(ui: &mut egui::Ui, script: &ScriptState) {
    ui.horizontal(|ui| {
        ui.strong("Lifecycle:");
        show_script_lifecycle(ui, script);
    });
    if let Some(error) = &script.latest_error {
        ui.colored_label(colors::ERROR, error);
    }
    ui.separator();
    ui.strong("Callbacks");
    egui::Grid::new(("script_callbacks", script.id))
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Loop");
            ui.label(script.activity.loop_callbacks.to_string());
            ui.end_row();
            ui.label("Global");
            ui.label(script.activity.global_callbacks.to_string());
            ui.end_row();
            ui.label("Keyboard");
            ui.label(script.activity.keyboard_callbacks.to_string());
            ui.end_row();
            ui.label("Timers");
            ui.label(script.activity.timers.to_string());
            ui.end_row();
        });
    ui.add_space(6.0);
    ui.strong("MIDI");
    ui.label(format!(
        "{} rules · {} connections · {} dropped messages · {} errors",
        script.midi.rules,
        script.midi.connections,
        script.midi.dropped_messages,
        script.midi.errors
    ));
    for rule in script.midi.rule_states.iter() {
        let direction = match rule.direction {
            crate::ScriptMidiRuleDirection::Input => "Input",
            crate::ScriptMidiRuleDirection::Output => "Output",
        };
        ui.collapsing(format!("{direction}: /{}/", rule.pattern), |ui| {
            if rule.matched_endpoints.is_empty() {
                ui.weak("No matching endpoints");
            } else {
                ui.label(format!("Matched: {}", rule.matched_endpoints.join(", ")));
            }
            if rule.connected_endpoints.is_empty() {
                ui.weak("Not connected");
            } else {
                ui.label(format!(
                    "Connected: {}",
                    rule.connected_endpoints.join(", ")
                ));
            }
            if let Some(error) = &rule.latest_error {
                ui.colored_label(colors::ERROR, format!("Latest failure: {error}"));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_settings::{
        SettingDefinition, SettingKey, SettingsDiagnostic, SettingsRegistryBuilder,
    };

    const COUNT: SettingKey<u32> = SettingKey::new("test.count");
    const FLAG: SettingKey<bool> = SettingKey::new("other.flag");

    fn fixture() -> (Arc<SettingsRegistry>, SettingsViewState) {
        let mut builder = SettingsRegistryBuilder::default();
        builder
            .register(SettingDefinition::new(
                COUNT,
                2,
                "Track defaults",
                "Count",
                "Default channel count",
            ))
            .unwrap();
        builder
            .register(SettingDefinition::new(
                FLAG,
                false,
                "Other",
                "Flag",
                "Another category",
            ))
            .unwrap();
        let registry = Arc::new(builder.finish());
        let active = Arc::new(registry.defaults(1));
        (
            Arc::clone(&registry),
            SettingsViewState {
                active,
                diagnostics: Arc::from([]),
                storage_location: "fixture".to_owned(),
                recovery_required: false,
                persistence: SettingsPersistenceState::Idle,
            },
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn dialog_paints_category_tabs_at_minimum_and_common_sizes() {
        let (registry, state) = fixture();
        let context = egui::Context::default();
        let mut dialog = SettingsDialog::new(registry);
        dialog.open(&state);
        for size in [egui::vec2(360.0, 200.0), egui::vec2(900.0, 600.0)] {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    dialog.show(
                        ui.ctx(),
                        &state,
                        &ScriptingState::default(),
                        &AudioDriverRuntimeState::default(),
                        None,
                    );
                },
            );
            output.textures_delta.clear();
            assert!(!output.shapes.is_empty());
            assert!(dialog.is_open());
        }
        assert_eq!(
            dialog.categories(),
            ["Other", "Track defaults", "Developer"]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn developer_category_starts_tracing_with_engine_detail() {
        let (registry, _) = fixture();
        let context = egui::Context::default();
        let mut dialog = SettingsDialog::new(registry);
        dialog.set_tracing_status(TracingStatus {
            available: true,
            active: false,
            buffer_capacity_bytes: 0,
        });
        dialog.tracing_engine_detail = true;
        let frame = |dialog: &mut SettingsDialog, events: Vec<egui::Event>| {
            let mut response = SettingsDialogResponse::default();
            let mut ignored_output_0 = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(500.0, 300.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| dialog.show_developer(ui, &mut response),
            );
            ignored_output_0.textures_delta.clear();
            response
        };

        frame(&mut dialog, Vec::new());
        let start = dialog.tracing_start_rect.unwrap().center();
        frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let response = frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(response.settings_actions.iter().any(|action| matches!(
            action,
            SettingsAction::StartTracing {
                engine_detail: true
            }
        )));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn settings_window_width_stabilizes_across_frames() {
        let (registry, state) = fixture();
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut dialog = SettingsDialog::new(registry);
        dialog.open(&state);
        let window_id = egui::Id::new("settings_dialog");
        let mut widths = Vec::new();
        for _ in 0..12 {
            let mut ignored_output_1 = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    dialog.show(
                        ui.ctx(),
                        &state,
                        &ScriptingState::default(),
                        &AudioDriverRuntimeState::default(),
                        None,
                    );
                },
            );
            ignored_output_1.textures_delta.clear();
            widths.push(
                context
                    .memory(|memory| memory.area_rect(window_id))
                    .unwrap()
                    .width(),
            );
        }
        assert!(
            widths[3..]
                .windows(2)
                .all(|pair| (pair[0] - pair[1]).abs() < 0.1),
            "settings window kept changing width: {widths:?}"
        );
        assert!(
            widths.last().unwrap() < &800.0,
            "unexpected width: {widths:?}"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn audio_category_and_exact_rate_warning_paint_at_supported_sizes() {
        let mut builder = SettingsRegistryBuilder::default();
        crate::register_audio_settings(&mut builder).unwrap();
        let registry = Arc::new(builder.finish());
        let state = SettingsViewState {
            active: Arc::new(registry.defaults(1)),
            diagnostics: Arc::from([]),
            storage_location: "fixture".to_owned(),
            recovery_required: false,
            persistence: SettingsPersistenceState::Idle,
        };
        let source = crate::ResolvedAudioDriverConfig {
            configured: crate::AudioDriverConfig::Dummy(crate::DummyAudioDriverConfig::default()),
            sample_rate: 48_000,
            buffer_size: 256,
            instance_name: "dummy".to_owned(),
        };
        let target = crate::ResolvedAudioDriverConfig {
            configured: crate::AudioDriverConfig::Dummy(crate::DummyAudioDriverConfig {
                sample_rate: 44_100,
                buffer_size: 128,
            }),
            sample_rate: 44_100,
            buffer_size: 128,
            instance_name: "dummy variant".to_owned(),
        };
        let audio = AudioDriverRuntimeState {
            supported: true,
            catalog: Arc::from([
                crate::AudioDriverDescriptor {
                    kind: AudioDriverKind::Dummy,
                    available: true,
                    ..Default::default()
                },
                crate::AudioDriverDescriptor {
                    kind: AudioDriverKind::Jack,
                    available: false,
                    unavailable_reason: Some("JACK server unavailable".to_owned()),
                    ..Default::default()
                },
                crate::AudioDriverDescriptor {
                    kind: AudioDriverKind::Cpal,
                    available: true,
                    unavailable_reason: None,
                    hosts: vec!["alsa".to_owned()],
                    input_devices: vec!["capture".to_owned()],
                    output_devices: vec!["playback".to_owned()],
                    midi_inputs: vec!["controller".to_owned()],
                    midi_outputs: vec!["synth".to_owned()],
                },
            ]),
            active: Some(source.clone()),
            switch: crate::AudioDriverSwitchState {
                request_id: 9,
                status: crate::AudioDriverSwitchStatus::AwaitingConfirmation,
                source: Some(source),
                target: Some(target),
                message: "Sample rate changes from 48000 Hz to 44100 Hz".to_owned(),
                persistence_retry_available: false,
            },
        };
        let context = egui::Context::default();
        let mut dialog = SettingsDialog::new(registry);
        dialog.open(&state);
        dialog.select_category("Audio");
        for size in [egui::vec2(360.0, 200.0), egui::vec2(900.0, 600.0)] {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    dialog.show(ui.ctx(), &state, &ScriptingState::default(), &audio, None);
                },
            );
            output.textures_delta.clear();
            assert!(!output.shapes.is_empty());
        }
        assert_eq!(dialog.audio_target, Some(AudioDriverKind::Dummy));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn generic_loop_audio_setting_is_visible_without_driver_switching() {
        let mut builder = SettingsRegistryBuilder::default();
        crate::register_audio_settings(&mut builder).unwrap();
        let registry = Arc::new(builder.finish());
        let state = SettingsViewState {
            active: Arc::new(registry.defaults(1)),
            diagnostics: Arc::from([]),
            storage_location: "fixture".to_owned(),
            recovery_required: false,
            persistence: SettingsPersistenceState::Idle,
        };
        let mut dialog = SettingsDialog::new(registry);
        dialog.open(&state);
        let context = egui::Context::default();
        let mut response = SettingsDialogResponse::default();
        let mut output = context.run_ui(Default::default(), |ui| {
            dialog.show_audio(ui, &AudioDriverRuntimeState::default(), &mut response);
        });
        output.textures_delta.clear();
        assert_eq!(dialog.setting_card_rects.len(), 1);
        assert!(response.app_actions.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn setting_cards_fill_the_available_width() {
        let (registry, state) = fixture();
        let mut dialog = SettingsDialog::new(registry);
        dialog.open(&state);
        let context = egui::Context::default();
        let mut ignored_output_2 = context.run_ui(Default::default(), |ui| {
            ui.set_width(480.0);
            dialog.show_definitions(ui, "Track defaults");
        });
        ignored_output_2.textures_delta.clear();
        assert_eq!(dialog.setting_card_rects.len(), 1);
        assert!(dialog.setting_card_rects[0].width() >= 479.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn save_preserves_stable_typed_draft_and_reset_restores_defaults() {
        let (registry, state) = fixture();
        let mut dialog = SettingsDialog::new(registry.clone());
        dialog.open(&state);
        let draft = dialog.draft_mut().unwrap();
        draft.set(COUNT, 8);
        draft.set(FLAG, true);
        assert_eq!(draft.get(COUNT).unwrap(), 8);
        let SettingsAction::Save(saved) = dialog.save_action().unwrap() else {
            panic!("expected save action");
        };
        assert_eq!(saved.get(COUNT).unwrap(), 8);
        assert!(saved.get(FLAG).unwrap());
        let draft = dialog.draft_mut().unwrap();
        draft.reset(registry.definition(COUNT.id()).unwrap());
        assert_eq!(draft.get(COUNT).unwrap(), 2);
        draft.reset_all(&registry);
        assert!(!draft.get(FLAG).unwrap());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn appearance_scale_changes_only_when_explicitly_applied() {
        let mut builder = SettingsRegistryBuilder::default();
        crate::register_settings(&mut builder).unwrap();
        let registry = Arc::new(builder.finish());
        let state = SettingsViewState {
            active: Arc::new(registry.defaults(1)),
            diagnostics: Arc::from([]),
            storage_location: "fixture".to_owned(),
            recovery_required: false,
            persistence: SettingsPersistenceState::Idle,
        };
        let mut dialog = SettingsDialog::new(registry);
        dialog.open(&state);
        dialog.draft_mut().unwrap().set(UI_SCALE_FACTOR, 1.5);
        let context = egui::Context::default();

        let mut ignored_output_3 =
            context.run_ui(Default::default(), |ui| dialog.show_appearance(ui));
        ignored_output_3.textures_delta.clear();
        assert!((context.zoom_factor() - 1.0).abs() < f32::EPSILON);

        dialog.apply_appearance(&context);
        let mut ignored_output_4 = context.run_ui(Default::default(), |_| {});
        ignored_output_4.textures_delta.clear();
        assert!((context.zoom_factor() - 1.5).abs() < f32::EPSILON);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn user_script_paths_are_typed_deduplicated_draft_values() {
        let mut builder = SettingsRegistryBuilder::default();
        crate::register_settings(&mut builder).unwrap();
        crate::register_script_settings(&mut builder).unwrap();
        let registry = Arc::new(builder.finish());
        let state = SettingsViewState {
            active: Arc::new(registry.defaults(1)),
            diagnostics: Arc::from([]),
            storage_location: "fixture".to_owned(),
            recovery_required: false,
            persistence: SettingsPersistenceState::Idle,
        };
        let mut dialog = SettingsDialog::new(registry);
        dialog.open(&state);
        dialog
            .add_user_script_path("/tmp/controller.lua".to_owned())
            .unwrap();
        dialog
            .add_user_script_path("/tmp/controller.lua".to_owned())
            .unwrap();
        assert_eq!(
            dialog.draft_mut().unwrap().get(USER_SCRIPTS).unwrap(),
            StringToggleList(vec![StringToggle {
                value: "/tmp/controller.lua".to_owned(),
                enabled: true,
            }])
        );
        assert_eq!(dialog.active_category.as_deref(), Some("Scripts"));
        dialog.remove_user_script_path("/tmp/controller.lua");
        assert_eq!(
            dialog.draft_mut().unwrap().get(USER_SCRIPTS).unwrap(),
            StringToggleList::default()
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn script_kinds_have_stable_table_groups() {
        assert_eq!(
            [
                ScriptKind::Bundled,
                ScriptKind::Example,
                ScriptKind::User,
                ScriptKind::Session,
                ScriptKind::Ephemeral,
            ]
            .map(script_kind_heading),
            [
                "Built-in scripts",
                "Example scripts",
                "User scripts",
                "Session scripts",
                "Run-once scripts",
            ]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn example_script_group_is_collapsed_by_default_and_can_expand() {
        let mut builder = SettingsRegistryBuilder::default();
        crate::register_settings(&mut builder).unwrap();
        crate::register_script_settings(&mut builder).unwrap();
        let registry = Arc::new(builder.finish());
        let state = SettingsViewState {
            active: Arc::new(registry.defaults(1)),
            diagnostics: Arc::from([]),
            storage_location: "fixture".to_owned(),
            recovery_required: false,
            persistence: SettingsPersistenceState::Idle,
        };
        let mut dialog = SettingsDialog::new(registry);
        dialog.open(&state);
        let script_id = ScriptId::from_raw(10);
        let scripting = ScriptingState {
            supported: true,
            scripts: Arc::from([crate::ScriptState {
                id: script_id,
                name: "dialogs.lua".to_owned(),
                identity: None,
                kind: ScriptKind::Example,
                enabled: false,
                lifecycle: ScriptLifecycle::Inactive,
                documentation: Some("# Dialog example".to_owned()),
                resource_base_uri: None,
                latest_error: None,
                activity: Default::default(),
                midi: Default::default(),
                logs: Arc::from([]),
            }]),
            ..Default::default()
        };
        let context = egui::Context::default();
        crate::initialize(&context);
        let frame = |dialog: &mut SettingsDialog, events: Vec<egui::Event>| {
            let mut ignored_output_5 = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    dialog.show_script_runtime(
                        ui,
                        &scripting,
                        None,
                        &mut SettingsDialogResponse::default(),
                    )
                },
            );
            ignored_output_5.textures_delta.clear();
        };

        frame(&mut dialog, Vec::new());
        assert!(dialog.restart_rect(script_id).is_none());
        let position = dialog.script_group_rects["Example scripts"].center();
        frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(&mut dialog, Vec::new());
        assert!(dialog.restart_rect(script_id).is_some());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn script_documentation_window_renders_markdown() {
        let (registry, _) = fixture();
        let mut dialog = SettingsDialog::new(registry);
        let script_id = ScriptId::from_raw(9);
        let scripting = ScriptingState {
            supported: true,
            scripts: Arc::from([crate::ScriptState {
                id: script_id,
                name: "documented.lua".to_owned(),
                identity: None,
                kind: ScriptKind::User,
                enabled: true,
                lifecycle: ScriptLifecycle::Listening,
                documentation: Some(
                    "# Guide\n\n| Key | Action |\n| --- | --- |\n| Space | Play |\n".to_owned(),
                ),
                resource_base_uri: None,
                latest_error: None,
                activity: Default::default(),
                midi: Default::default(),
                logs: Arc::from([]),
            }]),
            ..Default::default()
        };
        dialog.script_documentation_windows.insert(script_id);
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 700.0),
                )),
                ..Default::default()
            },
            |ui| dialog.show_script_windows(ui.ctx(), &scripting, None),
        );
        output.textures_delta.clear();
        assert!(output.shapes.len() > 5);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn incompatible_script_is_labeled_and_cannot_emit_restart() {
        let (registry, state) = fixture();
        let mut dialog = SettingsDialog::new(registry);
        dialog.open(&state);
        let script_id = ScriptId::from_raw(11);
        let scripting = ScriptingState {
            supported: true,
            scripts: Arc::from([crate::ScriptState {
                id: script_id,
                name: "future.lua".to_owned(),
                identity: None,
                kind: ScriptKind::Ephemeral,
                enabled: true,
                lifecycle: ScriptLifecycle::Incompatible,
                documentation: None,
                resource_base_uri: None,
                latest_error: Some("script requests 2.0, host supports 1.2".to_owned()),
                activity: Default::default(),
                midi: Default::default(),
                logs: Arc::from([]),
            }]),
            ..Default::default()
        };
        assert_eq!(
            script_lifecycle_label(ScriptLifecycle::Incompatible),
            "Incompatible"
        );
        let context = egui::Context::default();
        crate::initialize(&context);
        let frame = |dialog: &mut SettingsDialog, events: Vec<egui::Event>| {
            let mut response = SettingsDialogResponse::default();
            let mut ignored_output_6 = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| dialog.show_script_runtime(ui, &scripting, None, &mut response),
            );
            ignored_output_6.textures_delta.clear();
            response
        };
        frame(&mut dialog, Vec::new());
        let restart = dialog.restart_rect(script_id).unwrap().center();
        frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(restart),
                egui::Event::PointerButton {
                    pos: restart,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let response = frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(restart),
                egui::Event::PointerButton {
                    pos: restart,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(!response
            .app_actions
            .contains(&AppAction::RestartScript { script_id }));
        assert!(dialog.export_rect(script_id).is_some());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn runtime_script_controls_emit_typed_actions_inside_the_settings_content() {
        let mut builder = SettingsRegistryBuilder::default();
        crate::register_settings(&mut builder).unwrap();
        crate::register_script_settings(&mut builder).unwrap();
        let registry = Arc::new(builder.finish());
        let state = SettingsViewState {
            active: Arc::new(registry.defaults(1)),
            diagnostics: Arc::from([]),
            storage_location: "fixture".to_owned(),
            recovery_required: false,
            persistence: SettingsPersistenceState::Idle,
        };
        let mut dialog = SettingsDialog::new(registry);
        dialog.open(&state);
        let script_id = ScriptId::from_raw(7);
        let second_script_id = ScriptId::from_raw(8);
        let scripting = ScriptingState {
            supported: true,
            scripts: Arc::from([
                crate::ScriptState {
                    id: script_id,
                    name: "controller.lua".to_owned(),
                    identity: None,
                    kind: ScriptKind::User,
                    enabled: true,
                    lifecycle: crate::ScriptLifecycle::Listening,
                    documentation: Some("Controller documentation".to_owned()),
                    resource_base_uri: None,
                    latest_error: None,
                    activity: Default::default(),
                    midi: Default::default(),
                    logs: Arc::from([]),
                },
                crate::ScriptState {
                    id: second_script_id,
                    name: "second.lua".to_owned(),
                    identity: None,
                    kind: ScriptKind::User,
                    enabled: false,
                    lifecycle: crate::ScriptLifecycle::Inactive,
                    documentation: None,
                    resource_base_uri: None,
                    latest_error: None,
                    activity: Default::default(),
                    midi: Default::default(),
                    logs: Arc::from([]),
                },
            ]),
            ..Default::default()
        };
        let paths = BTreeMap::from([
            (script_id, "/tmp/controller.lua".to_owned()),
            (second_script_id, "/tmp/second.lua".to_owned()),
        ]);
        let context = egui::Context::default();
        crate::initialize(&context);
        let frame = |dialog: &mut SettingsDialog, events: Vec<egui::Event>| {
            let mut response = SettingsDialogResponse::default();
            let mut ignored_output_7 = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| dialog.show_script_runtime(ui, &scripting, Some(&paths), &mut response),
            );
            ignored_output_7.textures_delta.clear();
            response
        };
        frame(&mut dialog, Vec::new());
        let picker = dialog.ephemeral_picker_rect().unwrap().center();
        frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(picker),
                egui::Event::PointerButton {
                    pos: picker,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let response = frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(picker),
                egui::Event::PointerButton {
                    pos: picker,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(response
            .settings_actions
            .iter()
            .any(|action| matches!(action, SettingsAction::RequestEphemeralScriptPicker)));

        let restart = dialog.restart_rect(script_id).unwrap().center();
        frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(restart),
                egui::Event::PointerButton {
                    pos: restart,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let response = frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(restart),
                egui::Event::PointerButton {
                    pos: restart,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(response
            .app_actions
            .contains(&AppAction::RestartScript { script_id }));

        let reload = dialog.reload_rect(script_id).unwrap().center();
        frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(reload),
                egui::Event::PointerButton {
                    pos: reload,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let response = frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(reload),
                egui::Event::PointerButton {
                    pos: reload,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(response.settings_actions.iter().any(|action| matches!(
            action,
            SettingsAction::RequestReloadUserScript { script_id: id } if *id == script_id
        )));
        let export = dialog.export_rect(script_id).unwrap().center();
        frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(export),
                egui::Event::PointerButton {
                    pos: export,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let response = frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(export),
                egui::Event::PointerButton {
                    pos: export,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(response
            .app_actions
            .contains(&AppAction::ExportScript { script_id }));

        let ownership = dialog.ownership_rect(script_id).unwrap().center();
        frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(ownership),
                egui::Event::PointerButton {
                    pos: ownership,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let response = frame(
            &mut dialog,
            vec![
                egui::Event::PointerMoved(ownership),
                egui::Event::PointerButton {
                    pos: ownership,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(response
            .app_actions
            .contains(&AppAction::ConvertScriptKind {
                script_id,
                kind: ScriptKind::Session,
            }));
        assert!(dialog.log_rect(script_id).is_some());
        assert!(dialog.documentation_rect(script_id).is_some());
        assert!(dialog.status_rect(script_id).is_some());
        let first_row = dialog.restart_rect(script_id).unwrap();
        let second_row = dialog.restart_rect(second_script_id).unwrap();
        assert!((first_row.center().x - second_row.center().x).abs() < 1.0);
        assert!(second_row.top() > first_row.bottom());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn recovery_and_diagnostics_paint_without_editing_rejected_values() {
        let (registry, mut state) = fixture();
        state.recovery_required = true;
        state.persistence = SettingsPersistenceState::Failed;
        state.diagnostics = Arc::from([SettingsDiagnostic {
            key: None,
            message: "future version rejected".to_owned(),
        }]);
        let context = egui::Context::default();
        let mut dialog = SettingsDialog::new(registry);
        dialog.open(&state);
        let mut output = context.run_ui(Default::default(), |ui| {
            dialog.show(
                ui.ctx(),
                &state,
                &ScriptingState::default(),
                &AudioDriverRuntimeState::default(),
                None,
            );
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }
}
