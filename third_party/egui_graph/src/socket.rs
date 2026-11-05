pub mod layout;

use crate::node::NodeId;

/// Describes either an input or output.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SocketKind {
    Input,
    Output,
}

/// Uniquely identifies a socket.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Socket {
    /// The node that owns this socket.
    pub node: NodeId,
    /// Whether the socket is an input or output.
    pub kind: SocketKind,
    /// The index of the socket of this kind.
    pub index: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct PositionedSocket {
    pub socket: Socket,
    /// Screen-space position of the socket.
    pub pos: egui::Pos2,
    /// The normal of the edge along which this socket resides.
    pub normal: egui::Vec2,
}

/// Collected [`egui::Response`]s for all sockets on a node.
///
/// Each socket is allocated as an interactive widget (with [`egui::Sense::hover`]),
/// enabling standard egui interactions like tooltips and hover detection.
///
/// A node out of view of its graph has no socket responses.
#[derive(Default)]
pub struct SocketResponses {
    inputs: std::collections::BTreeMap<usize, egui::Response>,
    outputs: std::collections::BTreeMap<usize, egui::Response>,
}

impl SocketResponses {
    /// The response for the input socket at the given index.
    pub fn input(&self, ix: usize) -> Option<&egui::Response> {
        self.inputs.get(&ix)
    }

    /// The response for the output socket at the given index.
    pub fn output(&self, ix: usize) -> Option<&egui::Response> {
        self.outputs.get(&ix)
    }

    /// Iterator over all input socket responses, yielding `(index, response)`.
    pub fn inputs(&self) -> impl Iterator<Item = (usize, &egui::Response)> {
        self.inputs.iter().map(|(&ix, r)| (ix, r))
    }

    /// Iterator over all output socket responses, yielding `(index, response)`.
    pub fn outputs(&self) -> impl Iterator<Item = (usize, &egui::Response)> {
        self.outputs.iter().map(|(&ix, r)| (ix, r))
    }
}

/// The padding from the ends of a node's edge within which its sockets are
/// laid out, as used by [`Node`](crate::node::Node).
///
/// Useful for deriving socket positions outside of node instantiation, e.g.
/// when providing socket offsets to the automatic layout.
pub fn socket_padding(style: &egui::Style) -> f32 {
    let min_interact_len = style
        .spacing
        .interact_size
        .x
        .min(style.spacing.interact_size.y);
    style.visuals.window_corner_radius.ne as f32 + min_interact_len * 0.5
}

/// The radius of the faded semicircle behind a highlighted socket.
pub(crate) fn highlight_radius(socket_radius: f32) -> f32 {
    (socket_radius + 4.0).max(4.0)
}

/// Adaptive segment count for a semicircle, matching egui's circle tessellation
/// heuristic (halved, since a semicircle spans half the arc).
fn semicircle_segments(radius: f32) -> usize {
    if radius <= 2.0 {
        4
    } else if radius <= 5.0 {
        8
    } else if radius < 18.0 {
        16
    } else if radius < 50.0 {
        32
    } else {
        64
    }
}

/// Paint a filled semicircle facing outward along `normal`.
///
/// The flat edge is perpendicular to `normal` and passes through `center`.
/// The curved part extends outward from `center` by `radius` along `normal`.
fn paint_semicircle(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    normal: egui::Vec2,
    color: egui::Color32,
) {
    let segments = semicircle_segments(radius);
    let perp = egui::Vec2::new(-normal.y, normal.x);
    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let angle = std::f32::consts::PI * (i as f32) / (segments as f32);
        let (sin, cos) = angle.sin_cos();
        // Trace from +perp through +normal to -perp.
        pts.push(center + perp * (radius * cos) + normal * (radius * sin));
    }
    painter.add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));
}

