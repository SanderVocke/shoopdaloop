use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{
    AppIntent, ApplicationPortOwner, ConnectionViewState, CueOutputSelection,
    LatencyCertaintyState, LatencyComponentKind, LatencyComponentPolicyState,
    LatencyObservationState, LatencyRangeSelectionState, LatencyValueMode, LoopId, PortDirection,
    StatusState, TrackLatencyPolicyState, TrackState,
};

const COMPONENTS: [LatencyComponentKind; 5] = [
    LatencyComponentKind::ExternalCapture,
    LatencyComponentKind::Processor,
    LatencyComponentKind::CuePlayback,
    LatencyComponentKind::BackendBuffering,
    LatencyComponentKind::Manual,
];

#[derive(Clone, Copy)]
pub(crate) struct LatencyPanelContext<'a> {
    pub status: &'a StatusState,
    pub connections: &'a ConnectionViewState,
}

#[derive(Debug, Default)]
pub(crate) struct LatencyPanel {
    open: bool,
    take_edits: BTreeMap<LoopId, i32>,
}

impl LatencyPanel {
    pub(crate) fn open(&mut self) {
        self.open = true;
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        track: &TrackState,
        runtime: Option<LatencyPanelContext<'_>>,
    ) -> Vec<AppIntent> {
        if !self.open {
            return Vec::new();
        }
        let _span = tracing::trace_span!(
            "frontend.egui.latency_panel",
            track_id = track.id.raw(),
            loop_count = track.loops.len()
        )
        .entered();
        let mut intents = Vec::new();
        let mut open = self.open;
        egui::Window::new(format!("{} latency", track.name))
            .id(egui::Id::new(("track_latency", track.id)))
            .open(&mut open)
            .resizable(true)
            .default_width(560.0)
            .show(context, |ui| {
                let sample_rate = runtime.map_or(0, |runtime| runtime.status.sample_rate);
                let mut policy = normalized_policy(&track.latency_policy);
                let source_revision = policy.revision;
                let mut changed = false;

                ui.heading("Future operations");
                ui.horizontal(|ui| {
                    changed |= ui
                        .checkbox(&mut policy.cue_followed, "Performance followed Shoop cue")
                        .changed();
                    if policy.pending {
                        ui.spinner();
                        ui.label("applying");
                    }
                });
                changed |= cue_selector(ui, track, runtime.as_ref(), &mut policy.cue_output);
                ui.separator();

                egui::Grid::new(("latency_components", track.id))
                    .striped(true)
                    .num_columns(6)
                    .show(ui, |ui| {
                        ui.strong("Use");
                        ui.strong("Component");
                        ui.strong("Mode");
                        ui.strong("Value / trim");
                        ui.strong("Range point");
                        ui.strong("Current observation");
                        ui.end_row();
                        for component in Arc::make_mut(&mut policy.components) {
                            changed |= ui.checkbox(&mut component.enabled, "").changed();
                            ui.label(component_label(component.kind));
                            let old_mode = mode_index(component.value_mode);
                            let mut mode = old_mode;
                            egui::ComboBox::from_id_salt((track.id, component.kind, "mode"))
                                .selected_text(mode_label(mode))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut mode, 0, "Automatic");
                                    ui.selectable_value(&mut mode, 1, "Manual replacement");
                                    ui.selectable_value(&mut mode, 2, "Automatic + trim");
                                });
                            if mode != old_mode {
                                component.value_mode = match mode {
                                    1 => LatencyValueMode::Manual(0),
                                    2 => LatencyValueMode::AutomaticPlusTrim(0),
                                    _ => LatencyValueMode::Automatic,
                                };
                                changed = true;
                            }
                            match &mut component.value_mode {
                                LatencyValueMode::Automatic => {
                                    ui.label("detected");
                                }
                                LatencyValueMode::Manual(frames) => {
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(frames)
                                                .range(0..=shoop_latency::MAX_COMPENSATION_FRAMES)
                                                .suffix(" frames"),
                                        )
                                        .changed();
                                }
                                LatencyValueMode::AutomaticPlusTrim(frames) => {
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(frames)
                                                .range(
                                                    -(shoop_latency::MAX_COMPENSATION_FRAMES as i32)
                                                        ..=shoop_latency::MAX_COMPENSATION_FRAMES
                                                            as i32,
                                                )
                                                .suffix(" frames"),
                                        )
                                        .changed();
                                }
                            }
                            let old_range = component.range_selection;
                            egui::ComboBox::from_id_salt((track.id, component.kind, "range"))
                                .selected_text(range_label(component.range_selection))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut component.range_selection,
                                        LatencyRangeSelectionState::Minimum,
                                        "Minimum",
                                    );
                                    ui.selectable_value(
                                        &mut component.range_selection,
                                        LatencyRangeSelectionState::Midpoint,
                                        "Midpoint",
                                    );
                                    ui.selectable_value(
                                        &mut component.range_selection,
                                        LatencyRangeSelectionState::Maximum,
                                        "Maximum",
                                    );
                                });
                            changed |= old_range != component.range_selection;
                            let observation = current_observation(track, runtime.as_ref(), component.kind);
                            ui.label(observation_text(observation, sample_rate));
                            ui.end_row();
                        }
                    });

                let total = selected_total(&policy, track, runtime.as_ref());
                ui.label(match total {
                    Some(frames) => format!(
                        "Selected policy total: {}",
                        frames_and_ms(frames, sample_rate)
                    ),
                    None => "Selected policy total: unresolved".to_owned(),
                });
                if let Some(error) = &policy.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                if changed {
                    policy.revision = source_revision.saturating_add(1);
                    policy.pending = false;
                    policy.error = None;
                    intents.push(AppIntent::SetTrackLatencyPolicy {
                        track_id: track.id,
                        policy,
                    });
                }

                if let Some(runtime) = runtime.as_ref() {
                    ui.separator();
                    ui.heading("Latency diagnostics");
                    let diagnostics = runtime.status.latency_diagnostics;
                    ui.label(format!(
                        "Unresolved {} · changes {} · margins {} · deferred {} · finalization {} · ambiguity {} · providers {}",
                        diagnostics.unresolved_recipes,
                        diagnostics.observation_changes,
                        diagnostics.insufficient_margins,
                        diagnostics.deferred_transitions,
                        diagnostics.finalization_overruns,
                        diagnostics.path_ambiguities,
                        diagnostics.provider_failures,
                    ));
                    paint_diagnostic_plots(ui, diagnostics);
                }

                ui.separator();
                ui.heading("Frozen takes");
                if track.loops.is_empty() {
                    ui.label("No loop rows are available.");
                }
                for loop_state in &track.loops {
                    ui.push_id(loop_state.id, |ui| {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.strong(&loop_state.name);
                                ui.label(format!(
                                    "applied capture {} · render advance {}",
                                    signed_frames_and_ms(
                                        loop_state.latency.capture_alignment_frames,
                                        sample_rate,
                                    ),
                                    frames_and_ms(loop_state.latency.render_advance_frames, sample_rate)
                                ));
                            });
                            ui.label(format!(
                                "Snapshot: {} · observed {} · retained before/after {}/{} frames · revision {}",
                                certainty_label(loop_state.latency.certainty),
                                observation_range_text(
                                    loop_state.latency.observation_min_frames,
                                    loop_state.latency.observation_max_frames,
                                ),
                                loop_state.latency.retained_before_frames,
                                loop_state.latency.retained_after_frames,
                                loop_state.latency.observation_revision,
                            ));
                            if let Some(current) = current_take_comparison(track, runtime.as_ref()) {
                                ui.label(format!(
                                    "Current path: {} (take {})",
                                    observation_text(current, sample_rate),
                                    if current.revision == loop_state.latency.observation_revision {
                                        "matches revision"
                                    } else {
                                        "differs from frozen revision"
                                    }
                                ));
                            }
                            take_warnings(ui, loop_state);
                            let edit = self
                                .take_edits
                                .entry(loop_state.id)
                                .or_insert(loop_state.latency.capture_alignment_frames);
                            ui.horizontal(|ui| {
                                ui.label("Manual take alignment");
                                if ui
                                    .add(
                                        egui::DragValue::new(edit)
                                            .range(
                                                -(shoop_latency::MAX_COMPENSATION_FRAMES as i32)
                                                    ..=shoop_latency::MAX_COMPENSATION_FRAMES as i32,
                                            )
                                            .suffix(" frames"),
                                    )
                                    .changed()
                                {
                                    intents.push(AppIntent::SetTakeLatencyPolicy {
                                        loop_id: loop_state.id,
                                        capture_alignment_frames: *edit,
                                    });
                                }
                                if ui
                                    .button("Consolidate / bake")
                                    .on_hover_text(
                                        "Render the logical compensated window into canonical media and clear take alignment.",
                                    )
                                    .clicked()
                                {
                                    intents.push(AppIntent::ConsolidateTakeLatency {
                                        loop_id: loop_state.id,
                                    });
                                }
                            });
                        });
                    });
                }
            });
        self.open = open;
        self.take_edits
            .retain(|id, _| track.loops.iter().any(|loop_state| loop_state.id == *id));
        intents
    }
}

