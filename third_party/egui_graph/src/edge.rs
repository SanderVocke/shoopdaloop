use crate::{bezier, EdgesCtx, NodeId};
use std::ops;

/// A simple bezier-curve Edge widget.
///
/// Handles interaction (selection, deselection, deletion) and painting the
/// bezier curve.
///
/// By default, the stroke for each state is adopted from the egui visuals:
///
/// - Selected: `ui.visuals().selection.stroke`.
/// - Hovered: `ui.visuals().widgets.hovered.fg_stroke`.
/// - Otherwise: `ui.visuals().widgets.noninteractive.fg_stroke`.
///
/// Each of these may be overridden per-edge via [`Edge::selected_stroke`],
/// [`Edge::hovered_stroke`] and [`Edge::stroke`].
pub struct Edge<'a> {
    edge: ((NodeId, OutputIx), (NodeId, InputIx)),
    waypoints: &'a [egui::Pos2],
    distance_per_point: f32,
    shape: EdgeShape,
    stroke: Option<egui::Stroke>,
    hovered_stroke: Option<egui::Stroke>,
    selected_stroke: Option<egui::Stroke>,
    selected: &'a mut bool,
}

/// A response returned from the [`Edge`] widget.
///
/// Similar to [`egui::Response`], however as there's no clear rectangular space
/// allocated to the edge, we use a more minimal custom response.
pub struct EdgeResponse {
    response: egui::Response,
    changed: bool,
    deleted: bool,
    /// The path of the edge, or `None` if either socket does not exist.
    path: Option<bezier::Path>,
    /// The pointer position, from which to find the closest point on the path.
    pointer: egui::Pos2,
    distance_per_point: f32,
}

/// The resolved inputs for painting an edge, handed to the closure given to
/// [`Edge::show_with`].
///
/// All coordinates share the layer-local space of the edge's sockets.
#[non_exhaustive]
pub struct EdgePaintCtx<'a> {
    /// The edge's piecewise-cubic bezier path.
    pub path: &'a bezier::Path,
    /// The path flattened at the edge's distance-per-point, ready for
    /// [`egui::Shape::line`].
    pub points: &'a [egui::Pos2],
    /// Whether the edge is currently selected.
    pub selected: bool,
    /// Whether the edge is hover-highlighted, including the highlight shown
    /// while a pending selection rectangle covers the edge.
    pub hovered: bool,
    /// The stroke default painting would use for the current state, with the
    /// per-edge stroke overrides applied and falling back to the egui visuals.
    pub stroke: egui::Stroke,
}

/// How an edge runs from its output socket to its input socket.
///
/// Every shape leaves each socket along the socket's normal, so an edge between
/// sockets that line up along the flow is straight in each shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EdgeShape {
    /// A curve whose handles leave the sockets along their normals. Each
    /// handle is `curvature` times half the distance between the sockets, and
    /// at most `max_handle` long, so an edge between distant nodes does not
    /// swing far out. A `curvature` of `0.0` gives a straight line.
    Bezier { curvature: f32, max_handle: f32 },
    /// A curve whose handles follow how far the edge runs along the flow, not
    /// across it, so a long run across the flow stays flat. Each handle is
    /// `curvature` times the run along the flow, taken as at least `min_span`
    /// so a near-level edge still bends. An edge that runs back against the
    /// flow takes `curvature` times `min_span` plus its run back, and loops
    /// around. A `curvature` of `0.0` gives a straight line.
    Flow { curvature: f32, min_span: f32 },
    /// Straight runs along the flow, across it at the midpoint, and along it
    /// again, with corners rounded to `radius`. An edge that runs back
    /// against the flow steps around. See [`bezier::Path::step_via`].
    Step { radius: f32 },
}

/// An index of a node's input or output socket.
pub type SocketIx = usize;
/// An index of a node's input socket.
pub type InputIx = SocketIx;
/// An index of a node's output socket.
pub type OutputIx = SocketIx;

impl<'a> Edge<'a> {
    pub const DEFAULT_DISTANCE_PER_POINT: f32 = 5.0;