/// Paint and interact with all sockets for a node.
///
/// Phase A: extracts highlight state (pressed/closest socket) from graph memory, then drops the lock.
/// Phase B: paints each socket as an outward-facing semicircle on the socket layer
/// (using `Shape::convex_polygon`), and calls `ui.interact()` to produce per-socket responses.
///
/// By rendering semicircles that only extend outward from the frame border, sockets avoid
/// visual overlap with node frames regardless of sublayer ordering.
#[allow(clippy::too_many_arguments)]
pub(crate) fn show(
    ui: &egui::Ui,
    graph_id: egui::Id,
    node_id: NodeId,
    egui_id: egui::Id,
    socket_layer: egui::LayerId,
    node_sockets: &crate::NodeSockets,
    socket_color: egui::Color32,
    socket_radius: f32,
) -> SocketResponses {
    // Phase A: Extract highlight state, then drop the lock.
    let (pressed_socket, closest_socket) = {
        let gmem_arc = crate::memory(ui, graph_id);
        let gmem = gmem_arc.lock().expect("failed to lock graph temp memory");

        let pressed_socket = gmem
            .pressed
            .as_ref()
            .and_then(|pressed| match pressed.action {
                crate::PressAction::Socket(socket) if socket.node == node_id => {
                    Some((socket.kind, socket.index))
                }
                _ => None,
            });

        let closest_socket = match gmem.closest_socket {
            Some(closest) if closest.node == node_id => {
                match gmem.pressed.as_ref().map(|p| &p.action) {
                    Some(crate::PressAction::Socket(socket)) if closest.kind == socket.kind => None,
                    _ => Some((closest.kind, closest.index)),
                }
            }
            _ => None,
        };

        (pressed_socket, closest_socket)
    };

    let paint_highlight =
        |kind, ix| pressed_socket == Some((kind, ix)) || closest_socket == Some((kind, ix));
    paint(
        ui,
        egui_id,
        socket_layer,
        node_sockets,
        socket_color,
        socket_radius,
        paint_highlight,
    )
}

