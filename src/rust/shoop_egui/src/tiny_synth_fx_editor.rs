use crate::{
    TinySynthFxControl, TinySynthFxMidiCcAssignment, TinySynthFxParameter, TrackAction,
    TrackProcessorDescriptor, TrackProcessorEditorDescriptor, TrackProcessorEditorState,
    TrackState, MAX_TINY_SYNTH_FX_EQ_GAIN_DB, MAX_TINY_SYNTH_FX_GAIN_DB,
    MIN_TINY_SYNTH_FX_EQ_GAIN_DB, MIN_TINY_SYNTH_FX_GAIN_DB,
};

#[derive(Debug)]
pub(crate) struct TinySynthFxEditor {
    midi_learn_open: bool,
    selected_midi_parameter: TinySynthFxParameter,
    #[cfg(test)]
    window_rect: Option<egui::Rect>,
    #[cfg(test)]
    preset_rect: Option<egui::Rect>,
    #[cfg(test)]
    preset_item_rects: Vec<(String, egui::Rect)>,
    #[cfg(test)]
    panic_rect: Option<egui::Rect>,
    #[cfg(test)]
    gain_rect: Option<egui::Rect>,
    #[cfg(test)]
    reverb_rect: Option<egui::Rect>,
    #[cfg(test)]
    reverb_amount_rect: Option<egui::Rect>,
    #[cfg(test)]
    distortion_rect: Option<egui::Rect>,
    #[cfg(test)]
    distortion_drive_rect: Option<egui::Rect>,
    #[cfg(test)]
    compressor_rect: Option<egui::Rect>,
    #[cfg(test)]
    compressor_amount_rect: Option<egui::Rect>,
    #[cfg(test)]
    eq_rect: Option<egui::Rect>,
    #[cfg(test)]
    eq_low_rect: Option<egui::Rect>,
    #[cfg(test)]
    eq_mid_rect: Option<egui::Rect>,
    #[cfg(test)]
    eq_high_rect: Option<egui::Rect>,
    #[cfg(test)]
    midi_learn_rect: Option<egui::Rect>,
    #[cfg(test)]
    midi_assign_rect: Option<egui::Rect>,
    #[cfg(test)]
    midi_remove_all_rect: Option<egui::Rect>,
    #[cfg(test)]
    midi_remove_rects: Vec<(TinySynthFxParameter, egui::Rect)>,
}

impl Default for TinySynthFxEditor {
    fn default() -> Self {
        Self {
            midi_learn_open: false,
            selected_midi_parameter: TinySynthFxParameter::MasterGain,
            #[cfg(test)]
            window_rect: None,
            #[cfg(test)]
            preset_rect: None,
            #[cfg(test)]
            preset_item_rects: Vec::new(),
            #[cfg(test)]
            panic_rect: None,
            #[cfg(test)]
            gain_rect: None,
            #[cfg(test)]
            reverb_rect: None,
            #[cfg(test)]
            reverb_amount_rect: None,
            #[cfg(test)]
            distortion_rect: None,
            #[cfg(test)]
            distortion_drive_rect: None,
            #[cfg(test)]
            compressor_rect: None,
            #[cfg(test)]
            compressor_amount_rect: None,
            #[cfg(test)]
            eq_rect: None,
            #[cfg(test)]
            eq_low_rect: None,
            #[cfg(test)]
            eq_mid_rect: None,
            #[cfg(test)]
            eq_high_rect: None,
            #[cfg(test)]
            midi_learn_rect: None,
            #[cfg(test)]
            midi_assign_rect: None,
            #[cfg(test)]
            midi_remove_all_rect: None,
            #[cfg(test)]
            midi_remove_rects: Vec::new(),
        }
    }
}

