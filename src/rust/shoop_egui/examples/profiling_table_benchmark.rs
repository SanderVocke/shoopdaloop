pub use shoop_egui::{colors, ProfilingView, StatusState};
use std::time::Instant;
#[path = "../src/profiling.rs"]
mod profiling;

fn main() {
    for nodes in [12, 102, 502] {
        let context = egui::Context::default();
        let mut view = ProfilingView {
            generation: 1,
            active: true,
            requested: true,
            available: true,
            ..Default::default()
        };
        for node in 0..nodes {
            view.live.rows.push(Default::default());
            let row = view.live.rows.last_mut().unwrap();
            row.name = format!("Strip {} audio/MIDI input", node + 1);
            row.samples = 1024;
            row.avg_ns = 1250.;
            row.worst_ns = 3000;
            row.stddev_ns = 500.;
        }
        let mut window = profiling::Window::default();
        window.open = true;
        let mut times = Vec::new();
        for frame in 0..264 {
            let started = Instant::now();
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 900.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    window.show(
                        ui.ctx(),
                        &view,
                        &StatusState {
                            sample_rate: 48000,
                            quantum_frames: 32,
                            dsp_load_percent: 23.4,
                            ..Default::default()
                        },
                        true,
                    );
                },
            );
            let _ = window.take_request();
            output.textures_delta.clear();
            drop(output);
            let elapsed = started.elapsed().as_nanos() as u64;
            if frame >= 8 {
                times.push(elapsed);
            }
        }
        println!("{{\"nodes\":{nodes},\"frame_ns\":{times:?}}}");
    }
}
