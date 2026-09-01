use crate::{
    BuiltInFxControl, BuiltInFxState, TrackAction, TrackProcessorDescriptor,
    TrackProcessorEditorDescriptor, TrackProcessorEditorState, TrackState,
};

pub(crate) const FUNDSP_URL: &str = "https://github.com/SamiPerttu/fundsp";
pub(crate) const POWERED_BY_TEXT: &str = "Powered by FunDSP";

#[derive(Debug, Default)]
pub(crate) struct BuiltInFxEditor {
    #[cfg(test)]
    window_rect: Option<egui::Rect>,
    #[cfg(test)]
    reverb_rect: Option<egui::Rect>,
    #[cfg(test)]
    attribution_rect: Option<egui::Rect>,
}

impl BuiltInFxEditor {
    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        state: &TrackState,
        processor: Option<&TrackProcessorDescriptor>,
    ) -> Vec<TrackAction> {
        let Some(fx) = &state.fx else {
            return Vec::new();
        };
        let Some(TrackProcessorEditorState::BuiltInFx(BuiltInFxState { reverb_enabled, .. })) =
            &fx.editor
        else {
            return Vec::new();
        };
        if !matches!(
            processor.and_then(|processor| processor.editor.as_ref()),
            Some(TrackProcessorEditorDescriptor::BuiltInFx)
        ) || !fx.visible
        {
            return Vec::new();
        }

        let mut actions = Vec::new();
        let mut open = true;
        let _shown = egui::Window::new(format!("{} — Built-in FX", state.name))
            .id(egui::Id::new(("builtin_fx_editor", state.id)))
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                let _attribution = ui.hyperlink_to(POWERED_BY_TEXT, FUNDSP_URL);
                #[cfg(test)]
                {
                    self.attribution_rect = Some(_attribution.rect);
                }
                ui.separator();
                let mut enabled = *reverb_enabled;
                let response = ui.checkbox(&mut enabled, "Reverb");
                #[cfg(test)]
                {
                    self.reverb_rect = Some(response.rect);
                }
                if response.changed() {
                    actions.push(TrackAction::BuiltInFx(BuiltInFxControl::SetReverbEnabled(
                        enabled,
                    )));
                }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        FxLifecycle, TrackFxState, TrackId, TrackProcessorConstraints, TrackProcessorFeatures,
        TrackProcessorMidiPolicy, TrackProcessorTypeId,
    };

    fn fixture() -> (TrackState, TrackProcessorDescriptor) {
        let processor = TrackProcessorDescriptor {
            id: TrackProcessorTypeId::new(TrackProcessorTypeId::BUILTIN_FX),
            label: "Built-in FX".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: TrackProcessorConstraints {
                min_dry_audio_channels: Some(2),
                max_dry_audio_channels: Some(2),
                min_wet_audio_channels: Some(2),
                max_wet_audio_channels: Some(2),
                matching_audio_channels: true,
                midi: TrackProcessorMidiPolicy::Unsupported,
            },
            features: TrackProcessorFeatures {
                state: true,
                embedded_ui: true,
                ..TrackProcessorFeatures::default()
            },
            editor: Some(TrackProcessorEditorDescriptor::BuiltInFx),
        };
        let state = TrackState {
            id: TrackId::from_raw(42),
            name: "Effects".to_owned(),
            fx: Some(TrackFxState {
                processor_type: processor.id.clone(),
                active: true,
                visible: true,
                lifecycle: FxLifecycle::Running,
                generation: 0,
                crash_summary: None,
                logs: Arc::from([]),
                editor: Some(TrackProcessorEditorState::BuiltInFx(
                    BuiltInFxState::default(),
                )),
            }),
            ..TrackState::default()
        };
        (state, processor)
    }

    fn frame(
        context: &egui::Context,
        editor: &mut BuiltInFxEditor,
        state: &TrackState,
        processor: &TrackProcessorDescriptor,
        events: Vec<egui::Event>,
    ) -> (Vec<TrackAction>, Vec<egui::OutputCommand>) {
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
        (actions, output.platform_output.commands)
    }

    fn click(
        context: &egui::Context,
        editor: &mut BuiltInFxEditor,
        state: &TrackState,
        processor: &TrackProcessorDescriptor,
        position: egui::Pos2,
    ) -> (Vec<TrackAction>, Vec<egui::OutputCommand>) {
        let mut actions = Vec::new();
        let mut commands = Vec::new();
        let (moved_actions, moved_commands) = frame(
            context,
            editor,
            state,
            processor,
            vec![egui::Event::PointerMoved(position)],
        );
        actions.extend(moved_actions);
        commands.extend(moved_commands);
        for pressed in [true, false] {
            let (frame_actions, frame_commands) = frame(
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
            );
            actions.extend(frame_actions);
            commands.extend(frame_commands);
        }
        actions.dedup();
        (actions, commands)
    }

    #[shoop_wasm_test_support::shoop_test]
    fn reverb_toggle_and_fundsp_attribution_are_interactive() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let (state, processor) = fixture();
        let mut editor = BuiltInFxEditor::default();
        let (actions, _) = frame(&context, &mut editor, &state, &processor, Vec::new());
        assert!(actions.is_empty());
        assert!(editor.window_rect.is_some());

        let reverb = editor.reverb_rect.unwrap().center();
        let (actions, _) = click(&context, &mut editor, &state, &processor, reverb);
        assert_eq!(
            actions,
            [TrackAction::BuiltInFx(BuiltInFxControl::SetReverbEnabled(
                false
            ))]
        );

        let attribution = editor.attribution_rect.unwrap().center();
        let (_, commands) = click(&context, &mut editor, &state, &processor, attribution);
        assert!(commands.iter().any(|command| matches!(
            command,
            egui::OutputCommand::OpenUrl(url) if url.url == FUNDSP_URL
        )));
        assert_eq!(POWERED_BY_TEXT, "Powered by FunDSP");
    }

    #[shoop_wasm_test_support::shoop_test]
    fn hidden_and_other_processor_state_do_not_open_the_editor() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let (mut state, mut processor) = fixture();
        state.fx.as_mut().unwrap().visible = false;
        let mut editor = BuiltInFxEditor::default();
        assert!(frame(&context, &mut editor, &state, &processor, Vec::new())
            .0
            .is_empty());
        assert!(editor.window_rect.is_none());

        state.fx.as_mut().unwrap().visible = true;
        processor.editor = Some(TrackProcessorEditorDescriptor::OxiSynth {
            presets: Arc::from([]),
        });
        assert!(frame(&context, &mut editor, &state, &processor, Vec::new())
            .0
            .is_empty());
        assert!(editor.window_rect.is_none());
    }
}
