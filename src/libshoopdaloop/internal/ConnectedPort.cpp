#include "ConnectedPort.h"
#include "AudioPortInterface.h"
#include "MidiPortInterface.h"
#include "InternalAudioPort.h"
#include "InternalLV2MidiOutputPort.h"
#include <stdexcept>
#include <algorithm>
#include "MidiStateTracker.h"
#include "MidiMergingBuffer.h"
#include "Backend.h"

using namespace shoop_types;
using namespace shoop_constants;

std::string ConnectedPort::log_module_name() const {
    return "Backend.ConnectedPort";
}

ConnectedPort::ConnectedPort (std::shared_ptr<PortInterface> const& port, std::shared_ptr<Backend> const& backend) :
    port(port),
    maybe_audio_buffer(nullptr),
    maybe_midi_input_buffer(nullptr),
    maybe_midi_output_buffer(nullptr),
    maybe_midi_state(nullptr),
    volume(1.0f),
    passthrough_volume(1.0f),
    muted(false),
    passthrough_muted(false),
    backend(backend),
    peak(0.0f),
    n_events_processed(0) {
    log_init();

    bool is_internal = (dynamic_cast<InternalAudioPort<float>*>(port.get()) ||
                        dynamic_cast<InternalLV2MidiOutputPort*>(port.get()));
    bool is_fx_in = is_internal && (port->direction() == PortDirection::Output);
    bool is_ext_in = !is_internal && (port->direction() == PortDirection::Input);

    ma_process_when = (is_ext_in || is_fx_in) ?
        shoop_types::ProcessWhen::BeforeFXChains : shoop_types::ProcessWhen::AfterFXChains;

    if (auto m = dynamic_cast<MidiPortInterface*>(port.get())) {
        maybe_midi_state = std::make_shared<MidiStateTracker>(true, true, true);
        if(m->direction() == PortDirection::Output) {
            maybe_midi_output_merging_buffer = std::make_shared<MidiMergingBuffer>();
        }
    }
}

void ConnectedPort::PROC_reset_buffers() {
    log_trace();
    maybe_audio_buffer = nullptr;
    maybe_midi_input_buffer = nullptr;
    maybe_midi_output_buffer = nullptr;
}

void ConnectedPort::PROC_ensure_buffer(size_t n_frames) {
    log_trace();
    auto maybe_midi = dynamic_cast<MidiPortInterface*>(port.get());
    auto maybe_audio = dynamic_cast<AudioPortInterface<shoop_types::audio_sample_t>*>(port.get());

    if (maybe_midi) {
        if(maybe_midi->direction() == PortDirection::Input) {
            if (maybe_midi_input_buffer) { return; } // already there
            maybe_midi_input_buffer = &maybe_midi->PROC_get_read_buffer(n_frames);
            if (!muted) {
                n_events_processed += maybe_midi_input_buffer->PROC_get_n_events();
                for(size_t i=0; i<maybe_midi_input_buffer->PROC_get_n_events(); i++) {
                    auto &msg = maybe_midi_input_buffer->PROC_get_event_reference(i);
                    uint32_t size, time;
                    const uint8_t *data;
                    msg.get(size, time, data);
                    maybe_midi_state->process_msg(data);
                }
            }
        } else {
            maybe_midi_output_merging_buffer->PROC_clear();
            if (maybe_midi_output_buffer) { return; } // already there
            maybe_midi_output_buffer = &maybe_midi->PROC_get_write_buffer(n_frames);
        }
    } else if (maybe_audio) {
        if (maybe_audio_buffer) { return; } // already there
        maybe_audio_buffer = maybe_audio->PROC_get_buffer(n_frames);
        if (port->direction() == PortDirection::Input) {
            float max = 0.0f;
            if (muted) {
                memset((void*)maybe_audio_buffer, 0, n_frames * sizeof(audio_sample_t));
            } else {
                for(size_t i=0; i<n_frames; i++) {
                    max = std::max(max, std::abs(maybe_audio_buffer[i]));
                }
            }
            peak = std::max(peak.load(), max);
        }
    } else {
        throw std::runtime_error("Invalid port");
    }
}

