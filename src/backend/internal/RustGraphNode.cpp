#include "RustGraphNode.h"

#include <stdexcept>
#include <algorithm>

RustGraphNode::RustGraphNode() {
    m_rust = backend_rust::new_graph_node_wrapper_cxx();
}

RustGraphNode::RustGraphNode(const std::string& name) {
    m_rust = backend_rust::new_named_graph_node_wrapper_cxx(name);
}

RustGraphNode::~RustGraphNode() = default;

std::string RustGraphNode::graph_node_name() const {
    if (!m_rust.has_value()) {
        return "GraphNode";
    }
    return std::string(backend_rust::graph_node_wrapper_cxx_name(**m_rust));
}

void RustGraphNode::graph_node_process(uint32_t nframes) {
    if (!m_rust.has_value()) {
        return;
    }
    backend_rust::PROC_process_graph_node(**m_rust, nframes);
}

void RustGraphNode::graph_node_co_process(const std::set<shoop_shared_ptr<GraphNode>>& nodes,
                                           uint32_t nframes) {
    // For now, just process this node
    // Full co-processing implementation would require more complex bridging
    graph_node_process(nframes);
}

WeakGraphNodeSet RustGraphNode::graph_node_outgoing_edges() {
    return m_outgoing;
}

WeakGraphNodeSet RustGraphNode::graph_node_incoming_edges() {
    return m_incoming;
}

WeakGraphNodeSet RustGraphNode::graph_node_co_process_nodes() {
    return m_coprocess_nodes;
}

void RustGraphNode::add_outgoing_edge(shoop_shared_ptr<GraphNode> node) {
    m_outgoing.insert(node);
}

void RustGraphNode::add_incoming_edge(shoop_shared_ptr<GraphNode> node) {
    m_incoming.insert(node);
}

void RustGraphNode::add_coprocess_node(shoop_shared_ptr<GraphNode> node) {
    m_coprocess_nodes.insert(node);
}

void RustGraphNode::clear_edges() {
    m_outgoing.clear();
    m_incoming.clear();
    m_coprocess_nodes.clear();
}