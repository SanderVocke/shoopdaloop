//! Socket-aware automatic layout of nodes.
//!
//! A dependency-free, port-aware layered ("Sugiyama") layout: nodes are
//! assigned to layers along the flow direction, ordered within layers to
//! minimise edge crossings, and positioned so that edges run as straight as
//! possible - all computed per *socket* connection point rather than per node
//! centre, following Schulze, Spönemann and von Hanxleden, "Drawing Layered
//! Graphs with Port Constraints" (JVLC 2014).

use crate::edge::{InputIx, OutputIx};
use crate::{Layout, NodeId};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub use route::route_edges;

mod acyclic;
mod order;
mod place;
mod rank;
mod route;

/// Per-node input to [`layout`]: the node's size and the layout of its
/// sockets.
pub struct LayoutNode {
    size: egui::Vec2,
    socket_padding: f32,
    inputs: LayoutSockets,
    outputs: LayoutSockets,
    flow: Option<egui::Direction>,
}

/// Parameters controlling [`layout`].
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LayoutParams {
    /// The direction in which edges should flow.
    pub flow: egui::Direction,
    /// The gap between adjacent layers along the flow direction.
    pub layer_gap: f32,
    /// The gap between adjacent nodes within a layer.
    pub node_gap: f32,
    /// The gap between disconnected components of the graph.
    pub component_gap: f32,
    /// Whether the layout accounts for the socket each edge connects to.
    ///
    /// When `false`, sockets are ignored: every edge anchors at its nodes'
    /// cross-axis centres and socket order plays no part in crossing
    /// minimisation, matching classic node-size-only layered layouts.
    /// Defaults to `true`.
    pub socket_aware: bool,
    /// Whether node placement favours straight edges.
    ///
    /// When `true`, a node lines up with one of the sockets it connects to
    /// rather than with the midpoint between two, and a last pass moves nodes
    /// to where more edges run straight. Layer order and node gaps hold either
    /// way, so edges cross no more often and nodes never overlap. Defaults to
    /// `false`.
    pub straighten_edges: bool,
}

/// Corridor waypoints for edges, produced by [`layout_routed`] or
/// [`route_edges`].
///
/// A route exists only for an edge whose direct socket-to-socket curve could
/// overlap a node; all other edges look best as plain curves.
///
/// Routes are tied to the node positions they were produced against: once
/// nodes move away from them, recompute (or discard) the routes rather than
/// threading edges through outdated corridors.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EdgeRoutes {
    routes: HashMap<((NodeId, OutputIx), (NodeId, InputIx)), Vec<Vec<egui::Pos2>>>,
}

/// Socket positions along a node's edge, mirroring
/// [`SocketLayout`](crate::SocketLayout).
enum LayoutSockets {
    EvenlySpaced(usize),
    /// Cross-axis offsets relative to the node's top-left corner.
    Explicit(Vec<f32>),
}

/// A connected component of the graph, canonicalised into layout space: the
/// main axis points from sources toward sinks and the cross axis spans each
/// layer.
struct CGraph {
    /// Main-axis extent of each node.
    size_main: Vec<f32>,
    /// Cross-axis extent of each node.
    size_cross: Vec<f32>,
    /// Cross-axis offsets of each node's input sockets from its centre.
    in_anchors: Vec<Vec<f32>>,
    /// Cross-axis offsets of each node's output sockets from its centre.
    out_anchors: Vec<Vec<f32>>,
    edges: Vec<CEdge>,
}

/// An edge between component-local node indices.
struct CEdge {
    src: usize,
    src_socket: usize,
    dst: usize,
    dst_socket: usize,
}

/// The bounding box of a laid-out component in canonical space.
struct Bounds {
    min_main: f32,
    max_main: f32,
    min_cross: f32,
    max_cross: f32,
}

/// A laid-out component, in canonical space.
struct Placed {
    /// Global node indices of the component's members.
    members: Vec<usize>,
    /// The shared flow direction of the component's members.
    flow: egui::Direction,
    /// The `(main, cross)` centre of each member.
    centers: Vec<(f32, f32)>,
    /// Corridor waypoints of each component edge.
    routes: Vec<Vec<(f32, f32)>>,
    /// The global edge index of each component edge.
    edge_ixs: Vec<usize>,
    bounds: Bounds,
}

impl LayoutNode {
    /// A node of the given size with no sockets.
    pub fn new(size: impl Into<egui::Vec2>) -> Self {
        Self {
            size: size.into(),
            socket_padding: 0.0,
            inputs: LayoutSockets::EvenlySpaced(0),
            outputs: LayoutSockets::EvenlySpaced(0),
            flow: None,
        }
    }

    /// The padding used when deriving evenly spaced socket positions.
    ///
    /// Pass [`socket_padding`](crate::socket_padding) for exact agreement
    /// with the socket positions used when rendering nodes. Defaults to
    /// `0.0`.
    pub fn socket_padding(mut self, padding: f32) -> Self {
        self.socket_padding = padding;
        self
    }

    /// The number of input sockets, evenly spaced along the node's edge.
    pub fn inputs(mut self, count: usize) -> Self {
        self.inputs = LayoutSockets::EvenlySpaced(count);
        self
    }

    /// The number of output sockets, evenly spaced along the node's edge.
    pub fn outputs(mut self, count: usize) -> Self {
        self.outputs = LayoutSockets::EvenlySpaced(count);
        self
    }

    /// Explicit input socket offsets along the cross axis (`y` for horizontal
    /// flows, `x` for vertical), relative to the node's top-left corner.
    ///
    /// Non-finite offsets anchor at the node's cross-axis centre.
    pub fn input_offsets(mut self, offsets: Vec<f32>) -> Self {
        self.inputs = LayoutSockets::Explicit(offsets);
        self
    }

    /// Explicit output socket offsets along the cross axis (`y` for
    /// horizontal flows, `x` for vertical), relative to the node's top-left
    /// corner.
    ///
    /// Non-finite offsets anchor at the node's cross-axis centre.
    pub fn output_offsets(mut self, offsets: Vec<f32>) -> Self {
        self.outputs = LayoutSockets::Explicit(offsets);
        self
    }

    /// Override the flow direction for this node alone.
    ///
    /// Nodes connected only to others of the same effective flow are laid out
    /// together in that flow; edges joining nodes of different flows do not
    /// bind them into one cluster, and the clusters are instead arranged
    /// along the outer direction ([`LayoutParams::flow`]). Defaults to the
    /// params' flow.
    pub fn flow(mut self, flow: egui::Direction) -> Self {
        self.flow = Some(flow);
        self
    }
}

impl LayoutParams {
    /// Default gap between adjacent layers along the flow direction.
    pub const DEFAULT_LAYER_GAP: f32 = 50.0;
    /// Default gap between adjacent nodes within a layer.
    pub const DEFAULT_NODE_GAP: f32 = 25.0;
    /// Default gap between disconnected components of the graph.
    pub const DEFAULT_COMPONENT_GAP: f32 = 50.0;

    /// Parameters for the given flow direction with default gaps.
    pub fn new(flow: egui::Direction) -> Self {
        Self {
            flow,
            layer_gap: Self::DEFAULT_LAYER_GAP,
            node_gap: Self::DEFAULT_NODE_GAP,
            component_gap: Self::DEFAULT_COMPONENT_GAP,
            socket_aware: true,
            straighten_edges: false,
        }
    }

    /// Set the gap between adjacent layers along the flow direction.
    pub fn layer_gap(mut self, gap: f32) -> Self {
        self.layer_gap = gap;
        self
    }

    /// Set the gap between adjacent nodes within a layer.
    pub fn node_gap(mut self, gap: f32) -> Self {
        self.node_gap = gap;
        self
    }

    /// Set the gap between disconnected components of the graph.
    pub fn component_gap(mut self, gap: f32) -> Self {
        self.component_gap = gap;
        self
    }

    /// Set whether the layout accounts for the socket each edge connects to.
    pub fn socket_aware(mut self, socket_aware: bool) -> Self {
        self.socket_aware = socket_aware;
        self
    }

