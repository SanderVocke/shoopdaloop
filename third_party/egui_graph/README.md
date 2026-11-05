# egui_graph

A general-purpose node graph widget for [egui](https://github.com/emilk/egui).

Build interactive node-based editors with nodes connected by edges for visual
programming interfaces, shader editors, DSP graphs, or any graph-based UI.

**Note:** This library is the basis for
[nannou-org/gantz](https://github.com/nannou-org/gantz). For a more
sophisticated example of what can be built with egui_graph, check out
[gantz](https://nannou-org.github.io/gantz).

## Key Design Philosophy

One of the core design decisions of `egui_graph` is to avoid requiring that
users model their graph with any particular data structure. The library provides
immediate-mode widgets for rendering and interacting with graphs, but leaves
the underlying data model up to you. Store your graph however makes sense for
your application - whether that's an adjacency list, entity-component system,
or any other representation.

## Features

- **Interactive Nodes**: Drag, select, and delete nodes with intuitive controls
- **Edge Creation**: Connect nodes via input/output sockets with bezier curve edges
- **Multi-Selection**: Rectangle selection and Ctrl+click for selecting multiple nodes
- **Automatic Layout**: Optional socket-aware layered graph layout
- **Customizable**: Configure node flow direction, socket/frame appearance, and more
- **Zoom & Pan**: Navigate large graphs with mouse controls
- **Model-Agnostic**: No prescribed graph data structure - use whatever fits your needs

## Quick Start

```rust
use egui_graph::{edge::Edge, node::Node, Graph, NodeId, View};

// The `View` stores the node positions and the "camera". Keep it, and the
// selection state of each edge, between frames.
fn graph_ui(ui: &mut egui::Ui, view: &mut View, edge_selected: &mut bool) {
    Graph::new("my_graph").show(view, ui, |ui, show| {
        // Add nodes to the graph.
        show.nodes(ui, |nctx, ui| {
            Node::new("node_1")
                .inputs(2)
                .outputs(1)
                .show(nctx, ui, |node_ctx| {
                    node_ctx.framed(|ui, _sockets| ui.label("My Node"))
                });
            Node::new("node_2")
                .inputs(2)
                .show(nctx, ui, |node_ctx| {
                    node_ctx.framed(|ui, _sockets| ui.label("Other Node"))
                });
        })
        // Add edges between nodes.
        .edges(ui, |ectx, ui| {
            Edge::new(
                (NodeId::new("node_1"), 0), // From output 0
                (NodeId::new("node_2"), 1), // To input 1
                edge_selected,
            )
            .show(ectx, ui);
        });
    });
}
```

Visit the demo.rs example for a more thorough, up-to-date example. It shows
how to add and remove nodes and edges in response to the node and edge
responses.

## Core Components

### Graph Widget

The main widget that contains all nodes and edges:

```rust
use egui_graph::{Graph, View};

fn graph_ui(ui: &mut egui::Ui, view: &mut View) {
    Graph::new("my_graph")
        .background(true)      // Enable background
        .dot_grid(true)        // Show dot grid
        .zoom_range(0.1..=2.0) // Set zoom limits
        .center_view(true)     // Center the camera
        .show(view, ui, |ui, show| { /* ... */ });
}
```

### Nodes

Nodes are containers with input/output sockets:

```rust
use egui::{Color32, Direction};
use egui_graph::{node::Node, NodesCtx};

fn node_ui(nctx: &mut NodesCtx, ui: &mut egui::Ui) {
    Node::new("my_node")
        .inputs(3)                    // Number of input sockets
        .outputs(2)                   // Number of output sockets
        .flow(Direction::LeftToRight) // Socket arrangement
        .socket_color(Color32::BLUE)
        .socket_radius(5.0)
        .show(nctx, ui, |node_ctx| {
            // Node content goes here
            node_ctx.framed(|ui, _sockets| ui.label("Node Content"))
        });
}
```

### Edges

Connect nodes with bezier curve edges:

```rust
use egui_graph::{edge::Edge, EdgesCtx, NodeId};

fn edge_ui(ectx: &mut EdgesCtx, ui: &mut egui::Ui, selected: &mut bool) {
    let (source_node_id, output_index) = (NodeId::new("node_1"), 0);
    let (target_node_id, input_index) = (NodeId::new("node_2"), 1);
    Edge::new(
        (source_node_id, output_index),
        (target_node_id, input_index),
        selected,
    )
    .distance_per_point(1.0) // Curve sampling distance
    .show(ectx, ui);
}
```

### Automatic Layout

With the `layout` feature enabled, a built-in socket-aware layered layout
orders and positions nodes to minimise edge crossings and keep edges straight,
taking the socket each edge connects to into account:

```rust
use egui::Direction;
use egui_graph::{layout, LayoutNode, LayoutParams, NodeId, View};

// Each node is `(id, size, inputs, outputs)`, and each edge is
// `(output node, output index, input node, input index)`.
fn layout_graph(
    view: &mut View,
    style: &egui::Style,
    nodes: &[(NodeId, egui::Vec2, usize, usize)],
    edges: &[(NodeId, usize, NodeId, usize)],
) {
    let positions = layout(
        nodes.iter().map(|(id, size, inputs, outputs)| {
            let node = LayoutNode::new(*size)
                .socket_padding(egui_graph::socket_padding(style))
                .inputs(*inputs)
                .outputs(*outputs);
            (*id, node)
        }),
        // Edge endpoints are `(node, socket index)`, as in `Edge::new`.
        edges.iter().map(|(a, out_ix, b, in_ix)| ((*a, *out_ix), (*b, *in_ix))),
        LayoutParams::new(Direction::LeftToRight),
    );
    view.layout = positions;
}
```

For graphs without socket information, `layout_from_sizes` accepts plain
`(NodeId, size)` nodes and `(NodeId, NodeId)` edges.

To keep long edges from passing over unrelated nodes, use `layout_routed`,
which additionally returns corridor waypoints for the edges that need them,
and thread each edge through its route when drawing:

```rust
use egui_graph::{edge::Edge, layout_routed, EdgeRoutes, EdgesCtx};
use egui_graph::{LayoutNode, LayoutParams, NodeId, View};

type Socket = (NodeId, usize);

fn layout_graph(
    view: &mut View,
    nodes: Vec<(NodeId, LayoutNode)>,
    edges: &[(Socket, Socket)],
    params: LayoutParams,
) -> EdgeRoutes {
    let (positions, routes) = layout_routed(nodes, edges.iter().copied(), params);
    view.layout = positions;
    routes
}

// ... when drawing each edge:
fn edge_ui(
    ectx: &mut EdgesCtx,
    ui: &mut egui::Ui,
    routes: &EdgeRoutes,
    ((src, out_ix), (dst, in_ix)): (Socket, Socket),
    selected: &mut bool,
) {
    let waypoints = routes.route((src, out_ix), (dst, in_ix), 0).unwrap_or(&[]);
    Edge::new((src, out_ix), (dst, in_ix), selected)
        .waypoints(waypoints)
        .show(ectx, ui);
}
```

Nodes may flow in different directions within one graph. Give a node its own
flow with `LayoutNode::flow`; nodes joined only to others of the same flow are
laid out together in that flow, while edges crossing between flows split the
graph into clusters that are arranged along the outer direction
(`LayoutParams::flow`):

```rust
use egui::Direction;
use egui_graph::LayoutNode;

let size = egui::vec2(80.0, 40.0);
let node = LayoutNode::new(size).inputs(1).outputs(1).flow(Direction::TopDown);
```

## Controls

### Mouse Controls
- **Left Click**: Select node/edge
- **Ctrl + Left Click**: Toggle selection
- **Left Drag on Background**: Rectangle selection of nodes
- **Shift + Left Drag on Background**: Rectangle selection of edges
- **Left Drag on Node**: Move selected nodes
- **Middle Mouse Drag**: Pan view
- **Scroll Wheel**: Pan view
- **Ctrl/Cmd + Scroll Wheel or Pinch**: Zoom in/out

### Keyboard Controls
- **Delete/Backspace**: Remove selected nodes/edges

### Socket Interaction
- **Press a Socket**: Start edge creation, from an input or an output
- **Drag to a Socket**: Preview connection
- **Release on a Socket**: Create edge, if the socket kind is the opposite of the start socket
- **Release Elsewhere**: Cancel edge creation

## Examples

Run the included demo:

```bash
cargo run --release --example demo
```

The demo showcases:
- Multiple node types (labels, buttons, sliders)
- Dynamic node creation and deletion
- Edge creation between nodes
- Automatic layout
- Configuration options

For a focused look at laying out graphs whose nodes flow in different
directions, run the mixed-flow example:

```bash
cargo run --release --example mixed_flow
```

## Architecture

The library follows egui's immediate-mode paradigm while maintaining necessary
state for graph interactions. Internal state includes:

- Node sizes
- Selection state for nodes (the application keeps the selection state of each
  edge)
- Active edge creation
- Socket positions for edge rendering

State is stored in egui's data store and accessed through the widget APIs.
