use crate::{
    OxiSynthControl, OxiSynthMidiCcAssignment, OxiSynthParameter, TrackAction,
    TrackProcessorDescriptor, TrackProcessorEditorDescriptor, TrackProcessorEditorState,
    TrackState, MAX_OXISYNTH_SEND, MIN_OXISYNTH_SEND,
};
use egui_material_icons::icons::ICON_INFO;

const OXISYNTH_LOGO_BYTES: &[u8] = include_bytes!("../../../../third_party/oxisynth/logo.png");
const OXISYNTH_URL: &str = "https://github.com/PolyMeilex/oxisynth";
const TIMGM6MB_URL: &str = "https://timbrechbill.com/saxguru/Timidity.php";
const POWERED_BY_TEXT: &str = "Powered by OxiSynth and TimGM6mb.sf2";

pub(crate) struct OxiSynthEditor {
    filter: String,
    midi_learn_open: bool,
    info_open: bool,
    selected_midi_parameter: OxiSynthParameter,
    logo: Option<egui::TextureHandle>,
    #[cfg(test)]
    window_rect: Option<egui::Rect>,
    #[cfg(test)]
    preset_rect: Option<egui::Rect>,
    #[cfg(test)]
    preset_item_rects: Vec<(String, egui::Rect)>,
    #[cfg(test)]
    panic_rect: Option<egui::Rect>,
    #[cfg(test)]
    reverb_send_rect: Option<egui::Rect>,
    #[cfg(test)]
    chorus_send_rect: Option<egui::Rect>,
    #[cfg(test)]
    midi_learn_rect: Option<egui::Rect>,
    #[cfg(test)]
    midi_assign_rect: Option<egui::Rect>,
    #[cfg(test)]
    info_button_rect: Option<egui::Rect>,
    #[cfg(test)]
    info_logo_rect: Option<egui::Rect>,
    #[cfg(test)]
    oxisynth_url_rect: Option<egui::Rect>,
    #[cfg(test)]
    timgm6mb_url_rect: Option<egui::Rect>,
}

impl std::fmt::Debug for OxiSynthEditor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OxiSynthEditor")
            .field("filter", &self.filter)
            .field("midi_learn_open", &self.midi_learn_open)
            .field("info_open", &self.info_open)
            .field("selected_midi_parameter", &self.selected_midi_parameter)
            .finish_non_exhaustive()
    }
}

impl Default for OxiSynthEditor {
    fn default() -> Self {
        Self {
            filter: String::new(),
            midi_learn_open: false,
            info_open: false,
            selected_midi_parameter: OxiSynthParameter::ReverbSend,
            logo: None,
            #[cfg(test)]
            window_rect: None,
            #[cfg(test)]
            preset_rect: None,
            #[cfg(test)]
            preset_item_rects: Vec::new(),
            #[cfg(test)]
            panic_rect: None,
            #[cfg(test)]
            reverb_send_rect: None,
            #[cfg(test)]
            chorus_send_rect: None,
            #[cfg(test)]
            midi_learn_rect: None,
            #[cfg(test)]
            midi_assign_rect: None,
            #[cfg(test)]
            info_button_rect: None,
            #[cfg(test)]
            info_logo_rect: None,
            #[cfg(test)]
            oxisynth_url_rect: None,
            #[cfg(test)]
            timgm6mb_url_rect: None,
        }
    }
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

