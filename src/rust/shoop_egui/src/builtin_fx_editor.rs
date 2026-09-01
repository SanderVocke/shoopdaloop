use crate::{
    BuiltInFxControl, BuiltInFxDriveType, BuiltInFxMidiCcAssignment, BuiltInFxModulationType,
    BuiltInFxParameter, BuiltInFxReverbType, BuiltInFxStage, BuiltInFxState, TrackAction,
    TrackProcessorDescriptor, TrackProcessorEditorDescriptor, TrackProcessorEditorState,
    TrackState,
};

pub(crate) const FUNDSP_URL: &str = "https://github.com/SamiPerttu/fundsp";
pub(crate) const POWERED_BY_TEXT: &str = "Powered by FunDSP";

#[derive(Debug)]
pub(crate) struct BuiltInFxEditor {
    midi_learn_open: bool,
    selected_midi_parameter: BuiltInFxParameter,
    #[cfg(test)]
    window_rect: Option<egui::Rect>,
    #[cfg(test)]
    stage_rects: Vec<(BuiltInFxStage, egui::Rect)>,
    #[cfg(test)]
    parameter_rects: Vec<(BuiltInFxParameter, egui::Rect)>,
    #[cfg(test)]
    midi_learn_rect: Option<egui::Rect>,
    #[cfg(test)]
    midi_assign_rect: Option<egui::Rect>,
    #[cfg(test)]
    midi_remove_rects: Vec<(BuiltInFxParameter, egui::Rect)>,
    #[cfg(test)]
    midi_remove_all_rect: Option<egui::Rect>,
    #[cfg(test)]
    attribution_rect: Option<egui::Rect>,
}

impl Default for BuiltInFxEditor {
    fn default() -> Self {
        Self {
            midi_learn_open: false,
            selected_midi_parameter: BuiltInFxParameter::CompressorThreshold,
            #[cfg(test)]
            window_rect: None,
            #[cfg(test)]
            stage_rects: Vec::new(),
            #[cfg(test)]
            parameter_rects: Vec::new(),
            #[cfg(test)]
            midi_learn_rect: None,
            #[cfg(test)]
            midi_assign_rect: None,
            #[cfg(test)]
            midi_remove_rects: Vec::new(),
            #[cfg(test)]
            midi_remove_all_rect: None,
            #[cfg(test)]
            attribution_rect: None,
        }
    }
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
        let Some(TrackProcessorEditorState::BuiltInFx(editor)) = &fx.editor else {
            return Vec::new();
        };
        if !matches!(
            processor.and_then(|processor| processor.editor.as_ref()),
            Some(TrackProcessorEditorDescriptor::BuiltInFx)
        ) || !fx.visible
        {
            return Vec::new();
        }

