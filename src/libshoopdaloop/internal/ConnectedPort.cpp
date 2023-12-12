#include "ConnectedPort.h"
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

ConnectedPort::ConnectedPort (std::shared_ptr<PortInterface> const& port,
                              std::shared_ptr<BackendSession> const& backend) :
    port(port),
    maybe_audio_buffer(nullptr),
    maybe_midi_input_buffer(nullptr),
    maybe_midi_output_buffer(nullptr),
    maybe_midi_state(nullptr),
    volume(1.0f),
    muted(false),
    passthrough_muted(false),
    backend(backend),
    peak(0.0f),
    n_events_processed(0) {

#ifdef SHOOP_HAVE_LV2
    bool is_internal = (dynamic_cast<InternalAudioPort<float>*>(port.get()) ||
                        dynamic_cast<InternalLV2MidiOutputPort*>(port.get()));
#else
    bool is_internal = (dynamic_cast<InternalAudioPort<float>*>(port.get()));
#endif
}

void ConnectedPort::PROC_reset_buffers() {
    log_trace();
    maybe_audio_buffer = nullptr;
    maybe_midi_input_buffer = nullptr;
    maybe_midi_output_buffer = nullptr;
}

bool ConnectedPort::PROC_check_buffer(bool raise_if_absent) {
    auto maybe_midi = dynamic_cast<MidiPort*>(port.get());
    auto maybe_audio = dynamic_cast<AudioPort<shoop_types::audio_sample_t>*>(port.get());
    bool result = true;

    if (maybe_midi) {
        if(maybe_midi->has_internal_write_access()) {
            result = (bool) maybe_midi_input_buffer;
        } else {
            result = (bool) maybe_midi_output_buffer;
        }
    } else if (maybe_audio) {
        result = (maybe_audio_buffer != nullptr);
    } else {
        throw std::runtime_error("Invalid port");
    }

    if (!result && raise_if_absent) {
        throw std::runtime_error("No buffer available.");
    }

    return (bool)result;
}

void ConnectedPort::PROC_passthrough(uint32_t n_frames) {
    log_trace();
    for(auto & other : mp_passthrough_to) {
        auto o = other.lock();
        if(o && o->PROC_check_buffer(false)) {
            if (dynamic_cast<AudioPort<shoop_types::audio_sample_t>*>(port.get())) { PROC_passthrough_audio(n_frames, *o); }
            else if (dynamic_cast<MidiPort*>(port.get())) { PROC_passthrough_midi(n_frames, *o); }
            else { throw std::runtime_error("Invalid port"); }
        }
    }
}

void ConnectedPort::PROC_passthrough_audio(uint32_t n_frames, ConnectedPort &to) {
    log_trace();
    if (!muted && !passthrough_muted) {
        for (uint32_t i=0; i<n_frames; i++) {
            to.maybe_audio_buffer[i] += maybe_audio_buffer[i];
        }
    }
}

void ConnectedPort::PROC_passthrough_midi(uint32_t n_frames, ConnectedPort &to) {
    log_trace();
    if(!muted && !passthrough_muted) {
        for(uint32_t i=0; i<maybe_midi_output_buffer->PROC_get_n_events(); i++) {
            auto &msg = maybe_midi_output_buffer->PROC_get_event_reference(i);
            uint32_t t = msg.get_time();
            void* to_ptr = &to;
            log<log_level_debug>("Passthrough midi message reference to {} @ {}", to_ptr, t);
            to.maybe_midi_output_merging_buffer->PROC_write_event_reference(msg);
        }
    }
}

void ConnectedPort::PROC_process_buffer(uint32_t n_frames) {
    log_trace();
    if (auto a = dynamic_cast<AudioPort<shoop_types::audio_sample_t>*>(port.get())) {
        if (a->has_internal_read_access() || a->has_implicit_output_sink()) {
            // Apply volume
            float max = 0.0f;
            for (uint32_t i=0; i<n_frames; i++) {
                float vol = volume.load();
                maybe_audio_buffer[i] *= muted.load() ? 0.0f : vol;
                max = std::max(std::abs(maybe_audio_buffer[i]), max);
            }
            peak = std::max(peak.load(), max);
        }
    } else if (auto m = dynamic_cast<MidiPort*>(port.get())) {
        if (a->has_internal_read_access() || a->has_implicit_output_sink()) {
            // Process messages
            if (!muted) {
                uint32_t n_events = maybe_midi_output_merging_buffer->PROC_get_n_events();
                maybe_midi_output_merging_buffer->PROC_sort();
                
                for(uint32_t i=0; i<n_events; i++) {
                    uint32_t size, time;
                    const uint8_t* data;
                    maybe_midi_output_merging_buffer->PROC_get_event_reference(i).get(size, time, data);
                    log<log_level_trace>("Output midi message reference @ {}", time);
                    maybe_midi_input_buffer->PROC_write_event_value(size, time, data);
                    maybe_midi_state->process_msg(data);
                }

                n_events_processed += n_events;
            }
        }
    }

    auto maybe_dummy_audio = dynamic_cast<DummyAudioPort*>(port.get());
    if (maybe_dummy_audio) {
        maybe_dummy_audio->PROC_process(n_frames);       
    }
    auto maybe_dummy_midi = dynamic_cast<DummyMidiPort*>(port.get());
    if (maybe_dummy_midi) {
        maybe_dummy_midi->PROC_post_process(n_frames);       
    }
}

std::shared_ptr<AudioPort<shoop_types::audio_sample_t>> ConnectedPort::maybe_audio() {
    return std::dynamic_pointer_cast<AudioPort<shoop_types::audio_sample_t>>(port);
}

std::shared_ptr<MidiPort> ConnectedPort::maybe_midi() {
    return std::dynamic_pointer_cast<MidiPort>(port);
}

BackendSession &ConnectedPort::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
}

void ConnectedPort::connect_passthrough(const std::shared_ptr<ConnectedPort> &other) {
    log_trace();
    for (auto &_other : mp_passthrough_to) {
        if(auto __other = _other.lock()) {
            if (__other.get() == other.get()) { return; } // already connected
        }
    }
    mp_passthrough_to.push_back(other);
    get_backend().recalculate_processing_schedule();
}


void ConnectedPort::PROC_notify_changed_buffer_size(uint32_t buffer_size) {}

void ConnectedPort::graph_node_0_process(uint32_t nframes) {
    PROC_reset_buffers();
    port->PROC_prepare(nframes);
}

void ConnectedPort::graph_node_1_process(uint32_t nframes) {
    PROC_process_buffer(nframes);
    PROC_passthrough(nframes);
}


WeakGraphNodeSet ConnectedPort::graph_node_1_incoming_edges() {
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

WeakGraphNodeSet ConnectedPort::graph_node_1_outgoing_edges() {
    WeakGraphNodeSet rval;
    for (auto &other : mp_passthrough_to) {
        if(auto _other = other.lock()) {
            auto shared = _other->second_graph_node();
            rval.insert(shared);
        }
    }
    return rval;
}