    /// Set whether node placement favours straight edges.
    pub fn straighten_edges(mut self, straighten: bool) -> Self {
        self.straighten_edges = straighten;
        self
    }
}

impl EdgeRoutes {
    /// The corridor waypoints for the given edge, ordered from the output
    /// socket toward the input socket, in the same coordinate space as the
    /// accompanying [`Layout`].
    ///
    /// `occurrence` distinguishes multiple edges connecting the same pair of
    /// sockets; pass `0` for the first (or only) such edge.
    ///
    /// Returns `None` when the edge needs no routing or is unknown.
    pub fn route(
        &self,
        a: (NodeId, OutputIx),
        b: (NodeId, InputIx),
        occurrence: usize,
    ) -> Option<&[egui::Pos2]> {
        self.routes
            .get(&(a, b))
            .and_then(|routes| routes.get(occurrence))
            .map(Vec::as_slice)
    }

    /// The number of routed edges.
    pub fn len(&self) -> usize {
        self.routes.values().map(Vec::len).sum()
    }

    /// Whether no edges required routing.
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}

impl Bounds {
    fn main_extent(&self) -> f32 {
        self.max_main - self.min_main
    }

    fn cross_extent(&self) -> f32 {
        self.max_cross - self.min_cross
    }
}

impl From<egui::Direction> for LayoutParams {
    fn from(flow: egui::Direction) -> Self {
        Self::new(flow)
    }
}

/// Construct a layout for a directed graph from its nodes' sizes and socket
/// layouts and the socket-level connections between them.
///
/// Edges are identified by `(node, output socket)` -> `(node, input socket)`
/// pairs, matching [`Edge::new`](crate::edge::Edge::new). Socket indices
/// without a known offset anchor at the node's cross-axis centre. Edges
/// referencing unknown nodes and self-loops are ignored; when a node id
/// occurs more than once, the last occurrence wins.
///
/// Returns a [`Layout`] with the position of each node's top-left corner,
/// with the bounding box of the laid-out graph centred around the origin.
/// The same input always produces the same layout.
pub fn layout(
    nodes: impl IntoIterator<Item = (NodeId, LayoutNode)>,
    edges: impl IntoIterator<Item = ((NodeId, OutputIx), (NodeId, InputIx))>,
    params: impl Into<LayoutParams>,
) -> Layout {
    layout_routed(nodes, edges, params).0
}

