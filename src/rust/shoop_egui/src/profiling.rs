#[path = "profiling/graph.rs"]
mod graph;

use crate::{colors, ProfilingView, StatusState};

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum Units {
    #[default]
    Microseconds,
    Budget,
}
impl Units {
    fn format(self, ns: f64, budget: Option<f64>) -> String {
        match self {
            Self::Microseconds => format!("{:.3}", ns / 1000.),
            Self::Budget => {
                budget.map_or_else(|| "—".into(), |budget| format!("{:.2}", 100. * ns / budget))
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct Window {
    pub open: bool,
    capture: bool,
    units: Units,
    graph: graph::State,
    table: bool,
}
impl Window {
    pub fn take_request(&mut self) -> Option<shoop_app_api::profiling_data::CaptureRequest> {
        self.graph.request.take()
    }
    pub fn show(
        &mut self,
        context: &egui::Context,
        view: &ProfilingView,
        status: &StatusState,
        enabled: bool,
    ) -> Option<bool> {
        let mut open = self.open;
        let mut changed = None;
        egui::Window::new("Runtime profiling")
            .open(&mut open)
            .default_size([960., 700.])
            .min_size([510., 300.])
            .collapsible(false)
            .show(context, |ui| {
                changed = show_inner(
                    ui,
                    view,
                    status,
                    &mut self.capture,
                    &mut self.units,
                    enabled,
                    Some(&mut self.graph),
                    &mut self.table,
                );
            });
        self.open = open;
        changed
    }
}

fn switch(ui: &mut egui::Ui, value: &mut bool) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(egui::vec2(40., 22.), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::Role::CheckBox,
            ui.is_enabled(),
            *value,
            "Runtime profiling",
        )
    });
    let position = ui.ctx().animate_bool(response.id, *value);
    let visuals = ui.style().interact(&response);
    let fill = if *value {
        colors::COLORED_HIGHLIGHT
    } else {
        visuals.bg_fill
    };
    ui.painter().rect(
        rect.shrink(1.),
        11.,
        fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let center = egui::pos2(
        egui::lerp(rect.left() + 11. ..=rect.right() - 11., position),
        rect.center().y,
    );
    ui.painter().circle_filled(center, 7., colors::FOREGROUND);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
pub(crate) fn show(
    ui: &mut egui::Ui,
    view: &ProfilingView,
    status: &StatusState,
    capture: &mut bool,
    units: &mut Units,
    controls_enabled: bool,
) -> Option<bool> {
    show_inner(
        ui,
        view,
        status,
        capture,
        units,
        controls_enabled,
        None,
        &mut true,
    )
}

fn show_inner(
    ui: &mut egui::Ui,
    view: &ProfilingView,
    status: &StatusState,
    capture: &mut bool,
    units: &mut Units,
    controls_enabled: bool,
    graph: Option<&mut graph::State>,
    table: &mut bool,
) -> Option<bool> {
    let (sample_rate, quantum_frames) = view
        .topology
        .as_ref()
        .map_or((status.sample_rate, status.quantum_frames), |t| {
            (t.sample_rate, t.quantum)
        });
    let budget = view
        .topology
        .as_ref()
        .and_then(|t| t.budget_ns())
        .or_else(|| {
            (status.sample_rate > 0 && status.quantum_frames > 0)
                .then(|| f64::from(status.quantum_frames) * 1e9 / f64::from(status.sample_rate))
        });
    let mut changed = None;
    ui.horizontal(|ui| {
        let mut requested = view.requested;
        if ui
            .add_enabled_ui(controls_enabled, |ui| switch(ui, &mut requested))
            .inner
            .changed()
        {
            changed = Some(requested);
        }
        ui.strong(if !view.available && view.requested {
            "Timing unavailable"
        } else if view.requested != view.active {
            "Applying…"
        } else if view.active {
            "Profiling on"
        } else {
            "Profiling off"
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(budget.is_some(), |ui| {
                ui.selectable_value(units, Units::Budget, "% budget");
            });
            ui.selectable_value(units, Units::Microseconds, "µs");
        });
    });
    ui.add_space(12.);
    egui::Frame::group(ui.style())
        .inner_margin(12)
        .corner_radius(8)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Overall DSP");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.monospace(format!("{:.1}%", status.dsp_load_percent));
                });
            });
            ui.add(
                egui::ProgressBar::new((status.dsp_load_percent / 100.).clamp(0., 1.))
                    .desired_width(ui.available_width())
                    .desired_height(8.)
                    .corner_radius(4)
                    .fill(if status.dsp_load_percent > 100. {
                        colors::ERROR
                    } else {
                        colors::COLORED_HIGHLIGHT
                    }),
            );
            if let Some(budget) = budget {
                ui.small(format!(
                    "{} frames @ {} Hz · {:.3} µs per quantum",
                    quantum_frames,
                    sample_rate,
                    budget / 1000.
                ));
            }
        });
    ui.add_space(10.);
    ui.horizontal(|ui| {
        ui.selectable_value(table, false, "Graph");
        ui.selectable_value(table, true, "Table");
        ui.separator();
        ui.selectable_value(capture, false, "Live");
        ui.add_enabled_ui(view.capture.is_some(), |ui| {
            ui.selectable_value(capture, true, "Latest xrun");
        });
    });
    let report = if *capture {
        match &view.capture {
            Some(report) => report,
            None => {
                *capture = false;
                &view.live
            }
        }
    } else {
        &view.live
    };
    if !view.available {
        ui.weak("This host has no elapsed-time profiling clock.");
    }
    if report.rows.is_empty() && (*table || graph.is_none() || view.topology.is_none()) {
        ui.add_space(28.);
        ui.vertical_centered(|ui| {
            ui.strong("No measured graph node samples");
            ui.weak(if !view.available {
                "Timing measurements require a host with an elapsed-time clock."
            } else if view.requested {
                "Waiting for timing data from the host."
            } else {
                "Turn on profiling to measure this graph."
            });
        });
        return changed;
    }
    let seconds = report
        .interval
        .end_ns
        .saturating_sub(report.interval.start_ns) as f64
        / 1e9;
    let min_samples = report.rows.iter().map(|r| r.samples).min().unwrap_or(0);
    let max_samples = report.rows.iter().map(|r| r.samples).max().unwrap_or(0);
    ui.horizontal(|ui| {
        ui.label(format!(
            "{seconds:.3} s measured · {} nodes",
            report.rows.len()
        ));
        ui.weak(if min_samples == max_samples {
            format!("{min_samples} samples / node")
        } else {
            format!("{min_samples}–{max_samples} samples / node")
        });
    })
    .response
    .on_hover_text(format!(
        "Frames {}–{} · generation {}",
        report.interval.first_frame, report.interval.end_frame, report.generation
    ));
    if report.incomplete || view.dropped != 0 {
        ui.colored_label(
            colors::WARNING,
            format!(
                "Incomplete timing data · {} live batches dropped",
                view.dropped
            ),
        );
    }
    if view.skipped_captures != 0 {
        ui.weak(format!(
            "{} further xrun captures could not be retained",
            view.skipped_captures
        ));
    }
    if report.capture != 0 {
        let mut causes = Vec::new();
        if report.trigger & 1 != 0 {
            causes.push("host xrun (callback attribution may be delayed)");
        }
        if report.trigger & 2 != 0 {
            causes.push("measured callback budget overrun");
        }
        ui.weak(format!(
            "Capture {} · {} · {}",
            report.capture,
            if report.complete {
                "received"
            } else {
                "receiving…"
            },
            causes.join(", ")
        ));
    }
    ui.add_space(8.);
    if !*table {
        if let Some(graph) = graph {
            graph.show(ui, view, report, *capture, *units);
            return changed;
        }
    }
    ui.weak(match units { Units::Microseconds => "Node durations in µs", Units::Budget => "Node durations as % of one quantum's DSP time budget" })
        .on_hover_text("Includes input preparation, routing, processing and output recovery. Elapsed time includes preemption. The overall DSP bar measures complete callbacks.");
    egui::Frame::group(ui.style())
        .inner_margin(8)
        .corner_radius(8)
        .show(ui, |ui| {
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .max_height((ui.available_height() - 26.).max(100.))
                .show(ui, |ui| {
                    let name_width = (ui.available_width() - 3. * 96. - 3. * 20.).max(160.);
                    egui::Grid::new("runtime_profiling_table")
                        .striped(true)
                        .spacing([20., 7.])
                        .min_col_width(76.)
                        .show(ui, |ui| {
                            ui.strong("Node");
                            for title in ["Avg", "Worst", "Stddev"] {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(96., 20.),
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.strong(title);
                                    },
                                );
                            }
                            ui.end_row();
                            let names = view.node_names(report);
                            for row in &report.rows {
                                let name = &names[&row.key];
                                ui.allocate_ui_with_layout(
                                    egui::vec2(name_width, 20.),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(name).color(colors::FOREGROUND),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(name);
                                    },
                                );
                                for ns in [row.avg_ns, row.worst_ns as f64, row.stddev_ns] {
                                    let text = if row.samples == 0 {
                                        "—".into()
                                    } else {
                                        units.format(ns, budget)
                                    };
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(96., 20.),
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new(text)
                                                    .monospace()
                                                    .color(colors::FOREGROUND),
                                            );
                                        },
                                    );
                                }
                                ui.end_row();
                            }
                        });
                });
        });
    if view.available {
        ui.add_space(5.);
        ui.weak(if view.resolution_ns == 0 {
            "Clock resolution unknown".into()
        } else {
            format!(
                "Observed clock step ≈ {:.3} µs",
                view.resolution_ns as f64 / 1000.
            )
        });
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    fn texts(shape: &egui::Shape, output: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => output.push(text.galley.job.text.clone()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    texts(shape, output);
                }
            }
            _ => {}
        }
    }
    #[shoop_test]
    fn switch_emits_an_immediate_request_and_respects_connection_state() {
        let context = egui::Context::default();
        let view = ProfilingView {
            available: true,
            ..Default::default()
        };
        let click = |enabled: bool| {
            let mut changed = None;
            for button in [None, Some(true), Some(false)] {
                let position = egui::pos2(25., 18.);
                let mut events = vec![egui::Event::PointerMoved(position)];
                if let Some(pressed) = button {
                    events.push(egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    });
                }
                let mut output = context.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        changed = show(
                            ui,
                            &view,
                            &StatusState::default(),
                            &mut false,
                            &mut Units::Microseconds,
                            enabled,
                        )
                        .or(changed);
                    },
                );
                output.textures_delta.clear();
            }
            changed
        };
        assert_eq!(click(true), Some(true));
        assert!(!view.requested);
        assert_eq!(click(false), None);
    }
    #[shoop_test]
    fn table_renders_known_statistics_and_clears_on_generation_reset() {
        let context = egui::Context::default();
        let mut profiling = ProfilingView {
            generation: 1,
            available: true,
            ..Default::default()
        };
        profiling.live.rows.push(Default::default());
        let row = &mut profiling.live.rows[0];
        row.name = "Loop 1 MIDI playback".into();
        row.samples = 4;
        row.avg_ns = 1250.;
        row.worst_ns = 3000;
        row.stddev_ns = 500.;
        let render = |profiling: &ProfilingView, mut capture: bool, mut units: Units| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 900.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    show(
                        ui,
                        profiling,
                        &StatusState {
                            sample_rate: 48000,
                            quantum_frames: 32,
                            dsp_load_percent: 23.4,
                            ..Default::default()
                        },
                        &mut capture,
                        &mut units,
                        true,
                    );
                },
            );
            output.textures_delta.clear();
            let mut labels = Vec::new();
            for shape in output.shapes {
                texts(&shape.shape, &mut labels);
            }
            labels
        };
        let labels = render(&profiling, false, Units::Microseconds);
        for expected in [
            "Node",
            "Avg",
            "Worst",
            "Stddev",
            "Loop 1 MIDI playback",
            "1.250",
            "3.000",
            "0.500",
        ] {
            assert!(
                labels.iter().any(|text| text == expected),
                "missing {expected}: {labels:?}"
            );
        }
        let percentages = render(&profiling, false, Units::Budget);
        for expected in ["0.19", "0.45", "23.4%"] {
            assert!(
                percentages.iter().any(|text| text == expected),
                "missing {expected}: {percentages:?}"
            );
        }
        profiling.capture = Some(profiling.live.clone());
        profiling.capture.as_mut().unwrap().rows[0].name = "Captured Loop 1".into();
        let labels = render(&profiling, true, Units::Microseconds);
        assert!(labels.iter().any(|text| text == "Captured Loop 1"));
        assert!(!labels.iter().any(|text| text == "Loop 1 MIDI playback"));
        profiling.generation = 2;
        profiling.live.rows.clear();
        profiling.capture = None;
        profiling.available = false;
        profiling.requested = true;
        let labels = render(&profiling, false, Units::Microseconds);
        assert!(!labels
            .iter()
            .any(|text| text.contains("Loop 1 MIDI playback")));
        assert!(labels.iter().any(|text| text.contains("No measured")));
        assert!(labels
            .iter()
            .any(|text| text.contains("no elapsed-time profiling clock")));
    }
}
