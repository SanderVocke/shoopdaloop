use std::collections::BTreeMap;

use crate::{
    AppAction, AppState, ConnectionDialog, ConnectionScope, DetailsPane, DirectTrackSpec,
    GlobalControls, SettingsAction, SettingsDialog, TrackWidget, TracksWidget,
};
use shoop_settings::{
    SettingDefinition, SettingEffect, SettingKey, SettingsRegistry, SettingsRegistryBuilder,
    SettingsRegistryError, SettingsViewState, StringToggleList,
};
use std::sync::Arc;

const LOGO_BYTES: &[u8] = include_bytes!("../../../../resources/logo-small.png");

pub const DEFAULT_NEW_TRACK_AUDIO_CHANNELS: SettingKey<u32> =
    SettingKey::new("tracks.new.default_audio_channels");
pub const DEFAULT_NEW_TRACK_MIDI: SettingKey<bool> = SettingKey::new("tracks.new.default_midi");
pub const KEYBOARD_SCRIPT_ENABLED: SettingKey<bool> =
    SettingKey::new("scripting.bundled.keyboard.enabled");
pub const APC_MINI_SCRIPT_ENABLED: SettingKey<bool> =
    SettingKey::new("scripting.bundled.akai_apc_mini_mk1.enabled");
pub const USER_SCRIPTS: SettingKey<StringToggleList> = SettingKey::new("scripting.user_scripts");

pub fn register_settings(
    builder: &mut SettingsRegistryBuilder,
) -> Result<(), SettingsRegistryError> {
    builder.register(
        SettingDefinition::new(
            DEFAULT_NEW_TRACK_AUDIO_CHANNELS,
            2,
            "Track defaults",
            "New track audio channels",
            "Audio channel count used when a new Add Track dialog is opened.",
        )
        .category_order(10)
        .setting_order(10)
        .effect(SettingEffect::NextUse),
    )?;
    builder.register(
        SettingDefinition::new(
            DEFAULT_NEW_TRACK_MIDI,
            false,
            "Track defaults",
            "Enable MIDI on new tracks",
            "MIDI state used when a new Add Track dialog is opened.",
        )
        .category_order(10)
        .setting_order(20)
        .effect(SettingEffect::NextUse),
    )
}

pub fn register_bundled_script_settings(
    builder: &mut SettingsRegistryBuilder,
) -> Result<(), SettingsRegistryError> {
    builder.register(
        SettingDefinition::new(
            KEYBOARD_SCRIPT_ENABLED,
            true,
            "Scripts",
            "Enable keyboard controls",
            "Run the bundled keyboard.lua script at startup.",
        )
        .category_order(20)
        .setting_order(10)
        .effect(SettingEffect::Immediate),
    )?;
    builder.register(
        SettingDefinition::new(
            APC_MINI_SCRIPT_ENABLED,
            false,
            "Scripts",
            "Enable Akai APC Mini MK1 controls",
            "Run the bundled akai_apc_mini_mk1.lua script at startup.",
        )
        .category_order(20)
        .setting_order(20)
        .effect(SettingEffect::Immediate),
    )?;
    Ok(())
}

pub fn register_script_settings(
    builder: &mut SettingsRegistryBuilder,
) -> Result<(), SettingsRegistryError> {
    register_bundled_script_settings(builder)?;
    builder.register(
        SettingDefinition::new(
            USER_SCRIPTS,
            StringToggleList::default(),
            "Scripts",
            "User Lua scripts",
            "Lua source files known to this machine and whether they run at startup.",
        )
        .category_order(20)
        .setting_order(30)
        .effect(SettingEffect::Immediate),
    )
}

pub struct AppWidgetResponse {
    pub app_actions: Vec<AppAction>,
    pub settings_actions: Vec<SettingsAction>,
}