        #[cfg(test)]
        {
            self.stage_rects.clear();
            self.parameter_rects.clear();
            self.midi_assign_rect = None;
            self.midi_remove_rects.clear();
            self.midi_remove_all_rect = None;
        }
        let mut actions = Vec::new();
        let mut open = true;
        let _shown = egui::Window::new(format!("{} — Built-in FX", state.name))
            .id(egui::Id::new(("builtin_fx_editor", state.id)))
            .open(&mut open)
            .resizable(true)
            .default_width(430.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    let _attribution = ui.hyperlink_to(POWERED_BY_TEXT, FUNDSP_URL);
                    #[cfg(test)]
                    {
                        self.attribution_rect = Some(_attribution.rect);
                    }
                    let midi_learn = ui.button("MIDI Learn…");
                    #[cfg(test)]
                    {
                        self.midi_learn_rect = Some(midi_learn.rect);
                    }
                    if midi_learn.clicked() {
                        self.midi_learn_open = true;
                    }
                });
                ui.separator();

                macro_rules! stage_checkbox {
                    ($ui:expr, $stage:expr, $label:expr, $enabled:expr $(,)?) => {{
                        let mut value = $enabled;
                        let response = $ui.checkbox(&mut value, $label);
                        #[cfg(test)]
                        self.stage_rects.push(($stage, response.rect));
                        if response.changed() {
                            actions.push(TrackAction::BuiltInFx(
                                BuiltInFxControl::SetStageEnabled($stage, value),
                            ));
                        }
                    }};
                }
                macro_rules! parameter_slider {
                    ($ui:expr, $parameter:expr, $enabled:expr $(,)?) => {{
                        let mut value = parameter_value(editor, $parameter);
                        let (minimum, maximum) = $parameter.range();
                        let slider = egui::Slider::new(&mut value, minimum..=maximum)
                            .text($parameter.label())
                            .logarithmic(matches!(
                                $parameter,
                                BuiltInFxParameter::CompressorAttack
                                    | BuiltInFxParameter::CompressorRelease
                                    | BuiltInFxParameter::ChorusRate
                                    | BuiltInFxParameter::ModulationRate
                            ));
                        let response = $ui.add_enabled($enabled, slider);
                        #[cfg(test)]
                        self.parameter_rects.push(($parameter, response.rect));
                        if response.changed() {
                            actions.push(TrackAction::BuiltInFx(BuiltInFxControl::SetParameter(
                                $parameter, value,
                            )));
                        }
                    }};
                }

                ui.group(|ui| {
                    stage_checkbox!(
                        ui,
                        BuiltInFxStage::Compressor,
                        "Compressor",
                        editor.compressor_enabled,
                    );
                    parameter_slider!(ui, BuiltInFxParameter::CompressorThreshold, true);
                    parameter_slider!(ui, BuiltInFxParameter::CompressorRatio, true);
                    parameter_slider!(ui, BuiltInFxParameter::CompressorAttack, true);
                    parameter_slider!(ui, BuiltInFxParameter::CompressorRelease, true);
                    parameter_slider!(ui, BuiltInFxParameter::CompressorMakeup, true);
                });
                ui.group(|ui| {
                    stage_checkbox!(ui, BuiltInFxStage::Drive, "Drive", editor.drive_enabled);
                    let mut drive_type = editor.drive_type;
                    egui::ComboBox::from_id_salt("builtin_fx_drive_type")
                        .selected_text(drive_type_label(drive_type))
                        .show_ui(ui, |ui| {
                            for candidate in [
                                BuiltInFxDriveType::Saturation,
                                BuiltInFxDriveType::Overdrive,
                                BuiltInFxDriveType::Distortion,
                                BuiltInFxDriveType::Fuzz,
                            ] {
                                ui.selectable_value(
                                    &mut drive_type,
                                    candidate,
                                    drive_type_label(candidate),
                                );
                            }
                        });
                    if let Some(control) = drive_type_control(editor.drive_type, drive_type) {
                        actions.push(TrackAction::BuiltInFx(control));
                    }
                    parameter_slider!(ui, BuiltInFxParameter::Drive, true);
                    parameter_slider!(ui, BuiltInFxParameter::DriveTone, true);
                    parameter_slider!(ui, BuiltInFxParameter::DriveMix, true);
                    parameter_slider!(ui, BuiltInFxParameter::DriveOutput, true);
                });
                ui.group(|ui| {
                    stage_checkbox!(ui, BuiltInFxStage::Eq, "Three-band EQ", editor.eq_enabled);
                    parameter_slider!(ui, BuiltInFxParameter::EqLow, true);
                    parameter_slider!(ui, BuiltInFxParameter::EqMid, true);
                    parameter_slider!(ui, BuiltInFxParameter::EqHigh, true);
                });
                ui.group(|ui| {
                    stage_checkbox!(ui, BuiltInFxStage::Chorus, "Chorus", editor.chorus_enabled);
                    parameter_slider!(ui, BuiltInFxParameter::ChorusRate, true);
                    parameter_slider!(ui, BuiltInFxParameter::ChorusDepth, true);
                    parameter_slider!(ui, BuiltInFxParameter::ChorusMix, true);
                    parameter_slider!(ui, BuiltInFxParameter::ChorusWidth, true);
                });
                ui.group(|ui| {
                    stage_checkbox!(
                        ui,
                        BuiltInFxStage::Modulation,
                        "Modulation",
                        editor.modulation_enabled,
                    );
                    let mut modulation_type = editor.modulation_type;
                    egui::ComboBox::from_id_salt("builtin_fx_modulation_type")
                        .selected_text(modulation_type_label(modulation_type))
                        .show_ui(ui, |ui| {
                            for candidate in [
                                BuiltInFxModulationType::Tremolo,
                                BuiltInFxModulationType::Flanger,
                                BuiltInFxModulationType::Phaser,
                            ] {
                                ui.selectable_value(
                                    &mut modulation_type,
                                    candidate,
                                    modulation_type_label(candidate),
                                );
                            }
                        });
                    if let Some(control) =
                        modulation_type_control(editor.modulation_type, modulation_type)
                    {
                        actions.push(TrackAction::BuiltInFx(control));
                    }
                    parameter_slider!(ui, BuiltInFxParameter::ModulationRate, true);
                    parameter_slider!(ui, BuiltInFxParameter::ModulationDepth, true);
                    parameter_slider!(ui, BuiltInFxParameter::ModulationMix, true);
                    parameter_slider!(
                        ui,
                        BuiltInFxParameter::ModulationFeedback,
                        editor.modulation_type != BuiltInFxModulationType::Tremolo,
                    );
                    parameter_slider!(ui, BuiltInFxParameter::ModulationSpread, true);
                });
                ui.group(|ui| {
                    stage_checkbox!(ui, BuiltInFxStage::Reverb, "Reverb", editor.reverb_enabled);
                    let mut reverb_type = editor.reverb_type;
                    egui::ComboBox::from_id_salt("builtin_fx_reverb_type")
                        .selected_text(reverb_type_label(reverb_type))
                        .show_ui(ui, |ui| {
                            for candidate in [
                                BuiltInFxReverbType::Room,
                                BuiltInFxReverbType::Hall,
                                BuiltInFxReverbType::Plate,
                            ] {
                                ui.selectable_value(
                                    &mut reverb_type,
                                    candidate,
                                    reverb_type_label(candidate),
                                );
                            }
                        });
                    if let Some(control) = reverb_type_control(editor.reverb_type, reverb_type) {
                        actions.push(TrackAction::BuiltInFx(control));
                    }
                    parameter_slider!(ui, BuiltInFxParameter::ReverbAmount, true);
                    parameter_slider!(ui, BuiltInFxParameter::ReverbTone, true);
                });
            });

        if self.midi_learn_open {
            let latest_cc = state
                .controls
                .latest_input_midi_message
                .and_then(|message| message.midi_cc());
            let mut learn_open = self.midi_learn_open;
            let mut selected_parameter = self.selected_midi_parameter;
            egui::Window::new(format!("{} — MIDI Learn", state.name))
                .id(egui::Id::new(("builtin_fx_midi_learn", state.id)))
                .open(&mut learn_open)
                .resizable(true)
                .show(context, |ui| {
                    if let Some((channel, controller, value)) = latest_cc {
                        ui.label(format!(
                            "Channel {} · CC {} · Value {}",
                            channel + 1,
                            controller,
                            value
                        ));
                    } else {
                        ui.weak("No valid CC received");
                    }
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("builtin_fx_midi_cc_parameter")
                            .selected_text(selected_parameter.label())
                            .show_ui(ui, |ui| {
                                for parameter in BuiltInFxParameter::ALL {
                                    ui.selectable_value(
                                        &mut selected_parameter,
                                        parameter,
                                        parameter.label(),
                                    );
                                }
                            });
                        let assign =
                            ui.add_enabled(latest_cc.is_some(), egui::Button::new("Assign"));
                        #[cfg(test)]
                        {
                            self.midi_assign_rect = Some(assign.rect);
                        }
                        if assign.clicked() {
                            let (channel, controller, _) = latest_cc.expect("button is enabled");
                            actions.push(TrackAction::BuiltInFx(BuiltInFxControl::AssignMidiCc(
                                BuiltInFxMidiCcAssignment {
                                    parameter: selected_parameter,
                                    channel,
                                    controller,
                                },
                            )));
                        }
                    });
                    ui.separator();
                    ui.label("Assignments");
                    if editor.midi_cc_assignments.is_empty() {
                        ui.weak("None");
                    }
                    for assignment in editor.midi_cc_assignments.iter() {
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{} — Channel {} · CC {}",
                                assignment.parameter.label(),
                                assignment.channel + 1,
                                assignment.controller
                            ));
                            let remove = ui.button("Remove");
                            #[cfg(test)]
                            self.midi_remove_rects
                                .push((assignment.parameter, remove.rect));
                            if remove.clicked() {
                                actions.push(TrackAction::BuiltInFx(
                                    BuiltInFxControl::RemoveMidiCc(assignment.parameter),
                                ));
                            }
                        });
                    }
                    let remove_all = ui.add_enabled(
                        !editor.midi_cc_assignments.is_empty(),
                        egui::Button::new("Remove all"),
                    );
                    #[cfg(test)]
                    {
                        self.midi_remove_all_rect = Some(remove_all.rect);
                    }
                    if remove_all.clicked() {
                        actions.push(TrackAction::BuiltInFx(
                            BuiltInFxControl::ClearMidiCcAssignments,
                        ));
                    }
                });
            self.midi_learn_open = learn_open;
            self.selected_midi_parameter = selected_parameter;
        }

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

