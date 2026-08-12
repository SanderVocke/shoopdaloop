use std::collections::{BTreeMap, BTreeSet};

use crate::{
    colors, AppIntent, AppState, ApplicationPortOwner, ApplicationPortState, ConnectionPolicy,
    ConnectionViewState, HostPortId, HostPortState, PortDataType, PortDirection, PortId, ScriptId,
    TrackId,
};

const COLUMN_WIDTH: f32 = 190.0;
const COLUMN_GAP: f32 = 72.0;
const ENDPOINT_HEIGHT: f32 = 28.0;
const CONNECTOR_RADIUS: f32 = 5.0;
const CONNECTOR_HIT_RADIUS: f32 = 11.0;
const CURVE_HIT_DISTANCE: f32 = 7.0;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConnectionScope {
    AllTracks,
    Track(TrackId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TrackFilter {
    All,
    Selected(BTreeSet<TrackId>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConnectionFilters {
    audio: bool,
    midi: bool,
    tracks: TrackFilter,
}

impl ConnectionFilters {
    fn for_scope(scope: ConnectionScope) -> Self {
        Self {
            audio: true,
            midi: true,
            tracks: match scope {
                ConnectionScope::AllTracks => TrackFilter::All,
                ConnectionScope::Track(track_id) => {
                    TrackFilter::Selected(BTreeSet::from([track_id]))
                }
            },
        }
    }

    fn includes_type(&self, data_type: PortDataType) -> bool {
        match data_type {
            PortDataType::Audio => self.audio,
            PortDataType::Midi => self.midi,
        }
    }

    fn includes_owner(&self, owner: &ApplicationPortOwner) -> bool {
        match (&self.tracks, owner) {
            (TrackFilter::All, _) => true,
            (TrackFilter::Selected(track_ids), ApplicationPortOwner::Track { track_id, .. }) => {
                track_ids.contains(track_id)
            }
            (TrackFilter::Selected(_), ApplicationPortOwner::LuaControl { .. }) => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum GraphColumn {
    SystemSources,
    ShoopSinks,
    ShoopSources,
    SystemSinks,
}

impl GraphColumn {
    const ORDERED: [Self; 4] = [
        Self::SystemSources,
        Self::ShoopSinks,
        Self::ShoopSources,
        Self::SystemSinks,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::SystemSources => "System sources",
            Self::ShoopSinks => "ShoopDaLoop sinks",
            Self::ShoopSources => "ShoopDaLoop sources",
            Self::SystemSinks => "System sinks",
        }
    }

    const fn is_source(self) -> bool {
        matches!(self, Self::SystemSources | Self::ShoopSources)
    }

    const fn connector_on_right(self) -> bool {
        matches!(self, Self::SystemSources | Self::ShoopSources)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum EndpointId {
    Application(PortId),
    Host(HostPortId),
}

#[derive(Clone, Debug)]
struct GraphEndpoint {
    id: EndpointId,
    column: GraphColumn,
    group: String,
    label: String,
    full_name: String,
    data_type: PortDataType,
    policy: ConnectionPolicy,
    pending: bool,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GraphRouteState {
    Confirmed,
    PendingConnect,
    PendingDisconnect,
    Error,
}

#[derive(Clone, Debug)]
struct GraphRoute {
    application_port_id: PortId,
    host_port_id: HostPortId,
    source: EndpointId,
    sink: EndpointId,
    data_type: PortDataType,
    policy: ConnectionPolicy,
    state: GraphRouteState,
    error: Option<String>,
}

#[derive(Debug, Default)]
struct ConnectionGraph {
    endpoints: BTreeMap<GraphColumn, Vec<GraphEndpoint>>,
    routes: Vec<GraphRoute>,
}

impl ConnectionGraph {
    fn build(app_state: &AppState, filters: &ConnectionFilters) -> Self {
        let state = app_state.connections.as_ref();
        let track_names: BTreeMap<_, _> = app_state
            .tracks
            .iter()
            .map(|track| (track.id, track.name.as_str()))
            .collect();
        let script_names: BTreeMap<_, _> = app_state
            .scripting
            .scripts
            .iter()
            .map(|script| (script.id, script.name.as_str()))
            .collect();
        let visible_application_ports: Vec<_> = state
            .application_ports
            .iter()
            .filter(|port| {
                filters.includes_type(port.data_type) && filters.includes_owner(&port.owner)
            })
            .collect();
        let visible_application_ids: BTreeSet<_> = visible_application_ports
            .iter()
            .map(|port| port.id)
            .collect();

        let mut graph = Self::default();
        for port in visible_application_ports {
            let column = match port.direction {
                PortDirection::Input => GraphColumn::ShoopSinks,
                PortDirection::Output => GraphColumn::ShoopSources,
            };
            let error = latest_error(state, port.id, None);
            let pending = state
                .pending_links
                .iter()
                .any(|link| link.application_port_id == port.id);
            graph
                .endpoints
                .entry(column)
                .or_default()
                .push(GraphEndpoint {
                    id: EndpointId::Application(port.id),
                    column,
                    group: application_group_label(&port.owner, &track_names, &script_names),
                    label: short_port_name(&port.name).to_owned(),
                    full_name: port.name.clone(),
                    data_type: port.data_type,
                    policy: port.connection_policy,
                    pending,
                    error,
                });
        }

        let visible_host_ports: Vec<_> = state
            .host_ports
            .iter()
            .filter(|host| {
                filters.includes_type(host.data_type)
                    && visible_application_ids.iter().any(|port_id| {
                        find_application_port(state, *port_id).is_some_and(|port| {
                            port.data_type == host.data_type && port.direction != host.direction
                        })
                    })
            })
            .collect();
        let visible_host_ids: BTreeSet<_> = visible_host_ports
            .iter()
            .map(|host| host.id.clone())
            .collect();
        for host in visible_host_ports {
            let column = match host.direction {
                PortDirection::Output => GraphColumn::SystemSources,
                PortDirection::Input => GraphColumn::SystemSinks,
            };
            let (group, label) = external_name_parts(&host.name);
            let error = state.errors.iter().rev().find_map(|error| {
                (error.external_port.as_deref() == Some(host.id.as_str()))
                    .then(|| error.message.clone())
            });
            let pending = state
                .pending_links
                .iter()
                .any(|link| link.host_port_id == host.id);
            graph
                .endpoints
                .entry(column)
                .or_default()
                .push(GraphEndpoint {
                    id: EndpointId::Host(host.id.clone()),
                    column,
                    group,
                    label,
                    full_name: host.name.clone(),
                    data_type: host.data_type,
                    policy: ConnectionPolicy::UserManaged,
                    pending,
                    error,
                });
        }

        for endpoints in graph.endpoints.values_mut() {
            endpoints.sort_by(|left, right| {
                (&left.group, &left.label, &left.id).cmp(&(&right.group, &right.label, &right.id))
            });
        }

        let confirmed: BTreeSet<_> = state
            .confirmed_links
            .iter()
            .map(|link| (link.application_port_id, link.host_port_id.clone()))
            .collect();
        let pending: BTreeMap<_, _> = state
            .pending_links
            .iter()
            .map(|link| {
                (
                    (link.application_port_id, link.host_port_id.clone()),
                    link.desired_connected,
                )
            })
            .collect();
        let mut route_keys = confirmed.clone();
        route_keys.extend(pending.keys().cloned());
        route_keys.extend(state.errors.iter().filter_map(|error| {
            Some((
                error.port_id?,
                HostPortId::new(error.external_port.clone()?),
            ))
        }));
        for (application_port_id, host_port_id) in route_keys {
            if !visible_application_ids.contains(&application_port_id)
                || !visible_host_ids.contains(&host_port_id)
            {
                continue;
            }
            let Some(port) = find_application_port(state, application_port_id) else {
                continue;
            };
            let Some(host) = find_host_port(state, &host_port_id) else {
                continue;
            };
            if port.data_type != host.data_type || port.direction == host.direction {
                continue;
            }
            let key = (application_port_id, host_port_id.clone());
            let route_state = match pending.get(&key) {
                Some(true) => GraphRouteState::PendingConnect,
                Some(false) => GraphRouteState::PendingDisconnect,
                None if confirmed.contains(&key) => GraphRouteState::Confirmed,
                None => GraphRouteState::Error,
            };
            let (source, sink) = match port.direction {
                PortDirection::Input => (
                    EndpointId::Host(host_port_id.clone()),
                    EndpointId::Application(application_port_id),
                ),
                PortDirection::Output => (
                    EndpointId::Application(application_port_id),
                    EndpointId::Host(host_port_id.clone()),
                ),
            };
            graph.routes.push(GraphRoute {
                application_port_id,
                host_port_id: host_port_id.clone(),
                source,
                sink,
                data_type: port.data_type,
                policy: port.connection_policy,
                state: route_state,
                error: latest_error(state, application_port_id, Some(&host_port_id)),
            });
        }
        graph
            .routes
            .sort_by(|left, right| (&left.source, &left.sink).cmp(&(&right.source, &right.sink)));
        graph
    }

    fn column(&self, column: GraphColumn) -> &[GraphEndpoint] {
        self.endpoints
            .get(&column)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn endpoint(&self, id: &EndpointId) -> Option<&GraphEndpoint> {
        self.endpoints
            .values()
            .flatten()
            .find(|endpoint| &endpoint.id == id)
    }

    fn blocks_pair(&self, source: &EndpointId, sink: &EndpointId) -> bool {
        self.routes.iter().any(|route| {
            &route.source == source && &route.sink == sink && route.state != GraphRouteState::Error
        })
    }

    fn compatible_drop(&self, source: &EndpointId, sink: &EndpointId) -> bool {
        let (Some(source_endpoint), Some(sink_endpoint)) =
            (self.endpoint(source), self.endpoint(sink))
        else {
            return false;
        };
        source_endpoint.column.is_source()
            && !sink_endpoint.column.is_source()
            && source_endpoint.data_type == sink_endpoint.data_type
            && adjacent_columns(source_endpoint.column, sink_endpoint.column)
            && application_endpoint(source_endpoint, sink_endpoint).is_some_and(|endpoint| {
                endpoint.policy == ConnectionPolicy::UserManaged && !endpoint.pending
            })
            && !self.blocks_pair(source, sink)
    }
}

fn adjacent_columns(source: GraphColumn, sink: GraphColumn) -> bool {
    matches!(
        (source, sink),
        (GraphColumn::SystemSources, GraphColumn::ShoopSinks)
            | (GraphColumn::ShoopSources, GraphColumn::SystemSinks)
    )
}

fn application_endpoint<'a>(
    first: &'a GraphEndpoint,
    second: &'a GraphEndpoint,
) -> Option<&'a GraphEndpoint> {
    if matches!(first.id, EndpointId::Application(_)) {
        Some(first)
    } else if matches!(second.id, EndpointId::Application(_)) {
        Some(second)
    } else {
        None
    }
}

fn find_application_port(state: &ConnectionViewState, id: PortId) -> Option<&ApplicationPortState> {
    state.application_ports.iter().find(|port| port.id == id)
}

fn find_host_port<'a>(
    state: &'a ConnectionViewState,
    id: &HostPortId,
) -> Option<&'a HostPortState> {
    state.host_ports.iter().find(|host| &host.id == id)
}

fn latest_error(
    state: &ConnectionViewState,
    port_id: PortId,
    host_port_id: Option<&HostPortId>,
) -> Option<String> {
    state.errors.iter().rev().find_map(|error| {
        (error.port_id == Some(port_id)
            && host_port_id
                .is_none_or(|host_id| error.external_port.as_deref() == Some(host_id.as_str())))
        .then(|| error.message.clone())
    })
}

fn application_group_label(
    owner: &ApplicationPortOwner,
    track_names: &BTreeMap<TrackId, &str>,
    script_names: &BTreeMap<ScriptId, &str>,
) -> String {
    match owner {
        ApplicationPortOwner::Track { track_id, .. } => track_names
            .get(track_id)
            .map(|name| (*name).to_owned())
            .unwrap_or_else(|| format!("Track {track_id}")),
        ApplicationPortOwner::LuaControl { script_id, .. } => script_names
            .get(script_id)
            .map(|name| format!("Script: {name}"))
            .unwrap_or_else(|| format!("Script {script_id}")),
    }
}

#[derive(Clone, Debug)]
struct DragState {
    source: EndpointId,
    revision: u64,
    filters: ConnectionFilters,
}

#[derive(Clone, Debug)]
struct EndpointAnchor {
    center: egui::Pos2,
    hit_rect: egui::Rect,
}

#[derive(Debug)]
pub struct ConnectionDialog {
    open: bool,
    scope: ConnectionScope,
    filters: ConnectionFilters,
    drag: Option<DragState>,
    #[cfg(test)]
    endpoint_rects: BTreeMap<EndpointId, egui::Rect>,
    #[cfg(test)]
    route_points: Vec<(PortId, HostPortId, Vec<egui::Pos2>)>,
    #[cfg(test)]
    graph_clip_rect: Option<egui::Rect>,
    #[cfg(test)]
    hovered_route: Option<(PortId, HostPortId)>,
}

impl Default for ConnectionDialog {
    fn default() -> Self {
        Self {
            open: false,
            scope: ConnectionScope::AllTracks,
            filters: ConnectionFilters::for_scope(ConnectionScope::AllTracks),
            drag: None,
            #[cfg(test)]
            endpoint_rects: BTreeMap::new(),
            #[cfg(test)]
            route_points: Vec::new(),
            #[cfg(test)]
            graph_clip_rect: None,
            #[cfg(test)]
            hovered_route: None,
        }
    }
}

impl ConnectionDialog {
    pub fn open(&mut self, scope: ConnectionScope) {
        self.scope = scope;
        self.filters = ConnectionFilters::for_scope(scope);
        self.drag = None;
        self.open = true;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn scope(&self) -> ConnectionScope {
        self.scope
    }

    #[cfg(test)]
    pub(crate) fn test_selected_tracks(&self) -> Option<BTreeSet<TrackId>> {
        match &self.filters.tracks {
            TrackFilter::All => None,
            TrackFilter::Selected(ids) => Some(ids.clone()),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_data_type_filters(&self) -> (bool, bool) {
        (self.filters.audio, self.filters.midi)
    }

    pub fn show(&mut self, context: &egui::Context, state: &AppState) -> Vec<AppIntent> {
        if !self.open {
            return Vec::new();
        }
        #[cfg(test)]
        {
            self.endpoint_rects.clear();
            self.route_points.clear();
            self.graph_clip_rect = None;
            self.hovered_route = None;
        }
        let scoped_track = match self.scope {
            ConnectionScope::AllTracks => None,
            ConnectionScope::Track(track_id) => {
                state.tracks.iter().find(|track| track.id == track_id)
            }
        };
        let title = match self.scope {
            ConnectionScope::AllTracks => "Connections".to_owned(),
            ConnectionScope::Track(track_id) => scoped_track
                .map(|track| format!("{} Connections", track.name))
                .unwrap_or_else(|| format!("Track {track_id} Connections")),
        };
        let mut open = self.open;
        let mut intents = Vec::new();
        egui::Window::new(title)
            .id(egui::Id::new("connections_dialog"))
            .open(&mut open)
            .resizable(true)
            .default_size([980.0, 560.0])
            .min_size([360.0, 220.0])
            .show(context, |ui| {
                if matches!(self.scope, ConnectionScope::Track(_)) && scoped_track.is_none() {
                    ui.colored_label(colors::WARNING, "This track is no longer available.");
                }
                self.show_contents(ui, state, &mut intents);
            });
        if !open {
            self.drag = None;
        }
        self.open = open;
        intents
    }

    fn show_contents(&mut self, ui: &mut egui::Ui, state: &AppState, intents: &mut Vec<AppIntent>) {
        if state.connections.loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading connection state…");
            });
            return;
        }
        if !state.connections.backend_available {
            ui.colored_label(
                colors::WARNING,
                "Host connection management is unavailable for this audio backend.",
            );
        }

        let previous_filters = self.filters.clone();
        self.show_filters(ui, state);
        if self.filters != previous_filters {
            self.drag = None;
        }
        if self.drag.as_ref().is_some_and(|drag| {
            drag.revision != state.connections.revision || drag.filters != self.filters
        }) {
            self.drag = None;
        }
        ui.separator();

        let graph = ConnectionGraph::build(state, &self.filters);
        if graph.endpoints.values().all(Vec::is_empty) {
            ui.label("No ports match the current filters.");
        } else {
            self.show_graph(ui, state.connections.revision, &graph, intents);
        }

        if let Some(error) = state.connections.errors.last() {
            ui.separator();
            ui.colored_label(colors::ERROR, &error.message);
        }
    }

    fn show_filters(&mut self, ui: &mut egui::Ui, state: &AppState) {
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::fonts::bold_text("Show:"));
            ui.toggle_value(&mut self.filters.audio, "Audio")
                .on_hover_text("Include audio ports and routes");
            ui.toggle_value(&mut self.filters.midi, "MIDI")
                .on_hover_text("Include MIDI ports and routes");
            ui.separator();
            ui.label(crate::fonts::bold_text("Tracks:"));
            let summary = match &self.filters.tracks {
                TrackFilter::All => "All tracks".to_owned(),
                TrackFilter::Selected(ids) if ids.len() == 1 => ids
                    .first()
                    .and_then(|id| state.tracks.iter().find(|track| track.id == *id))
                    .map(|track| track.name.clone())
                    .unwrap_or_else(|| "Unavailable track".to_owned()),
                TrackFilter::Selected(ids) => format!("{} tracks", ids.len()),
            };
            ui.menu_button(summary, |ui| {
                if ui
                    .selectable_label(
                        matches!(self.filters.tracks, TrackFilter::All),
                        "All tracks",
                    )
                    .clicked()
                {
                    self.filters.tracks = TrackFilter::All;
                    ui.close();
                }
                ui.separator();
                for track in &state.tracks {
                    let selected = match &self.filters.tracks {
                        TrackFilter::All => false,
                        TrackFilter::Selected(ids) => ids.contains(&track.id),
                    };
                    if ui.selectable_label(selected, &track.name).clicked() {
                        match &mut self.filters.tracks {
                            TrackFilter::All => {
                                self.filters.tracks =
                                    TrackFilter::Selected(BTreeSet::from([track.id]));
                            }
                            TrackFilter::Selected(ids) => {
                                if !ids.insert(track.id) {
                                    ids.remove(&track.id);
                                }
                                if ids.is_empty() {
                                    self.filters.tracks = TrackFilter::All;
                                }
                            }
                        }
                    }
                }
            })
            .response
            .on_hover_text("Filter ShoopDaLoop ports by one or more tracks");
        });
    }

    fn show_graph(
        &mut self,
        ui: &mut egui::Ui,
        revision: u64,
        graph: &ConnectionGraph,
        intents: &mut Vec<AppIntent>,
    ) {
        let scroll = egui::ScrollArea::both()
            .id_salt(("connection_graph", self.scope))
            .auto_shrink([false, false])
            .scroll_source(crate::control_safe_scroll_source())
            .show(ui, |ui| {
                let mut anchors = BTreeMap::new();
                ui.spacing_mut().item_spacing.x = COLUMN_GAP;
                ui.horizontal_top(|ui| {
                    for column in GraphColumn::ORDERED {
                        ui.allocate_ui_with_layout(
                            egui::vec2(COLUMN_WIDTH, ui.available_height().max(140.0)),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| self.show_column(ui, revision, column, graph, &mut anchors),
                        );
                    }
                });
                let clip_rect = ui.clip_rect();
                self.paint_routes_and_interact(ui, clip_rect, graph, &anchors, intents);
                self.paint_drag(ui, clip_rect, graph, &anchors);
                (anchors, clip_rect)
            });
        #[cfg(test)]
        {
            self.graph_clip_rect = Some(scroll.inner.1);
        }
        if let Some(drag) = self.drag.clone() {
            let released =
                ui.input(|input| input.pointer.button_released(egui::PointerButton::Primary));
            if released {
                let pointer = ui.input(|input| {
                    input
                        .pointer
                        .interact_pos()
                        .or_else(|| input.pointer.hover_pos())
                });
                if let Some((sink, _)) = pointer.and_then(|pointer| {
                    scroll
                        .inner
                        .0
                        .iter()
                        .filter(|(id, _)| {
                            graph
                                .endpoint(id)
                                .is_some_and(|endpoint| !endpoint.column.is_source())
                        })
                        .filter(|(_, anchor)| anchor.hit_rect.contains(pointer))
                        .min_by(|(_, left), (_, right)| {
                            left.center
                                .distance(pointer)
                                .total_cmp(&right.center.distance(pointer))
                        })
                }) {
                    if graph.compatible_drop(&drag.source, sink) {
                        if let Some(intent) = route_intent(graph, &drag.source, sink, true) {
                            intents.push(intent);
                        }
                    }
                }
                self.drag = None;
            }
        }
    }

    fn show_column(
        &mut self,
        ui: &mut egui::Ui,
        revision: u64,
        column: GraphColumn,
        graph: &ConnectionGraph,
        anchors: &mut BTreeMap<EndpointId, EndpointAnchor>,
    ) {
        ui.set_min_width(COLUMN_WIDTH);
        ui.set_max_width(COLUMN_WIDTH);
        ui.vertical_centered(|ui| {
            ui.label(crate::fonts::bold_text(column.label()));
        });
        ui.separator();
        let endpoints = graph.column(column);
        if endpoints.is_empty() {
            ui.weak(match column {
                GraphColumn::SystemSources | GraphColumn::SystemSinks => {
                    "No compatible system ports"
                }
                GraphColumn::ShoopSinks | GraphColumn::ShoopSources => "No ShoopDaLoop ports",
            });
            return;
        }
        let mut previous_group: Option<&str> = None;
        for endpoint in endpoints {
            if previous_group != Some(endpoint.group.as_str()) {
                ui.add_space(4.0);
                ui.label(crate::fonts::bold_italic_text(&endpoint.group));
                previous_group = Some(&endpoint.group);
            }
            let (rect, row_response) = ui.allocate_exact_size(
                egui::vec2(COLUMN_WIDTH, ENDPOINT_HEIGHT),
                egui::Sense::hover(),
            );
            row_response.on_hover_text(endpoint_hover(endpoint, column.is_source()));
            let visuals = ui.visuals().widgets.inactive;
            ui.painter().rect(
                rect.shrink(1.0),
                4.0,
                visuals.bg_fill,
                visuals.bg_stroke,
                egui::StrokeKind::Inside,
            );
            let connector = if column.connector_on_right() {
                egui::pos2(rect.right() - 3.0, rect.center().y)
            } else {
                egui::pos2(rect.left() + 3.0, rect.center().y)
            };
            let hit_rect = egui::Rect::from_center_size(
                connector,
                egui::vec2(CONNECTOR_HIT_RADIUS * 2.0, CONNECTOR_HIT_RADIUS * 2.0),
            );
            let mutable = endpoint.policy == ConnectionPolicy::UserManaged;
            let response = ui
                .interact(
                    hit_rect,
                    ui.id().with(("connector", &endpoint.id)),
                    if column.is_source() && mutable {
                        egui::Sense::drag()
                    } else {
                        egui::Sense::hover()
                    },
                )
                .on_hover_text(endpoint_hover(endpoint, column.is_source()));
            let primary_pressed_on_connector = ui.input(|input| {
                input.pointer.button_pressed(egui::PointerButton::Primary)
                    && input
                        .pointer
                        .interact_pos()
                        .is_some_and(|pointer| hit_rect.contains(pointer))
            });
            if column.is_source()
                && mutable
                && (response.drag_started() || primary_pressed_on_connector)
            {
                self.drag = Some(DragState {
                    source: endpoint.id.clone(),
                    revision,
                    filters: self.filters.clone(),
                });
            }
            let compatible_target = self.drag.as_ref().is_some_and(|drag| {
                !column.is_source() && graph.compatible_drop(&drag.source, &endpoint.id)
            });
            let connector_color = if endpoint.error.is_some() {
                colors::ERROR
            } else if endpoint.pending {
                colors::WARNING
            } else if endpoint.policy == ConnectionPolicy::OwnerManaged {
                colors::MUTED_FOREGROUND
            } else if compatible_target || response.hovered() || response.dragged() {
                colors::COLORED_HIGHLIGHT
            } else {
                data_type_color(endpoint.data_type)
            };
            ui.painter()
                .circle_filled(connector, CONNECTOR_RADIUS, connector_color);
            if compatible_target {
                ui.painter().circle_stroke(
                    connector,
                    CONNECTOR_RADIUS + 3.0,
                    egui::Stroke::new(2.0, colors::SUCCESS),
                );
            }
            let glyph = if endpoint.error.is_some() {
                "!"
            } else if endpoint.pending {
                "…"
            } else if endpoint.policy == ConnectionPolicy::OwnerManaged {
                "◆"
            } else {
                data_type_glyph(endpoint.data_type)
            };
            let text_rect = rect.shrink2(egui::vec2(12.0, 3.0));
            let align = if column.connector_on_right() {
                egui::Align2::LEFT_CENTER
            } else {
                egui::Align2::RIGHT_CENTER
            };
            let text = if column.connector_on_right() {
                format!("{glyph}  {}", endpoint.label)
            } else {
                format!("{}  {glyph}", endpoint.label)
            };
            ui.painter().with_clip_rect(text_rect).text(
                match align {
                    egui::Align2::LEFT_CENTER => text_rect.left_center(),
                    _ => text_rect.right_center(),
                },
                align,
                text,
                egui::TextStyle::Body.resolve(ui.style()),
                ui.visuals().text_color(),
            );
            anchors.insert(
                endpoint.id.clone(),
                EndpointAnchor {
                    center: connector,
                    hit_rect,
                },
            );
            #[cfg(test)]
            self.endpoint_rects.insert(endpoint.id.clone(), rect);
        }
    }

    fn paint_routes_and_interact(
        &mut self,
        ui: &mut egui::Ui,
        clip_rect: egui::Rect,
        graph: &ConnectionGraph,
        anchors: &BTreeMap<EndpointId, EndpointAnchor>,
        intents: &mut Vec<AppIntent>,
    ) {
        let painter = ui.painter().with_clip_rect(clip_rect);
        let pointer = ui.input(|input| input.pointer.hover_pos());
        let was_dragging = self.drag.is_some();
        let mut hovered_route = None;
        let mut closest_distance = f32::INFINITY;
        let mut route_shapes = Vec::new();
        for (index, route) in graph.routes.iter().enumerate() {
            let (Some(source), Some(sink)) = (anchors.get(&route.source), anchors.get(&route.sink))
            else {
                continue;
            };
            let curve = route_curve(source.center, sink.center);
            let points = curve.flatten(Some(1.0));
            #[cfg(test)]
            self.route_points.push((
                route.application_port_id,
                route.host_port_id.clone(),
                points.clone(),
            ));
            if let Some(pointer) = pointer.filter(|point| clip_rect.contains(*point)) {
                let distance = distance_to_polyline(pointer, &points);
                if distance <= CURVE_HIT_DISTANCE && distance < closest_distance {
                    closest_distance = distance;
                    hovered_route = Some(index);
                }
            }
            route_shapes.push((route, curve, points));
        }

        for (index, (route, curve, points)) in route_shapes.into_iter().enumerate() {
            let hovered = hovered_route == Some(index);
            let midpoint = curve.sample(0.5);
            let color = match route.state {
                GraphRouteState::Error => colors::ERROR,
                GraphRouteState::PendingConnect | GraphRouteState::PendingDisconnect => {
                    colors::WARNING
                }
                GraphRouteState::Confirmed => data_type_color(route.data_type),
            };
            let stroke = egui::Stroke::new(if hovered { 4.0 } else { 2.0 }, color);
            if matches!(route.state, GraphRouteState::Confirmed) {
                let mut shape = curve;
                shape.stroke = stroke.into();
                painter.add(shape);
            } else {
                paint_dashed_polyline(&painter, &points, stroke);
            }
            if route.policy == ConnectionPolicy::OwnerManaged {
                paint_diamond(&painter, midpoint, color);
            } else if route.state == GraphRouteState::PendingDisconnect {
                painter.text(
                    midpoint,
                    egui::Align2::CENTER_CENTER,
                    "−",
                    egui::TextStyle::Heading.resolve(ui.style()),
                    color,
                );
            }
        }

        if let Some(index) = hovered_route {
            let route = &graph.routes[index];
            #[cfg(test)]
            {
                self.hovered_route = Some((route.application_port_id, route.host_port_id.clone()));
            }
            ui.ctx().set_cursor_icon(
                if route.policy == ConnectionPolicy::UserManaged
                    && route.state == GraphRouteState::Confirmed
                {
                    egui::CursorIcon::PointingHand
                } else {
                    egui::CursorIcon::NotAllowed
                },
            );
            let tooltip = route_hover(route);
            let _ = egui::Tooltip::always_open(
                ui.ctx().clone(),
                ui.layer_id(),
                ui.id().with(("route_tooltip", index)),
                egui::PopupAnchor::Pointer,
            )
            .show(|ui| {
                ui.label(tooltip);
            });
            let clicked = ui.input(|input| input.pointer.primary_clicked());
            if clicked
                && !was_dragging
                && route.policy == ConnectionPolicy::UserManaged
                && route.state == GraphRouteState::Confirmed
            {
                intents.push(AppIntent::SetPortConnected {
                    port_id: route.application_port_id,
                    host_port_id: route.host_port_id.clone(),
                    connected: false,
                });
            }
        }
    }

    fn paint_drag(
        &self,
        ui: &mut egui::Ui,
        clip_rect: egui::Rect,
        graph: &ConnectionGraph,
        anchors: &BTreeMap<EndpointId, EndpointAnchor>,
    ) {
        let Some(drag) = &self.drag else {
            return;
        };
        let (Some(source), Some(pointer), Some(endpoint)) = (
            anchors.get(&drag.source),
            ui.input(|input| input.pointer.interact_pos()),
            graph.endpoint(&drag.source),
        ) else {
            return;
        };
        let painter = ui.painter().with_clip_rect(clip_rect);
        let mut curve = route_curve(source.center, pointer);
        curve.stroke = egui::Stroke::new(2.0, data_type_color(endpoint.data_type)).into();
        painter.add(curve);
    }
}

fn route_intent(
    graph: &ConnectionGraph,
    source: &EndpointId,
    sink: &EndpointId,
    connected: bool,
) -> Option<AppIntent> {
    let (application_port_id, host_port_id) = match (source, sink) {
        (EndpointId::Host(host), EndpointId::Application(port))
        | (EndpointId::Application(port), EndpointId::Host(host)) => (*port, host.clone()),
        _ => return None,
    };
    graph
        .compatible_drop(source, sink)
        .then_some(AppIntent::SetPortConnected {
            port_id: application_port_id,
            host_port_id,
            connected,
        })
}

fn route_curve(source: egui::Pos2, sink: egui::Pos2) -> egui::epaint::CubicBezierShape {
    let control = ((sink.x - source.x).abs() * 0.55).max(24.0);
    egui::epaint::CubicBezierShape::from_points_stroke(
        [
            source,
            source + egui::vec2(control, 0.0),
            sink - egui::vec2(control, 0.0),
            sink,
        ],
        false,
        egui::Color32::TRANSPARENT,
        egui::Stroke::NONE,
    )
}

fn distance_to_polyline(pointer: egui::Pos2, points: &[egui::Pos2]) -> f32 {
    points
        .windows(2)
        .map(|segment| distance_to_segment(pointer, segment[0], segment[1]))
        .fold(f32::INFINITY, f32::min)
}

fn distance_to_segment(point: egui::Pos2, start: egui::Pos2, end: egui::Pos2) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_sq();
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }
    let projection = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance(start + segment * projection)
}

fn paint_dashed_polyline(painter: &egui::Painter, points: &[egui::Pos2], stroke: egui::Stroke) {
    for (index, segment) in points.windows(2).enumerate() {
        if index % 2 == 0 {
            painter.line_segment([segment[0], segment[1]], stroke);
        }
    }
}

fn paint_diamond(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    painter.add(egui::Shape::convex_polygon(
        vec![
            center + egui::vec2(0.0, -5.0),
            center + egui::vec2(5.0, 0.0),
            center + egui::vec2(0.0, 5.0),
            center + egui::vec2(-5.0, 0.0),
        ],
        color,
        egui::Stroke::NONE,
    ));
}

fn endpoint_hover(endpoint: &GraphEndpoint, source: bool) -> String {
    let mut text = format!(
        "{}\n{} {}",
        endpoint.full_name,
        match endpoint.data_type {
            PortDataType::Audio => "Audio",
            PortDataType::Midi => "MIDI",
        },
        if source { "source" } else { "sink" }
    );
    if endpoint.policy == ConnectionPolicy::OwnerManaged {
        text.push_str("\nConnections are managed by the port owner");
    } else if source {
        text.push_str("\nDrag to a compatible sink to connect");
    }
    if endpoint.pending {
        text.push_str("\nA connection change is pending");
    }
    if let Some(error) = &endpoint.error {
        text.push_str("\n");
        text.push_str(error);
    }
    text
}

fn route_hover(route: &GraphRoute) -> String {
    match route.state {
        GraphRouteState::Confirmed if route.policy == ConnectionPolicy::UserManaged => {
            "Connected; click the line to disconnect".to_owned()
        }
        GraphRouteState::Confirmed => "Connected; managed by the port owner".to_owned(),
        GraphRouteState::PendingConnect => "Connecting…".to_owned(),
        GraphRouteState::PendingDisconnect => "Disconnecting…".to_owned(),
        GraphRouteState::Error => route
            .error
            .clone()
            .unwrap_or_else(|| "Connection failed".to_owned()),
    }
}

fn data_type_color(data_type: PortDataType) -> egui::Color32 {
    match data_type {
        PortDataType::Audio => colors::AUDIO_ACTIVITY,
        PortDataType::Midi => colors::MIDI_ACTIVITY,
    }
}

fn data_type_glyph(data_type: PortDataType) -> &'static str {
    match data_type {
        PortDataType::Audio => "A",
        PortDataType::Midi => "M",
    }
}

