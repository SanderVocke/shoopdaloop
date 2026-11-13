use crate::socket::{SocketKind, SocketResponses};
use crate::NodesCtx;
use std::hash::Hash;
use std::ops::{Deref, DerefMut};

/// Derive the `egui::Id` used internally for a node's egui state (layers, area_rect, etc).
pub fn egui_id(graph_id: egui::Id, node_id: NodeId) -> egui::Id {
    graph_id.with(node_id.0)
}

/// Describes the interaction state of a node.
///
/// This is passed to the frame closure to allow styling based on selection state.
#[derive(Clone, Copy, Debug, Default)]
pub struct NodeInteraction {
    /// Whether the node is currently selected.
    pub selected: bool,
    /// Whether the node is within the current selection rectangle.
    pub in_selection_rect: bool,
    /// Whether or not the pointer is hovered over the node.
    pub hovered: bool,
}

/// The default node widget.
///
/// A `Node` is a thin wrapper around a `Window` and allows for instantiating arbitrary widgets
/// internally.
pub struct Node {
    id: NodeId,
    inputs: usize,
    outputs: usize,
    collapsed: bool,
    flow: egui::Direction,
    socket_radius: f32,
    socket_color: Option<egui::Color32>,
    max_width: Option<f32>,
    animation_time: f32,
    content_enabled: bool,
}

/// A unique identifier for a node within a graph.
///
/// This is decoupled from `egui::Id` to allow node identity to remain stable
/// even when the graph's egui ID context changes (e.g., when viewing the same
/// graph through different heads or paths).
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct NodeId(pub u64);

/// An extension around the `egui::Response` that indicates node selection.
pub struct NodeResponse<T> {
    response: egui::InnerResponse<T>,
    sockets: SocketResponses,
    selection_changed: bool,
    selected: bool,
    removed: bool,
    /// Some event occurred related to the creation of an edge.
    edge_event: Option<EdgeEvent>,
}

/// The response returned by [`NodeCtx::framed`] and [`NodeCtx::framed_with`].
///
/// Carries the content's [`egui::InnerResponse`] alongside the
/// [`SocketLayout`](crate::SocketLayout) describing socket positions for this node.
pub struct FramedResponse<T> {
    pub inner: egui::InnerResponse<T>,
    pub sockets: crate::SocketLayout,
}

/// The outcome of the content closure of a node in [`Node::show_impl`].
enum Content<R> {
    /// The content ran.
    Shown(FramedResponse<R>),
    /// The node is out of view, so the content did not run.
    Culled(R),
}

/// The response returned by [`Node::show_static`].
pub struct StaticNodeResponse<T> {
    /// The framed content's response.
    pub inner: egui::InnerResponse<T>,
    /// The hover response of each socket.
    pub sockets: SocketResponses,
}

/// Events related to the creation of an edge to or from a node.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EdgeEvent {
    /// Occurs if a socket was just pressed to start creating an edge.
    Started { kind: SocketKind, index: usize },
    /// Occurs when the primary mouse button was released creating an edge ending at the specified
    /// socket.
    Ended { kind: SocketKind, index: usize },
    /// If there was an edge in progress starting from this node, this indicates that it was
    /// cancelled.
    Cancelled,
}

/// Context passed to the node's content closure.
///
/// Provides access to interaction state and style, and ensures content is properly framed.
pub struct NodeCtx<'a> {
    ui: &'a mut egui::Ui,
    interaction: NodeInteraction,
    min_size: egui::Vec2,
    graph_id: egui::Id,
    node_id: NodeId,
    /// Whether the content widgets are enabled. False for a node with
    /// disabled content and for any node in an immutable graph.
    content_enabled: bool,
    flow: egui::Direction,
    inputs: usize,
    outputs: usize,
}

impl Node {
    const COLLAPSED_SOCKET_GAP_FACTOR: f32 = 0.25;

    /// Begin instantiating a new node widget.
    pub fn new(id_src: impl Hash) -> Self {
        Self::from_id(NodeId::new(id_src))
    }

    /// Construct the node directly from its `NodeId`.
    pub fn from_id(id: NodeId) -> Self {
        Self {
            id,
            max_width: None,
            socket_color: None,
            inputs: 0,
            outputs: 0,
            collapsed: false,
            flow: egui::Direction::LeftToRight,
            socket_radius: 3.0,
            animation_time: 0.1,
            content_enabled: true,
        }
    }

    /// Optionally specify the max width of the `Node`'s window.
    ///
    /// By default, `ui.spacing().text_edit_width` is used.
    pub fn max_width(mut self, w: f32) -> Self {
        self.max_width = Some(w);
        self
    }

    pub fn inputs(mut self, n: usize) -> Self {
        self.inputs = n;
        self
    }

    pub fn outputs(mut self, n: usize) -> Self {
        self.outputs = n;
        self
    }

    /// Allow the node to shrink below the usual socket-spacing-driven minimum size
    /// when there are two or more sockets.
    ///
    /// Collapsed nodes still preserve a small amount of socket separation so
    /// multiple sockets do not fully overlap.
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// The direction of dataflow through the graph.
    ///
    /// This determines which edges the inputs and outputs are distributed across.
    ///
    /// E.g. `LeftToRight` will have inputs on the left and outputs on the right.
    ///
    /// On vertical edges, inputs/outputs start at the top and end at the bottom.
    ///
    /// On horizontal edges, inputs/outputs start at the left and end at the right.
    ///
    /// Default direction is `LeftToRight`.
    pub fn flow(mut self, flow: egui::Direction) -> Self {
        self.flow = flow;
        self
    }

