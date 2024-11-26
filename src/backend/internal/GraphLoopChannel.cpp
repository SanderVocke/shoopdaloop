#include "GraphLoopChannel.h"
#include "GraphPort.h"
#include "ChannelInterface.h"
#include "GraphLoop.h"
#include "AudioChannel.h"
#include "AudioPort.h"
#include "MidiChannel.h"
#include "DummyMidiBufs.h"
#include "MidiPort.h"
#include "types.h"
#include <stdexcept>
#include <algorithm>
#include <BackendSession.h>
#include <shoop_globals.h>

using namespace shoop_types;
using namespace shoop_constants;

namespace {
std::vector<audio_sample_t> g_dummy_audio_input_buffer (default_audio_dummy_buffer_size);
std::vector<audio_sample_t> g_dummy_audio_output_buffer (default_audio_dummy_buffer_size);
shoop_shared_ptr<DummyReadMidiBuf> g_dummy_midi_input_buffer = shoop_make_shared<DummyReadMidiBuf>();
shoop_shared_ptr<DummyWriteMidiBuf> g_dummy_midi_output_buffer = shoop_make_shared<DummyWriteMidiBuf>();
}

GraphLoopChannel::GraphLoopChannel(shoop_shared_ptr<ChannelInterface> chan,
                shoop_shared_ptr<GraphLoop> loop,
                shoop_shared_ptr<BackendSession> backend) :
        ModuleLoggingEnabled<"Backend.GraphLoopChannel">(),
        channel(chan),
        loop(loop),
        backend(backend)
{
    ma_data_sequence_nr = 0;
}

GraphLoopChannel &GraphLoopChannel::operator= (GraphLoopChannel const& other) {
        if (backend.lock() != other.backend.lock()) {
            throw std::runtime_error("Cannot copy channels between back-ends");
        }
        loop = other.loop;
        channel = other.channel;
        mp_input_port_mapping = other.mp_input_port_mapping;
        mp_output_port_mapping = other.mp_output_port_mapping;
        return *this;
    }

void GraphLoopChannel::clear_data_dirty() {
    ma_data_sequence_nr = channel->get_data_seq_nr();
}

bool GraphLoopChannel::get_data_dirty() const {
    return ma_data_sequence_nr != channel->get_data_seq_nr();
}

void GraphLoopChannel::connect_output_port(shoop_shared_ptr<GraphPort> port, bool thread_safe) {
    mp_output_port_mapping = port;
    get_backend().set_graph_node_changes_pending();
}

void GraphLoopChannel::connect_input_port(shoop_shared_ptr<GraphPort> port, bool thread_safe) {
    mp_input_port_mapping = port;
    get_backend().set_graph_node_changes_pending();
}

void GraphLoopChannel::disconnect_output_port(shoop_shared_ptr<GraphPort> port, bool thread_safe) {
    auto locked = mp_output_port_mapping.lock();
    if (locked) {
        if (port != locked) {
            throw std::runtime_error("Attempting to disconnect unconnected output");
        }
    }
    mp_output_port_mapping.reset();
    get_backend().set_graph_node_changes_pending();
}

void GraphLoopChannel::disconnect_port(shoop_shared_ptr<GraphPort> port, bool thread_safe) {
    auto in_locked = mp_input_port_mapping.lock();
    auto out_locked = mp_output_port_mapping.lock();
    if (in_locked && in_locked == port) {
        mp_input_port_mapping.reset();
    } else if (out_locked && out_locked == port) {
        mp_output_port_mapping.reset();
    } else {
        throw std::runtime_error("Attempting to disconnect unconnected port");
    }
}

void GraphLoopChannel::disconnect_output_ports(bool thread_safe) {
    mp_output_port_mapping.reset();
    get_backend().set_graph_node_changes_pending();
}

void GraphLoopChannel::disconnect_input_port(shoop_shared_ptr<GraphPort> port, bool thread_safe) {
    auto locked = mp_input_port_mapping.lock();
    if (locked) {
        if (port != locked) {
            throw std::runtime_error("Attempting to disconnect unconnected input");
        }
    }
    mp_input_port_mapping.reset();
    get_backend().set_graph_node_changes_pending();
}

void GraphLoopChannel::disconnect_input_ports(bool thread_safe) {
    mp_input_port_mapping.reset();
    get_backend().set_graph_node_changes_pending();
}

