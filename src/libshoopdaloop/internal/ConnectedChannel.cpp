#include "ConnectedChannel.h"
#include "ConnectedPort.h"
#include "ChannelInterface.h"
#include "ConnectedLoop.h"
#include "AudioChannel.h"
#include "MidiChannel.h"
#include "DummyMidiBufs.h"
#include "MidiSortingBuffer.h"
#include "MidiPort.h"
#include <stdexcept>
#include <algorithm>
#include <BackendSession.h>

using namespace shoop_types;
using namespace shoop_constants;

namespace {
std::vector<audio_sample_t> g_dummy_audio_input_buffer (default_audio_dummy_buffer_size);
std::vector<audio_sample_t> g_dummy_audio_output_buffer (default_audio_dummy_buffer_size);
std::shared_ptr<DummyReadMidiBuf> g_dummy_midi_input_buffer = std::make_shared<DummyReadMidiBuf>();
std::shared_ptr<DummyWriteMidiBuf> g_dummy_midi_output_buffer = std::make_shared<DummyWriteMidiBuf>();
}

ConnectedChannel::ConnectedChannel(std::shared_ptr<ChannelInterface> chan,
                std::shared_ptr<ConnectedLoop> loop,
                std::shared_ptr<BackendSession> backend) :
        channel(chan),
        loop(loop),
        backend(backend)
{
    ma_data_sequence_nr = 0;
}

ConnectedChannel &ConnectedChannel::operator= (ConnectedChannel const& other) {
        if (backend.lock() != other.backend.lock()) {
            throw std::runtime_error("Cannot copy channels between back-ends");
        }
        loop = other.loop;
        channel = other.channel;
        mp_input_port_mapping = other.mp_input_port_mapping;
        mp_output_port_mapping = other.mp_output_port_mapping;
        return *this;
    }

void ConnectedChannel::clear_data_dirty() {
    ma_data_sequence_nr = channel->get_data_seq_nr();
}

bool ConnectedChannel::get_data_dirty() const {
    return ma_data_sequence_nr != channel->get_data_seq_nr();
}

void ConnectedChannel::connect_output_port(std::shared_ptr<ConnectedPort> port, bool thread_safe) {
    mp_output_port_mapping = port;
    get_backend().recalculate_processing_schedule(thread_safe);
}

void ConnectedChannel::connect_input_port(std::shared_ptr<ConnectedPort> port, bool thread_safe) {
    mp_input_port_mapping = port;
    get_backend().recalculate_processing_schedule(thread_safe);
}

void ConnectedChannel::disconnect_output_port(std::shared_ptr<ConnectedPort> port, bool thread_safe) {
    auto locked = mp_output_port_mapping.lock();
    if (locked) {
        if (port != locked) {
            throw std::runtime_error("Attempting to disconnect unconnected output");
        }
    }
    mp_output_port_mapping.reset();
    get_backend().recalculate_processing_schedule(thread_safe);
}

void ConnectedChannel::disconnect_output_ports(bool thread_safe) {
    mp_output_port_mapping.reset();
    get_backend().recalculate_processing_schedule(thread_safe);
}

void ConnectedChannel::disconnect_input_port(std::shared_ptr<ConnectedPort> port, bool thread_safe) {
    auto locked = mp_input_port_mapping.lock();
    if (locked) {
        if (port != locked) {
            throw std::runtime_error("Attempting to disconnect unconnected input");
        }
    }
    mp_input_port_mapping.reset();
    get_backend().recalculate_processing_schedule(thread_safe);
}

void ConnectedChannel::disconnect_input_ports(bool thread_safe) {
    mp_input_port_mapping.reset();
    get_backend().recalculate_processing_schedule(thread_safe);
}