    /// The color of the input and output sockets.
    pub fn socket_color(mut self, color: egui::Color32) -> Self {
        self.socket_color = Some(color);
        self
    }

    /// The radius of the input and output sockets.
    pub fn socket_radius(mut self, radius: f32) -> Self {
        self.socket_radius = radius;
        self
    }

    /// The time taken (seconds) for the node to interpolate toward a new location.
    ///
    /// Default: `0.1`.
    pub fn animation_time(mut self, time: f32) -> Self {
        self.animation_time = time;
        self
    }

    /// Whether the widgets inside the node are enabled.
    ///
    /// A node with disabled content still drags, selects and connects edges,
    /// unlike a node in an [`immutable`](crate::Graph::immutable) graph. Use
    /// this for a single node whose content must not be edited.
    ///
    /// Default: `true`.
    pub fn content_enabled(mut self, enabled: bool) -> Self {
        self.content_enabled = enabled;
        self
    }

    /// Present the `Node`'s `Window` and add the given contents.
    ///
    /// The content closure receives a [`NodeCtx`] which provides access to interaction state
    /// and ensures content is properly framed. Use [`NodeCtx::framed`] for default styling
    /// or [`NodeCtx::framed_with`] for custom frame styling:
    ///
    /// ```ignore
    /// Node::from_id(id).show(ctx, ui, |node_ctx| {
    ///     node_ctx.framed(|ui, _sockets| {
    ///         ui.label("Hello");
    ///     })
    /// });
    /// ```
    pub fn show<R>(
        self,
        ctx: &mut NodesCtx,
        ui: &mut egui::Ui,
        content: impl FnOnce(NodeCtx<'_>) -> FramedResponse<R>,
    ) -> NodeResponse<R> {
        self.show_impl(ctx, ui, Box::new(content) as Box<_>, None)
    }

    /// As [`Node::show`], but `content` does not run while the node is out of
    /// view.
    ///
    /// The node is out of view when its frame, at its size from the last
    /// frame, lies outside the visible area of the graph, with a margin for
    /// the frame shadow and the socket highlights. Then the node keeps its
    /// last size and socket positions, so that edges, selection and the
    /// automatic layout still use them. Selection, removal and edge events
    /// occur as for a node in view. The response holds `None` in place of
    /// the output of `content`, and has no socket responses.
    ///
    /// Use this in a large graph, where most nodes are out of view. Use
    /// [`Node::show`] for content that must run every frame, such as a widget
    /// that must keep keyboard focus while out of view.
    pub fn show_or_cull<R>(
        self,
        ctx: &mut NodesCtx,
        ui: &mut egui::Ui,
        content: impl FnOnce(NodeCtx<'_>) -> FramedResponse<R>,
    ) -> NodeResponse<Option<R>> {
        let content = |node_ctx: NodeCtx<'_>| {
            let FramedResponse { inner, sockets } = content(node_ctx);
            let inner = egui::InnerResponse::new(Some(inner.inner), inner.response);
            FramedResponse { inner, sockets }
        };
        self.show_impl(ctx, ui, Box::new(content) as Box<_>, Some(None))
    }

    /// Show the node as a static widget at the ui cursor, outside any graph.
    ///
    /// The frame and sockets paint exactly as within a
    /// [`Graph`](crate::Graph), but there is no layout, selection, dragging
    /// or edge interaction. `content` must call [`NodeCtx::framed`] or
    /// [`NodeCtx::framed_with`], as within a graph. The socket responses
    /// sense hover only, for tooltips.
    pub fn show_static<R>(
        self,
        ui: &mut egui::Ui,
        content: impl FnOnce(NodeCtx<'_>) -> FramedResponse<R>,
    ) -> StaticNodeResponse<R> {
        let (min_size, content_min_size, socket_padding) = self.min_sizes(ui);
        let max_w = self.max_width.unwrap_or(ui.spacing().text_edit_width);
        let put_size = egui::Vec2::new(max_w, min_size.y);
        let put_rect = egui::Rect::from_min_size(ui.cursor().min, put_size);
        let graph_id = ui.scope_id();
        let egui_id = egui_id(graph_id, self.id);
        let (socket_layer, frame_layer) = sublayers(ui, egui_id, true);

        let builder = egui::UiBuilder::new()
            .max_rect(put_rect)
            .layer_id(frame_layer);
        let inner_response = ui.scope_builder(builder, |ui| {
            let node_ctx = NodeCtx {
                ui,
                interaction: NodeInteraction::default(),
                min_size: content_min_size,
                graph_id,
                node_id: self.id,
                content_enabled: self.content_enabled,
                flow: self.flow,
                inputs: self.inputs,
                outputs: self.outputs,
            };
            content(node_ctx)
        });
        let FramedResponse {
            inner,
            sockets: socket_layout,
        } = inner_response.inner;

        let rect = inner.response.rect;
        let node_sockets = socket_layout.resolve(self.flow, rect, socket_padding);
        let socket_color = self.socket_color.unwrap_or(ui.visuals().text_color());
        let sockets = crate::socket::paint(
            ui,
            egui_id,
            socket_layer,
            &node_sockets,
            socket_color,
            self.socket_radius,
            |_, _| false,
        );
        StaticNodeResponse { inner, sockets }
    }

    /// The minimum frame size that fits the sockets, the minimum content
    /// size within the frame's corner radius, and the socket padding.
    fn min_sizes(&self, ui: &egui::Ui) -> (egui::Vec2, egui::Vec2, f32) {
        // The window should always be at least the interaction size.
        let min_item_spacing = ui.spacing().item_spacing.x.min(ui.spacing().item_spacing.y);
        let min_interact_len = ui
            .spacing()
            .interact_size
            .x
            .min(ui.spacing().interact_size.y);
        let mut min_size = egui::Vec2::splat(min_interact_len);
        // However, it should also always be at least large enough to comfortably show all
        // inlets/outlets.
        let max_sockets = std::cmp::max(self.inputs, self.outputs);
        let min_socket_gap = min_interact_len + min_item_spacing;
        let win_corner_radius = ui.visuals().window_corner_radius.ne as f32;
        let socket_padding = crate::socket::socket_padding(ui.style());
        if max_sockets > 1 {
            let socket_gap_factor = if self.collapsed {
                Self::COLLAPSED_SOCKET_GAP_FACTOR
            } else {
                1.0
            };
            let min_len = (max_sockets - 1) as f32 * min_socket_gap * socket_gap_factor
                + socket_padding * 2.0;
            match self.flow {
                egui::Direction::LeftToRight | egui::Direction::RightToLeft => {
                    min_size.y = min_size.y.max(min_len);
                }
                egui::Direction::TopDown | egui::Direction::BottomUp => {
                    min_size.x = min_size.x.max(min_len);
                }
            }
        }
        // The content accounts for the frame corner radius.
        let gap = egui::Vec2::splat(win_corner_radius * 2.0);
        let content_min_size = min_size - gap;
        (min_size, content_min_size, socket_padding)
    }

    /// Show the node with `content`.
    ///
    /// With `culled`, `content` does not run while the node is out of view,
    /// and the response holds `culled` in place of the output of `content`.
    fn show_impl<'a, R>(
        self,
        ctx: &mut NodesCtx,
        ui: &mut egui::Ui,
        content: Box<dyn FnOnce(NodeCtx<'_>) -> FramedResponse<R> + 'a>,
        culled: Option<R>,
    ) -> NodeResponse<R> {
        let snap = ctx.snap;
        let snap_step = ctx.snap_step;
        let layout = &mut ctx.layout;

        // Indicate that we've visited this node this update.
        ctx.visited.insert(self.id);

        // Determine the current position of the window relative to the graph origin.
        let target_pos_graph = layout.entry(self.id).or_insert_with(|| {
            // If the mouse is over the graph, add the node under the mouse.
            // Otherwise, add the node to the top-left.
            let clip_rect = ui.clip_rect();
            let mut pos = clip_rect.center();
            if ui.rect_contains_pointer(clip_rect) {
                if let Some(ptr) = ui.response().hover_pos() {
                    pos = ptr;
                }
            }
            // Snap the initial position so a freshly-spawned node is aligned on
            // its very first frame (later frames are snapped in `Graph::show`).
            match snap {
                Some(snap) => crate::snap_pos(snap, snap_step, pos),
                None => pos,
            }
        });

        // Interpolate toward the desired position over time for auto-layout.
        // Only do so if this node is not selected with the primary mouse down.
        let is_selected = crate::is_node_selected(ui, ctx.graph_id, self.id);
        let is_primary_down = ui.input(|i| i.pointer.primary_down());
        let animation_time = if is_selected && is_primary_down {
            0.0
        } else {
            self.animation_time
        };
        let pos_graph = {
            let idx = ctx.graph_id.with(self.id.0).with("x");
            let idy = ctx.graph_id.with(self.id.0).with("y");
            let ctx = ui.ctx();
            let x = ctx.animate_value_with_time(idx, target_pos_graph.x, animation_time);
            let y = ctx.animate_value_with_time(idy, target_pos_graph.y, animation_time);
            // With no animation time, egui resets the animation to the target but
            // returns the value from before, which would place the node a pass
            // late.
            match animation_time > 0.0 {
                true => egui::Pos2::new(x, y),
                false => *target_pos_graph,
            }
        };

        let (min_size, content_min_size, socket_padding) = self.min_sizes(ui);

        let max_w = self.max_width.unwrap_or(ui.spacing().text_edit_width);
        let max_size = egui::Vec2::new(max_w, ctx.graph_rect.height());

        // Track changes in selection for the node response.
        let mut selection_changed = false;

        // Determine whether or not this node is within the selection rect.
        // If `shift` is down, rectangle selection is reserved for edges.
        // NOTE: We use the size from last frame as we don't know the size until
        // the user's content is added... Is there a better way to handle this?
        let (mut selected, in_selection_rect, last_size) = {
            let gmem_arc = crate::memory(ui, ctx.graph_id);
            let mut gmem = gmem_arc.lock().expect("failed to lock graph temp memory");
            let last_size = gmem.node_sizes.get(&self.id).copied();
            let in_selection_rect = match ctx.selection_rect {
                Some(sel_rect) if ui.input(|i| !i.modifiers.shift) => {
                    let size = last_size.unwrap_or(egui::Vec2::ZERO);
                    let rect = egui::Rect::from_min_size(pos_graph, size);
                    sel_rect.intersects(rect)
                }
                _ => false,
            };

            // Update the selection if the primary mouse button was just released.
            if ctx.select {
                if in_selection_rect && ui.input(|i| !i.modifiers.shift) {
                    selection_changed |= gmem.selection.nodes.insert(self.id);
                } else if !ui.input(|i| i.modifiers.ctrl) {
                    selection_changed |= gmem.selection.nodes.remove(&self.id);
                }
            }

            let selected = gmem.selection.nodes.contains(&self.id);

            (selected, in_selection_rect, last_size)
        };

        // Custom framed node container that remains in the scene's layer
        let put_size = egui::Vec2::new(max_size.x, min_size.y);
        let put_rect = egui::Rect::from_min_size(pos_graph, put_size);

        // A node out of view skips the work that only affects what is drawn.
        let in_view = last_size.is_none_or(|size| {
            let rect = egui::Rect::from_min_size(pos_graph, size);
            let margin = view_margin(ui.style(), self.socket_radius);
            ui.clip_rect().intersects(rect.expand(margin))
        });
        let culled = culled.zip(last_size).filter(|_| !in_view);

        let node_id = self.id;
        let egui_id = egui_id(ctx.graph_id, node_id);
        let (socket_layer, frame_layer) = sublayers(ui, egui_id, in_view);

        // A `Ui` scope for the node's layer.
        let builder = egui::UiBuilder::new()
            .max_rect(put_rect)
            .layer_id(frame_layer)
            .sense(egui::Sense::click_and_drag());
        let immutable = ctx.immutable;
        let inner_response = ui.scope_builder(builder, |ui| match culled {
            // Take the last size without the content, so that the node uses
            // the same space and egui ids as in view.
            Some((culled, size)) => {
                ui.set_min_size(size);
                Content::Culled(culled)
            }
            None => {
                let hovered = ui.response().hovered();
                // Create the NodeCtx and call the user's content closure.
                // The user is responsible for calling `framed` or `default_framed`
                // on the context.
                let node_ctx = NodeCtx {
                    ui,
                    interaction: NodeInteraction {
                        selected,
                        in_selection_rect,
                        hovered,
                    },
                    min_size: content_min_size,
                    graph_id: ctx.graph_id,
                    node_id,
                    content_enabled: self.content_enabled && !immutable,
                    flow: self.flow,
                    inputs: self.inputs,
                    outputs: self.outputs,
                };
                Content::Shown(content(node_ctx))
            }
        });

        // Take the union of the ui scope and the frame response to monitor for
        // interactions, and resolve the socket layout to concrete positions.
        let (mut response, content_output, node_sockets) = match inner_response.inner {
            Content::Shown(FramedResponse { inner, sockets }) => {
                let response = inner_response.response.union(inner.response);
                let node_sockets = sockets.resolve(self.flow, response.rect, socket_padding);
                (response, inner.inner, Some(node_sockets))
            }
            Content::Culled(culled) => (inner_response.response, culled, None),
        };

        // Update the stored data for this node and check for edge events.
        let mut edge_event = None;
        {
            let gmem_arc = crate::memory(ui, ctx.graph_id);
            let mut gmem = gmem_arc.lock().expect("failed to lock graph temp memory");
            match &node_sockets {
                Some(node_sockets) => {
                    // Snap the recorded size so auto-layout and selection see tidy
                    // values. Sizes always round (never floor) so they can't shrink
                    // below the rendered frame and break hit-testing or packing.
                    let size = match snap {
                        Some(_) => {
                            crate::snap_vec(crate::Snap::Round, snap_step, response.rect.size())
                        }
                        None => response.rect.size(),
                    };
                    gmem.node_sizes.insert(self.id, size);
                    gmem.sockets.insert(self.id, node_sockets.clone());
                }
                // A culled node keeps its size, and its sockets move with it.
                None => {
                    let last_pos = gmem.node_positions.get(&self.id).copied();
                    let delta = last_pos.map_or(egui::Vec2::ZERO, |p| response.rect.min - p);
                    if let Some(sockets) = gmem.sockets.get_mut(&self.id) {
                        sockets.translate(delta);
                    }
                }
            }
            gmem.node_positions.insert(self.id, response.rect.min);

            // Treat the pointer being over this node's frame as "over the graph"
            // for next frame's socket detection while an edge is in progress
            // (see `lib.rs`). `contains_pointer` stays true mid-drag
            // (drag-and-drop target semantics) and is false when a window fully
            // occludes the frame.
            gmem.ptr_over_node |= response.contains_pointer();

            let ctrl_down = ui.input(|i| i.modifiers.ctrl);

            // If the window is pressed, select the node.
            let pointer = &ui.input(|i| i.pointer.clone());
            if response.is_pointer_button_down_on() && pointer.primary_pressed() {
                // If ctrl is down, check for deselection.
                let was_selected = gmem.selection.nodes.contains(&self.id);
                if ctrl_down && was_selected {
                    selection_changed |= gmem.selection.nodes.remove(&self.id);
                    selected = false;
                } else {
                    // Clear other selections if ctrl is not pressed and this is newly pressed.
                    if !ctrl_down && !was_selected {
                        gmem.selection.changed |= !gmem.selection.nodes.is_empty();
                        gmem.selection.nodes.clear();
                    }
                    selection_changed |= gmem.selection.nodes.insert(self.id);
                    selected = true;
                    // Initialize drag - skip when immutable (still allow click-to-select above).
                    if !immutable && gmem.pressed.is_none() {
                        let ptr_graph = response.hover_pos().unwrap_or_default();
                        gmem.pressed = Some(crate::Pressed {
                            over_selection_at_origin: true,
                            origin_pos: ptr_graph,
                            current_pos: ptr_graph,
                            action: crate::PressAction::DragNodes {
                                node: Some(crate::PressedNode {
                                    id: self.id,
                                    position_at_origin: pos_graph,
                                }),
                            },
                        });
                    }
                }

            // If the primary button was pressed, check for edge events (skip when immutable).
            } else if !immutable
                && !response.is_pointer_button_down_on()
                && pointer.primary_pressed()
            {
                // If this node's socket was pressed, create a start event.
                if let Some(ref pressed) = gmem.pressed {
                    if let crate::PressAction::Socket(socket) = pressed.action {
                        if self.id == socket.node {
                            let kind = socket.kind;
                            let index = socket.index;
                            edge_event = Some(EdgeEvent::Started { kind, index });
                        }
                    }
                }

                // Also check for deselection.
                if !ctrl_down
                    && !gmem
                        .pressed
                        .as_ref()
                        .map(|p| p.over_selection_at_origin)
                        .unwrap_or(false)
                    && !gmem.selection.nodes.contains(&self.id)
                {
                    selection_changed |= gmem.selection.nodes.remove(&self.id);
                    selected = false;
                }

            // Check for edge creation / cancellation events (skip when immutable).
            } else if !immutable {
                if let Some(r) = ctx.socket_press_released {
                    if let Some(c) = gmem.closest_socket {
                        if r.kind == c.kind && self.id == r.node {
                            edge_event = Some(EdgeEvent::Cancelled);
                        } else if self.id == c.node && ui.input(|i| i.pointer.primary_released()) {
                            let kind = c.kind;
                            let index = c.index;
                            edge_event = Some(EdgeEvent::Ended { kind, index });
                        }
                    } else if edge_event.is_none() && self.id == r.node {
                        edge_event = Some(EdgeEvent::Cancelled);
                    }
                }
            }
        }

        // Paint and interact with all sockets of a node in view.
        let socket_responses = match node_sockets.filter(|_| in_view) {
            Some(node_sockets) => {
                let socket_color = self.socket_color.unwrap_or(ui.visuals().text_color());
                crate::socket::show(
                    ui,
                    ctx.graph_id,
                    self.id,
                    egui_id,
                    socket_layer,
                    &node_sockets,
                    socket_color,
                    self.socket_radius,
                )
            }
            None => SocketResponses::default(),
        };

        // If the delete or backspace key was pressed and the node is selected, remove it.
        // Skip when immutable.
        let removed = if !immutable
            && selected
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|i| i.key_pressed(egui::Key::Delete) | i.key_pressed(egui::Key::Backspace))
        {
            // Remove ourselves from the selection.
            let gmem_arc = crate::memory(ui, ctx.graph_id);
            let mut gmem = gmem_arc.lock().expect("failed to lock graph temp memory");
            selection_changed |= gmem.selection.nodes.remove(&self.id);
            selected = false;
            true
        } else {
            false
        };

        // Propagate this node's selection change to the graph-level dirty flag.
        if selection_changed {
            let gmem_arc = crate::memory(ui, ctx.graph_id);
            let mut gmem = gmem_arc.lock().expect("failed to lock graph temp memory");
            gmem.selection.changed = true;
        }

        if selection_changed || removed || edge_event.is_some() {
            response.mark_changed();
        }

        NodeResponse {
            response: egui::InnerResponse::new(content_output, response),
            sockets: socket_responses,
            selection_changed,
            selected,
            removed,
            edge_event,
        }
    }
}

impl NodeId {
    /// Create a new NodeId by hashing any hashable value.
    ///
    /// The id is the SipHash-1-3 hash of the value, with zero keys. This hash
    /// does not change between Rust releases, so ids made from the same value
    /// can be saved and used again by a later build, e.g. as the keys of a
    /// serialized [`View`](crate::View) layout.
    ///
    /// The id also depends on the [`Hash`] impl of the value. For example, a
    /// `usize` gives a different id on 32-bit and 64-bit targets.
    pub fn new(id_src: impl Hash) -> Self {
        use std::hash::Hasher;
        let mut hasher = siphasher::sip::SipHasher13::new();
        id_src.hash(&mut hasher);
        NodeId(hasher.finish())
    }