void ConnectedPort::PROC_check_buffer() {
    auto maybe_midi = dynamic_cast<MidiPortInterface*>(port.get());
    auto maybe_audio = dynamic_cast<AudioPortInterface<shoop_types::audio_sample_t>*>(port.get());
    bool result;

    if (maybe_midi) {
        if(maybe_midi->direction() == PortDirection::Input) {
            result = (bool) maybe_midi_input_buffer;
        } else {
            result = (bool) maybe_midi_output_buffer;
        }
    } else if (maybe_audio) {
        result = (maybe_audio_buffer != nullptr);
    } else {
        throw std::runtime_error("Invalid port");
    }

    if (!result) {
        throw std::runtime_error("No buffer available.");
    }
}

void ConnectedPort::PROC_passthrough(size_t n_frames) {
    log_trace();
    if (port->direction() == PortDirection::Input) {
        for(auto & other : mp_passthrough_to) {
            auto o = other.lock();
            if(o) {
                o->PROC_check_buffer();
                if (dynamic_cast<AudioPortInterface<shoop_types::audio_sample_t>*>(port.get())) { PROC_passthrough_audio(n_frames, *o); }
                else if (dynamic_cast<MidiPortInterface*>(port.get())) { PROC_passthrough_midi(n_frames, *o); }
                else { throw std::runtime_error("Invalid port"); }
            }
        }
    }
}

void ConnectedPort::PROC_passthrough_audio(size_t n_frames, ConnectedPort &to) {
    log_trace();
    if (!muted && !passthrough_muted) {
        for (size_t i=0; i<n_frames; i++) {
            to.maybe_audio_buffer[i] += passthrough_volume * maybe_audio_buffer[i];
        }
    }
}

void ConnectedPort::PROC_passthrough_midi(size_t n_frames, ConnectedPort &to) {
    log_trace();
    if(!muted && !passthrough_muted) {
        for(size_t i=0; i<maybe_midi_input_buffer->PROC_get_n_events(); i++) {
            auto &msg = maybe_midi_input_buffer->PROC_get_event_reference(i);
            to.maybe_midi_output_merging_buffer->PROC_write_event_reference(msg);
        }
    }
}

void ConnectedPort::PROC_finalize_process(size_t n_frames) {
    log_trace();
    if (auto a = dynamic_cast<AudioPortInterface<shoop_types::audio_sample_t>*>(port.get())) {
        if (a->direction() == PortDirection::Output) {
            float max = 0.0f;
            for (size_t i=0; i<n_frames; i++) {
                maybe_audio_buffer[i] *= muted.load() ? 0.0f : volume.load();
                max = std::max(std::abs(maybe_audio_buffer[i]), max);
            }
            peak = std::max(peak.load(), max);
        }
    } else if (auto m = dynamic_cast<MidiPortInterface*>(port.get())) {
        if (m->direction() == PortDirection::Output) {
            if (!muted) {
                maybe_midi_output_merging_buffer->PROC_sort();
                size_t n_events = maybe_midi_output_merging_buffer->PROC_get_n_events();
                for(size_t i=0; i<n_events; i++) {
                    uint32_t size, time;
                    const uint8_t* data;
                    maybe_midi_output_merging_buffer->PROC_get_event_reference(i).get(size, time, data);
                    maybe_midi_output_buffer->PROC_write_event_value(size, time, data);
                    maybe_midi_state->process_msg(data);
                }
                n_events_processed += n_events;
            }
        }
    }
}

AudioPortInterface<shoop_types::audio_sample_t> *ConnectedPort::maybe_audio() {
    return dynamic_cast<AudioPortInterface<shoop_types::audio_sample_t>*>(port.get());
}

MidiPortInterface *ConnectedPort::maybe_midi() {
    return dynamic_cast<MidiPortInterface*>(port.get());
}

Backend &ConnectedPort::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
}

void ConnectedPort::connect_passthrough(const std::shared_ptr<ConnectedPort> &other) {
    log_trace();
    get_backend().cmd_queue.queue([this, other]() {
        for (auto &_other : mp_passthrough_to) {
            if(auto __other = _other.lock()) {
                if (__other.get() == other.get()) { return; } // already connected
            }
        }
        mp_passthrough_to.push_back(other);
    });
}