impl TinySynthFxEditor {
    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        state: &TrackState,
        processor: Option<&TrackProcessorDescriptor>,
    ) -> Vec<TrackAction> {
        let Some(fx) = &state.fx else {
            return Vec::new();
        };
        let Some(TrackProcessorEditorState::TinySynthFx(editor)) = &fx.editor else {
            return Vec::new();
        };
        let Some(TrackProcessorEditorDescriptor::TinySynthFx { presets }) =
            processor.and_then(|processor| processor.editor.as_ref())
        else {
            return Vec::new();
        };
        if !fx.visible {
            return Vec::new();
        }

        let mut actions = Vec::new();
        #[cfg(test)]
        {
            self.preset_item_rects.clear();
            self.midi_remove_rects.clear();
        }
        let mut open = true;
        let _shown = egui::Window::new(format!("{} — Tiny Synth/FX", state.name))
            .id(egui::Id::new(("tiny_synth_fx_editor", state.id)))
            .open(&mut open)
            .resizable(true)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Preset");
                    let selected_name = editor
                        .selected_preset_id
                        .as_ref()
                        .and_then(|selected| {
                            presets
                                .iter()
                                .find(|preset| preset.id == *selected)
                                .map(|preset| preset.name.as_str())
                        })
                        .unwrap_or("Custom");
                    let _preset_combo = egui::ComboBox::from_id_salt("preset")
                        .selected_text(selected_name)
                        .show_ui(ui, |ui| {
                            for preset in presets.iter() {
                                let selected = editor.selected_preset_id.as_deref()
                                    == Some(preset.id.as_str());
                                let response = ui.selectable_label(selected, &preset.name);
                                #[cfg(test)]
                                self.preset_item_rects
                                    .push((preset.id.clone(), response.rect));
                                if response.clicked() {
                                    actions.push(TrackAction::TinySynthFx(
                                        TinySynthFxControl::SelectPreset(preset.id.clone()),
                                    ));
                                }
                            }
                        });
                    #[cfg(test)]
                    {
                        self.preset_rect = Some(_preset_combo.response.rect);
                    }
                    let panic = ui.button("Panic");
                    #[cfg(test)]
                    {
                        self.panic_rect = Some(panic.rect);
                    }
                    if panic.clicked() {
                        actions.push(TrackAction::TinySynthFx(TinySynthFxControl::Panic));
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

                let mut gain = editor.master_gain_db;
                let gain_response = ui.add(
                    egui::Slider::new(
                        &mut gain,
                        MIN_TINY_SYNTH_FX_GAIN_DB..=MAX_TINY_SYNTH_FX_GAIN_DB,
                    )
                    .text("Master gain")
                    .suffix(" dB"),
                );
                #[cfg(test)]
                {
                    self.gain_rect = Some(gain_response.rect);
                }
                if gain_response.changed() {
                    actions.push(TrackAction::TinySynthFx(
                        TinySynthFxControl::SetMasterGainDb(gain),
                    ));
                }

                let mut reverb_enabled = editor.reverb_enabled;
                let reverb = ui.checkbox(&mut reverb_enabled, "Reverb");
                #[cfg(test)]
                {
                    self.reverb_rect = Some(reverb.rect);
                }
                if reverb.changed() {
                    actions.push(TrackAction::TinySynthFx(
                        TinySynthFxControl::SetReverbEnabled(reverb_enabled),
                    ));
                }
                let mut reverb_amount = editor.reverb_amount;
                let reverb_amount_response = ui.add_enabled(
                    reverb_enabled,
                    egui::Slider::new(&mut reverb_amount, 0.0..=1.0).text("Amount"),
                );
                #[cfg(test)]
                {
                    self.reverb_amount_rect = Some(reverb_amount_response.rect);
                }
                if reverb_amount_response.changed() {
                    actions.push(TrackAction::TinySynthFx(
                        TinySynthFxControl::SetReverbAmount(reverb_amount),
                    ));
                }

                let mut distortion_enabled = editor.distortion_enabled;
                let distortion = ui.checkbox(&mut distortion_enabled, "Distortion");
                #[cfg(test)]
                {
                    self.distortion_rect = Some(distortion.rect);
                }
                if distortion.changed() {
                    actions.push(TrackAction::TinySynthFx(
                        TinySynthFxControl::SetDistortionEnabled(distortion_enabled),
                    ));
                }
                let mut distortion_drive = editor.distortion_drive;
                let distortion_drive_response = ui.add_enabled(
                    distortion_enabled,
                    egui::Slider::new(&mut distortion_drive, 1.0..=20.0).text("Drive"),
                );
                #[cfg(test)]
                {
                    self.distortion_drive_rect = Some(distortion_drive_response.rect);
                }
                if distortion_drive_response.changed() {
                    actions.push(TrackAction::TinySynthFx(
                        TinySynthFxControl::SetDistortionDrive(distortion_drive),
                    ));
                }

                let mut compressor_enabled = editor.compressor_enabled;
                let compressor = ui.checkbox(&mut compressor_enabled, "Compressor");
                #[cfg(test)]
                {
                    self.compressor_rect = Some(compressor.rect);
                }
                if compressor.changed() {
                    actions.push(TrackAction::TinySynthFx(
                        TinySynthFxControl::SetCompressorEnabled(compressor_enabled),
                    ));
                }
                let mut compressor_amount = editor.compressor_amount;
                let compressor_amount_response = ui.add_enabled(
                    compressor_enabled,
                    egui::Slider::new(&mut compressor_amount, 0.0..=1.0).text("Amount"),
                );
                #[cfg(test)]
                {
                    self.compressor_amount_rect = Some(compressor_amount_response.rect);
                }
                if compressor_amount_response.changed() {
                    actions.push(TrackAction::TinySynthFx(
                        TinySynthFxControl::SetCompressorAmount(compressor_amount),
                    ));
                }

                let mut eq_enabled = editor.eq_enabled;
                let eq = ui.checkbox(&mut eq_enabled, "Three-band EQ");
                #[cfg(test)]
                {
                    self.eq_rect = Some(eq.rect);
                }
                if eq.changed() {
                    actions.push(TrackAction::TinySynthFx(TinySynthFxControl::SetEqEnabled(
                        eq_enabled,
                    )));
                }
                for (label, gain, control) in [
                    (
                        "Low",
                        editor.eq_low_db,
                        TinySynthFxControl::SetEqLowDb as fn(f32) -> TinySynthFxControl,
                    ),
                    (
                        "Mid",
                        editor.eq_mid_db,
                        TinySynthFxControl::SetEqMidDb as fn(f32) -> TinySynthFxControl,
                    ),
                    (
                        "High",
                        editor.eq_high_db,
                        TinySynthFxControl::SetEqHighDb as fn(f32) -> TinySynthFxControl,
                    ),
                ] {
                    let mut gain = gain;
                    let response = ui.add_enabled(
                        eq_enabled,
                        egui::Slider::new(
                            &mut gain,
                            MIN_TINY_SYNTH_FX_EQ_GAIN_DB..=MAX_TINY_SYNTH_FX_EQ_GAIN_DB,
                        )
                        .text(label)
                        .suffix(" dB"),
                    );
                    #[cfg(test)]
                    match label {
                        "Low" => self.eq_low_rect = Some(response.rect),
                        "Mid" => self.eq_mid_rect = Some(response.rect),
                        "High" => self.eq_high_rect = Some(response.rect),
                        _ => unreachable!(),
                    }
                    if response.changed() {
                        actions.push(TrackAction::TinySynthFx(control(gain)));
                    }
                }
            });

        if self.midi_learn_open {
            let latest_cc = state
                .controls
                .latest_input_midi_message
                .and_then(|message| message.midi_cc());
            let mut learn_open = self.midi_learn_open;
            let mut selected_parameter = self.selected_midi_parameter;
            egui::Window::new(format!("{} — MIDI Learn", state.name))
                .id(egui::Id::new(("tiny_synth_fx_midi_learn", state.id)))
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
                        egui::ComboBox::from_id_salt("midi_cc_parameter")
                            .selected_text(selected_parameter.label())
                            .show_ui(ui, |ui| {
                                for parameter in TinySynthFxParameter::ALL {
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
                            actions.push(TrackAction::TinySynthFx(
                                TinySynthFxControl::AssignMidiCc(TinySynthFxMidiCcAssignment {
                                    parameter: selected_parameter,
                                    channel,
                                    controller,
                                }),
                            ));
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
                                actions.push(TrackAction::TinySynthFx(
                                    TinySynthFxControl::RemoveMidiCc(assignment.parameter),
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
                        actions.push(TrackAction::TinySynthFx(
                            TinySynthFxControl::ClearMidiCcAssignments,
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

    #[cfg(test)]
    pub(crate) fn window_rect(&self) -> Option<egui::Rect> {
        self.window_rect
    }

    #[cfg(test)]
    pub(crate) fn preset_rect(&self) -> Option<egui::Rect> {
        self.preset_rect
    }

    #[cfg(test)]
    pub(crate) fn preset_item_rect(&self, id: &str) -> Option<egui::Rect> {
        self.preset_item_rects
            .iter()
            .find_map(|(item_id, rect)| (item_id == id).then_some(*rect))
    }

    #[cfg(test)]
    pub(crate) fn panic_rect(&self) -> Option<egui::Rect> {
        self.panic_rect
    }

    #[cfg(test)]
    pub(crate) fn gain_rect(&self) -> Option<egui::Rect> {
        self.gain_rect
    }

    #[cfg(test)]
    pub(crate) fn reverb_rect(&self) -> Option<egui::Rect> {
        self.reverb_rect
    }

    #[cfg(test)]
    pub(crate) fn reverb_amount_rect(&self) -> Option<egui::Rect> {
        self.reverb_amount_rect
    }

    #[cfg(test)]
    pub(crate) fn distortion_rect(&self) -> Option<egui::Rect> {
        self.distortion_rect
    }

    #[cfg(test)]
    pub(crate) fn distortion_drive_rect(&self) -> Option<egui::Rect> {
        self.distortion_drive_rect
    }

    #[cfg(test)]
    pub(crate) fn compressor_rect(&self) -> Option<egui::Rect> {
        self.compressor_rect
    }

    #[cfg(test)]
    pub(crate) fn compressor_amount_rect(&self) -> Option<egui::Rect> {
        self.compressor_amount_rect
    }

    #[cfg(test)]
    pub(crate) fn eq_rect(&self) -> Option<egui::Rect> {
        self.eq_rect
    }

    #[cfg(test)]
    pub(crate) fn eq_low_rect(&self) -> Option<egui::Rect> {
        self.eq_low_rect
    }

    #[cfg(test)]
    pub(crate) fn eq_mid_rect(&self) -> Option<egui::Rect> {
        self.eq_mid_rect
    }

    #[cfg(test)]
    pub(crate) fn eq_high_rect(&self) -> Option<egui::Rect> {
        self.eq_high_rect
    }

    #[cfg(test)]
    pub(crate) fn midi_learn_rect(&self) -> Option<egui::Rect> {
        self.midi_learn_rect
    }

    #[cfg(test)]
    pub(crate) fn midi_assign_rect(&self) -> Option<egui::Rect> {
        self.midi_assign_rect
    }

    #[cfg(test)]
    pub(crate) fn midi_remove_all_rect(&self) -> Option<egui::Rect> {
        self.midi_remove_all_rect
    }

    #[cfg(test)]
    pub(crate) fn midi_remove_rect(&self, parameter: TinySynthFxParameter) -> Option<egui::Rect> {
        self.midi_remove_rects
            .iter()
            .find_map(|(item, rect)| (*item == parameter).then_some(*rect))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        FxLifecycle, LatestMidiMessage, TinySynthFxState, TrackFxState, TrackId,
        TrackProcessorConstraints, TrackProcessorFeatures, TrackProcessorMidiPolicy,
        TrackProcessorPresetDescriptor, TrackProcessorTypeId,
    };

    fn fixture() -> (TrackState, TrackProcessorDescriptor) {
        let processor = TrackProcessorDescriptor {
            id: TrackProcessorTypeId::new(TrackProcessorTypeId::TINY_SYNTH_FX),
            label: "Tiny Synth/FX".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: TrackProcessorConstraints {
                max_dry_audio_channels: None,
                max_wet_audio_channels: None,
                matching_audio_channels: true,
                midi: TrackProcessorMidiPolicy::Required,
            },
            features: TrackProcessorFeatures {
                state: true,
                external_ui: false,
                embedded_ui: true,
                recovery: false,
                logs: false,
            },
            editor: Some(TrackProcessorEditorDescriptor::TinySynthFx {
                presets: Arc::from([
                    TrackProcessorPresetDescriptor {
                        id: "sine".to_owned(),
                        name: "Sine".to_owned(),
                    },
                    TrackProcessorPresetDescriptor {
                        id: "pad".to_owned(),
                        name: "Pad".to_owned(),
                    },
                    TrackProcessorPresetDescriptor {
                        id: "pluck".to_owned(),
                        name: "Pluck".to_owned(),
                    },
                ]),
            }),
        };
        let state = TrackState {
            id: TrackId::from_raw(42),
            name: "Tiny".to_owned(),
            fx: Some(TrackFxState {
                processor_type: processor.id.clone(),
                active: true,
                visible: true,
                lifecycle: FxLifecycle::Running,
                generation: 0,
                crash_summary: None,
                logs: Arc::from([]),
                editor: Some(TrackProcessorEditorState::TinySynthFx(TinySynthFxState {
                    selected_preset_id: Some("sine".to_owned()),
                    master_gain_db: -6.0,
                    reverb_enabled: false,
                    reverb_amount: 0.25,
                    distortion_enabled: false,
                    distortion_drive: 4.0,
                    compressor_enabled: false,
                    compressor_amount: 0.5,
                    eq_enabled: false,
                    eq_low_db: 0.0,
                    eq_mid_db: 0.0,
                    eq_high_db: 0.0,
                    midi_cc_assignments: Arc::from([]),
                })),
            }),
            ..Default::default()
        };
        (state, processor)
    }

    fn frame(
        context: &egui::Context,
        editor: &mut TinySynthFxEditor,
        state: &TrackState,
        processor: &TrackProcessorDescriptor,
        events: Vec<egui::Event>,
    ) -> Vec<TrackAction> {
        let mut actions = Vec::new();
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500.0, 400.0),
                )),
                events,
                ..Default::default()
            },
            |ui| actions = editor.show(ui.ctx(), state, Some(processor)),
        );
        actions
    }

    fn click(
        context: &egui::Context,
        editor: &mut TinySynthFxEditor,
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
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        ));
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
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        ));
        actions.dedup();
        actions
    }

    fn state_mut(state: &mut TrackState) -> &mut TinySynthFxState {
        let Some(TrackProcessorEditorState::TinySynthFx(editor)) =
            state.fx.as_mut().and_then(|fx| fx.editor.as_mut())
        else {
            panic!("missing Tiny Synth/FX editor fixture");
        };
        editor
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn stable_track_ids_isolate_multiple_embedded_editors() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let (first_state, processor) = fixture();
        let mut second_state = first_state.clone();
        second_state.id = TrackId::from_raw(43);
        second_state.name = "Other Tiny".to_owned();
        let mut first = TinySynthFxEditor::default();
        let mut second = TinySynthFxEditor::default();
        let run = |events: Vec<egui::Event>,
                   first: &mut TinySynthFxEditor,
                   second: &mut TinySynthFxEditor| {
            let mut actions = (Vec::new(), Vec::new());
            let _ = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    actions.0 = first.show(ui.ctx(), &first_state, Some(&processor));
                    actions.1 = second.show(ui.ctx(), &second_state, Some(&processor));
                },
            );
            actions
        };
        assert_eq!(run(Vec::new(), &mut first, &mut second), (vec![], vec![]));
        assert_ne!(first.window_rect(), second.window_rect());
        let position = first.panic_rect().unwrap().center();
        let _ = run(
            vec![egui::Event::PointerMoved(position)],
            &mut first,
            &mut second,
        );
        let _ = run(
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            &mut first,
            &mut second,
        );
        let actions = run(
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            &mut first,
            &mut second,
        );
        assert_eq!(
            actions,
            (
                vec![TrackAction::TinySynthFx(TinySynthFxControl::Panic)],
                vec![]
            )
        );
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn midi_learn_emits_assignment_removal_and_clear_actions() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let (mut state, processor) = fixture();
        let mut editor = TinySynthFxEditor::default();
        frame(&context, &mut editor, &state, &processor, Vec::new());

        let open = editor.midi_learn_rect().unwrap().center();
        assert!(click(&context, &mut editor, &state, &processor, open).is_empty());
        let assign = editor.midi_assign_rect().unwrap().center();
        assert!(click(&context, &mut editor, &state, &processor, assign).is_empty());

        state.controls.latest_input_midi_message = LatestMidiMessage::new([0x90, 64, 127, 0], 3);
        frame(&context, &mut editor, &state, &processor, Vec::new());
        let assign = editor.midi_assign_rect().unwrap().center();
        assert!(click(&context, &mut editor, &state, &processor, assign).is_empty());

        state.controls.latest_input_midi_message = LatestMidiMessage::new([0xb4, 19, 88, 0], 3);
        frame(&context, &mut editor, &state, &processor, Vec::new());
        let assign = editor.midi_assign_rect().unwrap().center();
        assert_eq!(
            click(&context, &mut editor, &state, &processor, assign),
            [TrackAction::TinySynthFx(TinySynthFxControl::AssignMidiCc(
                TinySynthFxMidiCcAssignment {
                    parameter: TinySynthFxParameter::MasterGain,
                    channel: 4,
                    controller: 19,
                }
            ))]
        );

        state_mut(&mut state).midi_cc_assignments = Arc::from([
            TinySynthFxMidiCcAssignment {
                parameter: TinySynthFxParameter::MasterGain,
                channel: 4,
                controller: 19,
            },
            TinySynthFxMidiCcAssignment {
                parameter: TinySynthFxParameter::EqHigh,
                channel: 1,
                controller: 74,
            },
        ]);
        frame(&context, &mut editor, &state, &processor, Vec::new());
        let remove = editor
            .midi_remove_rect(TinySynthFxParameter::EqHigh)
            .unwrap()
            .center();
        assert_eq!(
            click(&context, &mut editor, &state, &processor, remove),
            [TrackAction::TinySynthFx(TinySynthFxControl::RemoveMidiCc(
                TinySynthFxParameter::EqHigh
            ))]
        );
        let remove_all = editor.midi_remove_all_rect().unwrap().center();
        assert_eq!(
            click(&context, &mut editor, &state, &processor, remove_all),
            [TrackAction::TinySynthFx(
                TinySynthFxControl::ClearMidiCcAssignments
            )]
        );
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn embedded_editor_emits_typed_intents_for_every_control_and_close() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let (mut state, processor) = fixture();
        let mut editor = TinySynthFxEditor::default();
        assert!(frame(&context, &mut editor, &state, &processor, Vec::new()).is_empty());

        let panic = editor.panic_rect().unwrap().center();
        assert_eq!(
            click(&context, &mut editor, &state, &processor, panic),
            [TrackAction::TinySynthFx(TinySynthFxControl::Panic)]
        );
        let gain_rect = editor.gain_rect().unwrap();
        let gain_actions = click(
            &context,
            &mut editor,
            &state,
            &processor,
            egui::pos2(gain_rect.left() + 8.0, gain_rect.center().y),
        );
        assert!(
            matches!(
                gain_actions.as_slice(),
                [TrackAction::TinySynthFx(TinySynthFxControl::SetMasterGainDb(value))]
                    if (-60.0..=0.0).contains(value)
            ),
            "unexpected gain actions: {gain_actions:?}"
        );

        let preset = editor.preset_rect().unwrap().center();
        assert!(click(&context, &mut editor, &state, &processor, preset).is_empty());
        let pluck = editor.preset_item_rect("pluck").unwrap().center();
        let _ = frame(
            &context,
            &mut editor,
            &state,
            &processor,
            vec![egui::Event::PointerMoved(pluck)],
        );
        let pluck = editor.preset_item_rect("pluck").unwrap().center();
        assert_eq!(
            click(&context, &mut editor, &state, &processor, pluck),
            [TrackAction::TinySynthFx(TinySynthFxControl::SelectPreset(
                "pluck".to_owned()
            ))]
        );

        let reverb = editor.reverb_rect().unwrap().center();
        assert_eq!(
            click(&context, &mut editor, &state, &processor, reverb),
            [TrackAction::TinySynthFx(
                TinySynthFxControl::SetReverbEnabled(true)
            )]
        );
        state_mut(&mut state).reverb_enabled = true;
        let _ = frame(&context, &mut editor, &state, &processor, Vec::new());
        let amount_rect = editor.reverb_amount_rect().unwrap();
        let amount_actions = click(
            &context,
            &mut editor,
            &state,
            &processor,
            egui::pos2(amount_rect.left() + 8.0, amount_rect.center().y),
        );
        assert!(matches!(
            amount_actions.as_slice(),
            [TrackAction::TinySynthFx(TinySynthFxControl::SetReverbAmount(value))]
                if (0.0..=1.0).contains(value)
        ));

        let distortion = editor.distortion_rect().unwrap().center();
        assert_eq!(
            click(&context, &mut editor, &state, &processor, distortion),
            [TrackAction::TinySynthFx(
                TinySynthFxControl::SetDistortionEnabled(true)
            )]
        );
        state_mut(&mut state).distortion_enabled = true;
        let _ = frame(&context, &mut editor, &state, &processor, Vec::new());
        let drive_rect = editor.distortion_drive_rect().unwrap();
        let drive_actions = click(
            &context,
            &mut editor,
            &state,
            &processor,
            egui::pos2(drive_rect.left() + 8.0, drive_rect.center().y),
        );
        assert!(matches!(
            drive_actions.as_slice(),
            [TrackAction::TinySynthFx(TinySynthFxControl::SetDistortionDrive(value))]
                if (1.0..=20.0).contains(value)
        ));

        let compressor = editor.compressor_rect().unwrap().center();
        assert_eq!(
            click(&context, &mut editor, &state, &processor, compressor),
            [TrackAction::TinySynthFx(
                TinySynthFxControl::SetCompressorEnabled(true)
            )]
        );
        state_mut(&mut state).compressor_enabled = true;
        let _ = frame(&context, &mut editor, &state, &processor, Vec::new());
        let amount_rect = editor.compressor_amount_rect().unwrap();
        let amount_actions = click(
            &context,
            &mut editor,
            &state,
            &processor,
            egui::pos2(amount_rect.left() + 8.0, amount_rect.center().y),
        );
        assert!(matches!(
            amount_actions.as_slice(),
            [TrackAction::TinySynthFx(TinySynthFxControl::SetCompressorAmount(value))]
                if (0.0..=1.0).contains(value)
        ));

        let eq = editor.eq_rect().unwrap().center();
        assert_eq!(
            click(&context, &mut editor, &state, &processor, eq),
            [TrackAction::TinySynthFx(TinySynthFxControl::SetEqEnabled(
                true
            ))]
        );
        state_mut(&mut state).eq_enabled = true;
        let _ = frame(&context, &mut editor, &state, &processor, Vec::new());
        for (rect, expected) in [
            (editor.eq_low_rect().unwrap(), "low"),
            (editor.eq_mid_rect().unwrap(), "mid"),
            (editor.eq_high_rect().unwrap(), "high"),
        ] {
            let actions = click(
                &context,
                &mut editor,
                &state,
                &processor,
                egui::pos2(rect.left() + 8.0, rect.center().y),
            );
            assert!(matches!(
                (expected, actions.as_slice()),
                ("low", [TrackAction::TinySynthFx(TinySynthFxControl::SetEqLowDb(value))])
                    | ("mid", [TrackAction::TinySynthFx(TinySynthFxControl::SetEqMidDb(value))])
                    | ("high", [TrackAction::TinySynthFx(TinySynthFxControl::SetEqHighDb(value))])
                    if (MIN_TINY_SYNTH_FX_EQ_GAIN_DB..=MAX_TINY_SYNTH_FX_EQ_GAIN_DB).contains(value)
            ));
        }

        let window = editor.window_rect().unwrap();
        let close = egui::pos2(window.right() - 12.0, window.top() + 12.0);
        assert_eq!(
            click(&context, &mut editor, &state, &processor, close),
            [TrackAction::FxVisibilityChanged(false)]
        );
    }
}
