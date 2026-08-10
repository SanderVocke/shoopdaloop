use crate::{LoopDetailsState, WaveformWidget};

#[derive(Debug, Default)]
pub struct DetailsPane {
    generation: u64,
    waveforms: Vec<WaveformWidget>,
}

impl DetailsPane {
    pub fn show(&mut self, ui: &mut egui::Ui, details: Option<&LoopDetailsState>) {
        let Some(details) = details else {
            ui.label("Make a selection to show additional details here.");
            return;
        };

        if self.generation != details.generation {
            self.generation = details.generation;
            self.waveforms.clear();
        }

        ui.heading(&details.title);
        if details.loading {
            ui.label("Details will be shown once audio data is available.");
            return;
        }
        if details.channels.is_empty() {
            ui.label("The selected loop has no audio waveform data.");
            return;
        }

        self.waveforms
            .resize_with(details.channels.len(), WaveformWidget::default);
        egui::ScrollArea::vertical()
            .id_salt("details_waveforms")
            .scroll_source(crate::control_safe_scroll_source())
            .show(ui, |ui| {
                for (channel, waveform) in details.channels.iter().zip(&mut self.waveforms) {
                    waveform.show(ui, channel);
                    ui.add_space(4.0);
                }
            });
    }
}
