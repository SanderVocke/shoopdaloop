#pragma once

/**
 * RustGraphNode - C++ wrapper around Rust GraphNode for C++ interop
 * 
 * This provides a bridge between C++ GraphNode interface and the Rust implementation.
 * Uses the CXX bridge to communicate with the Rust side.
 * 
 * Key design:
 * - RustGraphNode implements the full GraphNode interface
 * - Delegates to Rust GraphNodeWrapperCxx for the actual implementation
 * - Edge management uses raw pointers which are bridged to Rust
 */

#include "GraphNode.h"
#include "shoop_shared_ptr.h"
#include "backend_rust/src/graph_node_cxx.rs.h"

#include <cstdint>
#include <string>
#include <memory>
#include <optional>
#include <chrono>

/**
 * RustGraphNode - GraphNode backed by Rust implementation
 * 
 * This wraps the Rust GraphNodeWrapperCxx and provides:
 * - Full GraphNode interface implementation
 * - Edge management (outgoing/incoming/co-process)
 * - Process timing with callback
 */
class RustGraphNode : public GraphNode {
public:
    using NodePtr = backend_rust::GraphNodeWrapperCxx;
    
protected:
    // Rust implementation - wrapped in optional since rust::Box has no default constructor
    std::optional<rust::Box<NodePtr>> m_rust;
    
    // C++ side storage for edge pointers (weak references to other GraphNodes)
    // Use std::owner_less for proper weak_ptr comparison (matches WeakGraphNodeSet definition)
    std::set<shoop_weak_ptr<GraphNode>, std::owner_less<shoop_weak_ptr<GraphNode>>> m_outgoing;
    std::set<shoop_weak_ptr<GraphNode>, std::owner_less<shoop_weak_ptr<GraphNode>>> m_incoming;
    std::set<shoop_weak_ptr<GraphNode>, std::owner_less<shoop_weak_ptr<GraphNode>>> m_coprocess_nodes;

public:
    /**
     * Create a new RustGraphNode with default settings
     */
    RustGraphNode();
    
    /**
     * Create a new RustGraphNode with a name
     */
    explicit RustGraphNode(const std::string& name);
    
    virtual ~RustGraphNode();

    // GraphNode interface implementation
    
    WeakGraphNodeSet graph_node_outgoing_edges() override;
    WeakGraphNodeSet graph_node_incoming_edges() override;
    WeakGraphNodeSet graph_node_co_process_nodes() override;
    
    std::string graph_node_name() const override;
    
    void graph_node_process(uint32_t nframes) override;
    
    void graph_node_co_process(const std::set<shoop_shared_ptr<GraphNode>>& nodes,
                               uint32_t nframes) override;
    
    // Edge management - for adding edges from C++ side
    void add_outgoing_edge(shoop_shared_ptr<GraphNode> node);
    void add_incoming_edge(shoop_shared_ptr<GraphNode> node);
    void add_coprocess_node(shoop_shared_ptr<GraphNode> node);
    
    void clear_edges();
    
    // Timing callback - inherited from GraphNode
    // Uses m_processed_cb from base class

protected:
    // Helper to sync edges to Rust side
    void sync_edges_to_rust();
};

// Type alias for convenience
using _RustGraphNode = RustGraphNode;