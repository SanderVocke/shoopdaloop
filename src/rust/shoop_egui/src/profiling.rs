#[path = "profiling/graph.rs"]
mod graph;

use crate::{colors, ProfilingView, StatusState};
use shoop_app_api::DspMeasurement;

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

    fn dsp_value(self, sample: DspMeasurement) -> Option<f64> {
        match self {
            Self::Microseconds => sample.microseconds(),
            Self::Budget => sample.percent(),
        }
    }

    fn format_dsp(self, value: f64) -> String {
        match self {
            Self::Microseconds => format!("{value:.3} µs"),
            Self::Budget => format!("{value:.1}%"),
        }
    }
}

/// Overall DSP measurements over the last `SPAN_S` seconds.
#[derive(Default)]
pub(crate) struct DspHistory {
    samples: std::collections::VecDeque<(f64, Option<DspMeasurement>)>,
}
impl DspHistory {
    const SPAN_S: f64 = 30.;
    const INTERVAL_S: f64 = 0.05;

    fn record(&mut self, now: f64, sample: Option<DspMeasurement>) {
        if self.samples.back().is_some_and(|&(time, _)| now < time) {
            self.samples.clear();
        }
        if self
            .samples
            .back()
            .is_some_and(|&(time, _)| now - time < Self::INTERVAL_S)
        {
            return;
        }
        self.samples.push_back((now, sample));
        while self
            .samples
            .front()
            .is_some_and(|&(time, _)| now - time > Self::SPAN_S)
        {
            self.samples.pop_front();
        }
    }

    fn peak(&self, units: Units) -> Option<f64> {
        self.samples
            .iter()
            .filter_map(|&(_, sample)| units.dsp_value(sample?))
            .reduce(f64::max)
    }

    fn show(&self, ui: &mut egui::Ui, now: f64, units: Units, peak: Option<f64>) {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 72.), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4., ui.visuals().extreme_bg_color);
        let top = match units {
            Units::Budget => peak.unwrap_or(0.).max(100.),
            Units::Microseconds => peak.filter(|&value| value > 0.).unwrap_or(1.),
        };
        let y = |value: f64| rect.bottom() - rect.height() * (value / top).clamp(0., 1.) as f32;
        let x = |time: f64| rect.right() - rect.width() * ((now - time) / Self::SPAN_S) as f32;
        let grid_top = if units == Units::Budget { 100. } else { top };
        for fraction in [0.25, 0.5, 0.75] {
            painter.hline(
                rect.x_range(),
                y(fraction * grid_top),
                egui::Stroke::new(1., colors::MUTED_FOREGROUND.gamma_multiply(0.35)),
            );
        }
        if units == Units::Budget {
            painter.hline(
                rect.x_range(),
                y(100.),
                egui::Stroke::new(1., colors::ERROR.gamma_multiply(0.6)),
            );
        }
        let stroke = egui::Stroke::new(1.5, colors::COLORED_HIGHLIGHT);
        let positions = self.samples.iter().map(|&(time, sample)| {
            units
                .dsp_value(sample?)
                .map(|value| egui::pos2(x(time), y(value)))
        });
        let mut points = Vec::new();
        for position in positions.chain(core::iter::once(None)) {
            if let Some(point) = position {
                points.push(point);
            } else {
                match points.as_slice() {
                    [] => {}
                    [point] => {
                        painter.circle_filled(*point, stroke.width, stroke.color);
                        points.clear();
                    }
                    _ => {
                        painter.add(egui::Shape::line(core::mem::take(&mut points), stroke));
                    }
                }
            }
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(Self::INTERVAL_S));
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
    dsp: DspHistory,
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
                    &mut self.dsp,
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
    show_health(ui, diagnostics);
    ui.separator();
    ui.strong("Task statistics");
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

