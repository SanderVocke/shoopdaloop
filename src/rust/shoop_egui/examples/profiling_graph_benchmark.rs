pub use shoop_egui::{
    colors, diagnostics_protocol, task_profiling, DiagnosticsView, ProfilingView, StatusState,
};
#[path = "../src/profiling.rs"]
mod profiling;

use shoop_app_api::profiling_data::{
    Collector, GraphEdge, GraphNode, Group, Packet, Phase, Port, Sample, Status, Topology,
    CALLBACK, CAPTURE_END, CHUNK_SAMPLES, HOST, HOST_XRUN, PHASE,
};
use std::{sync::Arc, time::Instant};

fn fixture(nodes: usize, dense: bool) -> (Collector, ProfilingView) {
    let port = |node| Port {
        node,
        kind: 0,
        channel: 0,
    };
    let mut edges = Vec::new();
    for key in 0..nodes as u64 {
        if dense && key > 0 {
            for distance in [1, 4, 16, 64] {
                if key >= distance {
                    edges.push(GraphEdge {
                        source: port(Some(key - distance)),
                        destination: port(Some(key)),
                    });
                }
            }
            continue;
        }
        edges.push(GraphEdge {
            source: port((key > 0).then_some((key.saturating_sub(1)) / 2)),
            destination: port(Some(key)),
        });
    }
    edges.push(GraphEdge {
        source: port(Some(nodes as u64 - 1)),
        destination: port(None),
    });
    let mut collector = Collector::default();
    collector.status(Status {
        generation: 1,
        requested: true,
        active: true,
        available: true,
        resolution_ns: 1,
        ..Status::default()
    });
    for key in 0..nodes as u64 {
        collector.name(1, key, &format!("Processor {}", key + 1));
    }
    for tick in 0..32u64 {
        let start = 1_000_000 + tick * 10_000_000;
        let mut at = start;
        let mut samples = vec![Sample {
            key: PHASE | Phase::GraphStart as u64,
            at,
        }];
        for key in 0..nodes as u64 {
            at += if key == 5 {
                if tick == 31 {
                    350_000
                } else {
                    180_000 + tick % 5 * 1000
                }
            } else if tick == 31 {
                4000
            } else {
                1000 + tick % 3 * 100
            };
            samples.push(Sample { key, at });
        }
        samples.push(Sample {
            key: PHASE | Phase::OutputRouting as u64,
            at,
        });
        for (i, chunk) in samples.chunks(CHUNK_SAMPLES).enumerate() {
            let mut packet = Packet {
                generation: 1,
                sequence: tick + 1,
                epoch: 1,
                frame: tick * 32,
                frames: 32,
                start_ns: start,
                end_ns: at,
                index: (i * CHUNK_SAMPLES) as u32,
                total: samples.len() as u32,
                len: chunk.len() as u8,
                ..Packet::default()
            };
            packet.samples[..chunk.len()].copy_from_slice(chunk);
            collector.receive(packet);
            packet.capture = 7;
            packet.trigger = HOST_XRUN;
            collector.receive(packet);
        }
        let mut callback = Packet {
            generation: 1,
            capture: 7,
            sequence: 1000 + tick,
            callback: 100 + tick,
            epoch: 1,
            frame: tick * 32,
            callback_frame: tick * 32,
            frames: 32,
            start_ns: start - if tick == 31 { 250_000 } else { 10_000 },
            end_ns: at + 1000,
            budget_ns: 666_667,
            graphs: 1,
            total: 2,
            len: 2,
            flags: CALLBACK | HOST | if tick == 31 { CAPTURE_END } else { 0 },
            trigger: HOST_XRUN,
            ..Packet::default()
        };
        callback.samples[0] = Sample {
            key: PHASE | Phase::HostInput as u64,
            at: start,
        };
        callback.samples[1] = Sample {
            key: PHASE | Phase::HostOutput as u64,
            at: at + 1000,
        };
        collector.receive(callback);
        callback.capture = 0;
        callback.flags = CALLBACK | HOST;
        collector.receive(callback);
    }
    let mut view = collector.view();
    view.topology = Some(Arc::new(Topology {
        generation: 1,
        epoch: 1,
        sample_rate: 48000,
        quantum: 32,
        nodes: (0..nodes as u64)
            .map(|key| GraphNode {
                key,
                group: 1,
                slot: (key / 3) as u32,
                owner_generation: 1,
            })
            .collect(),
        edges,
    }));
    view.groups = (0..nodes.div_ceil(3))
        .map(|slot| Group {
            kind: 1,
            slot: slot as u32,
            entity: slot as u64 + 1,
            name: format!("Track {}", slot + 1),
        })
        .collect::<Vec<_>>()
        .into();
    (collector, view)
}

fn status() -> StatusState {
    StatusState {
        sample_rate: 48000,
        quantum_frames: 32,
        dsp_load_percent: 38.6,
        ..StatusState::default()
    }
}

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::args().any(|a| a == "--window") {
        struct Preview {
            window: profiling::Window,
            view: ProfilingView,
            collector: Collector,
        }
        impl eframe::App for Preview {
            fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
                ui.label("Synthetic profiling graph fixture — rendering and interaction preview");
                self.window.show(
                    ui.ctx(),
                    &self.view,
                    &status(),
                    true,
                    &DiagnosticsView::default(),
                );
                if let Some(request) = self.window.take_request() {
                    if self.collector.select(request) {
                        self.view.detail = self.collector.view().detail;
                        ui.ctx().request_repaint();
                    }
                }
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
                let (collector, view) = fixture(12, false);
                Ok(Box::new(Preview {
                    window,
                    view,
                    collector,
                }))
            }),
        )
        .unwrap();
        return;
    }
    let dense = std::env::args().any(|a| a == "--dense");
    let sizes = if dense {
        vec![4094]
    } else {
        vec![12, 102, 502]
    };
    for nodes in sizes {
        let context = egui::Context::default();
        let (_, view) = fixture(nodes, dense);
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
                        window.show(
                            ui.ctx(),
                            &view,
                            &status(),
                            true,
                            &DiagnosticsView::default(),
                        );
                    },
                )
                .textures_delta
                .clear();
            let _ = window.take_request();
            let _ = window.take_control();
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