    /// Create a NodeId directly from a u64.
    pub const fn from_u64(v: u64) -> Self {
        NodeId(v)
    }

    /// Get the raw u64 value.
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for NodeId {
    fn from(v: u64) -> Self {
        NodeId(v)
    }
}

impl<R> NodeResponse<R> {
    /// Whether or not the selection changed and, if so, whether or not the node is now selected.
    pub fn selection(&self) -> Option<bool> {
        if self.selection_changed {
            Some(self.selected)
        } else {
            None
        }
    }

    /// Whether or not the node is selected.
    pub fn selected(&self) -> bool {
        self.selected
    }

    /// Whether or not the node was removed from the graph.
    ///
    /// This occurs if the `Delete` key is pressed while the node is selected.
    pub fn removed(&self) -> bool {
        self.removed
    }

    /// A reference to the inner value returned by the node's content.
    pub fn inner(&self) -> &R {
        &self.response.inner
    }

    /// A mutable reference to the inner value returned by the node's content.
    pub fn inner_mut(&mut self) -> &mut R {
        &mut self.response.inner
    }

    /// Consume the node response and produce the inner `egui::InnerResponse`.
    pub fn into_inner(self) -> egui::InnerResponse<R> {
        self.response
    }

    /// Whether or not any events occurred related to the creation of an edge.
    pub fn edge_event(&self) -> Option<EdgeEvent> {
        self.edge_event
    }

