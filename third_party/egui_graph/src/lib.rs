// The README examples use the automatic layout.
#![cfg_attr(feature = "layout", doc = include_str!("../README.md"))]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;
use std::sync::{Arc, Mutex};

#[cfg(feature = "layout")]
pub use layout::{
    layout, layout_from_sizes, layout_routed, route_edges, EdgeRoutes, LayoutNode, LayoutParams,
};
pub use node::{FramedResponse, NodeCtx, NodeId, NodeInteraction};
pub use socket::layout::{grid::SocketGrid, SocketLayout};
pub use socket::{socket_padding, SocketKind, SocketResponses};

pub mod bezier;
pub mod edge;
#[cfg(feature = "layout")]
pub mod layout;
pub mod node;
pub mod paint;
pub mod socket;

/// The main interface for the `Graph` widget.
pub struct Graph {
    background: bool,
    dot_grid: bool,
    /// The base spacing of the dot grid in graph-space units, or `None` to
    /// derive it from the style's interaction size.
    dot_grid_step: Option<f32>,
    zoom_range: egui::Rangef,
    max_inner_size: Option<egui::Vec2>,
    center_view: bool,
    /// How the view responds when the available viewport size changes.
    resize_behavior: ResizeBehavior,
    /// How the view pans while a drag holds the pointer near the edge of the
    /// graph area, or `None` to disable it.
    auto_pan: Option<AutoPan>,
    id: egui::Id,
    /// If set, overwrite the graph's selected nodes at the start of the frame.
    selected_nodes: Option<HashSet<NodeId>>,
    /// When `true`, prevents structural changes while preserving navigation and
    /// selection.
    ///
    /// Unlike `Ui::set_enabled(false)` which disables all interaction
    /// (including panning, zooming, and selection), `immutable` only prevents
    /// structural changes - node positions, edges, and node content remain
    /// view-only while navigation and selection continue to work.
    immutable: bool,
    /// How node positions and frame sizes are snapped to whole graph-space
    /// units, or `None` to disable snapping.
    snap: Option<Snap>,
    /// The granularity (in graph-space units) used when snapping.
    snap_step: f32,
    /// Whether dragged nodes snap to align their edges, centers or sockets
    /// with other nodes.
    align: bool,
    /// Which node features are considered when aligning.
    align_targets: AlignTargets,
    /// The screen-pixel distance within which an alignment snaps, or `None` to
    /// derive it from the style's `interact_radius`.
    align_threshold: Option<f32>,
    /// A held modifier that temporarily disables alignment during a drag.
    align_disable_modifier: egui::Modifiers,
    /// Whether to draw a subtle guide line along an edge/center that is
    /// currently aligning during a drag.
    align_guides: bool,
    /// The stroke for alignment guide lines (width in screen pixels), or `None`
    /// to derive a subtle default from the style.
    align_guide_stroke: Option<egui::Stroke>,
}

/// How the view responds when the available viewport size changes
/// (e.g. a pane or window resize).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum ResizeBehavior {
    /// Preserve the zoom level (pixels-per-world-unit), revealing more or less
    /// of the scene as the viewport grows or shrinks. This is the default.
    MaintainZoom,
    /// Preserve the visible region of the scene, letting egui's [`Scene`] refit
    /// it into the new viewport size. This changes the apparent zoom and
    /// matches the behaviour prior to [`ResizeBehavior::MaintainZoom`].
    ///
    /// [`Scene`]: egui::containers::Scene
    MaintainView,
}

/// How the view pans while a drag holds the pointer near the edge of the
/// graph area. See [`Graph::auto_pan`].
///
/// On each axis, the pan speed grows linearly from zero at `margin` pixels
/// inside an edge to `max_speed` at the edge and beyond.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct AutoPan {
    /// The width of the band along each edge of the graph area within which
    /// the view pans, in screen pixels.
    pub margin: f32,
    /// The pan speed with the pointer at or past an edge, in screen pixels
    /// per second.
    pub max_speed: f32,
}

/// How node positions and frame sizes are snapped to whole graph-space units.
///
/// Snapping happens in graph space (egui's [`Scene`][egui::containers::Scene]
/// applies zoom/pan separately), keeping serialized layouts tidy and nodes
/// easy to align. "Off" is expressed as `Option<Snap>::None`; see
/// [`Graph::snap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum Snap {
    /// Round to the nearest multiple of the step. This is the default.
    Round,
    /// Round down (toward negative infinity) to a multiple of the step.
    Floor,
}

/// Which node features are considered when snap-aligning a dragged selection to
/// other nodes. See [`Graph::align`] and [`Graph::align_targets`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AlignTargets {
    /// Align the left/right/top/bottom edges.
    pub edges: bool,
    /// Align the horizontal and vertical centers.
    pub centers: bool,
    /// Align an output socket with an input socket downstream of it, across
    /// the flow, so the edge between them runs straight. Only nodes of the
    /// same flow direction align their sockets.
    pub sockets: bool,
}

impl Default for AlignTargets {
    fn default() -> Self {
        AlignTargets {
            edges: true,
            centers: false,
            sockets: true,
        }
    }
}

/// The line a selection of nodes is aligned into by [`align_nodes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Alignment {
    /// Nodes share an x coordinate, forming a vertical column.
    Column,
    /// Nodes share a y coordinate, forming a horizontal row.
    Row,
}

/// Which feature of each node [`align_nodes`] unifies onto the common line.
///
/// For a [`Alignment::Column`] this selects the left edges / centers / right
/// edges; for a [`Alignment::Row`], the top edges / centers / bottom edges.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum AlignBy {
    /// Align the top-left corners (the stored positions). Ignores node sizes.
    #[default]
    Min,
    /// Align the node centers.
    Center,
    /// Align the bottom-right corners.
    Max,
}

/// State related to the graph UI.
#[derive(Clone, Default)]
pub struct GraphTempMemory {
    /// The most recently recorded size of each node.
    ///
    /// Primarily used to check for node selection, as we don't know the size of the node until the
    /// contents have been instantiated.
    node_sizes: NodeSizes,
    /// The position of each node when its size was recorded.
    ///
    /// Used to move the recorded sockets of a node that moves while out of
    /// view, as it does not resolve its sockets again.
    node_positions: HashMap<NodeId, egui::Pos2>,
    /// The currently selected nodes and edges.
    selection: Selection,
    /// Whether or not the primary button was pressed on the graph area and is still down.
    ///
    /// Used for tracking selection and dragging.
    pressed: Option<Pressed>,
    /// Collect information about the layout of each node's sockets during node instantiation.
    ///
    /// This is used to provide the position and normal of each socket when instantiating edges.
    sockets: HashMap<NodeId, NodeSockets>,
    /// The closest socket that the pointer can press or connect to.
    ///
    /// Over a node frame, this is `Some` only while an edge is in progress, as
    /// a press there drags the node. Always `None` when the graph is immutable.
    closest_socket: Option<socket::Socket>,
    /// Whether the pointer was over any node frame during the previous frame.
    ///
    /// Node frames are sublayers that win egui's hit-test over the scene, so the
    /// scene response reports "not hovered" while the pointer is over a node.
    /// This lets an edge in progress still connect to a socket from within its
    /// node frame (e.g. on release over a socket that straddles a frame edge).
    ptr_over_node: bool,
    /// The most recently observed available viewport size.
    ///
    /// Used to preserve the zoom level across viewport resizes; see
    /// [`ResizeBehavior::MaintainZoom`].
    last_viewport_size: Option<egui::Vec2>,
}

type NodeSizes = HashMap<NodeId, egui::Vec2>;

#[derive(Clone, Default)]
struct Selection {
    /// The set of currently selected nodes.
    nodes: HashSet<NodeId>,
    /// Whether the selection was modified this frame.
    changed: bool,
}

/// State related to the last press of the primary pointer button over the graph.
#[derive(Clone, Debug)]
struct Pressed {
    /// Whether or not the pointer is currently over one of the selected nodes.
    ///
    /// This is used to assist with determining whether or not nodes should deselect. E.g. if
    /// multiple nodes are selected and a non-selected node is pressed, then we should deselect the
    /// originally selected nodes. However, if a selected node is pressed, then the selection
    /// should stay the same and a drag will begin.
    over_selection_at_origin: bool,
    /// The origin of the pointer over the graph at the begining of the press.
    origin_pos: egui::Pos2,
    /// The current position over the graph.
    current_pos: egui::Pos2,
    /// The action performed by this press.
    action: PressAction,
}

#[derive(Clone, Debug)]
enum PressAction {
    /// A node was pressed and a drag is taking place.
    DragNodes {
        /// The node that was pressed to initiate the drag.
        ///
        /// We don't know exactly which until the node itself emits the pressed event, so this
        /// remains `None` until we know.
        node: Option<PressedNode>,
    },
    /// The graph was pressed and we are performing a selection.
    Select,
    /// A node's socket was pressed in order to start creating a connection.
    Socket(socket::Socket),
}

#[derive(Clone, Debug)]
struct PressedNode {
    /// Unique Id of the node.
    id: NodeId,
    /// The position of the node over the graph at the origin of the press.
    position_at_origin: egui::Pos2,
}

/// Configuration for the graph.
// TODO: Consider storing this in graph widget "memory"?
// The thing is, it might be nice to let the user modify these externally.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct View {
    /// The visible area of the graph's [`Scene`][egui::containers::Scene].
    pub scene_rect: egui::Rect,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_sorted_layout"))]
    pub layout: Layout,
}

#[cfg(feature = "serde")]
fn serialize_sorted_layout<S: serde::Serializer>(layout: &Layout, s: S) -> Result<S::Ok, S::Error> {
    use serde::Serialize;
    let sorted: BTreeMap<_, _> = layout.iter().collect();
    sorted.serialize(s)
}

/// The location of the top-left of each node relative to the centre of the graph area.
pub type Layout = HashMap<NodeId, egui::Pos2>;

/// The context returned by the `Graph` widget. Allows for setting nodes and edges.
pub struct Show<'a> {
    /// Useful for accessing the `GraphTempMemory`.
    graph_id: egui::Id,
    /// The full area covered by the `Graph` within the UI.
    graph_rect: egui::Rect,
    /// If a selection is being performed with the pointer, this is the covered area.
    selection_rect: Option<egui::Rect>,
    /// Whether or not the primary mouse button was just released to perform the selection.
    select: bool,
    /// The closest socket that the pointer can press or connect to.
    closest_socket: Option<socket::Socket>,
    /// Whether or not the primary mouse button was just released to end edge creation.
    socket_press_released: Option<socket::Socket>,
    /// Track all nodes that were visited this update.
    ///
    /// We will use this to remove old node state on `drop`.
    visited: &'a mut HashSet<NodeId>,
    layout: &'a mut Layout,
    /// Whether the graph is in immutable (view-only) mode.
    immutable: bool,
    /// How frame sizes are snapped, or `None` to disable snapping.
    snap: Option<Snap>,
    /// The granularity (in graph-space units) used when snapping.
    snap_step: f32,
}

/// Information about the inputs and outputs for a particular node.
#[derive(Clone)]
pub struct NodeSockets {
    flow: egui::Direction,
    inputs: BTreeMap<usize, egui::Pos2>,
    outputs: BTreeMap<usize, egui::Pos2>,
}

/// A context to assist with the instantiation of node widgets.
pub struct NodesCtx<'a> {
    pub graph_id: egui::Id,
    graph_rect: egui::Rect,
    selection_rect: Option<egui::Rect>,
    select: bool,
    socket_press_released: Option<socket::Socket>,
    visited: &'a mut HashSet<NodeId>,
    layout: &'a mut Layout,
    /// Whether the graph is in immutable (view-only) mode.
    pub immutable: bool,
    /// How frame sizes are snapped, or `None` to disable snapping.
    snap: Option<Snap>,
    /// The granularity (in graph-space units) used when snapping.
    snap_step: f32,
}

/// A context to assist with the instantiation of edge widgets.
pub struct EdgesCtx {
    graph_id: egui::Id,
    graph_rect: egui::Rect,
    selection_rect: Option<egui::Rect>,
    closest_socket: Option<socket::Socket>,
    /// Whether an edge is in progress. Found once for all edges of the frame.
    edge_in_progress: bool,
    /// The graph's temporary memory. Fetched once for all edges of the frame.
    memory: Arc<Mutex<GraphTempMemory>>,
    /// Whether the graph is in immutable (view-only) mode.
    pub immutable: bool,
}

/// The set of detected graph interaction for a single graph widget update prior
/// to node interaction.
struct GraphInteraction {
    pressed: Option<Pressed>,
    socket_press_released: Option<socket::Socket>,
    select: bool,
    selection_rect: Option<egui::Rect>,
    drag_nodes_delta: egui::Vec2,
    /// The pressed node of a node drag that ended this update.
    drag_released: Option<PressedNode>,
}

/// The response returned by [`Graph::show`].
pub struct GraphResponse<R> {
    /// The user's return value from the content closure.
    pub inner: R,
    /// The egui [`Response`][egui::Response] for the graph's scene area.
    pub response: egui::Response,
    /// The set of selected nodes, present only when the selection changed this frame.
    pub selection_changed: Option<HashSet<NodeId>>,
    /// Whether a drag of the selected nodes ended this frame with the nodes in
    /// a new position.
    ///
    /// This is `true` only on the frame the drag ends. A press and release in
    /// place, or a drag back to the start, leaves it `false`. Use it to tell a
    /// move by the user from layout changes the graph makes itself, such as
    /// snapping or placing a node that has no position.
    pub nodes_moved: bool,
}

