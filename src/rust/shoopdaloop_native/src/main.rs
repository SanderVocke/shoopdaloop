use std::time::Duration;

use eframe::egui;
use shoop_app::{ApplicationHandle, ApplicationRuntime};
use shoop_backend::EngineBackend;
use shoop_egui::AppWidget;

struct NativeApp {
    _runtime: ApplicationRuntime,
    handle: ApplicationHandle,
    widget: AppWidget,
}

impl NativeApp {
    fn new() -> anyhow::Result<Self> {
        let backend = EngineBackend::new_dummy(48_000, 256)?;
        let runtime = ApplicationRuntime::start(Box::new(backend))?;
        let handle = runtime.handle();
        Ok(Self {
            _runtime: runtime,
            handle,
            widget: AppWidget::default(),
        })
    }
}

impl eframe::App for NativeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let snapshot = self.handle.snapshot();
        for intent in self.widget.show(ui, &snapshot) {
            if let Err(error) = self.handle.dispatch(intent) {
                eprintln!("could not dispatch GUI intent: {error}");
            }
        }
        if let Some(notification) = snapshot.notifications.last() {
            egui::Area::new(egui::Id::new("latest_notification"))
                .anchor(egui::Align2::CENTER_TOP, [0.0, 8.0])
                .show(ui.ctx(), |ui| {
                    ui.label(&notification.message);
                });
        }
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ShoopDaLoop")
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([360.0, 200.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ShoopDaLoop",
        options,
        Box::new(|context| {
            shoop_egui::initialize(&context.egui_ctx);
            NativeApp::new()
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|error| error.into())
        }),
    )
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Instant;

    use shoop_egui::{
        AppIntent, DirectTrackSpec, LoopAction, LoopMode, SelectionModifiers, TrackAction,
    };

    use super::*;

    #[test]
    fn dummy_native_workflow_creates_and_controls_tracks_and_loops() {
        let app = NativeApp::new().unwrap();
        app.handle
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Native stereo".to_owned(),
                audio_channels: 2,
                midi: true,
            }))
            .unwrap();
        let started = Instant::now();
        let snapshot = loop {
            let snapshot = app.handle.snapshot();
            if snapshot.tracks.len() == 2 {
                break snapshot;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        };
        let track = &snapshot.tracks[1];
        assert_eq!(track.loops.len(), 8);
        assert!(track.controls.output_stereo);
        let track_id = track.id;
        let loop_id = track.loops[0].id;
        app.handle
            .dispatch(AppIntent::Track {
                track_id,
                action: TrackAction::OutputGainChanged(-3.0),
            })
            .unwrap();
        app.handle
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::IconClicked(SelectionModifiers::default()),
            })
            .unwrap();
        app.handle
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        let started = Instant::now();
        loop {
            let snapshot = app.handle.snapshot();
            if snapshot.tracks[1].controls.output_gain_db == -3.0
                && (snapshot.tracks[1].loops[0].mode == LoopMode::Playing
                    || snapshot.tracks[1].loops[0].next_mode == LoopMode::Playing)
                && snapshot.details.is_some()
            {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        }
    }
}
