#include "GraphPort.h"
#include "AudioPort.h"
#include "MidiPort.h"
#include "InternalAudioPort.h"
#include <memory>
#include <stdexcept>
#include <algorithm>
#include "MidiStateTracker.h"
#include "MidiSortingBuffer.h"
#include "BackendSession.h"
#include "DummyAudioMidiDriver.h"
#include "shoop_globals.h"

#ifdef SHOOP_HAVE_LV2
#include "InternalLV2MidiOutputPort.h"
#endif

using namespace shoop_types;
using namespace shoop_constants;

GraphPort::GraphPort (std::shared_ptr<BackendSession> const& backend) :
    passthrough_enabled(true),
    backend(backend) {}

BackendSession &GraphPort::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
}

void GraphPort::connect_passthrough(const std::shared_ptr<GraphPort> &other) {
    log_trace();
    for (auto &_other : mp_passthrough_to) {
        if(auto __other = _other.lock()) {
            if (__other.get() == other.get()) { return; } // already connected
        }
    }
    mp_passthrough_to.push_back(other);
    get_backend().recalculate_processing_schedule();
}


void GraphPort::PROC_notify_changed_buffer_size(uint32_t buffer_size) {}

void GraphPort::graph_node_0_process(uint32_t nframes) {
    get_port()->PROC_prepare(nframes);
}

void GraphPort::graph_node_1_process(uint32_t nframes) {
    get_port()->PROC_process(nframes);
    PROC_passthrough(nframes);
}

WeakGraphNodeSet GraphPort::graph_node_1_incoming_edges() {
    WeakGraphNodeSet rval;
    // Ensure we execute after our first node
    rval.insert(first_graph_node());
    for (auto &other : mp_passthrough_to) {
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
    for (auto &other : mp_passthrough_to) {
        if(auto _other = other.lock()) {
            auto shared = _other->second_graph_node();
            rval.insert(shared);
        }
    }
    return rval;
}