    /// The collected [`egui::Response`]s for each socket on this node.
    pub fn sockets(&self) -> &SocketResponses {
        &self.sockets
    }
}

impl<R> Deref for NodeResponse<R> {
    type Target = egui::Response;
    fn deref(&self) -> &Self::Target {
        &self.response.response
    }
}

impl<R> DerefMut for NodeResponse<R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.response.response
    }
}

impl<'a> NodeCtx<'a> {
    /// The current interaction state of the node.
    pub fn interaction(&self) -> NodeInteraction {
        self.interaction
    }

    /// The current egui style.
    pub fn style(&self) -> &egui::Style {
        self.ui.style()
    }

    /// The ID of the graph with which this node is associated.
    pub fn graph_id(&self) -> egui::Id {
        self.graph_id
    }

    /// The unique node ID for this node (i.e. it's index within the graph).
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// The unique egui ID for this node (the graph ID combined with the node ID).
    ///
    /// This is useful for creating persistent IDs for sub-components like Resize containers.
    pub fn egui_id(&self) -> egui::Id {
        egui_id(self.graph_id(), self.node_id())
    }

    /// Show content with the default frame styling.
    ///
    /// This consumes the context, ensuring content is only added once.
    ///
    /// Returns a [`FramedResponse`] containing the combined response and
    /// socket layout. The response:
    /// - Has a rect covering the entire framed area.
    /// - Reports interactions (clicks, drags, hovers) on any part of the node.
    /// - Is used by [`Node::show`] for selection and drag handling.
    ///
    /// The content closure receives a `&mut SocketLayout` which can be used to
    /// register explicit socket positions (e.g. via [`SocketLayout::row`](crate::SocketLayout::row)).
    /// If left unmodified, sockets are evenly spaced along the node edge.
    ///
    /// For custom frame styling, use [`NodeCtx::framed_with`].
    pub fn framed<T>(
        self,
        content: impl FnOnce(&mut egui::Ui, &mut crate::SocketLayout) -> T,
    ) -> FramedResponse<T> {
        let frame = default_frame(self.style(), self.interaction);
        self.framed_with(frame, content)
    }

