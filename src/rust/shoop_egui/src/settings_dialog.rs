use std::sync::Arc;

use shoop_settings::{
    SettingEditor, SettingValue, SettingsDraft, SettingsPersistenceState, SettingsRegistry,
    SettingsViewState,
};

#[derive(Clone, Debug)]
pub enum SettingsAction {
    Save(SettingsDraft),
    RecoverWithDefaults,
}

pub struct SettingsDialog {
    registry: Arc<SettingsRegistry>,
    open: bool,
    draft: Option<SettingsDraft>,
}

impl SettingsDialog {
    pub fn new(registry: Arc<SettingsRegistry>) -> Self {
        Self {
            registry,
            open: false,
            draft: None,
        }
    }

    pub fn open(&mut self, state: &SettingsViewState) {
        self.open = true;
        self.draft = Some(SettingsDraft::from_snapshot(&state.active));
    }

    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub fn show(
        &mut self,
        context: &egui::Context,
        state: &SettingsViewState,
    ) -> Vec<SettingsAction> {
        if !self.open {
            return Vec::new();
        }
        if self.draft.is_none() {
            self.draft = Some(SettingsDraft::from_snapshot(&state.active));
        }
        let mut actions = Vec::new();
        let mut open = self.open;
        let mut close_without_save = false;
        egui::Window::new("Settings")
            .id(egui::Id::new("settings_dialog"))
            .open(&mut open)
            .resizable(true)
            .default_size([560.0, 430.0])
            .min_size([320.0, 180.0])
            .show(context, |ui| {
                self.show_status(ui, state, &mut actions);
                ui.separator();
                if state.recovery_required {
                    ui.label(
                        "The stored document must be explicitly replaced before settings can be edited.",
                    );
                    if ui
                        .add_enabled(
                            state.persistence != SettingsPersistenceState::Saving,
                            egui::Button::new("Replace stored settings with defaults"),
                        )
                        .clicked()
                    {
                        actions.push(SettingsAction::RecoverWithDefaults);
                    }
                    return;
                }

                egui::ScrollArea::vertical()
                    .id_salt("settings_values")
                    .show(ui, |ui| self.show_definitions(ui));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Reset all").clicked() {
                        if let Some(draft) = &mut self.draft {
                            draft.reset_all(&self.registry);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Cancel").clicked() {
                            close_without_save = true;
                        }
                        let stale = self
                            .draft
                            .as_ref()
                            .is_some_and(|draft| draft.base_revision() != state.active.revision());
                        let save = ui.add_enabled(
                            state.persistence != SettingsPersistenceState::Saving && !stale,
                            egui::Button::new("Save"),
                        );
                        if save.clicked() {
                            if let Some(action) = self.save_action() {
                                actions.push(action);
                            }
                        }
                        if stale {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                "Settings changed elsewhere; close and reopen this dialog.",
                            );
                        }
                    });
                });
            });
        if close_without_save || !open {
            self.open = false;
            self.draft = None;
        } else {
            self.open = open;
        }
        actions
    }

    fn save_action(&self) -> Option<SettingsAction> {
        self.draft.clone().map(SettingsAction::Save)
    }

    fn show_status(
        &self,
        ui: &mut egui::Ui,
        state: &SettingsViewState,
        _actions: &mut Vec<SettingsAction>,
    ) {
        ui.label(format!("Storage: {}", state.storage_location));
        match state.persistence {
            SettingsPersistenceState::Idle => {}
            SettingsPersistenceState::Saving => {
                ui.spinner();
                ui.label("Saving settings…");
            }
            SettingsPersistenceState::Saved => {
                ui.colored_label(egui::Color32::LIGHT_GREEN, "Settings saved");
            }
            SettingsPersistenceState::Failed => {
                ui.colored_label(egui::Color32::LIGHT_RED, "Settings were not saved");
            }
        }
        for diagnostic in state.diagnostics.iter() {
            ui.colored_label(egui::Color32::YELLOW, &diagnostic.message);
        }
    }

    fn show_definitions(&mut self, ui: &mut egui::Ui) {
        let definitions = self.registry.definitions().to_vec();
        let mut category = None::<String>;
        for definition in definitions {
            if category.as_deref() != Some(definition.category()) {
                if category.is_some() {
                    ui.add_space(8.0);
                }
                ui.heading(definition.category());
                category = Some(definition.category().to_owned());
            }
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong(definition.label());
                        if ui.small_button("Reset").clicked() {
                            if let Some(draft) = &mut self.draft {
                                draft.reset(&definition);
                            }
                        }
                    });
                    ui.label(definition.help());
                    ui.weak(definition.effect().label());
                    if let Some(draft) = &mut self.draft {
                        let Some(value) = draft.value(definition.key()).cloned() else {
                            ui.colored_label(egui::Color32::LIGHT_RED, "Missing draft value");
                            return;
                        };
                        let mut changed = value.clone();
                        match (definition.editor(), &mut changed) {
                            (SettingEditor::Checkbox, SettingValue::Bool(value)) => {
                                ui.checkbox(value, definition.label());
                            }
                            (
                                SettingEditor::UnsignedInteger { min, max },
                                SettingValue::U32(value),
                            ) => {
                                ui.add(egui::DragValue::new(value).range(*min..=*max).speed(1));
                            }
                            (
                                SettingEditor::SignedInteger { min, max },
                                SettingValue::I32(value),
                            ) => {
                                ui.add(egui::DragValue::new(value).range(*min..=*max).speed(1));
                            }
                            (SettingEditor::Number { min, max }, SettingValue::F64(value)) => {
                                ui.add(egui::DragValue::new(value).range(*min..=*max));
                            }
                            (SettingEditor::Text, SettingValue::String(value)) => {
                                ui.text_edit_singleline(value);
                            }
                            _ => {
                                ui.colored_label(
                                    egui::Color32::LIGHT_RED,
                                    "Definition and value types do not match",
                                );
                            }
                        }
                        if changed != value {
                            draft.set_value(definition.key(), changed);
                        }
                    }
                });
        }
    }

    #[cfg(test)]
    pub(crate) fn draft_mut(&mut self) -> Option<&mut SettingsDraft> {
        self.draft.as_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_settings::{
        SettingDefinition, SettingKey, SettingsDiagnostic, SettingsRegistryBuilder,
    };

    const COUNT: SettingKey<u32> = SettingKey::new("test.count");
    const FLAG: SettingKey<bool> = SettingKey::new("test.flag");

    fn fixture() -> (Arc<SettingsRegistry>, SettingsViewState) {
        let mut builder = SettingsRegistryBuilder::default();
        builder
            .register(SettingDefinition::new(
                COUNT,
                2,
                "Track defaults",
                "Count",
                "Default channel count",
            ))
            .unwrap();
        builder
            .register(SettingDefinition::new(
                FLAG,
                false,
                "Track defaults",
                "Flag",
                "Default flag",
            ))
            .unwrap();
        let registry = Arc::new(builder.finish());
        let active = Arc::new(registry.defaults(1));
        (
            Arc::clone(&registry),
            SettingsViewState {
                active,
                diagnostics: Arc::from([]),
                storage_location: "fixture".to_owned(),
                recovery_required: false,
                persistence: SettingsPersistenceState::Idle,
            },
        )
    }

    #[test]
    fn dialog_paints_all_types_at_minimum_and_common_sizes() {
        let (registry, state) = fixture();
        let context = egui::Context::default();
        let mut dialog = SettingsDialog::new(registry);
        dialog.open(&state);
        for size in [egui::vec2(360.0, 200.0), egui::vec2(900.0, 600.0)] {
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    dialog.show(ui.ctx(), &state);
                },
            );
            assert!(!output.shapes.is_empty());
            assert!(dialog.is_open());
        }
    }

    #[test]
    fn save_preserves_stable_typed_draft_and_reset_restores_defaults() {
        let (registry, state) = fixture();
        let mut dialog = SettingsDialog::new(registry.clone());
        dialog.open(&state);
        let draft = dialog.draft_mut().unwrap();
        draft.set(COUNT, 8);
        draft.set(FLAG, true);
        assert_eq!(draft.get(COUNT).unwrap(), 8);
        let SettingsAction::Save(saved) = dialog.save_action().unwrap() else {
            panic!("expected save action");
        };
        assert_eq!(saved.get(COUNT).unwrap(), 8);
        assert!(saved.get(FLAG).unwrap());
        let draft = dialog.draft_mut().unwrap();
        draft.reset(registry.definition(COUNT.id()).unwrap());
        assert_eq!(draft.get(COUNT).unwrap(), 2);
        draft.reset_all(&registry);
        assert!(!draft.get(FLAG).unwrap());
    }

    #[test]
    fn recovery_and_diagnostics_paint_without_editing_rejected_values() {
        let (registry, mut state) = fixture();
        state.recovery_required = true;
        state.persistence = SettingsPersistenceState::Failed;
        state.diagnostics = Arc::from([SettingsDiagnostic {
            key: None,
            message: "future version rejected".to_owned(),
        }]);
        let context = egui::Context::default();
        let mut dialog = SettingsDialog::new(registry);
        dialog.open(&state);
        let output = context.run_ui(Default::default(), |ui| {
            dialog.show(ui.ctx(), &state);
        });
        assert!(!output.shapes.is_empty());
    }
}
