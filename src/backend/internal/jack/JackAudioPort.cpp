#include "JackAudioPort.h"
#include "backend_rust/src/audio_port_cxx.rs.h"
#include <string>
#include "JackPort.h"
#include "PortInterface.h"
#include <stdexcept>
#include <cstring>
#include <iostream>

#ifdef _WIN32
#undef min
#undef max
#endif

JackAudioPort::JackAudioPort(std::string name, shoop_port_direction_t direction,
                             uintptr_t client, std::shared_ptr<JackAllPorts> all_ports_tracker,
                             rust::Box<backend_rust::JackApiBridgeStrong> api,
                             std::shared_ptr<RustAudioPortF32::UsedBufferPool> buffer_pool)
    : RustAudioPortF32(buffer_pool, 32),
      JackPort(name, direction, PortDataType::Audio, client, std::move(all_ports_tracker), std::move(api)) {}

void JackAudioPort::PROC_prepare(uint32_t nframes) {
    JackPort::PROC_prepare(nframes);
    auto buf = m_buffer.load();
    if (!buf) {
        // If JACK fails to give us a buffer, provide an internal fallback.
        m_fallback_buffer.resize(std::max(nframes, (uint32_t) m_fallback_buffer.size()));
        m_buffer = (void*)m_fallback_buffer.data();
    }
    if (!has_implicit_input_source()) {
        // JACK output buffers should be zero'd
        memset((void*) m_buffer.load(), 0, sizeof(jack_default_audio_sample_t) * nframes);
    }
}

float *JackAudioPort::PROC_get_buffer(uint32_t n_frames) {
    auto rval = (float*) m_buffer.load();
    if (!rval) {
        if(m_fallback_buffer.size() < std::max(n_frames, (uint32_t)1)) {
            m_fallback_buffer.resize(std::max(n_frames, (uint32_t)1));
        }
        rval = m_fallback_buffer.data();
    }
    return rval;
}

void JackAudioPort::PROC_process(uint32_t nframes) {
    // Get the buffer we're supposed to process
    auto buf = (float*) m_buffer.load();
    if (!buf) {
        buf = m_fallback_buffer.data();
    }

    if (!buf || nframes == 0) {
        return;
    }

    if (!m_rust.has_value()) {
        return;
    }

    // Call Rust process with the correct buffer
    backend_rust::audio_port_process(**m_rust, buf, nframes);
}

unsigned JackAudioPort::input_connectability() const {
    return (m_direction == ShoopPortDirection_Input) ? ShoopPortConnectability_External : ShoopPortConnectability_Internal;
}

unsigned JackAudioPort::output_connectability() const {
    return (m_direction == ShoopPortDirection_Output) ? ShoopPortConnectability_External : ShoopPortConnectability_Internal;
}