void GraphLoopChannel::PROC_prepare(uint32_t n_frames) {
    log<log_level_debug_trace>("prepare ({} frames)", n_frames);
    auto in_locked = mp_input_port_mapping.lock();
    auto out_locked = mp_output_port_mapping.lock();

    if (maybe_audio()) {
        auto in_locked_audio = in_locked ? in_locked->maybe_audio_port() : nullptr;
        auto out_locked_audio = out_locked ? out_locked->maybe_audio_port() : nullptr;
        if (in_locked && in_locked_audio) {
            auto chan = dynamic_cast<LoopAudioChannel*>(channel.get());
            auto buf = in_locked_audio->PROC_get_buffer(n_frames);
            log<log_level_debug_trace>("set recording buffer from input ({} samples)", n_frames);
            chan->PROC_set_recording_buffer(buf, n_frames);
        } else {
            if (g_dummy_audio_input_buffer.size() < n_frames*sizeof(audio_sample_t)) {
                g_dummy_audio_input_buffer.resize(n_frames*sizeof(audio_sample_t));
                memset((void*)g_dummy_audio_input_buffer.data(), 0, n_frames*sizeof(audio_sample_t));
            }
            auto chan = dynamic_cast<LoopAudioChannel*>(channel.get());
            log<log_level_debug_trace>("set recording buffer from fallback ({} samples)", n_frames);
            chan->PROC_set_recording_buffer(g_dummy_audio_input_buffer.data(), n_frames);
        }
        if (out_locked && out_locked_audio) {
            auto chan = dynamic_cast<LoopAudioChannel*>(channel.get());
            log<log_level_debug_trace>("set playback buffer from output ({} samples)", n_frames);
            chan->PROC_set_playback_buffer(out_locked_audio->PROC_get_buffer(n_frames), n_frames);
        } else {
            if (g_dummy_audio_output_buffer.size() < n_frames*sizeof(audio_sample_t)) {
                g_dummy_audio_output_buffer.resize(n_frames*sizeof(audio_sample_t));
                memset((void*)g_dummy_audio_output_buffer.data(), 0, n_frames*sizeof(audio_sample_t));
            }
            auto chan = dynamic_cast<LoopAudioChannel*>(channel.get());
            log<log_level_debug_trace>("set playback buffer from fallback ({} samples)", n_frames);
            chan->PROC_set_playback_buffer(g_dummy_audio_output_buffer.data(), n_frames);
        }
    } else if (maybe_midi()) {
        auto in_locked_midi = in_locked ? in_locked->maybe_midi_port() : nullptr;
        auto out_locked_midi = out_locked ? out_locked->maybe_midi_port() : nullptr;
        if (in_locked && in_locked_midi) {
            auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
            chan->PROC_set_recording_buffer(in_locked_midi->PROC_get_read_output_data_buffer(n_frames), n_frames);
        } else {
            auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
            chan->PROC_set_recording_buffer(g_dummy_midi_input_buffer.get(), n_frames);
        }
        if (out_locked && out_locked_midi) {
            auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
            chan->PROC_set_playback_buffer(out_locked_midi->PROC_get_write_data_into_port_buffer(n_frames), n_frames);
        } else {
            auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
            chan->PROC_set_playback_buffer(g_dummy_midi_output_buffer.get(), n_frames);
        }
    }
}

LoopAudioChannel *GraphLoopChannel::maybe_audio() {
    return dynamic_cast<LoopAudioChannel*>(channel.get());
}

LoopMidiChannel *GraphLoopChannel::maybe_midi() {
    return dynamic_cast<LoopMidiChannel*>(channel.get());
}

BackendSession &GraphLoopChannel::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
}

void GraphLoopChannel::graph_node_0_process(uint32_t nframes) {
    PROC_prepare(nframes);
}

WeakGraphNodeSet GraphLoopChannel::graph_node_0_incoming_edges() {
    WeakGraphNodeSet rval;
    if (auto in_locked = mp_input_port_mapping.lock()) {
        rval.insert(shoop_static_pointer_cast<GraphNode>(in_locked->first_graph_node()));
    }
    if (auto out_locked = mp_output_port_mapping.lock()) {
        rval.insert(shoop_static_pointer_cast<GraphNode>(out_locked->first_graph_node()));
    }
    return rval;
}

WeakGraphNodeSet GraphLoopChannel::graph_node_0_outgoing_edges() {
    WeakGraphNodeSet rval;
    if (auto l = loop.lock()) {
        rval.insert(shoop_static_pointer_cast<GraphNode>(l->graph_node()));
    }
    return rval;
}

void GraphLoopChannel::PROC_process(uint32_t nframes) {
    channel->PROC_finalize_process();
}

void GraphLoopChannel::graph_node_1_process(uint32_t nframes) {
    PROC_process(nframes);
}

WeakGraphNodeSet GraphLoopChannel::graph_node_1_incoming_edges() {
    WeakGraphNodeSet rval;
    rval.insert(first_graph_node());
    if (auto l = loop.lock()) {
        rval.insert(shoop_static_pointer_cast<GraphNode>(l->graph_node()));
    }
    if (auto in_locked = mp_input_port_mapping.lock()) {
        log<log_level_debug_trace>("found incoming edge from port node {}", in_locked->graph_node_1_name());
        rval.insert(shoop_static_pointer_cast<GraphNode>(in_locked->second_graph_node()));
    } else {
        log<log_level_debug_trace>("found no incoming edge to any port node");
    }
    return rval;
}

WeakGraphNodeSet GraphLoopChannel::graph_node_1_outgoing_edges() {
    WeakGraphNodeSet rval;
    if (auto out_locked = mp_output_port_mapping.lock()) {
        log<log_level_debug_trace>("found outgoing edge to port node {}", out_locked->graph_node_1_name());
        rval.insert(shoop_static_pointer_cast<GraphNode>(out_locked->second_graph_node()));
    } else {
        log<log_level_debug_trace>("found no outgoing edge to any port node");
    }
    return rval;
}

void GraphLoopChannel::adopt_ringbuffer_contents(
    std::optional<unsigned> reverse_start_offset,
    std::optional<unsigned> keep_before_start_offset,
    bool thread_safe) {
    if (auto input = mp_input_port_mapping.lock()) {
        if (channel) {
            channel->adopt_ringbuffer_contents(input->maybe_shared_port(), reverse_start_offset, keep_before_start_offset, thread_safe);
        }
    }
}