fn paint_diagnostic_plots(ui: &mut egui::Ui, diagnostics: crate::LatencyDiagnosticsState) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(120.0), 86.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, crate::colors::MUTED_FOREGROUND),
        egui::StrokeKind::Inside,
    );
    let len = usize::from(diagnostics.plot_len).min(crate::LATENCY_DIAGNOSTIC_PLOT_SAMPLES);
    if len < 2 {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Waiting for diagnostic samples",
            egui::FontId::proportional(11.0),
            crate::colors::MUTED_FOREGROUND,
        );
        return;
    }
    let start = (usize::from(diagnostics.plot_cursor) + crate::LATENCY_DIAGNOSTIC_PLOT_SAMPLES
        - len)
        % crate::LATENCY_DIAGNOSTIC_PLOT_SAMPLES;
    let max_value = (0..len)
        .map(|offset| {
            let index = (start + offset) % crate::LATENCY_DIAGNOSTIC_PLOT_SAMPLES;
            diagnostics.applied_capture_plot[index]
                .unsigned_abs()
                .max(diagnostics.render_advance_plot[index])
                .max(diagnostics.active_postroll_plot[index])
        })
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let line = |values: &[(usize, u32)], color: egui::Color32| {
        let points = values
            .iter()
            .map(|(offset, value)| {
                egui::pos2(
                    rect.left() + *offset as f32 * rect.width() / (len - 1) as f32,
                    rect.bottom() - *value as f32 * rect.height() / max_value,
                )
            })
            .collect::<Vec<_>>();
        painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, color)));
    };
    let capture = (0..len)
        .map(|offset| {
            let index = (start + offset) % crate::LATENCY_DIAGNOSTIC_PLOT_SAMPLES;
            (
                offset,
                diagnostics.applied_capture_plot[index].unsigned_abs(),
            )
        })
        .collect::<Vec<_>>();
    let render = (0..len)
        .map(|offset| {
            let index = (start + offset) % crate::LATENCY_DIAGNOSTIC_PLOT_SAMPLES;
            (offset, diagnostics.render_advance_plot[index])
        })
        .collect::<Vec<_>>();
    let postroll = (0..len)
        .map(|offset| {
            let index = (start + offset) % crate::LATENCY_DIAGNOSTIC_PLOT_SAMPLES;
            (offset, diagnostics.active_postroll_plot[index])
        })
        .collect::<Vec<_>>();
    line(&capture, crate::colors::WAVEFORM_LOGICAL_START_MARKER);
    line(&render, egui::Color32::LIGHT_BLUE);
    line(&postroll, egui::Color32::YELLOW);
}

