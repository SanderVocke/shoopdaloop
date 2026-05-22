# Plan: Complete Integration of Rust GraphNode into C++

## Overview

This document describes how to complete the integration of the Rust GraphNode implementation, replacing the C++ GraphNode subsystem entirely.

## Current State

- Rust `graph_processing_order` infrastructure is complete
- C++ `graph_processing_order_rust()` wrapper exists in `RustGraphProcessingOrder.h`
- The C++ version is still used because GraphNode instances aren't registered in the Rust registry
- All 153 C++ tests, 118 Rust tests, and QML self-tests pass

## Integration Strategy

### Core Principle
Each class that implements `GraphNode` (or uses `HasGraphNode`/`HasTwoGraphNodes`) must:
1. **Register** itself in the Rust registry on construction
2. **Unregister** itself on destruction
3. **Update** edge data when connections change

### Registration Function Signature
```cpp
backend_rust::register_graph_node(
    reinterpret_cast<usize>(this),           // Pointer to this node
    graph_node_name(),                         // Node name string
    get_outgoing_ptrs(),                       // Vec<usize> of outgoing edge pointers
    get_incoming_ptrs(),                       // Vec<usize> of incoming edge pointers
    get_coprocess_ptrs()                       // Vec<usize> of coprocess node pointers
);
```

## Classes to Modify

### 1. GraphNode (Base Class)
**File:** `src/backend/internal/GraphNode.h`

**Changes:**
- Add `backend_rust::register_graph_node()` call in constructor
- Add `backend_rust::unregister_graph_node()` call in destructor
- Add helper methods for edge management that sync to Rust registry:
  - `add_outgoing_ptr()`
  - `add_incoming_ptr()`
  - `get_outgoing_ptrs()`
  - `get_incoming_ptrs()`
- Store edge pointers locally (or compute on-the-fly)

**Implementation pattern:**
```cpp
class GraphNode : public shoop_enable_shared_from_this<GraphNode> {
    // ... existing code ...

    // Edge pointers for Rust registry
    std::set<GraphNode*> m_outgoing_ptrs;
    std::set<GraphNode*> m_incoming_ptrs;
    std::set<GraphNode*> m_coprocess_ptrs;

public:
    GraphNode() {
        // Existing init...
        backend_rust::register_graph_node(
            reinterpret_cast<usize>(this),
            graph_node_name(),
            ptrs_to_vec(m_outgoing_ptrs),
            ptrs_to_vec(m_incoming_ptrs),
            ptrs_to_vec(m_coprocess_ptrs)
        );
    }

    virtual ~GraphNode() {
        backend_rust::unregister_graph_node(reinterpret_cast<usize>(this));
    }

    // Override to add edge to Rust registry
    void add_outgoing_ptr(GraphNode* node) {
        m_outgoing_ptrs.insert(node);
        backend_rust::update_graph_node_outgoing(
            reinterpret_cast<usize>(this), ptrs_to_vec(m_outgoing_ptrs));
    }

    // Helper to convert set to vector
    static Vec<usize> ptrs_to_vec(const std::set<GraphNode*>& set);
};
```

### 2. NodeWithParent<Parent> (Template)
**File:** `src/backend/internal/GraphNode.h`

**Changes:**
- Automatically registers with parent pointer
- When parent registers, this node is already included
- No changes needed - registration happens through concrete class

### 3. HasGraphNode (Template Class)
**File:** `src/backend/internal/GraphNode.h`

**Changes:**
- Inner `Node` class needs to register itself
- When `all_graph_nodes()` is called, ensure nodes are registered
- Use `init_graph_node()` method to register on first access

**Implementation pattern:**
```cpp
class HasGraphNode : public HasGraphNodesInterface, public shoop_enable_shared_from_this<HasGraphNode> {
    class Node : public NodeWithParent<HasGraphNode> {
    public:
        Node(shoop_weak_ptr<HasGraphNode> parent) : NodeWithParent<HasGraphNode>(parent) {
            // Register this inner node
            backend_rust::register_graph_node(
                reinterpret_cast<usize>(this),
                graph_node_name(),
                ptrs_to_vec(outgoing_ptrs()),
                ptrs_to_vec(incoming_ptrs()),
                ptrs_to_vec(coprocess_ptrs())
            );
        }
        ~Node() {
            backend_rust::unregister_graph_node(reinterpret_cast<usize>(this));
        }
        // ... existing methods ...
    };

    void ensure_node_registered() {
        if (!m_node) { 
            m_node = shoop_make_shared<Node>(weak_from_this()); 
        }
    }
};
```

### 4. HasTwoGraphNodes (Template Class)
**File:** `src/backend/internal/GraphNode.h`

**Changes:**
- Same pattern as HasGraphNode
- Both `FirstNode` and `SecondNode` inner classes need registration
- Call registration on first access in `ensure_nodes()`

### 5. GraphLoop
**File:** `src/backend/internal/GraphLoop.h`

**Changes:**
- Inherits from `HasGraphNode` (registration handled by inner Node)
- When graph connections change (edges to other loops), update via parent
- Edge management likely handled by channel nodes

### 6. GraphLoopChannel
**File:** `src/backend/internal/GraphLoopChannel.h`

