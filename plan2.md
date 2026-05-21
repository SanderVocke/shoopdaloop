# Plan: Bridge Rust GraphNode to C++ and Remove C++ Version

## Overview

This document describes the remaining work to complete the GraphNode port from C++ to Rust. The Rust implementation is already in place (`src/rust/backend_rust/src/graph_node.rs`), and this plan covers integrating it with the C++ codebase and eventually removing the C++ implementation.

## Current State

The Rust GraphNode implementation provides:
- `GraphNodeVirtual` trait with all virtual methods
- `GraphNodeWrapper` struct with timing callbacks (PROC_process, PROC_co_process)
- `graph_processing_order()` function for topological sorting
- Interface traits: `NotifyProcessParametersInterface`, `HasGraphNodesInterface`, `HasGraphNode`, `HasTwoGraphNodes`
- CXX bridge in `graph_node_cxx.rs` exposing `GraphNodeWrapperCxx`

The C++ implementation is in:
- `src/backend/internal/GraphNode.h` - main GraphNode class
- `src/backend/internal/graph_processing_order.cpp` - topological sort algorithm

## Remaining Work

### Phase 1: Create C++ Wrapper Header

1. Create `src/backend/internal/RustGraphNode.h` following the pattern of `RustAudioPort.h`

2. The header should:
   - Include the generated CXX header (`backend_rust/src/graph_node_cxx.rs.h`)
   - Define `RustGraphNode` class that wraps the Rust `GraphNodeWrapperCxx`
   - Implement the full `GraphNode` interface (all virtual methods)
   - Delegate to the Rust implementation

3. Key methods to implement:
   - `graph_node_outgoing_edges()` - return `WeakGraphNodeSet`
   - `graph_node_incoming_edges()` - return `WeakGraphNodeSet`
   - `graph_node_co_process_nodes()` - return `WeakGraphNodeSet`
   - `graph_node_name()` - return `std::string`
   - `graph_node_process(uint32_t nframes)` - void
   - `graph_node_co_process(nodes, nframes)` - void
   - `PROC_process(uint32_t nframes)` - timing wrapper
   - `PROC_co_process(nodes, nframes)` - timing wrapper with co-processing

4. Key challenge: C++ uses `std::set<shoop_weak_ptr<GraphNode>>` for edge sets. Need to convert between:
   - Rust: `Vec<WeakGraphNode>` (where `WeakGraphNode = Rc<RefCell<dyn GraphNodeVirtual>>`)
   - C++: `std::set<shoop_weak_ptr<GraphNode>>`

### Phase 2: Bridge graph_processing_order()

1. The C++ function signature is:
   ```cpp
   std::vector<std::set<GraphNode*>> graph_processing_order(std::set<GraphNode*> nodes);
   ```

2. Options for bridging:
   - Option A: Expose Rust version via CXX, create C++ wrapper that converts
   - Option B: Keep C++ version but have it call Rust for core logic
   - Option C: Rewrite in C++ using new Rust types

3. Recommended: Option A with C++ wrapper
   - Add CXX bridge for `graph_processing_order()` that returns raw node pointers
   - C++ wrapper converts `Vec<Vec<usize>>` to `std::vector<std::set<GraphNode*>>`

### Phase 3: Update Existing C++ Code to Use Rust Types

1. Identify all C++ files that use GraphNode:
   ```bash
   grep -r "GraphNode" src/backend/internal/*.h | grep -v RustGraphNode
   ```

2. Key classes to update:
   - `GraphLoop` - extends GraphNode
   - `GraphLoopChannel` - extends GraphNode
   - `GraphAudioPort` - extends HasGraphNode
   - `GraphPort` - extends GraphNode
   - Any other classes using the interface

3. Strategy: Use `RustGraphNode` as a drop-in replacement
   - Change inheritance from `public GraphNode` to `public RustGraphNode`
   - Or use composition: `std::shared_ptr<backend_rust::GraphNodeWrapperCxx>`

### Phase 4: Deprecate and Remove C++ GraphNode

1. First: mark C++ GraphNode as deprecated
   ```cpp
   [[deprecated("Use RustGraphNode instead")]]
   class GraphNode { ... };
   ```

2. Update all usages to the new Rust-backed type

3. Remove C++ implementation files:
   - `src/backend/internal/GraphNode.h`
   - `src/backend/internal/GraphNode.cpp` (if exists)
   - `src/backend/internal/graph_processing_order.cpp`

4. Update CMakeLists.txt if needed

### Phase 5: Testing

1. Run Rust unit tests:
   ```bash
   cargo test -p backend_rust graph_node
   ```

2. Run C++ integration tests:
   ```bash
   ./target/debug/test_runner "[GraphConstruct]"
   ```

3. Run full test suite:
   ```bash
   ./target/debug/test_runner
   ```

4. Run QML self-tests:
   ```bash
   ./target/debug/shoopdaloop_dev.sh --self-test
   ```

## Build Instructions

1. Build the project:
   ```bash
   cargo build
   ```

2. If final build:
   ```bash
   cargo fmt --all
   RUSTFLAGS="-D warnings" cargo build
   ```

## Key Files

### Rust (already implemented)
- `src/rust/backend_rust/src/graph_node.rs` - Core types and algorithm
- `src/rust/backend_rust/src/graph_node_cxx.rs` - CXX bridge
- `src/rust/backend_rust/src/lib.rs` - Module exports

### C++ (to be created/modified)
- `src/backend/internal/RustGraphNode.h` - NEW: C++ wrapper header
- `src/backend/internal/GraphNode.h` - TO BE DEPRECATED
- `src/backend/internal/graph_processing_order.cpp` - TO BE DEPRECATED

## Dependencies

- `cxx` crate (already in Cargo.toml)
- No new dependencies needed

## Notes

- The thread-safety requirements (`Send + Sync`) are already satisfied in Rust
- The C++ wrapper must handle the `shoop_shared_ptr` / `shoop_weak_ptr` vs `Arc<RefCell<...>>` conversion
- Edge set conversion is the most complex part - requires bidirectional pointer mapping
