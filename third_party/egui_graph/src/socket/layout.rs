pub mod grid;

use crate::NodeSockets;
use std::collections::BTreeMap;

/// Controls how socket positions are determined for a node.
///
/// Pre-initialized as `EvenlySpaced` from the `Node` builder's input/output counts.
/// Users can ignore it for automatic spacing, or switch to explicit positioning
/// via [`SocketLayout::input`], [`SocketLayout::output`], [`SocketLayout::row`],
/// or [`SocketLayout::col`].
pub struct SocketLayout {
    flow: egui::Direction,
    inputs: SocketPositions,
    outputs: SocketPositions,
}

enum SocketPositions {
    /// Evenly spaced along the edge (current behavior).
    EvenlySpaced(usize),
    /// Explicit cross-axis positions. Only positioned sockets are rendered.
    Explicit(BTreeMap<usize, f32>),
}

impl SocketLayout {
    /// Create a new `SocketLayout` that evenly spaces sockets along the node edge.
    pub fn evenly_spaced(flow: egui::Direction, inputs: usize, outputs: usize) -> Self {
        Self {
            flow,
            inputs: SocketPositions::EvenlySpaced(inputs),
            outputs: SocketPositions::EvenlySpaced(outputs),
        }
    }

    /// Set an input socket's explicit cross-axis position.
    ///
    /// Switches to explicit positioning on first call.
    pub fn input_at(&mut self, ix: usize, cross: f32) {
        self.inputs.set_explicit(ix, cross);
    }

    /// Set an output socket's explicit cross-axis position.
    ///
    /// Switches to explicit positioning on first call.
    pub fn output_at(&mut self, ix: usize, cross: f32) {
        self.outputs.set_explicit(ix, cross);
    }

    /// Register an input socket aligned with the cross-axis center of `rect`.
    ///
    /// Switches to explicit positioning on first call.
    pub fn input(&mut self, ix: usize, rect: egui::Rect) {
        self.input_at(ix, cross_axis_center(self.flow, rect));
    }

    /// Register an output socket aligned with the cross-axis center of `rect`.
    ///
    /// Switches to explicit positioning on first call.
    pub fn output(&mut self, ix: usize, rect: egui::Rect) {
        self.output_at(ix, cross_axis_center(self.flow, rect));
    }

    /// Render content in a `ui.scope`, registering sockets aligned with its
    /// cross-axis center. Shared logic for `row` and `col`.
    fn aligned<R>(
        &mut self,
        ui: &mut egui::Ui,
        input: Option<usize>,
        output: Option<usize>,
        content: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        let ir = ui.scope(content);
        if let Some(ix) = input {
            self.input(ix, ir.response.rect);
        }
        if let Some(ix) = output {
            self.output(ix, ir.response.rect);
        }
        ir
    }

    /// Register sockets aligned with a row of content (horizontal flows).
    pub fn row<R>(
        &mut self,
        ui: &mut egui::Ui,
        input: Option<usize>,
        output: Option<usize>,
        content: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        self.aligned(ui, input, output, content)
    }

    /// Register sockets aligned with a column of content (vertical flows).
    pub fn col<R>(
        &mut self,
        ui: &mut egui::Ui,
        input: Option<usize>,
        output: Option<usize>,
        content: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        self.aligned(ui, input, output, content)
    }

    /// Resolve the layout into concrete `NodeSockets` given the final frame
    /// rect and socket padding.
    pub(crate) fn resolve(
        self,
        flow: egui::Direction,
        rect: egui::Rect,
        socket_padding: f32,
    ) -> NodeSockets {
        let inputs = resolve_positions(&self.inputs, flow, rect, socket_padding, true);
        let outputs = resolve_positions(&self.outputs, flow, rect, socket_padding, false);
        NodeSockets {
            flow,
            inputs,
            outputs,
        }
    }
}

