use crate::{
    OxiSynthControl, TrackAction, TrackProcessorDescriptor, TrackProcessorEditorDescriptor,
    TrackProcessorEditorState, TrackState,
};
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

#[derive(Debug, Default)]
pub(crate) struct OxiSynthEditor {
    channel: usize,
    search: String,
    auditioning: bool,
    last_midi_activity_revision: u64,
    favorites: BTreeSet<(String, u32, u8)>,
    recent: VecDeque<(String, u32, u8)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn releasing_hidden_editor_ends_audition() {
        let mut editor = OxiSynthEditor {
            channel: 3,
            auditioning: true,
            ..Default::default()
        };
        assert_eq!(
            editor.release_audition(),
            vec![TrackAction::OxiSynth(OxiSynthControl::Audition {
                channel: 3,
                key: 60,
                velocity: 100,
                pressed: false,
            })]
        );
        assert!(editor.release_audition().is_empty());
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
        let Some(TrackProcessorEditorDescriptor::OxiSynth { .. }) =
            processor.and_then(|processor| processor.editor.as_ref())
        else {
            return Vec::new();
        };
        if !fx.visible {
            return self.release_audition();
        }

        let mut actions = Vec::new();
        let mut open = true;
        egui::Window::new(format!("{} — OxiSynth", state.name))
            .id(egui::Id::new(("oxisynth_editor", state.id)))
            .open(&mut open)
            .resizable(true)
            .show(context, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("SoundFont");
                    egui::ComboBox::from_id_salt("oxisynth_soundfont")
                        .selected_text(editor.soundfont_name.as_ref())
                        .show_ui(ui, |ui| {
                            for asset in editor.available_soundfonts.iter() {
                                if ui
                                    .selectable_label(
                                        asset.sha256 == editor.soundfont_sha256,
                                        format!(
                                            "{} ({} presets{})",
                                            asset.name,
                                            asset.presets.len(),
                                            if asset.built_in { ", built in" } else { "" }
                                        ),
                                    )
                                    .on_hover_text(format!(
                                        "{}\n{} bytes\nSHA-256 {}",
                                        asset.original_filename, asset.byte_len, asset.sha256
                                    ))
                                    .clicked()
                                {
                                    actions.push(TrackAction::OxiSynth(
                                        OxiSynthControl::SelectSoundFont(asset.sha256.clone()),
                                    ));
                                }
                            }
                        });
                    ui.label("Drop an .sf2 file into the application to import it.");
                });
                ui.collapsing("Manage SoundFont library", |ui| {
                    for asset in editor.available_soundfonts.iter() {
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{} — {} presets, {} bytes",
                                asset.name,
                                asset.presets.len(),
                                asset.byte_len
                            ));
                            if !asset.built_in && asset.sha256 != editor.soundfont_sha256 {
                                if ui.button("Remove").clicked() {
                                    actions.push(TrackAction::RemoveSoundFont(Arc::clone(
                                        &asset.sha256,
                                    )));
                                }
                            } else if asset.built_in {
                                ui.label("Built in");
                            } else {
                                ui.label("In use");
                            }
                        });
                    }
                });
                ui.separator();
                ui.label("Output");
                let mut master_gain = editor.master_gain;
                if ui
                    .add(
                        egui::Slider::new(&mut master_gain, 0.0..=10.0)
                            .text("Master gain")
                            .logarithmic(true),
                    )
                    .changed()
                {
                    actions.push(TrackAction::OxiSynth(OxiSynthControl::SetMasterGain(
                        master_gain,
                    )));
                }
                ui.horizontal(|ui| {
                    ui.label("Stereo output");
                    ui.add(
                        egui::ProgressBar::new(
                            ((state.controls.output_peak_left_db + 60.0) / 60.0).clamp(0.0, 1.0),
                        )
                        .text(format!("L {:.1} dB", state.controls.output_peak_left_db)),
                    );
                    ui.add(
                        egui::ProgressBar::new(
                            ((state.controls.output_peak_right_db + 60.0) / 60.0).clamp(0.0, 1.0),
                        )
                        .text(format!("R {:.1} dB", state.controls.output_peak_right_db)),
                    );
                });
                ui.collapsing("Reverb", |ui| {
                    let mut value = editor.reverb;
                    let mut changed = false;
                    changed |= ui
                        .add(egui::Slider::new(&mut value.room_size, 0.0..=1.0).text("Room size"))
                        .changed();
                    changed |= ui
                        .add(egui::Slider::new(&mut value.damp, 0.0..=1.0).text("Damping"))
                        .changed();
                    changed |= ui
                        .add(egui::Slider::new(&mut value.width, 0.0..=1.0).text("Stereo width"))
                        .changed();
                    changed |= ui
                        .add(egui::Slider::new(&mut value.level, 0.0..=1.0).text("Level"))
                        .changed();
                    if changed {
                        actions.push(TrackAction::OxiSynth(OxiSynthControl::SetReverb(value)));
                    }
                });
                ui.collapsing("Chorus", |ui| {
                    let mut value = editor.chorus;
                    let mut changed = false;
                    changed |= ui
                        .add(egui::Slider::new(&mut value.voices, 0..=99).text("Voices"))
                        .changed();
                    changed |= ui
                        .add(egui::Slider::new(&mut value.level, 0.0..=10.0).text("Level"))
                        .changed();
                    changed |= ui
                        .add(egui::Slider::new(&mut value.speed_hz, 0.1..=5.0).text("Speed (Hz)"))
                        .changed();
                    changed |= ui
                        .add(egui::Slider::new(&mut value.depth_ms, 0.0..=256.0).text("Depth (ms)"))
                        .changed();
                    if changed {
                        actions.push(TrackAction::OxiSynth(OxiSynthControl::SetChorus(value)));
                    }
                });
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
                let selected = editor
                    .presets
                    .iter()
                    .find(|preset| {
                        preset.bank == channel.current_bank
                            && preset.program == channel.current_program
                    })
                    .map(|preset| preset.name.as_ref())
                    .unwrap_or("Unavailable preset");
                let selected_index = editor.presets.iter().position(|preset| {
                    preset.bank == channel.current_bank && preset.program == channel.current_program
                });
                let selected_key = (
                    editor.soundfont_sha256.to_string(),
                    channel.current_bank,
                    channel.current_program,
                );
                let mut favorite = self.favorites.contains(&selected_key);
                if ui.checkbox(&mut favorite, "Favorite preset").changed() {
                    if favorite {
                        self.favorites.insert(selected_key.clone());
                    } else {
                        self.favorites.remove(&selected_key);
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("Favorites:");
                    for (_, bank, program) in self
                        .favorites
                        .iter()
                        .filter(|(digest, _, _)| digest == editor.soundfont_sha256.as_ref())
                    {
                        let name = editor
                            .presets
                            .iter()
                            .find(|preset| (preset.bank, preset.program) == (*bank, *program))
                            .map(|preset| preset.name.as_ref())
                            .unwrap_or("Unavailable");
                        if ui.small_button(name).clicked() {
                            actions.push(TrackAction::OxiSynth(OxiSynthControl::SelectProgram {
                                channel: self.channel as u8,
                                bank: *bank,
                                program: *program,
                            }));
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Recent:");
                    for (_, bank, program) in self
                        .recent
                        .iter()
                        .filter(|(digest, _, _)| digest == editor.soundfont_sha256.as_ref())
                        .take(8)
                    {
                        let name = editor
                            .presets
                            .iter()
                            .find(|preset| (preset.bank, preset.program) == (*bank, *program))
                            .map(|preset| preset.name.as_ref())
                            .unwrap_or("Unavailable");
                        if ui.small_button(name).clicked() {
                            actions.push(TrackAction::OxiSynth(OxiSynthControl::SelectProgram {
                                channel: self.channel as u8,
                                bank: *bank,
                                program: *program,
                            }));
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            selected_index.is_some_and(|index| index > 0),
                            egui::Button::new("Previous"),
                        )
                        .clicked()
                    {
                        let preset = &editor.presets[selected_index.unwrap() - 1];
                        actions.push(TrackAction::OxiSynth(OxiSynthControl::SelectProgram {
                            channel: self.channel as u8,
                            bank: preset.bank,
                            program: preset.program,
                        }));
                    }
                    if ui
                        .add_enabled(
                            selected_index.is_some_and(|index| index + 1 < editor.presets.len()),
                            egui::Button::new("Next"),
                        )
                        .clicked()
                    {
                        let preset = &editor.presets[selected_index.unwrap() + 1];
                        actions.push(TrackAction::OxiSynth(OxiSynthControl::SelectProgram {
                            channel: self.channel as u8,
                            bank: preset.bank,
                            program: preset.program,
                        }));
                    }
                });
                egui::ComboBox::from_id_salt("oxisynth_preset")
                    .selected_text(format!(
                        "{selected} ({}:{})",
                        channel.current_bank, channel.current_program
                    ))
                    .show_ui(ui, |ui| {
                        let needle = self.search.to_lowercase();
                        for preset in editor.presets.iter().filter(|preset| {
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
                                let key = (
                                    editor.soundfont_sha256.to_string(),
                                    preset.bank,
                                    preset.program,
                                );
                                self.recent.retain(|entry| entry != &key);
                                self.recent.push_front(key);
                                self.recent.truncate(16);
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
                ui.collapsing("All channel assignments", |ui| {
                    egui::Grid::new("oxisynth_channel_overview")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            for (index, channel) in editor.channels.iter().enumerate() {
                                if ui
                                    .selectable_label(
                                        self.channel == index,
                                        format!("Ch {}", index + 1),
                                    )
                                    .clicked()
                                {
                                    self.channel = index;
                                }
                                let name = editor
                                    .presets
                                    .iter()
                                    .find(|preset| {
                                        preset.bank == channel.current_bank
                                            && preset.program == channel.current_program
                                    })
                                    .map(|preset| preset.name.as_ref())
                                    .unwrap_or("Unavailable preset");
                                ui.label(format!(
                                    "{} ({}:{})",
                                    name, channel.current_bank, channel.current_program
                                ));
                                ui.end_row();
                            }
                        });
                });
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
            actions.extend(self.release_audition());
            actions.push(TrackAction::FxVisibilityChanged(false));
        }
        actions
    }

    fn release_audition(&mut self) -> Vec<TrackAction> {
        if !self.auditioning {
            return Vec::new();
        }
        self.auditioning = false;
        vec![TrackAction::OxiSynth(OxiSynthControl::Audition {
            channel: self.channel as u8,
            key: 60,
            velocity: 100,
            pressed: false,
        })]
    }
}