/// Like [`layout`], but also returns corridor waypoints for the edges that
/// span multiple layers and whose direct curves could otherwise overlap
/// nodes.
///
/// Pass each edge's route to [`Edge::waypoints`](crate::edge::Edge::waypoints)
/// to thread its curve through the corridors reserved by the layout. Routes
/// share the returned [`Layout`]'s coordinate space.
pub fn layout_routed(
    nodes: impl IntoIterator<Item = (NodeId, LayoutNode)>,
    edges: impl IntoIterator<Item = ((NodeId, OutputIx), (NodeId, InputIx))>,
    params: impl Into<LayoutParams>,
) -> (Layout, EdgeRoutes) {
    let params = params.into();

    // Canonicalise the nodes in a deterministic order, each by its own flow.
    let nodes: BTreeMap<NodeId, LayoutNode> = nodes.into_iter().collect();
    let ids: Vec<NodeId> = nodes.keys().copied().collect();
    let index_of: HashMap<NodeId, usize> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut node_flow = Vec::with_capacity(ids.len());
    let mut size_screen = Vec::with_capacity(ids.len());
    let mut size_main = Vec::with_capacity(ids.len());
    let mut size_cross = Vec::with_capacity(ids.len());
    let mut in_anchors = Vec::with_capacity(ids.len());
    let mut out_anchors = Vec::with_capacity(ids.len());
    for node in nodes.values() {
        let flow = node.flow.unwrap_or(params.flow);
        let size = sanitize_size(node.size);
        let (main, cross) = canonical_size(flow, size);
        node_flow.push(flow);
        size_screen.push(size);
        size_main.push(main);
        size_cross.push(cross);
        // Empty anchor sets make every socket fall back to the cross-axis
        // centre.
        let (ins, outs) = if params.socket_aware {
            (
                resolve_anchors(&node.inputs, cross, node.socket_padding),
                resolve_anchors(&node.outputs, cross, node.socket_padding),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        in_anchors.push(ins);
        out_anchors.push(outs);
    }

    // Sanitise the edges; their order must not affect the result.
    let mut cedges: Vec<CEdge> = edges
        .into_iter()
        .filter_map(|((a, src_socket), (b, dst_socket))| {
            let (Some(&src), Some(&dst)) = (index_of.get(&a), index_of.get(&b)) else {
                return None;
            };
            (src != dst).then_some(CEdge {
                src,
                src_socket,
                dst,
                dst_socket,
            })
        })
        .collect();
    cedges.sort_by_key(|e| (e.src, e.dst, e.src_socket, e.dst_socket));

    // Split into clusters: weakly-connected components that share a flow.
    // An edge joining two flows never unions its endpoints, so every cluster
    // has a single, well-defined flow (proven by induction over the unions).
    let mut parent: Vec<usize> = (0..ids.len()).collect();
    for e in &cedges {
        if node_flow[e.src] != node_flow[e.dst] {
            continue;
        }
        let (ra, rb) = (uf_find(&mut parent, e.src), uf_find(&mut parent, e.dst));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    let mut members_by_root: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut local_of = vec![0usize; ids.len()];
    for v in 0..ids.len() {
        let members = members_by_root.entry(uf_find(&mut parent, v)).or_default();
        local_of[v] = members.len();
        members.push(v);
    }
    // When ignoring sockets, collapse the socket indices fed to the layout
    // pipeline so all of a node's edges share one centre port; `cedges`
    // keeps the originals for route keys.
    let layout_socket = |socket: usize| if params.socket_aware { socket } else { 0 };
    let mut edges_by_root: BTreeMap<usize, (Vec<CEdge>, Vec<usize>)> = BTreeMap::new();
    for (e, edge) in cedges.iter().enumerate() {
        let root = uf_find(&mut parent, edge.src);
        // Cut edges join different clusters; they carry no corridor and are
        // positioned by the outer arrangement, not any single cluster.
        if root != uf_find(&mut parent, edge.dst) {
            continue;
        }
        let (local, edge_ixs) = edges_by_root.entry(root).or_default();
        local.push(CEdge {
            src: local_of[edge.src],
            src_socket: layout_socket(edge.src_socket),
            dst: local_of[edge.dst],
            dst_socket: layout_socket(edge.dst_socket),
        });
        edge_ixs.push(e);
    }

    // Lay out each cluster independently in canonical space, recording which
    // cluster every node belongs to for the outer arrangement.
    let mut cluster_of = vec![0usize; ids.len()];
    let mut placed: Vec<Placed> = members_by_root
        .into_iter()
        .enumerate()
        .map(|(cluster, (root, members))| {
            for &v in &members {
                cluster_of[v] = cluster;
            }
            let (edges, edge_ixs) = edges_by_root.remove(&root).unwrap_or_default();
            let cg = CGraph {
                size_main: members.iter().map(|&v| size_main[v]).collect(),
                size_cross: members.iter().map(|&v| size_cross[v]).collect(),
                in_anchors: members.iter().map(|&v| in_anchors[v].clone()).collect(),
                out_anchors: members.iter().map(|&v| out_anchors[v].clone()).collect(),
                edges,
            };
            let (centers, routes, bounds) = layout_connected(&cg, &params);
            Placed {
                flow: node_flow[members[0]],
                members,
                centers,
                routes,
                edge_ixs,
                bounds,
            }
        })
        .collect();

    // Arrange the clusters in screen space. A single shared flow keeps the
    // historical cross-axis packing (preserving its mirror and transpose
    // symmetries; with one flow there are no cross-cluster edges). Mixed
    // flows are arranged by an outer pass that gives each cluster its own
    // orientation, so canonical cross axes never collide in screen space.
    let single_flow = node_flow
        .first()
        .map_or(true, |&f0| node_flow.iter().all(|&f| f == f0));
    let (mut tls, mut edge_waypoints) = if single_flow {
        arrange_packed(&mut placed, &params, &size_screen, ids.len(), cedges.len())
    } else {
        arrange_mixed(
            &placed,
            &params,
            &size_screen,
            &cluster_of,
            &cedges,
            ids.len(),
        )
    };

    // Centre the overall bounding box around the origin.
    let mut min = egui::Pos2::new(f32::INFINITY, f32::INFINITY);
    let mut max = egui::Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for (&tl, &size) in tls.iter().zip(&size_screen) {
        min = min.min(tl);
        max = max.max(tl + size);
    }
    let center_shift = if tls.is_empty() {
        egui::Vec2::ZERO
    } else {
        (min.to_vec2() + max.to_vec2()) * 0.5
    };
    for tl in &mut tls {
        *tl -= center_shift;
    }
    for route in &mut edge_waypoints {
        for wp in route.iter_mut() {
            *wp -= center_shift;
        }
    }

    // Keep only the routes whose edges actually need them.
    let rects: Vec<egui::Rect> = tls
        .iter()
        .zip(&size_screen)
        .map(|(&tl, &size)| egui::Rect::from_min_size(tl, size))
        .collect();
    let mut routes = EdgeRoutes::default();
    for (edge, waypoints) in cedges.iter().zip(edge_waypoints) {
        if waypoints.is_empty() {
            continue;
        }
        // Cut edges (between clusters) reserve no corridor and were skipped
        // above; an intra-cluster edge's endpoints share their cluster's flow.
        let flow = node_flow[edge.src];
        let a_anchor = anchor(&out_anchors[edge.src], edge.src_socket);
        let b_anchor = anchor(&in_anchors[edge.dst], edge.dst_socket);
        let a = anchored_socket_pos(flow, rects[edge.src], false, a_anchor);
        let b = anchored_socket_pos(flow, rects[edge.dst], true, b_anchor);
        if direct_curve_clear(flow, &params, &rects, edge, a, b) {
            continue;
        }
        let key = (
            (ids[edge.src], edge.src_socket),
            (ids[edge.dst], edge.dst_socket),
        );
        routes.routes.entry(key).or_default().push(waypoints);
    }

    let layout = ids.iter().copied().zip(tls).collect();
    (layout, routes)
}

/// Construct a layout from node sizes alone.
///
/// A convenience over [`layout`] for graphs without socket information:
/// every edge anchors at its nodes' cross-axis centres.
pub fn layout_from_sizes(
    nodes: impl IntoIterator<Item = (NodeId, egui::Vec2)>,
    edges: impl IntoIterator<Item = (NodeId, NodeId)>,
    flow: egui::Direction,
) -> Layout {
    layout(
        nodes
            .into_iter()
            .map(|(id, size)| (id, LayoutNode::new(size))),
        edges.into_iter().map(|(a, b)| ((a, 0), (b, 0))),
        flow,
    )
}

/// Lay out a single weakly-connected component in canonical space, returning
/// the `(main, cross)` centre of each node, the corridor waypoints of each
/// edge (ordered from its output end to its input end), and the component
/// bounds.
fn layout_connected(
    cg: &CGraph,
    params: &LayoutParams,
) -> (Vec<(f32, f32)>, Vec<Vec<(f32, f32)>>, Bounds) {
    let n = cg.size_main.len();
    let pairs: Vec<(usize, usize)> = cg.edges.iter().map(|e| (e.src, e.dst)).collect();
    let reversed = acyclic::break_cycles(n, &pairs);
    let oriented: Vec<(usize, usize)> = pairs
        .iter()
        .zip(&reversed)
        .map(|(&(src, dst), &rev)| if rev { (dst, src) } else { (src, dst) })
        .collect();
    let layer = rank::assign_layers(n, &oriented);
    let mut pg = order::build_proper(cg, &reversed, &layer);
    order::minimize_crossings(&mut pg);

    // Dummy vertices occupy no space.
    let num_v = pg.pos.len();
    let mut size_main_all = vec![0.0f32; num_v];
    let mut size_cross_all = vec![0.0f32; num_v];
    size_main_all[..n].copy_from_slice(&cg.size_main);
    size_cross_all[..n].copy_from_slice(&cg.size_cross);
    let cross = place::assign_cross(
        &pg,
        &size_cross_all,
        params.node_gap,
        params.straighten_edges,
    );
    let main = place::assign_main(&pg, &size_main_all, params.layer_gap);

    let centers: Vec<(f32, f32)> = (0..n).map(|v| (main[v], cross[v])).collect();
    // The dummy chains become corridor waypoints; reversed edges run their
    // chain from the input end, so flip them to output-to-input order.
    let routes: Vec<Vec<(f32, f32)>> = pg
        .edge_dummies
        .iter()
        .zip(&reversed)
        .map(|(dummies, &rev)| {
            let mut route: Vec<(f32, f32)> = dummies.iter().map(|&d| (main[d], cross[d])).collect();
            if rev {
                route.reverse();
            }
            route
        })
        .collect();

    let mut bounds = Bounds {
        min_main: f32::INFINITY,
        max_main: f32::NEG_INFINITY,
        min_cross: f32::INFINITY,
        max_cross: f32::NEG_INFINITY,
    };
    for (v, &(main, cross)) in centers.iter().enumerate() {
        bounds.min_main = bounds.min_main.min(main - cg.size_main[v] * 0.5);
        bounds.max_main = bounds.max_main.max(main + cg.size_main[v] * 0.5);
        bounds.min_cross = bounds.min_cross.min(cross - cg.size_cross[v] * 0.5);
        bounds.max_cross = bounds.max_cross.max(cross + cg.size_cross[v] * 0.5);
    }
    // Corridors claim space too, keeping them clear of neighbouring
    // components when packing.
    let half_corridor = params.node_gap * 0.5;
    for &(main, cross) in routes.iter().flatten() {
        bounds.min_main = bounds.min_main.min(main);
        bounds.max_main = bounds.max_main.max(main);
        bounds.min_cross = bounds.min_cross.min(cross - half_corridor);
        bounds.max_cross = bounds.max_cross.max(cross + half_corridor);
    }
    (centers, routes, bounds)
}

/// Split a node's screen size into canonical `(main, cross)` extents for the
/// given flow: the main axis runs along the flow, the cross axis across it.
fn canonical_size(flow: egui::Direction, size: egui::Vec2) -> (f32, f32) {
    if matches!(
        flow,
        egui::Direction::LeftToRight | egui::Direction::RightToLeft
    ) {
        (size.x, size.y)
    } else {
        (size.y, size.x)
    }
}

/// Map a canonical `(main, cross)` point back to screen space for `flow`,
/// after shifting the cluster's bounds by `main_shift`/`cross_shift`.
fn to_screen(
    flow: egui::Direction,
    main_shift: f32,
    cross_shift: f32,
    (main, cross): (f32, f32),
) -> egui::Pos2 {
    let main = main + main_shift;
    let cross = cross + cross_shift;
    let main = match flow {
        egui::Direction::LeftToRight | egui::Direction::TopDown => main,
        egui::Direction::RightToLeft | egui::Direction::BottomUp => -main,
    };
    if matches!(
        flow,
        egui::Direction::LeftToRight | egui::Direction::RightToLeft
    ) {
        egui::Pos2::new(main, cross)
    } else {
        egui::Pos2::new(cross, main)
    }
}

/// The screen-space bounding box covering every rect and point.
fn screen_bounds(rects: &[egui::Rect], points: &[egui::Pos2]) -> egui::Rect {
    let mut min = egui::Pos2::new(f32::INFINITY, f32::INFINITY);
    let mut max = egui::Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for r in rects {
        min = min.min(r.min);
        max = max.max(r.max);
    }
    for &p in points {
        min = min.min(p);
        max = max.max(p);
    }
    egui::Rect::from_min_max(min, max)
}

/// Pack clusters that all share one flow along the cross axis, largest first,
/// returning each node's screen top-left and each edge's screen waypoints.
///
/// With a single flow every cluster maps its canonical cross axis onto the
/// same screen axis, so a shared cross cursor stacks them without overlap and
/// reproduces the historical single-flow layout.
fn arrange_packed(
    placed: &mut [Placed],
    params: &LayoutParams,
    size_screen: &[egui::Vec2],
    num_nodes: usize,
    num_edges: usize,
) -> (Vec<egui::Pos2>, Vec<Vec<egui::Pos2>>) {
    placed.sort_by(|a, b| {
        b.bounds
            .main_extent()
            .total_cmp(&a.bounds.main_extent())
            .then(b.bounds.cross_extent().total_cmp(&a.bounds.cross_extent()))
            .then(a.members[0].cmp(&b.members[0]))
    });
    let mut tls = vec![egui::Pos2::ZERO; num_nodes];
    let mut edge_waypoints: Vec<Vec<egui::Pos2>> = vec![Vec::new(); num_edges];
    let mut cross_cursor = 0.0;
    for p in placed.iter() {
        let main_shift = -p.bounds.min_main;
        let cross_shift = cross_cursor - p.bounds.min_cross;
        for (&global, &center) in p.members.iter().zip(&p.centers) {
            tls[global] =
                to_screen(p.flow, main_shift, cross_shift, center) - size_screen[global] * 0.5;
        }
        for (&e, route) in p.edge_ixs.iter().zip(&p.routes) {
            edge_waypoints[e] = route
                .iter()
                .map(|&wp| to_screen(p.flow, main_shift, cross_shift, wp))
                .collect();
        }
        cross_cursor += p.bounds.cross_extent() + params.component_gap;
    }
    (tls, edge_waypoints)
}

/// A cluster laid out into a local screen frame, its bounding box min at the
/// origin, ready to be positioned by the outer arrangement.
struct ClusterFrame {
    /// Screen top-left of each member, relative to the cluster's bbox min.
    member_tls: Vec<egui::Pos2>,
    /// Screen waypoints of each cluster edge, relative to the bbox min.
    routes: Vec<Vec<egui::Pos2>>,
    /// The cluster's screen-space bounding box size.
    size: egui::Vec2,
}

/// Place each cluster into a local screen frame using its own flow.
fn cluster_frames(placed: &[Placed], size_screen: &[egui::Vec2]) -> Vec<ClusterFrame> {
    placed
        .iter()
        .map(|p| {
            let main_shift = -p.bounds.min_main;
            let cross_shift = -p.bounds.min_cross;
            let member_tls: Vec<egui::Pos2> = p
                .members
                .iter()
                .zip(&p.centers)
                .map(|(&g, &c)| {
                    to_screen(p.flow, main_shift, cross_shift, c) - size_screen[g] * 0.5
                })
                .collect();
            let routes: Vec<Vec<egui::Pos2>> = p
                .routes
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|&wp| to_screen(p.flow, main_shift, cross_shift, wp))
                        .collect()
                })
                .collect();
            let rects: Vec<egui::Rect> = member_tls
                .iter()
                .zip(&p.members)
                .map(|(&tl, &g)| egui::Rect::from_min_size(tl, size_screen[g]))
                .collect();
            let route_pts: Vec<egui::Pos2> = routes.iter().flatten().copied().collect();
            let bbox = screen_bounds(&rects, &route_pts);
            let shift = bbox.min.to_vec2();
            ClusterFrame {
                member_tls: member_tls.iter().map(|&tl| tl - shift).collect(),
                routes: routes
                    .iter()
                    .map(|r| r.iter().map(|&p| p - shift).collect())
                    .collect(),
                size: bbox.size(),
            }
        })
        .collect()
}

