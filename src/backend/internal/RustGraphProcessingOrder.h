#pragma once

/**
 * RustGraphProcessingOrder - C++ wrapper around Rust graph_processing_order
 * 
 * This provides a wrapper that calls the Rust implementation while maintaining
 * the same interface as the original C++ version.
 * 
 * NOTE: The graph_processing_order function in BackendSession.cpp still uses
 * the original C++ implementation. This header provides the Rust version
 * as graph_processing_order_rust() for testing/integration when ready.
 * 
 * To switch to Rust implementation:
 * 1. Include this header in BackendSession.cpp
 * 2. Change graph_processing_order() call to graph_processing_order_rust()
 * 3. Ensure all GraphNode instances are registered via register_graph_node()
 */

#include "GraphNode.h"
#include "shoop_shared_ptr.h"
#include "backend_rust/src/graph_node_cxx.rs.h"

#include <vector>
#include <set>
#include <cstdint>

/**
 * Compute processing order using Rust implementation.
 * 
 * This function wraps the Rust compute_graph_processing_order() function,
 * converting between C++ and Rust data types.
 * 
 * NOTE: This requires nodes to be registered first via:
 *   backend_rust::register_graph_node(ptr, name, outgoing, incoming, coprocess)
 * 
 * @param nodes Set of GraphNode pointers to compute processing order for
 * @return Vector of sets, where each set contains nodes that should be co-processed
 */
inline std::vector<std::set<GraphNode*>> graph_processing_order_rust(std::set<GraphNode*> nodes) {
    std::vector<std::set<GraphNode*>> result;
    
    // Convert C++ node pointers to Rust-usable format (rust::Vec<usize>)
    rust::Vec<std::size_t> input_ptrs;
    input_ptrs.reserve(nodes.size());
    for (auto* node : nodes) {
        input_ptrs.push_back(reinterpret_cast<std::size_t>(node));
    }
    
    // Call Rust implementation
    auto flat_result = backend_rust::compute_graph_processing_order(input_ptrs);
    
    // Convert flat result back to groups
    std::set<GraphNode*> current_group;
    for (auto ptr : flat_result) {
        if (ptr == std::numeric_limits<std::size_t>::max()) {
            // Separator - end of group
            if (!current_group.empty()) {
                result.push_back(current_group);
                current_group.clear();
            }
        } else {
            // Node pointer
            auto* node = reinterpret_cast<GraphNode*>(ptr);
            current_group.insert(node);
        }
    }
    
    // Add remaining group if any
    if (!current_group.empty()) {
        result.push_back(current_group);
    }
    
    return result;
}