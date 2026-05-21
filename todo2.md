# TODO: Bridge Rust GraphNode to C++ and Remove C++ Version

## Phase 1: Create C++ Wrapper Header
- [ ] Create `src/backend/internal/RustGraphNode.h`
- [ ] Define `RustGraphNode` class wrapping `backend_rust::GraphNodeWrapperCxx`
- [ ] Implement `graph_node_outgoing_edges()` returning `WeakGraphNodeSet`
- [ ] Implement `graph_node_incoming_edges()` returning `WeakGraphNodeSet`
- [ ] Implement `graph_node_co_process_nodes()` returning `WeakGraphNodeSet`
- [ ] Implement `graph_node_name()` returning `std::string`
- [ ] Implement `graph_node_process(uint32_t nframes)`
- [ ] Implement `graph_node_co_process(std::set<GraphNode*> const& nodes, uint32_t nframes)`
- [ ] Implement `PROC_process()` and `PROC_co_process()` timing wrappers
- [ ] Verify RustGraphNode.h compiles without warnings

## Phase 2: Bridge graph_processing_order()
- [ ] Add CXX bridge for `graph_processing_order()` returning raw node pointers
- [ ] Create C++ wrapper that converts return type
- [ ] Add unit tests for the bridged version
- [ ] Verify algorithm produces same output as C++ version

## Phase 3: Update Existing C++ Code
- [ ] Find all C++ files using GraphNode: `grep -r "GraphNode" src/backend/internal/*.h`
- [ ] Update `GraphLoop` to use Rust-backed type
- [ ] Update `GraphLoopChannel` to use Rust-backed type
- [ ] Update `GraphAudioPort` to use Rust-backed type
- [ ] Update `GraphPort` to use Rust-backed type
- [ ] Update any other classes using the interface
- [ ] Run `cargo build` to verify changes compile

## Phase 4: Deprecate and Remove C++ GraphNode
- [ ] Mark `GraphNode.h` as deprecated with `[[deprecated]]`
- [ ] Update all usages to the new Rust-backed type
- [ ] Remove `GraphNode.h` (after all usages updated)
- [ ] Remove `GraphNode.cpp` if exists
- [ ] Remove `graph_processing_order.cpp` (use Rust version)
- [ ] Update CMakeLists.txt if needed

## Phase 5: Testing
- [ ] Run Rust unit tests: `cargo test -p backend_rust graph_node`
- [ ] Run C++ integration tests: `./target/debug/test_runner "[GraphConstruct]"`
- [ ] Run full test_runner: `./target/debug/test_runner`
- [ ] Run QML self-tests: `./target/debug/shoopdaloop_dev.sh --self-test`

## Phase 6: Final Cleanup
- [ ] Format code: `cargo fmt --all`
- [ ] Build with warnings as errors: `RUSTFLAGS="-D warnings" cargo build`
- [ ] Verify no compiler warnings in C++ code
- [ ] Document the changes in code comments

## Completion Criteria
- [ ] All Rust tests pass
- [ ] All C++ integration tests pass
- [ ] QML self-tests pass
- [ ] No compiler warnings
- [ ] Code is formatted with `cargo fmt`
- [ ] C++ GraphNode implementation removed
- [ ] Original C++ functionality preserved and working