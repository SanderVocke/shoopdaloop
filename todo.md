# TODO: Port GraphNode to Rust

## Phase 1: Core Types
- [x] Create `src/rust/backend_rust/src/graph_node.rs` with type aliases (WeakGraphNodeSet, SharedGraphNodeSet)
- [x] Define `GraphNodeVirtual` trait with all virtual methods
- [x] Implement `GraphNodeWrapper` struct
- [x] Implement `PROC_process()` with timing callback
- [x] Implement `PROC_co_process()` with timing callback
- [x] Add unit tests for GraphNodeWrapper

## Phase 2: Parent/Child Relationships
- [x] Add `NodeWithParent<T>` wrapper trait in `graph_node.rs`
- [x] Add `graph_node_parent_as<T>()` helper function
- [ ] Add unit tests for parent/child relationships

## Phase 3: Interface Traits
- [x] Add `NotifyProcessParametersInterface` trait
- [x] Add `HasGraphNodesInterface` trait with `all_graph_nodes()` method
- [x] Add `HasGraphNode` trait with lazy `graph_node()` creation
- [x] Add `HasTwoGraphNodes` trait with lazy `first_graph_node()` and `second_graph_node()`
- [x] Add unit tests for interface traits (basic tests pass)

## Phase 4: CXX Bridge
- [x] Create `src/rust/backend_rust/src/graph_node_cxx.rs`
- [x] Expose GraphNodeWrapper types via CXX bridge
- [x] Implement bridge functions for basic methods (name, processing, etc.)
- [x] Create CXX bridge structure
- [ ] Create C++ wrapper header `RustGraphNode.h` that includes generated CXX code (deferred - full integration requires more C++ work)
- [ ] Verify bridge compiles without warnings

## Phase 5: Graph Processing Algorithm
- [x] Port `graph_processing_order()` from C++ to Rust
- [x] Add unit tests for `graph_processing_order()` (2 tests pass)
- [ ] Create CXX bridge for the algorithm (deferred - Vec<Vec<usize>> return type is complex to bridge)
- [ ] Verify algorithm produces same output as C++ version

## Phase 6: Integration
- [x] Run Rust unit tests: `cargo test` (115 tests pass)
- [x] Format code: `cargo fmt --all`
- [x] Build with warnings as errors: `RUSTFLAGS="-D warnings" cargo build` (complete workspace builds)
- [ ] Run C++ integration tests: `./target/debug/test_runner "[GraphConstruct]"` (deferred)
- [ ] Run full test_runner: `./target/debug/test_runner` (deferred)
- [ ] Run QML self-tests: `./target/debug/shoopdaloop_dev.sh --self-test` (deferred)

## Phase 7: Cleanup
- [ ] Remove or deprecate C++ GraphNode implementation (deferred - keep C++ working during transition)
- [ ] Update CMakeLists.txt to ensure Rust lib is linked (already linked)
- [ ] Verify no compiler warnings across full build
- [ ] Document the new Rust implementation in code

## Completion Criteria
- [x] All Rust tests pass (115 tests in backend_rust)
- [ ] All C++ integration tests pass (deferred)
- [ ] QML self-tests pass (deferred)
- [x] No compiler warnings (in Rust code with -D warnings)
- [x] Code is formatted with `cargo fmt`
- [ ] Original C++ functionality preserved and working (deferred)