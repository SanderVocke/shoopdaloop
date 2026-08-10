use std::collections::BTreeMap;

use crate::{
    colors, AppIntent, AppState, ClickTrackKind, ClickTrackPreviewStatus, ClickTrackRequest,
    ClickTrackState, LoopId, LoopState,
};

#[derive(Debug)]
pub struct ClickTrackDialog {
    target: Option<LoopId>,
    drafts: BTreeMap<LoopId, ClickTrackRequest>,
    validation_message: Option<String>,
    preview_available: bool,
    #[cfg(test)]
    generate_rect: Option<egui::Rect>,
    #[cfg(test)]
    preview_rect: Option<egui::Rect>,
    #[cfg(test)]
    preview_enabled: bool,
}

impl Default for ClickTrackDialog {
    fn default() -> Self {
        Self {
            target: None,
            drafts: BTreeMap::new(),
            validation_message: None,
            preview_available: true,
            #[cfg(test)]
            generate_rect: None,
            #[cfg(test)]
            preview_rect: None,
            #[cfg(test)]
            preview_enabled: false,
        }
    }
}

impl ClickTrackDialog {
    pub fn set_preview_available(&mut self, available: bool) {
        self.preview_available = available;
    }

    pub fn open(&mut self, target: &LoopState, click_state: &ClickTrackState) {
        let draft = self.drafts.entry(target.id).or_default();
        reconcile_draft(draft, target, click_state);
        self.target = Some(target.id);
        self.validation_message = None;
    }

