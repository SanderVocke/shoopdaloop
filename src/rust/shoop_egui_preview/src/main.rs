use eframe::egui;
use shoop_app_api::{
    AppIntent, AppSnapshot, DirectTrackSpec, LoopId, LoopMode, LoopState, TrackAction,
    TrackControlState, TrackId, TrackState,
};
use shoop_egui::AppWidget;

struct PreviewApp {
    widget: AppWidget,
    snapshot: AppSnapshot,
    next_track_id: u64,
    next_loop_id: u64,
    last_intent: String,
}

impl Default for PreviewApp {
    fn default() -> Self {
        Self {
            widget: AppWidget::default(),
            snapshot: representative_snapshot(),
            next_track_id: 5,
            next_loop_id: 30,
            last_intent: "No intent yet".to_owned(),
        }
    }
}

impl eframe::App for PreviewApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let intents = self.widget.show(ui, &self.snapshot);
        for intent in intents {
            self.last_intent = format!("{intent:?}");
            self.apply(intent);
        }
        egui::Area::new(egui::Id::new("preview_intent_log"))
            .anchor(egui::Align2::RIGHT_TOP, [-8.0, 8.0])
            .show(ui.ctx(), |ui| {
                ui.label(egui::RichText::new(&self.last_intent).small());
            });
    }
}

impl PreviewApp {
    fn apply(&mut self, intent: AppIntent) {
        match intent {
            AppIntent::AddTrack(spec) => self.add_track(spec),
            AppIntent::AddLoop { track_id } => {
                let id = LoopId::from_raw(self.next_loop_id);
                self.next_loop_id += 1;
                if let Some(track) = self
                    .snapshot
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == track_id)
                {
                    track.loops.push(LoopState {
                        id,
                        name: format!("({})", track.loops.len() + 1),
                        ..Default::default()
                    });
                }
            }
            AppIntent::Track { track_id, action } => {
                if let Some(track) = self
                    .snapshot
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == track_id)
                {
                    match action {
                        TrackAction::NameChanged(value) => track.name = value,
                        TrackAction::OutputGainChanged(value) => {
                            track.controls.output_gain_db = value
                        }
                        TrackAction::OutputBalanceChanged(value) => {
                            track.controls.output_balance = value
                        }
                        TrackAction::OutputMuteChanged(value) => {
                            track.controls.output_muted = value
                        }
                        TrackAction::InputGainChanged(value) => {
                            track.controls.input_gain_db = value
                        }
                        TrackAction::InputBalanceChanged(value) => {
                            track.controls.input_balance = value
                        }
                        TrackAction::InputMonitoringChanged(value) => {
                            track.controls.input_monitoring = value
                        }
                    }
                }
            }
            AppIntent::Loop {
                loop_id, action, ..
            } => {
                let loops = self
                    .snapshot
                    .tracks
                    .iter_mut()
                    .flat_map(|track| &mut track.loops);
                if let Some(loop_state) = loops.into_iter().find(|state| state.id == loop_id) {
                    match action {
                        shoop_app_api::LoopAction::IconClicked(_) => {
                            loop_state.selected = !loop_state.selected
                        }
                        shoop_app_api::LoopAction::IconDoubleClicked => {
                            loop_state.targeted = !loop_state.targeted
                        }
                        shoop_app_api::LoopAction::PlayClicked => {
                            loop_state.mode = LoopMode::Playing
                        }
                        shoop_app_api::LoopAction::RecordClicked => {
                            loop_state.mode = LoopMode::Recording
                        }
                        shoop_app_api::LoopAction::StopClicked => {
                            loop_state.mode = LoopMode::Stopped
                        }
                        shoop_app_api::LoopAction::GainChanged(value) => loop_state.gain = value,
                    }
                }
            }
            AppIntent::Global(action) => match action {
                shoop_app_api::GlobalControlAction::SetDefaultRecordingAction(value) => {
                    self.snapshot.global_controls.default_recording_action = value
                }
                shoop_app_api::GlobalControlAction::SetPlayAfterRecord(value) => {
                    self.snapshot.global_controls.play_after_record = value
                }
                shoop_app_api::GlobalControlAction::SetSync(value) => {
                    self.snapshot.global_controls.sync = value
                }
                shoop_app_api::GlobalControlAction::SetSolo(value) => {
                    self.snapshot.global_controls.solo = value
                }
                shoop_app_api::GlobalControlAction::SetApplyNCycles(value) => {
                    self.snapshot.global_controls.apply_n_cycles = value
                }
                _ => {}
            },
        }
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
    }

    fn add_track(&mut self, spec: DirectTrackSpec) {
        let track_id = TrackId::from_raw(self.next_track_id);
        self.next_track_id += 1;
        let loops = (0..8)
            .map(|index| {
                let id = LoopId::from_raw(self.next_loop_id);
                self.next_loop_id += 1;
                LoopState {
                    id,
                    name: format!("({})", index + 1),
                    stereo: spec.audio_channels == 2,
                    show_gain: spec.audio_channels > 0,
                    ..Default::default()
                }
            })
            .collect();
        self.snapshot.tracks.push(TrackState {
            id: track_id,
            name: spec.name,
            loops,
            controls: controls(spec.audio_channels, spec.midi),
            ..Default::default()
        });
    }
}