/// Paint each socket on `socket_layer` and allocate its hover response.
///
/// The hover responses live on the layer of `ui`, not on `socket_layer`. On a
/// layer above the graph scene, a socket response covers egui's hit-test area
/// and hides the scene behind it, so egui gives a press on the socket to the
/// nearby node frame.
///
/// `highlight` names the sockets that get the larger, faded semicircle
/// behind them, such as a pressed socket or the closest one to the pointer
/// while an edge is in progress. This is the graph-free half of [`show`].
pub(crate) fn paint(
    ui: &egui::Ui,
    egui_id: egui::Id,
    socket_layer: egui::LayerId,
    node_sockets: &crate::NodeSockets,
    socket_color: egui::Color32,
    socket_radius: f32,
    highlight: impl Fn(SocketKind, usize) -> bool,
) -> SocketResponses {
    let hl_size = highlight_radius(socket_radius);
    let interact_diameter = ui
        .spacing()
        .interact_size
        .x
        .min(ui.spacing().interact_size.y);
    let painter = ui.painter().clone().with_layer_id(socket_layer);

    let socket = |kind, ix, pos, normal| {
        if highlight(kind, ix) {
            let color = socket_color.linear_multiply(0.25);
            paint_semicircle(&painter, pos, hl_size, normal, color);
        }
        paint_semicircle(&painter, pos, socket_radius, normal, socket_color);
        let id = match kind {
            SocketKind::Input => egui_id.with("in"),
            SocketKind::Output => egui_id.with("out"),
        };
        let rect = egui::Rect::from_center_size(pos, egui::Vec2::splat(interact_diameter));
        (ix, ui.interact(rect, id.with(ix), egui::Sense::hover()))
    };

    SocketResponses {
        inputs: node_sockets
            .inputs()
            .map(|(ix, pos, normal)| socket(SocketKind::Input, ix, pos, normal))
            .collect(),
        outputs: node_sockets
            .outputs()
            .map(|(ix, pos, normal)| socket(SocketKind::Output, ix, pos, normal))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Socket, SocketKind};
    use crate::node::{EdgeEvent, Node, NodeId};
    use crate::{Graph, View};

    /// A graph of nodes `A` and `B`, driven one pass at a time.
    struct TestGraph {
        ctx: egui::Context,
        view: View,
        immutable: bool,
        /// The number of inputs of `B`, or `None` to not show `B`.
        b_inputs: Option<usize>,
    }

    /// The graph state observed at the end of a pass.
    struct Pass {
        /// The screen position and normal of the output of `A`.
        a_out: (egui::Pos2, egui::Vec2),
        /// The screen position and normal of the input of `B`, as edges see it.
        b_in: Option<(egui::Pos2, egui::Vec2)>,
        closest_socket: Option<Socket>,
        /// Whether the hover response of the input of `B` is hovered.
        b_in_hovered: bool,
        a_edge_event: Option<EdgeEvent>,
        b_edge_event: Option<EdgeEvent>,
    }

    const GRAPH: &str = "graph";
    const SCREEN: egui::Rect = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(800.0, 600.0));
    /// Has one output on its right edge.
    const A: NodeId = NodeId(0);
    /// Has one input on its left edge, to the right of `A`.
    const B: NodeId = NodeId(1);
    const B_IN: Socket = Socket {
        node: B,
        kind: SocketKind::Input,
        index: 0,
    };
    /// The distance of a point from a socket along its normal, within the
    /// drawn socket.
    const NEAR: f32 = 2.0;

    /// Lay out a new graph over two passes, since sizes settle after the first.
    fn test_graph(immutable: bool) -> (TestGraph, Pass) {
        let view = View {
            scene_rect: SCREEN,
            layout: [(A, egui::pos2(100.0, 100.0)), (B, egui::pos2(400.0, 100.0))].into(),
        };
        let mut g = TestGraph {
            ctx: egui::Context::default(),
            view,
            immutable,
            b_inputs: Some(1),
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
        let b_inputs = g.b_inputs;
        let mut out = None;
        g.ctx
            .run_ui(input, |ui| {
                let graph = Graph::new(GRAPH).immutable(g.immutable);
                graph.show(&mut g.view, ui, |ui, show| {
                    let mut nodes = None;
                    let mut sockets = None;
                    show.nodes(ui, |nctx, ui| {
                        let mut node = |id, inputs, outputs| {
                            let node = Node::from_id(id).inputs(inputs).outputs(outputs);
                            node.show(nctx, ui, |c| c.framed(|ui, _| ui.label("x")))
                        };
                        nodes = Some((node(A, 0, 1), b_inputs.map(|n| node(B, n, 0))));
                    })
                    .edges(ui, |ectx, ui| {
                        sockets = Some((ectx.output(ui, A, 0), ectx.input(ui, B, 0)));
                    });
                    let (a, b) = nodes.expect("the nodes ran");
                    let (a_out, b_in) = sockets.expect("the edges ran");
                    let to_screen = ui
                        .ctx()
                        .layer_transform_to_global(ui.layer_id())
                        .unwrap_or_default();
                    let screen = |(pos, normal)| (to_screen.mul_pos(pos), normal);
                    let gmem_arc = crate::memory(ui, crate::id(GRAPH));
                    let gmem = gmem_arc.lock().expect("failed to lock graph temp memory");
                    let b_in_response = b.as_ref().and_then(|b| b.sockets().input(0));
                    out = Some(Pass {
                        a_out: screen(a_out.expect("A has an output")),
                        b_in: b_in.map(screen),
                        closest_socket: gmem.closest_socket,
                        b_in_hovered: b_in_response.is_some_and(|r| r.hovered()),
                        a_edge_event: a.edge_event(),
                        b_edge_event: b.and_then(|b| b.edge_event()),
                    });
                });
            })
            .textures_delta
            .clear();
        out.expect("the graph ran")
    }

    /// Move the pointer to `pos`, then run a second pass, since pointer
    /// coverage of node frames takes effect on the next pass.
    fn hover(g: &mut TestGraph, pos: egui::Pos2) -> Pass {
        pass(g, vec![egui::Event::PointerMoved(pos)]);
        pass(g, vec![])
    }

    /// A press or release of the primary button at `pos`.
    fn primary(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// A point just outside the node frame, on the socket.
    fn off_frame((pos, normal): (egui::Pos2, egui::Vec2)) -> egui::Pos2 {
        pos + normal * NEAR
    }

    /// A point just inside the node frame, near the socket.
    fn on_frame((pos, normal): (egui::Pos2, egui::Vec2)) -> egui::Pos2 {
        pos - normal * NEAR
    }

    /// The screen position and normal of the input of `B`.
    fn b_in(p: &Pass) -> (egui::Pos2, egui::Vec2) {
        p.b_in.expect("B has an input")
    }

    /// Off the frame, a press starts an edge, so the socket is detected. Its
    /// response is hovered, for tooltips.
    #[test]
    fn hover_off_frame_detects_socket() {
        let (mut g, p) = test_graph(false);
        let p = hover(&mut g, off_frame(b_in(&p)));
        assert_eq!(p.closest_socket, Some(B_IN));
        assert!(p.b_in_hovered);
    }

    /// On the frame, a press drags the node, so the socket is not detected.
    /// Its response is not hovered.
    #[test]
    fn hover_on_frame_skips_socket() {
        let (mut g, p) = test_graph(false);
        let p = hover(&mut g, on_frame(b_in(&p)));
        assert_eq!(p.closest_socket, None);
        assert!(!p.b_in_hovered);
    }

    /// An immutable graph cannot start an edge, so no socket is detected.
    #[test]
    fn immutable_hover_skips_socket() {
        let (mut g, p) = test_graph(true);
        let p = hover(&mut g, off_frame(b_in(&p)));
        assert_eq!(p.closest_socket, None);
    }

    /// A press on a socket starts an edge. While the edge is in progress, a
    /// socket is detected from inside its node frame, and a release there
    /// ends the edge on it.
    #[test]
    fn edge_drag_detects_socket_on_frame() {
        let (mut g, p) = test_graph(false);
        let start = off_frame(p.a_out);
        let end = on_frame(b_in(&p));
        hover(&mut g, start);
        let p = pass(&mut g, vec![primary(start, true)]);
        assert_eq!(
            p.a_edge_event,
            Some(EdgeEvent::Started {
                kind: SocketKind::Output,
                index: 0,
            })
        );
        let p = hover(&mut g, end);
        assert_eq!(p.closest_socket, Some(B_IN));
        let p = pass(&mut g, vec![primary(end, false)]);
        assert_eq!(
            p.b_edge_event,
            Some(EdgeEvent::Ended {
                kind: SocketKind::Input,
                index: 0,
            })
        );
    }

    /// A node that no longer has sockets keeps none for edges or detection.
    #[test]
    fn socketless_node_keeps_no_sockets() {
        let (mut g, p) = test_graph(false);
        let old_b_in = b_in(&p);
        g.b_inputs = Some(0);
        let p = hover(&mut g, off_frame(old_b_in));
        assert_eq!(p.b_in, None);
        assert_eq!(p.closest_socket, None);
    }

    /// A node that is no longer shown keeps no sockets in graph memory.
    #[test]
    fn removed_node_keeps_no_sockets() {
        let (mut g, _) = test_graph(false);
        g.b_inputs = None;
        pass(&mut g, vec![]);
        let has_b = crate::with_graph_memory(&g.ctx, crate::id(GRAPH), |gmem| {
            gmem.node_sockets().contains_key(&B)
        });
        assert!(!has_b);
    }
}