pub struct AppWidget {
    tracks: TracksWidget,
    global_controls: GlobalControls,
    details: DetailsPane,
    sync_track: TrackWidget,
    connections: ConnectionDialog,
    settings: SettingsDialog,
    details_open: bool,
    add_track_open: bool,
    add_track_name: String,
    add_track_audio_channels: u32,
    add_track_midi: bool,
    logo: Option<egui::TextureHandle>,
    io_channel_mappings: BTreeMap<crate::TaskId, Vec<u32>>,
    io_channel_selections: BTreeMap<crate::TaskId, Vec<u32>>,
    pressed_script_keys: BTreeMap<egui::Key, (i64, i64)>,
    #[cfg(test)]
    add_track_accept_rect: Option<egui::Rect>,
    #[cfg(test)]
    add_track_cancel_rect: Option<egui::Rect>,
}

impl Default for AppWidget {
    fn default() -> Self {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).expect("built-in settings must be valid");
        Self::new(Arc::new(builder.finish()))
    }
}

impl AppWidget {
    pub fn new(settings_registry: Arc<SettingsRegistry>) -> Self {
        Self {
            tracks: TracksWidget::default(),
            global_controls: GlobalControls::default(),
            details: DetailsPane::default(),
            sync_track: TrackWidget::default(),
            connections: ConnectionDialog::default(),
            settings: SettingsDialog::new(settings_registry),
            details_open: true,
            add_track_open: false,
            add_track_name: String::new(),
            add_track_audio_channels: 2,
            add_track_midi: false,
            logo: None,
            io_channel_mappings: BTreeMap::new(),
            io_channel_selections: BTreeMap::new(),
            pressed_script_keys: BTreeMap::new(),
            #[cfg(test)]
            add_track_accept_rect: None,
            #[cfg(test)]
            add_track_cancel_rect: None,
        }
    }

    pub fn open_connections(&mut self, scope: ConnectionScope) {
        self.connections.open(scope);
    }