impl Graph {
    /// The default zoom range.
    ///
    /// Allows zooming out 4x, but does not allow zooming in past the
    /// pixel-perfect default level.
    pub const DEFAULT_ZOOM_RANGE: egui::Rangef = egui::Rangef {
        min: 0.25,
        max: 1.0,
    };
    pub const DEFAULT_CENTER_VIEW: bool = false;
    /// The default [`ResizeBehavior`].
    pub const DEFAULT_RESIZE_BEHAVIOR: ResizeBehavior = ResizeBehavior::MaintainZoom;
    /// The default [`AutoPan`]. Auto-pan is enabled by default.
    pub const DEFAULT_AUTO_PAN: Option<AutoPan> = Some(AutoPan::DEFAULT);
    /// The default snapping mode. Snaps node positions and frame sizes to the
    /// nearest whole graph-space unit.
    pub const DEFAULT_SNAP: Option<Snap> = Some(Snap::Round);
    /// The default snapping granularity, in graph-space units.
    pub const DEFAULT_SNAP_STEP: f32 = 1.0;
    /// Snap-align is enabled by default.
    pub const DEFAULT_ALIGN: bool = true;
    /// By default, alignment considers edges and sockets, but not centers
    /// (centers can feel busy).
    pub const DEFAULT_ALIGN_TARGETS: AlignTargets = AlignTargets {
        edges: true,
        centers: false,
        sockets: true,
    };
    /// The default alignment threshold. `None` derives it from the style's
    /// `interact_radius`.
    pub const DEFAULT_ALIGN_THRESHOLD: Option<f32> = None;
    /// The default modifier held to temporarily disable alignment.
    pub const DEFAULT_ALIGN_DISABLE_MODIFIER: egui::Modifiers = egui::Modifiers::ALT;
    /// Subtle alignment guide lines are drawn by default.
    pub const DEFAULT_ALIGN_GUIDES: bool = true;
    /// The default guide stroke. `None` derives a subtle stroke from the style.
    pub const DEFAULT_ALIGN_GUIDE_STROKE: Option<egui::Stroke> = None;

    /// Begin building the new graph widget.
    pub fn new(id_src: impl core::hash::Hash + core::fmt::Debug) -> Self {
        Self::from_id(id(id_src))
    }

    /// The same as [`Graph::new`], but allows providing an `egui::Id` directly.
    pub fn from_id(id: egui::Id) -> Self {
        Self {
            background: true,
            dot_grid: true,
            dot_grid_step: None,
            zoom_range: Self::DEFAULT_ZOOM_RANGE,
            max_inner_size: None,
            center_view: Self::DEFAULT_CENTER_VIEW,
            resize_behavior: Self::DEFAULT_RESIZE_BEHAVIOR,
            auto_pan: Self::DEFAULT_AUTO_PAN,
            id,
            selected_nodes: None,
            immutable: false,
            snap: Self::DEFAULT_SNAP,
            snap_step: Self::DEFAULT_SNAP_STEP,
            align: Self::DEFAULT_ALIGN,
            align_targets: Self::DEFAULT_ALIGN_TARGETS,
            align_threshold: Self::DEFAULT_ALIGN_THRESHOLD,
            align_disable_modifier: Self::DEFAULT_ALIGN_DISABLE_MODIFIER,
            align_guides: Self::DEFAULT_ALIGN_GUIDES,
            align_guide_stroke: Self::DEFAULT_ALIGN_GUIDE_STROKE,
        }
    }

    /// Whether or not to fill the background. Default is `true`.
    pub fn background(mut self, show: bool) -> Self {
        self.background = show;
        self
    }

    /// Whether or not to show the dot grid. Default is `true`.
    pub fn dot_grid(mut self, show: bool) -> Self {
        self.dot_grid = show;
        self
    }

    /// The base spacing of the dot grid, in graph-space units.
    ///
    /// The grid is anchored at the origin `(0, 0)` and coarsens in power-of-two
    /// multiples when zoomed far out (to bound the dots painted per frame).
    /// Both the dot grid and [`snap`][Self::snap] are anchored at the origin,
    /// so setting this equal to (or an integer multiple of) [`snap_step`] makes
    /// snapped nodes land on the grid. The two remain independent - neither is
    /// derived from the other.
    ///
    /// Default: `None`, deriving the spacing from the style's interaction size.
    ///
    /// [`snap_step`]: Self::snap_step
    pub fn dot_grid_step(mut self, step: f32) -> Self {
        self.dot_grid_step = Some(step);
        self
    }

    /// Set the allowed zoom range.
    ///
    /// A zoom < 1.0 zooms out, while a zoom > 1.0 zooms in.
    ///
    /// Default: [Graph::DEFAULT_ZOOM_RANGE].
    pub fn zoom_range(mut self, zoom_range: impl Into<egui::Rangef>) -> Self {
        self.zoom_range = zoom_range.into();
        self
    }

    /// Set the maximum size of the scene's inner [`Ui`](egui::Ui) that will be created.
    #[inline]
    pub fn max_inner_size(mut self, max_inner_size: impl Into<egui::Vec2>) -> Self {
        self.max_inner_size = Some(max_inner_size.into());
        self
    }

    /// Whether or not to center the view around the content of the graph.
    ///
    /// Default: [Self::DEFAULT_CENTER_VIEW].
    pub fn center_view(mut self, center_view: bool) -> Self {
        self.center_view = center_view;
        self
    }

    /// How the view responds when the available viewport size changes
    /// (e.g. a pane or window resize).
    ///
    /// Default: [`Self::DEFAULT_RESIZE_BEHAVIOR`].
    pub fn resize_behavior(mut self, behavior: ResizeBehavior) -> Self {
        self.resize_behavior = behavior;
        self
    }

    /// How the view pans while a drag holds the pointer near the edge of the
    /// graph area, or `None` to disable it.
    ///
    /// This lets a node drag, an edge in progress or a rectangle selection
    /// reach parts of the graph outside the view. In an
    /// [`immutable`](Self::immutable) graph, only a rectangle selection pans
    /// the view. The view pans only once a press becomes a drag, so a click
    /// near the edge does not move it. Has no effect while
    /// [`center_view`](Self::center_view) is set.
    ///
    /// Default: [`Self::DEFAULT_AUTO_PAN`] ([`AutoPan::DEFAULT`]).
    pub fn auto_pan(mut self, auto_pan: Option<AutoPan>) -> Self {
        self.auto_pan = auto_pan;
        self
    }

    /// Set the selected nodes for this frame.
    ///
    /// This overwrites the current selection in the graph's temporary memory
    /// at the start of the next `show` call.
    pub fn selected_nodes(mut self, nodes: HashSet<NodeId>) -> Self {
        self.selected_nodes = Some(nodes);
        self
    }

    /// Set immutable (view-only) mode.
    ///
    /// When `true`, prevents structural changes while preserving navigation
    /// and selection. Node dragging, edge creation/deletion, node deletion,
    /// and node content widgets are all disabled.
    ///
    /// Default: `false`.
    pub fn immutable(mut self, immutable: bool) -> Self {
        self.immutable = immutable;
        self
    }

    /// How node positions and frame sizes are snapped to whole graph-space
    /// units, or `None` to disable snapping entirely.
    ///
    /// Snapping keeps serialized layouts tidy, makes nodes easier to align,
    /// and avoids sub-pixel rendering at pixel-perfect zoom. Positions honour
    /// the chosen mode; recorded frame sizes always round to nearest (never
    /// floor) so they can't shrink below the rendered frame.
    ///
    /// Default: [`Self::DEFAULT_SNAP`] ([`Snap::Round`]).
    pub fn snap(mut self, snap: Option<Snap>) -> Self {
        self.snap = snap;
        self
    }

    /// The granularity (in graph-space units) used when snapping.
    ///
    /// Values `<= 0` (or non-finite) disable snapping for that value. Use a
    /// coarser step (e.g. `8.0`) to align nodes to a visible grid.
    ///
    /// Default: [`Self::DEFAULT_SNAP_STEP`] (`1.0`).
    pub fn snap_step(mut self, step: f32) -> Self {
        self.snap_step = step;
        self
    }

    /// Whether dragged nodes snap to align their edges, centers or sockets
    /// with other nodes.
    ///
    /// When a dragged node (or selection) comes within
    /// [`align_threshold`](Self::align_threshold) of another node, it snaps
    /// along that axis so the features in [`align_targets`](Self::align_targets)
    /// line up. Each axis is decided independently and takes priority over
    /// [`snap`](Self::snap): an axis that finds an alignment uses it, while an
    /// axis that doesn't falls back to the snap setting. Hold
    /// [`align_disable_modifier`](Self::align_disable_modifier) to suppress it.
    ///
    /// Default: [`Self::DEFAULT_ALIGN`] (`true`).
    pub fn align(mut self, align: bool) -> Self {
        self.align = align;
        self
    }

    /// Which node features ([`AlignTargets`]) are considered when aligning.
    ///
    /// Default: [`Self::DEFAULT_ALIGN_TARGETS`] (edges and sockets).
    pub fn align_targets(mut self, targets: AlignTargets) -> Self {
        self.align_targets = targets;
        self
    }

    /// The distance (in *screen* pixels) within which an edge, center or
    /// socket snaps to align.
    ///
    /// The value is converted to graph units using the live zoom each frame, so
    /// the snap "feel" stays constant in pixels across zoom levels. When unset
    /// (the default), the threshold is derived from the style's
    /// `interact_radius`.
    ///
    /// Default: [`Self::DEFAULT_ALIGN_THRESHOLD`] (`None`).
    pub fn align_threshold(mut self, threshold: f32) -> Self {
        self.align_threshold = Some(threshold);
        self
    }

    /// The keyboard modifier held to temporarily disable alignment during a
    /// drag (the axis then falls back to the [`snap`](Self::snap) setting).
    ///
    /// Default: [`Self::DEFAULT_ALIGN_DISABLE_MODIFIER`] ([`Modifiers::ALT`]).
    ///
    /// [`Modifiers::ALT`]: egui::Modifiers::ALT
    pub fn align_disable_modifier(mut self, modifier: egui::Modifiers) -> Self {
        self.align_disable_modifier = modifier;
        self
    }

    /// Whether to draw a subtle guide line along an edge/center that is
    /// currently aligning during a drag.
    ///
    /// Default: [`Self::DEFAULT_ALIGN_GUIDES`] (`true`).
    pub fn align_guides(mut self, guides: bool) -> Self {
        self.align_guides = guides;
        self
    }

    /// The stroke used for alignment guide lines.
    ///
    /// The width is interpreted in *screen* pixels (converted to graph units via
    /// the live zoom, so it stays a constant on-screen thickness). When unset
    /// (the default), a subtle stroke is derived from the style's selection
    /// colour.
    ///
    /// Default: [`Self::DEFAULT_ALIGN_GUIDE_STROKE`] (`None`).
    pub fn align_guide_stroke(mut self, stroke: egui::Stroke) -> Self {
        self.align_guide_stroke = Some(stroke);
        self
    }

