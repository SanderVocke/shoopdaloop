# TODO: Bridge Rust GraphNode to C++ and Remove C++ Version

**Status: Phase 1 COMPLETE, Phase 2 IN PROGRESS**

## Phase 1: Create C++ Wrapper Header [COMPLETE ✅]
- [x] Study C++ GraphNode.h interface and RustAudioPort.h pattern
- [x] Create `src/backend/internal/RustGraphNode.h`
- [x] Define `RustGraphNode` class wrapping `backend_rust::GraphNodeWrapperCxx`
- [x] Implement `graph_node_outgoing_edges()` returning `WeakGraphNodeSet`
- [x] Implement `graph_node_incoming_edges()` returning `WeakGraphNodeSet`
- [x] Implement `graph_node_co_process_nodes()` returning `WeakGraphNodeSet`
- [x] Implement `graph_node_name()` returning `std::string`
- [x] Implement `graph_node_process(uint32_t nframes)`
- [x] Implement `graph_node_co_process(std::set<GraphNode*> const& nodes, uint32_t nframes)`
- [x] Implement `PROC_process()` and `PROC_co_process()` timing wrappers (via base class)
- [x] Verify RustGraphNode.h compiles without warnings

## Phase 2: Bridge graph_processing_order() [IN PROGRESS]
- [x] Add graph_node_cxx.rs to build.rs bridges list
- [ ] Add CXX bridge for `graph_processing_order()` returning raw node pointers
- [ ] Create C++ wrapper that converts return type
- [ ] Add unit tests for the bridged version
- [ ] Verify algorithm produces same output as C++ version

## Phase 3: Update Existing C++ Code [PENDING]
- [ ] Find all C++ files using GraphNode: `grep -r "GraphNode" src/backend/internal/*.h`
- [ ] Update `GraphLoop` to use Rust-backed type
- [ ] Update `GraphLoopChannel` to use Rust-backed type
- [ ] Update `GraphAudioPort` to use Rust-backed type
- [ ] Update `GraphPort` to use Rust-backed type
- [ ] Update any other classes using the interface
- [ ] Run `cargo build` to verify changes compile

## Phase 4: Deprecate and Remove C++ GraphNode [PENDING]
- [ ] Mark `GraphNode.h` as deprecated with `[[deprecated]]`
- [ ] Update all usages to the new Rust-backed type
- [ ] Remove `GraphNode.h` (after all usages updated)
- [ ] Remove `GraphNode.cpp` if exists
- [ ] Remove `graph_processing_order.cpp` (use Rust version)
- [ ] Update CMakeLists.txt if needed

## Phase 5: Testing [IN PROGRESS]
- [x] Run Rust unit tests: `cargo test -p backend_rust graph_node` (115 tests pass)
- [x] Run C++ integration tests: `./target/debug/test_runner "[GraphConstruct]"`: 3 test cases passed
- [x] Run full test_runner: `./target/debug/test_runner`: 153 test cases, 5910 assertions passed
- [ ] Run QML self-tests: `./target/debug/shoopdaloop_dev.sh --self-test`

## Phase 6: Final Cleanup [PENDING]
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