fn show_health(ui: &mut egui::Ui, diagnostics: &crate::DiagnosticsView) {
    use shoop_app_api::host_health;
    ui.strong("Host health");
    let Some(source) = diagnostics
        .sources
        .iter()
        .find(|source| source.descriptor.stream.source == host_health::SOURCE)
    else {
        ui.label("This host does not advertise health diagnostics.");
        return;
    };
    if !source.supported {
        ui.label("Unsupported host health report schema");
        return;
    }
    let Some(sample) = &diagnostics.health else {
        ui.label("Waiting for a host health sample");
        return;
    };
    let stale = !source.descriptor.available
        || source.status.is_none_or(|status| {
            !status.active
                || status.outcome != crate::diagnostics_protocol::Outcome::Applied
                || source.age_ms > u64::from(status.interval_ms).saturating_mul(3)
        });
    ui.label(format!(
        "Health sample: {} ms ago{} · incomplete/lost reports {}",
        source.age_ms,
        if stale {
            " · stale / monitoring inactive"
        } else {
            ""
        },
        source.assembly_losses
    ));
    if sample.temperatures.iter().all(Option::is_none) {
        ui.label("Temperature sensors unavailable on this host");
    } else {
        egui::Grid::new("host_temperatures")
            .striped(true)
            .show(ui, |ui| {
                ui.strong("Temperature sensor");
                ui.strong(if stale {
                    "Last reading (stale)"
                } else {
                    "Current"
                });
                ui.strong("Sensor maximum");
                ui.end_row();
                for sensor in sample.temperatures.iter().flatten() {
                    ui.label(sensor.name.as_str());
                    for value in [sensor.current_millicelsius, sensor.peak_millicelsius] {
                        ui.label(value.map_or_else(
                            || "Unavailable".into(),
                            |value| format!("{:.1} °C", f64::from(value) / 1000.),
                        ));
                    }
                    ui.end_row();
                }
            });
        ui.small("Sensor maxima may include readings before this host session.");
    }
    match sample.overtemperature {
        Some(true) => {
            ui.colored_label(colors::ERROR, "Overtemperature alarm: asserted");
        }
        Some(false) => {
            ui.label("Overtemperature alarm: not asserted in this sample");
        }
        None => {
            ui.label("Overtemperature alarm status unavailable");
        }
    }
    ui.small("Health telemetry is not thermal protection; local safeguards operate independently.");
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
        &mut DspHistory::default(),
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
    dsp: &mut DspHistory,
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
    let now = ui.input(|input| input.time);
    dsp.record(now, status.dsp);
    let percent_available = budget.is_some()
        || status.dsp.and_then(DspMeasurement::percent).is_some()
        || dsp.peak(Units::Budget).is_some();
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
            ui.add_enabled_ui(percent_available, |ui| {
                ui.selectable_value(units, Units::Budget, "% budget");
            });
            ui.selectable_value(units, Units::Microseconds, "µs");
        });
    });
    ui.add_space(12.);
    let current = status.dsp.and_then(|sample| units.dsp_value(sample));
    let peak = dsp.peak(*units);
    egui::Frame::group(ui.style())
        .inner_margin(12)
        .corner_radius(8)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Overall DSP").on_hover_text(
                    status
                        .dsp
                        .map_or("Waiting for a DSP measurement", |sample| sample.label()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.monospace(
                        current.map_or_else(|| "—".into(), |value| units.format_dsp(value)),
                    );
                    if let Some(peak) = peak {
                        ui.weak(format!(
                            "peak {} · last {:.0} s",
                            units.format_dsp(peak),
                            DspHistory::SPAN_S
                        ));
                    }
                });
            });
            dsp.show(ui, now, *units, peak);
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
    ui.collapsing("Usage", |ui| {
        crate::show_resource_usage(ui, status.memory.as_ref())
    });
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
    ui.weak(match units { Units::Microseconds => "Node durations in µs", Units::Budget => "Node durations as % of one quantum's DSP time budget; callback entry and exit as % of the callback's" })
        .on_hover_text("Includes input preparation, routing, processing and output recovery. Elapsed time includes preemption. The overall DSP graph measures complete callbacks.");
    let wrapper = |name: &'static str,
                   hint: &'static str,
                   time: &shoop_app_api::profiling_data::Statistics,
                   share: &shoop_app_api::profiling_data::Statistics| {
        let (stats, budget) = match *units {
            Units::Microseconds => (time, budget),
            Units::Budget => (
                share,
                Some(shoop_app_api::profiling_data::WrappingStats::BUDGET_SCALE as f64),
            ),
        };
        (stats.count > 0).then(|| {
            (
                name.to_owned(),
                Some(hint),
                stats.count,
                [stats.mean, stats.worst as f64, stats.stddev()],
                budget,
            )
        })
    };
    let names = view.node_names(report);
    let entry = wrapper(
        "Callback entry",
        "Callback entry to first graph start, including input preparation.",
        &report.wrapping.before,
        &report.wrapping.before_budget,
    );
    let exit = wrapper(
        "Callback exit",
        "Last processor completion to callback exit, including final output routing.",
        &report.wrapping.after,
        &report.wrapping.after_budget,
    );
    let rows: Vec<_> = entry
        .into_iter()
        .chain(report.rows.iter().map(|row| {
            (
                names[&row.key].clone(),
                None,
                row.samples,
                [row.avg_ns, row.worst_ns as f64, row.stddev_ns],
                budget,
            )
        }))
        .chain(exit)
        .collect();
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
                            for (name, hint, samples, values, budget) in &rows {
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
                                        .on_hover_text(hint.unwrap_or(name.as_str()));
                                    },
                                );
                                for &ns in values {
                                    let text = if *samples == 0 {
                                        "—".into()
                                    } else {
                                        units.format(ns, *budget)
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
            sources: vec![shoop_app_api::ProfilingSource {
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
    fn health_ui_presents_sensor_absence_temperature_alarm_and_stale_readings() {
        use shoop_app_api::{diagnostics_protocol as protocol, host_health as health};
        let mut diagnostics = crate::DiagnosticsView::default();
        assert!(host_labels(&diagnostics)
            .iter()
            .any(|text| text.contains("does not advertise health")));
        diagnostics.sources.push(shoop_app_api::ProfilingSource {
            descriptor: health::descriptor(1),
            status: Some(protocol::Status {
                stream: health::stream(1),
                requested: true,
                active: true,
                interval_ms: 1000,
                outcome: protocol::Outcome::Applied,
                dropped: 0,
                coalesced: 0,
            }),
            supported: true,
            age_ms: 0,
            assembly_losses: 0,
        });
        assert!(host_labels(&diagnostics)
            .iter()
            .any(|text| text.contains("Waiting for a host health sample")));
        diagnostics.health = Some(health::Snapshot::default());
        assert!(host_labels(&diagnostics)
            .iter()
            .any(|text| text.contains("Temperature sensors unavailable")));
        let sample = diagnostics.health.as_mut().unwrap();
        sample.temperatures[0] = Some(health::Temperature {
            name: protocol::Name::new("SoC die (PS/PL)").unwrap(),
            current_millicelsius: Some(42500),
            peak_millicelsius: Some(63000),
        });
        sample.overtemperature = Some(true);
        let labels = host_labels(&diagnostics);
        for expected in [
            "SoC die (PS/PL)",
            "42.5 °C",
            "63.0 °C",
            "Overtemperature alarm: asserted",
        ] {
            assert!(
                labels.iter().any(|text| text == expected),
                "missing {expected}: {labels:?}"
            );
        }
        diagnostics.sources[0].age_ms = 3001;
        let labels = host_labels(&diagnostics);
        assert!(labels
            .iter()
            .any(|text| text.contains("stale / monitoring inactive")));
        assert!(labels.iter().any(|text| text == "Last reading (stale)"));
        diagnostics.health.as_mut().unwrap().temperatures[0]
            .as_mut()
            .unwrap()
            .current_millicelsius = None;
        assert!(host_labels(&diagnostics)
            .iter()
            .any(|text| text == "Unavailable"));
        diagnostics.sources[0].supported = false;
        assert!(host_labels(&diagnostics)
            .iter()
            .any(|text| text == "Unsupported host health report schema"));
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
    fn dsp_history_uses_each_samples_audio_period_and_keeps_missing_values() {
        let mut history = DspHistory::default();
        let callback = |elapsed_ns, frames| {
            Some(DspMeasurement::Callback {
                elapsed_ns,
                frames,
                sample_rate: 1000,
                paced: true,
            })
        };
        history.record(0., callback(4_000_000, 8));
        history.record(0.1, callback(8_000_000, 32));
        assert_eq!(history.peak(Units::Microseconds), Some(8_000.));
        assert_eq!(history.peak(Units::Budget), Some(50.));
        let context = egui::Context::default();
        let points = |units| {
            let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                history.show(ui, 0.1, units, history.peak(units));
            });
            output.textures_delta.clear();
            output
                .shapes
                .into_iter()
                .find_map(|shape| match shape.shape {
                    egui::Shape::Path(path) => Some(path.points),
                    _ => None,
                })
                .unwrap()
        };
        let time = points(Units::Microseconds);
        let percent = points(Units::Budget);
        assert!(time[0].y > time[1].y);
        assert!(percent[0].y < percent[1].y);

        let jack = DspMeasurement::JackServer { percent: 125. };
        history.record(0.2, Some(jack));
        assert_eq!(Units::Microseconds.dsp_value(jack), None);
        assert_eq!(history.peak(Units::Microseconds), Some(8_000.));
        assert_eq!(history.peak(Units::Budget), Some(125.));

        history.record(DspHistory::SPAN_S + 1., None);
        assert_eq!(history.peak(Units::Microseconds), None);
        assert_eq!(history.peak(Units::Budget), None);
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
        profiling.live.wrapping.before.add(2000);
        profiling.live.wrapping.after.add(4000);
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
                            dsp: Some(DspMeasurement::Callback {
                                elapsed_ns: 156_000,
                                frames: 32,
                                sample_rate: 48_000,
                                paced: true,
                            }),
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
            "Callback entry",
            "Callback exit",
            "2.000",
            "4.000",
            "156.000 µs",
            "peak 156.000 µs · last 30 s",
        ] {
            assert!(
                labels.iter().any(|text| text == expected),
                "missing {expected}: {labels:?}"
            );
        }
        let percentages = render(&profiling, false, Units::Budget);
        for expected in ["0.19", "0.45", "23.4%", "peak 23.4% · last 30 s"] {
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