    /// Begin showing the Graph.
    ///
    /// Returns a [`GraphResponse`] containing the user's return value,
    /// the scene [`egui::Response`], and the current set of selected nodes.
    pub fn show<R>(
        mut self,
        view: &mut View,
        ui: &mut egui::Ui,
        content: impl FnOnce(&mut egui::Ui, Show) -> R,
    ) -> GraphResponse<R> {
        // The full area to be occuppied by the graph.
        let graph_rect = ui.available_rect_before_wrap();

        let View {
            ref mut scene_rect,
            ref mut layout,
        } = *view;

        // Preserve the zoom level across viewport resizes (see
        // `ResizeBehavior::MaintainZoom`). egui's `Scene` treats `scene_rect`
        // as the visible region and refits it into the available size every
        // frame, so without this a resize changes the apparent zoom. We rescale
        // `scene_rect` so the next fit reproduces the previous scale.
        let viewport_size = graph_rect.size();
        {
            let gmem_arc = memory(ui, self.id);
            let mut gmem = gmem_arc.lock().expect("failed to lock graph temp memory");
            // Skip when `center_view` is set: `scene_rect` is overwritten after
            // the scene is shown, so any value here would be clobbered.
            if self.resize_behavior == ResizeBehavior::MaintainZoom && !self.center_view {
                if let Some(prev) = gmem.last_viewport_size {
                    *scene_rect =
                        maintain_zoom_scene_rect(*scene_rect, prev, viewport_size, self.zoom_range);
                }
            }
            // Record unconditionally so toggling `center_view`/behaviour works.
            gmem.last_viewport_size = Some(viewport_size);

            // Pan the view while a drag holds the pointer near the edge of the
            // graph area. As this happens before the scene shows, the
            // graph-space pointer of this frame includes the pan, so dragged
            // nodes, an edge in progress and a selection rect follow the
            // pointer. Skip when `center_view` is set, as above.
            if let Some(auto_pan) = self.auto_pan.filter(|_| !self.center_view) {
                let action = gmem.pressed.as_ref().map(|p| &p.action);
                let velocity = drag_pan_velocity(ui, auto_pan, graph_rect, action, self.immutable);
                if velocity != egui::Vec2::ZERO {
                    let delta = velocity * ui.input(|i| i.stable_dt);
                    *scene_rect =
                        pan_scene_rect(*scene_rect, viewport_size, self.zoom_range, delta);
                    ui.ctx().request_repaint();
                }
            }
        }

        // Create the Scene.
        let mut scene = egui::containers::Scene::new()
            .zoom_range(self.zoom_range)
            .drag_pan_buttons(egui::containers::DragPanButtons::MIDDLE);
        if let Some(max_inner_size) = self.max_inner_size {
            scene = scene.max_inner_size(max_inner_size);
        }

        // Track the bounding area of all widgets in the scene.
        let mut bounding_rect = None;

        let scene_response = scene.show(ui, scene_rect, |ui| {
            // Draw the selection rectangle if there is one.
            let mut selection_rect = None;
            let mut select = false;
            let mut closest_socket = None;
            let mut socket_press_released = None;
            let mut drag_released = None;
            // Alignment guide lines collected during a drag, as (is_x_axis, line).
            let mut guide_lines: Vec<(bool, AlignLine)> = Vec::new();

            // Check for interactions with the scene area.
            let scene_response = ui.response();
            let ptr_on_graph = scene_response.hovered();

            // Check for selection rectangle and node dragging.
            let gmem_arc = memory(ui, self.id);
            let mut gmem = gmem_arc.lock().expect("failed to lock graph temp memory");

            // Apply externally-provided selection if set.
            if let Some(nodes) = self.selected_nodes.take() {
                gmem.selection.nodes = nodes;
            }

            // Reset the selection dirty flag for this frame.
            gmem.selection.changed = false;

            // Take the previous frame's "pointer over a node frame" flag (it is
            // re-accumulated from each node's `contains_pointer()` as the nodes
            // draw) so an edge in progress can connect to sockets from within a
            // node frame - see the `closest_socket` gate below.
            let ptr_over_node_prev = std::mem::take(&mut gmem.ptr_over_node);

            // FIXME: Here we grab the global pointer and transform its position
            // to the graph scene space in order to check for initialising node
            // drag events. However, doing this means we run the risk of
            // incorrectly responding to events that should be captured by
            // widgets floating above (like a window floating above the graph).
            // We should change this to get the pointer only if it is hovered or
            // interacting with the scene or any of its child nodes somehow.
            let pointer = ui.input(|i| i.pointer.clone());
            if let Some(ptr_global) = pointer.interact_pos().or(pointer.hover_pos()) {
                let ptr_graph = ui
                    .ctx()
                    .layer_transform_from_global(ui.layer_id())
                    .unwrap_or_default()
                    .mul_pos(ptr_global);

                // Only detect a socket that the pointer can press or connect to.
                // Over the scene background, a press near a socket starts an
                // edge. Over a node frame, a press drags the node, so a socket
                // there is only detectable as the end of an edge in progress.
                // Use the raw graph-space pointer, as the scene's `hover_pos()`
                // is `None` over a node frame. An immutable graph detects no
                // sockets, so its presses map to `Select`.
                let edge_in_progress = matches!(
                    gmem.pressed.as_ref().map(|p| &p.action),
                    Some(PressAction::Socket(_))
                );
                let ptr_over_sockets = ptr_on_graph || (edge_in_progress && ptr_over_node_prev);
                closest_socket = if !self.immutable && ptr_over_sockets {
                    find_closest_socket(ptr_graph, layout, &gmem, ui)
                        .map(|(socket, _dist_sqrd)| socket)
                } else {
                    None
                };

                // Check for graph interactions.
                let interaction = graph_interaction(
                    layout,
                    &pointer,
                    closest_socket,
                    ptr_on_graph,
                    ptr_graph,
                    gmem.pressed.as_ref(),
                );

                // Move all selected nodes by a single delta (skip when
                // immutable), keeping the group rigid (preserving its relative
                // layout) by translating every node by the same offset rather
                // than snapping each independently. Per axis, snap-align wins if
                // a within-threshold edge/center is found; otherwise the axis
                // falls back to snapping the pressed node's target to the grid.
                // The per-node snap below skips the dragged nodes for the same
                // (rigidity) reason.
                if !self.immutable && interaction.drag_nodes_delta != egui::Vec2::ZERO {
                    if let Some(pressed) = gmem.pressed.as_ref() {
                        if let PressAction::DragNodes {
                            node: Some(pressed_node),
                        } = &pressed.action
                        {
                            let raw = interaction.drag_nodes_delta;

                            // The alignment threshold is configured in screen
                            // pixels; convert it to graph units via the live
                            // zoom so the snap zone stays a constant on-screen
                            // size. Guard a missing/degenerate transform.
                            let scale = ui
                                .ctx()
                                .layer_transform_to_global(ui.layer_id())
                                .map(|t| t.scaling)
                                .filter(|s| s.is_finite() && *s > 0.0)
                                .unwrap_or(1.0);
                            let modifiers = ui.input(|i| i.modifiers);
                            let align_on =
                                self.align && !modifiers.contains(self.align_disable_modifier);
                            let threshold_px = self
                                .align_threshold
                                .filter(|t| t.is_finite() && *t > 0.0)
                                .unwrap_or_else(|| ui.style().interaction.interact_radius);
                            let threshold = threshold_px / scale;

                            // Per-axis alignment of the dragged group's bounding
                            // box to the surrounding (non-selected) nodes.
                            let (line_x, line_y) = if align_on {
                                let selected_rects: Vec<egui::Rect> = gmem
                                    .selection
                                    .nodes
                                    .iter()
                                    .filter_map(|id| {
                                        let pos = *layout.get(id)?;
                                        let size = *gmem.node_sizes.get(id)?;
                                        Some(egui::Rect::from_min_size(pos, size))
                                    })
                                    .collect();
                                // Sort references by id for deterministic
                                // tie-breaks (HashMap order is unstable).
                                let mut refs: Vec<(NodeId, egui::Rect)> = layout
                                    .iter()
                                    .filter(|(id, _)| !gmem.selection.nodes.contains(id))
                                    .filter_map(|(id, &pos)| {
                                        let size = *gmem.node_sizes.get(id)?;
                                        Some((*id, egui::Rect::from_min_size(pos, size)))
                                    })
                                    .collect();
                                refs.sort_by_key(|(id, _)| *id);
                                let reference_rects: Vec<egui::Rect> =
                                    refs.iter().map(|(_, r)| *r).collect();
                                let (rect_x, rect_y) = align_adjust(
                                    self.align_targets,
                                    &selected_rects,
                                    &reference_rects,
                                    raw,
                                    threshold,
                                );
                                let (socket_x, socket_y) = match self.align_targets.sockets {
                                    true => {
                                        let sockets =
                                            |id: &NodeId| align_sockets(&gmem, layout, id);
                                        let mut selected_ids: Vec<NodeId> =
                                            gmem.selection.nodes.iter().copied().collect();
                                        selected_ids.sort();
                                        let selected: Vec<AlignSockets> =
                                            selected_ids.iter().filter_map(sockets).collect();
                                        let references: Vec<AlignSockets> =
                                            refs.iter().filter_map(|(id, _)| sockets(id)).collect();
                                        socket_align_adjust(&selected, &references, raw, threshold)
                                    }
                                    false => (None, None),
                                };
                                (nearer(socket_x, rect_x), nearer(socket_y, rect_y))
                            } else {
                                (None, None)
                            };

                            // Combine per axis: alignment, else snap fallback of
                            // the anchor's target (equivalent to the previous
                            // `snap_pos(..) - current` math, applied per axis).
                            let anchor = layout.get(&pressed_node.id).copied();
                            let snap = self.snap;
                            let snap_step = self.snap_step;
                            let axis =
                                |line: Option<AlignLine>, raw_a: f32, anchor_a: Option<f32>| {
                                    match line {
                                        Some(l) => raw_a + l.adjust,
                                        None => match (snap, anchor_a) {
                                            (Some(snap), Some(c)) => {
                                                snap_f32(snap, snap_step, c + raw_a) - c
                                            }
                                            _ => raw_a,
                                        },
                                    }
                                };
                            let delta = egui::vec2(
                                axis(line_x, raw.x, anchor.map(|p| p.x)),
                                axis(line_y, raw.y, anchor.map(|p| p.y)),
                            );

                            // Record the aligning edges so a subtle guide can be
                            // drawn along them once the nodes are laid out.
                            if self.align_guides {
                                guide_lines.extend(line_x.map(|l| (true, l)));
                                guide_lines.extend(line_y.map(|l| (false, l)));
                            }

                            for &n_id in &gmem.selection.nodes {
                                if let Some(pos) = layout.get_mut(&n_id) {
                                    *pos += delta;
                                }
                            }
                        }
                    }
                }

                gmem.pressed = interaction.pressed;
                gmem.closest_socket = closest_socket;
                selection_rect = interaction.selection_rect;
                select = interaction.select;
                socket_press_released = interaction.socket_press_released;
                drag_released = interaction.drag_released;
            }

            // Snap node positions to whole graph-space units. Runs every frame
            // (regardless of pointer presence) and after any drag delta has
            // been applied, so it normalises dragged, auto-laid-out, and
            // deserialized positions alike before the nodes render.
            //
            // Nodes in an active drag are skipped: they're translated rigidly
            // above (anchored on the pressed node) to preserve the selection's
            // relative layout, so snapping them per-node here would distort it.
            if let Some(snap) = self.snap {
                let dragging = matches!(
                    gmem.pressed.as_ref().map(|p| &p.action),
                    Some(PressAction::DragNodes { .. })
                );
                for (id, pos) in layout.iter_mut() {
                    if dragging && gmem.selection.nodes.contains(id) {
                        continue;
                    }
                    *pos = snap_pos(snap, self.snap_step, *pos);
                }
            }

            // The whole selection moves with the pressed node, so the pressed
            // node alone tells whether the drag moved the nodes. Compare after
            // snapping, so a drag back to the start counts as no move.
            let nodes_moved = drag_released
                .is_some_and(|n: PressedNode| layout.get(&n.id) != Some(&n.position_at_origin));

            // Paint the background rect.
            let visible_rect = ui.clip_rect();
            if self.background {
                paint_background(visible_rect, ui);
            }

            // Paint some subtle dots to check camera movement.
            if self.dot_grid {
                paint_dot_grid(visible_rect, self.dot_grid_step, ui);
            }

            // Paint subtle guide lines along edges/centers aligning this frame.
            if !guide_lines.is_empty() {
                paint_align_guides(&guide_lines, self.align_guide_stroke, ui);
            }

            // Draw the selection area if there is one.
            if let Some(sel_rect) = selection_rect {
                paint_selection_area(sel_rect, self.id, ui);
            }

            let mut visited = HashSet::default();

            let show = Show {
                graph_id: self.id,
                graph_rect,
                selection_rect,
                select,
                closest_socket,
                socket_press_released,
                visited: &mut visited,
                layout,
                immutable: self.immutable,
                snap: self.snap,
                snap_step: self.snap_step,
            };

            // Drop the lock before running the content.
            std::mem::drop(gmem);

            let output = content(ui, show);

            prune_unused_nodes(self.id, &visited, ui);
            bounding_rect = Some(ui.min_rect());

            // Snapshot selection only if it changed this frame.
            let gmem_arc = memory(ui, self.id);
            let gmem = gmem_arc.lock().expect("failed to lock graph temp memory");
            let selection_changed = if gmem.selection.changed {
                Some(gmem.selection.nodes.clone())
            } else {
                None
            };

            (output, selection_changed, nodes_moved)
        });

        if self.center_view {
            if let Some(rect) = bounding_rect {
                view.scene_rect = rect.expand(rect.width() * 0.1);
            }
        }

        let (inner, selection_changed, nodes_moved) = scene_response.inner;
        GraphResponse {
            inner,
            response: scene_response.response,
            selection_changed,
            nodes_moved,
        }
    }
}

impl AutoPan {
    /// A 32 pixel margin with a maximum speed of 1000 pixels per second.
    pub const DEFAULT: Self = Self {
        margin: 32.0,
        max_speed: 1000.0,
    };
}

impl GraphTempMemory {
    /// Get the recorded sizes of all nodes.
    pub fn node_sizes(&self) -> &NodeSizes {
        &self.node_sizes
    }

    /// The most recently resolved socket positions for each node.
    ///
    /// Useful for deriving exact node-relative socket offsets (e.g. for
    /// `LayoutNode::input_offsets`) when nodes position their sockets
    /// explicitly.
    pub fn node_sockets(&self) -> &HashMap<NodeId, NodeSockets> {
        &self.sockets
    }
}

impl NodeSockets {
    /// Move all sockets by `delta`.
    fn translate(&mut self, delta: egui::Vec2) {
        for pos in self.inputs.values_mut().chain(self.outputs.values_mut()) {
            *pos += delta;
        }
    }

    /// The screen position and normal of the input at the given index.
    ///
    /// Returns `None` if there is no input at the given index.
    pub fn input(&self, ix: usize) -> Option<(egui::Pos2, egui::Vec2)> {
        self.inputs
            .get(&ix)
            .map(|&pos| (pos, input_normal(self.flow)))
    }

    /// The screen position and normal of the output at the given index.
    ///
    /// Returns `None` if there is no output at the given index.
    pub fn output(&self, ix: usize) -> Option<(egui::Pos2, egui::Vec2)> {
        self.outputs
            .get(&ix)
            .map(|&pos| (pos, output_normal(self.flow)))
    }

    /// Produces an iterator yielding the index, position, and normal for each input.
    pub fn inputs(&self) -> impl Iterator<Item = (usize, egui::Pos2, egui::Vec2)> + '_ {
        let norm = input_normal(self.flow);
        self.inputs.iter().map(move |(&ix, &pos)| (ix, pos, norm))
    }

    /// Produces an iterator yielding the index, position, and normal for each output.
    pub fn outputs(&self) -> impl Iterator<Item = (usize, egui::Pos2, egui::Vec2)> + '_ {
        let norm = output_normal(self.flow);
        self.outputs.iter().map(move |(&ix, &pos)| (ix, pos, norm))
    }
}

fn input_normal(flow: egui::Direction) -> egui::Vec2 {
    match flow {
        egui::Direction::LeftToRight => egui::Vec2::new(-1.0, 0.0),
        egui::Direction::RightToLeft => egui::Vec2::new(1.0, 0.0),
        egui::Direction::TopDown => egui::Vec2::new(0.0, -1.0),
        egui::Direction::BottomUp => egui::Vec2::new(0.0, 1.0),
    }
}

fn output_normal(flow: egui::Direction) -> egui::Vec2 {
    match flow {
        egui::Direction::LeftToRight => egui::Vec2::new(1.0, 0.0),
        egui::Direction::RightToLeft => egui::Vec2::new(-1.0, 0.0),
        egui::Direction::TopDown => egui::Vec2::new(0.0, 1.0),
        egui::Direction::BottomUp => egui::Vec2::new(0.0, -1.0),
    }
}

impl<'a> Show<'a> {
    /// Instantiate the nodes of the graph.
    pub fn nodes(
        mut self,
        ui: &mut egui::Ui,
        content: impl FnOnce(&mut NodesCtx, &mut egui::Ui),
    ) -> Self {
        {
            let Self {
                graph_id,
                graph_rect,
                selection_rect,
                select,
                socket_press_released,
                ref mut visited,
                ref mut layout,
                immutable,
                snap,
                snap_step,
                ..
            } = self;
            let mut ctx = NodesCtx {
                graph_id,
                graph_rect,
                selection_rect,
                select,
                socket_press_released,
                visited,
                layout,
                immutable,
                snap,
                snap_step,
            };
            content(&mut ctx, ui);
        }
        self
    }

    /// Instantiate the edges of the graph.
    pub fn edges(
        self,
        ui: &mut egui::Ui,
        content: impl FnOnce(&mut EdgesCtx, &mut egui::Ui),
    ) -> Self {
        {
            let Self {
                graph_rect,
                graph_id,
                selection_rect,
                closest_socket,
                immutable,
                ..
            } = self;
            let memory = memory(ui, graph_id);
            let edge_in_progress = {
                let gmem = memory.lock().expect("failed to lock graph temp memory");
                edge_in_progress(&gmem).is_some()
            };
            let mut ctx = EdgesCtx {
                graph_id,
                graph_rect,
                selection_rect,
                closest_socket,
                edge_in_progress,
                memory,
                immutable,
            };
            content(&mut ctx, ui);
        }
        self
    }
}