/// Arrange clusters of differing flows. Each cluster is laid out in its own
/// flow, then the clusters' screen bounding boxes are positioned by an outer
/// layout over the edges that cross cluster boundaries, and the members
/// translated into place.
fn arrange_mixed(
    placed: &[Placed],
    params: &LayoutParams,
    size_screen: &[egui::Vec2],
    cluster_of: &[usize],
    cedges: &[CEdge],
    num_nodes: usize,
) -> (Vec<egui::Pos2>, Vec<Vec<egui::Pos2>>) {
    let frames = cluster_frames(placed, size_screen);

    // Position the cluster boxes with a meta-graph: one node per cluster,
    // sized by its screen bounding box, joined by the cut edges (deduplicated,
    // keeping their direction). The outer flow is the params' flow. The
    // meta-graph is single-flow, so this never recurses back into this branch.
    let meta_id = |c: usize| NodeId::from_u64(c as u64);
    let meta_nodes = frames
        .iter()
        .enumerate()
        .map(|(c, f)| (meta_id(c), LayoutNode::new(f.size)));
    let mut meta_edges: BTreeSet<(usize, usize)> = BTreeSet::new();
    for e in cedges {
        let (src, dst) = (cluster_of[e.src], cluster_of[e.dst]);
        if src != dst {
            meta_edges.insert((src, dst));
        }
    }
    let meta_params = LayoutParams {
        socket_aware: false,
        ..params.clone()
    };
    let meta = layout(
        meta_nodes,
        meta_edges
            .iter()
            .map(|&(a, b)| ((meta_id(a), 0), (meta_id(b), 0))),
        meta_params,
    );

    // Each cluster fills its meta node's rect, so its bbox min lands on the
    // meta node's top-left.
    let mut tls = vec![egui::Pos2::ZERO; num_nodes];
    let mut edge_waypoints: Vec<Vec<egui::Pos2>> = vec![Vec::new(); cedges.len()];
    for (c, (p, frame)) in placed.iter().zip(&frames).enumerate() {
        let offset = meta[&meta_id(c)].to_vec2();
        for (&g, &tl) in p.members.iter().zip(&frame.member_tls) {
            tls[g] = tl + offset;
        }
        for (&e, route) in p.edge_ixs.iter().zip(&frame.routes) {
            edge_waypoints[e] = route.iter().map(|&wp| wp + offset).collect();
        }
    }
    (tls, edge_waypoints)
}

/// The anchor offset of `socket` within `anchors`, falling back to the
/// cross-axis centre for sockets without a known offset.
fn anchor(anchors: &[f32], socket: usize) -> f32 {
    anchors.get(socket).copied().unwrap_or(0.0)
}

/// The screen-space position of a node's socket given its final rect and the
/// socket's cross-axis anchor offset from the node centre.
fn anchored_socket_pos(
    flow: egui::Direction,
    rect: egui::Rect,
    is_input: bool,
    anchor: f32,
) -> egui::Pos2 {
    let cross_center = match flow {
        egui::Direction::LeftToRight | egui::Direction::RightToLeft => rect.center().y,
        egui::Direction::TopDown | egui::Direction::BottomUp => rect.center().x,
    };
    crate::socket::layout::socket_pos(flow, rect, is_input, cross_center + anchor)
}

/// The screen-space unit vector along the flow direction.
fn flow_vec(flow: egui::Direction) -> egui::Vec2 {
    match flow {
        egui::Direction::LeftToRight => egui::Vec2::new(1.0, 0.0),
        egui::Direction::RightToLeft => egui::Vec2::new(-1.0, 0.0),
        egui::Direction::TopDown => egui::Vec2::new(0.0, 1.0),
        egui::Direction::BottomUp => egui::Vec2::new(0.0, -1.0),
    }
}

/// Whether the direct socket-to-socket curve between `a` and `b` is
/// guaranteed to clear every node other than `edge`'s own endpoints, making
/// a corridor route unnecessary.
///
/// The direct cubic's control points extend from the sockets along their
/// normals by at most half the socket distance, so the curve is bounded by
/// the hull of the sockets and the strongest possible control points.
fn direct_curve_clear(
    flow: egui::Direction,
    params: &LayoutParams,
    rects: &[egui::Rect],
    edge: &CEdge,
    a: egui::Pos2,
    b: egui::Pos2,
) -> bool {
    let flow = flow_vec(flow);
    let ctrl_len = a.distance(b) * crate::bezier::Cubic::MAX_CURVATURE_FACTOR;
    let mut hull = egui::Rect::from_two_pos(a, b);
    hull.extend_with(a + flow * ctrl_len);
    hull.extend_with(b - flow * ctrl_len);
    let clearance = params.node_gap * 0.25;
    rects
        .iter()
        .enumerate()
        .all(|(n, rect)| n == edge.src || n == edge.dst || !hull.intersects(rect.expand(clearance)))
}

