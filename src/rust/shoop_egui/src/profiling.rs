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
    pub(super) host: bool,
    control: Option<crate::diagnostics_protocol::Request>,
}
impl Window {
    pub fn take_control(&mut self) -> Option<crate::diagnostics_protocol::Request> {
        self.control.take()
    }

    pub fn take_request(&mut self) -> Option<shoop_app_api::profiling_data::CaptureRequest> {
        self.graph.request.take()
    }
    pub fn show(
        &mut self,
        context: &egui::Context,
        view: &ProfilingView,
        status: &StatusState,
        enabled: bool,
        diagnostics: &crate::DiagnosticsView,
    ) -> Option<bool> {
        let mut open = self.open;
        let mut changed = None;
        egui::Window::new("Runtime profiling")
            .open(&mut open)
            .default_size([960., 700.])
            .min_size([510., 300.])
            .collapsible(false)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.host, false, "Audio");
                    ui.selectable_value(&mut self.host, true, "Host");
                });
                ui.separator();
                if self.host {
                    self.control = show_host(ui, diagnostics, enabled);
                    return;
                }
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

fn show_host(
    ui: &mut egui::Ui,
    diagnostics: &crate::DiagnosticsView,
    enabled: bool,
) -> Option<crate::diagnostics_protocol::Request> {
    use crate::{
        diagnostics_protocol::{Action, Request},
        task_profiling,
    };
    let mut control = None;
    let Some(source) = diagnostics
        .sources
        .iter()
        .find(|s| s.descriptor.stream.source == task_profiling::SOURCE)
    else {
        ui.label("This host does not advertise task profiling.");
        for source in &diagnostics.sources {
            if !source.supported {
                ui.label(format!(
                    "{}: unsupported schema {}",
                    source.descriptor.name.as_str(),
                    source.descriptor.stream.schema
                ));
            }
        }
        return None;
    };
    if !source.supported {
        ui.label("Unsupported host task report schema");
        return None;
    }
    let mut requested = source.status.is_some_and(|s| s.requested);
    ui.horizontal(|ui| {
        if ui
            .add_enabled_ui(enabled && source.descriptor.available, |ui| {
                ui.checkbox(&mut requested, "Collect task statistics")
            })
            .inner
            .changed()
        {
            control = Some(Request {
                source: source.descriptor.stream.source,
                epoch: source.descriptor.stream.epoch,
                action: Action::Subscribe {
                    enabled: requested,
                    interval_ms: 1000,
                },
            });
        }
        if let Some(status) = source.status {
            ui.label(format!(
                "{:?} · every {} ms · delivery drops {} · coalesced {}",
                status.outcome, status.interval_ms, status.dropped, status.coalesced
            ));
        }
    });
    ui.label(format!(
        "Report age: {} ms ago · incomplete/lost reports {}",
        source.age_ms, source.assembly_losses
    ));
    let Some(sample) = &diagnostics.tasks.sample else {
        ui.label("No task sample available");
        return control;
    };
    ui.label(format!(
        "Sample at {} µs · collection {} µs{}",
        sample.captured_us,
        sample.collection_us,
        if sample.complete {
            ""
        } else {
            " · incomplete"
        }
    ));
    ui.label("CPU is scheduler-accounted runtime; stack headroom is the historical minimum unused bytes.");
    egui::ScrollArea::both().show(ui, |ui| {
        egui::Grid::new("host_tasks").striped(true).show(ui, |ui| {
            for title in [
                "Task",
                "State",
                "Priority / base",
                "CPU %",
                "Stack free bytes",
            ] {
                ui.strong(title);
            }
            ui.end_row();
            for (index, task) in sample.tasks.iter().enumerate() {
                ui.label(format!("{} ({})", task.name.as_str(), task.id));
                ui.label(
                    [
                        "Running",
                        "Ready",
                        "Blocked",
                        "Suspended",
                        "Deleted",
                        "Invalid",
                    ]
                    .get(task.state as usize)
                    .copied()
                    .unwrap_or("Unknown"),
                );
                ui.label(format!(
                    "{} / {}",
                    task.priority,
                    task.base_priority
                        .map_or_else(|| "—".into(), |n| n.to_string())
                ));
                ui.label(
                    diagnostics
                        .tasks
                        .cpu_percent
                        .get(index)
                        .and_then(|n| *n)
                        .map_or_else(|| "—".into(), |n| format!("{n:.1}")),
                );
                ui.label(
                    task.stack_free_bytes
                        .map_or_else(|| "—".into(), |n| n.to_string()),
                );
                ui.end_row();
            }
        });
    });
    control
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
                                        ui.set_min_width(96.);
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
                                        ui.set_min_width(name_width);
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
                                            ui.set_min_width(96.);
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
    fn host_data() -> crate::DiagnosticsView {
        use crate::{diagnostics_protocol as protocol, task_profiling as tasks};
        let stream = tasks::stream(77);
        let mut diagnostics = crate::DiagnosticsView {
            sources: vec![crate::ProfilingSource {
                descriptor: protocol::Descriptor {
                    stream,
                    name: protocol::Name::new("host.freertos.tasks").unwrap(),
                    available: true,
                    min_interval_ms: 1000,
                    max_interval_ms: 60000,
                    snapshot: true,
                    inspect: false,
                },
                status: Some(protocol::Status {
                    stream,
                    requested: false,
                    active: false,
                    interval_ms: 2000,
                    outcome: protocol::Outcome::Applied,
                    dropped: 4,
                    coalesced: 3,
                }),
                age_ms: 1234,
                assembly_losses: 2,
                supported: true,
            }],
            ..Default::default()
        };
        let mut sample = tasks::Snapshot {
            generation: 4,
            clock_domain: 77,
            captured_us: 1_000_000,
            collection_us: 321,
            total_runtime: Some(1000),
            counter_bits: 64,
            counter_hz: 1000,
            complete: true,
            tasks: vec![
                tasks::Task {
                    id: 11,
                    name: protocol::Name::new("render").unwrap(),
                    state: 0,
                    priority: 6,
                    base_priority: Some(4),
                    runtime: Some(100),
                    stack_free_bytes: Some(12288),
                },
                tasks::Task {
                    id: 12,
                    name: protocol::Name::new("control").unwrap(),
                    state: 3,
                    priority: 3,
                    base_priority: None,
                    runtime: None,
                    stack_free_bytes: None,
                },
            ],
        };
        diagnostics.tasks.receive(sample.clone());
        sample.captured_us = 2_000_000;
        sample.total_runtime = Some(2000);
        sample.tasks[0].runtime = Some(350);
        diagnostics.tasks.receive(sample);
        diagnostics
    }
    fn host_labels(diagnostics: &crate::DiagnosticsView) -> Vec<String> {
        let context = egui::Context::default();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200., 900.),
                )),
                ..Default::default()
            },
            |ui| {
                assert!(show_host(ui, diagnostics, true).is_none());
            },
        );
        output.textures_delta.clear();
        let mut labels = Vec::new();
        for shape in output.shapes {
            texts(&shape.shape, &mut labels);
        }
        labels
    }
    #[shoop_test]
    fn host_table_displays_interval_cpu_optional_values_freshness_and_delivery_loss() {
        let mut diagnostics = host_data();
        let labels = host_labels(&diagnostics);
        for expected in [
            "render (11)",
            "control (12)",
            "Running",
            "Suspended",
            "6 / 4",
            "3 / —",
            "25.0",
            "12288",
            "—",
        ] {
            assert!(
                labels.iter().any(|text| text == expected),
                "missing {expected}: {labels:?}"
            );
        }
        for expected in [
            "every 2000 ms",
            "delivery drops 4",
            "coalesced 3",
            "1234 ms ago",
            "collection 321 µs",
        ] {
            assert!(
                labels.iter().any(|text| text.contains(expected)),
                "missing {expected}: {labels:?}"
            );
        }
        let mut failed = diagnostics.tasks.sample.as_ref().unwrap().clone();
        failed.complete = false;
        failed.captured_us += 1_000_000;
        diagnostics.tasks.receive(failed);
        let labels = host_labels(&diagnostics);
        assert!(labels.iter().any(|text| text.contains("incomplete")));
        assert!(!labels.iter().any(|text| text == "25.0"));
        diagnostics.tasks = Default::default();
        assert!(host_labels(&diagnostics)
            .iter()
            .any(|text| text == "No task sample available"));
        diagnostics.sources[0].supported = false;
        assert!(host_labels(&diagnostics)
            .iter()
            .any(|text| text == "Unsupported host task report schema"));
        diagnostics.sources[0].descriptor.stream.source = 999;
        diagnostics.sources[0].descriptor.stream.schema = 99;
        let labels = host_labels(&diagnostics);
        assert!(labels
            .iter()
            .any(|text| text.contains("unsupported schema 99")));
        diagnostics.sources.clear();
        assert!(host_labels(&diagnostics)
            .iter()
            .any(|text| text == "This host does not advertise task profiling."));
    }
    #[shoop_test]
    fn host_window_routes_its_own_subscription_without_an_audio_clock_and_retains_disabled_data() {
        fn position(shape: &egui::Shape) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.job.text == "Collect task statistics" => {
                    Some(t.pos + t.galley.rect.center().to_vec2())
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(position),
                _ => None,
            }
        }
        let context = egui::Context::default();
        let mut window = Window {
            open: true,
            host: true,
            ..Default::default()
        };
        let mut diagnostics = host_data();
        let audio = ProfilingView::default();
        let mut click = |diagnostics: &crate::DiagnosticsView, enabled: bool| {
            let mut target = None;
            let mut command = None;
            for button in [None, None, Some(true), Some(false)] {
                let mut events = Vec::new();
                if let Some(pos) = target {
                    events.push(egui::Event::PointerMoved(pos));
                    if let Some(pressed) = button {
                        events.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        });
                    }
                }
                let mut output = context.run_ui(
                    egui::RawInput {
                        events,
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1200., 900.),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        assert!(
                            window
                                .show(
                                    ui.ctx(),
                                    &audio,
                                    &StatusState::default(),
                                    enabled,
                                    diagnostics
                                )
                                .is_none(),
                            "host control emitted an audio subscription"
                        );
                    },
                );
                output.textures_delta.clear();
                target = output
                    .shapes
                    .iter()
                    .find_map(|shape| position(&shape.shape));
                command = window.take_control().or(command);
            }
            assert!(target.is_some());
            command
        };
        use crate::diagnostics_protocol::{Action, Request};
        assert_eq!(
            click(&diagnostics, true),
            Some(Request {
                source: crate::task_profiling::SOURCE,
                epoch: 77,
                action: Action::Subscribe {
                    enabled: true,
                    interval_ms: 1000
                }
            })
        );
        assert!(!audio.requested && !audio.available);
        diagnostics.sources[0].status.as_mut().unwrap().requested = true;
        assert_eq!(
            click(&diagnostics, true).unwrap().action,
            Action::Subscribe {
                enabled: false,
                interval_ms: 1000
            }
        );
        assert!(diagnostics.tasks.sample.is_some());
        assert!(click(&diagnostics, false).is_none());
        diagnostics.sources[0].descriptor.available = false;
        assert!(click(&diagnostics, true).is_none());
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