        self.ensure_logo(context);
        #[cfg(test)]
        self.preset_item_rects.clear();
        let mut actions = Vec::new();
        let mut open = true;
        let _shown = egui::Window::new(format!("{} — Built-in Synth", state.name))
            .id(egui::Id::new(("oxisynth_editor", state.id)))
            .open(&mut open)
            .resizable(true)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.weak(POWERED_BY_TEXT);
                    let info = ui
                        .small_button(ICON_INFO.rich_text().size(14.0))
                        .on_hover_text("About OxiSynth and TimGM6mb.sf2");
                    #[cfg(test)]
                    {
                        self.info_button_rect = Some(info.rect);
                    }
                    if info.clicked() {
                        self.info_open = true;
                    }
                });
                ui.separator();
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
                    let midi_learn = ui.button("MIDI Learn…");
                    #[cfg(test)]
                    {
                        self.midi_learn_rect = Some(midi_learn.rect);
                    }
                    if midi_learn.clicked() {
                        self.midi_learn_open = true;
                    }
                });

                let mut reverb_send = editor.reverb_send;
                let reverb = ui.add(
                    egui::Slider::new(&mut reverb_send, MIN_OXISYNTH_SEND..=MAX_OXISYNTH_SEND)
                        .text("Reverb send"),
                );
                #[cfg(test)]
                {
                    self.reverb_send_rect = Some(reverb.rect);
                }
                if reverb.changed() {
                    actions.push(TrackAction::OxiSynth(OxiSynthControl::SetReverbSend(
                        reverb_send,
                    )));
                }

                let mut chorus_send = editor.chorus_send;
                let chorus = ui.add(
                    egui::Slider::new(&mut chorus_send, MIN_OXISYNTH_SEND..=MAX_OXISYNTH_SEND)
                        .text("Chorus send"),
                );
                #[cfg(test)]
                {
                    self.chorus_send_rect = Some(chorus.rect);
                }
                if chorus.changed() {
                    actions.push(TrackAction::OxiSynth(OxiSynthControl::SetChorusSend(
                        chorus_send,
                    )));
                }
            });

        if self.info_open {
            let mut info_open = self.info_open;
            egui::Window::new("Built-in Synth information")
                .id(egui::Id::new(("oxisynth_info", state.id)))
                .open(&mut info_open)
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    if let Some(logo) = &self.logo {
                        let size = logo.size_vec2();
                        let width = 320.0;
                        let _response = ui.add(egui::Image::new((
                            logo.id(),
                            egui::vec2(width, width * size.y / size.x),
                        )));
                        #[cfg(test)]
                        {
                            self.info_logo_rect = Some(_response.rect);
                        }
                    }
                    ui.label("OxiSynth GitHub page (copy and paste into your browser):");
                    let _oxisynth_url = ui.add(
                        egui::Label::new(egui::RichText::new(OXISYNTH_URL).monospace())
                            .selectable(true),
                    );
                    #[cfg(test)]
                    {
                        self.oxisynth_url_rect = Some(_oxisynth_url.rect);
                    }
                    ui.separator();
                    ui.label("TimGM6mb.sf2 is a sound font by Tim Brechbill.");
                    ui.label("SoundFont information (copy and paste into your browser):");
                    let _timgm6mb_url = ui.add(
                        egui::Label::new(egui::RichText::new(TIMGM6MB_URL).monospace())
                            .selectable(true),
                    );
                    #[cfg(test)]
                    {
                        self.timgm6mb_url_rect = Some(_timgm6mb_url.rect);
                    }
                });
            self.info_open = info_open;
        }

        if self.midi_learn_open {
            let latest_cc = state
                .controls
                .latest_input_midi_message
                .and_then(|message| message.midi_cc());
            let mut learn_open = self.midi_learn_open;
            let mut selected_parameter = self.selected_midi_parameter;
            egui::Window::new(format!("{} — MIDI Learn", state.name))
                .id(egui::Id::new(("oxisynth_midi_learn", state.id)))
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
                        egui::ComboBox::from_id_salt("oxisynth_midi_cc_parameter")
                            .selected_text(selected_parameter.label())
                            .show_ui(ui, |ui| {
                                for parameter in OxiSynthParameter::ALL {
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
                            actions.push(TrackAction::OxiSynth(OxiSynthControl::AssignMidiCc(
                                OxiSynthMidiCcAssignment {
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
                            if ui.button("Remove").clicked() {
                                actions.push(TrackAction::OxiSynth(OxiSynthControl::RemoveMidiCc(
                                    assignment.parameter,
                                )));
                            }
                        });
                    }
                    if ui
                        .add_enabled(
                            !editor.midi_cc_assignments.is_empty(),
                            egui::Button::new("Remove all"),
                        )
                        .clicked()
                    {
                        actions.push(TrackAction::OxiSynth(
                            OxiSynthControl::ClearMidiCcAssignments,
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

    fn ensure_logo(&mut self, context: &egui::Context) {
        if self.logo.is_some() {
            return;
        }
        let Ok(image) = image::load_from_memory(OXISYNTH_LOGO_BYTES) else {
            return;
        };
        let rgba = image.into_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        self.logo = Some(context.load_texture("oxisynth-logo", image, Default::default()));
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
            label: "Built-in Synth".to_owned(),
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
        let mut state = TrackState {
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
                latency: Default::default(),
                editor: Some(TrackProcessorEditorState::OxiSynth(OxiSynthState {
                    selected_preset_id: "0:0".to_owned(),
                    reverb_send: 0.25,
                    chorus_send: 0.0,
                    midi_cc_assignments: Arc::from([]),
                })),
            }),
            ..TrackState::default()
        };
        state.controls.latest_input_midi_message =
            crate::LatestMidiMessage::new([0xb3, 74, 99, 0], 3);
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
    fn sends_midi_learn_and_synth_information_are_typed() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let (state, processor) = fixture();
        let mut editor = OxiSynthEditor::default();
        frame(&context, &mut editor, &state, &processor, Vec::new());

        let reverb = editor.reverb_send_rect.unwrap();
        let actions = click(
            &context,
            &mut editor,
            &state,
            &processor,
            egui::pos2(reverb.left() + 8.0, reverb.center().y),
        );
        assert!(actions.iter().any(|action| matches!(
            action,
            TrackAction::OxiSynth(OxiSynthControl::SetReverbSend(value))
                if (0.0..=1.0).contains(value)
        )));

        let midi_learn = editor.midi_learn_rect.unwrap().center();
        click(&context, &mut editor, &state, &processor, midi_learn);
        frame(&context, &mut editor, &state, &processor, Vec::new());
        let assign = editor.midi_assign_rect.unwrap().center();
        assert_eq!(
            click(&context, &mut editor, &state, &processor, assign),
            [TrackAction::OxiSynth(OxiSynthControl::AssignMidiCc(
                OxiSynthMidiCcAssignment {
                    parameter: OxiSynthParameter::ReverbSend,
                    channel: 3,
                    controller: 74,
                }
            ))]
        );

        assert_eq!(POWERED_BY_TEXT, "Powered by OxiSynth and TimGM6mb.sf2");
        let info = editor.info_button_rect.unwrap().center();
        assert!(click(&context, &mut editor, &state, &processor, info).is_empty());
        frame(&context, &mut editor, &state, &processor, Vec::new());
        assert!(editor.info_open);
        assert!(editor.info_logo_rect.unwrap().width() > 84.0);
        assert!(editor.oxisynth_url_rect.is_some());
        assert!(editor.timgm6mb_url_rect.is_some());
        assert_eq!(OXISYNTH_URL, "https://github.com/PolyMeilex/oxisynth");
        assert_eq!(
            TIMGM6MB_URL,
            "https://timbrechbill.com/saxguru/Timidity.php"
        );
        let logo = editor.logo.as_ref().unwrap();
        assert_eq!(logo.size()[0] / logo.size()[1], 4);
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