    /// Show content within a custom frame.
    ///
    /// This consumes the context, ensuring content is only added once.
    ///
    /// Returns a [`FramedResponse`] containing the combined response and
    /// socket layout. The response:
    /// - Has a rect covering the entire framed area.
    /// - Reports interactions (clicks, drags, hovers) on any part of the node.
    /// - Is used by [`Node::show`] for selection and drag handling.
    ///
    /// The content closure receives a `&mut SocketLayout` which can be used to
    /// register explicit socket positions (e.g. via [`SocketLayout::row`](crate::SocketLayout::row)).
    /// If left unmodified, sockets are evenly spaced along the node edge.
    ///
    /// For default frame styling, use [`NodeCtx::framed`].
    pub fn framed_with<T>(
        self,
        frame: egui::Frame,
        content: impl FnOnce(&mut egui::Ui, &mut crate::SocketLayout) -> T,
    ) -> FramedResponse<T> {
        let min_size = self.min_size;
        let content_enabled = self.content_enabled;
        let mut socket_layout =
            crate::SocketLayout::evenly_spaced(self.flow, self.inputs, self.outputs);
        let builder = egui::UiBuilder::new().sense(egui::Sense::click_and_drag());
        let inner_response = frame.show(self.ui, |ui| {
            ui.scope_builder(builder, |ui| {
                ui.set_min_size(min_size);
                // Disable content widgets inside the frame, so that the frame
                // itself keeps its normal styling.
                if !content_enabled {
                    ui.disable();
                }
                content(ui, &mut socket_layout)
            })
        });
        let response = inner_response.response.union(inner_response.inner.response);
        let content_output = inner_response.inner.inner;
        FramedResponse {
            inner: egui::InnerResponse::new(content_output, response),
            sockets: socket_layout,
        }
    }
}