/// Cross-axis socket anchor offsets relative to the node's centre.
fn resolve_anchors(sockets: &LayoutSockets, cross_len: f32, socket_padding: f32) -> Vec<f32> {
    let padding = if socket_padding.is_finite() {
        socket_padding
    } else {
        0.0
    };
    let center = cross_len * 0.5;
    match sockets {
        LayoutSockets::EvenlySpaced(count) => {
            crate::socket::layout::evenly_spaced_cross_offsets(*count, cross_len, padding)
                .map(|offset| offset - center)
                .collect()
        }
        LayoutSockets::Explicit(offsets) => offsets
            .iter()
            .map(|&offset| {
                if offset.is_finite() {
                    offset - center
                } else {
                    0.0
                }
            })
            .collect(),
    }
}

fn sanitize_size(size: egui::Vec2) -> egui::Vec2 {
    let clean = |v: f32| if v.is_finite() { v.max(0.0) } else { 0.0 };
    egui::Vec2::new(clean(size.x), clean(size.y))
}

/// The root of `v`, with path compression.
fn uf_find(parent: &mut [usize], v: usize) -> usize {
    let mut root = v;
    while parent[root] != root {
        root = parent[root];
    }
    let mut cur = v;
    while parent[cur] != root {
        let next = parent[cur];
        parent[cur] = root;
        cur = next;
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nid(v: u64) -> NodeId {
        NodeId::from_u64(v)
    }

    /// The centre of the node's rect within `l`.
    fn center(l: &Layout, id: u64, size: egui::Vec2) -> egui::Pos2 {
        l[&nid(id)] + size * 0.5
    }

    fn simple_nodes(count: u64, size: [f32; 2]) -> Vec<(NodeId, LayoutNode)> {
        (0..count)
            .map(|v| (nid(v), LayoutNode::new(size).inputs(1).outputs(1)))
            .collect()
    }

    #[test]
    fn empty_graph() {
        let l = layout(Vec::new(), Vec::new(), egui::Direction::LeftToRight);
        assert!(l.is_empty());
    }

    #[test]
    fn single_node_centred_at_origin() {
        let nodes = vec![(nid(0), LayoutNode::new([100.0, 50.0]))];
        let l = layout(nodes, Vec::new(), egui::Direction::LeftToRight);
        assert_eq!(l[&nid(0)], egui::Pos2::new(-50.0, -25.0));
    }

    #[test]
    fn degenerate_edges_are_total() {
        let nodes = simple_nodes(2, [100.0, 50.0]);
        let edges = vec![
            ((nid(0), 0), (nid(1), 0)),
            ((nid(0), 0), (nid(1), 0)), // multi-edge
            ((nid(0), 0), (nid(0), 0)), // self-loop
            ((nid(7), 0), (nid(1), 0)), // unknown node
            ((nid(0), 9), (nid(1), 9)), // out-of-range sockets
        ];
        let l = layout(nodes, edges, egui::Direction::LeftToRight);
        assert_eq!(l.len(), 2);
        assert!(l.values().all(|p| p.x.is_finite() && p.y.is_finite()));
    }

    #[test]
    fn unknown_node_edges_are_ignored() {
        let with_unknown = layout(
            simple_nodes(2, [100.0, 50.0]),
            vec![
                ((nid(0), 0), (nid(1), 0)),
                ((nid(9), 0), (nid(1), 0)),
                ((nid(0), 0), (nid(9), 0)),
            ],
            egui::Direction::LeftToRight,
        );
        let without = layout(
            simple_nodes(2, [100.0, 50.0]),
            vec![((nid(0), 0), (nid(1), 0))],
            egui::Direction::LeftToRight,
        );
        assert_eq!(with_unknown, without);
    }

    #[test]
    fn deterministic() {
        let nodes = || simple_nodes(5, [100.0, 50.0]);
        let edges = || {
            vec![
                ((nid(0), 0), (nid(1), 0)),
                ((nid(0), 0), (nid(2), 0)),
                ((nid(1), 0), (nid(3), 0)),
                ((nid(2), 0), (nid(3), 0)),
                ((nid(3), 0), (nid(4), 0)),
            ]
        };
        let a = layout(nodes(), edges(), egui::Direction::LeftToRight);
        let b = layout(nodes(), edges(), egui::Direction::LeftToRight);
        assert_eq!(a, b);
        // Nor should the caller's edge order matter.
        let mut rev = edges();
        rev.reverse();
        let c = layout(nodes(), rev, egui::Direction::LeftToRight);
        assert_eq!(a, c);
    }

    #[test]
    fn flow_directions_mirror_and_transpose() {
        let size = egui::Vec2::new(80.0, 80.0);
        let nodes = || simple_nodes(4, [80.0, 80.0]);
        let edges = || {
            vec![
                ((nid(0), 0), (nid(1), 0)),
                ((nid(0), 0), (nid(2), 0)),
                ((nid(1), 0), (nid(3), 0)),
                ((nid(2), 0), (nid(3), 0)),
            ]
        };
        let ltr = layout(nodes(), edges(), egui::Direction::LeftToRight);
        let rtl = layout(nodes(), edges(), egui::Direction::RightToLeft);
        let td = layout(nodes(), edges(), egui::Direction::TopDown);
        for v in 0..4 {
            let c_ltr = center(&ltr, v, size);
            let c_rtl = center(&rtl, v, size);
            assert!((c_rtl.x + c_ltr.x).abs() < 1e-3);
            assert!((c_rtl.y - c_ltr.y).abs() < 1e-3);
            // Square nodes: top-down is the transpose of left-to-right.
            let c_td = center(&td, v, size);
            assert!((c_td.x - c_ltr.y).abs() < 1e-3);
            assert!((c_td.y - c_ltr.x).abs() < 1e-3);
        }
    }

    #[test]
    fn bounding_box_centred_on_origin() {
        let size = egui::Vec2::new(100.0, 50.0);
        let l = layout(
            simple_nodes(4, [100.0, 50.0]),
            vec![
                ((nid(0), 0), (nid(1), 0)),
                ((nid(1), 0), (nid(2), 0)),
                // `3` is a disconnected component.
            ],
            egui::Direction::LeftToRight,
        );
        let mut min = egui::Pos2::new(f32::INFINITY, f32::INFINITY);
        let mut max = egui::Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for &tl in l.values() {
            min = min.min(tl);
            max = max.max(tl + size);
        }
        assert!((min.x + max.x).abs() < 1e-3);
        assert!((min.y + max.y).abs() < 1e-3);
    }

    #[test]
    fn nodes_never_overlap() {
        let size = egui::Vec2::new(100.0, 50.0);
        // Two components: a diamond with a feedback edge, and a lone pair.
        let l = layout(
            simple_nodes(7, [100.0, 50.0]),
            vec![
                ((nid(0), 0), (nid(1), 0)),
                ((nid(0), 0), (nid(2), 0)),
                ((nid(1), 0), (nid(3), 0)),
                ((nid(2), 0), (nid(3), 0)),
                ((nid(3), 0), (nid(0), 0)), // cycle
                ((nid(0), 0), (nid(3), 0)), // long edge
                ((nid(4), 0), (nid(5), 0)),
                // `6` is isolated.
            ],
            egui::Direction::LeftToRight,
        );
        let rects: Vec<egui::Rect> = l
            .values()
            .map(|&tl| egui::Rect::from_min_size(tl, size))
            .collect();
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                assert!(!a.intersects(b.shrink(1.0)), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn straight_chain_has_aligned_sockets() {
        // 1-in/1-out nodes of differing heights: socket anchors (not centres)
        // should align so the edges run straight.
        let sizes = [
            egui::Vec2::new(100.0, 40.0),
            egui::Vec2::new(100.0, 80.0),
            egui::Vec2::new(100.0, 120.0),
        ];
        let padding = 10.0;
        let nodes: Vec<_> = sizes
            .iter()
            .enumerate()
            .map(|(v, &size)| {
                let node = LayoutNode::new(size)
                    .socket_padding(padding)
                    .inputs(1)
                    .outputs(1);
                (nid(v as u64), node)
            })
            .collect();
        let edges = vec![((nid(0), 0), (nid(1), 0)), ((nid(1), 0), (nid(2), 0))];
        let l = layout(nodes, edges, egui::Direction::LeftToRight);
        // A lone socket sits `padding` below the node's top edge.
        let socket_y = |v: u64| l[&nid(v)].y + padding;
        assert!((socket_y(0) - socket_y(1)).abs() < 1e-3);
        assert!((socket_y(1) - socket_y(2)).abs() < 1e-3);
    }

    #[test]
    fn duplicate_ids_last_wins() {
        let nodes = vec![
            (nid(0), LayoutNode::new([10.0, 10.0])),
            (nid(0), LayoutNode::new([100.0, 50.0])),
        ];
        let l = layout(nodes, Vec::new(), egui::Direction::LeftToRight);
        assert_eq!(l[&nid(0)], egui::Pos2::new(-50.0, -25.0));
    }

    #[test]
    fn short_edges_have_no_routes() {
        let (_, routes) = layout_routed(
            simple_nodes(3, [100.0, 50.0]),
            vec![((nid(0), 0), (nid(1), 0)), ((nid(1), 0), (nid(2), 0))],
            egui::Direction::LeftToRight,
        );
        assert!(routes.is_empty());
    }

    #[test]
    fn long_edge_threads_reserved_corridor() {
        // A -> B -> C chain plus a long A -> C edge whose direct curve would
        // pass through the tall B.
        let sizes = [[100.0, 100.0], [100.0, 300.0], [100.0, 100.0]];
        let nodes = vec![
            (nid(0), LayoutNode::new(sizes[0]).inputs(1).outputs(2)),
            (nid(1), LayoutNode::new(sizes[1]).inputs(1).outputs(1)),
            (nid(2), LayoutNode::new(sizes[2]).inputs(2).outputs(1)),
        ];
        let edges = vec![
            ((nid(0), 0), (nid(1), 0)),
            ((nid(1), 0), (nid(2), 0)),
            ((nid(0), 1), (nid(2), 1)),
        ];
        let (l, routes) = layout_routed(nodes, edges, egui::Direction::LeftToRight);
        assert_eq!(routes.len(), 1);
        let route = routes.route((nid(0), 1), (nid(2), 1), 0).expect("a route");
        assert_eq!(route.len(), 1);
        // The waypoint sits in B's layer, clear of every node.
        for (v, &size) in sizes.iter().enumerate() {
            let rect = egui::Rect::from_min_size(l[&nid(v as u64)], size.into());
            assert!(!rect.contains(route[0]), "waypoint inside node {v}");
        }
        assert!(route[0].x > l[&nid(0)].x + sizes[0][0]);
        assert!(route[0].x < l[&nid(2)].x);
    }

    #[test]
    fn clear_long_edge_keeps_direct_curve() {
        // The chain hangs from the top sockets of the tall A and C, leaving
        // the direct A -> C curve along their bottom sockets well clear of
        // the tiny B.
        let nodes = vec![
            (
                nid(0),
                LayoutNode::new([100.0, 400.0])
                    .inputs(0)
                    .output_offsets(vec![10.0, 390.0]),
            ),
            (nid(1), LayoutNode::new([100.0, 20.0]).inputs(1).outputs(1)),
            (
                nid(2),
                LayoutNode::new([100.0, 400.0])
                    .input_offsets(vec![10.0, 390.0])
                    .outputs(0),
            ),
        ];
        let edges = vec![
            ((nid(0), 0), (nid(1), 0)),
            ((nid(1), 0), (nid(2), 0)),
            ((nid(0), 1), (nid(2), 1)),
        ];
        let (_, routes) = layout_routed(nodes, edges, egui::Direction::LeftToRight);
        assert!(routes.is_empty());
    }

    #[test]
    fn feedback_route_runs_output_to_input() {
        // 0 -> 1 -> 2 -> 3 with a feedback edge 3 -> 0 spanning three layers.
        let nodes = simple_nodes(4, [100.0, 50.0]);
        let edges = vec![
            ((nid(0), 0), (nid(1), 0)),
            ((nid(1), 0), (nid(2), 0)),
            ((nid(2), 0), (nid(3), 0)),
            ((nid(3), 0), (nid(0), 0)),
        ];
        let (_, routes) = layout_routed(nodes, edges, egui::Direction::LeftToRight);
        let route = routes.route((nid(3), 0), (nid(0), 0), 0).expect("a route");
        assert_eq!(route.len(), 2);
        // Waypoints run from `3`'s output back toward `0`'s input.
        assert!(route[0].x > route[1].x);
    }

    #[test]
    fn multi_edges_route_per_occurrence() {
        let nodes = vec![
            (nid(0), LayoutNode::new([100.0, 100.0]).inputs(1).outputs(2)),
            (nid(1), LayoutNode::new([100.0, 300.0]).inputs(1).outputs(1)),
            (nid(2), LayoutNode::new([100.0, 100.0]).inputs(2).outputs(1)),
        ];
        // Tripled chain edges pin the socket alignment to B so the duplicated
        // long edges stay blocked by it (a lone duplicate pair would win the
        // placement median, align with its corridor, and need no route).
        let edges = vec![
            ((nid(0), 0), (nid(1), 0)),
            ((nid(0), 0), (nid(1), 0)),
            ((nid(0), 0), (nid(1), 0)),
            ((nid(1), 0), (nid(2), 0)),
            ((nid(1), 0), (nid(2), 0)),
            ((nid(1), 0), (nid(2), 0)),
            ((nid(0), 1), (nid(2), 1)),
            ((nid(0), 1), (nid(2), 1)),
        ];
        let (_, routes) = layout_routed(nodes, edges, egui::Direction::LeftToRight);
        assert_eq!(routes.len(), 2);
        assert!(routes.route((nid(0), 1), (nid(2), 1), 0).is_some());
        assert!(routes.route((nid(0), 1), (nid(2), 1), 1).is_some());
        assert!(routes.route((nid(0), 1), (nid(2), 1), 2).is_none());
    }

    #[test]
    fn routes_are_deterministic() {
        let nodes = || {
            vec![
                (nid(0), LayoutNode::new([100.0, 100.0]).inputs(1).outputs(2)),
                (nid(1), LayoutNode::new([100.0, 300.0]).inputs(1).outputs(1)),
                (nid(2), LayoutNode::new([100.0, 100.0]).inputs(2).outputs(1)),
            ]
        };
        let edges = || {
            vec![
                ((nid(0), 0), (nid(1), 0)),
                ((nid(1), 0), (nid(2), 0)),
                ((nid(0), 1), (nid(2), 1)),
            ]
        };
        let a = layout_routed(nodes(), edges(), egui::Direction::TopDown);
        let b = layout_routed(nodes(), edges(), egui::Direction::TopDown);
        assert_eq!(a, b);
    }

    #[test]
    fn socket_blind_matches_collapsed_input() {
        // With `socket_aware(false)`, socketed input lays out identically to
        // the same graph stripped of socket information.
        let socketed = || {
            vec![
                (
                    nid(0),
                    LayoutNode::new([100.0, 100.0])
                        .socket_padding(10.0)
                        .inputs(1)
                        .outputs(3),
                ),
                (nid(1), LayoutNode::new([100.0, 300.0]).inputs(1).outputs(1)),
                (nid(2), LayoutNode::new([100.0, 100.0]).inputs(2).outputs(1)),
            ]
        };
        let edges = vec![
            ((nid(0), 2), (nid(1), 0)),
            ((nid(1), 0), (nid(2), 1)),
            ((nid(0), 0), (nid(2), 0)),
        ];
        let blind = layout(
            socketed(),
            edges.clone(),
            LayoutParams::new(egui::Direction::TopDown).socket_aware(false),
        );
        let stripped = socketed()
            .into_iter()
            .map(|(id, node)| (id, LayoutNode::new(node.size)));
        let collapsed = edges.iter().map(|&((a, _), (b, _))| ((a, 0), (b, 0)));
        let plain = layout(stripped, collapsed, egui::Direction::TopDown);
        assert_eq!(blind, plain);
    }

    #[test]
    fn socket_blind_routes_keep_real_socket_keys() {
        let nodes = vec![
            (nid(0), LayoutNode::new([100.0, 100.0]).inputs(1).outputs(2)),
            (nid(1), LayoutNode::new([100.0, 300.0]).inputs(1).outputs(1)),
            (nid(2), LayoutNode::new([100.0, 100.0]).inputs(2).outputs(1)),
        ];
        let edges = vec![
            ((nid(0), 0), (nid(1), 0)),
            ((nid(1), 0), (nid(2), 0)),
            ((nid(0), 1), (nid(2), 1)),
        ];
        let (_, routes) = layout_routed(
            nodes,
            edges,
            LayoutParams::new(egui::Direction::LeftToRight).socket_aware(false),
        );
        // The long edge still routes around the tall `1`, keyed by its real
        // socket indices rather than the collapsed ones fed to the pipeline.
        assert!(routes.route((nid(0), 1), (nid(2), 1), 0).is_some());
    }

    /// A 1-in/1-out node with an explicit flow.
    fn flow_node(v: u64, flow: egui::Direction) -> (NodeId, LayoutNode) {
        (
            nid(v),
            LayoutNode::new([100.0, 50.0])
                .inputs(1)
                .outputs(1)
                .flow(flow),
        )
    }

    /// Two perpendicular chains joined by one cross-flow edge: `0->1->2` flows
    /// top-down, `3->4->5` flows left-to-right, and `2->3` crosses between
    /// them.
    #[allow(clippy::type_complexity)]
    fn perpendicular_chains() -> (
        Vec<(NodeId, LayoutNode)>,
        Vec<((NodeId, usize), (NodeId, usize))>,
    ) {
        use egui::Direction::{LeftToRight, TopDown};
        let nodes = vec![
            flow_node(0, TopDown),
            flow_node(1, TopDown),
            flow_node(2, TopDown),
            flow_node(3, LeftToRight),
            flow_node(4, LeftToRight),
            flow_node(5, LeftToRight),
        ];
        let edges = vec![
            ((nid(0), 0), (nid(1), 0)),
            ((nid(1), 0), (nid(2), 0)),
            ((nid(3), 0), (nid(4), 0)),
            ((nid(4), 0), (nid(5), 0)),
            ((nid(2), 0), (nid(3), 0)), // cross-flow cut edge
        ];
        (nodes, edges)
    }

    #[test]
    fn per_node_flow_none_matches_global() {
        let edges = || {
            vec![
                ((nid(0), 0), (nid(1), 0)),
                ((nid(0), 0), (nid(2), 0)),
                ((nid(1), 0), (nid(3), 0)),
                ((nid(2), 0), (nid(3), 0)),
                ((nid(3), 0), (nid(4), 0)),
            ]
        };
        // Every node defaulting to the params' flow...
        let implicit = layout(
            simple_nodes(5, [100.0, 50.0]),
            edges(),
            egui::Direction::TopDown,
        );
        // ...equals every node overriding to that same flow, regardless of the
        // params' flow (which is then only a fallback nobody falls back to).
        let explicit: Vec<_> = (0..5)
            .map(|v| flow_node(v, egui::Direction::TopDown))
            .collect();
        let explicit = layout(explicit, edges(), egui::Direction::LeftToRight);
        assert_eq!(implicit, explicit);
    }

    #[test]
    fn mixed_flow_lays_each_chain_in_its_own_flow() {
        let size = egui::Vec2::new(100.0, 50.0);
        let (nodes, edges) = perpendicular_chains();
        let l = layout(nodes, edges, egui::Direction::TopDown);
        // The top-down chain stacks vertically: shared cross (x), growing y.
        let (c0, c1, c2) = (
            center(&l, 0, size),
            center(&l, 1, size),
            center(&l, 2, size),
        );
        assert!((c0.x - c1.x).abs() < 1e-3, "{c0:?} {c1:?}");
        assert!((c1.x - c2.x).abs() < 1e-3, "{c1:?} {c2:?}");
        assert!(c0.y < c1.y && c1.y < c2.y, "{c0:?} {c1:?} {c2:?}");
        // The left-to-right chain runs horizontally: shared cross (y), growing x.
        let (c3, c4, c5) = (
            center(&l, 3, size),
            center(&l, 4, size),
            center(&l, 5, size),
        );
        assert!((c3.y - c4.y).abs() < 1e-3, "{c3:?} {c4:?}");
        assert!((c4.y - c5.y).abs() < 1e-3, "{c4:?} {c5:?}");
        assert!(c3.x < c4.x && c4.x < c5.x, "{c3:?} {c4:?} {c5:?}");
    }

    #[test]
    fn mixed_flow_no_overlap() {
        let size = egui::Vec2::new(100.0, 50.0);
        let (nodes, edges) = perpendicular_chains();
        let l = layout(nodes, edges, egui::Direction::TopDown);
        let rects: Vec<egui::Rect> = (0..6)
            .map(|v| egui::Rect::from_min_size(l[&nid(v)], size))
            .collect();
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                assert!(!a.intersects(b.shrink(1.0)), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn mixed_flow_clusters_follow_outer_flow() {
        let size = egui::Vec2::new(100.0, 50.0);
        let (nodes, edges) = perpendicular_chains();
        // Outer flow left-to-right: the cut edge `2->3` should place the first
        // chain's cluster entirely left of the second's.
        let l = layout(nodes, edges, egui::Direction::LeftToRight);
        let a_max_x = (0..3)
            .map(|v| l[&nid(v)].x + size.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let b_min_x = (3..6).map(|v| l[&nid(v)].x).fold(f32::INFINITY, f32::min);
        assert!(a_max_x <= b_min_x + 1e-3, "{a_max_x} !<= {b_min_x}");
    }

    #[test]
    fn mixed_flow_deterministic() {
        let l = || {
            let (nodes, edges) = perpendicular_chains();
            layout_routed(nodes, edges, egui::Direction::TopDown)
        };
        assert_eq!(l(), l());
        // The caller's edge order must not matter either.
        let (nodes, mut edges) = perpendicular_chains();
        edges.reverse();
        let reversed = layout_routed(nodes, edges, egui::Direction::TopDown);
        assert_eq!(l(), reversed);
    }

    #[test]
    fn cut_edges_are_unrouted() {
        use egui::Direction::{LeftToRight, TopDown};
        // A top-down cluster whose long edge must route around its wide middle
        // node, plus a left-to-right node joined by a cross-flow cut edge.
        let nodes = vec![
            (
                nid(0),
                LayoutNode::new([100.0, 100.0])
                    .inputs(1)
                    .outputs(2)
                    .flow(TopDown),
            ),
            (
                nid(1),
                LayoutNode::new([300.0, 100.0])
                    .inputs(1)
                    .outputs(1)
                    .flow(TopDown),
            ),
            (
                nid(2),
                LayoutNode::new([100.0, 100.0])
                    .inputs(2)
                    .outputs(1)
                    .flow(TopDown),
            ),
            (nid(3), flow_node(3, LeftToRight).1),
        ];
        let edges = vec![
            ((nid(0), 0), (nid(1), 0)),
            ((nid(1), 0), (nid(2), 0)),
            ((nid(0), 1), (nid(2), 1)), // long intra-cluster edge over wide `1`
            ((nid(2), 0), (nid(3), 0)), // cross-flow cut edge
        ];
        let (_, routes) = layout_routed(nodes, edges, TopDown);
        // The long intra-cluster edge reserves a corridor...
        assert!(routes.route((nid(0), 1), (nid(2), 1), 0).is_some());
        // ...while the cut edge between clusters is left as a plain curve.
        assert!(routes.route((nid(2), 0), (nid(3), 0), 0).is_none());
    }

    /// Minimal xorshift PRNG, avoiding a dev-dependency.
    struct XorShift(u64);

    impl XorShift {
        fn range(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n as u64) as usize
        }
    }

    /// A random graph of up to 40 nodes, including self-loops, multi-edges
    /// and cycles.
    #[allow(clippy::type_complexity)]
    fn random_graph(
        seed: u64,
    ) -> (
        Vec<(NodeId, egui::Vec2)>,
        Vec<((NodeId, usize), (NodeId, usize))>,
    ) {
        let mut rng = XorShift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let num_nodes = 1 + rng.range(40);
        let nodes = (0..num_nodes)
            .map(|v| {
                let size =
                    egui::Vec2::new(40.0 + rng.range(160) as f32, 30.0 + rng.range(120) as f32);
                (nid(v as u64), size)
            })
            .collect();
        let edges = (0..rng.range(2 * num_nodes + 1))
            .map(|_| {
                let a = nid(rng.range(num_nodes) as u64);
                let b = nid(rng.range(num_nodes) as u64);
                ((a, rng.range(4)), (b, rng.range(4)))
            })
            .collect();
        (nodes, edges)
    }

    fn layout_random(seed: u64, flow: egui::Direction) -> Layout {
        let (nodes, edges) = random_graph(seed);
        let nodes = nodes.into_iter().map(|(id, size)| {
            let node = LayoutNode::new(size)
                .socket_padding(8.0)
                .inputs(4)
                .outputs(4);
            (id, node)
        });
        layout(nodes, edges, flow)
    }

    #[test]
    fn random_graphs_are_total_and_deterministic() {
        for seed in 0..50 {
            let (nodes, _) = random_graph(seed);
            let l = layout_random(seed, egui::Direction::LeftToRight);
            assert_eq!(l.len(), nodes.len());
            assert!(l.values().all(|p| p.x.is_finite() && p.y.is_finite()));
            assert_eq!(l, layout_random(seed, egui::Direction::LeftToRight));
        }
    }

    #[test]
    fn random_graphs_never_overlap_nodes() {
        for seed in 0..50 {
            let (nodes, _) = random_graph(seed);
            let l = layout_random(seed, egui::Direction::TopDown);
            let rects: Vec<egui::Rect> = nodes
                .iter()
                .map(|&(id, size)| egui::Rect::from_min_size(l[&id], size))
                .collect();
            for (i, a) in rects.iter().enumerate() {
                for b in &rects[i + 1..] {
                    assert!(
                        !a.intersects(b.shrink(1.0)),
                        "seed {seed}: {a:?} overlaps {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn random_graphs_mirror_between_opposite_flows() {
        for seed in 0..20 {
            let (nodes, _) = random_graph(seed);
            let ltr = layout_random(seed, egui::Direction::LeftToRight);
            let rtl = layout_random(seed, egui::Direction::RightToLeft);
            for &(id, size) in &nodes {
                let c_ltr = ltr[&id] + size * 0.5;
                let c_rtl = rtl[&id] + size * 0.5;
                assert!((c_rtl.x + c_ltr.x).abs() < 1e-2, "seed {seed}");
                assert!((c_rtl.y - c_ltr.y).abs() < 1e-2, "seed {seed}");
            }
        }
    }

    /// As [`layout_random`], but gives every node one of the four flow
    /// directions, so the graph splits into many clusters joined by cross-flow
    /// cut edges.
    fn layout_random_mixed(seed: u64) -> Layout {
        let (nodes, edges) = random_graph(seed);
        let mut rng = XorShift(seed.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1);
        let flows = [
            egui::Direction::LeftToRight,
            egui::Direction::RightToLeft,
            egui::Direction::TopDown,
            egui::Direction::BottomUp,
        ];
        let nodes = nodes.into_iter().map(move |(id, size)| {
            let node = LayoutNode::new(size)
                .socket_padding(8.0)
                .inputs(4)
                .outputs(4)
                .flow(flows[rng.range(flows.len())]);
            (id, node)
        });
        layout(nodes, edges, egui::Direction::TopDown)
    }

    #[test]
    fn mixed_random_graphs_are_total_and_deterministic() {
        for seed in 0..50 {
            let (nodes, _) = random_graph(seed);
            let l = layout_random_mixed(seed);
            assert_eq!(l.len(), nodes.len());
            assert!(l.values().all(|p| p.x.is_finite() && p.y.is_finite()));
            assert_eq!(l, layout_random_mixed(seed));
        }
    }

    #[test]
    fn mixed_random_graphs_never_overlap_nodes() {
        for seed in 0..50 {
            let (nodes, _) = random_graph(seed);
            let l = layout_random_mixed(seed);
            let rects: Vec<egui::Rect> = nodes
                .iter()
                .map(|&(id, size)| egui::Rect::from_min_size(l[&id], size))
                .collect();
            for (i, a) in rects.iter().enumerate() {
                for b in &rects[i + 1..] {
                    assert!(
                        !a.intersects(b.shrink(1.0)),
                        "seed {seed}: {a:?} overlaps {b:?}"
                    );
                }
            }
        }
    }

    /// The number of edges whose sockets line up across the flow, in a
    /// top-down layout of nodes with `sockets` evenly spaced sockets a side.
    fn straight_edges(
        nodes: &[(NodeId, egui::Vec2)],
        edges: &[((NodeId, usize), (NodeId, usize))],
        sockets: usize,
        padding: f32,
        l: &Layout,
    ) -> usize {
        let size: HashMap<NodeId, egui::Vec2> = nodes.iter().copied().collect();
        let socket_x = |id: NodeId, ix: usize| {
            let mut offsets =
                crate::socket::layout::evenly_spaced_cross_offsets(sockets, size[&id].x, padding);
            l[&id].x + offsets.nth(ix).unwrap_or(size[&id].x * 0.5)
        };
        edges
            .iter()
            .filter(|&&((a, out), (b, inp))| {
                a != b && (socket_x(a, out) - socket_x(b, inp)).abs() < 0.5
            })
            .count()
    }

    #[test]
    fn diamond_runs_straight_through_one_side() {
        // A bang fanning out to two nodes that feed one node's two inputs.
        // The bang, the left node and the join line up their first sockets,
        // which all sit at the padding.
        let size = egui::Vec2::new(100.0, 30.0);
        let nodes = (0..4).map(|v| {
            let inputs = if v == 3 { 2 } else { 1 };
            let node = LayoutNode::new(size)
                .socket_padding(10.0)
                .inputs(inputs)
                .outputs(1);
            (nid(v), node)
        });
        let edges = vec![
            ((nid(0), 0), (nid(1), 0)),
            ((nid(0), 0), (nid(2), 0)),
            ((nid(1), 0), (nid(3), 0)),
            ((nid(2), 0), (nid(3), 1)),
        ];
        let params = LayoutParams::new(egui::Direction::TopDown).straighten_edges(true);
        let l = layout(nodes, edges, params);
        let column = l[&nid(1)].x;
        assert!((l[&nid(0)].x - column).abs() < 1e-3);
        assert!((l[&nid(3)].x - column).abs() < 1e-3);
    }

    #[test]
    fn straightening_lines_up_more_sockets() {
        let count = |straighten| {
            (0..50)
                .map(|seed| {
                    let (nodes, edges) = random_graph(seed);
                    let layout_nodes = nodes.iter().map(|&(id, size)| {
                        let node = LayoutNode::new(size)
                            .socket_padding(8.0)
                            .inputs(4)
                            .outputs(4);
                        (id, node)
                    });
                    let params =
                        LayoutParams::new(egui::Direction::TopDown).straighten_edges(straighten);
                    let l = layout(layout_nodes, edges.clone(), params);
                    straight_edges(&nodes, &edges, 4, 8.0, &l)
                })
                .sum::<usize>()
        };
        let (before, after) = (count(false), count(true));
        assert!(after > before, "{after} straight edges, {before} without");
    }
}