    /// An edge from node `a`'s output socket to node `b`'s input socket.
    pub fn new(a: (NodeId, OutputIx), b: (NodeId, InputIx), selected: &'a mut bool) -> Self {
        Self {
            edge: (a, b),
            waypoints: &[],
            distance_per_point: Self::DEFAULT_DISTANCE_PER_POINT,
            shape: EdgeShape::DEFAULT,
            stroke: None,
            hovered_stroke: None,
            selected_stroke: None,
            selected,
        }
    }

    /// Thread the edge's curve through the given intermediate waypoints,
    /// ordered from the output socket toward the input socket.
    ///
    /// Use this with the corridor routes produced by the automatic layout
    /// (`EdgeRoutes`, from `layout_routed`) to keep long edges from passing
    /// over unrelated nodes. Waypoints share the node layout's coordinate
    /// space.
    ///
    /// Routes are only meaningful while node positions match the layout that
    /// produced them - when nodes are arranged freely instead, omit the
    /// waypoints so edges fall back to direct curves.
    ///
    /// Default: none (a single curve directly between the sockets).
    pub fn waypoints(mut self, waypoints: &'a [egui::Pos2]) -> Self {
        self.waypoints = waypoints;
        self
    }

    /// The distance-per-point used to render the bezier curve path.
    ///
    /// This path is also used to check for selection interaction.
    ///
    /// The smaller the distance, the higher-quality rendering and interactions
    /// will be, at the cost of performance.
    ///
    /// Default: `Self::DEFAULT_DISTANCE_PER_POINT`
    pub fn distance_per_point(mut self, dist: f32) -> Self {
        self.distance_per_point = dist;
        self
    }

    /// How the edge runs between its sockets.
    ///
    /// Default: [`EdgeShape::DEFAULT`].
    pub fn shape(mut self, shape: EdgeShape) -> Self {
        self.shape = shape;
        self
    }

    /// Override the stroke used when the edge is in its default (unselected,
    /// unhovered) state.
    ///
    /// Default: `ui.visuals().widgets.noninteractive.fg_stroke`.
    pub fn stroke(mut self, stroke: egui::Stroke) -> Self {
        self.stroke = Some(stroke);
        self
    }

    /// Override the stroke used when the edge is hovered.
    ///
    /// Default: `ui.visuals().widgets.hovered.fg_stroke`.
    pub fn hovered_stroke(mut self, stroke: egui::Stroke) -> Self {
        self.hovered_stroke = Some(stroke);
        self
    }

    /// Override the stroke used when the edge is selected.
    ///
    /// Default: `ui.visuals().selection.stroke`.
    pub fn selected_stroke(mut self, stroke: egui::Stroke) -> Self {
        self.selected_stroke = Some(stroke);
        self
    }

    /// Process any user interaction with the edge and present it.
    pub fn show(self, ectx: &mut EdgesCtx, ui: &mut egui::Ui) -> EdgeResponse {
        self.show_with(ectx, ui, |ui, cx| {
            ui.painter()
                .add(egui::Shape::line(cx.points.to_vec(), cx.stroke));
        })
    }

