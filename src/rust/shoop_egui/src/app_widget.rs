use std::collections::BTreeMap;

use crate::{
    AppAction, AppState, ConnectionDialog, ConnectionScope, DetailsPane, DirectTrackSpec,
    GlobalControls, TrackWidget, TracksWidget,
};

const LOGO_BYTES: &[u8] = include_bytes!("../../../../resources/logo-small.png");

pub struct AppWidget {
    tracks: TracksWidget,
    global_controls: GlobalControls,
    details: DetailsPane,
    sync_track: TrackWidget,
    connections: ConnectionDialog,
    details_open: bool,
    add_track_open: bool,
    add_track_name: String,
    add_track_audio_channels: u32,
    add_track_midi: bool,
    logo: Option<egui::TextureHandle>,
    io_channel_mappings: BTreeMap<crate::TaskId, Vec<u32>>,
    #[cfg(test)]
    add_track_accept_rect: Option<egui::Rect>,
    #[cfg(test)]
    add_track_cancel_rect: Option<egui::Rect>,
}

impl Default for AppWidget {
    fn default() -> Self {
        Self {
            tracks: TracksWidget::default(),
            global_controls: GlobalControls::default(),
            details: DetailsPane::default(),
            sync_track: TrackWidget::default(),
            connections: ConnectionDialog::default(),
            details_open: true,
            add_track_open: false,
            add_track_name: String::new(),
            add_track_audio_channels: 2,
            add_track_midi: false,
            logo: None,
            io_channel_mappings: BTreeMap::new(),
            #[cfg(test)]
            add_track_accept_rect: None,
            #[cfg(test)]
            add_track_cancel_rect: None,
        }
    }
}

impl AppWidget {
    pub fn open_connections(&mut self, scope: ConnectionScope) {
        self.connections.open(scope);
    }

    pub fn open_connection_scope(&self) -> Option<ConnectionScope> {
        self.connections.is_open().then(|| self.connections.scope())
    }

    pub fn show(&mut self, ui: &mut egui::Ui, state: &AppState) -> Vec<AppAction> {
        self.ensure_logo(ui.ctx());
        let mut actions = Vec::new();

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
                    self.add_track_name = format!("Track {}", main_tracks.len() + 1);
                    self.add_track_audio_channels = 2;
                    self.add_track_midi = false;
                    self.add_track_open = true;
                }
                if let Some(track_id) = response.connection_track_requested {
                    self.connections.open(ConnectionScope::Track(track_id));
                }
                actions.extend(response.intents);
            });

        self.show_add_track_dialog(ui.ctx(), &mut actions);
        self.show_io_task_dialog(ui.ctx(), state, &mut actions);
        actions.extend(self.connections.show(ui.ctx(), state));
        actions
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

    fn frame(
        context: &egui::Context,
        widget: &mut AppWidget,
        state: &AppState,
        events: Vec<egui::Event>,
    ) -> Vec<AppAction> {
        let mut actions = Vec::new();
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| actions = widget.show(ui, state),
        );
        actions
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

        let mut uploaded_logo = false;
        for size in [egui::vec2(360.0, 200.0), egui::vec2(900.0, 600.0)] {
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    widget.show(ui, &state);
                },
            );

            assert!(output.shapes.len() > 10);
            uploaded_logo |= !output.textures_delta.set.is_empty();
        }
        assert!(uploaded_logo);
    }
}