**Changes:**
- Inherits from `HasTwoGraphNodes`
- `graph_node_0_incoming_edges()` returns edges to port preparation nodes
- `graph_node_0_outgoing_edges()` returns edges from port preparation nodes
- `graph_node_1_incoming_edges()` includes loop node
- `graph_node_1_outgoing_edges()` includes connected output port's processing node
- On `connect_output_port()` / `disconnect_port()`, update edge registration

**Key methods to modify:**
```cpp
void GraphLoopChannel::connect_output_port(shoop_shared_ptr<GraphPort> port, bool thread_safe) {
    // ... existing code ...
    
    // Update edge registration for node 1
    if (m_secondnode) {
        auto outgoing = graph_node_1_outgoing_edges();
        backend_rust::update_graph_node_outgoing(
            reinterpret_cast<usize>(m_secondnode.get()), 
            weak_ptrs_to_vec(outgoing));
    }
}

void GraphLoopChannel::disconnect_output_port(shoop_shared_ptr<GraphPort> port, bool thread_safe) {
    // ... existing code ...
    
    // Update edge registration
    if (m_secondnode) {
        auto outgoing = graph_node_1_outgoing_edges();
        backend_rust::update_graph_node_outgoing(
            reinterpret_cast<usize>(m_secondnode.get()),
            weak_ptrs_to_vec(outgoing));
    }
}
```

### 7. GraphPort (Abstract Base)
**File:** `src/backend/internal/GraphPort.h`

**Changes:**
- Inherits from `HasTwoGraphNodes`
- Node 0 (prepare): incoming from other ports' node 1 (processing)
- Node 0 (prepare): outgoing to node 1 (process)
- Node 1 (process): incoming from node 0 and other ports' node 0
- Node 1 (process): outgoing to connected ports' node 0
- On `connect_internal()`, update edge registrations

**Key methods to modify:**
```cpp
void GraphPort::connect_internal(shoop_shared_ptr<GraphPort> other) {
    // ... existing code ...
    
    // Update edge registrations
    // Node 0 of this port gets incoming from node 1 of other port
    // Node 1 of other port gets outgoing to node 0 of this port
    update_edges();
}
```

### 8. GraphAudioPort
**File:** `src/backend/internal/GraphAudioPort.h`

**Changes:**
- Inherits from `GraphPort`
- Registration handled by `GraphPort` (base class)
- Ensure `init_graph_nodes()` is called properly

### 9. GraphMidiPort
**File:** `src/backend/internal/GraphMidiPort.h`

**Changes:**
- Inherits from `GraphPort`
- Same as GraphAudioPort

### 10. GraphFXChain
**File:** `src/backend/internal/GraphFXChain.h`

**Changes:**
- Inherits from `HasGraphNode`
- `graph_node_incoming_edges()` returns edges from input ports' node 1
- `graph_node_outgoing_edges()` returns edges to output ports' node 0
- On port connection changes, update edge registration

## Execution Order

### Phase 1: Modify Base Classes (Must be first!)
1. **GraphNode.h**: Add registration infrastructure
2. **GraphNode.h**: Modify HasGraphNode and HasTwoGraphNodes
3. Build and test

### Phase 2: Modify Concrete Classes
4. **GraphLoop.h/.cpp**: Already handled via HasGraphNode
5. **GraphLoopChannel.h/.cpp**: Handle explicit edge updates
6. **GraphPort.h/.cpp**: Handle explicit edge updates
7. **GraphFXChain.h/.cpp**: Handle explicit edge updates
8. Build and test

### Phase 3: Wire Up BackendSession
9. **BackendSession.cpp**: Include RustGraphProcessingOrder.h
10. **BackendSession.cpp**: Use `graph_processing_order_rust()` instead of `graph_processing_order()`
11. Build and test

### Phase 4: Deprecate and Remove C++
12. Mark GraphNode.h as deprecated
13. Remove graph_processing_order.cpp and graph_processing_order.h
14. Full test suite

## Helper Function Needed

Add to a utility header or directly in GraphNode.h:
```cpp
// Convert set<GraphNode*> to Vec<usize>
template<typename T>
Vec<usize> graph_node_ptrs_to_vec(const std::set<T*>& ptrs) {
    Vec<usize> result;
    result.reserve(ptrs.size());
    for (auto* p : ptrs) {
        result.push_back(reinterpret_cast<usize>(p));
    }
    return result;
}

// Convert set<weak_ptr<GraphNode>> to Vec<usize>
Vec<usize> graph_node_weak_ptrs_to_vec(const WeakGraphNodeSet& ptrs) {
    Vec<usize> result;
    result.reserve(ptrs.size());
    for (auto& wp : ptrs) {
        if (auto sp = wp.lock()) {
            result.push_back(reinterpret_cast<usize>(sp.get()));
        }
    }
    return result;
}
```

## Build and Test After Each Phase

1. `cargo build`
2. `cargo test -p backend_rust`
3. `./target/debug/build/backend-*/out/cmake_build/build/test/test_runner`
4. `./target/debug/shoopdaloop_dev.sh --self-test`

## Files to Delete

After full integration:
- `src/backend/internal/GraphNode.h` (replaced by RustGraphNode.h with minimal C++ shim)
- `src/backend/internal/graph_processing_order.cpp` (use Rust version)
- `src/backend/internal/graph_processing_order.h` (use RustGraphProcessingOrder.h)

## Dependencies

- `backend_rust/src/graph_node_cxx.rs.h` (generated by CXX)
- No new dependencies required