# Plan: Port GraphNode to Rust

## Overview

This document describes the plan to port the graph node subsystem from C++ to Rust. The graph system is fundamental to the audio processing architecture, enabling topological processing order determination for audio/MIDI nodes.

## Files to Create

Create the following files in `src/rust/backend_rust/src/`:

1. `graph_node.rs` - Core `GraphNode` trait and implementation
2. `graph_node_cxx.rs` - CXX bridge for C++ interop

## Components to Port

### 1. Type Aliases (Rust equivalents)
```rust
// SharedGraphNodeSet -> SharedGraphNodeSet
// WeakGraphNodeSet -> WeakGraphNodeSet
```

### 2. Core Trait: `GraphNodeTrait`
A Rust trait equivalent to the C++ `GraphNode` class with:
- Virtual methods: `graph_node_outgoing_edges()`, `graph_node_incoming_edges()`, `graph_node_co_process_nodes()`
- Virtual methods: `graph_node_name()`, `graph_node_process()`, `graph_node_co_process()`
- `set_processed_cb()` callback for profiling

### 3. Concrete Type: `GraphNodeWrapper`
A concrete implementation of `GraphNodeTrait` that:
- Stores a boxed virtual trait object (`GraphNodeVirtual`)
- Provides default implementations that call through to the stored object
- Implements timing wrapper in `PROC_process()` and `PROC_co_process()`

### 4. Helper Types
- `NodeWithParent<Parent>` - Template for nodes that hold a weak reference to parent
- `graph_node_parent_as<Parent>()` - Helper function to extract parent from a node

### 5. Interface Traits
- `NotifyProcessParametersInterface` - For buffer_size/sample_rate change notifications
- `HasGraphNodesInterface` - Base interface for objects with graph nodes
- `HasGraphNode` - Trait for objects that have a single graph node
- `HasTwoGraphNodes` - Trait for objects that have two graph nodes

### 6. Graph Processing Order
The `graph_processing_order()` function that computes topological sort.

## Phases

### Phase 1: Core Types
1. Create `graph_node.rs` with:
   - `GraphNodeTrait` - main trait
   - `GraphNodeWrapper` - concrete implementation with boxed virtual trait
   - Type aliases for sets
   - `PROC_process()` and `PROC_co_process()` timing wrappers

### Phase 2: Parent/Child Relationships
1. Add `NodeWithParent<T>` wrapper trait
2. Add `graph_node_parent_as<T>()` helper

### Phase 3: Interfaces
1. Add `NotifyProcessParametersInterface` trait
2. Add `HasGraphNodesInterface` trait
3. Add `HasGraphNode` trait with lazy `graph_node()` creation
4. Add `HasTwoGraphNodes` trait with lazy `first_graph_node()`/`second_graph_node()` creation

### Phase 4: CXX Bridge
1. Create `graph_node_cxx.rs`
2. Expose `GraphNodeWrapper` to C++ via CXX
3. Expose factory functions

### Phase 5: Graph Processing Algorithm
1. Port `graph_processing_order()` to Rust
2. Create CXX bridge for the algorithm
3. Update C++ code to call Rust implementation

### Phase 6: C++ Integration
1. Create C++ wrapper headers that include generated CXX code
2. Update existing C++ classes (`GraphLoop`, `GraphLoopChannel`, etc.) to use Rust types
3. Eventually remove C++ `GraphNode.h` implementation

## Implementation Details

### Virtual Trait Pattern
Since Rust doesn't have inheritance, we use a boxed trait object:
```rust
trait GraphNodeVirtual: Send + Sync {
    fn graph_node_outgoing_edges(&self) -> WeakGraphNodeSet;
    fn graph_node_incoming_edges(&self) -> WeakGraphNodeSet;
    // ... etc
}

struct GraphNodeWrapper {
    virtual_node: Box<dyn GraphNodeVirtual>,
    processed_cb: Option<Box<dyn Fn(u32) + Send>>,
}
```

### Weak Pointer Handling
The C++ uses `std::weak_ptr<GraphNode>` in edge sets. In Rust, we use `std::rc::Weak` or `crossbeam` for the same purpose.

### Thread Safety
Since this is called from the audio thread, all implementations must be `Send + Sync`.

## FFI Boundaries

The C++ side will:
1. Create `GraphNodeWrapper` instances via CXX bridge
2. Call virtual methods through the bridge
3. Use Rust `graph_processing_order()` for topological sorting

## Build Instructions

1. Navigate to repo root: `cd /home/sander/dev/shoopdaloop`
2. Build: `cargo build`
3. Run tests: `cargo test`
4. For strict warnings: `RUSTFLAGS="-D warnings" cargo build`

## Test Strategy

1. **Rust unit tests**: Add tests for `GraphNodeTrait` behavior, edge sets, timing callbacks
2. **Rust integration tests**: Test `graph_processing_order()` algorithm
3. **C++ integration tests**: Run existing `test_graph_construction` tests to verify nothing breaks
4. **QML self-tests**: Run `./target/debug/shoopdaloop_dev.sh --self-test`

## Key Files Reference

- **Source**: `src/backend/internal/GraphNode.h` - Main header to port
- **Source**: `src/backend/internal/graph_processing_order.h` and `.cpp` - Algorithm to port
- **Tests**: `src/backend/test/integration/test_graph_construction.cpp` - Integration tests
- **Rust modules**: `src/rust/backend_rust/src/` - Where to create new files
- **Existing pattern**: See `src/rust/backend_rust/src/port_core.rs` and `port_core_cxx.rs` for CXX bridge pattern

## Dependencies

- `cxx` crate (already in Cargo.toml)
- `crossbeam-queue` (for lock-free queues if needed)
- `parking_lot` (already in dependencies)

No new dependencies should be needed.