fn external_name_parts(full_name: &str) -> (String, String) {
    match full_name.split_once(':') {
        Some((client, port)) => (client.to_owned(), port.to_owned()),
        None => ("Other".to_owned(), full_name.to_owned()),
    }
}

fn short_port_name(full_name: &str) -> &str {
    full_name
        .split_once(':')
        .map(|(_, port)| port)
        .unwrap_or(full_name)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        ConfirmedConnectionState, ConnectionErrorState, PendingConnectionState, PortRole,
        ScriptActivityDiagnostics, ScriptKind, ScriptLifecycle, ScriptMidiDiagnostics, ScriptState,
        ScriptingState, TrackPortOwnerKind, TrackState,
    };

    fn application_port(
        id: u64,
        track_id: TrackId,
        name: &str,
        data_type: PortDataType,
        direction: PortDirection,
        policy: ConnectionPolicy,
    ) -> ApplicationPortState {
        ApplicationPortState {
            id: PortId::from_raw(id),
            owner: ApplicationPortOwner::Track {
                track_id,
                kind: TrackPortOwnerKind::Main,
            },
            name: name.to_owned(),
            data_type,
            direction,
            role: match (data_type, direction) {
                (PortDataType::Audio, PortDirection::Input) => PortRole::AudioInput,
                (PortDataType::Audio, PortDirection::Output) => PortRole::AudioOutput,
                (PortDataType::Midi, PortDirection::Input) => PortRole::MidiInput,
                (PortDataType::Midi, PortDirection::Output) => PortRole::MidiOutput,
            },
            connection_policy: policy,
        }
    }

    fn host_port(
        id: &str,
        name: &str,
        data_type: PortDataType,
        direction: PortDirection,
    ) -> HostPortState {
        HostPortState {
            id: HostPortId::new(id),
            name: name.to_owned(),
            data_type,
            direction,
        }
    }

    fn state() -> AppState {
        let one = TrackId::from_raw(1);
        let two = TrackId::from_raw(2);
        AppState {
            tracks: vec![
                TrackState {
                    id: one,
                    name: "One".to_owned(),
                    ..Default::default()
                },
                TrackState {
                    id: two,
                    name: "Two".to_owned(),
                    ..Default::default()
                },
            ],
            connections: Arc::new(ConnectionViewState {
                revision: 2,
                loading: false,
                backend_available: true,
                application_ports: Arc::from([
                    application_port(
                        11,
                        one,
                        "one:audio_in",
                        PortDataType::Audio,
                        PortDirection::Input,
                        ConnectionPolicy::UserManaged,
                    ),
                    application_port(
                        12,
                        one,
                        "one:midi_out",
                        PortDataType::Midi,
                        PortDirection::Output,
                        ConnectionPolicy::UserManaged,
                    ),
                    application_port(
                        13,
                        two,
                        "two:audio_out",
                        PortDataType::Audio,
                        PortDirection::Output,
                        ConnectionPolicy::UserManaged,
                    ),
                ]),
                host_ports: Arc::from([
                    host_port(
                        "device:audio_source",
                        "device:audio_source",
                        PortDataType::Audio,
                        PortDirection::Output,
                    ),
                    host_port(
                        "synth:midi_sink",
                        "synth:midi_sink",
                        PortDataType::Midi,
                        PortDirection::Input,
                    ),
                    host_port(
                        "speaker:audio_sink",
                        "speaker:audio_sink",
                        PortDataType::Audio,
                        PortDirection::Input,
                    ),
                    host_port(
                        "orphan:midi_source",
                        "orphan:midi_source",
                        PortDataType::Midi,
                        PortDirection::Output,
                    ),
                ]),
                confirmed_links: Arc::from([ConfirmedConnectionState {
                    application_port_id: PortId::from_raw(12),
                    host_port_id: HostPortId::new("synth:midi_sink"),
                }]),
                pending_links: Arc::from([]),
                errors: Arc::from([]),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn graph_classifies_four_columns_and_prunes_incompatible_system_ports() {
        let state = state();
        let graph = ConnectionGraph::build(
            &state,
            &ConnectionFilters::for_scope(ConnectionScope::AllTracks),
        );
        assert_eq!(
            GraphColumn::ORDERED.map(GraphColumn::label),
            [
                "System sources",
                "ShoopDaLoop sinks",
                "ShoopDaLoop sources",
                "System sinks"
            ]
        );
        assert_eq!(graph.column(GraphColumn::SystemSources).len(), 1);
        assert_eq!(graph.column(GraphColumn::ShoopSinks).len(), 1);
        assert_eq!(graph.column(GraphColumn::ShoopSources).len(), 2);
        assert_eq!(graph.column(GraphColumn::SystemSinks).len(), 2);
        assert!(graph
            .endpoints
            .values()
            .flatten()
            .all(|endpoint| endpoint.full_name != "orphan:midi_source"));
    }

    #[test]
    fn data_type_and_multi_track_filters_remove_endpoints_and_routes() {
        let state = state();
        let midi_track_one = ConnectionGraph::build(
            &state,
            &ConnectionFilters {
                audio: false,
                midi: true,
                tracks: TrackFilter::Selected(BTreeSet::from([TrackId::from_raw(1)])),
            },
        );
        assert!(midi_track_one
            .endpoints
            .values()
            .flatten()
            .all(|endpoint| endpoint.data_type == PortDataType::Midi));
        assert_eq!(midi_track_one.routes.len(), 1);
        let all_ids: BTreeSet<_> = midi_track_one
            .endpoints
            .values()
            .flatten()
            .map(|endpoint| endpoint.id.clone())
            .collect();
        assert!(all_ids.contains(&EndpointId::Application(PortId::from_raw(12))));
        assert!(!all_ids.contains(&EndpointId::Application(PortId::from_raw(13))));

        let both_tracks = ConnectionGraph::build(
            &state,
            &ConnectionFilters {
                audio: true,
                midi: true,
                tracks: TrackFilter::Selected(BTreeSet::from([
                    TrackId::from_raw(1),
                    TrackId::from_raw(2),
                ])),
            },
        );
        let application_ids: BTreeSet<_> = both_tracks
            .endpoints
            .values()
            .flatten()
            .filter_map(|endpoint| match endpoint.id {
                EndpointId::Application(id) => Some(id),
                EndpointId::Host(_) => None,
            })
            .collect();
        assert_eq!(
            application_ids,
            BTreeSet::from([
                PortId::from_raw(11),
                PortId::from_raw(12),
                PortId::from_raw(13),
            ])
        );
    }

    #[test]
    fn system_and_application_groups_use_user_facing_names_and_stable_fallbacks() {
        let mut state = state();
        Arc::make_mut(&mut state.connections).host_ports = Arc::from([
            host_port(
                "plain-id",
                "plain",
                PortDataType::Audio,
                PortDirection::Output,
            ),
            host_port(
                "z:port",
                "z:port",
                PortDataType::Audio,
                PortDirection::Output,
            ),
        ]);
        let graph = ConnectionGraph::build(
            &state,
            &ConnectionFilters::for_scope(ConnectionScope::AllTracks),
        );
        assert_eq!(
            graph
                .column(GraphColumn::SystemSources)
                .iter()
                .map(|endpoint| endpoint.group.as_str())
                .collect::<Vec<_>>(),
            ["Other", "z"]
        );
        assert_eq!(graph.column(GraphColumn::ShoopSinks)[0].group, "One");
    }

    #[test]
    fn selected_tracks_exclude_lua_ports_but_global_filter_groups_them_by_script() {
        let mut state = state();
        let script_id = ScriptId::from_raw(7);
        state.scripting = Arc::new(ScriptingState {
            scripts: Arc::from([ScriptState {
                id: script_id,
                name: "APC".to_owned(),
                kind: ScriptKind::Bundled,
                enabled: true,
                lifecycle: ScriptLifecycle::Listening,
                documentation: None,
                latest_error: None,
                activity: ScriptActivityDiagnostics::default(),
                midi: ScriptMidiDiagnostics::default(),
                logs: Arc::from([]),
            }]),
            ..Default::default()
        });
        let connections = Arc::make_mut(&mut state.connections);
        let mut ports = connections.application_ports.to_vec();
        ports.push(ApplicationPortState {
            id: PortId::from_raw(99),
            owner: ApplicationPortOwner::LuaControl {
                script_id,
                registration: 0,
            },
            name: "APC:midi_in".to_owned(),
            data_type: PortDataType::Midi,
            direction: PortDirection::Input,
            role: PortRole::MidiInput,
            connection_policy: ConnectionPolicy::OwnerManaged,
        });
        connections.application_ports = ports.into();
        let mut confirmed_links = connections.confirmed_links.to_vec();
        confirmed_links.push(ConfirmedConnectionState {
            application_port_id: PortId::from_raw(99),
            host_port_id: HostPortId::new("orphan:midi_source"),
        });
        connections.confirmed_links = confirmed_links.into();
        let global = ConnectionGraph::build(
            &state,
            &ConnectionFilters::for_scope(ConnectionScope::AllTracks),
        );
        assert!(global
            .column(GraphColumn::ShoopSinks)
            .iter()
            .any(|endpoint| endpoint.group == "Script: APC"));
        let managed_route = global
            .routes
            .iter()
            .find(|route| route.application_port_id == PortId::from_raw(99))
            .expect("Lua owner-managed route should remain visible");
        assert_eq!(managed_route.policy, ConnectionPolicy::OwnerManaged);
        assert_eq!(managed_route.state, GraphRouteState::Confirmed);
        assert!(!global.compatible_drop(&managed_route.source, &managed_route.sink));
        let selected = ConnectionGraph::build(
            &state,
            &ConnectionFilters::for_scope(ConnectionScope::Track(TrackId::from_raw(1))),
        );
        assert!(!selected
            .endpoints
            .values()
            .flatten()
            .any(|endpoint| endpoint.id == EndpointId::Application(PortId::from_raw(99))));
    }

    #[test]
    fn duplicate_display_names_retain_distinct_stable_host_identities() {
        let mut state = state();
        let connections = Arc::make_mut(&mut state.connections);
        let mut host_ports = connections.host_ports.to_vec();
        host_ports.extend([
            host_port(
                "duplicate-id-1",
                "Duplicate Device:output",
                PortDataType::Audio,
                PortDirection::Output,
            ),
            host_port(
                "duplicate-id-2",
                "Duplicate Device:output",
                PortDataType::Audio,
                PortDirection::Output,
            ),
        ]);
        connections.host_ports = host_ports.into();
        let graph = ConnectionGraph::build(
            &state,
            &ConnectionFilters::for_scope(ConnectionScope::AllTracks),
        );
        let duplicates: Vec<_> = graph
            .column(GraphColumn::SystemSources)
            .iter()
            .filter(|endpoint| endpoint.full_name == "Duplicate Device:output")
            .map(|endpoint| endpoint.id.clone())
            .collect();
        assert_eq!(
            duplicates,
            [
                EndpointId::Host(HostPortId::new("duplicate-id-1")),
                EndpointId::Host(HostPortId::new("duplicate-id-2")),
            ]
        );
    }

    #[test]
    fn compatibility_rejects_wrong_lanes_types_managed_and_existing_pairs() {
        let state = state();
        let mut graph = ConnectionGraph::build(
            &state,
            &ConnectionFilters::for_scope(ConnectionScope::AllTracks),
        );
        let host_source = EndpointId::Host(HostPortId::new("device:audio_source"));
        let app_sink = EndpointId::Application(PortId::from_raw(11));
        assert!(graph.compatible_drop(&host_source, &app_sink));
        assert!(!graph.compatible_drop(&app_sink, &host_source));
        assert!(!graph.compatible_drop(
            &EndpointId::Application(PortId::from_raw(12)),
            &EndpointId::Host(HostPortId::new("synth:midi_sink"))
        ));
        graph.endpoints.get_mut(&GraphColumn::ShoopSinks).unwrap()[0].policy =
            ConnectionPolicy::OwnerManaged;
        assert!(!graph.compatible_drop(&host_source, &app_sink));
    }

    #[test]
    fn pending_disconnect_overrides_confirmed_and_errors_remain_visible() {
        let mut state = state();
        let connections = Arc::make_mut(&mut state.connections);
        connections.pending_links = Arc::from([PendingConnectionState {
            application_port_id: PortId::from_raw(12),
            host_port_id: HostPortId::new("synth:midi_sink"),
            desired_connected: false,
        }]);
        connections.errors = Arc::from([ConnectionErrorState {
            port_id: Some(PortId::from_raw(11)),
            external_port: Some("device:audio_source".to_owned()),
            kind: crate::ConnectionErrorKind::BackendRejected,
            message: "rejected".to_owned(),
        }]);
        let graph = ConnectionGraph::build(
            &state,
            &ConnectionFilters::for_scope(ConnectionScope::AllTracks),
        );
        assert!(graph
            .routes
            .iter()
            .any(|route| route.state == GraphRouteState::PendingDisconnect));
        assert!(graph.routes.iter().any(|route| {
            route.state == GraphRouteState::Error && route.error.as_deref() == Some("rejected")
        }));
    }

    #[test]
    fn failed_route_can_be_retried_after_remaining_visible() {
        let mut state = state();
        Arc::make_mut(&mut state.connections).errors = Arc::from([ConnectionErrorState {
            port_id: Some(PortId::from_raw(11)),
            external_port: Some("device:audio_source".to_owned()),
            kind: crate::ConnectionErrorKind::BackendRejected,
            message: "try again".to_owned(),
        }]);
        let graph = ConnectionGraph::build(
            &state,
            &ConnectionFilters::for_scope(ConnectionScope::AllTracks),
        );
        let source = EndpointId::Host(HostPortId::new("device:audio_source"));
        let sink = EndpointId::Application(PortId::from_raw(11));
        assert!(graph.routes.iter().any(|route| {
            route.source == source && route.sink == sink && route.state == GraphRouteState::Error
        }));
        assert!(graph.compatible_drop(&source, &sink));
    }

    #[test]
    fn open_always_applies_global_or_track_filter_presets() {
        let mut dialog = ConnectionDialog::default();
        dialog.filters.audio = false;
        dialog.open(ConnectionScope::Track(TrackId::from_raw(2)));
        assert!(dialog.filters.audio && dialog.filters.midi);
        assert_eq!(
            dialog.filters.tracks,
            TrackFilter::Selected(BTreeSet::from([TrackId::from_raw(2)]))
        );
        dialog.filters.midi = false;
        dialog.open(ConnectionScope::AllTracks);
        assert_eq!(
            dialog.filters,
            ConnectionFilters::for_scope(ConnectionScope::AllTracks)
        );
    }

    #[test]
    fn layout_paints_all_columns_at_small_and_common_sizes() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = state();
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::AllTracks);
        for size in [egui::vec2(360.0, 220.0), egui::vec2(1100.0, 700.0)] {
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| assert!(dialog.show(ui.ctx(), &state).is_empty()),
            );
            assert!(output.shapes.len() > 10);
            assert_eq!(dialog.endpoint_rects.len(), 6);
            assert_eq!(dialog.route_points.len(), 1);
        }
    }

    #[test]
    fn loading_unavailable_and_no_filter_results_are_safe_and_truthful() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut state = state();
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::AllTracks);

        Arc::make_mut(&mut state.connections).loading = true;
        let loading = context.run_ui(Default::default(), |ui| {
            assert!(dialog.show(ui.ctx(), &state).is_empty());
        });
        assert!(loading.shapes.len() > 2);
        assert!(dialog.endpoint_rects.is_empty());

        let connections = Arc::make_mut(&mut state.connections);
        connections.loading = false;
        connections.backend_available = false;
        let unavailable = context.run_ui(Default::default(), |ui| {
            assert!(dialog.show(ui.ctx(), &state).is_empty());
        });
        assert!(unavailable.shapes.len() > 10);
        assert_eq!(dialog.endpoint_rects.len(), 6);

        dialog.filters.audio = false;
        dialog.filters.midi = false;
        let filtered = context.run_ui(Default::default(), |ui| {
            assert!(dialog.show(ui.ctx(), &state).is_empty());
        });
        assert!(filtered.shapes.len() > 2);
        assert!(dialog.endpoint_rects.is_empty());
        assert!(dialog.route_points.is_empty());
    }

    fn frame(
        context: &egui::Context,
        dialog: &mut ConnectionDialog,
        state: &AppState,
        events: Vec<egui::Event>,
    ) -> Vec<AppIntent> {
        frame_at_size(context, dialog, state, egui::vec2(1100.0, 700.0), events)
    }

    fn frame_at_size(
        context: &egui::Context,
        dialog: &mut ConnectionDialog,
        state: &AppState,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> Vec<AppIntent> {
        let mut intents = Vec::new();
        let _ = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| intents = dialog.show(ui.ctx(), state),
        );
        intents
    }

    fn press(position: egui::Pos2) -> egui::Event {
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn release(position: egui::Pos2) -> egui::Event {
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn connector(dialog: &ConnectionDialog, id: &EndpointId, right: bool) -> egui::Pos2 {
        let rect = dialog.endpoint_rects[id];
        egui::pos2(
            if right {
                rect.right() - 3.0
            } else {
                rect.left() + 3.0
            },
            rect.center().y,
        )
    }

    fn drag_route(
        context: &egui::Context,
        dialog: &mut ConnectionDialog,
        state: &AppState,
        source: egui::Pos2,
        sink: egui::Pos2,
    ) -> Vec<AppIntent> {
        assert!(frame(
            context,
            dialog,
            state,
            vec![egui::Event::PointerMoved(source), press(source)]
        )
        .is_empty());
        assert!(dialog.drag.is_some(), "source press did not start a drag");
        let start_drag = source + egui::vec2(18.0, 0.0);
        assert!(frame(
            context,
            dialog,
            state,
            vec![egui::Event::PointerMoved(start_drag)]
        )
        .is_empty());
        assert!(frame(
            context,
            dialog,
            state,
            vec![egui::Event::PointerMoved(sink)]
        )
        .is_empty());
        frame(
            context,
            dialog,
            state,
            vec![egui::Event::PointerMoved(sink), release(sink)],
        )
    }

    #[test]
    fn dragging_across_either_lane_emits_one_exact_connect_intent() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = state();
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::AllTracks);
        assert!(frame(&context, &mut dialog, &state, Vec::new()).is_empty());
        let system_source = EndpointId::Host(HostPortId::new("device:audio_source"));
        let shoop_sink = EndpointId::Application(PortId::from_raw(11));
        let source = connector(&dialog, &system_source, true);
        let sink = connector(&dialog, &shoop_sink, false);
        assert_eq!(
            drag_route(&context, &mut dialog, &state, source, sink),
            [AppIntent::SetPortConnected {
                port_id: PortId::from_raw(11),
                host_port_id: HostPortId::new("device:audio_source"),
                connected: true,
            }]
        );

        let shoop_source = EndpointId::Application(PortId::from_raw(13));
        let system_sink = EndpointId::Host(HostPortId::new("speaker:audio_sink"));
        let source = connector(&dialog, &shoop_source, true);
        let sink = connector(&dialog, &system_sink, false);
        assert_eq!(
            drag_route(&context, &mut dialog, &state, source, sink),
            [AppIntent::SetPortConnected {
                port_id: PortId::from_raw(13),
                host_port_id: HostPortId::new("speaker:audio_sink"),
                connected: true,
            }]
        );
    }

    #[test]
    fn invalid_managed_and_pending_drops_emit_no_intents() {
        let context = egui::Context::default();
        crate::initialize(&context);
        for mode in ["invalid", "managed", "pending"] {
            let mut state = state();
            if mode == "managed" {
                let connections = Arc::make_mut(&mut state.connections);
                Arc::make_mut(&mut connections.application_ports)[0].connection_policy =
                    ConnectionPolicy::OwnerManaged;
            } else if mode == "pending" {
                Arc::make_mut(&mut state.connections).pending_links =
                    Arc::from([PendingConnectionState {
                        application_port_id: PortId::from_raw(11),
                        host_port_id: HostPortId::new("device:audio_source"),
                        desired_connected: true,
                    }]);
            }
            let mut dialog = ConnectionDialog::default();
            dialog.open(ConnectionScope::AllTracks);
            assert!(frame(&context, &mut dialog, &state, Vec::new()).is_empty());
            let source_id = EndpointId::Host(HostPortId::new("device:audio_source"));
            let sink_id = if mode == "invalid" {
                EndpointId::Host(HostPortId::new("speaker:audio_sink"))
            } else {
                EndpointId::Application(PortId::from_raw(11))
            };
            let source = connector(&dialog, &source_id, true);
            let sink = connector(&dialog, &sink_id, false);
            assert!(drag_route(&context, &mut dialog, &state, source, sink).is_empty());
        }
    }

    #[test]
    fn clicking_confirmed_user_managed_curve_emits_exact_disconnect() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = state();
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::AllTracks);
        assert!(frame(&context, &mut dialog, &state, Vec::new()).is_empty());
        let points = &dialog.route_points[0].2;
        let point = points[points.len() / 2];
        assert!(frame(
            &context,
            &mut dialog,
            &state,
            vec![egui::Event::PointerMoved(point), press(point)]
        )
        .is_empty());
        assert_eq!(
            frame(
                &context,
                &mut dialog,
                &state,
                vec![egui::Event::PointerMoved(point), release(point)]
            ),
            [AppIntent::SetPortConnected {
                port_id: PortId::from_raw(12),
                host_port_id: HostPortId::new("synth:midi_sink"),
                connected: false,
            }]
        );
    }

    #[test]
    fn nearby_curves_disconnect_only_the_nearest_route() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut state = state();
        let connections = Arc::make_mut(&mut state.connections);
        let mut hosts = connections.host_ports.to_vec();
        hosts.push(host_port(
            "synth2:midi_sink",
            "synth2:midi_sink",
            PortDataType::Midi,
            PortDirection::Input,
        ));
        connections.host_ports = hosts.into();
        connections.confirmed_links = Arc::from([
            ConfirmedConnectionState {
                application_port_id: PortId::from_raw(12),
                host_port_id: HostPortId::new("synth:midi_sink"),
            },
            ConfirmedConnectionState {
                application_port_id: PortId::from_raw(12),
                host_port_id: HostPortId::new("synth2:midi_sink"),
            },
        ]);
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::AllTracks);
        assert!(frame(&context, &mut dialog, &state, Vec::new()).is_empty());
        let points = &dialog
            .route_points
            .iter()
            .find(|(_, host, _)| host.as_str() == "synth2:midi_sink")
            .unwrap()
            .2;
        let point = points[points.len() * 3 / 4];
        assert!(frame(
            &context,
            &mut dialog,
            &state,
            vec![egui::Event::PointerMoved(point), press(point)]
        )
        .is_empty());
        assert_eq!(
            frame(
                &context,
                &mut dialog,
                &state,
                vec![egui::Event::PointerMoved(point), release(point)]
            ),
            [AppIntent::SetPortConnected {
                port_id: PortId::from_raw(12),
                host_port_id: HostPortId::new("synth2:midi_sink"),
                connected: false,
            }]
        );
    }

    #[test]
    fn a_horizontally_scrolled_visible_curve_remains_interactive() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = state();
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::AllTracks);
        let size = egui::vec2(520.0, 360.0);
        assert!(frame_at_size(&context, &mut dialog, &state, size, Vec::new()).is_empty());
        let graph_position = dialog.graph_clip_rect.unwrap().center();
        assert!(frame_at_size(
            &context,
            &mut dialog,
            &state,
            size,
            vec![
                egui::Event::PointerMoved(graph_position),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(-700.0, 0.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        )
        .is_empty());
        for _ in 0..30 {
            assert!(frame_at_size(&context, &mut dialog, &state, size, Vec::new()).is_empty());
        }
        let clip = dialog.graph_clip_rect.unwrap();
        let points = &dialog.route_points[0].2;
        let point = points
            .iter()
            .copied()
            .filter(|point| clip.shrink(8.0).contains(*point))
            .min_by(|left, right| {
                left.distance(clip.center())
                    .total_cmp(&right.distance(clip.center()))
            })
            .expect("scrolled route should enter the clipped viewport");
        assert!(frame_at_size(
            &context,
            &mut dialog,
            &state,
            size,
            vec![egui::Event::PointerMoved(point)]
        )
        .is_empty());
        assert_eq!(
            dialog.hovered_route,
            Some((PortId::from_raw(12), HostPortId::new("synth:midi_sink")))
        );
        assert!(frame_at_size(
            &context,
            &mut dialog,
            &state,
            size,
            vec![egui::Event::PointerMoved(point), press(point)]
        )
        .is_empty());
        assert_eq!(
            dialog.hovered_route,
            Some((PortId::from_raw(12), HostPortId::new("synth:midi_sink")))
        );
        let release_point = dialog.route_points[0]
            .2
            .iter()
            .copied()
            .filter(|candidate| clip.shrink(8.0).contains(*candidate))
            .min_by(|left, right| left.distance(point).total_cmp(&right.distance(point)))
            .unwrap();
        assert_eq!(
            frame_at_size(
                &context,
                &mut dialog,
                &state,
                size,
                vec![
                    egui::Event::PointerMoved(release_point),
                    release(release_point),
                ]
            ),
            [AppIntent::SetPortConnected {
                port_id: PortId::from_raw(12),
                host_port_id: HostPortId::new("synth:midi_sink"),
                connected: false,
            }]
        );
    }

    #[test]
    fn managed_and_pending_curves_cannot_be_disconnected() {
        let context = egui::Context::default();
        crate::initialize(&context);
        for mode in ["managed", "pending"] {
            let mut state = state();
            let connections = Arc::make_mut(&mut state.connections);
            if mode == "managed" {
                Arc::make_mut(&mut connections.application_ports)[1].connection_policy =
                    ConnectionPolicy::OwnerManaged;
            } else {
                connections.pending_links = Arc::from([PendingConnectionState {
                    application_port_id: PortId::from_raw(12),
                    host_port_id: HostPortId::new("synth:midi_sink"),
                    desired_connected: false,
                }]);
            }
            let mut dialog = ConnectionDialog::default();
            dialog.open(ConnectionScope::AllTracks);
            assert!(frame(&context, &mut dialog, &state, Vec::new()).is_empty());
            let points = &dialog.route_points[0].2;
            let point = points[points.len() / 2];
            assert!(frame(
                &context,
                &mut dialog,
                &state,
                vec![egui::Event::PointerMoved(point), press(point)]
            )
            .is_empty());
            assert!(frame(
                &context,
                &mut dialog,
                &state,
                vec![egui::Event::PointerMoved(point), release(point)]
            )
            .is_empty());
        }
    }

    #[test]
    fn filter_or_snapshot_change_cancels_an_active_drag() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut state = state();
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::AllTracks);
        assert!(frame(&context, &mut dialog, &state, Vec::new()).is_empty());
        let source_id = EndpointId::Host(HostPortId::new("device:audio_source"));
        let source = connector(&dialog, &source_id, true);
        assert!(frame(
            &context,
            &mut dialog,
            &state,
            vec![egui::Event::PointerMoved(source), press(source)]
        )
        .is_empty());
        assert!(dialog.drag.is_some());
        dialog.filters.audio = false;
        assert!(frame(&context, &mut dialog, &state, Vec::new()).is_empty());
        assert!(dialog.drag.is_none());

        dialog.filters.audio = true;
        assert!(frame(&context, &mut dialog, &state, vec![release(source)]).is_empty());
        assert!(frame(
            &context,
            &mut dialog,
            &state,
            vec![egui::Event::PointerMoved(source), press(source)]
        )
        .is_empty());
        assert!(dialog.drag.is_some());
        Arc::make_mut(&mut state.connections).revision += 1;
        assert!(frame(&context, &mut dialog, &state, Vec::new()).is_empty());
        assert!(dialog.drag.is_none());
    }

    #[test]
    fn large_graph_uses_linear_endpoint_layout_and_paints_visible_routes() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let track_id = TrackId::from_raw(1);
        let application_ports: Arc<[ApplicationPortState]> = (0..40)
            .map(|index| {
                application_port(
                    index + 1,
                    track_id,
                    &format!("track:output_{index}"),
                    PortDataType::Audio,
                    PortDirection::Output,
                    ConnectionPolicy::UserManaged,
                )
            })
            .chain((0..40).map(|index| {
                application_port(
                    index + 101,
                    track_id,
                    &format!("track:input_{index}"),
                    PortDataType::Audio,
                    PortDirection::Input,
                    ConnectionPolicy::UserManaged,
                )
            }))
            .collect::<Vec<_>>()
            .into();
        let host_ports: Arc<[HostPortState]> = (0..40)
            .map(|index| {
                host_port(
                    &format!("capture_{index}:out"),
                    &format!("capture_{}:out", index / 4),
                    PortDataType::Audio,
                    PortDirection::Output,
                )
            })
            .chain((0..40).map(|index| {
                host_port(
                    &format!("playback_{index}:in"),
                    &format!("playback_{}:in", index / 4),
                    PortDataType::Audio,
                    PortDirection::Input,
                )
            }))
            .collect::<Vec<_>>()
            .into();
        let confirmed_links: Arc<[ConfirmedConnectionState]> = (0..40)
            .map(|index| ConfirmedConnectionState {
                application_port_id: PortId::from_raw(index + 1),
                host_port_id: HostPortId::new(format!("playback_{index}:in")),
            })
            .collect::<Vec<_>>()
            .into();
        let state = AppState {
            tracks: vec![TrackState {
                id: track_id,
                name: "Large".to_owned(),
                ..Default::default()
            }],
            connections: Arc::new(ConnectionViewState {
                revision: 5,
                loading: false,
                backend_available: true,
                application_ports,
                host_ports,
                confirmed_links,
                pending_links: Arc::from([]),
                errors: Arc::from([]),
            }),
            ..Default::default()
        };
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::AllTracks);
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(360.0, 220.0),
                )),
                ..Default::default()
            },
            |ui| assert!(dialog.show(ui.ctx(), &state).is_empty()),
        );
        assert!(output.shapes.len() > 100);
        assert_eq!(dialog.endpoint_rects.len(), 160);
        assert_eq!(dialog.route_points.len(), 40);
    }

    #[test]
    fn stale_track_warning_does_not_block_switching_back_to_all_tracks() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let state = state();
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::Track(TrackId::from_raw(999)));
        let stale = context.run_ui(Default::default(), |ui| {
            assert!(dialog.show(ui.ctx(), &state).is_empty());
        });
        assert!(stale.shapes.len() > 2);
        assert!(dialog.endpoint_rects.is_empty());
        dialog.filters.tracks = TrackFilter::All;
        let recovered = context.run_ui(Default::default(), |ui| {
            assert!(dialog.show(ui.ctx(), &state).is_empty());
        });
        assert!(recovered.shapes.len() > 10);
        assert!(!dialog.endpoint_rects.is_empty());
    }

    #[test]
    fn empty_host_inventory_keeps_application_ports_visible() {
        let context = egui::Context::default();
        crate::initialize(&context);
        let mut state = state();
        Arc::make_mut(&mut state.connections).host_ports = Arc::from([]);
        Arc::make_mut(&mut state.connections).confirmed_links = Arc::from([]);
        let mut dialog = ConnectionDialog::default();
        dialog.open(ConnectionScope::AllTracks);
        let output = context.run_ui(Default::default(), |ui| {
            assert!(dialog.show(ui.ctx(), &state).is_empty());
        });
        assert!(output.shapes.len() > 5);
        assert_eq!(dialog.endpoint_rects.len(), 3);
    }

    #[test]
    fn name_splitting_matches_grouping_contract() {
        assert_eq!(
            external_name_parts("client:port:part"),
            ("client".to_owned(), "port:part".to_owned())
        );
        assert_eq!(
            external_name_parts("plain"),
            ("Other".to_owned(), "plain".to_owned())
        );
        assert_eq!(short_port_name("client:port"), "port");
    }
}