/// If a node didn't appear this update, it's likely because the user has
/// removed the node from their graph, so we should stop tracking it.
fn prune_unused_nodes(graph_id: egui::Id, visited: &HashSet<NodeId>, ui: &mut egui::Ui) {
    let gmem_arc = memory(ui, graph_id);
    let mut gmem = gmem_arc.lock().expect("failed to lock graph temp memory");
    gmem.node_sizes.retain(|k, _| visited.contains(k));
    gmem.sockets.retain(|k, _| visited.contains(k));
    gmem.node_positions.retain(|k, _| visited.contains(k));
    gmem.selection.nodes.retain(|k| visited.contains(k));
    if let Some(socket) = gmem.closest_socket.as_ref() {
        if !visited.contains(&socket.node) {
            gmem.closest_socket = None;
        }
    }
    if let Some(pressed) = gmem.pressed.as_ref() {
        match pressed.action {
            PressAction::DragNodes {
                node: Some(PressedNode { id: n, .. }),
            }
            | PressAction::Socket(socket::Socket { node: n, .. })
                if !visited.contains(&n) =>
            {
                gmem.pressed = None
            }
            _ => (),
        }
    }
}

impl EdgesCtx {
    /// Retrieves the position and normal of the specified input for the given node.
    ///
    /// Returns `None` if either the `node` or `input` do not exist.
    pub fn input(
        &self,
        ui: &egui::Ui,
        node: NodeId,
        input: usize,
    ) -> Option<(egui::Pos2, egui::Vec2)> {
        let gmem_arc = crate::memory(ui, self.graph_id);
        let gmem = gmem_arc.lock().expect("failed to lock graph temp memory");
        gmem.sockets
            .get(&node)
            .and_then(|sockets| sockets.input(input))
    }

    /// Retrieves the position and normal of the specified output for the given node.
    ///
    /// Returns `None` if either the `node` or `output` do not exist.
    pub fn output(
        &self,
        ui: &egui::Ui,
        node: NodeId,
        output: usize,
    ) -> Option<(egui::Pos2, egui::Vec2)> {
        let gmem_arc = memory(ui, self.graph_id);
        let gmem = gmem_arc.lock().expect("failed to lock graph temp memory");
        gmem.sockets
            .get(&node)
            .and_then(|sockets| sockets.output(output))
    }

    /// If the user is in the progress of creating an edge, this returns the relevant info.
    pub fn in_progress(&self, ui: &egui::Ui) -> Option<EdgeInProgress> {
        let gmem_arc = memory(ui, self.graph_id);
        let gmem = gmem_arc.lock().expect("failed to lock graph temp memory");
        edge_in_progress(&gmem)
    }

    /// The position and normal of output `a` and input `b`, for an edge from
    /// `a` to `b`.
    ///
    /// Returns `None` if either socket does not exist.
    fn edge_sockets(
        &self,
        (a, output): (NodeId, usize),
        (b, input): (NodeId, usize),
    ) -> Option<((egui::Pos2, egui::Vec2), (egui::Pos2, egui::Vec2))> {
        let gmem = self
            .memory
            .lock()
            .expect("failed to lock graph temp memory");
        let a_out = gmem.sockets.get(&a)?.output(output)?;
        let b_in = gmem.sockets.get(&b)?.input(input)?;
        Some((a_out, b_in))
    }

    /// The full rect occuppied by the graph widget.
    pub fn graph_rect(&self) -> egui::Rect {
        self.graph_rect
    }
}

pub struct EdgeInProgress {
    /// The socket at the start end of the edge.
    pub start: socket::PositionedSocket,
    /// The end position of the edge in progress.
    ///
    /// If there is no socket within the interaction radius, this will be the pointer position.
    /// Otherwise, this will be the position of the closest socket who's `SocketKind` is opposite
    /// to `start.kind`.
    pub end_pos: egui::Pos2,
    /// The closest socket who's `SocketKind` is opposite to `start.kind`.
    ///
    /// This is `None` in the case that there are no sockets within the interaction radius.
    pub end_socket: Option<(socket::SocketKind, egui::Vec2)>,
}

impl EdgeInProgress {
    /// The path of this in-progress edge, in the given `shape`, so it can
    /// match the graph's other edges.
    pub fn path(&self, shape: edge::EdgeShape) -> bezier::Path {
        let start = (self.start.pos, self.start.normal);
        let end_normal = self
            .end_socket
            .as_ref()
            .map(|&(_, n)| n)
            .unwrap_or(-self.start.normal);
        let end = (self.end_pos, end_normal);
        shape.path(start, &[], end)
    }

    /// Short-hand for painting the in-progress edge with some reasonable defaults.
    ///
    /// If you require custom styling of the in-progress edge, use
    /// [`EdgeInProgress::show_styled`], [`EdgeInProgress::path`] or the
    /// individual fields to paint it however you wish.
    pub fn show(&self, ui: &egui::Ui, shape: edge::EdgeShape) {
        self.show_styled(ui, None, shape);
    }

    /// As [`EdgeInProgress::show`], but with an optional `stroke` override, so the
    /// in-progress edge can match styled edges.
    ///
    /// When `stroke` is `None`, `ui.visuals().widgets.active.fg_stroke` is used.
    pub fn show_styled(&self, ui: &egui::Ui, stroke: Option<egui::Stroke>, shape: edge::EdgeShape) {
        let dist_per_pt = crate::edge::Edge::DEFAULT_DISTANCE_PER_POINT;
        let pts = self.path(shape).flatten(dist_per_pt).collect();
        let stroke = stroke.unwrap_or(ui.visuals().widgets.active.fg_stroke);
        ui.painter().add(egui::Shape::line(pts, stroke));
    }
}

impl Default for View {
    fn default() -> Self {
        Self {
            scene_rect: egui::Rect::ZERO,
            layout: Default::default(),
        }
    }
}

impl Default for AutoPan {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The edge that the user is in the progress of creating, if any.
fn edge_in_progress(gmem: &GraphTempMemory) -> Option<EdgeInProgress> {
    let pressed = gmem.pressed.as_ref()?;
    let start = match pressed.action {
        PressAction::Socket(socket) => {
            let sockets = gmem.sockets.get(&socket.node)?;
            let (pos, normal) = match socket.kind {
                socket::SocketKind::Input => sockets.input(socket.index)?,
                socket::SocketKind::Output => sockets.output(socket.index)?,
            };
            socket::PositionedSocket {
                socket,
                pos,
                normal,
            }
        }
        _ => return None,
    };
    let (end_pos, end_socket) = match gmem.closest_socket {
        Some(socket) if socket.kind != start.socket.kind => {
            let sockets = gmem.sockets.get(&socket.node)?;
            let (pos, normal) = match socket.kind {
                socket::SocketKind::Input => sockets.input(socket.index)?,
                socket::SocketKind::Output => sockets.output(socket.index)?,
            };
            (pos, Some((socket.kind, normal)))
        }
        _ => (pressed.current_pos, None),
    };
    Some(EdgeInProgress {
        start,
        end_pos,
        end_socket,
    })
}

/// Find the socket that is closest to the given point.
///
/// Returns the socket alongside the squared distance from the socket.
fn find_closest_socket(
    pos_graph: egui::Pos2,
    layout: &Layout,
    gmem: &GraphTempMemory,
    ui: &egui::Ui,
) -> Option<(socket::Socket, f32)> {
    // TODO: if we wanted to be super efficient, we could maintain a quadtree of
    // nodes and sockets...
    let mut closest_socket = None;
    let socket_radius = ui
        .spacing()
        .interact_size
        .x
        .min(ui.spacing().interact_size.y);
    let visible_rect = ui.clip_rect();
    let socket_radius_sq = socket_radius * socket_radius;
    for (&n_id, &n_graph) in layout {
        // Only check visible nodes.
        let n_screen = n_graph;
        let size = match gmem.node_sizes.get(&n_id) {
            None => continue,
            Some(&size) => size,
        };
        let rect = egui::Rect::from_min_size(n_screen, size);
        if !visible_rect.intersects(rect) {
            continue;
        }
        let sockets = match gmem.sockets.get(&n_id) {
            None => continue,
            Some(sockets) => sockets,
        };

        // Check inputs.
        for (ix, p, _) in sockets.inputs() {
            let dist_sq = pos_graph.distance_sq(p);
            if dist_sq < socket_radius_sq {
                let socket = socket::Socket {
                    node: n_id,
                    kind: socket::SocketKind::Input,
                    index: ix,
                };
                closest_socket = match closest_socket {
                    None => Some((socket, dist_sq)),
                    Some((_, d_sq)) if dist_sq < d_sq => Some((socket, dist_sq)),
                    _ => closest_socket,
                }
            }
        }

        // Check outputs.
        for (ix, p, _) in sockets.outputs() {
            let dist_sq = pos_graph.distance_sq(p);
            if dist_sq < socket_radius_sq {
                let socket = socket::Socket {
                    node: n_id,
                    kind: socket::SocketKind::Output,
                    index: ix,
                };
                closest_socket = match closest_socket {
                    None => Some((socket, dist_sq)),
                    Some((_, d_sq)) if dist_sq < d_sq => Some((socket, dist_sq)),
                    _ => closest_socket,
                }
            }
        }
    }

    closest_socket
}

/// Interpret some basic interactions from the state of the graph and recent input.
fn graph_interaction(
    layout: &Layout,
    pointer: &egui::PointerState,
    closest_socket: Option<socket::Socket>,
    ptr_on_graph: bool,
    ptr_graph: egui::Pos2,
    pressed: Option<&Pressed>,
) -> GraphInteraction {
    let mut select = false;
    let mut socket_press_released = None;
    let mut drag_nodes_delta = egui::Vec2::ZERO;
    let mut selection_rect = None;
    let mut drag_released = None;

    // Check for selecting/dragging.
    let pressed: Option<Pressed> = if let Some(pressed) = pressed {
        match pressed.action {
            PressAction::DragNodes {
                node: Some(ref node),
            } => {
                // Determine the drag delta.
                let delta = ptr_graph - pressed.origin_pos;
                let target = node.position_at_origin + delta;
                if let Some(current) = layout.get(&node.id) {
                    drag_nodes_delta = target - *current;
                }
            }
            PressAction::Select => {
                let min = pressed.origin_pos;
                let max = ptr_graph;
                selection_rect = Some(egui::Rect::from_two_pos(min, max));
            }
            _ => (),
        }

        // The press action has ended.
        if pointer.primary_released() {
            match pressed.action {
                PressAction::Select => select = true,
                PressAction::Socket(socket) => socket_press_released = Some(socket),
                PressAction::DragNodes { ref node } => drag_released = node.clone(),
            }
            None
        } else {
            Some(Pressed {
                current_pos: ptr_graph,
                ..pressed.clone()
            })
        }
    // Check for the beginning of a socket press or rectangular selection.
    } else if ptr_on_graph
        && pointer.button_down(egui::PointerButton::Primary)
        && pointer.button_pressed(egui::PointerButton::Primary)
    {
        // Choose which press action based on whether or not a socket was pressed.
        let action = match closest_socket {
            Some(socket) => PressAction::Socket(socket),
            None => {
                let min = ptr_graph;
                let max = ptr_graph;
                selection_rect = Some(egui::Rect::from_two_pos(min, max));
                PressAction::Select
            }
        };

        let pressed = Pressed {
            over_selection_at_origin: false,
            origin_pos: ptr_graph,
            current_pos: ptr_graph,
            action,
        };
        Some(pressed)

    // Otherwise, pass through existing state.
    } else {
        pressed.cloned()
    };

    GraphInteraction {
        pressed,
        socket_press_released,
        select,
        selection_rect,
        drag_nodes_delta,
        drag_released,
    }
}

// Paint a subtle dot grid to check camera movement.
fn paint_dot_grid(visible_rect: egui::Rect, base_step: Option<f32>, ui: &mut egui::Ui) {
    // Fall back to the style's interaction size, ignoring a non-positive or
    // non-finite override (which would produce a degenerate, unbounded grid).
    let base = base_step
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or_else(|| ui.spacing().interact_size.y);
    let dot_step = dot_grid_step(base, visible_rect);
    let color = ui.style().noninteractive().bg_stroke.color;
    let x_dots = (visible_rect.min.x / dot_step) as i32..=(visible_rect.max.x / dot_step) as i32;
    let y_dots = (visible_rect.min.y / dot_step) as i32..=(visible_rect.max.y / dot_step) as i32;
    for x_dot in x_dots {
        for y_dot in y_dots.clone() {
            let x = x_dot as f32 * dot_step;
            let y = y_dot as f32 * dot_step;
            ui.painter().circle_filled([x, y].into(), 0.5, color);
        }
    }
}

/// The dot grid step, doubled as needed from `base_step` to bound the number
/// of dots covering `visible_rect`.
///
/// Without a bound the per-frame cost grows with the visible scene area
/// (e.g. when zoomed out on a large window). Power-of-two multiples keep
/// coarser grids aligned with finer ones as the zoom changes.
fn dot_grid_step(base_step: f32, visible_rect: egui::Rect) -> f32 {
    /// The maximum number of dots painted per frame.
    const MAX_DOTS: f32 = 16_384.0;
    let dots = (visible_rect.width() / base_step) * (visible_rect.height() / base_step);
    if dots > MAX_DOTS {
        base_step * 2f32.powi((dots / MAX_DOTS).sqrt().log2().ceil() as i32)
    } else {
        base_step
    }
}

// Paint the background rect.
fn paint_background(visible_rect: egui::Rect, ui: &mut egui::Ui) {
    let vis = ui.style().noninteractive();
    let stroke = egui::Stroke {
        width: 0.0,
        ..vis.bg_stroke
    };
    let fill = vis.bg_fill;
    ui.painter()
        .rect(visible_rect, 0.0, fill, stroke, egui::StrokeKind::Inside);
}

/// Paint the selection area rectangle above the nodes.
///
/// The rectangle paints on a sublayer of the ui's layer, as do the node
/// frames and sockets. `move_to_top` sorts it after those sibling sublayers,
/// and egui keeps all sublayers directly above their parent, so areas such
/// as windows above the graph stay above the rectangle.
fn paint_selection_area(sel_rect: egui::Rect, graph_id: egui::Id, ui: &egui::Ui) {
    let parent = ui.layer_id();
    let layer = egui::LayerId::new(parent.order, graph_id.with("selection"));
    let ctx = ui.ctx();
    ctx.set_sublayer(parent, layer);
    ctx.move_to_top(layer);
    if let Some(transform) = ctx.layer_transform_to_global(parent) {
        ctx.set_transform_layer(layer, transform);
    }
    let color = ui.visuals().weak_text_color();
    let fill = color.linear_multiply(0.125);
    let width = 1.0;
    let stroke = egui::Stroke { width, color };
    ui.painter().clone().with_layer_id(layer).rect(
        sel_rect,
        0.0,
        fill,
        stroke,
        egui::StrokeKind::Inside,
    );
}

/// Combines the given id src with the `TypeId` of the `Graph` to produce a unique `egui::Id`.
pub fn id(id_src: impl core::hash::Hash + core::fmt::Debug) -> egui::Id {
    egui::Id::unique((std::any::TypeId::of::<Graph>(), id_src))
}

/// Access the graph's temporary memory for the given graph ID.
///
/// This allows reading graph state like node sizes without cloning.
/// If no memory exists for the graph ID, a default GraphTempMemory is created and stored.
pub fn with_graph_memory<R>(
    ctx: &egui::Context,
    graph_id: egui::Id,
    f: impl FnOnce(&GraphTempMemory) -> R,
) -> R {
    let gmem_arc = ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Arc<Mutex<GraphTempMemory>>>(graph_id)
            .clone()
    });
    let gmem = gmem_arc.lock().expect("failed to lock graph temp memory");
    f(&gmem)
}

