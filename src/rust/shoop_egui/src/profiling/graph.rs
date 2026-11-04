use std::collections::{BTreeMap, BTreeSet};

use egui_graph::{edge::Edge, node::Node, Graph, LayoutNode, LayoutParams, NodeId, View};
use shoop_app_api::profiling_data::{
    self as profiling, CapturePage, CaptureRequest, GraphEdge, NodeTiming, Port, Report, Row,
    Topology, TraceSummary,
};

use super::Units;
use crate::{colors, ProfilingView};

#[derive(Clone, Copy, Default, PartialEq)]
enum Metric {
    #[default]
    Average,
    Worst,
    Stddev,
}
impl Metric {
    fn value(self, row: &Row, ready: bool) -> f64 {
        match (self, ready) {
            (Self::Average, false) => row.avg_ns,
            (Self::Worst, false) => row.worst_ns as f64,
            (Self::Stddev, false) => row.stddev_ns,
            (Self::Average, true) => row.ready_avg_ns,
            (Self::Worst, true) => row.ready_worst_ns as f64,
            (Self::Stddev, true) => row.ready_stddev_ns,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Focus {
    Node(u64),
    Edge(usize),
}

#[derive(Default)]
struct VisualNode {
    id: u64,
    members: Vec<u64>,
    inputs: BTreeMap<(u8, u32), usize>,
    outputs: BTreeMap<(u8, u32), usize>,
}
struct VisualEdge {
    source: (NodeId, usize),
    destination: (NodeId, usize),
    members: Vec<usize>,
}

#[derive(Default)]
pub(super) struct State {
    identity: (u128, u64, u64, u64, u64),
    topology: Option<std::sync::Arc<Topology>>,
    view: View,
    nodes: BTreeMap<u64, VisualNode>,
    edges: Vec<VisualEdge>,
    routes: egui_graph::EdgeRoutes,
    collapsed: BTreeSet<(u8, u32)>,
    metric: Metric,
    rows: BTreeMap<u64, Row>,
    roles: BTreeMap<u64, String>,
    groups: BTreeMap<(u8, u32), String>,
    group_source: Option<std::sync::Arc<[profiling::Group]>>,
    report_stamp: (u64, u64, usize),
    successors: BTreeMap<u64, Vec<u64>>,
    predecessors: BTreeMap<u64, Vec<u64>>,
    pinned: Option<Focus>,
    hovered: Option<Focus>,
    callback: u64,
    tick: u64,
    capture_id: u64,
    index_identity: (usize, usize, u64),
    callbacks: Vec<TraceSummary>,
    actual: BTreeMap<u64, NodeTiming>,
    page: Option<std::sync::Arc<CapturePage>>,
    pending: Option<CaptureRequest>,
    sent: Option<CaptureRequest>,
    pub request: Option<CaptureRequest>,
    fit: bool,
}

fn group_id(group: u8, slot: u32) -> u64 {
    (1 << 62) | (u64::from(group) << 32) | u64::from(slot)
}
fn host_id(port: Port, source: bool) -> u64 {
    (1 << 63) | ((source as u64) << 48) | (u64::from(port.kind) << 32) | u64::from(port.channel)
}
fn group_name(group: u8, slot: u32) -> String {
    format!(
        "{} {}",
        match group {
            1 => "Track",
            2 => "Loop",
            3 => "Bus",
            _ => "Node",
        },
        slot + 1
    )
}
fn color(value: Option<f64>, max: f64) -> egui::Color32 {
    let Some(value) = value else {
        return egui::Color32::from_gray(80);
    };
    let t = (value / max.max(1.)).clamp(0., 1.) as f32;
    egui::Color32::from_rgb(
        (50. + 200. * t) as u8,
        (185. - 130. * t) as u8,
        (190. - 145. * t) as u8,
    )
}

impl State {
    fn project(&self, topology: &Topology, port: Port, source: bool) -> u64 {
        let Some(key) = port.node else {
            return host_id(port, source);
        };
        if let Some(node) = topology.nodes.iter().find(|n| n.key == key) {
            if self.collapsed.contains(&(node.group, node.slot)) {
                return group_id(node.group, node.slot);
            }
        }
        key
    }
    fn rebuild(&mut self, topology: &Topology, style: &egui::Style) {
        self.nodes.clear();
        self.edges.clear();
        self.successors.clear();
        self.predecessors.clear();
        self.pinned = None;
        self.hovered = None;
        for n in &topology.nodes {
            let id = if self.collapsed.contains(&(n.group, n.slot)) {
                group_id(n.group, n.slot)
            } else {
                n.key
            };
            let node = self.nodes.entry(id).or_default();
            node.id = id;
            node.members.push(n.key);
        }
        let mut edges: BTreeMap<(u64, u64, u8, u32, u8, u32), Vec<usize>> = BTreeMap::new();
        for (i, edge) in topology.edges.iter().enumerate() {
            let a = self.project(topology, edge.source, true);
            let b = self.project(topology, edge.destination, false);
            if a == b {
                continue;
            }
            let channel = |id: u64, p: Port| {
                if id & (1 << 62) != 0 && id & (1 << 63) == 0 {
                    0
                } else {
                    p.channel
                }
            };
            edges
                .entry((
                    a,
                    b,
                    edge.source.kind,
                    channel(a, edge.source),
                    edge.destination.kind,
                    channel(b, edge.destination),
                ))
                .or_default()
                .push(i);
        }
        for &(a, b, ak, ac, bk, bc) in edges.keys() {
            let node = self.nodes.entry(a).or_default();
            node.id = a;
            node.outputs.insert((ak, ac), 0);
            let node = self.nodes.entry(b).or_default();
            node.id = b;
            node.inputs.insert((bk, bc), 0);
        }
        for node in self.nodes.values_mut() {
            for (i, socket) in node.inputs.values_mut().enumerate() {
                *socket = i;
            }
            for (i, socket) in node.outputs.values_mut().enumerate() {
                *socket = i;
            }
        }
        for ((a, b, ak, ac, bk, bc), members) in edges {
            self.edges.push(VisualEdge {
                source: (NodeId(a), self.nodes[&a].outputs[&(ak, ac)]),
                destination: (NodeId(b), self.nodes[&b].inputs[&(bk, bc)]),
                members,
            });
        }
        for edge in &self.edges {
            self.successors
                .entry(edge.source.0 .0)
                .or_default()
                .push(edge.destination.0 .0);
            self.predecessors
                .entry(edge.destination.0 .0)
                .or_default()
                .push(edge.source.0 .0);
        }
        let (layout, routes) = egui_graph::layout_routed(
            self.nodes.values().map(|n| {
                (
                    NodeId(n.id),
                    LayoutNode::new(node_size(n))
                        .inputs(n.inputs.len())
                        .outputs(n.outputs.len())
                        .socket_padding(egui_graph::socket_padding(style)),
                )
            }),
            self.edges.iter().map(|e| (e.source, e.destination)),
            LayoutParams::new(egui::Direction::LeftToRight),
        );
        self.view.layout = layout;
        self.routes = routes;
        self.fit = true;
    }
    fn value(&self, key: u64, _report: &Report, ready: bool) -> Option<f64> {
        if self.callback != 0 {
            self.actual.get(&key).map(|n| if ready { n.ready_ns } else { n.end_ns - n.start_ns } as f64)
        } else {
            self.rows
                .get(&key)
                .filter(|r| r.samples > 0)
                .map(|r| self.metric.value(r, ready))
        }
    }
    fn host_baseline(&self) -> Option<f64> {
        (self.callback == 0
            || self.tick != 0
                && self.page.as_ref().is_some_and(|page| {
                    page.phases
                        .iter()
                        .any(|p| p.phase == profiling::Phase::GraphStart as u64)
                }))
        .then_some(0.)
    }
    fn edge_value(&self, edge: &VisualEdge, topology: &Topology, report: &Report) -> Option<f64> {
        let mut result = None;
        for &i in &edge.members {
            let e = topology.edges[i];
            let value = match e.source.node {
                Some(key) => self.value(key, report, true),
                None if e.destination.node.is_some() => self.host_baseline(),
                _ => None,
            }?;
            result = Some(result.map_or(value, |r: f64| r.max(value)));
        }
        result
    }
    fn group_name(&self, group: u8, slot: u32) -> String {
        let fallback = group_name(group, slot);
        self.groups
            .get(&(group, slot))
            .map_or(fallback.clone(), |name| format!("{name} · {fallback}"))
    }
    fn label(&self, node: &VisualNode, _report: &Report) -> String {
        if node.id & (1 << 63) != 0 {
            format!(
                "Host {} {} {}",
                if node.id & (1 << 48) != 0 {
                    "input"
                } else {
                    "output"
                },
                if (node.id >> 32) & 0xff == 0 {
                    "audio"
                } else {
                    "MIDI"
                },
                (node.id & 0xffff_ffff) + 1
            )
        } else if node.id & (1 << 62) != 0 {
            format!(
                "{} · {} nodes",
                self.group_name(((node.id >> 32) & 0xff) as u8, node.id as u32),
                node.members.len()
            )
        } else {
            self.rows
                .get(&node.id)
                .map_or_else(|| format!("Node {}", node.id), |r| r.name.clone())
        }
    }
    fn related(&self, focus: Option<Focus>) -> BTreeSet<u64> {
        let seeds = match focus {
            Some(Focus::Node(n)) => vec![n],
            Some(Focus::Edge(i)) => self
                .edges
                .get(i)
                .map_or_else(Vec::new, |e| vec![e.source.0 .0, e.destination.0 .0]),
            None => return BTreeSet::new(),
        };
        let mut all = BTreeSet::new();
        for forward in [false, true] {
            let mut seen = BTreeSet::new();
            let mut stack = seeds.clone();
            while let Some(n) = stack.pop() {
                if !seen.insert(n) {
                    continue;
                }
                let adjacent = if forward {
                    &self.successors
                } else {
                    &self.predecessors
                };
                if let Some(next) = adjacent.get(&n) {
                    stack.extend(next);
                }
            }
            all.extend(seen);
        }
        all
    }
    fn node_details(
        &self,
        ui: &mut egui::Ui,
        node: &VisualNode,
        report: &Report,
        units: Units,
        budget: Option<f64>,
    ) {
        ui.strong(self.label(node, report));
        if node.members.is_empty() {
            ui.label("Host boundary; processor duration unavailable.");
        }
        for &key in &node.members {
            ui.label(format!("Processor slot {key}"));
            if self.callback != 0 {
                if let Some(n) = self.actual.get(&key) {
                    ui.label(format!(
                        "Actual duration {} · readiness {}",
                        units.format((n.end_ns - n.start_ns) as f64, budget),
                        units.format(n.ready_ns as f64, budget)
                    ));
                } else {
                    ui.label("Actual timing unavailable / loading");
                }
            } else if let Some(r) = self.rows.get(&key).filter(|r| r.samples > 0) {
                ui.label(&r.name);
                ui.label(format!(
                    "Avg {} · Worst {} · Stddev {} · {} samples",
                    units.format(r.avg_ns, budget),
                    units.format(r.worst_ns as f64, budget),
                    units.format(r.stddev_ns, budget),
                    r.samples
                ));
            } else {
                ui.label("No completed processor samples");
            }
        }
        ui.small("Elapsed step includes incoming routing, recovery and preemption.");
    }
    fn edge_details(
        &self,
        ui: &mut egui::Ui,
        edge: &VisualEdge,
        t: &Topology,
        report: &Report,
        units: Units,
        budget: Option<f64>,
    ) {
        let endpoint = |port: Port| {
            port.node.map_or_else(
                || "Host".into(),
                |key| {
                    self.rows.get(&key).map_or_else(
                        || format!("Node {key}"),
                        |row| format!("{} [node {key}]", row.name),
                    )
                },
            )
        };
        ui.strong(format!(
            "{} connection(s) · source output readiness",
            edge.members.len()
        ));
        for &i in &edge.members {
            let GraphEdge {
                source,
                destination,
            } = t.edges[i];
            ui.label(format!(
                "{} {}:{} → {} {}:{}",
                endpoint(source),
                if source.kind == 0 { "audio" } else { "MIDI" },
                source.channel + 1,
                endpoint(destination),
                if destination.kind == 0 {
                    "audio"
                } else {
                    "MIDI"
                },
                destination.channel + 1
            ));
            if let Some(key) = source.node {
                if self.callback != 0 {
                    ui.label(format!(
                        "Actual {}",
                        self.value(key, report, true)
                            .map_or_else(|| "—".into(), |v| units.format(v, budget))
                    ));
                } else if let Some(r) = self.rows.get(&key).filter(|r| r.samples > 0) {
                    ui.label(format!(
                        "Avg {} · Worst {} · Stddev {} · {} samples",
                        units.format(r.ready_avg_ns, budget),
                        units.format(r.ready_worst_ns as f64, budget),
                        units.format(r.ready_stddev_ns, budget),
                        r.samples
                    ));
                } else {
                    ui.label("Readiness unavailable");
                }
            } else if destination.node.is_some() {
                ui.label(if self.host_baseline().is_some() {
                    "0: host input available at graph-start baseline"
                } else {
                    "Graph-start baseline unavailable for this selection"
                });
            } else {
                ui.label("Host bypass readiness unavailable");
            }
        }
        ui.small("Since graph execution began; includes earlier independent work. Not link transfer cost or device playback latency.");
        if edge.members.len() > 1 {
            ui.small(
                "Bundled color uses the maximum displayed source metric, not a combined statistic.",
            );
        }
    }
    fn inspection(&mut self, ui: &mut egui::Ui, view: &ProfilingView) {
        let id = view.capture.as_ref().map_or(0, |c| c.capture);
        if id != self.capture_id {
            self.capture_id = id;
            self.callback = 0;
            self.tick = 0;
            self.actual.clear();
            self.page = None;
            self.sent = None;
            self.pending = None;
        }
        let index_identity = (view.index.as_ptr() as usize, view.index.len(), id);
        if index_identity != self.index_identity {
            self.index_identity = index_identity;
            let hosts: Vec<_> = view
                .index
                .iter()
                .filter(|h| h.flags & profiling::HOST != 0)
                .collect();
            self.callbacks = view
                .index
                .iter()
                .filter(|c| {
                    c.flags & profiling::CALLBACK != 0
                        && (c.flags & profiling::HOST != 0 || !hosts.iter().any(|h| h.contains(c)))
                })
                .copied()
                .collect();
        }
        let callbacks = &self.callbacks;
        let previous = (self.callback, self.tick);
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("profiling_callback")
                .selected_text(if self.callback == 0 {
                    "Capture interval".into()
                } else {
                    format!("Callback {}", self.callback)
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.callback, 0, "Capture interval statistics");
                    for c in callbacks {
                        ui.selectable_value(
                            &mut self.callback,
                            c.sequence,
                            format!(
                                "{} · frame {} · {:.1} µs{}",
                                c.sequence,
                                c.frame,
                                (c.end_ns - c.start_ns) as f64 / 1000.,
                                if c.budget_ns != 0 && c.end_ns - c.start_ns > c.budget_ns {
                                    " · over budget"
                                } else {
                                    ""
                                }
                            ),
                        );
                    }
                });
            if let Some(c) = view.index.iter().find(|c| c.sequence == self.callback) {
                let ticks: Vec<_> = view
                    .index
                    .iter()
                    .filter(|t| t.flags & profiling::CALLBACK == 0 && c.contains(t))
                    .collect();
                if self.callback != previous.0 || !ticks.iter().any(|t| t.sequence == self.tick) {
                    self.tick = ticks.first().map_or(0, |t| t.sequence);
                }
                egui::ComboBox::from_id_salt("profiling_tick")
                    .selected_text(format!("Tick {}", self.tick))
                    .show_ui(ui, |ui| {
                        for tick in ticks {
                            ui.selectable_value(
                                &mut self.tick,
                                tick.sequence,
                                format!("Tick {} · frame {}", tick.sequence, tick.frame),
                            );
                        }
                    });
            }
        });
        if self.callback == 0 {
            self.actual.clear();
            self.page = None;
            self.sent = None;
            self.pending = None;
            return;
        }
        if previous != (self.callback, self.tick) {
            self.actual.clear();
            self.page = None;
            self.sent = None;
            self.pending = None;
        }
        let wanted = CaptureRequest {
            connection: view.connection,
            generation: view.generation,
            capture: id,
            callback: self.callback,
            tick: self.tick,
            offset: self.actual.len() as u32,
        };
        if let Some(page) = view
            .detail
            .as_ref()
            .filter(|d| self.pending == Some(d.request))
        {
            self.actual.extend(page.nodes.iter().map(|n| (n.key, *n)));
            self.page = Some(page.clone());
            self.pending = None;
        }
        let complete = self
            .page
            .as_ref()
            .is_some_and(|p| self.actual.len() >= p.total as usize);
        if !complete && self.pending.is_none() {
            let request = CaptureRequest {
                offset: self.actual.len() as u32,
                ..wanted
            };
            if self.sent != Some(request) {
                self.request = Some(request);
                self.pending = Some(request);
                self.sent = Some(request);
            }
        }
        ui.weak(if self.tick == 0 {
            "Selected callback has no retained tick; node timings unavailable"
        } else if complete {
            "Actual selected-tick timings"
        } else {
            "Loading selected-tick timings…"
        });
        if let Some(page) = &self.page {
            timeline(ui, page, &self.actual, &view.index);
        }
    }
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        view: &ProfilingView,
        report: &Report,
        capture: bool,
        units: Units,
    ) {
        if view.index_incomplete && capture {
            ui.colored_label(
                colors::WARNING,
                "Capture index truncated; only retained entries can be selected.",
            );
        }
        if view.available {
            ui.small(if view.resolution_ns == 0 {
                "Clock resolution unknown".into()
            } else {
                format!(
                    "Observed clock step ≈ {:.3} µs",
                    view.resolution_ns as f64 / 1000.
                )
            });
        }
        let identity = (
            view.lease,
            view.attachment,
            view.connection,
            view.generation,
            report.epoch,
        );
        if identity != self.identity {
            *self = Self {
                identity,
                metric: self.metric,
                ..Self::default()
            };
        }
        let groups_changed = self
            .group_source
            .as_ref()
            .is_none_or(|old| !std::sync::Arc::ptr_eq(old, &view.groups));
        if groups_changed {
            self.groups = view
                .groups
                .iter()
                .filter(|g| !g.name.is_empty())
                .map(|g| ((g.kind, g.slot), g.name.clone()))
                .collect();
            self.group_source = Some(view.groups.clone());
        }
        let stamp = (report.capture, report.interval.end_ns, report.rows.len());
        if stamp != self.report_stamp || groups_changed {
            let names = view.node_names(report);
            self.roles = report
                .rows
                .iter()
                .map(|r| (r.key, r.name.clone()))
                .collect();
            self.rows = report
                .rows
                .iter()
                .map(|r| {
                    let mut row = r.clone();
                    row.name = names[&r.key].clone();
                    (r.key, row)
                })
                .collect();
            self.report_stamp = stamp;
        }
        if capture {
            self.inspection(ui, view);
        } else {
            self.callback = 0;
            self.tick = 0;
            self.actual.clear();
            self.page = None;
            self.pending = None;
            self.sent = None;
        }
        let Some(t) = view
            .topology
            .as_ref()
            .filter(|t| {
                t.generation == view.generation
                    && (report.rows.is_empty() || t.epoch == report.epoch)
            })
            .cloned()
        else {
            ui.weak(if view.graph_limited { "Graph exceeds visualization capacity (4096 nodes / 16384 links); table timing remains available." } else { "Executable topology unavailable or still receiving." });
            return;
        };
        let mut rebuild = self
            .topology
            .as_ref()
            .is_none_or(|old| !std::sync::Arc::ptr_eq(old, &t));
        let budget = t.budget_ns();
        ui.horizontal(|ui| {
            for (value, name) in [
                (Metric::Average, "Avg"),
                (Metric::Worst, "Worst"),
                (Metric::Stddev, "Stddev"),
            ] {
                ui.add_enabled_ui(self.callback == 0, |ui| {
                    ui.selectable_value(&mut self.metric, value, name);
                });
            }
            if ui.button("Fit graph").clicked() {
                self.fit = true;
            }
            if ui.button("Clear pin").clicked() {
                self.pinned = None;
            }
            egui::ComboBox::from_id_salt("profiling_groups")
                .selected_text("Groups")
                .show_ui(ui, |ui| {
                    let groups: BTreeSet<_> = t
                        .nodes
                        .iter()
                        .filter(|n| n.group != 0)
                        .map(|n| (n.group, n.slot))
                        .collect();
                    for group in groups {
                        let mut collapsed = self.collapsed.contains(&group);
                        if ui
                            .checkbox(&mut collapsed, self.group_name(group.0, group.1))
                            .changed()
                        {
                            if collapsed {
                                self.collapsed.insert(group);
                            } else {
                                self.collapsed.remove(&group);
                            }
                            rebuild = true;
                        }
                    }
                    ui.small("Checked groups collapse into summary nodes.");
                });
        });
        if rebuild {
            self.rebuild(&t, ui.style());
            self.topology = Some(t.clone());
        }
        let duration_max = self
            .nodes
            .values()
            .filter(|n| n.members.len() == 1)
            .filter_map(|n| self.value(n.members[0], report, false))
            .fold(0., f64::max);
        let ready_max = self
            .edges
            .iter()
            .filter_map(|e| self.edge_value(e, &t, report))
            .fold(0., f64::max);
        ui.horizontal_wrapped(|ui| {
            for (label, max) in [
                ("Nodes: step duration", duration_max),
                ("Links: source readiness", ready_max),
            ] {
                ui.label(label);
                ui.colored_label(color(Some(0.), max), "0");
                ui.colored_label(
                    color(Some(max), max),
                    format!(
                        "{} {}",
                        units.format(max, budget),
                        if units == Units::Budget {
                            "% budget"
                        } else {
                            "µs"
                        }
                    ),
                );
            }
            ui.weak("Gray: unavailable");
        });
        ui.small("Readiness is elapsed since graph start. Hover traces influence; click pins details. Ctrl/Cmd+scroll to zoom; middle-drag to pan.");
        if let Some(focus) = self.pinned {
            egui::CollapsingHeader::new("Pinned details")
                .default_open(true)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(160.)
                        .show(ui, |ui| match focus {
                            Focus::Node(id) => {
                                if let Some(node) = self.nodes.get(&id) {
                                    self.node_details(ui, node, report, units, budget);
                                }
                            }
                            Focus::Edge(i) => {
                                if let Some(edge) = self.edges.get(i) {
                                    self.edge_details(ui, edge, &t, report, units, budget);
                                }
                            }
                        });
                });
        }
        let related = self.related(self.hovered.or(self.pinned));
        let fit = std::mem::take(&mut self.fit);
        if fit {
            let mut rect = egui::Rect::NOTHING;
            for n in self.nodes.values() {
                if let Some(p) = self.view.layout.get(&NodeId(n.id)) {
                    rect |= egui::Rect::from_min_size(*p, node_size(n));
                }
            }
            if rect.is_finite() {
                self.view.scene_rect = rect.expand(35.);
            }
        }
        let mut hovered = None;
        let mut pin = None;
        let mut widget_view = std::mem::take(&mut self.view);
        let height = ui.available_height().max(180.);
        ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
            Graph::new(("profiling_graph", identity))
                .immutable(true)
                .center_view(false)
                .zoom_range(0.0001..=4.)
                .show(&mut widget_view, ui, |ui, show| {
                    show.nodes(ui, |ctx, ui| {
                        let labels_visible = ui
                            .ctx()
                            .layer_transform_to_global(ui.layer_id())
                            .is_none_or(|t| t.scaling >= 0.35);
                        for node in self.nodes.values() {
                            let value = if node.members.len() == 1 {
                                self.value(node.members[0], report, false)
                            } else {
                                None
                            };
                            let stroke = if related.contains(&node.id) {
                                egui::Stroke::new(2.5, colors::FOREGROUND)
                            } else {
                                egui::Stroke::new(1., color(value, duration_max))
                            };
                            let frame = egui::Frame::group(ui.style())
                                .fill(color(value, duration_max).gamma_multiply(0.25))
                                .stroke(stroke);
                            let response = Node::from_id(NodeId(node.id))
                                .inputs(node.inputs.len())
                                .outputs(node.outputs.len())
                                .max_width(148.)
                                .show_or_cull(ctx, ui, |ctx| {
                                    ctx.framed_with(frame, |ui, _| {
                                        ui.set_opacity(1.);
                                        ui.set_min_size(node_size(node) - egui::vec2(12., 12.));
                                        if labels_visible {
                                            ui.spacing_mut().item_spacing.y = 1.;
                                            let name = self.label(node, report);
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&name)
                                                        .color(colors::FOREGROUND),
                                                )
                                                .truncate(),
                                            );
                                            if let Some(role) =
                                                self.roles.get(&node.id).filter(|r| **r != name)
                                            {
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(role)
                                                            .color(colors::FOREGROUND),
                                                    )
                                                    .truncate(),
                                                );
                                            }
                                            ui.monospace(value.map_or_else(
                                                || "—".into(),
                                                |v| units.format(v, budget),
                                            ));
                                        }
                                    })
                                });
                            if response.hovered() {
                                hovered = Some(Focus::Node(node.id));
                            }
                            if response.clicked() || response.selection() == Some(true) {
                                pin = Some(Focus::Node(node.id));
                            }
                            egui::Response::clone(&response).on_hover_ui(|ui| {
                                self.node_details(ui, node, report, units, budget)
                            });
                        }
                    })
                    .edges(ui, |ctx, ui| {
                        let scale = ui
                            .ctx()
                            .layer_transform_to_global(ui.layer_id())
                            .map_or(1., |t| t.scaling)
                            .max(0.0001);
                        ui.style_mut().interaction.interact_radius = 6. / scale;
                        for (i, edge) in self.edges.iter().enumerate() {
                            let mut selected = self.pinned == Some(Focus::Edge(i));
                            let value = self.edge_value(edge, &t, report);
                            let stroke = egui::Stroke::new(
                                if related.contains(&edge.source.0 .0)
                                    && related.contains(&edge.destination.0 .0)
                                {
                                    3.
                                } else {
                                    1.8
                                },
                                color(value, ready_max),
                            );
                            let response = Edge::new(edge.source, edge.destination, &mut selected)
                                .distance_per_point(2. / scale)
                                .waypoints(
                                    self.routes
                                        .route(edge.source, edge.destination, 0)
                                        .unwrap_or(&[]),
                                )
                                .stroke(stroke)
                                .hovered_stroke(egui::Stroke::new(4., stroke.color))
                                .selected_stroke(egui::Stroke::new(4., stroke.color))
                                .show(ctx, ui);
                            if response.hovered() {
                                hovered = Some(Focus::Edge(i));
                            }
                            if response.clicked() || response.changed() && selected {
                                pin = Some(Focus::Edge(i));
                            }
                            egui::Response::clone(&response).on_hover_ui(|ui| {
                                self.edge_details(ui, edge, &t, report, units, budget)
                            });
                        }
                    });
                });
        });
        self.view = widget_view;
        self.hovered = hovered;
        if let Some(pin) = pin {
            self.pinned = Some(pin);
        }
    }
}