    pub fn add_user_script_path(&mut self, path: String) -> Result<(), &'static str> {
        self.settings.add_user_script_path(path)
    }

    pub fn open_connection_scope(&self) -> Option<ConnectionScope> {
        self.connections.is_open().then(|| self.connections.scope())
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &AppState,
        settings_state: &SettingsViewState,
        script_paths: Option<&BTreeMap<crate::ScriptId, String>>,
    ) -> AppWidgetResponse {
        self.ensure_logo(ui.ctx());
        let events = ui.ctx().input(|input| input.events.clone());
        let text_entry_active = ui.ctx().egui_wants_keyboard_input();
        let mut actions = crate::key_input::translate_events(
            &events,
            text_entry_active,
            &mut self.pressed_script_keys,
        )
        .into_iter()
        .map(AppAction::KeyEvent)
        .collect::<Vec<_>>();
        let mut settings_actions = Vec::new();

        egui::Panel::top("global_controls")
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 30, 30))
                    .inner_margin(egui::Margin::symmetric(6, 4)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("global_controls_scroll")
                    .show(ui, |ui| {
                        actions.extend(
                            self.global_controls
                                .show(ui, &state.global_controls)
                                .into_iter()
                                .map(AppAction::Global),
                        );
                        if self.global_controls.take_connections_requested() {
                            self.connections.open(ConnectionScope::AllTracks);
                        }
                        if self.global_controls.take_save_session_requested() {
                            actions.push(AppAction::RequestSaveSession);
                        }
                        if self.global_controls.take_load_session_requested() {
                            actions.push(AppAction::RequestLoadSessionPicker);
                        }
                        if self.global_controls.take_settings_requested() {
                            self.settings.open(settings_state);
                        }
                    });
            });

        egui::Panel::bottom("details_toggle")
            .resizable(false)
            .exact_size(24.0)
            .show(ui, |ui| {
                if ui.selectable_label(self.details_open, "details").clicked() {
                    self.details_open = !self.details_open;
                }
            });

        if self.details_open {
            egui::Panel::bottom("details")
                .resizable(true)
                .default_size(200.0)
                .min_size(70.0)
                .max_size(400.0)
                .frame(
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(85, 85, 85))
                        .inner_margin(egui::Margin::same(6)),
                )
                .show(ui, |ui| self.details.show(ui, state.details.as_ref()));
        }

        egui::Panel::right("logo_status_and_sync")
            .resizable(false)
            .exact_size(190.0)
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(42, 42, 42))
                    .inner_margin(egui::Margin::same(5)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("status_and_sync_scroll")
                    .show(ui, |ui| {
                        self.show_logo_and_status(ui, state);
                        if let Some(sync) = state.tracks.iter().find(|track| track.is_sync) {
                            ui.add_space(8.0);
                            ui.separator();
                            let response = self.sync_track.show(ui, sync);
                            actions.extend(response.io_intents.iter().cloned());
                            actions.extend(response.loop_actions.into_iter().map(
                                |(loop_id, action)| AppAction::Loop {
                                    track_id: sync.id,
                                    loop_id,
                                    action,
                                },
                            ));
                            if response.connections_requested {
                                self.connections.open(ConnectionScope::Track(sync.id));
                            }
                            actions.extend(response.actions.into_iter().map(|action| {
                                AppAction::Track {
                                    track_id: sync.id,
                                    action,
                                }
                            }));
                        }
                    });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 30, 30))
                    .inner_margin(8.0),
            )
            .show(ui, |ui| {
                let main_tracks: Vec<_> = state
                    .tracks
                    .iter()
                    .filter(|track| !track.is_sync)
                    .cloned()
                    .collect();
                let response = self.tracks.show(ui, &main_tracks);
                if response.add_track_requested {
                    self.open_add_track_dialog(main_tracks.len(), settings_state);
                }
                if let Some(track_id) = response.connection_track_requested {
                    self.connections.open(ConnectionScope::Track(track_id));
                }
                actions.extend(response.intents);
            });

        self.show_add_track_dialog(ui.ctx(), &mut actions);
        self.show_io_task_dialog(ui.ctx(), state, &mut actions);
        actions.extend(self.connections.show(ui.ctx(), state));
        let settings_response =
            self.settings
                .show(ui.ctx(), settings_state, &state.scripting, script_paths);
        actions.extend(settings_response.app_actions);
        settings_actions.extend(settings_response.settings_actions);
        AppWidgetResponse {
            app_actions: actions,
            settings_actions,
        }
    }

    fn open_add_track_dialog(
        &mut self,
        main_track_count: usize,
        settings_state: &SettingsViewState,
    ) {
        self.add_track_name = format!("Track {}", main_track_count + 1);
        self.add_track_audio_channels = settings_state
            .active
            .get(DEFAULT_NEW_TRACK_AUDIO_CHANNELS)
            .expect("registered audio-channel setting must retain its type");
        self.add_track_midi = settings_state
            .active
            .get(DEFAULT_NEW_TRACK_MIDI)
            .expect("registered MIDI setting must retain its type");
        self.add_track_open = true;
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_settings_test_open_add_track(
        &mut self,
        settings_state: &SettingsViewState,
    ) -> (u32, bool) {
        self.open_add_track_dialog(0, settings_state);
        (self.add_track_audio_channels, self.add_track_midi)
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_settings_test_open_scripts(
        &mut self,
        settings_state: &SettingsViewState,
    ) -> bool {
        self.settings
            .browser_test_open_category(settings_state, "Scripts")
    }

    #[cfg(target_arch = "wasm32")]
    #[doc(hidden)]
    pub fn browser_test_open_global_connections(&mut self) {
        self.connections.open(ConnectionScope::AllTracks);
    }

    fn show_io_task_dialog(
        &mut self,
        context: &egui::Context,
        state: &AppState,
        actions: &mut Vec<AppAction>,
    ) {
        let Some(task) = &state.io_task else {
            return;
        };
        if matches!(
            task.status,
            crate::IoTaskStatus::Completed | crate::IoTaskStatus::Cancelled
        ) {
            return;
        }
        egui::Window::new("Session and loop I/O")
            .id(egui::Id::new(("io_task", task.id.raw())))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(&task.message);
                ui.add(egui::ProgressBar::new(task.progress).show_percentage());
                if let Some(mapping) = &task.audio_channel_mapping {
                    let draft = self
                        .io_channel_mappings
                        .entry(task.id)
                        .or_insert_with(|| mapping.default_mapping.clone());
                    if draft.len() != mapping.destination_channels.len() {
                        *draft = mapping.default_mapping.clone();
                    }
                    egui::Grid::new(("io_channel_mapping", task.id.raw()))
                        .num_columns(2)
                        .show(ui, |ui| {
                            for (index, destination) in
                                mapping.destination_channels.iter().enumerate()
                            {
                                ui.label(destination);
                                let selected = draft
                                    .get(index)
                                    .and_then(|source| {
                                        mapping.source_channels.get(*source as usize)
                                    })
                                    .cloned()
                                    .unwrap_or_else(|| "Unavailable".to_owned());
                                egui::ComboBox::from_id_salt((
                                    "io_channel_source",
                                    task.id.raw(),
                                    index,
                                ))
                                .selected_text(selected)
                                .show_ui(ui, |ui| {
                                    for (source, label) in
                                        mapping.source_channels.iter().enumerate()
                                    {
                                        ui.selectable_value(
                                            &mut draft[index],
                                            source as u32,
                                            label,
                                        );
                                    }
                                });
                                ui.end_row();
                            }
                        });
                    ui.horizontal(|ui| {
                        if ui.button("Import with this mapping").clicked() {
                            actions.push(AppAction::ConfirmAudioChannelMapping {
                                task_id: task.id,
                                source_for_destination: draft.clone(),
                            });
                        }
                        if ui.button("Cancel").clicked() {
                            actions.push(AppAction::CancelIoTask { task_id: task.id });
                        }
                    });
                } else if let Some(selection) = &task.audio_channel_selection {
                    let draft = self
                        .io_channel_selections
                        .entry(task.id)
                        .or_insert_with(|| selection.default_selection.clone());
                    draft
                        .retain(|channel| (*channel as usize) < selection.available_channels.len());
                    let mut remove = None;
                    let mut move_up = None;
                    let mut move_down = None;
                    for index in 0..draft.len() {
                        ui.horizontal(|ui| {
                            ui.label(format!("Output {}", index + 1));
                            let selected = selection
                                .available_channels
                                .get(draft[index] as usize)
                                .cloned()
                                .unwrap_or_else(|| "Unavailable".to_owned());
                            egui::ComboBox::from_id_salt((
                                "io_export_channel",
                                task.id.raw(),
                                index,
                            ))
                            .selected_text(selected)
                            .show_ui(ui, |ui| {
                                for (channel, label) in
                                    selection.available_channels.iter().enumerate()
                                {
                                    if !draft.iter().enumerate().any(|(other, value)| {
                                        other != index && *value == channel as u32
                                    }) {
                                        ui.selectable_value(
                                            &mut draft[index],
                                            channel as u32,
                                            label,
                                        );
                                    }
                                }
                            });
                            if ui.small_button("↑").clicked() && index > 0 {
                                move_up = Some(index);
                            }
                            if ui.small_button("↓").clicked() && index + 1 < draft.len() {
                                move_down = Some(index);
                            }
                            if ui.small_button("Remove").clicked() {
                                remove = Some(index);
                            }
                        });
                    }
                    if let Some(index) = remove {
                        draft.remove(index);
                    } else if let Some(index) = move_up {
                        draft.swap(index, index - 1);
                    } else if let Some(index) = move_down {
                        draft.swap(index, index + 1);
                    }
                    if let Some(channel) = (0..selection.available_channels.len() as u32)
                        .find(|channel| !draft.contains(channel))
                    {
                        if ui.button("Add channel").clicked() {
                            draft.push(channel);
                        }
                    }
                    ui.horizontal(|ui| {
                        let confirm =
                            ui.add_enabled(!draft.is_empty(), egui::Button::new("Export channels"));
                        if confirm.clicked() {
                            actions.push(audio_channel_selection_action(task.id, draft));
                        }
                        if ui.button("Cancel").clicked() {
                            actions.push(AppAction::CancelIoTask { task_id: task.id });
                        }
                    });
                } else if let Some(warning) = &task.sample_rate_warning {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!(
                            "This will resample {} from {} Hz to {} Hz.",
                            warning.affected_media, warning.source_rate, warning.target_rate
                        ),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Resample and load").clicked() {
                            actions.push(AppAction::ConfirmSampleRateConversion {
                                task_id: task.id,
                                accept: true,
                            });
                        }
                        if ui.button("Cancel").clicked() {
                            actions.push(AppAction::ConfirmSampleRateConversion {
                                task_id: task.id,
                                accept: false,
                            });
                        }
                    });
                } else if task.status == crate::IoTaskStatus::Running
                    && ui.button("Cancel").clicked()
                {
                    actions.push(AppAction::CancelIoTask { task_id: task.id });
                }
                if task.status == crate::IoTaskStatus::Failed {
                    ui.colored_label(
                        egui::Color32::RED,
                        "The operation failed; the prior session is unchanged.",
                    );
                }
            });
    }

    fn show_add_track_dialog(&mut self, context: &egui::Context, actions: &mut Vec<AppAction>) {
        if !self.add_track_open {
            return;
        }
        let mut open = self.add_track_open;
        let mut accepted = false;
        let mut cancelled = false;
        egui::Window::new("Add track")
            .id(egui::Id::new("add_track_dialog"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label("Choose the settings for your direct track.");
                egui::Grid::new("add_track_fields")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.add_track_name);
                        ui.end_row();
                        ui.label("Audio:");
                        egui::ComboBox::from_id_salt("add_track_audio")
                            .selected_text(audio_channel_label(self.add_track_audio_channels))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.add_track_audio_channels,
                                    0,
                                    "Disabled",
                                );
                                ui.selectable_value(&mut self.add_track_audio_channels, 1, "Mono");
                                ui.selectable_value(
                                    &mut self.add_track_audio_channels,
                                    2,
                                    "Stereo",
                                );
                                for channels in 3..=10 {
                                    ui.selectable_value(
                                        &mut self.add_track_audio_channels,
                                        channels,
                                        format!("Custom ({channels})"),
                                    );
                                }
                            });
                        ui.end_row();
                        ui.label("Custom channels:");
                        ui.add(
                            egui::DragValue::new(&mut self.add_track_audio_channels)
                                .range(0..=u32::MAX)
                                .speed(1),
                        );
                        ui.end_row();
                        ui.label("MIDI:");
                        ui.checkbox(&mut self.add_track_midi, "Enabled");
                        ui.end_row();
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    let valid = !self.add_track_name.trim().is_empty();
                    let add = ui.add_enabled(valid, egui::Button::new("Add"));
                    #[cfg(test)]
                    {
                        self.add_track_accept_rect = Some(add.rect);
                    }
                    if add.clicked() {
                        accepted = true;
                    }
                    let cancel = ui.button("Cancel");
                    #[cfg(test)]
                    {
                        self.add_track_cancel_rect = Some(cancel.rect);
                    }
                    if cancel.clicked() {
                        cancelled = true;
                    }
                });
            });
        if accepted {
            if let Some(action) = self.accept_add_track() {
                actions.push(action);
            }
            open = false;
        }
        if cancelled {
            self.cancel_add_track();
            open = false;
        }
        self.add_track_open = open;
    }

    fn accept_add_track(&mut self) -> Option<AppAction> {
        let name = self.add_track_name.trim();
        if name.is_empty() {
            return None;
        }
        self.add_track_open = false;
        Some(AppAction::AddTrack(DirectTrackSpec {
            name: name.to_owned(),
            audio_channels: self.add_track_audio_channels,
            midi: self.add_track_midi,
        }))
    }

    fn cancel_add_track(&mut self) {
        self.add_track_open = false;
    }

    fn ensure_logo(&mut self, context: &egui::Context) {
        if self.logo.is_some() {
            return;
        }
        let Ok(image) = image::load_from_memory(LOGO_BYTES) else {
            return;
        };
        let rgba = image.into_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        self.logo = Some(context.load_texture("shoopdaloop-logo", color_image, Default::default()));
    }

    fn show_logo_and_status(&self, ui: &mut egui::Ui, state: &AppState) {
        ui.vertical_centered(|ui| {
            if let Some(logo) = &self.logo {
                let size = logo.size_vec2();
                let width = ui.available_width().min(145.0);
                let height = width * size.y / size.x;
                ui.add(egui::Image::new((logo.id(), egui::vec2(width, height))));
            } else {
                ui.heading("ShoopDaLoop");
            }
            if !state.status.version.is_empty() {
                ui.label(format!("ShoopDaLoop v{}", state.status.version));
            }
        });

        ui.add_space(12.0);
        ui.label("DSP");
        ui.add(
            egui::ProgressBar::new((state.status.dsp_load_percent / 100.0).clamp(0.0, 1.0))
                .text(format!("{:.1}%", state.status.dsp_load_percent)),
        );
        ui.label(format!("xruns: {}", state.status.xruns));
        ui.label(format!("audio: {:?}", state.status.audio_driver));
        if state.status.callback_count > 0 {
            ui.label(format!("callbacks: {}", state.status.callback_count));
            ui.label(format!(
                "I/O peak: {:.3} / {:.3}",
                state.status.input_peak, state.status.output_peak
            ));
        }
        if state.status.callback_budget_overruns > 0
            || state.status.command_overflows > 0
            || state.status.storage_low_channels > 0
            || state.status.storage_exhaustions > 0
        {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!(
                    "audio limits: budget {} / queue {} / storage low {} / exhausted {}",
                    state.status.callback_budget_overruns,
                    state.status.command_overflows,
                    state.status.storage_low_channels,
                    state.status.storage_exhaustions
                ),
            );
        }
        ui.separator();
        ui.label(format!("latency: {} frames", state.status.buffer_size));
        match state.status.latency_ms() {
            Some(latency) => {
                ui.label(format!("{latency:.2} ms"));
            }
            None => {
                ui.label("-- ms");
            }
        }
    }
}