/// Checks if a node with the given ID is currently selected in the specified graph.
pub fn is_node_selected(ui: &egui::Ui, graph_id: egui::Id, node_id: NodeId) -> bool {
    let gmem_arc = memory(ui, graph_id);
    let gmem = gmem_arc.lock().expect("failed to lock graph temp memory");
    gmem.selection.nodes.contains(&node_id)
}

/// Rescale `scene_rect` so that egui's [`Scene`] reproduces the previous
/// effective scale (pixels-per-world-unit) at the new viewport size, holding
/// the scene center fixed.
///
/// Returns `scene_rect` unchanged when the size did not change or the inputs
/// are degenerate. See [`ResizeBehavior::MaintainZoom`].
///
/// [`Scene`]: egui::containers::Scene
fn maintain_zoom_scene_rect(
    scene_rect: egui::Rect,
    prev: egui::Vec2,
    cur: egui::Vec2,
    zoom_range: egui::Rangef,
) -> egui::Rect {
    if prev == cur {
        return scene_rect;
    }
    let Some(scale) = scene_scale(scene_rect, prev, zoom_range) else {
        return scene_rect;
    };
    // Using the same scalar on both axes makes `scene_rect` adopt the viewport
    // aspect ratio - the steady-state shape egui itself produces - while
    // keeping the center fixed. Next frame egui's fit yields `scale` again.
    egui::Rect::from_center_size(scene_rect.center(), cur / scale)
}

/// The scale (pixels-per-world-unit) with which egui's [`Scene`] fits
/// `scene_rect` into a viewport of `viewport_size`.
///
/// Returns `None` when the inputs are degenerate.
///
/// [`Scene`]: egui::containers::Scene
fn scene_scale(
    scene_rect: egui::Rect,
    viewport_size: egui::Vec2,
    zoom_range: egui::Rangef,
) -> Option<f32> {
    let size = scene_rect.size();
    if !scene_rect.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
        return None;
    }
    // The binding (letterbox) axis, i.e. the smaller ratio, sets the scale,
    // clamped to the same range egui enforces.
    let scale = zoom_range.clamp((viewport_size / size).min_elem());
    (scale.is_finite() && scale > 0.0).then_some(scale)
}

/// Translate `scene_rect` so that egui's [`Scene`] pans the view by `delta`
/// screen pixels within a viewport of `viewport_size`.
///
/// Returns `scene_rect` unchanged when the inputs are degenerate.
///
/// [`Scene`]: egui::containers::Scene
fn pan_scene_rect(
    scene_rect: egui::Rect,
    viewport_size: egui::Vec2,
    zoom_range: egui::Rangef,
    delta: egui::Vec2,
) -> egui::Rect {
    match scene_scale(scene_rect, viewport_size, zoom_range) {
        Some(scale) if delta.is_finite() => scene_rect.translate(delta / scale),
        _ => scene_rect,
    }
}

/// The auto-pan velocity for the current input, in screen pixels per second.
///
/// This is zero unless the primary button holds a drag whose press `action`
/// pans the view. `viewport` is the graph area in the coordinates of `ui`.
fn drag_pan_velocity(
    ui: &egui::Ui,
    auto_pan: AutoPan,
    viewport: egui::Rect,
    action: Option<&PressAction>,
    immutable: bool,
) -> egui::Vec2 {
    if !action.is_some_and(|action| drag_pans(action, immutable)) {
        return egui::Vec2::ZERO;
    }
    // A press that is still a possible click does not pan, so that a click
    // on a node near the edge does not move the view.
    let ptr_global = ui.input(|i| {
        let pointer = &i.pointer;
        let dragging = pointer.primary_down() && pointer.is_decidedly_dragging();
        pointer.interact_pos().filter(|_| dragging)
    });
    let Some(ptr_global) = ptr_global else {
        return egui::Vec2::ZERO;
    };
    let ptr = ui
        .ctx()
        .layer_transform_from_global(ui.layer_id())
        .map_or(ptr_global, |t| t.mul_pos(ptr_global));
    auto_pan_velocity(auto_pan, viewport, ptr)
}

/// Whether a drag with the given press action pans the view near the edge of
/// the graph area.
///
/// An immutable graph has no node drags or edges, so only a rectangle
/// selection pans it.
fn drag_pans(action: &PressAction, immutable: bool) -> bool {
    match action {
        PressAction::Select => true,
        PressAction::DragNodes { .. } | PressAction::Socket(_) => !immutable,
    }
}

/// The auto-pan velocity, in screen pixels per second, for a pointer at `ptr`
/// over the graph area `viewport`. See [`AutoPan`].
///
/// Returns zero for a margin or speed that is not positive and finite.
fn auto_pan_velocity(auto_pan: AutoPan, viewport: egui::Rect, ptr: egui::Pos2) -> egui::Vec2 {
    let AutoPan { margin, max_speed } = auto_pan;
    let valid = |v: f32| v.is_finite() && v > 0.0;
    if !valid(margin) || !valid(max_speed) || !ptr.is_finite() {
        return egui::Vec2::ZERO;
    }
    // The depth of `p` into the band along each edge, as a fraction of the
    // margin, signed toward the nearer edge.
    let axis = |min: f32, max: f32, p: f32| {
        let into_max = (p - (max - margin)) / margin;
        let into_min = (min + margin - p) / margin;
        into_max.clamp(0.0, 1.0) - into_min.clamp(0.0, 1.0)
    };
    let x = axis(viewport.min.x, viewport.max.x, ptr.x);
    let y = axis(viewport.min.y, viewport.max.y, ptr.y);
    egui::vec2(x, y) * max_speed
}

/// Snap a scalar to a multiple of `step` according to `snap`.
///
/// A non-positive or non-finite `step`, or a non-finite input, is returned
/// unchanged so a bad step can't poison the layout with `NaN`.
pub fn snap_f32(snap: Snap, step: f32, v: f32) -> f32 {
    if !v.is_finite() || !step.is_finite() || step <= 0.0 {
        return v;
    }
    match snap {
        Snap::Round => (v / step).round() * step,
        Snap::Floor => (v / step).floor() * step,
    }
}

/// Snap each axis of a position to a multiple of `step`. See [`snap_f32`].
pub fn snap_pos(snap: Snap, step: f32, p: egui::Pos2) -> egui::Pos2 {
    egui::pos2(snap_f32(snap, step, p.x), snap_f32(snap, step, p.y))
}

/// Snap each axis of a vector to a multiple of `step`. See [`snap_f32`].
pub fn snap_vec(snap: Snap, step: f32, v: egui::Vec2) -> egui::Vec2 {
    egui::vec2(snap_f32(snap, step, v.x), snap_f32(snap, step, v.y))
}

/// Align a selection of nodes onto a common line, tidying up the layout.
///
/// The chosen feature (`by`) of each node is unified onto the *mean* of that
/// feature: e.g. with [`AlignBy::Center`] the nodes' centers all move onto the
/// average center. When `alignment` is `None`, the orientation is inferred from
/// the spread of the nodes' centers - nodes spread more vertically become a
/// [`Alignment::Column`] (shared x), otherwise a [`Alignment::Row`] (shared y).
///
/// `sizes` (e.g. from [`GraphTempMemory::node_sizes`]) is consulted only for
/// [`AlignBy::Center`] and [`AlignBy::Max`]; a node missing from `sizes` is
/// treated as zero-sized. Nodes absent from `layout`, or whose position is
/// non-finite, are ignored. Returns the [`Alignment`] applied, or `None` if
/// fewer than two nodes resolve (in which case `layout` is left untouched).
///
/// Positions are written exactly as computed. Callers that snap node positions
/// (e.g. via [`Graph::snap`]) need not snap the result themselves: rendering the
/// graph re-snaps every node to the grid, so a fractional mean lands on the grid
/// on the same frame.
pub fn align_nodes(
    layout: &mut Layout,
    nodes: impl IntoIterator<Item = NodeId>,
    sizes: &HashMap<NodeId, egui::Vec2>,
    by: AlignBy,
    alignment: Option<Alignment>,
) -> Option<Alignment> {
    // Resolve the nodes that are actually present with a finite position.
    let resolved: Vec<(NodeId, egui::Pos2, egui::Vec2)> = nodes
        .into_iter()
        .filter_map(|id| {
            let pos = *layout.get(&id)?;
            if !pos.is_finite() {
                return None;
            }
            let size = sizes.get(&id).copied().unwrap_or(egui::Vec2::ZERO);
            Some((id, pos, size))
        })
        .collect();
    if resolved.len() < 2 {
        return None;
    }

    // The fraction of a node's size added to its top-left to reach the feature.
    let frac = match by {
        AlignBy::Min => 0.0,
        AlignBy::Center => 0.5,
        AlignBy::Max => 1.0,
    };
    let feature = |pos: egui::Pos2, size: egui::Vec2| pos + size * frac;

    // Infer the orientation from the centers' spread when not given.
    let alignment =
        alignment.unwrap_or_else(|| infer_alignment(resolved.iter().map(|&(_, p, s)| p + s * 0.5)));
    let coord = |p: egui::Pos2| match alignment {
        Alignment::Column => p.x,
        Alignment::Row => p.y,
    };

    // The shared line is the mean of the feature on the unified axis.
    let sum: f32 = resolved.iter().map(|&(_, p, s)| coord(feature(p, s))).sum();
    let target = sum / resolved.len() as f32;

    // Solve each node's top-left so its feature lands on `target`.
    for (id, _, size) in resolved {
        if let Some(pos) = layout.get_mut(&id) {
            match alignment {
                Alignment::Column => pos.x = target - size.x * frac,
                Alignment::Row => pos.y = target - size.y * frac,
            }
        }
    }
    Some(alignment)
}