fn node_size(node: &VisualNode) -> egui::Vec2 {
    egui::vec2(
        148.,
        (node.inputs.len().max(node.outputs.len()) as f32 * 16. + 24.).max(80.),
    )
}

fn timeline(
    ui: &mut egui::Ui,
    page: &CapturePage,
    nodes: &BTreeMap<u64, NodeTiming>,
    index: &[TraceSummary],
) {
    let c = page.callback;
    let duration = c.end_ns.saturating_sub(c.start_ns);
    ui.label(format!(
        "Callback {} · {:.3} µs · {}",
        c.sequence,
        duration as f64 / 1000.,
        if c.budget_ns == 0 {
            "deadline unavailable / incomplete outer context".into()
        } else {
            format!("budget {:.3} µs", c.budget_ns as f64 / 1000.)
        }
    ));
    if page.incomplete || c.flags & (profiling::FAULT | profiling::TRUNCATED) != 0 {
        ui.colored_label(
            colors::WARNING,
            "Incomplete capture; gaps are unmeasured and attribution may be uncertain.",
        );
    }
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 72.), egui::Sense::hover());
    let max = duration.max(c.budget_ns).max(1) as f64;
    let x =
        |at: u64| rect.left() + (at.saturating_sub(c.start_ns) as f64 / max) as f32 * rect.width();
    ui.painter()
        .rect_filled(rect, 3., egui::Color32::from_gray(22));
    for tick in index
        .iter()
        .filter(|t| t.flags & profiling::CALLBACK == 0 && c.contains(t))
    {
        let r = egui::Rect::from_min_max(
            egui::pos2(x(tick.start_ns), rect.top() + 4.),
            egui::pos2(x(tick.end_ns).max(x(tick.start_ns) + 1.), rect.top() + 22.),
        );
        ui.painter().rect_filled(
            r,
            1.,
            if page.tick.is_some_and(|t| t.sequence == tick.sequence) {
                colors::COLORED_HIGHLIGHT
            } else {
                egui::Color32::from_gray(85)
            },
        );
        ui.interact(
            r,
            ui.make_persistent_id(("timeline_tick", tick.sequence)),
            egui::Sense::hover(),
        )
        .on_hover_text(format!(
            "Tick {} · frame {} · {:.3} µs",
            tick.sequence,
            tick.frame,
            (tick.end_ns - tick.start_ns) as f64 / 1000.
        ));
    }
    for n in nodes.values() {
        let r = egui::Rect::from_min_max(
            egui::pos2(x(n.start_ns), rect.top() + 28.),
            egui::pos2(x(n.end_ns).max(x(n.start_ns) + 1.), rect.top() + 47.),
        );
        ui.painter().rect_filled(
            r,
            1.,
            color(Some((n.end_ns - n.start_ns) as f64), duration as f64),
        );
        ui.interact(
            r,
            ui.make_persistent_id(("timeline_node", n.key)),
            egui::Sense::hover(),
        )
        .on_hover_text(format!(
            "Node {} · actual {:.3} µs",
            n.key,
            (n.end_ns - n.start_ns) as f64 / 1000.
        ));
    }
    for phase in &page.phases {
        let px = x(phase.at);
        let r =
            egui::Rect::from_min_size(egui::pos2(px - 3., rect.top() + 50.), egui::vec2(6., 15.));
        ui.painter().line_segment(
            [egui::pos2(px, r.top()), egui::pos2(px, r.bottom())],
            egui::Stroke::new(1., colors::FOREGROUND),
        );
        ui.interact(
            r,
            ui.make_persistent_id(("timeline_phase", phase.at, phase.phase)),
            egui::Sense::hover(),
        )
        .on_hover_text(format!(
            "{} · {:.3} µs",
            phase_name(phase.phase),
            phase.at.saturating_sub(c.start_ns) as f64 / 1000.
        ));
    }
    if c.budget_ns != 0 {
        let px = x(c.start_ns.saturating_add(c.budget_ns));
        ui.painter().line_segment(
            [egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())],
            egui::Stroke::new(2., colors::ERROR),
        );
    }
    ui.small("Top: retained ticks · middle: selected tick's node steps · bottom: recorded phases · red: callback deadline. Gaps/preemption are not separately attributed.");
}

