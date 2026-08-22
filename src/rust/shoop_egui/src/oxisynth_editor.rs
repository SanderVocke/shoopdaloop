use crate::{
    OxiSynthControl, TrackAction, TrackProcessorDescriptor, TrackProcessorEditorDescriptor,
    TrackProcessorEditorState, TrackState,
};

#[derive(Debug, Default)]
pub(crate) struct OxiSynthEditor {
    channel: usize,
    search: String,
    auditioning: bool,
    last_midi_activity_revision: u64,
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
        let Some(TrackProcessorEditorDescriptor::OxiSynth {
            soundfont_name,
            presets,
            ..
        }) = processor.and_then(|processor| processor.editor.as_ref())
        else {
            return Vec::new();
        };
        if !fx.visible {
            return Vec::new();
        }

        let mut actions = Vec::new();
        let mut open = true;
        egui::Window::new(format!("{} — OxiSynth", state.name))
            .id(egui::Id::new(("oxisynth_editor", state.id)))
            .open(&mut open)
            .resizable(true)
            .show(context, |ui| {
                ui.label(format!("SoundFont: {soundfont_name} (built in)"));
                ui.horizontal(|ui| {
                    ui.label("MIDI channel");
                    egui::ComboBox::from_id_salt("oxisynth_channel")
                        .selected_text(format!("{}", self.channel + 1))
                        .show_ui(ui, |ui| {
                            for channel in 0..16 {
                                ui.selectable_value(
                                    &mut self.channel,
                                    channel,
                                    format!("{}", channel + 1),
                                );
                            }
                        });
                    if editor.midi_activity_revision != self.last_midi_activity_revision {
                        ui.label("MIDI ●");
                        self.last_midi_activity_revision = editor.midi_activity_revision;
                    } else {
                        ui.label("MIDI ○");
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search presets"));
                let channel = editor.channels[self.channel];
                let selected = presets
                    .iter()
                    .find(|preset| {
                        preset.bank == channel.current_bank
                            && preset.program == channel.current_program
                    })
                    .map(|preset| preset.name.as_ref())
                    .unwrap_or("Unavailable preset");
                egui::ComboBox::from_id_salt("oxisynth_preset")
                    .selected_text(format!(
                        "{selected} ({}:{})",
                        channel.current_bank, channel.current_program
                    ))
                    .show_ui(ui, |ui| {
                        let needle = self.search.to_lowercase();
                        for preset in presets.iter().filter(|preset| {
                            needle.is_empty() || preset.name.to_lowercase().contains(&needle)
                        }) {
                            let selected = preset.bank == channel.current_bank
                                && preset.program == channel.current_program;
                            if ui
                                .selectable_label(
                                    selected,
                                    format!("{} — {}:{}", preset.name, preset.bank, preset.program),
                                )
                                .clicked()
                            {
                                actions.push(TrackAction::OxiSynth(
                                    OxiSynthControl::SelectProgram {
                                        channel: self.channel as u8,
                                        bank: preset.bank,
                                        program: preset.program,
                                    },
                                ));
                            }
                        }
                    });
                if (channel.baseline_bank, channel.baseline_program)
                    != (channel.current_bank, channel.current_program)
                {
                    ui.label(format!(
                        "MIDI override; saved default {}:{}",
                        channel.baseline_bank, channel.baseline_program
                    ));
                }
                ui.horizontal(|ui| {
                    let pressed = ui.button("Hold to audition").is_pointer_button_down_on();
                    if pressed != self.auditioning {
                        self.auditioning = pressed;
                        actions.push(TrackAction::OxiSynth(OxiSynthControl::Audition {
                            channel: self.channel as u8,
                            key: 60,
                            velocity: 100,
                            pressed,
                        }));
                    }
                    if ui.button("Panic").clicked() {
                        actions.push(TrackAction::OxiSynth(OxiSynthControl::Panic));
                    }
                });
            });
        if !open {
            actions.push(TrackAction::FxVisibilityChanged(false));
        }
        actions
    }
}