/// Infer the orientation to align a set of points into from their spread: a
/// wider vertical spread suggests a [`Alignment::Column`] (unify x), otherwise a
/// [`Alignment::Row`] (unify y). Non-finite points are ignored; ties and the
/// degenerate all-coincident case resolve to [`Alignment::Column`].
fn infer_alignment(points: impl IntoIterator<Item = egui::Pos2>) -> Alignment {
    let mut min = egui::pos2(f32::INFINITY, f32::INFINITY);
    let mut max = egui::pos2(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for p in points {
        if !p.is_finite() {
            continue;
        }
        min = min.min(p);
        max = max.max(p);
    }
    let spread = max - min;
    // `>=` (and `false` for the NaN no-finite-points case) defaults to a column.
    if spread.y >= spread.x {
        Alignment::Column
    } else {
        Alignment::Row
    }
}

/// A matched alignment on one axis: how far to nudge the drag delta, and where
/// (and over what perpendicular span) to draw the guide line.
#[derive(Clone, Copy, Debug, PartialEq)]
struct AlignLine {
    /// The amount to add to the raw drag delta on this axis so the features
    /// coincide.
    adjust: f32,
    /// The aligned coordinate on this axis (where the guide line sits).
    pos: f32,
    /// The perpendicular extent the guide line spans (covering both the dragged
    /// group and the matched reference node).
    span: egui::Rangef,
}

/// The sockets of a node at its current layout position, for socket alignment.
#[derive(Clone, Debug, PartialEq)]
struct AlignSockets {
    flow: egui::Direction,
    inputs: Vec<egui::Pos2>,
    outputs: Vec<egui::Pos2>,
}

/// Per-axis snap-alignment of a dragged group to the surrounding nodes.
///
/// The dragged group's bounding box (the union of `selected_rects`) is moved by
/// `raw_delta`, then each axis is matched against the `reference_rects`: edges
/// are compared with edges and centers with centers (per `targets`), and the
/// closest candidate within `threshold` (in graph units) wins. Returns `None`
/// on an axis with no candidate within range.
fn align_adjust(
    targets: AlignTargets,
    selected_rects: &[egui::Rect],
    reference_rects: &[egui::Rect],
    raw_delta: egui::Vec2,
    threshold: f32,
) -> (Option<AlignLine>, Option<AlignLine>) {
    if selected_rects.is_empty()
        || reference_rects.is_empty()
        || !threshold.is_finite()
        || threshold <= 0.0
        || !(targets.edges || targets.centers)
    {
        return (None, None);
    }

    // The dragged group's bounding box at the target (post-delta) position.
    let mut bbox = selected_rects[0];
    for r in &selected_rects[1..] {
        bbox = bbox.union(*r);
    }
    let target = bbox.translate(raw_delta);

    // Keep the smallest within-threshold match (`reference - dragged`) for an
    // axis, tracking the reference rect so the guide can span both nodes.
    type Best = Option<(f32, f32, egui::Rect)>; // (adjust, pos, reference rect)
    let consider = |best: &mut Best, dragged: f32, reference: f32, r: egui::Rect| {
        let adjust = reference - dragged;
        if adjust.abs() <= threshold && best.map_or(true, |(b, _, _)| adjust.abs() < b.abs()) {
            *best = Some((adjust, reference, r));
        }
    };

    let mut best_x: Best = None;
    let mut best_y: Best = None;
    for &r in reference_rects {
        if targets.edges {
            // Any dragged x-edge may align to any reference x-edge (covers
            // both same-edge alignment and edge-to-edge "touching").
            consider(&mut best_x, target.min.x, r.min.x, r);
            consider(&mut best_x, target.min.x, r.max.x, r);
            consider(&mut best_x, target.max.x, r.min.x, r);
            consider(&mut best_x, target.max.x, r.max.x, r);
            consider(&mut best_y, target.min.y, r.min.y, r);
            consider(&mut best_y, target.min.y, r.max.y, r);
            consider(&mut best_y, target.max.y, r.min.y, r);
            consider(&mut best_y, target.max.y, r.max.y, r);
        }
        if targets.centers {
            consider(&mut best_x, target.center().x, r.center().x, r);
            consider(&mut best_y, target.center().y, r.center().y, r);
        }
    }

    // A vertical guide (x-axis match) spans the y extent of both nodes; a
    // horizontal guide spans their x extent.
    let line_x = best_x.map(|(adjust, pos, r)| AlignLine {
        adjust,
        pos,
        span: egui::Rangef::new(target.min.y.min(r.min.y), target.max.y.max(r.max.y)),
    });
    let line_y = best_y.map(|(adjust, pos, r)| AlignLine {
        adjust,
        pos,
        span: egui::Rangef::new(target.min.x.min(r.min.x), target.max.x.max(r.max.x)),
    });
    (line_x, line_y)
}

/// The sockets of node `id` at its position in `layout`.
///
/// The recorded sockets are from the frame in which the node last drew, so
/// they move by the distance the node has moved since.
fn align_sockets(gmem: &GraphTempMemory, layout: &Layout, id: &NodeId) -> Option<AlignSockets> {
    let sockets = gmem.sockets.get(id)?;
    let shift = *layout.get(id)? - *gmem.node_positions.get(id)?;
    Some(AlignSockets {
        flow: sockets.flow,
        inputs: sockets.inputs.values().map(|&p| p + shift).collect(),
        outputs: sockets.outputs.values().map(|&p| p + shift).collect(),
    })
}

/// Per-axis snap-alignment of the sockets of a dragged group to the sockets
/// of the surrounding nodes.
///
/// The `selected` sockets move by `raw_delta`. Each dragged output is then
/// matched with each reference input downstream of it, and each dragged input
/// with each reference output upstream of it. Only nodes of the same flow
/// match, and only on the axis across their flow. The closest match within
/// `threshold` (in graph units) wins. The guide line runs between the two
/// sockets.
fn socket_align_adjust(
    selected: &[AlignSockets],
    references: &[AlignSockets],
    raw_delta: egui::Vec2,
    threshold: f32,
) -> (Option<AlignLine>, Option<AlignLine>) {
    if !threshold.is_finite() || threshold <= 0.0 {
        return (None, None);
    }
    let mut best_x: Option<AlignLine> = None;
    let mut best_y: Option<AlignLine> = None;
    for d in selected {
        let downstream = output_normal(d.flow);
        // A flow along y aligns sockets on x, and the reverse.
        let across_x = downstream.x == 0.0;
        let (best, across, along): (_, fn(egui::Pos2) -> f32, fn(egui::Pos2) -> f32) =
            match across_x {
                true => (&mut best_x, |p| p.x, |p| p.y),
                false => (&mut best_y, |p| p.y, |p| p.x),
            };
        for r in references.iter().filter(|r| r.flow == d.flow) {
            let dragged_outputs = d.outputs.iter().map(|&o| o + raw_delta);
            let pairs_down = dragged_outputs.flat_map(|o| r.inputs.iter().map(move |&i| (o, i)));
            let dragged_inputs = d.inputs.iter().map(|&i| i + raw_delta);
            let pairs_up = dragged_inputs.flat_map(|i| r.outputs.iter().map(move |&o| (i, o)));
            // In each pair, the second socket is the reference.
            let downstream_pairs = pairs_down.filter(|&(o, i)| (i - o).dot(downstream) > 0.0);
            let upstream_pairs = pairs_up.filter(|&(i, o)| (i - o).dot(downstream) > 0.0);
            for (dragged, reference) in downstream_pairs.chain(upstream_pairs) {
                let adjust = across(reference) - across(dragged);
                if adjust.abs() <= threshold && best.map_or(true, |b| adjust.abs() < b.adjust.abs())
                {
                    *best = Some(AlignLine {
                        adjust,
                        pos: across(reference),
                        span: egui::Rangef::new(along(dragged), along(reference)).as_positive(),
                    });
                }
            }
        }
    }
    (best_x, best_y)
}

/// The alignment of `a` and `b` that moves the dragged nodes the least.
/// A tie keeps `a`.
fn nearer(a: Option<AlignLine>, b: Option<AlignLine>) -> Option<AlignLine> {
    match (a, b) {
        (Some(a), Some(b)) if b.adjust.abs() < a.adjust.abs() => Some(b),
        (a, b) => a.or(b),
    }
}

/// Paint the subtle alignment guide lines collected during a drag. Each entry
/// is `(is_x_axis, line)`: an x-axis match draws a vertical line, a y-axis match
/// a horizontal one. Drawn in graph space; the stroke width is treated as screen
/// pixels and scaled so the line stays a constant on-screen thickness.
fn paint_align_guides(
    lines: &[(bool, AlignLine)],
    guide_stroke: Option<egui::Stroke>,
    ui: &egui::Ui,
) {
    let scale = ui
        .ctx()
        .layer_transform_to_global(ui.layer_id())
        .map(|t| t.scaling)
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(1.0);
    // Default to a faint 1px line in the selection colour at reduced opacity.
    let base = guide_stroke.unwrap_or_else(|| {
        let color = ui.visuals().selection.stroke.color.gamma_multiply(0.5);
        egui::Stroke::new(1.0_f32, color)
    });
    let stroke = egui::Stroke::new(base.width / scale, base.color);
    let painter = ui.painter();
    for &(is_x_axis, line) in lines {
        if is_x_axis {
            // Vertical line at x = pos, spanning the y extent.
            painter.vline(line.pos, line.span, stroke);
        } else {
            // Horizontal line at y = pos, spanning the x extent.
            painter.hline(line.span, line.pos, stroke);
        }
    }
}

/// Short-hand for retrieving access to the graph's temporary memory from the `Ui`.
fn memory(ui: &egui::Ui, graph_id: egui::Id) -> Arc<Mutex<GraphTempMemory>> {
    ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_default::<Arc<Mutex<GraphTempMemory>>>(graph_id)
            .clone()
    })
}

#[cfg(test)]
mod tests {
    use super::{
        align_adjust, align_nodes, dot_grid_step, infer_alignment, maintain_zoom_scene_rect,
        nearer, snap_f32, socket_align_adjust, AlignBy, AlignLine, AlignSockets, AlignTargets,
        Alignment, Layout, NodeId, Snap,
    };
    use egui::{Rangef, Rect, Vec2};
    use std::collections::HashMap;

    /// The scale egui's `Scene` would apply when fitting `scene_rect` into a
    /// viewport of `size` (the binding/letterbox axis).
    fn fit_scale(size: Vec2, scene_rect: Rect) -> f32 {
        (size / scene_rect.size()).min_elem()
    }

