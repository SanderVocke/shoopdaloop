# TODO: Complete Rust GraphNode Integration - Remove C++ Version

**Status: STARTING - Phase 1**

## Phase 1: Modify GraphNode Base Class
- [ ] Modify `src/backend/internal/GraphNode.h`:
  - Add include: `#include "backend_rust/src/graph_node_cxx.rs.h"`
  - Add member variables: `m_outgoing_ptrs`, `m_incoming_ptrs`, `m_coprocess_ptrs` to GraphNode class
  - Add constructor registration: `backend_rust::register_graph_node(...)`
  - Add destructor unregistration: `backend_rust::unregister_graph_node(...)`
  - Add `add_outgoing_ptr()`, `add_incoming_ptr()`, `add_coprocess_ptr()` methods
  - Add `get_outgoing_ptrs()`, `get_incoming_ptrs()`, `get_coprocess_ptrs()` methods
  - Add static helper `ptrs_to_vec()` function
- [ ] Build: `cargo build` 
- [ ] Test C++: Run integration tests
- [ ] Test Rust: `cargo test -p backend_rust`
- [ ] Test QML: `./target/debug/shoopdaloop_dev.sh --self-test`

## Phase 2: Modify HasGraphNode and HasTwoGraphNodes Templates
- [ ] Modify `src/backend/internal/GraphNode.h` - HasGraphNode class:
  - Add registration in `Node` constructor
  - Add unregistration in `Node` destructor
  - Add `ensure_node_registered()` call in `all_graph_nodes()` and `graph_node()`
- [ ] Modify `src/backend/internal/GraphNode.h` - HasTwoGraphNodes class:
  - Add registration in `FirstNode` constructor
  - Add registration in `SecondNode` constructor
  - Add unregistration in destructors
  - Add `ensure_nodes()` call in `all_graph_nodes()`, `first_graph_node()`, `second_graph_node()`
- [ ] Build: `cargo build`
- [ ] Test C++: Run integration tests
- [ ] Test Rust: `cargo test -p backend_rust`
- [ ] Test QML: `./target/debug/shoopdaloop_dev.sh --self-test`

## Phase 3: Modify GraphLoopChannel
- [ ] Modify `src/backend/internal/GraphLoopChannel.cpp`:
  - Include `backend_rust/src/graph_node_cxx.rs.h`
  - Update edge registration in `connect_output_port()`
  - Update edge registration in `disconnect_output_port()`
  - Update edge registration in `disconnect_output_ports()`
  - Update edge registration in `connect_input_port()`
  - Update edge registration in `disconnect_input_port()`
  - Update edge registration in `disconnect_input_ports()`
  - Update edge registration in `disconnect_port()`
- [ ] Modify `src/backend/internal/GraphLoopChannel.cpp`:
  - Update `graph_node_0_incoming_edges()` to sync with Rust registry
  - Update `graph_node_0_outgoing_edges()` to sync with Rust registry
  - Update `graph_node_1_incoming_edges()` to sync with Rust registry
  - Update `graph_node_1_outgoing_edges()` to sync with Rust registry
- [ ] Build: `cargo build`
- [ ] Test C++: Run integration tests
- [ ] Test Rust: `cargo test -p backend_rust`
- [ ] Test QML: `./target/debug/shoopdaloop_dev.sh --self-test`

## Phase 4: Modify GraphPort
- [ ] Modify `src/backend/internal/GraphPort.cpp`:
  - Include `backend_rust/src/graph_node_cxx.rs.h`
  - Update edge registration in `connect_internal()`
  - Update edge registration in destructor if needed
- [ ] Modify `src/backend/internal/GraphPort.cpp`:
  - Update `graph_node_1_incoming_edges()` to sync with Rust registry
  - Update `graph_node_1_outgoing_edges()` to sync with Rust registry
- [ ] Build: `cargo build`
- [ ] Test C++: Run integration tests
- [ ] Test Rust: `cargo test -p backend_rust`
- [ ] Test QML: `./target/debug/shoopdaloop_dev.sh --self-test`

## Phase 5: Modify GraphFXChain
- [ ] Modify `src/backend/internal/GraphFXChain.cpp`:
  - Include `backend_rust/src/graph_node_cxx.rs.h`
  - Update edge registration in constructor after ports are added
  - Update edge registration when ports are added/removed (if dynamic)
- [ ] Modify `src/backend/internal/GraphFXChain.cpp`:
  - Update `graph_node_incoming_edges()` to sync with Rust registry
  - Update `graph_node_outgoing_edges()` to sync with Rust registry
- [ ] Build: `cargo build`
- [ ] Test C++: Run integration tests
- [ ] Test Rust: `cargo test -p backend_rust`
- [ ] Test QML: `./target/debug/shoopdaloop_dev.sh --self-test`

## Phase 6: Wire Up BackendSession
- [ ] Modify `src/backend/internal/BackendSession.cpp`:
  - Change include from `graph_processing_order.h` to `RustGraphProcessingOrder.h`
  - Change function call from `graph_processing_order()` to `graph_processing_order_rust()`
- [ ] Build: `cargo build`
- [ ] Test C++: Run integration tests
- [ ] Test Rust: `cargo test -p backend_rust`
- [ ] Test QML: `./target/debug/shoopdaloop_dev.sh --self-test`

## Phase 7: Deprecate C++ GraphNode
- [ ] Mark `src/backend/internal/GraphNode.h` as deprecated:
  - Add `[[deprecated("Use Rust-backed implementation. Registration happens automatically.")]]` to GraphNode class
- [ ] Verify all code compiles without deprecated warnings (except where explicitly acknowledged)

## Phase 8: Remove C++ GraphNode
- [ ] Remove `src/backend/internal/graph_processing_order.cpp`
- [ ] Remove `src/backend/internal/graph_processing_order.h`
- [ ] Update CMakeLists.txt if needed (remove graph_processing_order.cpp from sources)
- [ ] Build: `cargo build`
- [ ] Test C++: Run integration tests
- [ ] Test Rust: `cargo test -p backend_rust`
- [ ] Test QML: `./target/debug/shoopdaloop_dev.sh --self-test`

## Phase 9: Final Cleanup
- [ ] Format Rust code: `cargo fmt --all`
- [ ] Build with warnings as errors: `RUSTFLAGS="-D warnings" cargo build`
- [ ] Final C++ build check: No warnings
- [ ] Update documentation:
  - Update `todo2.md` to mark all items complete
  - Add notes about the migration in code comments
- [ ] Commit final changes

## Completion Criteria
- [ ] All Rust tests pass
- [ ] All C++ integration tests pass
- [ ] QML self-tests pass
- [ ] No compiler warnings in Rust code (`-D warnings`)
- [ ] No deprecated C++ GraphNode.h in production code
- [ ] `graph_processing_order.cpp` and `graph_processing_order.h` removed
- [ ] Code is formatted with `cargo fmt`
- [ ] Original functionality preserved (all tests pass)

## Test Commands Reference

```bash
# Rust tests
cargo test -p backend_rust

# C++ integration tests (find correct path)
./target/debug/build/backend-*/out/cmake_build/build/test/test_runner

# QML self-tests
./target/debug/shoopdaloop_dev.sh --self-test

# Full build
cargo build

# Build with warnings as errors
cargo fmt --all
RUSTFLAGS="-D warnings" cargo build
```