fn phase_name(phase: u64) -> &'static str {
    match phase {
        0 => "Callback start",
        1 => "Commands",
        2 => "Tick start",
        3 => "State settled",
        4 => "Graph start",
        5 => "Output routing complete",
        6 => "State advanced",
        7 => "Callback end",
        8 => "Host input ready",
        9 => "Host output complete",
        _ => "Unknown phase",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    use std::sync::Arc;

    fn fixture() -> ProfilingView {
        let t = Topology {
            generation: 1,
            epoch: 4,
            sample_rate: 48000,
            quantum: 32,
            nodes: (1..=4)
                .map(|key| profiling::GraphNode {
                    key,
                    group: if key <= 2 { 1 } else { 2 },
                    slot: 0,
                })
                .collect(),
            edges: [
                (None, Some(1)),
                (Some(1), Some(2)),
                (Some(2), Some(3)),
                (Some(3), None),
            ]
            .into_iter()
            .map(|(a, b)| GraphEdge {
                source: Port {
                    node: a,
                    ..Port::default()
                },
                destination: Port {
                    node: b,
                    ..Port::default()
                },
            })
            .collect(),
        };
        ProfilingView {
            generation: 1,
            available: true,
            topology: Some(Arc::new(t)),
            live: Report {
                generation: 1,
                epoch: 4,
                rows: (1..=4)
                    .map(|key| Row {
                        key,
                        name: format!("Processor {key}"),
                        samples: 8,
                        avg_ns: 1000.,
                        worst_ns: 2000,
                        stddev_ns: 500.,
                        ready_avg_ns: key as f64 * 1000.,
                        ready_worst_ns: key * 2000,
                        ready_stddev_ns: 750.,
                    })
                    .collect(),
                ..Report::default()
            },
            ..ProfilingView::default()
        }
    }

    #[shoop_test]
    fn friendly_names_update_without_relayout_and_preserve_processor_identity() {
        let mut view = fixture();
        view.groups = vec![profiling::Group {
            kind: 1,
            slot: 0,
            entity: 90,
            name: "Drums".into(),
        }]
        .into();
        let mut state = State::default();
        let context = egui::Context::default();
        render(&mut state, &view, &context, Vec::new())
            .textures_delta
            .clear();
        assert_eq!(state.rows[&1].name, "Drums · Processor 1");
        assert_eq!(state.rows[&2].name, "Drums · Processor 2");
        assert_eq!(state.rows[&3].name, "Processor 3");
        let layout = state.view.layout.clone();
        let source = state.topology.clone().unwrap();
        let mut renamed = view.groups.to_vec();
        renamed[0].name = "Percussion".into();
        view.groups = renamed.into();
        render(&mut state, &view, &context, Vec::new())
            .textures_delta
            .clear();
        assert_eq!(state.rows[&1].name, "Percussion · Processor 1");
        assert_eq!(state.view.layout, layout);
        assert!(Arc::ptr_eq(state.topology.as_ref().unwrap(), &source));
        let mut old_report = view.live.clone();
        old_report.epoch -= 1;
        assert_eq!(view.node_names(&old_report)[&1], "Processor 1");
    }

    fn render(
        state: &mut State,
        view: &ProfilingView,
        context: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200., 800.),
                )),
                events,
                ..egui::RawInput::default()
            },
            |ui| state.show(ui, view, &view.live, false, Units::Microseconds),
        )
    }

    #[shoop_test]
    fn group_projection_keeps_cross_group_edges_and_disconnected_processors() {
        let context = egui::Context::default();
        let view = fixture();
        let mut state = State {
            identity: (0, 0, 0, 1, 4),
            ..State::default()
        };
        state.collapsed.insert((1, 0));
        render(&mut state, &view, &context, vec![])
            .textures_delta
            .clear();
        assert_eq!(state.nodes[&group_id(1, 0)].members, [1, 2]);
        assert!(state.nodes.contains_key(&4));
        assert_eq!(state.edges.len(), 3);
        assert_eq!(state.related(Some(Focus::Node(group_id(1, 0)))).len(), 4);
        assert!(state.edges.iter().any(|e| e.members == [2]));
    }

    #[shoop_test]
    fn live_refresh_preserves_widget_geometry_and_generation_reset_clears_pins() {
        let context = egui::Context::default();
        let mut view = fixture();
        let mut state = State::default();
        render(&mut state, &view, &context, vec![])
            .textures_delta
            .clear();
        let positions = state.view.layout.clone();
        view.live.interval.end_ns += 1;
        view.live.rows[0].avg_ns = 90000.;
        state.pinned = Some(Focus::Node(1));
        render(&mut state, &view, &context, vec![])
            .textures_delta
            .clear();
        assert_eq!(positions, state.view.layout);
        assert_eq!(state.value(1, &view.live, false), Some(90000.));
        assert_eq!(state.pinned, Some(Focus::Node(1)));
        view.generation = 2;
        view.topology = None;
        view.live.rows.clear();
        render(&mut state, &view, &context, vec![])
            .textures_delta
            .clear();
        assert!(state.pinned.is_none() && state.actual.is_empty() && state.pending.is_none());
    }

    fn text_center(shapes: &[egui::epaint::ClippedShape], name: &str) -> egui::Pos2 {
        for shape in shapes {
            if let egui::Shape::Text(text) = &shape.shape {
                if text.galley.job.text == name {
                    return text.visual_bounding_rect().center();
                }
            }
        }
        panic!("missing graph text {name}");
    }

    #[shoop_test]
    fn group_menu_opens_and_collapses_members_through_pointer_input() {
        let context = egui::Context::default();
        let view = fixture();
        let mut state = State::default();
        render(&mut state, &view, &context, vec![])
            .textures_delta
            .clear();
        let mut output = render(&mut state, &view, &context, vec![]);
        let point = text_center(&output.shapes, "Groups");
        output.textures_delta.clear();
        let click = |state: &mut State, point| {
            render(
                state,
                &view,
                &context,
                vec![egui::Event::PointerMoved(point)],
            )
            .textures_delta
            .clear();
            render(
                state,
                &view,
                &context,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
            )
            .textures_delta
            .clear();
            render(
                state,
                &view,
                &context,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            )
        };
        click(&mut state, point).textures_delta.clear();
        let mut output = render(&mut state, &view, &context, vec![]);
        output.textures_delta.clear();
        let point = text_center(&output.shapes, "Track 1");
        click(&mut state, point).textures_delta.clear();
        assert!(state.collapsed.contains(&(1, 0)));
        assert_eq!(state.nodes[&group_id(1, 0)].members, [1, 2]);
    }

    #[shoop_test]
    fn node_hover_and_pinning_survive_widget_zoom() {
        let context = egui::Context::default();
        let view = fixture();
        let mut state = State::default();
        render(&mut state, &view, &context, vec![])
            .textures_delta
            .clear();
        let mut output = render(&mut state, &view, &context, vec![]);
        let point = text_center(&output.shapes, "Processor 1");
        output.textures_delta.clear();
        render(
            &mut state,
            &view,
            &context,
            vec![egui::Event::PointerMoved(point)],
        )
        .textures_delta
        .clear();
        assert_eq!(state.hovered, Some(Focus::Node(1)));
        let button = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        render(&mut state, &view, &context, vec![button(true)])
            .textures_delta
            .clear();
        render(&mut state, &view, &context, vec![button(false)])
            .textures_delta
            .clear();
        assert_eq!(state.pinned, Some(Focus::Node(1)));
        state.pinned = None;
        let mut output = render(&mut state, &view, &context, vec![egui::Event::Zoom(0.75)]);
        let point = text_center(&output.shapes, "Processor 1");
        output.textures_delta.clear();
        render(
            &mut state,
            &view,
            &context,
            vec![egui::Event::PointerMoved(point)],
        )
        .textures_delta
        .clear();
        assert_eq!(state.hovered, Some(Focus::Node(1)));
        assert_eq!(state.value(3, &view.live, true), Some(3000.));
    }

    #[shoop_test]
    fn link_hover_and_pin_keep_a_screen_space_radius_after_zoom() {
        fn point(output: &egui::FullOutput) -> egui::Pos2 {
            output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Path(p) if !p.closed && p.points.len() > 5 => {
                        Some(p.points[p.points.len() / 2])
                    }
                    _ => None,
                })
                .expect("rendered connection")
        }
        let context = egui::Context::default();
        let view = fixture();
        let mut state = State::default();
        render(&mut state, &view, &context, vec![])
            .textures_delta
            .clear();
        for zoom in [1., 0.2, 2.] {
            state.pinned = None;
            render(&mut state, &view, &context, vec![egui::Event::Zoom(zoom)])
                .textures_delta
                .clear();
            let mut output = render(&mut state, &view, &context, vec![]);
            output.textures_delta.clear();
            let point = point(&output) + egui::vec2(0., 4.);
            render(
                &mut state,
                &view,
                &context,
                vec![egui::Event::PointerMoved(point)],
            )
            .textures_delta
            .clear();
            render(&mut state, &view, &context, vec![])
                .textures_delta
                .clear();
            assert!(
                matches!(state.hovered, Some(Focus::Edge(_))),
                "zoom {zoom}: {:?}",
                state.hovered
            );
            let focus = state.hovered;
            for pressed in [true, false] {
                render(
                    &mut state,
                    &view,
                    &context,
                    vec![egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                )
                .textures_delta
                .clear();
            }
            assert_eq!(state.pinned, focus);
        }
    }

    #[shoop_test]
    fn a_callback_without_a_graph_start_has_no_host_link_readiness() {
        let context = egui::Context::default();
        let view = fixture();
        let mut state = State::default();
        render(&mut state, &view, &context, vec![])
            .textures_delta
            .clear();
        let topology = view.topology.as_ref().unwrap();
        let edge = state
            .edges
            .iter()
            .find(|e| topology.edges[e.members[0]].source.node.is_none())
            .unwrap();
        assert_eq!(state.edge_value(edge, topology, &view.live), Some(0.));
        state.callback = 99;
        assert_eq!(state.edge_value(edge, topology, &view.live), None);
        state.tick = 1;
        assert_eq!(state.edge_value(edge, topology, &view.live), None);
        state.page = Some(Arc::new(CapturePage {
            phases: vec![profiling::Boundary {
                phase: profiling::Phase::GraphStart as u64,
                at: 42,
            }],
            ..CapturePage::default()
        }));
        assert_eq!(state.edge_value(edge, topology, &view.live), Some(0.));
        assert_eq!(state.value(1, &view.live, false), None);
    }

    #[shoop_test]
    fn new_connection_or_attachment_clears_selection_even_when_graph_numbers_repeat() {
        let context = egui::Context::default();
        let mut view = fixture();
        view.connection = 7;
        view.lease = 100;
        view.attachment = 1;
        let mut state = State::default();
        render(&mut state, &view, &context, vec![])
            .textures_delta
            .clear();
        for (connection, lease, attachment) in [(8, 100, 1), (8, 100, 2), (8, 200, 1)] {
            state.pinned = Some(Focus::Node(1));
            state.pending = Some(CaptureRequest {
                connection: view.connection,
                generation: 1,
                capture: 9,
                callback: 5,
                tick: 4,
                offset: 0,
            });
            view.connection = connection;
            view.lease = lease;
            view.attachment = attachment;
            render(&mut state, &view, &context, vec![])
                .textures_delta
                .clear();
            assert!(state.pinned.is_none() && state.pending.is_none() && state.actual.is_empty());
        }
    }

    #[shoop_test]
    fn captured_selection_pages_are_generation_and_selection_fenced() {
        let context = egui::Context::default();
        let mut view = fixture();
        view.capture = Some(Report {
            generation: 1,
            capture: 9,
            ..view.live.clone()
        });
        let callback = TraceSummary {
            sequence: 5,
            epoch: 4,
            flags: profiling::CALLBACK | profiling::HOST,
            start_ns: 100,
            end_ns: 5000,
            budget_ns: 4000,
            ..TraceSummary::default()
        };
        let tick = TraceSummary {
            sequence: 4,
            epoch: 4,
            start_ns: 200,
            end_ns: 4000,
            ..TraceSummary::default()
        };
        view.index = Arc::from([callback, tick]);
        let mut state = State {
            identity: (0, 0, 0, 1, 4),
            capture_id: 9,
            callback: 5,
            tick: 4,
            ..State::default()
        };
        context
            .run_ui(egui::RawInput::default(), |ui| state.inspection(ui, &view))
            .textures_delta
            .clear();
        let request = state.request.take().unwrap();
        assert_eq!(request.tick, 4);
        view.detail = Some(Arc::new(CapturePage {
            request: CaptureRequest { tick: 3, ..request },
            total: 1,
            nodes: vec![NodeTiming {
                key: 1,
                ..NodeTiming::default()
            }],
            ..CapturePage::default()
        }));
        context
            .run_ui(egui::RawInput::default(), |ui| state.inspection(ui, &view))
            .textures_delta
            .clear();
        assert!(state.actual.is_empty());
        view.detail = Some(Arc::new(CapturePage {
            request,
            callback,
            tick: Some(tick),
            total: 1,
            nodes: vec![NodeTiming {
                key: 1,
                start_ns: 300,
                end_ns: 900,
                ready_ns: 700,
            }],
            ..CapturePage::default()
        }));
        context
            .run_ui(egui::RawInput::default(), |ui| state.inspection(ui, &view))
            .textures_delta
            .clear();
        assert_eq!(state.actual[&1].ready_ns, 700);
        view.capture.as_mut().unwrap().capture = 10;
        context
            .run_ui(egui::RawInput::default(), |ui| state.inspection(ui, &view))
            .textures_delta
            .clear();
        assert!(state.actual.is_empty() && state.page.is_none());
    }
}