/// Register the node's two sublayers of the ui's layer: the socket layer,
/// which paints sockets above the ui's own content such as graph edges, and
/// the frame layer above it.
/// Both follow the ui layer's transform.
///
/// Only a node in view registers its sublayers, as the cost for egui to
/// register a sublayer grows with the number of layers. The sublayers of a
/// node out of view still follow the transform, so that its widgets do not
/// respond at an old position.
fn sublayers(ui: &egui::Ui, egui_id: egui::Id, in_view: bool) -> (egui::LayerId, egui::LayerId) {
    let parent = ui.layer_id();
    let socket_layer = egui::LayerId::new(parent.order, egui_id.with("sockets"));
    let frame_layer = egui::LayerId::new(parent.order, egui_id);
    for layer in [socket_layer, frame_layer] {
        if in_view {
            ui.ctx().set_sublayer(parent, layer);
        }
        if let Some(transform) = ui.ctx().layer_transform_to_global(parent) {
            ui.ctx().set_transform_layer(layer, transform);
        }
    }
    (socket_layer, frame_layer)
}

/// How far out of the visible area of a graph a node still counts as in
/// view, as its frame shadow and socket highlights paint outside its frame.
fn view_margin(style: &egui::Style, socket_radius: f32) -> f32 {
    let shadow = style.visuals.window_shadow.margin();
    let shadow = shadow
        .left
        .max(shadow.right)
        .max(shadow.top)
        .max(shadow.bottom);
    shadow.max(crate::socket::highlight_radius(socket_radius))
}

