//! Best-effort edge routing for manually arranged graphs.

use super::{anchor, anchored_socket_pos, resolve_anchors, sanitize_size};
use super::{CEdge, EdgeRoutes, LayoutNode, LayoutParams};
use crate::edge::{InputIx, OutputIx};
use crate::NodeId;
use std::collections::{BTreeMap, HashMap};

/// Bound on recursive detours per edge; deeper blockages give up rather than
/// zig-zag indefinitely.
const MAX_DEPTH: usize = 6;

/// Compute corridor waypoints for edges against nodes at their *current*
/// positions, without performing any layout.
///
/// A greedy, best-effort router for manually arranged graphs: each edge's
/// direct socket-to-socket line is recursively detoured around the nearest
/// node it would cross. Unlike [`layout_routed`](super::layout_routed), no
/// space is reserved for the detours, so quality degrades in dense
/// arrangements - a detour clears the node it dodges, but may pass close to
/// others.
///
/// `nodes` pairs each node's current top-left position with the same socket
/// description [`layout`](super::layout) takes; `params` provides the flow
/// direction and `node_gap` (used as the detour clearance). Edges referencing
/// unknown nodes and self-loops are ignored, as in [`layout`](super::layout).
///
/// Routing assumes a single flow: every node and edge is dodged along
/// `params.flow`, so per-node [`LayoutNode::flow`](super::LayoutNode::flow)
/// overrides are ignored here. Mixed-flow graphs are best laid out and routed
/// by [`layout_routed`](super::layout_routed).
pub fn route_edges(
    nodes: impl IntoIterator<Item = (NodeId, egui::Pos2, LayoutNode)>,
    edges: impl IntoIterator<Item = ((NodeId, OutputIx), (NodeId, InputIx))>,
    params: impl Into<LayoutParams>,
) -> EdgeRoutes {
    let params = params.into();
    let horizontal = matches!(
        params.flow,
        egui::Direction::LeftToRight | egui::Direction::RightToLeft
    );

    // Canonicalise the nodes in a deterministic order.
    let nodes: BTreeMap<NodeId, (egui::Pos2, LayoutNode)> = nodes
        .into_iter()
        .map(|(id, pos, node)| (id, (pos, node)))
        .collect();
    let ids: Vec<NodeId> = nodes.keys().copied().collect();
    let index_of: HashMap<NodeId, usize> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut rects = Vec::with_capacity(ids.len());
    let mut in_anchors = Vec::with_capacity(ids.len());
    let mut out_anchors = Vec::with_capacity(ids.len());
    for (pos, node) in nodes.values() {
        let size = sanitize_size(node.size);
        let pos = sanitize_pos(*pos);
        let cross = if horizontal { size.y } else { size.x };
        rects.push(egui::Rect::from_min_size(pos, size));
        in_anchors.push(resolve_anchors(&node.inputs, cross, node.socket_padding));
        out_anchors.push(resolve_anchors(&node.outputs, cross, node.socket_padding));
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

    let clearance = params.node_gap * 0.5;
    let mut routes = EdgeRoutes::default();
    for edge in &cedges {
        let a_anchor = anchor(&out_anchors[edge.src], edge.src_socket);
        let b_anchor = anchor(&in_anchors[edge.dst], edge.dst_socket);
        let a = anchored_socket_pos(params.flow, rects[edge.src], false, a_anchor);
        let b = anchored_socket_pos(params.flow, rects[edge.dst], true, b_anchor);
        let mut waypoints = Vec::new();
        dodge(
            &rects,
            (edge.src, edge.dst),
            horizontal,
            clearance,
            a,
            b,
            MAX_DEPTH,
            &mut waypoints,
        );
        if waypoints.is_empty() {
            continue;
        }
        let key = (
            (ids[edge.src], edge.src_socket),
            (ids[edge.dst], edge.dst_socket),
        );
        routes.routes.entry(key).or_default().push(waypoints);
    }
    routes
}

fn sanitize_pos(pos: egui::Pos2) -> egui::Pos2 {
    let clean = |v: f32| if v.is_finite() { v } else { 0.0 };
    egui::Pos2::new(clean(pos.x), clean(pos.y))
}

/// Recursively detour the segment `p -> q` around the nearest node it
/// crosses, appending the detour waypoints in path order.
///
/// Each detour rounds the nearer cross-axis side of the blocking node at the
/// node's main-axis centre, mirroring the corridor waypoints the layered
/// layout produces. Nodes containing either endpoint cannot be dodged and
/// are skipped.
#[allow(clippy::too_many_arguments)]
fn dodge(
    rects: &[egui::Rect],
    skip: (usize, usize),
    horizontal: bool,
    clearance: f32,
    p: egui::Pos2,
    q: egui::Pos2,
    depth: usize,
    out: &mut Vec<egui::Pos2>,
) {
    if depth == 0 {
        return;
    }
    // The nearest node crossed by the segment, with clearance.
    let hit = rects
        .iter()
        .enumerate()
        .filter(|&(n, _)| n != skip.0 && n != skip.1)
        .filter_map(|(_, rect)| {
            let rect = rect.expand(clearance);
            if rect.contains(p) || rect.contains(q) {
                return None;
            }
            segment_rect_entry(p, q, rect).map(|t| (t, rect))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    let Some((_, rect)) = hit else { return };
    let wp = if horizontal {
        let y = if (p.y + q.y) * 0.5 < rect.center().y {
            rect.min.y
        } else {
            rect.max.y
        };
        egui::Pos2::new(rect.center().x, y)
    } else {
        let x = if (p.x + q.x) * 0.5 < rect.center().x {
            rect.min.x
        } else {
            rect.max.x
        };
        egui::Pos2::new(x, rect.center().y)
    };
    dodge(rects, skip, horizontal, clearance, p, wp, depth - 1, out);
    out.push(wp);
    dodge(rects, skip, horizontal, clearance, wp, q, depth - 1, out);
}

/// The parameter at which the segment `p + t * (q - p)` first enters `rect`,
/// if it does so for `t` within `0..=1`.
fn segment_rect_entry(p: egui::Pos2, q: egui::Pos2, rect: egui::Rect) -> Option<f32> {
    let d = q - p;
    let mut t_min = 0.0f32;
    let mut t_max = 1.0f32;
    for (p0, d0, min, max) in [
        (p.x, d.x, rect.min.x, rect.max.x),
        (p.y, d.y, rect.min.y, rect.max.y),
    ] {
        if d0.abs() < f32::EPSILON {
            if p0 < min || p0 > max {
                return None;
            }
        } else {
            let (t0, t1) = ((min - p0) / d0, (max - p0) / d0);
            let (t0, t1) = (t0.min(t1), t0.max(t1));
            t_min = t_min.max(t0);
            t_max = t_max.min(t1);
            if t_min > t_max {
                return None;
            }
        }
    }
    Some(t_min)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nid(v: u64) -> NodeId {
        NodeId::from_u64(v)
    }

    /// A 1-out node at `pos` with its socket at the cross-axis centre.
    fn out_node(pos: [f32; 2]) -> (egui::Pos2, LayoutNode) {
        (
            pos.into(),
            LayoutNode::new([100.0, 50.0]).output_offsets(vec![25.0]),
        )
    }

    /// A 1-in node at `pos` with its socket at the cross-axis centre.
    fn in_node(pos: [f32; 2]) -> (egui::Pos2, LayoutNode) {
        (
            pos.into(),
            LayoutNode::new([100.0, 50.0]).input_offsets(vec![25.0]),
        )
    }

    fn graph(
        blocker_pos: [f32; 2],
    ) -> (
        Vec<(NodeId, egui::Pos2, LayoutNode)>,
        Vec<((NodeId, usize), (NodeId, usize))>,
    ) {
        let (a_pos, a) = out_node([0.0, 0.0]);
        let (c_pos, c) = in_node([300.0, 0.0]);
        let nodes = vec![
            (nid(0), a_pos, a),
            (nid(1), blocker_pos.into(), LayoutNode::new([100.0, 100.0])),
            (nid(2), c_pos, c),
        ];
        let edges = vec![((nid(0), 0), (nid(2), 0))];
        (nodes, edges)
    }

    #[test]
    fn blocked_edge_dodges_around_node() {
        // The blocker straddles the straight line between the sockets.
        let (nodes, edges) = graph([150.0, -25.0]);
        let routes = route_edges(nodes, edges, egui::Direction::LeftToRight);
        let route = routes.route((nid(0), 0), (nid(2), 0), 0).expect("a route");
        let blocker = egui::Rect::from_min_size([150.0, -25.0].into(), [100.0, 100.0].into());
        assert!(!route.is_empty());
        for wp in route {
            assert!(!blocker.contains(*wp), "waypoint inside the blocker");
        }
    }

    #[test]
    fn clear_edge_has_no_route() {
        // The blocker sits well away from the straight line.
        let (nodes, edges) = graph([150.0, 200.0]);
        let routes = route_edges(nodes, edges, egui::Direction::LeftToRight);
        assert!(routes.is_empty());
    }

    #[test]
    fn degenerate_edges_are_total() {
        let (nodes, _) = graph([150.0, -25.0]);
        let edges = vec![
            ((nid(0), 0), (nid(0), 0)), // self-loop
            ((nid(9), 0), (nid(2), 0)), // unknown node
        ];
        let routes = route_edges(nodes, edges, egui::Direction::LeftToRight);
        assert!(routes.is_empty());
    }

    #[test]
    fn deterministic() {
        let route = || {
            let (nodes, edges) = graph([150.0, -25.0]);
            route_edges(nodes, edges, egui::Direction::LeftToRight)
        };
        assert_eq!(route(), route());
    }
}