fn normalized_policy(policy: &TrackLatencyPolicyState) -> TrackLatencyPolicyState {
    let mut normalized = policy.clone();
    let mut components = COMPONENTS
        .into_iter()
        .map(|kind| {
            policy
                .components
                .iter()
                .find(|component| component.kind == kind)
                .copied()
                .unwrap_or(LatencyComponentPolicyState {
                    kind,
                    enabled: false,
                    value_mode: if kind == LatencyComponentKind::Manual {
                        LatencyValueMode::Manual(0)
                    } else {
                        LatencyValueMode::Automatic
                    },
                    range_selection: LatencyRangeSelectionState::Maximum,
                })
        })
        .collect::<Vec<_>>();
    components.truncate(shoop_latency::MAX_RECIPE_COMPONENTS);
    normalized.components = components.into();
    normalized
}

fn cue_selector(
    ui: &mut egui::Ui,
    track: &TrackState,
    runtime: Option<&LatencyPanelContext<'_>>,
    selected: &mut Option<CueOutputSelection>,
) -> bool {
    let previous = selected.clone();
    let text = cue_selection_label(selected.as_ref(), runtime);
    egui::ComboBox::from_id_salt((track.id, "cue_output"))
        .selected_text(text)
        .show_ui(ui, |ui| {
            ui.selectable_value(selected, None, "No selected cue output");
            let Some(runtime) = runtime else {
                ui.label("Backend port inventory unavailable");
                return;
            };
            for port in runtime.connections.application_ports.iter().filter(|port| {
                port.direction == PortDirection::Output
                    && matches!(
                        port.owner,
                        ApplicationPortOwner::Track { track_id, .. } if track_id == track.id
                    )
            }) {
                ui.selectable_value(
                    selected,
                    Some(CueOutputSelection::ApplicationPort(port.id)),
                    format!("Application: {}", port.name),
                );
                for link in runtime
                    .connections
                    .confirmed_links
                    .iter()
                    .filter(|link| link.application_port_id == port.id)
                {
                    let label = runtime
                        .connections
                        .host_ports
                        .iter()
                        .find(|host| host.id == link.host_port_id)
                        .map(|host| host.name.as_str())
                        .unwrap_or_else(|| link.host_port_id.as_str());
                    ui.selectable_value(
                        selected,
                        Some(CueOutputSelection::HostPort(link.host_port_id.clone())),
                        format!("Host: {label}"),
                    );
                }
            }
        });
    previous != *selected
}