    pub fn show(&mut self, context: &egui::Context, state: &AppState) -> Vec<AppIntent> {
        let Some(target_id) = self.target else {
            return Vec::new();
        };
        let Some(target) = find_loop(state, target_id) else {
            self.target = None;
            self.validation_message = Some("The target loop is no longer available".to_owned());
            return Vec::new();
        };
        let draft = self.drafts.entry(target_id).or_default();
        reconcile_draft(draft, target, &state.click_track);
        let mut open = true;
        let mut cancel = false;
        let mut preview = false;
        let mut generate = false;
        let validation =
            validate_draft(draft, target, &state.click_track, state.status.sample_rate);

        egui::Window::new("Generate click track")
            .id(egui::Id::new(("click_track_dialog", target_id.raw())))
            .collapsible(false)
            .resizable(true)
            .default_width(390.0)
            .open(&mut open)
            .show(context, |ui| {
                ui.label(format!("Target: {}", target.name));
                ui.separator();
                egui::Grid::new(("click_track_fields", target_id.raw()))
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Kind:");
                        ui.horizontal(|ui| {
                            if target.has_audio {
                                ui.selectable_value(
                                    &mut draft.kind,
                                    ClickTrackKind::Audio,
                                    "Audio",
                                );
                            }
                            if target.has_midi {
                                ui.selectable_value(&mut draft.kind, ClickTrackKind::Midi, "MIDI");
                            }
                        });
                        ui.end_row();

                        if draft.kind == ClickTrackKind::Audio {
                            ui.label("Primary click:");
                            sound_combo(
                                ui,
                                ("primary_click", target_id.raw()),
                                &mut draft.primary_sound_id,
                                &state.click_track,
                            );
                            ui.end_row();

                            ui.label("Secondary click:");
                            secondary_sound_combo(
                                ui,
                                ("secondary_click", target_id.raw()),
                                &mut draft.secondary_sound_id,
                                &state.click_track,
                            );
                            ui.end_row();
                        } else {
                            ui.label("Click MIDI note:");
                            ui.add(egui::DragValue::new(&mut draft.midi_note).range(0..=127));
                            ui.end_row();

                            ui.label("Note length (s):");
                            ui.add(
                                egui::DragValue::new(&mut draft.midi_note_length_seconds)
                                    .range(0.0..=10.0)
                                    .speed(0.01)
                                    .max_decimals(3),
                            );
                            ui.end_row();
                        }

                        ui.label("Secondary clicks per primary:");
                        ui.add(
                            egui::DragValue::new(&mut draft.secondary_clicks_per_primary)
                                .range(0..=state.click_track.max_click_count.saturating_sub(1)),
                        );
                        ui.end_row();

                        ui.label("Clicks per minute:");
                        ui.add(
                            egui::DragValue::new(&mut draft.bpm)
                                .speed(0.1)
                                .max_decimals(3),
                        );
                        ui.end_row();

                        ui.label("Number of clicks:");
                        ui.add(
                            egui::DragValue::new(&mut draft.click_count)
                                .range(1..=state.click_track.max_click_count.max(1)),
                        );
                        ui.end_row();

                        ui.label("Delay odd clicks by (%):");
                        ui.add(
                            egui::DragValue::new(&mut draft.odd_click_delay_percent)
                                .range(0.0..=100.0)
                                .speed(0.1)
                                .max_decimals(2),
                        );
                        ui.end_row();
                    });

                let fill_reason = fill_disabled_reason(target, state.status.sample_rate, draft);
                let fill =
                    ui.add_enabled(fill_reason.is_none(), egui::Button::new("Fill loop length"));
                let fill = if let Some(reason) = fill_reason {
                    fill.on_hover_text(reason)
                } else {
                    fill.on_hover_text(
                        "Fit the selected number of clicks to the current loop length",
                    )
                };
                if fill.clicked() {
                    draft.bpm = draft.click_count as f64 * 60.0 * state.status.sample_rate as f64
                        / target.length_frames as f64;
                }

                if let Err(message) = &validation {
                    ui.colored_label(colors::STRONG_ERROR, message);
                }
                if let Some(message) = &self.validation_message {
                    ui.colored_label(colors::STRONG_ERROR, message);
                }
                if state.click_track.preview_status != ClickTrackPreviewStatus::Idle
                    && !state.click_track.preview_message.is_empty()
                {
                    let color =
                        if state.click_track.preview_status == ClickTrackPreviewStatus::Failed {
                            colors::STRONG_ERROR
                        } else {
                            colors::FOREGROUND
                        };
                    ui.colored_label(color, &state.click_track.preview_message);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let preview_button = ui
                        .add_enabled(
                            validation.is_ok()
                                && draft.kind == ClickTrackKind::Audio
                                && self.preview_available,
                            egui::Button::new("Preview"),
                        )
                        .on_disabled_hover_text(if self.preview_available {
                            "Preview requires valid audio click settings"
                        } else {
                            "Audio preview is unavailable on this system"
                        });
                    #[cfg(test)]
                    {
                        self.preview_rect = Some(preview_button.rect);
                        self.preview_enabled = preview_button.enabled();
                    }
                    if preview_button.clicked() {
                        preview = true;
                    }
                    let generate_button =
                        ui.add_enabled(validation.is_ok(), egui::Button::new("Generate"));
                    #[cfg(test)]
                    {
                        self.generate_rect = Some(generate_button.rect);
                    }
                    if generate_button.clicked() {
                        generate = true;
                    }
                    let cancel_button = ui.button("Cancel");
                    if cancel_button.clicked() {
                        cancel = true;
                    }
                });
            });

        self.validation_message = validation.err();
        let mut intents = Vec::new();
        if preview {
            intents.push(preview_intent(target_id, draft));
        }
        if generate {
            intents.push(generate_intent(target_id, draft));
            open = false;
        }
        if cancel {
            open = false;
        }
        if !open {
            self.close();
        }
        intents
    }

    fn close(&mut self) {
        self.target = None;
    }

    #[cfg(test)]
    fn target(&self) -> Option<LoopId> {
        self.target
    }
}

fn preview_intent(loop_id: LoopId, request: &ClickTrackRequest) -> AppIntent {
    AppIntent::PreviewClickTrack {
        loop_id,
        request: request.clone(),
    }
}

fn generate_intent(loop_id: LoopId, request: &ClickTrackRequest) -> AppIntent {
    AppIntent::GenerateClickTrack {
        loop_id,
        request: request.clone(),
    }
}

fn find_loop(state: &AppState, id: LoopId) -> Option<&LoopState> {
    state
        .tracks
        .iter()
        .flat_map(|track| &track.loops)
        .find(|loop_| loop_.id == id)
}

fn reconcile_draft(draft: &mut ClickTrackRequest, target: &LoopState, state: &ClickTrackState) {
    if draft.kind == ClickTrackKind::Audio && !target.has_audio {
        draft.kind = ClickTrackKind::Midi;
    } else if draft.kind == ClickTrackKind::Midi && !target.has_midi {
        draft.kind = ClickTrackKind::Audio;
    }
    if !state
        .sounds
        .iter()
        .any(|sound| sound.id == draft.primary_sound_id)
    {
        draft.primary_sound_id = state
            .sounds
            .first()
            .map(|sound| sound.id.clone())
            .unwrap_or_default();
    }
    if draft
        .secondary_sound_id
        .as_ref()
        .is_some_and(|selected| !state.sounds.iter().any(|sound| sound.id == *selected))
    {
        draft.secondary_sound_id = None;
    }
}