    /// As [`Edge::show`], but painting the edge via `paint` instead of the
    /// default solid line.
    ///
    /// Interaction (hover, selection, deletion) is identical to [`Edge::show`].
    /// The `paint` closure receives the resolved paint inputs - see
    /// [`EdgePaintCtx`]. It is not called when either socket position is
    /// unavailable, in which case there is nothing to paint. It is also not
    /// called when the edge is outside the visible area of the graph.
    pub fn show_with(
        self,
        ectx: &mut EdgesCtx,
        ui: &mut egui::Ui,
        paint: impl FnOnce(&mut egui::Ui, EdgePaintCtx),
    ) -> EdgeResponse {
        let Self {
            edge: ((a, output), (b, input)),
            waypoints,
            distance_per_point,
            shape,
            stroke,
            hovered_stroke,
            selected_stroke,
            selected,
        } = self;

        // Get the mouse position for computing the closest point on the edge.
        let ui_response = ui.response();
        let mouse_pos = ui_response
            .interact_pointer_pos()
            .or(ui_response.hover_pos())
            .unwrap_or_default();

        // Retrieve the location and direction of the node sockets.
        // If either socket position is unavailable (e.g. sparse explicit
        // layout), skip rendering entirely.
        let edge_id = ui.scope_id().with(("edge", a, output, b, input));
        let Some((a_out, b_in)) = ectx.edge_sockets((a, output), (b, input)) else {
            let response = ui.interact(egui::Rect::NOTHING, edge_id, egui::Sense::click());
            return EdgeResponse {
                response,
                changed: false,
                deleted: false,
                path: None,
                pointer: mouse_pos,
                distance_per_point,
            };
        };

        let path = shape.path(a_out, waypoints, b_in);

        // The curve lies within the bounds of its control points. Only flatten
        // the curve to paint it within the visible area, or to find its point
        // closest to a pointer within reach.
        let select_dist = ui.style().interaction.interact_radius;
        let reach = path.max_bounds().expand(select_dist);
        let near_pointer = reach.contains(mouse_pos);
        let visible = ui.clip_rect().intersects(reach);
        let pts: Vec<_> = if visible || near_pointer {
            path.flatten(distance_per_point).collect()
        } else {
            Vec::new()
        };

        // Create a per-edge response for interaction and context menu support.
        // The interact area follows the mouse along the edge curve.
        let closest_point = if near_pointer {
            let dist_sq = |p: &egui::Pos2| p.distance_sq(mouse_pos);
            pts.iter()
                .copied()
                .min_by(|p, q| dist_sq(p).total_cmp(&dist_sq(q)))
        } else {
            None
        };
        let interact_rect = closest_point.map_or(egui::Rect::NOTHING, |p| {
            egui::Rect::from_center_size(p, egui::Vec2::splat(select_dist * 2.0))
        });
        let response = ui.interact(interact_rect, edge_id, egui::Sense::click());

        // Determine if edge interactions should be processed.
        // Disable when drawing a new edge or when close to a socket.
        let edge_in_progress = ectx.edge_in_progress;
        let can_interact = !edge_in_progress && ectx.closest_socket.is_none();
        let clicked = can_interact && response.clicked();

        // Check if the edge intersects the selection rectangle.
        let under_selection_rect = ectx
            .selection_rect
            .map(|rect| path.intersects_rect(distance_per_point, rect))
            .unwrap_or(false);

        // Handle selection state changes.
        let old_selected = *selected;
        if *selected {
            // Deselect if: edge drawing started, ctrl+click, or click elsewhere without ctrl.
            if edge_in_progress
                || (clicked && ui.input(|i| i.modifiers.ctrl))
                || ui.input(|i| i.pointer.primary_pressed() && !i.modifiers.ctrl)
            {
                *selected = false;
            }
        } else if clicked
            || (under_selection_rect
                && ui.input(|i| i.modifiers.shift && i.pointer.primary_released()))
        {
            *selected = true;
        }

        // Check if the edge was deleted (skip when immutable).
        let mut deleted = false;
        // FIXME: We may only want to do this if `ui.scope_id()` has focus
        // (Memory::has_focus) or similar, but we still need to setup proper
        // focus-requesting and consider how to handle nodes too.
        if !ectx.immutable && *selected && !ui.ctx().egui_wants_keyboard_input() {
            let del_keys = [egui::Key::Delete, egui::Key::Backspace];
            if ui.input(|i| del_keys.iter().any(|&k| i.key_pressed(k))) {
                deleted = true;
            }
        }

        // Determine hover styling (additional conditions beyond response.hovered()).
        let show_hover = can_interact
            && response.hovered()
            && ui.input(|i| !i.pointer.primary_down() || i.pointer.could_any_button_be_click());

        // Paint the edge if it is within the visible area.
        let hovered = show_hover || (under_selection_rect && ui.input(|i| i.modifiers.shift));
        let stroke = if *selected {
            selected_stroke.unwrap_or(ui.style().visuals.selection.stroke)
        } else if hovered {
            hovered_stroke.unwrap_or(ui.style().visuals.widgets.hovered.fg_stroke)
        } else {
            stroke.unwrap_or(ui.style().visuals.widgets.noninteractive.fg_stroke)
        };
        if visible {
            let cx = EdgePaintCtx {
                path: &path,
                points: &pts,
                selected: *selected,
                hovered,
                stroke,
            };
            paint(ui, cx);
        }

        // Construct and return the response.
        let changed = old_selected != *selected;
        EdgeResponse {
            response,
            changed,
            deleted,
            path: Some(path),
            pointer: mouse_pos,
            distance_per_point,
        }
    }
}

