#include "GraphPort.h"
#include <memory>
#include <stdexcept>
#include <algorithm>
#include "BackendSession.h"
#include <fmt/format.h>

using namespace shoop_types;
using namespace shoop_constants;

GraphPort::GraphPort (shoop_shared_ptr<BackendSession> const& backend) :
    m_passthrough_enabled(true),
    backend(shoop_weak_ptr<BackendSession>(backend)) {}

BackendSession &GraphPort::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
}

void GraphPort::connect_internal(const shoop_shared_ptr<GraphPort> &other) {
    for (auto &_other : mp_internal_port_connections) {
        if(auto __other = _other.lock()) {
            if (__other.get() == other.get()) { return; } // already connected
        }
    }
    log<log_level_debug>("connect internally: {} -> {}", get_port().name(), other->get_port().name());
    mp_internal_port_connections.push_back(shoop_static_pointer_cast<GraphPort>(other));
    get_backend().set_graph_node_changes_pending();
}


void GraphPort::PROC_notify_changed_buffer_size(uint32_t buffer_size) {}

void GraphPort::graph_node_0_process(uint32_t nframes) {
    get_port().PROC_prepare(nframes);
}

void GraphPort::graph_node_1_process(uint32_t nframes) {
    get_port().PROC_process(nframes);
    PROC_internal_connections(nframes);
}

WeakGraphNodeSet GraphPort::graph_node_1_incoming_edges() {
    WeakGraphNodeSet rval;
    // Ensure we execute after our first node
    rval.insert(first_graph_node());
    for (auto &other : mp_internal_port_connections) {
        if(auto _other = other.lock()) {
            // ensure we execute after other port has
            // prepared buffers
            auto shared = _other->first_graph_node();
            rval.insert(shared);
        }
    }
    return rval;
}

WeakGraphNodeSet GraphPort::graph_node_1_outgoing_edges() {
    WeakGraphNodeSet rval;
    for (auto &other : mp_internal_port_connections) {
        if(auto _other = other.lock()) {
            auto shared = _other->second_graph_node();
            rval.insert(shared);
        }
    }
    return rval;
}