pub use shoop_egui::{colors, ProfilingView, StatusState};
#[path = "../src/profiling.rs"]
mod profiling;

use shoop_app_api::profiling_data::{GraphEdge, GraphNode, Port, Report, Row, Topology};
use std::{sync::Arc, time::Instant};

fn fixture(nodes: usize) -> ProfilingView {
    let port = |node| Port {
        node,
        kind: 0,
        channel: 0,
    };
    let mut edges = Vec::new();
    for key in 0..nodes as u64 {
        edges.push(GraphEdge {
            source: port((key > 0).then_some((key.saturating_sub(1)) / 2)),
            destination: port(Some(key)),
        });
    }
    edges.push(GraphEdge {
        source: port(Some(nodes as u64 - 1)),
        destination: port(None),
    });
    ProfilingView {
        generation: 1,
        requested: true,
        active: true,
        available: true,
        resolution_ns: 1,
        topology: Some(Arc::new(Topology {
            generation: 1,
            epoch: 1,
            sample_rate: 48000,
            quantum: 32,
            nodes: (0..nodes as u64)
                .map(|key| GraphNode {
                    key,
                    group: 1,
                    slot: (key / 3) as u32,
                })
                .collect(),
            edges,
        })),
        live: Report {
            generation: 1,
            epoch: 1,
            rows: (0..nodes as u64)
                .map(|key| Row {
                    key,
                    name: format!("Processor {}", key + 1),
                    samples: 1024,
                    avg_ns: if key == 5 { 180_000. } else { 1000. },
                    worst_ns: if key == 5 { 350_000 } else { 4000 },
                    stddev_ns: 300.,
                    ready_avg_ns: key as f64 * 1000. + if key >= 5 { 179_000. } else { 0. },
                    ready_worst_ns: key * 4000 + if key >= 5 { 346_000 } else { 0 },
                    ready_stddev_ns: 2000.,
                })
                .collect(),
            ..Report::default()
        },
        ..ProfilingView::default()
    }
}

fn status() -> StatusState {
    StatusState {
        sample_rate: 48000,
        quantum_frames: 32,
        dsp_load_percent: 38.6,
        ..StatusState::default()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if std::env::args().any(|a| a == "--window") {
        struct Preview {
            window: profiling::Window,
            view: ProfilingView,
        }
        impl eframe::App for Preview {
            fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
                ui.label("Synthetic profiling graph fixture — rendering and interaction preview");
                self.window.show(ui.ctx(), &self.view, &status(), true);
                let _ = self.window.take_request();
            }
        }
        eframe::run_native(
            "Profiling graph preview",
            eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default().with_inner_size([1200., 900.]),
                ..Default::default()
            },
            Box::new(|_| {
                let mut window = profiling::Window::default();
                window.open = true;
                Ok(Box::new(Preview {
                    window,
                    view: fixture(12),
                }))
            }),
        )
        .unwrap();
        return;
    }
    for nodes in [12, 102, 502] {
        let context = egui::Context::default();
        let view = fixture(nodes);
        let mut window = profiling::Window::default();
        window.open = true;
        let mut times = Vec::new();
        let mut layout_ns = 0;
        for frame in 0..72 {
            let start = Instant::now();
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1200., 900.),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        window.show(ui.ctx(), &view, &status(), true);
                    },
                )
                .textures_delta
                .clear();
            let _ = window.take_request();
            let elapsed = start.elapsed().as_nanos() as u64;
            if frame == 0 {
                layout_ns = elapsed;
            }
            if frame >= 8 {
                times.push(elapsed);
            }
        }
        println!("{{\"nodes\":{nodes},\"initial_frame_ns\":{layout_ns},\"frame_ns\":{times:?}}}");
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}