impl EdgeShape {
    /// The default: a [`EdgeShape::Bezier`] of [`bezier::Cubic::DEFAULT_CURVATURE`]
    /// with handles of at most [`EdgeShape::DEFAULT_MAX_HANDLE`].
    pub const DEFAULT: Self = Self::Bezier {
        curvature: bezier::Cubic::DEFAULT_CURVATURE,
        max_handle: Self::DEFAULT_MAX_HANDLE,
    };
    /// The `max_handle` of [`EdgeShape::bezier`].
    pub const DEFAULT_MAX_HANDLE: f32 = 50.0;
    /// The `min_span` of [`EdgeShape::flow`].
    pub const DEFAULT_MIN_SPAN: f32 = 24.0;
    /// The `radius` of [`EdgeShape::step`].
    pub const DEFAULT_RADIUS: f32 = 10.0;

    /// A [`EdgeShape::Bezier`] of `curvature`, with a `max_handle` of
    /// [`EdgeShape::DEFAULT_MAX_HANDLE`].
    pub fn bezier(curvature: f32) -> Self {
        Self::Bezier {
            curvature,
            max_handle: Self::DEFAULT_MAX_HANDLE,
        }
    }

    /// A [`EdgeShape::Flow`] of `curvature`, with a `min_span` of
    /// [`EdgeShape::DEFAULT_MIN_SPAN`].
    pub fn flow(curvature: f32) -> Self {
        Self::Flow {
            curvature,
            min_span: Self::DEFAULT_MIN_SPAN,
        }
    }

    /// A [`EdgeShape::Step`] with corners of [`EdgeShape::DEFAULT_RADIUS`].
    pub fn step() -> Self {
        Self::Step {
            radius: Self::DEFAULT_RADIUS,
        }
    }

    /// The path of an edge of this shape from the output socket `a` to the
    /// input socket `b`, each a position and normal, through `waypoints`.
    pub fn path(
        self,
        a: (egui::Pos2, egui::Vec2),
        waypoints: &[egui::Pos2],
        b: (egui::Pos2, egui::Vec2),
    ) -> bezier::Path {
        match self {
            Self::Bezier {
                curvature,
                max_handle,
            } => {
                let factor = curvature.clamp(0.0, 1.0) * bezier::Cubic::MAX_CURVATURE_FACTOR;
                let handle = |chord: egui::Vec2| (chord.length() * factor).min(max_handle);
                bezier::Path::curve_via(a, waypoints, b, handle)
            }
            Self::Flow {
                curvature,
                min_span,
            } => {
                let (curvature, flow) = (curvature.clamp(0.0, 1.0), a.1);
                let handle = |chord: egui::Vec2| {
                    let along = chord.dot(flow);
                    match along >= 0.0 {
                        true => curvature * along.max(min_span),
                        false => curvature * (min_span - along),
                    }
                };
                bezier::Path::curve_via(a, waypoints, b, handle)
            }
            Self::Step { radius } => bezier::Path::step_via(a, waypoints, b, radius),
        }
    }
}

impl EdgeResponse {
    /// Whether or not the edge selected state changed.
    pub fn changed(&self) -> bool {
        self.changed
    }

    /// The edge was selected while `Delete` or `Backspace` were pressed.
    pub fn deleted(&self) -> bool {
        self.deleted
    }

    /// The position on the edge closest to the pointer.
    ///
    /// This is found on each call. It is `Pos2::ZERO` if either socket of the
    /// edge does not exist.
    pub fn closest_point(&self) -> egui::Pos2 {
        let Some(path) = &self.path else {
            return egui::Pos2::ZERO;
        };
        path.closest_point(self.distance_per_point, self.pointer)
    }
}

impl Default for EdgeShape {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl ops::Deref for EdgeResponse {
    type Target = egui::Response;
    fn deref(&self) -> &Self::Target {
        &self.response
    }
}

impl From<EdgeResponse> for egui::Response {
    fn from(response: EdgeResponse) -> Self {
        response.response
    }
}

#[cfg(test)]
mod tests {
    use super::{Edge, EdgeShape};
    use crate::node::{Node, NodeId};
    use crate::{Graph, View};