fn cue_selection_label(
    selection: Option<&CueOutputSelection>,
    runtime: Option<&LatencyPanelContext<'_>>,
) -> String {
    match selection {
        None => "No selected cue output".to_owned(),
        Some(CueOutputSelection::ApplicationPort(id)) => runtime
            .and_then(|runtime| {
                runtime
                    .connections
                    .application_ports
                    .iter()
                    .find(|port| port.id == *id)
            })
            .map(|port| format!("Application: {}", port.name))
            .unwrap_or_else(|| format!("Missing application port {}", id.raw())),
        Some(CueOutputSelection::HostPort(id)) => runtime
            .and_then(|runtime| {
                runtime
                    .connections
                    .host_ports
                    .iter()
                    .find(|port| port.id == *id)
            })
            .map(|port| format!("Host: {}", port.name))
            .unwrap_or_else(|| format!("Missing host port {id}")),
    }
}

fn current_observation(
    track: &TrackState,
    runtime: Option<&LatencyPanelContext<'_>>,
    kind: LatencyComponentKind,
) -> LatencyObservationState {
    match kind {
        LatencyComponentKind::ExternalCapture => aggregate_observations(
            runtime
                .into_iter()
                .flat_map(|runtime| runtime.connections.application_ports.iter())
                .filter(|port| {
                    track.port_ids.contains(&port.id) && port.direction == PortDirection::Input
                })
                .map(|port| port.capture_latency),
        ),
        LatencyComponentKind::Processor => {
            track.fx.as_ref().map(|fx| fx.latency).unwrap_or_default()
        }
        LatencyComponentKind::CuePlayback => {
            let Some(runtime) = runtime else {
                return Default::default();
            };
            match track.latency_policy.cue_output.as_ref() {
                Some(CueOutputSelection::ApplicationPort(id)) => runtime
                    .connections
                    .application_ports
                    .iter()
                    .find(|port| port.id == *id)
                    .map(|port| port.playback_latency)
                    .unwrap_or_default(),
                Some(CueOutputSelection::HostPort(id)) => aggregate_observations(
                    runtime
                        .connections
                        .confirmed_links
                        .iter()
                        .filter(|link| link.host_port_id == *id)
                        .filter_map(|link| {
                            runtime
                                .connections
                                .application_ports
                                .iter()
                                .find(|port| port.id == link.application_port_id)
                        })
                        .map(|port| port.playback_latency),
                ),
                None => Default::default(),
            }
        }
        LatencyComponentKind::BackendBuffering => runtime
            .map(|runtime| runtime.status.backend_capture_latency)
            .unwrap_or_default(),
        LatencyComponentKind::Manual => Default::default(),
    }
}

