#include "ConnectedPort.h"
#include "AudioPortInterface.h"
#include "MidiPortInterface.h"
#include "InternalAudioPort.h"
#include <memory>
#include <stdexcept>
#include <algorithm>
#include "MidiStateTracker.h"
#include "MidiMergingBuffer.h"
#include "Backend.h"
#include "DummyAudioSystem.h"
#include "shoop_globals.h"

#ifdef SHOOP_HAVE_LV2
#include "InternalLV2MidiOutputPort.h"
#endif

using namespace shoop_types;
using namespace shoop_constants;

ConnectedPort::ConnectedPort (std::shared_ptr<PortInterface> const& port,
                              std::shared_ptr<Backend> const& backend,
                              shoop_types::ProcessWhen process_when) :
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
    n_events_processed(0),
    ma_process_when(process_when) {

#ifdef SHOOP_HAVE_LV2
    bool is_internal = (dynamic_cast<InternalAudioPort<float>*>(port.get()) ||
                        dynamic_cast<InternalLV2MidiOutputPort*>(port.get()));
#else
    bool is_internal = (dynamic_cast<InternalAudioPort<float>*>(port.get()));
#endif

    bool is_fx_in = is_internal && (port->direction() == PortDirection::Output);
    bool is_ext_in = !is_internal && (port->direction() == PortDirection::Input);

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

void ConnectedPort::PROC_ensure_buffer(uint32_t n_frames, bool do_zero) {
    log_trace();
    auto maybe_midi = dynamic_cast<MidiPortInterface*>(port.get());
    auto maybe_audio = dynamic_cast<AudioPortInterface<shoop_types::audio_sample_t>*>(port.get());

    if (maybe_midi) {
        if(maybe_midi->direction() == PortDirection::Input) {
            if (maybe_midi_input_buffer) { return; } // already there
            maybe_midi_input_buffer = &maybe_midi->PROC_get_read_buffer(n_frames);
            if (!muted) {
                n_events_processed += maybe_midi_input_buffer->PROC_get_n_events();
                for(uint32_t i=0; i<maybe_midi_input_buffer->PROC_get_n_events(); i++) {
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
        maybe_audio_buffer = maybe_audio->PROC_get_buffer(n_frames, do_zero);
        if (port->direction() == PortDirection::Input) {
            float max = 0.0f;
            if (muted) {
                memset((void*)maybe_audio_buffer, 0, n_frames * sizeof(audio_sample_t));
            } else {
                for(uint32_t i=0; i<n_frames; i++) {
                    float vol = volume.load();
                    maybe_audio_buffer[i] *= vol;
                    max = std::max(max, std::abs(maybe_audio_buffer[i]));
                }
            }
            peak = std::max(peak.load(), max);
        }
    } else {
        throw std::runtime_error("Invalid port");
    }
}

bool ConnectedPort::PROC_check_buffer(bool raise_if_absent) {
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

    if (!result && raise_if_absent) {
        throw std::runtime_error("No buffer available.");
    }

    return (bool)result;
}

void ConnectedPort::PROC_passthrough(uint32_t n_frames) {
    log_trace();
    if (port->direction() == PortDirection::Input) {
        for(auto & other : mp_passthrough_to) {
            auto o = other.lock();
            if(o && o->PROC_check_buffer(false)) {
                if (dynamic_cast<AudioPortInterface<shoop_types::audio_sample_t>*>(port.get())) { PROC_passthrough_audio(n_frames, *o); }
                else if (dynamic_cast<MidiPortInterface*>(port.get())) { PROC_passthrough_midi(n_frames, *o); }
                else { throw std::runtime_error("Invalid port"); }
            }
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
        for(uint32_t i=0; i<maybe_midi_input_buffer->PROC_get_n_events(); i++) {
            auto &msg = maybe_midi_input_buffer->PROC_get_event_reference(i);
            uint32_t t = msg.get_time();
            void* to_ptr = &to;
            log<debug>("Passthrough midi message reference to {} @ {}", to_ptr, t);
            to.maybe_midi_output_merging_buffer->PROC_write_event_reference(msg);
        }
    }
}

void ConnectedPort::PROC_finalize_process(uint32_t n_frames) {
    log_trace();
    if (auto a = dynamic_cast<AudioPortInterface<shoop_types::audio_sample_t>*>(port.get())) {
        if (a->direction() == PortDirection::Output) {
            float max = 0.0f;
            for (uint32_t i=0; i<n_frames; i++) {
                float vol = volume.load();
                maybe_audio_buffer[i] *= muted.load() ? 0.0f : vol;
                max = std::max(std::abs(maybe_audio_buffer[i]), max);
            }
            peak = std::max(peak.load(), max);
        }
    } else if (auto m = dynamic_cast<MidiPortInterface*>(port.get())) {
        if (m->direction() == PortDirection::Output) {
            if (!muted) {
                uint32_t n_events = maybe_midi_output_merging_buffer->PROC_get_n_events();
                maybe_midi_output_merging_buffer->PROC_sort();
                
                for(uint32_t i=0; i<n_events; i++) {
                    uint32_t size, time;
                    const uint8_t* data;
                    maybe_midi_output_merging_buffer->PROC_get_event_reference(i).get(size, time, data);
                    log<trace>("Output midi message reference @ {}", time);
                    maybe_midi_output_buffer->PROC_write_event_value(size, time, data);
                    maybe_midi_state->process_msg(data);
                }

                n_events_processed += n_events;
            }
        }
    }

    auto maybe_dummy_audio = dynamic_cast<DummyAudioPort*>(port.get());
    if (maybe_dummy_audio) {
        maybe_dummy_audio->PROC_post_process(maybe_audio_buffer, n_frames);       
    }
    auto maybe_dummy_midi = dynamic_cast<DummyMidiPort*>(port.get());
    if (maybe_dummy_midi) {
        maybe_dummy_midi->PROC_post_process(n_frames);       
    }
}

std::shared_ptr<AudioPortInterface<shoop_types::audio_sample_t>> ConnectedPort::maybe_audio() {
    return std::dynamic_pointer_cast<AudioPortInterface<shoop_types::audio_sample_t>>(port);
}

std::shared_ptr<MidiPortInterface> ConnectedPort::maybe_midi() {
    return std::dynamic_pointer_cast<MidiPortInterface>(port);
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