    /// The edge state observed at the end of a pass.
    struct Pass {
        painted: bool,
        hovered: bool,
        closest_point: egui::Pos2,
    }

    const SCREEN: egui::Rect = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(800.0, 600.0));
    /// Has one output on its right edge.
    const A: NodeId = NodeId(0);
    /// Has one input on its left edge, level with and to the right of `A`.
    const B: NodeId = NodeId(1);

    /// Show `A`, `B` and an edge from `A` to `B` over a few passes, with the
    /// view at `scene_rect` and the pointer at `pointer`.
    fn show_edge(scene_rect: egui::Rect, pointer: egui::Pos2) -> Pass {
        let ctx = egui::Context::default();
        let mut view = View {
            scene_rect,
            layout: [(A, egui::pos2(100.0, 100.0)), (B, egui::pos2(400.0, 100.0))].into(),
        };
        let mut selected = false;
        let mut out = None;
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(SCREEN),
                events: vec![egui::Event::PointerMoved(pointer)],
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                Graph::new("graph").show(&mut view, ui, |ui, show| {
                    show.nodes(ui, |nctx, ui| {
                        for (id, inputs, outputs) in [(A, 0, 1), (B, 1, 0)] {
                            let node = Node::from_id(id).inputs(inputs).outputs(outputs);
                            node.show(nctx, ui, |c| c.framed(|ui, _| ui.label("x")));
                        }
                    })
                    .edges(ui, |ectx, ui| {
                        let mut painted = false;
                        // Points 1 apart, so the closest point is within 1 of the
                        // true one whatever the edge's shape.
                        let edge = Edge::new((A, 0), (B, 0), &mut selected).distance_per_point(1.0);
                        let response = edge.show_with(ectx, ui, |_, _| painted = true);
                        out = Some(Pass {
                            painted,
                            hovered: response.hovered(),
                            closest_point: response.closest_point(),
                        });
                    });
                });
            })
            .textures_delta
            .clear();
        }
        out.expect("the edges ran")
    }

    /// A point on the straight edge between the output of `A` and the input
    /// of `B`, clear of both nodes.
    fn on_edge() -> egui::Pos2 {
        egui::pos2(300.0, 100.0 + node_height() / 2.0)
    }

    /// The height of a node with a single label, as laid out by the graph.
    fn node_height() -> f32 {
        let ctx = egui::Context::default();
        let mut height = None;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let node = Node::from_id(A).outputs(1);
            let r = node.show_static(ui, |c| c.framed(|ui, _| ui.label("x")));
            height = Some(r.inner.response.rect.height());
        })
        .textures_delta
        .clear();
        height.expect("the node ran")
    }

    /// An edge in view paints, and the pointer on it hovers it.
    #[test]
    fn edge_in_view_paints_and_hovers() {
        let p = show_edge(SCREEN, on_edge());
        assert!(p.painted);
        assert!(p.hovered);
        assert!(p.closest_point.distance(on_edge()) < 1.0);
    }

    /// An edge in view with the pointer away from it paints, but is not
    /// hovered.
    #[test]
    fn edge_away_from_pointer_is_not_hovered() {
        let p = show_edge(SCREEN, egui::pos2(300.0, 400.0));
        assert!(p.painted);
        assert!(!p.hovered);
    }

    /// An edge out of view does not paint. Its closest point to the pointer is
    /// still found on demand: the input of `B`, as the pointer is beyond it.
    #[test]
    fn edge_out_of_view_does_not_paint() {
        let away = SCREEN.translate(egui::vec2(2000.0, 2000.0));
        let p = show_edge(away, SCREEN.center());
        assert!(!p.painted);
        assert!(!p.hovered);
        let b_in = egui::pos2(400.0, on_edge().y);
        assert!(
            p.closest_point.distance(b_in) < 1.0,
            "{:?}",
            p.closest_point
        );
    }

    /// An output socket at `from` facing down and an input socket at `to`
    /// facing up, as in a top-down graph.
    fn sockets(
        from: (f32, f32),
        to: (f32, f32),
    ) -> ((egui::Pos2, egui::Vec2), (egui::Pos2, egui::Vec2)) {
        let a = (egui::pos2(from.0, from.1), egui::vec2(0.0, 1.0));
        let b = (egui::pos2(to.0, to.1), egui::vec2(0.0, -1.0));
        (a, b)
    }

    fn shapes() -> [EdgeShape; 4] {
        [
            EdgeShape::DEFAULT,
            EdgeShape::Bezier {
                curvature: 0.5,
                max_handle: 30.0,
            },
            EdgeShape::flow(0.5),
            EdgeShape::step(),
        ]
    }

    #[test]
    fn every_shape_is_straight_between_aligned_sockets() {
        let (a, b) = sockets((0.0, 0.0), (0.0, 120.0));
        for shape in shapes() {
            let path = shape.path(a, &[], b);
            assert!(path.flatten(2.0).all(|p| p.x.abs() < 1e-3), "{shape:?}");
        }
    }

    #[test]
    fn zero_curvature_is_straight() {
        let (a, b) = sockets((0.0, 0.0), (100.0, 50.0));
        for shape in [EdgeShape::bezier(0.0), EdgeShape::flow(0.0)] {
            let path = shape.path(a, &[], b);
            let on_line = |p: egui::Pos2| (p.y - p.x / 2.0).abs() < 1e-3;
            assert!(path.flatten(2.0).all(on_line), "{shape:?}");
        }
    }

    #[test]
    fn bezier_max_handle_limits_the_handles() {
        let (a, b) = sockets((0.0, 0.0), (400.0, 20.0));
        let open = EdgeShape::Bezier {
            curvature: 0.5,
            max_handle: f32::INFINITY,
        };
        let open = open.path(a, &[], b).segments()[0];
        let default = EdgeShape::DEFAULT.path(a, &[], b).segments()[0];
        let capped = EdgeShape::Bezier {
            curvature: 0.5,
            max_handle: 30.0,
        };
        let capped = capped.path(a, &[], b).segments()[0];
        // A quarter of the distance between the sockets.
        assert!((open.ctrl1.y - 400.0f32.hypot(20.0) * 0.25).abs() < 1e-3);
        assert_eq!(
            default.ctrl1,
            egui::pos2(0.0, EdgeShape::DEFAULT_MAX_HANDLE)
        );
        assert_eq!(capped.ctrl1, egui::pos2(0.0, 30.0));
        assert_eq!(capped.ctrl2, egui::pos2(400.0, -10.0));
    }

    #[test]
    fn flow_handles_follow_the_run_along_the_flow() {
        let handle = |to: (f32, f32)| {
            let (a, b) = sockets((0.0, 0.0), to);
            EdgeShape::flow(0.5).path(a, &[], b).segments()[0].ctrl1.y
        };
        // Half the run down, however far the run across.
        assert_eq!(handle((0.0, 100.0)), 50.0);
        assert_eq!(handle((600.0, 100.0)), 50.0);
        // A near-level edge bends as if it ran `min_span` down.
        assert_eq!(handle((400.0, 10.0)), 12.0);
        // An edge that runs back loops out by `min_span` plus its run back.
        assert_eq!(handle((40.0, -60.0)), 42.0);
    }

    #[test]
    fn step_turns_across_at_the_midpoint() {
        let (a, b) = sockets((0.0, 0.0), (100.0, 100.0));
        let path = EdgeShape::step().path(a, &[], b);
        let segments = path.segments();
        assert_eq!(segments[0].from, a.0);
        assert_eq!(segments[segments.len() - 1].to, b.0);
        let pts: Vec<_> = path.flatten(1.0).collect();
        let within = |v: f32| (-1e-3..=100.0 + 1e-3).contains(&v);
        assert!(pts.iter().all(|p| within(p.x) && within(p.y)));
        let mid = egui::pos2(50.0, 50.0);
        assert!(path.closest_point(1.0, mid).distance(mid) < 0.5);
    }

    #[test]
    fn step_goes_around_an_input_behind_its_output() {
        let (a, b) = sockets((0.0, 0.0), (20.0, -80.0));
        let path = EdgeShape::step().path(a, &[], b);
        let pts: Vec<_> = path.flatten(1.0).collect();
        // It leaves down, comes in from above, and ends on the input.
        assert!(pts.iter().any(|p| p.y >= 19.0));
        assert!(pts.iter().any(|p| p.y <= -99.0));
        assert_eq!(*pts.last().unwrap(), b.0);
    }
}