fn aggregate_observations(
    observations: impl Iterator<Item = LatencyObservationState>,
) -> LatencyObservationState {
    let known = observations
        .filter(|observation| {
            observation.minimum_frames.is_some() && observation.maximum_frames.is_some()
        })
        .collect::<Vec<_>>();
    if known.is_empty() {
        return Default::default();
    }
    let minimum_frames = known.iter().filter_map(|value| value.minimum_frames).min();
    let maximum_frames = known.iter().filter_map(|value| value.maximum_frames).max();
    let certainty = if minimum_frames == maximum_frames {
        LatencyCertaintyState::Exact
    } else if known
        .iter()
        .all(|value| value.certainty == LatencyCertaintyState::Estimated)
    {
        LatencyCertaintyState::Estimated
    } else {
        LatencyCertaintyState::Range
    };
    LatencyObservationState {
        minimum_frames,
        maximum_frames,
        certainty,
        sample_rate: known
            .iter()
            .map(|value| value.sample_rate)
            .max()
            .unwrap_or(0),
        revision: known.iter().map(|value| value.revision).max().unwrap_or(0),
    }
}

fn selected_total(
    policy: &TrackLatencyPolicyState,
    track: &TrackState,
    runtime: Option<&LatencyPanelContext<'_>>,
) -> Option<u32> {
    let mut total = 0_u32;
    for component in policy
        .components
        .iter()
        .filter(|component| component.enabled)
    {
        if component.kind == LatencyComponentKind::CuePlayback && !policy.cue_followed {
            continue;
        }
        let selected = match component.value_mode {
            LatencyValueMode::Manual(frames) => Some(frames),
            LatencyValueMode::Automatic | LatencyValueMode::AutomaticPlusTrim(_) => {
                let observation = current_observation(track, runtime, component.kind);
                let (minimum, maximum) = (observation.minimum_frames?, observation.maximum_frames?);
                let automatic = match component.range_selection {
                    LatencyRangeSelectionState::Minimum => minimum,
                    LatencyRangeSelectionState::Midpoint => minimum + (maximum - minimum) / 2,
                    LatencyRangeSelectionState::Maximum => maximum,
                };
                match component.value_mode {
                    LatencyValueMode::Automatic => Some(automatic),
                    LatencyValueMode::AutomaticPlusTrim(trim) => {
                        u32::try_from(i64::from(automatic) + i64::from(trim)).ok()
                    }
                    LatencyValueMode::Manual(_) => unreachable!(),
                }
            }
        }?;
        total = total.checked_add(selected)?;
    }
    Some(total)
}