/// The default frame styling used for the `Node`'s `Window`.
///
/// This applies selection styling based on the `NodeInteraction` state:
/// - Selected nodes get the selection stroke.
/// - Nodes in the selection rectangle get a dimmed selection stroke.
pub fn default_frame(style: &egui::Style, interaction: NodeInteraction) -> egui::Frame {
    let mut frame = egui::Frame::window(style);
    frame.shadow.offset = [2, 2];
    frame.shadow.spread = (frame.shadow.spread as f32 * 0.25) as u8;

    // Style the frame based on interaction.
    frame.stroke.width = style.visuals.selection.stroke.width;
    if interaction.selected {
        frame.stroke = style.visuals.selection.stroke;
    } else if interaction.in_selection_rect {
        let color = style.visuals.weak_text_color();
        frame.shadow.color = color;
        frame.stroke = style.visuals.selection.stroke;
        frame.stroke.color = color;
    }

    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A static node paints outside any graph and yields a hover response
    /// per socket. Inputs sit on the frame's top edge in index order and
    /// outputs on its bottom edge.
    #[test]
    fn static_node_paints_sockets_on_the_frame() {
        let ctx = egui::Context::default();
        let mut out = None;
        // Two passes, since fonts and sizes settle after the first.
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                let res = Node::from_id(NodeId(0))
                    .inputs(2)
                    .outputs(1)
                    .flow(egui::Direction::TopDown)
                    .show_static(ui, |c| c.framed(|ui, _| ui.label("x")));
                let rect = res.inner.response.rect;
                let inputs: Vec<egui::Pos2> =
                    res.sockets.inputs().map(|(_, r)| r.rect.center()).collect();
                let outputs: Vec<egui::Pos2> = res
                    .sockets
                    .outputs()
                    .map(|(_, r)| r.rect.center())
                    .collect();
                out = Some((rect, inputs, outputs));
            })
            .textures_delta
            .clear();
        }
        let (rect, inputs, outputs) = out.expect("the panel ran");
        assert_eq!(inputs.len(), 2);
        assert_eq!(outputs.len(), 1);
        for pos in &inputs {
            assert!((pos.y - rect.top()).abs() < 1e-3, "{pos:?} off {rect:?}");
        }
        assert!((outputs[0].y - rect.bottom()).abs() < 1e-3);
        assert!(inputs[0].x < inputs[1].x);
    }

    /// Whether the content ui is enabled for a node in a graph, given the
    /// graph's immutability and the node's content flag.
    fn graph_content_enabled(immutable: bool, content_enabled: bool) -> bool {
        let ctx = egui::Context::default();
        let mut view = crate::View::default();
        let mut enabled = None;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::Graph::from_id(egui::Id::new("graph"))
                .immutable(immutable)
                .show(&mut view, ui, |ui, show| {
                    show.nodes(ui, |nctx, ui| {
                        Node::from_id(NodeId(0))
                            .content_enabled(content_enabled)
                            .show(nctx, ui, |c| {
                                c.framed(|ui, _| enabled = Some(ui.is_enabled()))
                            });
                    });
                });
        })
        .textures_delta
        .clear();
        enabled.expect("the node content ran")
    }

    #[test]
    fn content_enabled_disables_only_the_node_content() {
        let cases = [
            (false, true, true),
            (false, false, false),
            (true, true, false),
            (true, false, false),
        ];
        for (immutable, content, expected) in cases {
            let enabled = graph_content_enabled(immutable, content);
            assert_eq!(
                enabled, expected,
                "immutable {immutable}, content {content}"
            );
        }

        // A static node follows the flag too.
        let ctx = egui::Context::default();
        let mut enabled = None;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            Node::from_id(NodeId(0))
                .content_enabled(false)
                .show_static(ui, |c| c.framed(|ui, _| enabled = Some(ui.is_enabled())));
        })
        .textures_delta
        .clear();
        assert_eq!(enabled, Some(false));
    }

    /// A node with no animation time takes a new position on the pass it moves
    /// to it, not on the pass after.
    #[test]
    fn zero_animation_time_places_the_node_at_once() {
        let ctx = egui::Context::default();
        let mut view = crate::View::default();
        let mut min = None;
        for x in [0.0, 0.0, 100.0] {
            view.layout.insert(NodeId(0), egui::pos2(x, 0.0));
            ctx.run_ui(egui::RawInput::default(), |ui| {
                crate::Graph::from_id(egui::Id::new("graph")).show(&mut view, ui, |ui, show| {
                    show.nodes(ui, |nctx, ui| {
                        let res =
                            Node::from_id(NodeId(0))
                                .animation_time(0.0)
                                .show(nctx, ui, |c| c.framed(|ui, _| ui.label("x")));
                        min = Some(res.rect.min);
                    });
                });
            })
            .textures_delta
            .clear();
        }
        assert_eq!(min, Some(egui::pos2(100.0, 0.0)));
    }

    /// Ids made from a value stay the same, so saved layouts stay valid. These
    /// are the ids that `std::hash::DefaultHasher` gave in Rust 1.94.
    #[test]
    fn node_id_new_is_stable() {
        let cases = [
            (NodeId::new("node_1"), 0x8448cd7200baabf5),
            (NodeId::new(""), 0x30406ea523c53def),
            (
                NodeId::new("a longer id that spans several words"),
                0x8d53347dbcaf72d1,
            ),
            (NodeId::new(0u64), 0xbd60acb658c79e45),
            (NodeId::new(("graph", 7u32)), 0x6771a91e081e9a6b),
        ];
        for (id, expected) in cases {
            assert_eq!(id.value(), expected);
        }
    }

    mod cull {
        use super::*;
        use crate::View;
        use std::collections::HashSet;

        /// A graph of nodes `A` and `B`, driven one pass at a time.
        struct TestGraph {
            ctx: egui::Context,
            view: View,
            /// Whether `A` shows with [`Node::show_or_cull`], else with
            /// [`Node::show`].
            cull: bool,
            selected: HashSet<NodeId>,
        }

        /// The state of the graph observed at the end of a pass.
        struct Pass {
            /// Whether the content of `A` ran.
            a_ran: bool,
            a_rect: egui::Rect,
            /// The position and normal of the output of `A`, as edges see it.
            a_out: Option<(egui::Pos2, egui::Vec2)>,
            /// Whether `A` has a response for its output.
            a_out_response: bool,
            a_removed: bool,
            /// The id of the content ui of `B`, which shows after `A`.
            b_content_id: egui::Id,
        }

        const SCREEN: egui::Rect =
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(800.0, 600.0));
        /// Has one output.
        const A: NodeId = NodeId(0);
        /// Has one input, to the right of `A`.
        const B: NodeId = NodeId(1);

        /// A view in which `A` is out of view and `B` is in view.
        fn panned() -> egui::Rect {
            SCREEN.translate(egui::vec2(300.0, 0.0))
        }

        /// Lay out a new graph in view over two passes, since sizes settle
        /// after the first.
        fn test_graph(cull: bool) -> (TestGraph, Pass) {
            let mut g = TestGraph {
                ctx: egui::Context::default(),
                view: View {
                    scene_rect: SCREEN,
                    layout: [(A, egui::pos2(100.0, 100.0)), (B, egui::pos2(400.0, 100.0))].into(),
                },
                cull,
                selected: HashSet::new(),
            };
            pass(&mut g, vec![]);
            let p = pass(&mut g, vec![]);
            (g, p)
        }

        /// Run one pass of the graph with the given input events.
        fn pass(g: &mut TestGraph, events: Vec<egui::Event>) -> Pass {
            let input = egui::RawInput {
                screen_rect: Some(SCREEN),
                events,
                ..Default::default()
            };
            let cull = g.cull;
            let selected = g.selected.clone();
            let mut out = None;
            g.ctx
                .run_ui(input, |ui| {
                    let graph = crate::Graph::new("graph").selected_nodes(selected.clone());
                    graph.show(&mut g.view, ui, |ui, show| {
                        let mut nodes = None;
                        let mut a_out = None;
                        show.nodes(ui, |nctx, ui| {
                            let mut a_ran = false;
                            let a = Node::from_id(A).outputs(1).animation_time(0.0);
                            let content = |c: NodeCtx| {
                                a_ran = true;
                                c.framed(|ui, _| ui.label("a"))
                            };
                            let (a_rect, a_out_response, a_removed) = if cull {
                                let r = a.show_or_cull(nctx, ui, content);
                                (r.rect, r.sockets().output(0).is_some(), r.removed())
                            } else {
                                let r = a.show(nctx, ui, content);
                                (r.rect, r.sockets().output(0).is_some(), r.removed())
                            };
                            let mut b_content_id = None;
                            Node::from_id(B).inputs(1).show(nctx, ui, |c| {
                                c.framed(|ui, _| b_content_id = Some(ui.scope_id()))
                            });
                            nodes = Some(Pass {
                                a_ran,
                                a_rect,
                                a_out: None,
                                a_out_response,
                                a_removed,
                                b_content_id: b_content_id.expect("B ran"),
                            });
                        })
                        .edges(ui, |ectx, ui| a_out = Some(ectx.output(ui, A, 0)));
                        let a_out = a_out.expect("the edges ran");
                        out = nodes.map(|p| Pass { a_out, ..p });
                    });
                })
                .textures_delta
                .clear();
            out.expect("the graph ran")
        }

        /// Out of view, `A` skips its content and keeps its recorded size and
        /// its sockets.
        /// Back in view, its content runs again.
        #[test]
        fn out_of_view_node_skips_content() {
            let (mut g, in_view) = test_graph(true);
            assert!(in_view.a_ran);
            assert!(in_view.a_out_response);
            g.view.scene_rect = panned();
            let p = pass(&mut g, vec![]);
            assert!(!p.a_ran);
            let size =
                crate::with_graph_memory(&g.ctx, crate::id("graph"), |gmem| gmem.node_sizes()[&A]);
            assert_eq!(
                p.a_rect,
                egui::Rect::from_min_size(in_view.a_rect.min, size)
            );
            assert_eq!(p.a_out, in_view.a_out);
            assert!(!p.a_out_response);
            g.view.scene_rect = SCREEN;
            let p = pass(&mut g, vec![]);
            assert!(p.a_ran);
            assert!(p.a_out_response);
        }

        /// Out of view, `A` moves its sockets with it.
        #[test]
        fn out_of_view_node_moves_its_sockets() {
            let (mut g, in_view) = test_graph(true);
            g.view.scene_rect = panned();
            pass(&mut g, vec![]);
            let delta = egui::vec2(0.0, 50.0);
            *g.view.layout.get_mut(&A).expect("A is laid out") += delta;
            // The node position follows the layout one pass later.
            pass(&mut g, vec![]);
            let p = pass(&mut g, vec![]);
            assert!(!p.a_ran);
            let (pos, normal) = in_view.a_out.expect("A has an output");
            assert_eq!(p.a_out, Some((pos + delta, normal)));
        }

        /// Out of view, a selected `A` is still removed on delete.
        #[test]
        fn out_of_view_node_is_removed() {
            let (mut g, _) = test_graph(true);
            g.view.scene_rect = panned();
            g.selected = [A].into();
            pass(&mut g, vec![]);
            let delete = egui::Event::Key {
                key: egui::Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            let p = pass(&mut g, vec![delete]);
            assert!(!p.a_ran);
            assert!(p.a_removed);
        }

        /// Culling `A` does not change the egui ids of the nodes after it.
        #[test]
        fn culled_node_keeps_later_ids() {
            let (mut g, in_view) = test_graph(true);
            g.view.scene_rect = panned();
            let p = pass(&mut g, vec![]);
            assert!(!p.a_ran);
            assert_eq!(p.b_content_id, in_view.b_content_id);
        }

        /// With [`Node::show`], `A` out of view runs its content and keeps its
        /// sockets, but has no socket responses.
        #[test]
        fn out_of_view_node_without_cull_runs_content() {
            let (mut g, in_view) = test_graph(false);
            g.view.scene_rect = panned();
            let p = pass(&mut g, vec![]);
            assert!(p.a_ran);
            assert_eq!(p.a_out, in_view.a_out);
            assert!(!p.a_out_response);
        }
    }
}