fn drive_type_control(
    current: BuiltInFxDriveType,
    selected: BuiltInFxDriveType,
) -> Option<BuiltInFxControl> {
    (current != selected).then_some(BuiltInFxControl::SetDriveType(selected))
}

fn modulation_type_control(
    current: BuiltInFxModulationType,
    selected: BuiltInFxModulationType,
) -> Option<BuiltInFxControl> {
    (current != selected).then_some(BuiltInFxControl::SetModulationType(selected))
}

fn reverb_type_control(
    current: BuiltInFxReverbType,
    selected: BuiltInFxReverbType,
) -> Option<BuiltInFxControl> {
    (current != selected).then_some(BuiltInFxControl::SetReverbType(selected))
}

fn parameter_value(state: &BuiltInFxState, parameter: BuiltInFxParameter) -> f32 {
    match parameter {
        BuiltInFxParameter::CompressorThreshold => state.compressor_threshold_db,
        BuiltInFxParameter::CompressorRatio => state.compressor_ratio,
        BuiltInFxParameter::CompressorAttack => state.compressor_attack_ms,
        BuiltInFxParameter::CompressorRelease => state.compressor_release_ms,
        BuiltInFxParameter::CompressorMakeup => state.compressor_makeup_db,
        BuiltInFxParameter::Drive => state.drive_db,
        BuiltInFxParameter::DriveTone => state.drive_tone,
        BuiltInFxParameter::DriveMix => state.drive_mix,
        BuiltInFxParameter::DriveOutput => state.drive_output_db,
        BuiltInFxParameter::EqLow => state.eq_low_db,
        BuiltInFxParameter::EqMid => state.eq_mid_db,
        BuiltInFxParameter::EqHigh => state.eq_high_db,
        BuiltInFxParameter::ChorusRate => state.chorus_rate_hz,
        BuiltInFxParameter::ChorusDepth => state.chorus_depth,
        BuiltInFxParameter::ChorusMix => state.chorus_mix,
        BuiltInFxParameter::ChorusWidth => state.chorus_width,
        BuiltInFxParameter::ModulationRate => state.modulation_rate_hz,
        BuiltInFxParameter::ModulationDepth => state.modulation_depth,
        BuiltInFxParameter::ModulationMix => state.modulation_mix,
        BuiltInFxParameter::ModulationFeedback => state.modulation_feedback,
        BuiltInFxParameter::ModulationSpread => state.modulation_spread,
        BuiltInFxParameter::ReverbAmount => state.reverb_amount,
        BuiltInFxParameter::ReverbTone => state.reverb_tone,
    }
}

