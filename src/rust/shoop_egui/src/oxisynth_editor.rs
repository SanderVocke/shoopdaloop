use crate::{
    OxiSynthControl, TrackAction, TrackProcessorDescriptor, TrackProcessorEditorDescriptor,
    TrackProcessorEditorState, TrackState,
};

#[derive(Debug, Default)]
pub(crate) struct OxiSynthEditor {
    filter: String,
    #[cfg(test)]
    window_rect: Option<egui::Rect>,
    #[cfg(test)]
    preset_rect: Option<egui::Rect>,
    #[cfg(test)]
    preset_item_rects: Vec<(String, egui::Rect)>,
    #[cfg(test)]
    panic_rect: Option<egui::Rect>,
}

impl OxiSynthEditor {
    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        state: &TrackState,
        processor: Option<&TrackProcessorDescriptor>,
    ) -> Vec<TrackAction> {
        let Some(fx) = &state.fx else {
            return Vec::new();
        };
        let Some(TrackProcessorEditorState::OxiSynth(editor)) = &fx.editor else {
            return Vec::new();
        };
        let Some(TrackProcessorEditorDescriptor::OxiSynth { presets }) =
            processor.and_then(|processor| processor.editor.as_ref())
        else {
            return Vec::new();
        };
        if !fx.visible {
            return Vec::new();
        }

        #[cfg(test)]
        self.preset_item_rects.clear();
        let mut actions = Vec::new();
        let mut open = true;
        let _shown = egui::Window::new(format!("{} — OxiSynth", state.name))
            .id(egui::Id::new(("oxisynth_editor", state.id)))
            .open(&mut open)
            .resizable(true)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Preset");
                    let selected = presets
                        .iter()
                        .find(|preset| preset.id == editor.selected_preset_id)
                        .map(preset_label)
                        .unwrap_or_else(|| editor.selected_preset_id.clone());
                    let _combo = egui::ComboBox::from_id_salt("preset")
                        .selected_text(selected)
                        .width(280.0)
                        .show_ui(ui, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.filter)
                                    .hint_text("Filter presets…"),
                            );
                            let filter = self.filter.trim().to_lowercase();
                            egui::ScrollArea::vertical()
                                .max_height(300.0)
                                .show(ui, |ui| {
                                    for preset in presets.iter().filter(|preset| {
                                        filter.is_empty()
                                            || preset.id.to_lowercase().contains(&filter)
                                            || preset.name.to_lowercase().contains(&filter)
                                    }) {
                                        let response = ui.selectable_label(
                                            preset.id == editor.selected_preset_id,
                                            preset_label(preset),
                                        );
                                        #[cfg(test)]
                                        self.preset_item_rects
                                            .push((preset.id.clone(), response.rect));
                                        if response.clicked() {
                                            actions.push(TrackAction::OxiSynth(
                                                OxiSynthControl::SelectPreset(preset.id.clone()),
                                            ));
                                        }
                                    }
                                });
                        });
                    #[cfg(test)]
                    {
                        self.preset_rect = Some(_combo.response.rect);
                    }
                    let panic = ui.button("Panic");
                    #[cfg(test)]
                    {
                        self.panic_rect = Some(panic.rect);
                    }
                    if panic.clicked() {
                        actions.push(TrackAction::OxiSynth(OxiSynthControl::Panic));
                    }
                });
            });
        #[cfg(test)]
        {
            self.window_rect = _shown.map(|response| response.response.rect);
        }
        if !open {
            actions.push(TrackAction::FxVisibilityChanged(false));
        }
        actions
    }
}