    #[test]
    fn preserves_scale_and_center_on_resize() {
        let unbounded = Rangef::new(0.0, f32::INFINITY);
        let scene = Rect::from_center_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 80.0));
        let prev = egui::vec2(300.0, 240.0); // scale = 3.0 on both axes
        let cur = egui::vec2(600.0, 240.0);

        let scale_before = fit_scale(prev, scene);
        let out = maintain_zoom_scene_rect(scene, prev, cur, unbounded);

        assert!((fit_scale(cur, out) - scale_before).abs() < 1e-4);
        assert!((out.center() - scene.center()).length() < 1e-4);
    }

    #[test]
    fn noop_when_size_unchanged() {
        let r = Rangef::new(0.0, f32::INFINITY);
        let scene = Rect::from_center_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
        let size = egui::vec2(200.0, 200.0);
        assert_eq!(maintain_zoom_scene_rect(scene, size, size, r), scene);
    }

    #[test]
    fn noop_on_degenerate_scene_rect() {
        let r = Rangef::new(0.0, f32::INFINITY);
        let prev = egui::vec2(200.0, 200.0);
        let cur = egui::vec2(400.0, 200.0);
        // Zero area on one axis.
        let zero = Rect::from_center_size(egui::Pos2::ZERO, egui::vec2(0.0, 100.0));
        assert_eq!(maintain_zoom_scene_rect(zero, prev, cur, r), zero);
        // Non-finite: returned unchanged. NaN can't be compared with `==`, so
        // assert the degenerate input was passed straight through.
        let nan = Rect::from_center_size(egui::Pos2::ZERO, egui::vec2(f32::NAN, 100.0));
        let out = maintain_zoom_scene_rect(nan, prev, cur, r);
        assert!(out.min.x.is_nan() && out.max.x.is_nan());
        assert_eq!(out.min.y, nan.min.y);
        assert_eq!(out.max.y, nan.max.y);
    }

    #[test]
    fn clamps_to_zoom_range() {
        let range = Rangef::new(0.25, 1.0);
        let scene = Rect::from_center_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
        let prev = egui::vec2(10.0, 10.0); // raw scale 0.1, below min -> clamps to 0.25
        let cur = egui::vec2(200.0, 200.0);
        let out = maintain_zoom_scene_rect(scene, prev, cur, range);
        // Reproduced scale should match the clamped value.
        assert!((fit_scale(cur, out) - 0.25).abs() < 1e-4);
    }

    #[test]
    fn dot_grid_step_bounds_dot_count() {
        let base = 18.0;
        // Typical visible areas keep the base step.
        let small = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1080.0));
        assert_eq!(dot_grid_step(base, small), base);
        // Larger areas coarsen the grid to bound the count, in power-of-two
        // multiples so the grids stay aligned across zoom levels.
        for scale in [4.0_f32, 16.0, 256.0, 1e6] {
            let size = egui::vec2(3000.0 * scale, 1600.0 * scale);
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let step = dot_grid_step(base, rect);
            let dots = (rect.width() / step) * (rect.height() / step);
            assert!(dots <= 16_384.0, "{dots} dots at scale {scale}");
            let multiple = step / base;
            assert_eq!(
                multiple.log2().fract(),
                0.0,
                "{multiple} not a power of two"
            );
        }
        // Degenerate rects stay harmless.
        let inf = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(f32::INFINITY, 100.0));
        assert!(dot_grid_step(base, inf) > 0.0);
    }

    #[test]
    fn snap_round_and_floor() {
        assert_eq!(snap_f32(Snap::Round, 1.0, 2.4), 2.0);
        assert_eq!(snap_f32(Snap::Round, 1.0, 2.6), 3.0);
        assert_eq!(snap_f32(Snap::Floor, 1.0, 2.9), 2.0);
        // Negatives floor toward negative infinity.
        assert_eq!(snap_f32(Snap::Floor, 1.0, -0.1), -1.0);
        assert_eq!(snap_f32(Snap::Round, 1.0, -0.4), 0.0);
        // Coarser steps.
        assert_eq!(snap_f32(Snap::Round, 8.0, 11.0), 8.0);
        assert_eq!(snap_f32(Snap::Round, 8.0, 13.0), 16.0);
        assert_eq!(snap_f32(Snap::Floor, 8.0, 15.0), 8.0);
    }

    #[test]
    fn snap_is_idempotent() {
        for &mode in &[Snap::Round, Snap::Floor] {
            for &step in &[1.0_f32, 8.0] {
                for &v in &[2.4_f32, 2.6, -0.1, 13.0, 100.49] {
                    let once = snap_f32(mode, step, v);
                    assert_eq!(snap_f32(mode, step, once), once);
                }
            }
        }
    }

    #[test]
    fn snap_guards_bad_inputs() {
        // Non-positive or non-finite step returns the input unchanged.
        assert_eq!(snap_f32(Snap::Round, 0.0, 3.7), 3.7);
        assert_eq!(snap_f32(Snap::Round, -1.0, 3.7), 3.7);
        assert_eq!(snap_f32(Snap::Round, f32::NAN, 3.7), 3.7);
        // Non-finite input passes straight through.
        assert!(snap_f32(Snap::Round, 1.0, f32::INFINITY).is_infinite());
        assert!(snap_f32(Snap::Round, 1.0, f32::NAN).is_nan());
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h))
    }

    const EDGES: AlignTargets = AlignTargets {
        edges: true,
        centers: false,
        sockets: false,
    };
    const CENTERS: AlignTargets = AlignTargets {
        edges: false,
        centers: true,
        sockets: false,
    };

    /// `align_adjust` reduced to just the per-axis adjustments, for brevity.
    fn align(
        targets: AlignTargets,
        sel: &[Rect],
        refs: &[Rect],
        raw: Vec2,
        threshold: f32,
    ) -> (Option<f32>, Option<f32>) {
        let (x, y) = align_adjust(targets, sel, refs, raw, threshold);
        (x.map(|l| l.adjust), y.map(|l| l.adjust))
    }

    #[test]
    fn align_no_candidates() {
        let sel = [rect(0.0, 0.0, 10.0, 10.0)];
        // No reference nodes.
        assert_eq!(align(EDGES, &sel, &[], Vec2::ZERO, 5.0), (None, None));
        // Empty selection.
        let refs = [rect(0.0, 0.0, 10.0, 10.0)];
        assert_eq!(align(EDGES, &[], &refs, Vec2::ZERO, 5.0), (None, None));
    }

    #[test]
    fn align_edge_within_and_beyond_threshold() {
        let sel = [rect(0.0, 0.0, 10.0, 10.0)];
        // Left edge 2 away on x, far on y -> aligns x only.
        let near = [rect(2.0, 100.0, 10.0, 10.0)];
        assert_eq!(
            align(EDGES, &sel, &near, Vec2::ZERO, 5.0),
            (Some(2.0), None)
        );
        // All edges beyond threshold -> no adjustment.
        let far = [rect(20.0, 200.0, 10.0, 10.0)];
        assert_eq!(align(EDGES, &sel, &far, Vec2::ZERO, 5.0), (None, None));
    }

    #[test]
    fn align_touching_edge() {
        // Dragged right edge (10) snaps to reference left edge (12).
        let sel = [rect(0.0, 0.0, 10.0, 10.0)];
        let refs = [rect(12.0, 100.0, 10.0, 10.0)];
        assert_eq!(
            align(EDGES, &sel, &refs, Vec2::ZERO, 5.0),
            (Some(2.0), None)
        );
    }

    #[test]
    fn align_closest_candidate_wins() {
        let sel = [rect(0.0, 0.0, 10.0, 10.0)];
        // Nearer left edge (2) wins over the farther one (4).
        let refs = [rect(4.0, 100.0, 10.0, 10.0), rect(2.0, 200.0, 10.0, 10.0)];
        assert_eq!(align(EDGES, &sel, &refs, Vec2::ZERO, 5.0).0, Some(2.0));
    }

    #[test]
    fn align_axes_independent() {
        let sel = [rect(0.0, 0.0, 10.0, 10.0)];
        let x_only = [rect(3.0, 200.0, 10.0, 10.0)];
        assert_eq!(
            align(EDGES, &sel, &x_only, Vec2::ZERO, 5.0),
            (Some(3.0), None)
        );
        let y_only = [rect(200.0, 3.0, 10.0, 10.0)];
        assert_eq!(
            align(EDGES, &sel, &y_only, Vec2::ZERO, 5.0),
            (None, Some(3.0))
        );
    }

    #[test]
    fn align_centers_only() {
        // Wide dragged vs narrow reference: centers coincide (0), edges out of range.
        let sel = [rect(0.0, 0.0, 20.0, 10.0)]; // center x = 10
        let refs = [rect(6.0, 100.0, 8.0, 10.0)]; // center x = 10, edges 6/14
        assert_eq!(
            align(CENTERS, &sel, &refs, Vec2::ZERO, 5.0),
            (Some(0.0), None)
        );
        // Edges-only finds nothing in the same arrangement.
        assert_eq!(align(EDGES, &sel, &refs, Vec2::ZERO, 5.0), (None, None));
    }

    #[test]
    fn align_uses_group_bbox_union() {
        // A 2-node group; only the union's far edge (30) is near the reference,
        // so a single-node bbox (right edge 10) would miss it.
        let sel = [rect(0.0, 0.0, 10.0, 10.0), rect(20.0, 0.0, 10.0, 10.0)];
        let refs = [rect(28.0, 200.0, 10.0, 10.0)]; // left edge 28
        assert_eq!(
            align(EDGES, &sel, &refs, Vec2::ZERO, 5.0),
            (Some(-2.0), None)
        );
    }

    #[test]
    fn align_applies_raw_delta() {
        // After moving +100 on x, the dragged left edge (100) aligns to 102.
        let sel = [rect(0.0, 0.0, 10.0, 10.0)];
        let refs = [rect(102.0, 100.0, 10.0, 10.0)];
        assert_eq!(
            align(EDGES, &sel, &refs, egui::vec2(100.0, 0.0), 5.0),
            (Some(2.0), None)
        );
    }

    #[test]
    fn align_threshold_guards() {
        let sel = [rect(0.0, 0.0, 10.0, 10.0)];
        let refs = [rect(0.0, 0.0, 10.0, 10.0)];
        assert_eq!(align(EDGES, &sel, &refs, Vec2::ZERO, 0.0), (None, None));
        assert_eq!(
            align(EDGES, &sel, &refs, Vec2::ZERO, f32::NAN),
            (None, None)
        );
    }

    #[test]
    fn align_deterministic_tie_break() {
        // Two equidistant references; with a fixed order the first one wins.
        let sel = [rect(0.0, 0.0, 10.0, 10.0)];
        let refs = [rect(-2.0, 100.0, 10.0, 10.0), rect(2.0, 200.0, 10.0, 10.0)];
        assert_eq!(align(EDGES, &sel, &refs, Vec2::ZERO, 5.0).0, Some(-2.0));
    }

    #[test]
    fn align_line_geometry() {
        // Left edges align (adjust +2); the vertical guide sits at the aligned
        // coordinate and spans both nodes' y extents.
        let sel = [rect(0.0, 0.0, 10.0, 10.0)]; // y: 0..10
        let refs = [rect(2.0, 20.0, 10.0, 8.0)]; // left edge 2, y: 20..28
        let (x, y) = align_adjust(EDGES, &sel, &refs, Vec2::ZERO, 5.0);
        let x = x.expect("x axis should align");
        assert_eq!(x.adjust, 2.0);
        assert_eq!(x.pos, 2.0); // the reference left edge
        assert_eq!(x.span, Rangef::new(0.0, 28.0));
        assert!(y.is_none());
    }

    /// A node with one input at `input` and one output at `output`.
    fn sockets(flow: egui::Direction, input: (f32, f32), output: (f32, f32)) -> AlignSockets {
        AlignSockets {
            flow,
            inputs: vec![egui::pos2(input.0, input.1)],
            outputs: vec![egui::pos2(output.0, output.1)],
        }
    }

    /// `socket_align_adjust` reduced to just the per-axis adjustments.
    fn socket_align(
        sel: &[AlignSockets],
        refs: &[AlignSockets],
        raw: Vec2,
    ) -> (Option<f32>, Option<f32>) {
        let (x, y) = socket_align_adjust(sel, refs, raw, 5.0);
        (x.map(|l| l.adjust), y.map(|l| l.adjust))
    }

    #[test]
    fn socket_align_output_to_input_below() {
        let down = egui::Direction::TopDown;
        // The dragged output at x 10 is 3 left of the input below at x 13.
        let sel = [sockets(down, (10.0, 0.0), (10.0, 20.0))];
        let below = [sockets(down, (13.0, 50.0), (13.0, 70.0))];
        assert_eq!(socket_align(&sel, &below, Vec2::ZERO), (Some(3.0), None));
        // A node above offers its output to the dragged input instead.
        let above = [sockets(down, (13.0, -70.0), (13.0, -50.0))];
        assert_eq!(socket_align(&sel, &above, Vec2::ZERO), (Some(3.0), None));
        // The raw delta moves the dragged sockets first.
        let raw = egui::vec2(100.0, 0.0);
        let far = [sockets(down, (112.0, 50.0), (112.0, 70.0))];
        assert_eq!(socket_align(&sel, &far, raw), (Some(2.0), None));
    }

    #[test]
    fn socket_align_needs_flow_order_and_range() {
        let down = egui::Direction::TopDown;
        let sel = [sockets(down, (10.0, 0.0), (10.0, 20.0))];
        // A node beside the dragged one is neither below nor above it.
        let beside = [sockets(down, (13.0, 0.0), (13.0, 20.0))];
        assert_eq!(socket_align(&sel, &beside, Vec2::ZERO), (None, None));
        // A socket beyond the threshold does not snap.
        let wide = [sockets(down, (16.0, 50.0), (16.0, 70.0))];
        assert_eq!(socket_align(&sel, &wide, Vec2::ZERO), (None, None));
        // Nodes of another flow do not align their sockets.
        let across = [sockets(
            egui::Direction::LeftToRight,
            (13.0, 50.0),
            (13.0, 70.0),
        )];
        assert_eq!(socket_align(&sel, &across, Vec2::ZERO), (None, None));
    }

    #[test]
    fn socket_align_follows_the_flow_axis() {
        let right = egui::Direction::LeftToRight;
        // Left to right, the output on the right aligns on y with an input
        // further right.
        let sel = [sockets(right, (0.0, 10.0), (20.0, 10.0))];
        let next = [sockets(right, (50.0, 12.0), (70.0, 12.0))];
        assert_eq!(socket_align(&sel, &next, Vec2::ZERO), (None, Some(2.0)));
    }

    #[test]
    fn socket_align_group_takes_the_closest_match() {
        let down = egui::Direction::TopDown;
        let sel = [
            sockets(down, (0.0, 0.0), (0.0, 20.0)),
            sockets(down, (40.0, 0.0), (40.0, 20.0)),
        ];
        let below = [
            sockets(down, (4.0, 50.0), (4.0, 70.0)),
            sockets(down, (39.0, 50.0), (39.0, 70.0)),
        ];
        assert_eq!(socket_align(&sel, &below, Vec2::ZERO), (Some(-1.0), None));
    }

    #[test]
    fn socket_align_line_runs_between_the_sockets() {
        let down = egui::Direction::TopDown;
        let sel = [sockets(down, (10.0, 0.0), (10.0, 20.0))];
        let below = [sockets(down, (12.0, 50.0), (12.0, 70.0))];
        let (x, _) = socket_align_adjust(&sel, &below, Vec2::ZERO, 5.0);
        let x = x.expect("x axis should align");
        assert_eq!(x.pos, 12.0);
        assert_eq!(x.span, Rangef::new(20.0, 50.0));
    }

    #[test]
    fn nearer_prefers_the_smaller_adjust_then_the_first() {
        let line = |adjust| AlignLine {
            adjust,
            pos: 0.0,
            span: Rangef::new(0.0, 1.0),
        };
        assert_eq!(nearer(Some(line(3.0)), Some(line(-1.0))), Some(line(-1.0)));
        assert_eq!(nearer(Some(line(2.0)), Some(line(-2.0))), Some(line(2.0)));
        assert_eq!(nearer(None, Some(line(1.0))), Some(line(1.0)));
        assert_eq!(nearer(None, None), None);
    }

    fn pos(x: f32, y: f32) -> egui::Pos2 {
        egui::pos2(x, y)
    }

    #[test]
    fn infer_alignment_from_spread() {
        // Spread mostly in y -> a vertical column (unify x).
        let column = [pos(0.0, 0.0), pos(1.0, 50.0), pos(-1.0, 100.0)];
        assert_eq!(infer_alignment(column), Alignment::Column);
        // Spread mostly in x -> a horizontal row (unify y).
        let row = [pos(0.0, 0.0), pos(50.0, 1.0), pos(100.0, -1.0)];
        assert_eq!(infer_alignment(row), Alignment::Row);
        // Ties (here, all coincident) default to a column.
        let tied = [pos(5.0, 5.0), pos(5.0, 5.0)];
        assert_eq!(infer_alignment(tied), Alignment::Column);
        // Non-finite points are ignored, leaving a clear vertical spread.
        let with_nan = [pos(f32::NAN, f32::NAN), pos(0.0, 0.0), pos(0.0, 80.0)];
        assert_eq!(infer_alignment(with_nan), Alignment::Column);
    }

    /// Build a layout/sizes pair from `(id, x, y, w, h)` rows.
    fn nodes(rows: &[(u64, f32, f32, f32, f32)]) -> (Layout, HashMap<NodeId, Vec2>) {
        let mut layout = Layout::new();
        let mut sizes = HashMap::new();
        for &(id, x, y, w, h) in rows {
            let id = NodeId::from_u64(id);
            layout.insert(id, pos(x, y));
            sizes.insert(id, egui::vec2(w, h));
        }
        (layout, sizes)
    }

    #[test]
    fn align_nodes_centers_to_mean() {
        // A vertical scatter of differing widths; centers should unify to their
        // mean x (centers at 5, 11, 8 -> mean 8), each solved back to top-left.
        let (mut layout, sizes) = nodes(&[
            (0, 0.0, 0.0, 10.0, 10.0),
            (1, 1.0, 50.0, 20.0, 10.0),
            (2, 3.0, 100.0, 10.0, 10.0),
        ]);
        let ids = [0, 1, 2].map(NodeId::from_u64);
        assert_eq!(
            align_nodes(&mut layout, ids, &sizes, AlignBy::Center, None),
            Some(Alignment::Column),
        );
        for (id, w) in [(0u64, 10.0), (1, 20.0), (2, 10.0)] {
            let p = layout[&NodeId::from_u64(id)];
            assert!((p.x + w / 2.0 - 8.0).abs() < 1e-4, "center x of {id}");
        }
        // The off-axis (y) is left untouched.
        assert_eq!(layout[&NodeId::from_u64(1)].y, 50.0);
    }

    #[test]
    fn align_nodes_min_and_max() {
        let rows = [(0u64, 0.0, 0.0, 10.0, 10.0), (1, 4.0, 80.0, 30.0, 10.0)];
        // Min unifies the top-left x to its mean (0 and 4 -> 2).
        let (mut layout, sizes) = nodes(&rows);
        let ids = [0, 1].map(NodeId::from_u64);
        align_nodes(&mut layout, ids, &sizes, AlignBy::Min, None);
        assert!((layout[&NodeId::from_u64(0)].x - 2.0).abs() < 1e-4);
        assert!((layout[&NodeId::from_u64(1)].x - 2.0).abs() < 1e-4);
        // Max unifies the right edges (10 and 34 -> mean 22).
        let (mut layout, sizes) = nodes(&rows);
        align_nodes(&mut layout, ids, &sizes, AlignBy::Max, None);
        assert!((layout[&NodeId::from_u64(0)].x + 10.0 - 22.0).abs() < 1e-4);
        assert!((layout[&NodeId::from_u64(1)].x + 30.0 - 22.0).abs() < 1e-4);
    }

    #[test]
    fn align_nodes_explicit_overrides_inference() {
        // A vertical scatter would infer a column, but Row is forced (unify y).
        let (mut layout, sizes) = nodes(&[(0, 0.0, 0.0, 10.0, 10.0), (1, 1.0, 100.0, 10.0, 10.0)]);
        let ids = [0, 1].map(NodeId::from_u64);
        assert_eq!(
            align_nodes(&mut layout, ids, &sizes, AlignBy::Min, Some(Alignment::Row)),
            Some(Alignment::Row),
        );
        assert_eq!(layout[&NodeId::from_u64(0)].y, 50.0);
        assert_eq!(layout[&NodeId::from_u64(1)].y, 50.0);
    }

    #[test]
    fn align_nodes_needs_two_nodes() {
        let (mut layout, sizes) = nodes(&[(0, 7.0, 7.0, 10.0, 10.0)]);
        let before = layout.clone();
        // A lone node, and a missing node, both resolve to a no-op `None`.
        assert_eq!(
            align_nodes(
                &mut layout,
                [NodeId::from_u64(0)],
                &sizes,
                AlignBy::Center,
                None
            ),
            None,
        );
        assert_eq!(
            align_nodes(
                &mut layout,
                [NodeId::from_u64(0), NodeId::from_u64(99)],
                &sizes,
                AlignBy::Center,
                None,
            ),
            None,
        );
        assert_eq!(layout, before);
    }

    mod selection_area {
        use crate::node::{Node, NodeId};
        use crate::{Graph, View};
        use egui::epaint::ClippedShape;

        const SCREEN: egui::Rect =
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(800.0, 600.0));
        /// A point on the scene background, above and left of the nodes.
        const START: egui::Pos2 = egui::pos2(50.0, 50.0);
        /// A point below and right of the nodes.
        const END: egui::Pos2 = egui::pos2(500.0, 300.0);

        /// Drag a selection area over two nodes, and return the shapes of the
        /// last pass with the style that painted them. With `window`, a window
        /// floats above the graph.
        fn drag_selection(window: bool) -> (Vec<ClippedShape>, std::sync::Arc<egui::Style>) {
            let ctx = egui::Context::default();
            let mut view = View {
                scene_rect: SCREEN,
                layout: [
                    (NodeId(0), egui::pos2(100.0, 100.0)),
                    (NodeId(1), egui::pos2(300.0, 100.0)),
                ]
                .into(),
            };
            let mut pass = |events: Vec<egui::Event>| {
                let input = egui::RawInput {
                    screen_rect: Some(SCREEN),
                    events,
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    Graph::new("graph").show(&mut view, ui, |ui, show| {
                        show.nodes(ui, |nctx, ui| {
                            for id in [NodeId(0), NodeId(1)] {
                                let node = Node::from_id(id).inputs(1).outputs(1);
                                node.show(nctx, ui, |c| c.framed(|ui, _| ui.label("x")));
                            }
                        });
                    });
                    if window {
                        egui::Window::new("window")
                            .fixed_pos(egui::pos2(200.0, 150.0))
                            .show(ui.ctx(), |ui| ui.label("window"));
                    }
                });
                output.textures_delta.clear();
                output.shapes
            };
            pass(vec![egui::Event::PointerMoved(START)]);
            pass(vec![]);
            pass(vec![egui::Event::PointerButton {
                pos: START,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }]);
            let shapes = pass(vec![egui::Event::PointerMoved(END)]);
            (shapes, ctx.global_style())
        }

        /// The index of the selection area among the shapes.
        fn selection_ix(shapes: &[ClippedShape], style: &egui::Style) -> Option<usize> {
            let fill = style.visuals.weak_text_color().linear_multiply(0.125);
            shapes
                .iter()
                .position(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.fill == fill))
        }

        /// The selection area paints after the node frames, content and
        /// sockets.
        #[test]
        fn paints_above_nodes() {
            let (shapes, style) = drag_selection(false);
            assert_eq!(selection_ix(&shapes, &style), Some(shapes.len() - 1));
        }

        /// A window above the graph paints after the selection area.
        #[test]
        fn paints_below_windows() {
            let (shapes, style) = drag_selection(true);
            let ix = selection_ix(&shapes, &style).expect("the selection area is painted");
            let window_text = |s: &ClippedShape| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "window");
            assert!(shapes[ix + 1..].iter().any(window_text));
        }
    }

    mod auto_pan {
        use crate::node::{Node, NodeId};
        use crate::socket::{Socket, SocketKind};
        use crate::{
            auto_pan_velocity, drag_pans, pan_scene_rect, AutoPan, Graph, PressAction, View,
        };
        use egui::{pos2, vec2, Pos2, Rangef, Rect, Vec2};

        /// The graph area of the input-driven tests.
        const SCREEN: Rect = Rect::from_min_max(Pos2::ZERO, pos2(800.0, 600.0));
        const GRAPH: &str = "graph";
        const A: NodeId = NodeId(0);
        /// The time between passes, in seconds.
        const DT: f32 = 1.0 / 60.0;

        /// A graph of the node `A`, driven one pass at a time.
        pub(super) struct TestGraph {
            ctx: egui::Context,
            view: View,
            time: f64,
        }

        /// The graph state observed at the end of a pass.
        pub(super) struct Pass {
            scene_rect: Rect,
            /// The graph-space position of `A`.
            pub(super) a_pos: Pos2,
            /// A screen-space point on the frame of `A`, clear of its content,
            /// as the label within the frame senses presses for text
            /// selection.
            pub(super) a_grip: Pos2,
            /// The graph-space pointer position.
            ptr: Option<Pos2>,
            /// See [`GraphResponse::nodes_moved`](crate::GraphResponse::nodes_moved).
            pub(super) nodes_moved: bool,
        }

        /// Lay out a new graph with `A` at `a_pos`, over two passes, since
        /// sizes settle after the first.
        pub(super) fn test_graph(a_pos: Pos2) -> (TestGraph, Pass) {
            let view = View {
                scene_rect: SCREEN,
                layout: [(A, a_pos)].into(),
            };
            let mut g = TestGraph {
                ctx: egui::Context::default(),
                view,
                time: 0.0,
            };
            pass(&mut g, vec![]);
            let p = pass(&mut g, vec![]);
            (g, p)
        }

        /// Run one pass of the graph with the given input events.
        pub(super) fn pass(g: &mut TestGraph, events: Vec<egui::Event>) -> Pass {
            g.time += f64::from(DT);
            let input = egui::RawInput {
                screen_rect: Some(SCREEN),
                time: Some(g.time),
                predicted_dt: DT,
                events,
                ..Default::default()
            };
            let mut seen = None;
            let mut nodes_moved = false;
            g.ctx
                .run_ui(input, |ui| {
                    let graph = Graph::new(GRAPH).snap(None);
                    let resp = graph.show(&mut g.view, ui, |ui, show| {
                        let mut a = None;
                        show.nodes(ui, |nctx, ui| {
                            let node = Node::from_id(A);
                            a = Some(node.show(nctx, ui, |c| c.framed(|ui, _| ui.label("x"))));
                        });
                        let a = a.expect("the node ran");
                        let ctx = ui.ctx();
                        let to_screen = ctx.layer_transform_to_global(ui.layer_id());
                        let to_screen = to_screen.unwrap_or_default();
                        let ptr = ui.input(|i| i.pointer.latest_pos());
                        let ptr = ptr.map(|p| to_screen.inverse().mul_pos(p));
                        let grip = a.rect.min + Vec2::splat(2.0);
                        seen = Some((to_screen.mul_pos(grip), ptr));
                    });
                    nodes_moved = resp.nodes_moved;
                })
                .textures_delta
                .clear();
            let (a_grip, ptr) = seen.expect("the graph ran");
            Pass {
                scene_rect: g.view.scene_rect,
                a_pos: g.view.layout[&A],
                a_grip,
                ptr,
                nodes_moved,
            }
        }

        /// A press or release of the primary button at `pos`.
        pub(super) fn primary(pos: Pos2, pressed: bool) -> egui::Event {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }
        }

        /// The default auto-pan velocity for a pointer at `ptr` over `SCREEN`.
        fn velocity(ptr: Pos2) -> Vec2 {
            auto_pan_velocity(AutoPan::DEFAULT, SCREEN, ptr)
        }

        #[test]
        fn velocity_is_zero_away_from_the_edges() {
            assert_eq!(velocity(SCREEN.center()), Vec2::ZERO);
            assert_eq!(velocity(pos2(33.0, 33.0)), Vec2::ZERO);
            assert_eq!(velocity(pos2(767.0, 567.0)), Vec2::ZERO);
        }

        #[test]
        fn velocity_grows_toward_each_edge() {
            let AutoPan { margin, max_speed } = AutoPan::DEFAULT;
            let half = max_speed * 0.5;
            let mid = SCREEN.center();
            let x_near_max = SCREEN.max.x - margin * 0.5;
            assert_eq!(velocity(pos2(x_near_max, mid.y)), vec2(half, 0.0));
            assert_eq!(velocity(pos2(margin * 0.5, mid.y)), vec2(-half, 0.0));
            assert_eq!(velocity(pos2(mid.x, margin * 0.5)), vec2(0.0, -half));
            let y_near_max = SCREEN.max.y - margin * 0.5;
            assert_eq!(velocity(pos2(mid.x, y_near_max)), vec2(0.0, half));
            // At an edge and past it, the speed is the maximum.
            assert_eq!(velocity(pos2(SCREEN.max.x, mid.y)), vec2(max_speed, 0.0));
            assert_eq!(velocity(pos2(-100.0, mid.y)), vec2(-max_speed, 0.0));
            // In a corner, both axes pan.
            assert_eq!(velocity(SCREEN.max), vec2(max_speed, max_speed));
        }

        #[test]
        fn velocity_is_zero_for_a_bad_config() {
            let ptr = SCREEN.max;
            for (margin, max_speed) in [(0.0, 1000.0), (32.0, 0.0), (f32::NAN, 1000.0)] {
                let auto_pan = AutoPan { margin, max_speed };
                assert_eq!(auto_pan_velocity(auto_pan, SCREEN, ptr), Vec2::ZERO);
            }
        }

        #[test]
        fn pan_scene_rect_pans_by_screen_pixels() {
            let unbounded = Rangef::new(0.0, f32::INFINITY);
            let scene = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
            // A scale of 2 halves the graph-space distance.
            let out = pan_scene_rect(scene, vec2(200.0, 200.0), unbounded, vec2(10.0, -4.0));
            assert_eq!(out, scene.translate(vec2(5.0, -2.0)));
            // The zoom range clamps the scale, as egui's `Scene` does.
            let range = Rangef::new(0.25, 1.0);
            let out = pan_scene_rect(scene, vec2(10.0, 10.0), range, vec2(10.0, 0.0));
            assert_eq!(out, scene.translate(vec2(40.0, 0.0)));
        }

        #[test]
        fn pan_scene_rect_skips_degenerate_inputs() {
            let unbounded = Rangef::new(0.0, f32::INFINITY);
            let size = vec2(200.0, 200.0);
            let zero = Rect::from_min_size(Pos2::ZERO, vec2(0.0, 100.0));
            assert_eq!(pan_scene_rect(zero, size, unbounded, vec2(1.0, 1.0)), zero);
            let scene = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
            let nan = vec2(f32::NAN, 0.0);
            assert_eq!(pan_scene_rect(scene, size, unbounded, nan), scene);
        }

        #[test]
        fn immutable_graph_pans_only_for_selection() {
            let socket = Socket {
                node: A,
                kind: SocketKind::Output,
                index: 0,
            };
            let drag = PressAction::DragNodes { node: None };
            let edge = PressAction::Socket(socket);
            assert!(drag_pans(&PressAction::Select, false));
            assert!(drag_pans(&drag, false));
            assert!(drag_pans(&edge, false));
            assert!(drag_pans(&PressAction::Select, true));
            assert!(!drag_pans(&drag, true));
            assert!(!drag_pans(&edge, true));
        }

        /// A node dragged to the right edge pans the view right, and the node
        /// stays under the pointer in graph space.
        #[test]
        fn node_drag_near_edge_pans_view() {
            let (mut g, p) = test_graph(pos2(400.0, 300.0));
            let start = p.a_grip;
            pass(&mut g, vec![egui::Event::PointerMoved(start)]);
            pass(&mut g, vec![primary(start, true)]);
            // Hold the pointer 4 pixels inside the right edge.
            let near_edge = pos2(SCREEN.max.x - 4.0, start.y);
            let first = pass(&mut g, vec![egui::Event::PointerMoved(near_edge)]);
            let ptr_offset = |p: &Pass| p.a_pos - p.ptr.expect("the pointer is over the graph");
            let offset = ptr_offset(&first);
            // A scene scale of 1, with the pointer 28 pixels into the margin.
            let step = velocity(near_edge).x * DT;
            assert!(step > 0.0);
            let mut prev = first;
            for _ in 0..5 {
                let p = pass(&mut g, vec![]);
                let pan = p.scene_rect.min - prev.scene_rect.min;
                assert!((pan - vec2(step, 0.0)).length() < 1e-2, "pan {pan:?}");
                assert!((ptr_offset(&p) - offset).length() < 1e-2);
                prev = p;
            }
        }

        /// A press on a node near the edge that does not move is a possible
        /// click, so the view does not pan.
        #[test]
        fn press_without_drag_does_not_pan() {
            let (mut g, p) = test_graph(pos2(780.0, 300.0));
            let start = p.a_grip;
            assert!(velocity(start).x > 0.0, "the node is within the margin");
            pass(&mut g, vec![egui::Event::PointerMoved(start)]);
            let pressed = pass(&mut g, vec![primary(start, true)]);
            for _ in 0..5 {
                let p = pass(&mut g, vec![]);
                assert_eq!(p.scene_rect, pressed.scene_rect);
            }
        }
    }

    mod nodes_moved {
        use super::auto_pan::{pass, primary, test_graph};
        use egui::{pos2, vec2};

        /// A nudge smaller than egui's drag threshold still moves the node,
        /// so it counts as a move, on the release frame only.
        #[test]
        fn a_small_nudge_is_a_move() {
            let (mut g, p) = test_graph(pos2(400.0, 300.0));
            let start = p.a_grip;
            let a_start = p.a_pos;
            pass(&mut g, vec![egui::Event::PointerMoved(start)]);
            assert!(!pass(&mut g, vec![primary(start, true)]).nodes_moved);
            let nudged = start + vec2(3.0, 0.0);
            let moving = pass(&mut g, vec![egui::Event::PointerMoved(nudged)]);
            assert!(!moving.nodes_moved);
            assert_eq!(moving.a_pos, a_start + vec2(3.0, 0.0));
            assert!(pass(&mut g, vec![primary(nudged, false)]).nodes_moved);
            assert!(!pass(&mut g, vec![]).nodes_moved);
        }

        #[test]
        fn a_press_in_place_is_not_a_move() {
            let (mut g, p) = test_graph(pos2(400.0, 300.0));
            let start = p.a_grip;
            pass(&mut g, vec![egui::Event::PointerMoved(start)]);
            pass(&mut g, vec![primary(start, true)]);
            assert!(!pass(&mut g, vec![primary(start, false)]).nodes_moved);
        }

        #[test]
        fn a_drag_back_to_the_start_is_not_a_move() {
            let (mut g, p) = test_graph(pos2(400.0, 300.0));
            let start = p.a_grip;
            pass(&mut g, vec![egui::Event::PointerMoved(start)]);
            pass(&mut g, vec![primary(start, true)]);
            let away = start + vec2(40.0, 20.0);
            pass(&mut g, vec![egui::Event::PointerMoved(away)]);
            pass(&mut g, vec![egui::Event::PointerMoved(start)]);
            let released = pass(&mut g, vec![primary(start, false)]);
            assert_eq!(released.a_pos, p.a_pos);
            assert!(!released.nodes_moved);
        }
    }
}