fn drive_type_label(value: BuiltInFxDriveType) -> &'static str {
    match value {
        BuiltInFxDriveType::Saturation => "Saturation",
        BuiltInFxDriveType::Overdrive => "Overdrive",
        BuiltInFxDriveType::Distortion => "Distortion",
        BuiltInFxDriveType::Fuzz => "Fuzz",
    }
}

fn modulation_type_label(value: BuiltInFxModulationType) -> &'static str {
    match value {
        BuiltInFxModulationType::Tremolo => "Tremolo",
        BuiltInFxModulationType::Flanger => "Flanger",
        BuiltInFxModulationType::Phaser => "Phaser",
    }
}

fn reverb_type_label(value: BuiltInFxReverbType) -> &'static str {
    match value {
        BuiltInFxReverbType::Room => "Room",
        BuiltInFxReverbType::Hall => "Hall",
        BuiltInFxReverbType::Plate => "Plate",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        FxLifecycle, LatestMidiMessage, TrackFxState, TrackId, TrackProcessorConstraints,
        TrackProcessorFeatures, TrackProcessorMidiPolicy, TrackProcessorTypeId,
    };

    fn fixture() -> (TrackState, TrackProcessorDescriptor) {
        let processor = TrackProcessorDescriptor {
            id: TrackProcessorTypeId::new(TrackProcessorTypeId::BUILTIN_FX),
            label: "Built-in FX".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: TrackProcessorConstraints {
                min_dry_audio_channels: Some(1),
                max_dry_audio_channels: None,
                min_wet_audio_channels: Some(1),
                max_wet_audio_channels: None,
                matching_audio_channels: true,
                midi: TrackProcessorMidiPolicy::Required,
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
            controls: crate::TrackControlState {
                latest_input_midi_message: LatestMidiMessage::new([0xb2, 17, 100, 0], 3),
                ..Default::default()
            },
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
                    egui::vec2(900.0, 1_200.0),
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
    fn rack_controls_and_fundsp_attribution_are_interactive() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let (state, processor) = fixture();
        let mut editor = BuiltInFxEditor::default();
        let (actions, _) = frame(&context, &mut editor, &state, &processor, Vec::new());
        assert!(actions.is_empty());
        assert!(editor.window_rect.is_some());
        assert_eq!(editor.stage_rects.len(), 6);
        assert_eq!(editor.parameter_rects.len(), BuiltInFxParameter::ALL.len());

        let reverb = editor
            .stage_rects
            .iter()
            .find(|(stage, _)| *stage == BuiltInFxStage::Reverb)
            .unwrap()
            .1
            .center();
        let (actions, _) = click(&context, &mut editor, &state, &processor, reverb);
        assert!(
            actions.contains(&TrackAction::BuiltInFx(BuiltInFxControl::SetStageEnabled(
                BuiltInFxStage::Reverb,
                false
            )))
        );

        assert_eq!(
            editor
                .parameter_rects
                .iter()
                .map(|(parameter, _)| *parameter)
                .collect::<Vec<_>>(),
            BuiltInFxParameter::ALL
        );

        let attribution = editor.attribution_rect.unwrap().center();
        let (_, commands) = click(&context, &mut editor, &state, &processor, attribution);
        assert!(commands.iter().any(|command| matches!(
            command,
            egui::OutputCommand::OpenUrl(url) if url.url == FUNDSP_URL
        )));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn selector_controls_emit_only_real_typed_changes() {
        assert_eq!(
            drive_type_control(BuiltInFxDriveType::Saturation, BuiltInFxDriveType::Fuzz),
            Some(BuiltInFxControl::SetDriveType(BuiltInFxDriveType::Fuzz))
        );
        assert_eq!(
            modulation_type_control(
                BuiltInFxModulationType::Tremolo,
                BuiltInFxModulationType::Phaser,
            ),
            Some(BuiltInFxControl::SetModulationType(
                BuiltInFxModulationType::Phaser
            ))
        );
        assert_eq!(
            reverb_type_control(BuiltInFxReverbType::Room, BuiltInFxReverbType::Plate),
            Some(BuiltInFxControl::SetReverbType(BuiltInFxReverbType::Plate))
        );
        assert_eq!(
            reverb_type_control(BuiltInFxReverbType::Room, BuiltInFxReverbType::Room),
            None
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn midi_learn_assigns_latest_cc_to_a_continuous_parameter() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let (state, processor) = fixture();
        let mut editor = BuiltInFxEditor::default();
        frame(&context, &mut editor, &state, &processor, Vec::new());
        let midi_learn = editor.midi_learn_rect.unwrap().center();
        click(&context, &mut editor, &state, &processor, midi_learn);
        frame(&context, &mut editor, &state, &processor, Vec::new());
        let assign = editor.midi_assign_rect.unwrap().center();
        let (actions, _) = click(&context, &mut editor, &state, &processor, assign);
        assert!(
            actions.contains(&TrackAction::BuiltInFx(BuiltInFxControl::AssignMidiCc(
                BuiltInFxMidiCcAssignment {
                    parameter: BuiltInFxParameter::CompressorThreshold,
                    channel: 2,
                    controller: 17,
                }
            )))
        );
        assert_eq!(BuiltInFxParameter::ALL.len(), 23);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn midi_learn_removes_one_or_all_assignments() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let (mut state, processor) = fixture();
        let Some(TrackProcessorEditorState::BuiltInFx(editor_state)) =
            state.fx.as_mut().and_then(|fx| fx.editor.as_mut())
        else {
            panic!("missing Built-in FX editor state")
        };
        editor_state.midi_cc_assignments = Arc::from([BuiltInFxMidiCcAssignment {
            parameter: BuiltInFxParameter::Drive,
            channel: 2,
            controller: 17,
        }]);
        let mut editor = BuiltInFxEditor {
            midi_learn_open: true,
            ..Default::default()
        };
        frame(&context, &mut editor, &state, &processor, Vec::new());
        let remove = editor.midi_remove_rects[0].1.center();
        let (actions, _) = click(&context, &mut editor, &state, &processor, remove);
        assert!(
            actions.contains(&TrackAction::BuiltInFx(BuiltInFxControl::RemoveMidiCc(
                BuiltInFxParameter::Drive
            )))
        );
        frame(&context, &mut editor, &state, &processor, Vec::new());
        let remove_all = editor.midi_remove_all_rect.unwrap().center();
        let (actions, _) = click(&context, &mut editor, &state, &processor, remove_all);
        assert!(actions.contains(&TrackAction::BuiltInFx(
            BuiltInFxControl::ClearMidiCcAssignments
        )));
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