fn audio_channel_selection_action(task_id: crate::TaskId, channels: &[u32]) -> AppAction {
    AppAction::ConfirmAudioChannelSelection {
        task_id,
        channels: channels.to_vec(),
    }
}

fn audio_channel_label(channels: u32) -> String {
    match channels {
        0 => "Disabled".to_owned(),
        1 => "Mono".to_owned(),
        2 => "Stereo".to_owned(),
        channels => format!("Custom ({channels})"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{LoopDetailsState, TrackState, WaveformChannelState};
    use shoop_settings::{
        SettingsDraft, SettingsPersistenceState, SettingsRegistryBuilder, SettingsViewState,
    };

    fn settings_state() -> SettingsViewState {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        let registry = builder.finish();
        SettingsViewState {
            active: Arc::new(registry.defaults(1)),
            diagnostics: Arc::from([]),
            storage_location: "fixture".to_owned(),
            recovery_required: false,
            persistence: SettingsPersistenceState::Idle,
        }
    }

    fn frame(
        context: &egui::Context,
        widget: &mut AppWidget,
        state: &AppState,
        events: Vec<egui::Event>,
    ) -> Vec<AppAction> {
        let mut actions = Vec::new();
        let settings = settings_state();
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| actions = widget.show(ui, state, &settings, None).app_actions,
        );
        actions
    }

    fn settings_frame(
        context: &egui::Context,
        widget: &mut AppWidget,
        state: &AppState,
        settings: &SettingsViewState,
        paths: &BTreeMap<crate::ScriptId, String>,
        events: Vec<egui::Event>,
    ) -> AppWidgetResponse {
        let mut response = None;
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| response = Some(widget.show(ui, state, settings, Some(paths))),
        );
        response.unwrap()
    }

    #[test]
    fn add_track_accept_emits_validated_spec() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = AppState::default();
        let mut widget = AppWidget::default();
        widget.add_track_open = true;
        widget.add_track_name = "New Track".to_owned();
        widget.add_track_audio_channels = 4;
        widget.add_track_midi = true;
        frame(&context, &mut widget, &state, Vec::new());
        assert!(widget.add_track_accept_rect.is_some());
        assert_eq!(
            widget.accept_add_track(),
            Some(AppAction::AddTrack(DirectTrackSpec {
                name: "New Track".to_owned(),
                audio_channels: 4,
                midi: true,
            }))
        );
        assert!(!widget.add_track_open);
    }

    #[test]
    fn cancelling_add_track_has_no_action() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = AppState::default();
        let mut widget = AppWidget::default();
        widget.add_track_open = true;
        widget.add_track_name = "Cancelled".to_owned();
        frame(&context, &mut widget, &state, Vec::new());
        assert!(widget.add_track_cancel_rect.is_some());
        widget.cancel_add_track();
        assert!(!widget.add_track_open);
    }

    #[test]
    fn ordered_audio_export_selection_emits_the_task_scoped_confirmation() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let task_id = crate::TaskId::from_raw(19);
        let state = AppState {
            io_task: Some(crate::IoTaskState {
                id: task_id,
                kind: crate::IoTaskKind::ExportLoopAudio,
                status: crate::IoTaskStatus::AwaitingChannelSelection,
                progress: 0.2,
                message: "Select channels".to_owned(),
                sample_rate_warning: None,
                audio_channel_mapping: None,
                audio_channel_selection: Some(crate::AudioChannelSelectionState {
                    available_channels: vec!["Direct 1".to_owned(), "Direct 2".to_owned()],
                    default_selection: vec![1, 0],
                }),
            }),
            ..Default::default()
        };
        let mut widget = AppWidget::default();
        let settings = settings_state();
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                ..Default::default()
            },
            |ui| {
                widget.show(ui, &state, &settings, None);
            },
        );
        assert!(!output.shapes.is_empty());
        assert_eq!(
            audio_channel_selection_action(task_id, &[1, 0]),
            AppAction::ConfirmAudioChannelSelection {
                task_id,
                channels: vec![1, 0],
            }
        );
    }

    #[test]
    fn scripts_tab_renders_lifecycle_errors_logs_and_midi_diagnostics() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let registry = Arc::new(builder.finish());
        let settings = SettingsViewState {
            active: Arc::new(registry.defaults(1)),
            diagnostics: Arc::from([]),
            storage_location: "fixture".to_owned(),
            recovery_required: false,
            persistence: SettingsPersistenceState::Idle,
        };
        let mut widget = AppWidget::new(registry);
        widget.settings.open(&settings);
        widget.settings.select_category("Scripts");
        let script_id = crate::ScriptId::from_raw(1);
        let state = AppState {
            scripting: crate::ScriptingState {
                supported: true,
                scripts: Arc::from([crate::ScriptState {
                    id: script_id,
                    name: "controller.lua".to_owned(),
                    kind: crate::ScriptKind::User,
                    enabled: true,
                    lifecycle: crate::ScriptLifecycle::Error,
                    documentation: Some("Controller help".to_owned()),
                    latest_error: Some("bad callback".to_owned()),
                    activity: crate::ScriptActivityDiagnostics {
                        loop_callbacks: 1,
                        global_callbacks: 2,
                        keyboard_callbacks: 3,
                        timers: 4,
                    },
                    midi: crate::ScriptMidiDiagnostics {
                        rules: 2,
                        connections: 1,
                        dropped_messages: 3,
                        errors: 4,
                        rule_states: Arc::from([crate::ScriptMidiRuleDiagnostics {
                            direction: crate::ScriptMidiRuleDirection::Output,
                            pattern: "APC Mini".to_owned(),
                            matched_endpoints: Arc::from(["APC Mini [sink]".to_owned()]),
                            connected_endpoints: Arc::from(["APC Mini [sink]".to_owned()]),
                            endpoints: Arc::from([crate::ScriptMidiEndpointDiagnostics {
                                id: "sink".to_owned(),
                                name: "APC Mini".to_owned(),
                                connected: true,
                            }]),
                            latest_error: Some("permission denied".to_owned()),
                        }]),
                    },
                    logs: Arc::from([crate::ScriptLogState {
                        level: crate::ScriptLogLevel::Warning,
                        message: "warning log".to_owned(),
                    }]),
                }]),
            }
            .into(),
            ..Default::default()
        };
        let paths = BTreeMap::from([(script_id, "/tmp/controller.lua".to_owned())]);
        settings_frame(&context, &mut widget, &state, &settings, &paths, Vec::new());
        assert!(widget.settings.is_open());

        assert!(widget.settings.restart_rect(script_id).is_some());
        assert!(widget.settings.reload_rect(script_id).is_some());
    }

    #[test]
    fn complete_application_state_produces_paint_commands() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut widget = AppWidget::default();
        let state = AppState {
            tracks: vec![TrackState {
                id: crate::TrackId::from_raw(1),
                name: "Track".to_owned(),
                ..Default::default()
            }],
            details: Some(LoopDetailsState {
                title: "Loop".to_owned(),
                channels: vec![WaveformChannelState {
                    id: crate::ChannelId::from_raw(1),
                    label: "audio".to_owned(),
                    samples: Arc::from([-0.5, 0.25, 0.75, -0.1]),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        let settings = settings_state();
        let mut uploaded_logo = false;
        for size in [egui::vec2(360.0, 200.0), egui::vec2(900.0, 600.0)] {
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    widget.show(ui, &state, &settings, None);
                },
            );

            assert!(output.shapes.len() > 10);
            uploaded_logo |= !output.textures_delta.set.is_empty();
        }
        assert!(uploaded_logo);
    }

    #[test]
    fn bundled_script_registry_excludes_native_user_path_workflow() {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_bundled_script_settings(&mut builder).unwrap();
        let registry = builder.finish();
        assert!(registry.definition(KEYBOARD_SCRIPT_ENABLED.id()).is_some());
        assert!(registry.definition(APC_MINI_SCRIPT_ENABLED.id()).is_some());
        assert!(registry.definition(USER_SCRIPTS.id()).is_none());
        let defaults = registry.defaults(1);
        assert!(defaults.get(KEYBOARD_SCRIPT_ENABLED).unwrap());
        assert!(!defaults.get(APC_MINI_SCRIPT_ENABLED).unwrap());
    }

    #[test]
    fn add_track_defaults_are_registered_and_read_only_when_a_new_draft_opens() {
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let initial = registry.defaults(4);
        let mut draft = SettingsDraft::from_snapshot(&initial);
        draft.set(DEFAULT_NEW_TRACK_AUDIO_CHANNELS, 6);
        draft.set(DEFAULT_NEW_TRACK_MIDI, true);
        let document = registry
            .document_from_draft(
                &shoop_settings::EgSettingsDocument::empty("test"),
                &draft,
                "test",
            )
            .unwrap();
        let state = SettingsViewState {
            active: Arc::new(registry.resolve(&document, 5).snapshot),
            diagnostics: Arc::from([]),
            storage_location: "fixture".to_owned(),
            recovery_required: false,
            persistence: SettingsPersistenceState::Saved,
        };
        let mut widget = AppWidget::default();
        widget.open_add_track_dialog(2, &state);
        assert_eq!(widget.add_track_name, "Track 3");
        assert_eq!(widget.add_track_audio_channels, 6);
        assert!(widget.add_track_midi);

        let replacement = registry.defaults(6);
        assert_eq!(
            replacement.get(DEFAULT_NEW_TRACK_AUDIO_CHANNELS).unwrap(),
            2
        );
        assert_eq!(widget.add_track_audio_channels, 6);
        assert!(widget.add_track_midi);
    }
}