fn preset_label(preset: &crate::TrackProcessorPresetDescriptor) -> String {
    format!("{} — {}", preset.id, preset.name)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        FxLifecycle, OxiSynthState, TrackFxState, TrackId, TrackProcessorConstraints,
        TrackProcessorFeatures, TrackProcessorMidiPolicy, TrackProcessorPresetDescriptor,
        TrackProcessorTypeId,
    };

    fn fixture() -> (TrackState, TrackProcessorDescriptor) {
        let presets = Arc::from([
            TrackProcessorPresetDescriptor {
                id: "0:0".to_owned(),
                name: "Piano 1".to_owned(),
            },
            TrackProcessorPresetDescriptor {
                id: "0:40".to_owned(),
                name: "Violin".to_owned(),
            },
            TrackProcessorPresetDescriptor {
                id: "128:0".to_owned(),
                name: "Standard".to_owned(),
            },
        ]);
        let processor = TrackProcessorDescriptor {
            id: TrackProcessorTypeId::new(TrackProcessorTypeId::OXISYNTH),
            label: "OxiSynth".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: TrackProcessorConstraints {
                min_dry_audio_channels: Some(2),
                max_dry_audio_channels: Some(2),
                min_wet_audio_channels: Some(2),
                max_wet_audio_channels: Some(2),
                matching_audio_channels: false,
                midi: TrackProcessorMidiPolicy::Required,
            },
            features: TrackProcessorFeatures {
                state: true,
                embedded_ui: true,
                ..TrackProcessorFeatures::default()
            },
            editor: Some(TrackProcessorEditorDescriptor::OxiSynth { presets }),
        };
        let state = TrackState {
            id: TrackId::from_raw(42),
            name: "Instrument".to_owned(),
            fx: Some(TrackFxState {
                processor_type: processor.id.clone(),
                active: true,
                visible: true,
                lifecycle: FxLifecycle::Running,
                generation: 0,
                crash_summary: None,
                logs: Arc::from([]),
                editor: Some(TrackProcessorEditorState::OxiSynth(OxiSynthState {
                    selected_preset_id: "0:0".to_owned(),
                })),
            }),
            ..TrackState::default()
        };
        (state, processor)
    }

    fn frame(
        context: &egui::Context,
        editor: &mut OxiSynthEditor,
        state: &TrackState,
        processor: &TrackProcessorDescriptor,
        events: Vec<egui::Event>,
    ) -> Vec<TrackAction> {
        let mut actions = Vec::new();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 500.0),
                )),
                events,
                ..egui::RawInput::default()
            },
            |ui| actions = editor.show(ui.ctx(), state, Some(processor)),
        );
        output.textures_delta.clear();
        actions
    }

    fn click(
        context: &egui::Context,
        editor: &mut OxiSynthEditor,
        state: &TrackState,
        processor: &TrackProcessorDescriptor,
        position: egui::Pos2,
    ) -> Vec<TrackAction> {
        let mut actions = frame(
            context,
            editor,
            state,
            processor,
            vec![egui::Event::PointerMoved(position)],
        );
        for pressed in [true, false] {
            actions.extend(frame(
                context,
                editor,
                state,
                processor,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            ));
        }
        actions.dedup();
        actions
    }

    #[shoop_wasm_test_support::shoop_test]
    fn preset_panic_visibility_and_labels_are_typed() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let (state, processor) = fixture();
        let mut editor = OxiSynthEditor::default();
        assert!(frame(&context, &mut editor, &state, &processor, Vec::new()).is_empty());

        let panic = editor.panic_rect.unwrap().center();
        assert_eq!(
            click(&context, &mut editor, &state, &processor, panic),
            [TrackAction::OxiSynth(OxiSynthControl::Panic)]
        );

        let combo = editor.preset_rect.unwrap().center();
        assert!(click(&context, &mut editor, &state, &processor, combo).is_empty());
        frame(&context, &mut editor, &state, &processor, Vec::new());
        let violin = editor
            .preset_item_rects
            .iter()
            .find_map(|(id, rect)| (id == "0:40").then_some(rect.center()))
            .unwrap();
        assert_eq!(
            click(&context, &mut editor, &state, &processor, violin),
            [TrackAction::OxiSynth(OxiSynthControl::SelectPreset(
                "0:40".to_owned()
            ))]
        );
        assert_eq!(
            preset_label(&TrackProcessorPresetDescriptor {
                id: "128:0".to_owned(),
                name: "Standard".to_owned(),
            }),
            "128:0 — Standard"
        );

        let window = editor.window_rect.unwrap();
        let close = egui::pos2(window.right() - 12.0, window.top() + 12.0);
        assert_eq!(
            click(&context, &mut editor, &state, &processor, close),
            [TrackAction::FxVisibilityChanged(false)]
        );
        let mut hidden = state;
        hidden.fx.as_mut().unwrap().visible = false;
        assert!(frame(&context, &mut editor, &hidden, &processor, Vec::new()).is_empty());
    }
}