fn controls(audio_channels: u8, midi: bool) -> TrackControlState {
    TrackControlState {
        has_output: audio_channels > 0 || midi,
        has_output_audio: audio_channels > 0,
        output_stereo: audio_channels == 2,
        output_midi_activity: midi,
        has_input: audio_channels > 0 || midi,
        has_input_audio: audio_channels > 0,
        input_stereo: audio_channels == 2,
        input_midi_activity: midi,
        ..Default::default()
    }
}

fn track(id: u64, name: &str, audio_channels: u8, midi: bool, loop_base: u64) -> TrackState {
    TrackState {
        id: TrackId::from_raw(id),
        name: name.to_owned(),
        loops: (0..8)
            .map(|index| LoopState {
                id: LoopId::from_raw(loop_base + index),
                name: format!("({})", index + 1),
                stereo: audio_channels == 2,
                show_gain: audio_channels > 0,
                midi_activity: midi && index == 0,
                ..Default::default()
            })
            .collect(),
        controls: controls(audio_channels, midi),
        ..Default::default()
    }
}

fn representative_snapshot() -> AppSnapshot {
    AppSnapshot {
        tracks: vec![
            TrackState {
                id: TrackId::from_raw(1),
                name: "Sync".to_owned(),
                is_sync: true,
                loops: vec![LoopState {
                    id: LoopId::from_raw(1),
                    name: "sync loop".to_owned(),
                    sync: true,
                    show_gain: true,
                    ..Default::default()
                }],
                controls: controls(1, false),
            },
            track(2, "Stereo audio + MIDI", 2, true, 2),
            track(3, "Mono audio", 1, false, 10),
            track(4, "MIDI only", 0, true, 20),
        ],
        status: shoop_app_api::StatusState {
            version: "preview".to_owned(),
            dsp_load_percent: 12.5,
            buffer_size: 256,
            sample_rate: 48_000,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ShoopDaLoop egui preview")
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([360.0, 200.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ShoopDaLoop egui preview",
        options,
        Box::new(|context| {
            shoop_egui::initialize(&context.egui_ctx);
            Ok(Box::new(PreviewApp::default()))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_contains_all_representative_track_shapes() {
        let snapshot = representative_snapshot();
        assert!(snapshot.tracks.iter().any(|track| track.is_sync));
        assert!(snapshot
            .tracks
            .iter()
            .any(|track| { track.controls.output_stereo && track.controls.output_midi_activity }));
        assert!(snapshot
            .tracks
            .iter()
            .any(|track| { track.controls.has_output_audio && !track.controls.output_stereo }));
        assert!(snapshot.tracks.iter().any(|track| {
            !track.controls.has_output_audio && track.controls.output_midi_activity
        }));
    }
}