impl SocketPositions {
    fn set_explicit(&mut self, ix: usize, cross: f32) {
        match self {
            SocketPositions::EvenlySpaced(_) => {
                let mut map = BTreeMap::new();
                map.insert(ix, cross);
                *self = SocketPositions::Explicit(map);
            }
            SocketPositions::Explicit(map) => {
                map.insert(ix, cross);
            }
        }
    }
}

/// The cross-axis center of a rect relative to the flow direction.
fn cross_axis_center(flow: egui::Direction, rect: egui::Rect) -> f32 {
    match flow {
        egui::Direction::LeftToRight | egui::Direction::RightToLeft => rect.center().y,
        egui::Direction::TopDown | egui::Direction::BottomUp => rect.center().x,
    }
}

/// Resolve a `SocketPositions` into a `BTreeMap<usize, Pos2>`.
fn resolve_positions(
    positions: &SocketPositions,
    flow: egui::Direction,
    rect: egui::Rect,
    socket_padding: f32,
    is_input: bool,
) -> BTreeMap<usize, egui::Pos2> {
    match positions {
        SocketPositions::EvenlySpaced(count) => {
            resolve_evenly_spaced(*count, flow, rect, socket_padding, is_input)
        }
        SocketPositions::Explicit(map) => resolve_explicit(map, flow, rect, is_input),
    }
}

/// Cross-axis offsets (relative to the cross-axis min of the node rect) for
/// `count` sockets evenly spaced along an edge of length `cross_len`, inset by
/// `socket_padding` at both ends.
///
/// A lone socket sits at the padded start of the edge.
pub(crate) fn evenly_spaced_cross_offsets(
    count: usize,
    cross_len: f32,
    socket_padding: f32,
) -> impl Iterator<Item = f32> {
    let gap = if count > 1 {
        (cross_len - socket_padding * 2.0) / (count - 1) as f32
    } else {
        0.0
    };
    (0..count).map(move |ix| socket_padding + gap * ix as f32)
}

/// The position of a socket on the node's main-axis edge given its absolute
/// cross-axis coordinate.
pub(crate) fn socket_pos(
    flow: egui::Direction,
    rect: egui::Rect,
    is_input: bool,
    cross: f32,
) -> egui::Pos2 {
    match flow {
        egui::Direction::LeftToRight => {
            let x = if is_input { rect.min.x } else { rect.max.x };
            egui::Pos2::new(x, cross)
        }
        egui::Direction::RightToLeft => {
            let x = if is_input { rect.max.x } else { rect.min.x };
            egui::Pos2::new(x, cross)
        }
        egui::Direction::TopDown => {
            let y = if is_input { rect.min.y } else { rect.max.y };
            egui::Pos2::new(cross, y)
        }
        egui::Direction::BottomUp => {
            let y = if is_input { rect.max.y } else { rect.min.y };
            egui::Pos2::new(cross, y)
        }
    }
}

/// Evenly space sockets along the edge.
fn resolve_evenly_spaced(
    count: usize,
    flow: egui::Direction,
    rect: egui::Rect,
    socket_padding: f32,
    is_input: bool,
) -> BTreeMap<usize, egui::Pos2> {
    let (cross_min, cross_len) = match flow {
        egui::Direction::LeftToRight | egui::Direction::RightToLeft => (rect.min.y, rect.height()),
        egui::Direction::TopDown | egui::Direction::BottomUp => (rect.min.x, rect.width()),
    };
    evenly_spaced_cross_offsets(count, cross_len, socket_padding)
        .enumerate()
        .map(|(ix, offset)| (ix, socket_pos(flow, rect, is_input, cross_min + offset)))
        .collect()
}

/// Explicit mode: place sockets at the main-axis edge, using the stored
/// cross-axis positions.
fn resolve_explicit(
    map: &BTreeMap<usize, f32>,
    flow: egui::Direction,
    rect: egui::Rect,
    is_input: bool,
) -> BTreeMap<usize, egui::Pos2> {
    map.iter()
        .map(|(&ix, &cross)| (ix, socket_pos(flow, rect, is_input, cross)))
        .collect()
}