fn current_take_comparison(
    track: &TrackState,
    runtime: Option<&LatencyPanelContext<'_>>,
) -> Option<LatencyObservationState> {
    let processor = current_observation(track, runtime, LatencyComponentKind::Processor);
    if processor.minimum_frames.is_some() {
        Some(processor)
    } else {
        let capture = current_observation(track, runtime, LatencyComponentKind::ExternalCapture);
        capture.minimum_frames.map(|_| capture)
    }
}

fn take_warnings(ui: &mut egui::Ui, loop_state: &crate::LoopState) {
    let latency = &loop_state.latency;
    if latency.changed_during_operation {
        ui.colored_label(
            egui::Color32::YELLOW,
            "Provider latency changed after this operation was latched; the take remains frozen.",
        );
    }
    if latency.incomplete {
        ui.colored_label(
            egui::Color32::LIGHT_RED,
            "Retained latency margin was insufficient; consolidate, rerecord, or reduce compensation.",
        );
    }
    if latency.finalizing {
        ui.colored_label(egui::Color32::YELLOW, "Finalizing retained postroll…");
    }
    if let Some(mode) = latency.deferred_mode {
        ui.colored_label(
            egui::Color32::YELLOW,
            format!("Transition to {mode:?} is deferred until compensated media is ready."),
        );
    }
    if let Some(error) = &latency.error {
        ui.colored_label(egui::Color32::LIGHT_RED, error);
    }
}

fn component_label(kind: LatencyComponentKind) -> &'static str {
    match kind {
        LatencyComponentKind::ExternalCapture => "External capture",
        LatencyComponentKind::Processor => "Processor / FX",
        LatencyComponentKind::CuePlayback => "Cue / output",
        LatencyComponentKind::BackendBuffering => "Backend buffering",
        LatencyComponentKind::Manual => "Manual correction",
    }
}

fn mode_index(mode: LatencyValueMode) -> u8 {
    match mode {
        LatencyValueMode::Automatic => 0,
        LatencyValueMode::Manual(_) => 1,
        LatencyValueMode::AutomaticPlusTrim(_) => 2,
    }
}

fn mode_label(mode: u8) -> &'static str {
    match mode {
        1 => "Manual",
        2 => "Auto + trim",
        _ => "Automatic",
    }
}

fn range_label(range: LatencyRangeSelectionState) -> &'static str {
    match range {
        LatencyRangeSelectionState::Minimum => "Minimum",
        LatencyRangeSelectionState::Midpoint => "Midpoint",
        LatencyRangeSelectionState::Maximum => "Maximum",
    }
}

fn certainty_label(certainty: LatencyCertaintyState) -> &'static str {
    match certainty {
        LatencyCertaintyState::Exact => "exact",
        LatencyCertaintyState::Range => "range",
        LatencyCertaintyState::Estimated => "estimated",
        LatencyCertaintyState::ManualOnly => "manual only",
        LatencyCertaintyState::Unknown => "unknown",
    }
}

fn observation_range_text(minimum: Option<u32>, maximum: Option<u32>) -> String {
    match (minimum, maximum) {
        (Some(minimum), Some(maximum)) if minimum == maximum => format!("{minimum} frames"),
        (Some(minimum), Some(maximum)) => format!("{minimum}..{maximum} frames"),
        _ => "unknown".to_owned(),
    }
}

fn observation_text(observation: LatencyObservationState, sample_rate: u32) -> String {
    let range = observation_range_text(observation.minimum_frames, observation.maximum_frames);
    let ms = observation
        .maximum_frames
        .filter(|_| sample_rate > 0)
        .map(|frames| format!(" / {:.3} ms", frames as f64 * 1000.0 / sample_rate as f64))
        .unwrap_or_default();
    format!(
        "{} · {}{}",
        certainty_label(observation.certainty),
        range,
        ms
    )
}