fn validate_draft(
    draft: &ClickTrackRequest,
    target: &LoopState,
    state: &ClickTrackState,
    sample_rate: u32,
) -> Result<(), String> {
    if sample_rate == 0 {
        return Err("The active audio sample rate is unavailable".to_owned());
    }
    if !draft.bpm.is_finite() || draft.bpm <= 0.0 {
        return Err("Clicks per minute must be greater than zero".to_owned());
    }
    if draft.click_count == 0 || draft.click_count > state.max_click_count {
        return Err(format!(
            "Number of clicks must be in 1..={}",
            state.max_click_count
        ));
    }
    if !draft.odd_click_delay_percent.is_finite()
        || !(0.0..=100.0).contains(&draft.odd_click_delay_percent)
    {
        return Err("Odd-click delay must be between 0 and 100 percent".to_owned());
    }
    let frames = draft.click_count as f64 * 60.0 * sample_rate as f64 / draft.bpm;
    if !frames.is_finite() || frames < 1.0 || frames > state.max_output_frames as f64 {
        return Err(format!(
            "Generated duration must be in 1..={} frames",
            state.max_output_frames
        ));
    }
    match draft.kind {
        ClickTrackKind::Audio => {
            if !target.has_audio {
                return Err("The target loop has no audio channels".to_owned());
            }
            if !state
                .sounds
                .iter()
                .any(|sound| sound.id == draft.primary_sound_id)
            {
                return Err("Select an available primary click".to_owned());
            }
            if draft
                .secondary_sound_id
                .as_ref()
                .is_some_and(|selected| !state.sounds.iter().any(|sound| sound.id == *selected))
            {
                return Err("Select an available secondary click".to_owned());
            }
            if draft.secondary_clicks_per_primary >= state.max_click_count {
                return Err("Secondary click count is too large".to_owned());
            }
        }
        ClickTrackKind::Midi => {
            if !target.has_midi {
                return Err("The target loop has no MIDI channels".to_owned());
            }
            if !draft.midi_note_length_seconds.is_finite()
                || !(0.0..=10.0).contains(&draft.midi_note_length_seconds)
            {
                return Err("MIDI note length must be between 0 and 10 seconds".to_owned());
            }
        }
    }
    Ok(())
}

fn fill_disabled_reason(
    target: &LoopState,
    sample_rate: u32,
    draft: &ClickTrackRequest,
) -> Option<&'static str> {
    if target.length_frames == 0 {
        Some("The current loop has no length")
    } else if sample_rate == 0 {
        Some("The active sample rate is unavailable")
    } else if draft.click_count == 0 {
        Some("Choose at least one click")
    } else {
        None
    }
}

fn sound_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    selected: &mut String,
    state: &ClickTrackState,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected.as_str())
        .show_ui(ui, |ui| {
            for sound in state.sounds.iter() {
                ui.selectable_value(selected, sound.id.clone(), &sound.name);
            }
        });
}