void ConnectedChannel::PROC_prepare_process_audio(uint32_t n_frames) {
    auto in_locked = mp_input_port_mapping.lock();
    if (in_locked) {
        auto chan = dynamic_cast<LoopAudioChannel*>(channel.get());
        chan->PROC_set_recording_buffer(in_locked->maybe_audio_buffer, n_frames);
    } else {
        if (g_dummy_audio_input_buffer.size() < n_frames*sizeof(audio_sample_t)) {
            g_dummy_audio_input_buffer.resize(n_frames*sizeof(audio_sample_t));
            memset((void*)g_dummy_audio_input_buffer.data(), 0, n_frames*sizeof(audio_sample_t));
        }
        auto chan = dynamic_cast<LoopAudioChannel*>(channel.get());
        chan->PROC_set_recording_buffer(g_dummy_audio_input_buffer.data(), n_frames);
    }
    auto out_locked = mp_output_port_mapping.lock();
    if (out_locked) {
        auto chan = dynamic_cast<LoopAudioChannel*>(channel.get());
        chan->PROC_set_playback_buffer(out_locked->maybe_audio_buffer, n_frames);
    } else {
        if (g_dummy_audio_output_buffer.size() < n_frames*sizeof(audio_sample_t)) {
            g_dummy_audio_output_buffer.resize(n_frames*sizeof(audio_sample_t));
            memset((void*)g_dummy_audio_output_buffer.data(), 0, n_frames*sizeof(audio_sample_t));
        }
        auto chan = dynamic_cast<LoopAudioChannel*>(channel.get());
        chan->PROC_set_playback_buffer(g_dummy_audio_output_buffer.data(), n_frames);
    }
}


void ConnectedChannel::PROC_prepare_process_midi(uint32_t n_frames) {
    auto in_locked = mp_input_port_mapping.lock();
    if (in_locked) {
        auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
        chan->PROC_set_recording_buffer(in_locked->maybe_midi_output_buffer, n_frames);
    } else {
        auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
        chan->PROC_set_recording_buffer(g_dummy_midi_input_buffer.get(), n_frames);
    }
    auto out_locked = mp_output_port_mapping.lock();
    if (out_locked) {
        auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
        chan->PROC_set_playback_buffer(out_locked->maybe_midi_output_merging_buffer.get(), n_frames);
    } else {
        auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
        chan->PROC_set_playback_buffer(g_dummy_midi_output_buffer.get(), n_frames);
    }
}

LoopAudioChannel *ConnectedChannel::maybe_audio() {
    return dynamic_cast<LoopAudioChannel*>(channel.get());
}

LoopMidiChannel *ConnectedChannel::maybe_midi() {
    return dynamic_cast<LoopMidiChannel*>(channel.get());
}

BackendSession &ConnectedChannel::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
}

void ConnectedChannel::graph_node_0_process(uint32_t nframes) {
    if (maybe_audio()) {
        PROC_prepare_process_audio(nframes);
    } else if (maybe_midi()) {
        PROC_prepare_process_midi(nframes);
    }
}

WeakGraphNodeSet ConnectedChannel::graph_node_0_incoming_edges() {
    WeakGraphNodeSet rval;
    if (auto in_locked = mp_input_port_mapping.lock()) {
        rval.insert(in_locked->first_graph_node());
    }
    if (auto out_locked = mp_output_port_mapping.lock()) {
        rval.insert(out_locked->first_graph_node());
    }
    return rval;
}

WeakGraphNodeSet ConnectedChannel::graph_node_0_outgoing_edges() {
    WeakGraphNodeSet rval;
    if (auto l = loop.lock()) {
        rval.insert(l->graph_node());
    }
    return rval;
}

void ConnectedChannel::graph_node_1_process(uint32_t nframes) {
    channel->PROC_finalize_process();
}

WeakGraphNodeSet ConnectedChannel::graph_node_1_incoming_edges() {
    WeakGraphNodeSet rval;
    rval.insert(first_graph_node());
    if (auto l = loop.lock()) {
        rval.insert(l->graph_node());
    }
    if (auto in_locked = mp_input_port_mapping.lock()) {
        rval.insert(in_locked->second_graph_node());
    }
    return rval;
}

WeakGraphNodeSet ConnectedChannel::graph_node_1_outgoing_edges() {
    WeakGraphNodeSet rval;
    if (auto out_locked = mp_output_port_mapping.lock()) {
        rval.insert(out_locked->second_graph_node());
    }
    return rval;
}