fn signed_frames_and_ms(frames: i32, sample_rate: u32) -> String {
    if sample_rate == 0 {
        format!("{frames:+} frames")
    } else {
        format!(
            "{frames:+} frames / {:+.3} ms",
            frames as f64 * 1000.0 / sample_rate as f64
        )
    }
}

fn frames_and_ms(frames: u32, sample_rate: u32) -> String {
    if sample_rate == 0 {
        format!("{frames} frames")
    } else {
        format!(
            "{frames} frames / {:.3} ms",
            frames as f64 * 1000.0 / sample_rate as f64
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ApplicationPortState, ConfirmedConnectionState, HostPortId, HostPortState, PortDataType,
        PortId, PortRole, TrackPortOwnerKind,
    };

    fn runtime() -> (StatusState, ConnectionViewState) {
        let mut diagnostics = crate::LatencyDiagnosticsState::default();
        diagnostics.plot_len = 2;
        diagnostics.plot_cursor = 2;
        diagnostics.applied_capture_plot[..2].copy_from_slice(&[4, 8]);
        diagnostics.render_advance_plot[..2].copy_from_slice(&[2, 6]);
        diagnostics.active_postroll_plot[..2].copy_from_slice(&[1, 3]);
        let status = StatusState {
            sample_rate: 48_000,
            latency_diagnostics: diagnostics,
            backend_capture_latency: LatencyObservationState {
                minimum_frames: Some(4),
                maximum_frames: Some(4),
                certainty: LatencyCertaintyState::Exact,
                sample_rate: 48_000,
                revision: 1,
            },
            ..Default::default()
        };
        let output = PortId::from_raw(2);
        let host = HostPortId::new("system:playback_1");
        let connections = ConnectionViewState {
            backend_available: true,
            application_ports: Arc::from([
                ApplicationPortState {
                    id: PortId::from_raw(1),
                    owner: ApplicationPortOwner::Track {
                        track_id: crate::TrackId::from_raw(7),
                        kind: TrackPortOwnerKind::Main,
                    },
                    name: "Input".to_owned(),
                    data_type: PortDataType::Audio,
                    direction: PortDirection::Input,
                    role: PortRole::AudioInput,
                    connection_policy: crate::ConnectionPolicy::UserManaged,
                    capture_latency: LatencyObservationState {
                        minimum_frames: Some(8),
                        maximum_frames: Some(12),
                        certainty: LatencyCertaintyState::Range,
                        sample_rate: 48_000,
                        revision: 3,
                    },
                    playback_latency: Default::default(),
                },
                ApplicationPortState {
                    id: output,
                    owner: ApplicationPortOwner::Track {
                        track_id: crate::TrackId::from_raw(7),
                        kind: TrackPortOwnerKind::Main,
                    },
                    name: "Output".to_owned(),
                    data_type: PortDataType::Audio,
                    direction: PortDirection::Output,
                    role: PortRole::AudioOutput,
                    connection_policy: crate::ConnectionPolicy::UserManaged,
                    capture_latency: Default::default(),
                    playback_latency: LatencyObservationState {
                        minimum_frames: Some(5),
                        maximum_frames: Some(5),
                        certainty: LatencyCertaintyState::Exact,
                        sample_rate: 48_000,
                        revision: 4,
                    },
                },
            ]),
            host_ports: Arc::from([HostPortState {
                id: host.clone(),
                name: "Speakers".to_owned(),
                data_type: PortDataType::Audio,
                direction: PortDirection::Input,
            }]),
            confirmed_links: Arc::from([ConfirmedConnectionState {
                application_port_id: output,
                host_port_id: host,
            }]),
            ..Default::default()
        };
        (status, connections)
    }

    #[shoop_wasm_test_support::shoop_test]
    fn policy_normalization_and_totals_cover_modes_ranges_cue_and_no_backend() {
        let (status, connections) = runtime();
        let mut track = TrackState {
            id: crate::TrackId::from_raw(7),
            port_ids: Arc::from([PortId::from_raw(1), PortId::from_raw(2)]),
            ..Default::default()
        };
        let mut policy = normalized_policy(&track.latency_policy);
        assert_eq!(policy.components.len(), 5);
        let capture = Arc::make_mut(&mut policy.components)
            .iter_mut()
            .find(|component| component.kind == LatencyComponentKind::ExternalCapture)
            .unwrap();
        capture.enabled = true;
        capture.range_selection = LatencyRangeSelectionState::Midpoint;
        let manual = Arc::make_mut(&mut policy.components)
            .iter_mut()
            .find(|component| component.kind == LatencyComponentKind::Manual)
            .unwrap();
        manual.enabled = true;
        manual.value_mode = LatencyValueMode::AutomaticPlusTrim(2);
        track.latency_policy = policy.clone();
        let context = LatencyPanelContext {
            status: &status,
            connections: &connections,
        };
        assert_eq!(selected_total(&policy, &track, Some(&context)), None);
        Arc::make_mut(&mut policy.components)
            .iter_mut()
            .find(|component| component.kind == LatencyComponentKind::Manual)
            .unwrap()
            .value_mode = LatencyValueMode::Manual(2);
        assert_eq!(selected_total(&policy, &track, Some(&context)), Some(12));
        assert_eq!(
            current_observation(
                &track,
                Some(&context),
                LatencyComponentKind::ExternalCapture,
            )
            .maximum_frames,
            Some(12)
        );
        assert_eq!(
            current_observation(&track, None, LatencyComponentKind::ExternalCapture),
            LatencyObservationState::default()
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn panel_renders_cue_identity_warnings_and_no_backend_state_without_hover() {
        let (status, connections) = runtime();
        let host = HostPortId::new("system:playback_1");
        let mut track = TrackState {
            id: crate::TrackId::from_raw(7),
            name: "Latency UI".to_owned(),
            port_ids: Arc::from([PortId::from_raw(1), PortId::from_raw(2)]),
            latency_policy: TrackLatencyPolicyState {
                cue_followed: true,
                cue_output: Some(CueOutputSelection::HostPort(host.clone())),
                ..Default::default()
            },
            loops: vec![crate::LoopState {
                id: LoopId::from_raw(9),
                name: "Changed take".to_owned(),
                latency: crate::TakeLatencyProvenanceState {
                    changed_during_operation: true,
                    incomplete: true,
                    finalizing: true,
                    deferred_mode: Some(crate::LoopMode::Playing),
                    error: Some("insufficient retained margin".to_owned()),
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        };
        let context = LatencyPanelContext {
            status: &status,
            connections: &connections,
        };
        assert_eq!(
            cue_selection_label(track.latency_policy.cue_output.as_ref(), Some(&context)),
            "Host: Speakers"
        );
        track.latency_policy.cue_output =
            Some(CueOutputSelection::ApplicationPort(PortId::from_raw(999)));
        assert!(
            cue_selection_label(track.latency_policy.cue_output.as_ref(), Some(&context))
                .contains("Missing application port")
        );

        let egui_context = egui::Context::default();
        let mut panel = LatencyPanel::default();
        panel.open();
        let mut output = egui_context.run_ui(egui::RawInput::default(), |ui| {
            assert!(panel.show(ui.ctx(), &track, Some(context)).is_empty());
        });
        assert!(!output.shapes.is_empty());
        output.textures_delta.clear();
    }

    #[shoop_wasm_test_support::shoop_test]
    fn snapshot_warning_text_and_frame_units_are_authoritative() {
        assert_eq!(frames_and_ms(48, 48_000), "48 frames / 1.000 ms");
        assert_eq!(signed_frames_and_ms(-48, 48_000), "-48 frames / -1.000 ms");
        assert!(observation_text(
            LatencyObservationState {
                minimum_frames: Some(3),
                maximum_frames: Some(5),
                certainty: LatencyCertaintyState::Range,
                sample_rate: 48_000,
                revision: 9,
            },
            48_000,
        )
        .contains("3..5 frames"));
    }
}