fn secondary_sound_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    selected: &mut Option<String>,
    state: &ClickTrackState,
) {
    let label = selected.as_deref().unwrap_or("None");
    egui::ComboBox::from_id_salt(id)
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(selected, None, "None");
            for sound in state.sounds.iter() {
                ui.selectable_value(selected, Some(sound.id.clone()), &sound.name);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClickSoundDescriptor, StatusState, TrackState};
    use std::sync::Arc;

    fn state() -> AppState {
        let loop_id = LoopId::from_raw(42);
        AppState {
            status: StatusState {
                sample_rate: 48_000,
                ..Default::default()
            },
            tracks: vec![TrackState {
                loops: vec![LoopState {
                    id: loop_id,
                    name: "Target".to_owned(),
                    length_frames: 96_000,
                    has_audio: true,
                    has_midi: true,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            click_track: ClickTrackState {
                sounds: Arc::from([
                    ClickSoundDescriptor {
                        id: "click_high".to_owned(),
                        name: "click_high".to_owned(),
                    },
                    ClickSoundDescriptor {
                        id: "click_low".to_owned(),
                        name: "click_low".to_owned(),
                    },
                ]),
                max_click_count: 4_096,
                max_output_frames: 10_000_000,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn defaults_fill_and_capability_reconciliation_are_stable_by_loop_id() {
        let state = state();
        let target = &state.tracks[0].loops[0];
        let mut dialog = ClickTrackDialog::default();
        dialog.open(target, &state.click_track);
        let draft = dialog.drafts.get_mut(&target.id).unwrap();
        assert_eq!(*draft, ClickTrackRequest::default());
        assert_eq!(
            fill_disabled_reason(target, state.status.sample_rate, draft),
            None
        );
        draft.bpm = draft.click_count as f64 * 60.0 * state.status.sample_rate as f64
            / target.length_frames as f64;
        assert_eq!(draft.bpm, 120.0);
        draft.kind = ClickTrackKind::Midi;
        let audio_only = LoopState {
            has_audio: true,
            has_midi: false,
            ..target.clone()
        };
        dialog.open(&audio_only, &state.click_track);
        assert_eq!(dialog.drafts[&target.id].kind, ClickTrackKind::Audio);
    }

    #[test]
    fn validation_covers_fractional_bpm_limits_and_kind_specific_state() {
        let state = state();
        let target = &state.tracks[0].loops[0];
        let mut draft = ClickTrackRequest::default();
        draft.bpm = 100.5;
        assert!(validate_draft(&draft, target, &state.click_track, 48_000).is_ok());
        draft.bpm = 0.0;
        assert!(validate_draft(&draft, target, &state.click_track, 48_000).is_err());
        draft.bpm = 100.0;
        draft.click_count = 4_097;
        assert!(validate_draft(&draft, target, &state.click_track, 48_000).is_err());
        draft.click_count = 4;
        draft.kind = ClickTrackKind::Midi;
        draft.midi_note_length_seconds = 10.1;
        assert!(validate_draft(&draft, target, &state.click_track, 48_000).is_err());
    }

    #[test]
    fn cancel_closes_without_discarding_the_stable_loop_draft() {
        let state = state();
        let target = &state.tracks[0].loops[0];
        let mut dialog = ClickTrackDialog::default();
        dialog.open(target, &state.click_track);
        dialog.drafts.get_mut(&target.id).unwrap().bpm = 123.5;
        dialog.close();
        assert_eq!(dialog.target(), None);
        dialog.open(target, &state.click_track);
        assert_eq!(dialog.drafts[&target.id].bpm, 123.5);
    }

    #[test]
    fn preview_and_generate_actions_preserve_the_exact_stable_target_and_draft() {
        let state = state();
        let target = &state.tracks[0].loops[0];
        let request = ClickTrackRequest::default();
        assert_eq!(
            preview_intent(target.id, &request),
            AppIntent::PreviewClickTrack {
                loop_id: target.id,
                request: request.clone(),
            }
        );
        assert_eq!(
            generate_intent(target.id, &request),
            AppIntent::GenerateClickTrack {
                loop_id: target.id,
                request,
            }
        );
    }

    #[test]
    fn dialog_paints_at_minimum_and_common_sizes_and_drops_stale_target() {
        for size in [egui::vec2(360.0, 200.0), egui::vec2(900.0, 600.0)] {
            let context = egui::Context::default();
            crate::initialize(&context);
            let state = state();
            let mut dialog = ClickTrackDialog::default();
            dialog.open(&state.tracks[0].loops[0], &state.click_track);
            let _ = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |_ui| {
                    let intents = dialog.show(&context, &state);
                    assert!(intents.is_empty());
                },
            );
            assert!(dialog.generate_rect.is_some());
            assert!(dialog.preview_rect.is_some());
            let stale = AppState {
                status: state.status.clone(),
                click_track: state.click_track.clone(),
                ..Default::default()
            };
            assert!(dialog.show(&context, &stale).is_empty());
            assert_eq!(dialog.target(), None);
        }
    }

    #[test]
    fn preview_is_disabled_when_the_platform_has_no_audio_path() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = state();
        let mut dialog = ClickTrackDialog::default();
        dialog.set_preview_available(false);
        dialog.open(&state.tracks[0].loops[0], &state.click_track);
        let _ = context.run_ui(Default::default(), |_ui| {
            assert!(dialog.show(&context, &state).is_empty());
        });
        assert!(!dialog.preview_enabled);
    }
}
