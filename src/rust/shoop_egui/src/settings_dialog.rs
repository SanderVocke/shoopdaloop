use std::{collections::BTreeMap, sync::Arc};

use shoop_settings::{
    SettingEditor, SettingValue, SettingsDraft, SettingsPersistenceState, SettingsRegistry,
    SettingsViewState, StringToggle, StringToggleList,
};

use crate::{
    audio_driver_config_from_draft, colors, AppAction, AudioDriverKind, AudioDriverRuntimeState,
    ScriptId, ScriptKind, ScriptLogLevel, ScriptingState, USER_SCRIPTS,
};

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
    RequestEphemeralScriptPicker,
    RequestReloadUserScript {
        script_id: ScriptId,
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
            Self::RequestEphemeralScriptPicker => "settings.pick_ephemeral_script",
            Self::RequestReloadUserScript { .. } => "settings.reload_user_script",
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
    #[cfg(test)]
    ephemeral_picker_rect: Option<egui::Rect>,
    #[cfg(test)]
    restart_rects: BTreeMap<ScriptId, egui::Rect>,
    #[cfg(test)]
    reload_rects: BTreeMap<ScriptId, egui::Rect>,
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
            #[cfg(test)]
            ephemeral_picker_rect: None,
            #[cfg(test)]
            restart_rects: BTreeMap::new(),
            #[cfg(test)]
            reload_rects: BTreeMap::new(),
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

                self.show_category_tabs(ui);
                ui.separator();
                let active_category = self.active_category.clone().unwrap_or_default();
                egui::ScrollArea::vertical()
                    .id_salt("settings_values")
                    .scroll_source(crate::control_safe_scroll_source())
                    .show(ui, |ui| {
                        if active_category == "Audio" {
                            self.show_audio(ui, audio_drivers, &mut response);
                        } else {
                            self.show_definitions(ui, &active_category);
                        }
                        if active_category == "Scripts" {
                            ui.add_space(8.0);
                            self.show_script_runtime(
                                ui,
                                scripting,
                                script_paths,
                                &mut response,
                            );
                        }
                    });
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
                        let save = ui.add_enabled(
                            state.persistence != SettingsPersistenceState::Saving && !stale,
                            egui::Button::new("Save"),
                        );
                        if save.clicked() {
                            if let Some(action) = self.save_action() {
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
        } else {
            self.open = open;
        }
        self.show_audio_confirmation(context, audio_drivers, &mut response);
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
        egui::ScrollArea::horizontal()
            .id_salt("settings_category_tabs")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for category in self.categories() {
                        if ui
                            .selectable_label(
                                self.active_category.as_deref() == Some(category.as_str()),
                                &category,
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
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
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
                            (
                                SettingEditor::UnsignedInteger { min, max },
                                SettingValue::U32(value),
                            ) => {
                                ui.add(egui::DragValue::new(value).range(*min..=*max).speed(1));
                            }
                            (
                                SettingEditor::SignedInteger { min, max },
                                SettingValue::I32(value),
                            ) => {
                                ui.add(egui::DragValue::new(value).range(*min..=*max).speed(1));
                            }
                            (SettingEditor::Number { min, max }, SettingValue::F64(value)) => {
                                ui.add(egui::DragValue::new(value).range(*min..=*max));
                            }
                            (
                                SettingEditor::StringChoice { choices },
                                SettingValue::String(value),
                            ) => {
                                let selected = choices
                                    .iter()
                                    .find(|(choice, _label)| *choice == value.as_str())
                                    .map_or(value.as_str(), |(_choice, label)| *label);
                                egui::ComboBox::from_id_salt(("setting_choice", definition.key()))
                                    .selected_text(selected)
                                    .show_ui(ui, |ui| {
                                        for (choice, label) in *choices {
                                            ui.selectable_value(
                                                value,
                                                (*choice).to_owned(),
                                                *label,
                                            );
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
                                if let Some(choices) = choices.filter(|choices| !choices.is_empty())
                                {
                                    egui::ComboBox::from_id_salt((
                                        "audio_choice",
                                        definition.key(),
                                    ))
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
        ui.heading("Runtime status");
        if !scripting.supported {
            ui.colored_label(
                colors::WARNING,
                "Lua scripting and MIDI control are unavailable in this build.",
            );
            return;
        }
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
        for script in scripting.scripts.iter() {
            let user_path = script_paths
                .and_then(|paths| paths.get(&script.id))
                .filter(|_| script.kind == ScriptKind::User)
                .cloned();
            ui.push_id(("script_runtime", script.id), |ui| {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        if script.kind == ScriptKind::Session {
                            let mut enabled = script.enabled;
                            if ui.checkbox(&mut enabled, "enabled").changed() {
                                response.app_actions.push(AppAction::SetScriptEnabled {
                                    script_id: script.id,
                                    enabled,
                                });
                            }
                        }
                        ui.strong(&script.name);
                        let kind = match script.kind {
                            ScriptKind::Bundled => "Built-in",
                            ScriptKind::User => "User",
                            ScriptKind::Session => "Session",
                            ScriptKind::Ephemeral => "Run once",
                        };
                        ui.label(format!("{kind} · {:?}", script.lifecycle));
                    });
                    if let Some(path) = script_paths.and_then(|paths| paths.get(&script.id)) {
                        ui.weak(path);
                    }
                    ui.horizontal(|ui| {
                        let restart = ui.button("Restart");
                        #[cfg(test)]
                        self.restart_rects.insert(script.id, restart.rect);
                        if restart.clicked() {
                            response.app_actions.push(AppAction::RestartScript {
                                script_id: script.id,
                            });
                        }
                        if ui.button("Stop").clicked() {
                            response.app_actions.push(AppAction::StopScript {
                                script_id: script.id,
                            });
                        }
                        if user_path.is_some() {
                            let reload = ui.button("Reload file");
                            #[cfg(test)]
                            self.reload_rects.insert(script.id, reload.rect);
                            if reload.clicked() {
                                response.settings_actions.push(
                                    SettingsAction::RequestReloadUserScript {
                                        script_id: script.id,
                                    },
                                );
                            }
                        }
                        if let Some(path) = &user_path {
                            let remove = ui.button("Remove");
                            #[cfg(test)]
                            self.remove_rects.insert(script.id, remove.rect);
                            if remove.clicked() {
                                self.remove_user_script_path(path);
                            }
                        }
                    });
                    if let Some(error) = &script.latest_error {
                        ui.colored_label(colors::ERROR, error);
                    }
                    if let Some(documentation) = &script.documentation {
                        ui.collapsing("Documentation", |ui| {
                            ui.label(documentation);
                        });
                    }
                    ui.label(format!(
                        "Callbacks: {} loop, {} global, {} keyboard; {} timers",
                        script.activity.loop_callbacks,
                        script.activity.global_callbacks,
                        script.activity.keyboard_callbacks,
                        script.activity.timers
                    ));
                    ui.label(format!(
                        "MIDI: {} rules, {} connections, {} dropped, {} errors",
                        script.midi.rules,
                        script.midi.connections,
                        script.midi.dropped_messages,
                        script.midi.errors
                    ));
                    for rule in script.midi.rule_states.iter() {
                        let direction = match rule.direction {
                            crate::ScriptMidiRuleDirection::Input => "input",
                            crate::ScriptMidiRuleDirection::Output => "output",
                        };
                        ui.collapsing(format!("MIDI {direction}: /{}/", rule.pattern), |ui| {
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
                    ui.collapsing(format!("Log ({})", script.logs.len()), |ui| {
                        if script.logs.is_empty() {
                            ui.weak("No messages");
                        }
                        for entry in script.logs.iter() {
                            let color = match entry.level {
                                ScriptLogLevel::Warning => colors::WARNING,
                                ScriptLogLevel::Error => colors::ERROR,
                                _ => ui.visuals().text_color(),
                            };
                            ui.colored_label(color, &entry.message);
                        }
                    });
                });
            });
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
    pub(crate) fn reload_rect(&self, script_id: ScriptId) -> Option<egui::Rect> {
        self.reload_rects.get(&script_id).copied()
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

    #[tracy_nextest_capture::tracy_capture_test]
    fn dialog_paints_category_tabs_at_minimum_and_common_sizes() {
        let (registry, state) = fixture();
        let context = egui::Context::default();
        let mut dialog = SettingsDialog::new(registry);
        dialog.open(&state);
        for size in [egui::vec2(360.0, 200.0), egui::vec2(900.0, 600.0)] {
            let output = context.run_ui(
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
            assert!(!output.shapes.is_empty());
            assert!(dialog.is_open());
        }
        assert_eq!(dialog.categories(), ["Other", "Track defaults"]);
    }

    #[tracy_nextest_capture::tracy_capture_test]
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
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    dialog.show(ui.ctx(), &state, &ScriptingState::default(), &audio, None);
                },
            );
            assert!(!output.shapes.is_empty());
        }
        assert_eq!(dialog.audio_target, Some(AudioDriverKind::Dummy));
    }

    #[tracy_nextest_capture::tracy_capture_test]
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

    #[tracy_nextest_capture::tracy_capture_test]
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

    #[tracy_nextest_capture::tracy_capture_test]
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
        let scripting = ScriptingState {
            supported: true,
            scripts: Arc::from([crate::ScriptState {
                id: script_id,
                name: "controller.lua".to_owned(),
                kind: ScriptKind::User,
                enabled: true,
                lifecycle: crate::ScriptLifecycle::Listening,
                documentation: None,
                latest_error: None,
                activity: Default::default(),
                midi: Default::default(),
                logs: Arc::from([]),
            }]),
            ..Default::default()
        };
        let paths = BTreeMap::from([(script_id, "/tmp/controller.lua".to_owned())]);
        let context = egui::Context::default();
        let frame = |dialog: &mut SettingsDialog, events: Vec<egui::Event>| {
            let mut response = SettingsDialogResponse::default();
            let _ = context.run_ui(
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
    }

    #[tracy_nextest_capture::tracy_capture_test]
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
        let output = context.run_ui(Default::default(), |ui| {
            dialog.show(
                ui.ctx(),
                &state,
                &ScriptingState::default(),
                &AudioDriverRuntimeState::default(),
                None,
            );
        });
        assert!(!output.shapes.is_empty());
    }
}
