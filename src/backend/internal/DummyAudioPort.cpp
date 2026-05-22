#include "DummyAudioPort.h"
#include "backend_rust/src/audio_port_cxx.rs.h"
#include <algorithm>
#include "fmt/format.h"
#include <fmt/ranges.h>

#ifdef _WIN32
#undef min
#undef max
#endif


DummyAudioPort::DummyAudioPort(std::string name, shoop_port_direction_t direction, shoop_shared_ptr<RustAudioPortF32::UsedBufferPool> buffer_pool, shoop_weak_ptr<DummyExternalConnections> external_connections)
    : RustAudioPortF32(buffer_pool, 32),
      m_rust_dummy(backend_rust::new_dummy_audio_port(name, direction == ShoopPortDirection_Output, reinterpret_cast<size_t>(this))),
      m_dummy_port_core(name, direction, this, external_connections),
      m_command_queue(100, 1000, 1000) { }

float *DummyAudioPort::PROC_get_buffer(uint32_t n_frames) {
    return m_rust_dummy->proc_get_buffer(n_frames);
}

void DummyAudioPort::queue_data(uint32_t n_frames, audio_sample_t const *data) {
    m_command_queue.queue_and_wait([this, n_frames, data]() {
        auto slice = rust::Slice<const float>(data, n_frames);
        m_rust_dummy->queue_data(n_frames, slice);
    });
}

bool DummyAudioPort::get_queue_empty() {
    bool is_empty = false;
    m_command_queue.queue_and_wait([this, &is_empty]() {
        is_empty = m_rust_dummy->get_queue_empty();
    });
    return is_empty;
}

DummyAudioPort::~DummyAudioPort() { m_dummy_port_core.close(); }

void DummyAudioPort::PROC_process(uint32_t n_frames) {
    if (n_frames > 0) {
        log<log_level_debug_trace>("Process {} frames", n_frames);
    }

    // Process the buffer using the base class AudioPort (gain/mute/peaks/ringbuffer)
    auto buf = PROC_get_buffer(n_frames);
    if (n_frames > 0 && buf) {
        if (m_rust.has_value()) {
            backend_rust::audio_port_process(**m_rust, buf, n_frames);
        }
    }

    // Copy processed buffer to retained samples for output ports
    m_rust_dummy->proc_process(n_frames);
}

void DummyAudioPort::PROC_prepare(uint32_t n_frames) {
    m_command_queue.PROC_exec_all();
    m_rust_dummy->proc_prepare(n_frames);
}

void DummyAudioPort::request_data(uint32_t n_frames) {
    m_command_queue.queue_and_wait([this, n_frames]() {
        m_rust_dummy->request_data(n_frames);
    });
}

std::vector<audio_sample_t> DummyAudioPort::dequeue_data(uint32_t n) {
    std::vector<audio_sample_t> rval;
    m_command_queue.queue_and_wait([this, n, &rval]() {
        auto rust_vec = m_rust_dummy->dequeue_data(n);
        rval.reserve(rust_vec.size());
        for (auto &sample : rust_vec) {
            rval.push_back(sample);
        }
    });
    return rval;
}

unsigned DummyAudioPort::input_connectability() const {
    return (m_dummy_port_core.direction() == ShoopPortDirection_Input) ? ShoopPortConnectability_External : ShoopPortConnectability_Internal;
}

unsigned DummyAudioPort::output_connectability() const {
    return (m_dummy_port_core.direction() == ShoopPortDirection_Input) ? ShoopPortConnectability_Internal : ShoopPortConnectability_External;
}

PortExternalConnectionStatus DummyAudioPort::get_external_connection_status() const {
    return PortExternalConnectionStatus();
}

void DummyAudioPort::connect_external(std::string name) {
    (void)name;
    throw std::runtime_error("DummyAudioPort does not support external connections.");
}

void DummyAudioPort::disconnect_external(std::string name) {
    (void)name;
    throw std::runtime_error("DummyAudioPort does not support external connections.");
}
