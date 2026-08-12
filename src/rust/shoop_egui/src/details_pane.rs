use crate::{CompositeLoopWidget, LoopDetailsState, LoopId, MidiSequenceWidget, WaveformWidget};

#[derive(Debug, Default)]
pub struct DetailsPane {
    loop_id: LoopId,
    waveforms: Vec<WaveformWidget>,
    midi_sequences: Vec<MidiSequenceWidget>,
    composite: CompositeLoopWidget,
}

impl DetailsPane {
    pub fn show(&mut self, ui: &mut egui::Ui, details: Option<&LoopDetailsState>) {
        let Some(details) = details else {
            ui.label("Make a selection to show additional details here.");
            return;
        };

        if self.loop_id != details.loop_id {
            self.loop_id = details.loop_id;
            self.waveforms.clear();
            self.midi_sequences.clear();
        }

        ui.heading(&details.title);
        if let Some(composite) = &details.composite {
            self.composite.show(ui, details.loop_id, composite);
            return;
        }
        if details.loading {
            ui.label("Audio waveform data is loading.");
        }
        if details.midi_loading {
            ui.label("MIDI data is loading.");
        }
        if !details.loading
            && !details.midi_loading
            && details.channels.is_empty()
            && details.midi_channels.is_empty()
        {
            ui.label("The selected loop has no audio or MIDI data.");
            return;
        }

        self.waveforms
            .resize_with(details.channels.len(), WaveformWidget::default);
        self.midi_sequences
            .resize_with(details.midi_channels.len(), MidiSequenceWidget::default);
        egui::ScrollArea::vertical()
            .id_salt("details_media")
            .scroll_source(crate::control_safe_scroll_source())
            .show(ui, |ui| {
                for (channel, waveform) in details.channels.iter().zip(&mut self.waveforms) {
                    waveform.show(ui, channel);
                    ui.add_space(4.0);
                }
                for (channel, sequence) in
                    details.midi_channels.iter().zip(&mut self.midi_sequences)
                {
                    sequence.show(ui, channel);
                    ui.add_space(4.0);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CompositeDetailsState, CompositeEventDetailsState, CompositeKind,
        CompositeTrackDetailsState, MidiEventState, MidiSequenceChannelState, TrackId,
        WaveformChannelState,
    };
    use std::sync::Arc;

    #[test]
    fn composite_details_take_precedence_over_primitive_empty_state() {
        let context = egui::Context::default();
        let mut pane = DetailsPane::default();
        let loop_id = LoopId::from_raw(7);
        let details = LoopDetailsState {
            loop_id,
            title: "Arrangement".to_owned(),
            composite: Some(CompositeDetailsState {
                kind: CompositeKind::Script,
                cycle_length_frames: 100,
                timeline_length_frames: 200,
                tracks: vec![CompositeTrackDetailsState {
                    id: TrackId::from_raw(2),
                    name: "Rhythm".to_owned(),
                }],
                events: vec![CompositeEventDetailsState {
                    loop_id: LoopId::from_raw(9),
                    loop_name: "Beat".to_owned(),
                    track_id: TrackId::from_raw(2),
                    start_frame: 0,
                    end_frame: 200,
                    ..Default::default()
                }],
            }),
            ..Default::default()
        };
        let _ = context.run_ui(Default::default(), |ui| pane.show(ui, Some(&details)));
        assert_eq!(pane.composite.shown_loop_id(), loop_id);
        assert_eq!(pane.composite.rendered_event_count(), 1);
        assert!(pane.waveforms.is_empty());
        assert!(pane.midi_sequences.is_empty());
    }

    #[test]
    fn midi_only_and_mixed_details_create_the_expected_lanes() {
        let context = egui::Context::default();
        let mut pane = DetailsPane::default();
        let midi = MidiSequenceChannelState {
            label: "MIDI 1".to_owned(),
            events: Arc::from([
                MidiEventState {
                    frame: 1,
                    data: Arc::from([0x90, 60, 100]),
                },
                MidiEventState {
                    frame: 8,
                    data: Arc::from([0x80, 60, 0]),
                },
            ]),
            loop_length: 16,
            ..Default::default()
        };
        let midi_only = LoopDetailsState {
            loop_id: LoopId::from_raw(1),
            title: "MIDI only".to_owned(),
            midi_channels: vec![midi.clone()],
            ..Default::default()
        };
        let _ = context.run_ui(Default::default(), |ui| pane.show(ui, Some(&midi_only)));
        assert!(pane.waveforms.is_empty());
        assert_eq!(pane.midi_sequences.len(), 1);

        let mixed = LoopDetailsState {
            loop_id: LoopId::from_raw(2),
            title: "Mixed".to_owned(),
            channels: vec![WaveformChannelState {
                samples: Arc::from([0.25, -0.25]),
                ..Default::default()
            }],
            midi_channels: vec![midi],
            ..Default::default()
        };
        let _ = context.run_ui(Default::default(), |ui| pane.show(ui, Some(&mixed)));
        assert_eq!(pane.waveforms.len(), 1);
        assert_eq!(pane.midi_sequences.len(), 1);
    }
}
