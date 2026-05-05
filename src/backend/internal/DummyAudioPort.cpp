
#include "DummyAudioPort.h"
#include <algorithm>
#include "fmt/format.h"
#include <fmt/ranges.h>

#ifdef _WIN32
#undef min
#undef max
#endif


DummyAudioPort::DummyAudioPort(std::string name, shoop_port_direction_t direction, shoop_shared_ptr<AudioPort<audio_sample_t>::UsedBufferPool> buffer_pool, shoop_weak_ptr<DummyExternalConnections> external_connections)
    : AudioPort<audio_sample_t>(buffer_pool), m_name(name),
      DummyPort(name, direction, PortDataType::Audio, external_connections),
      WithCommandQueue(100),
      m_direction(direction),
      m_queued_data(128),
      m_plot_input_queued("input_queued"),
      m_plot_output_retained("output_retained"),
      m_plot_frames_processed("frames_processed"),
      m_plot_input_checksum("input_checksum"),
      m_plot_output_checksum("output_checksum") { }

float *DummyAudioPort::PROC_get_buffer(uint32_t n_frames) {
    size_t new_size = std::max({m_buffer_data.size(), (size_t)n_frames, (size_t)1});
    if (new_size > m_buffer_data.size()) {
        m_buffer_data.resize(new_size);
    }
    auto rval = m_buffer_data.data();
    return rval;
}

void DummyAudioPort::queue_data(uint32_t n_frames, audio_sample_t const *data) {
    exec_process_thread_command([=]() {
        auto s = this->m_queued_data.read_available();
        auto v = std::vector<audio_sample_t>(data, data + n_frames);
        log<log_level_debug>("Queueing {} samples, {} sets queued total", n_frames, s);
        if (should_log<log_level_debug_trace>()) {
            log<log_level_debug_trace>("--> Queued samples: {}", v);
        }
        this->m_queued_data.push(v);
        this->m_plot_input_queued.plot(static_cast<double>(s), this->name());
    });
}

bool DummyAudioPort::get_queue_empty() {
    bool is_empty = false;
    exec_process_thread_command([=,&is_empty]() {
        is_empty = this->m_queued_data.empty();
    });
    return is_empty;
}

DummyAudioPort::~DummyAudioPort() { DummyPort::close(); }

void DummyAudioPort::PROC_process(uint32_t n_frames) {
    if (n_frames > 0) {
        log<log_level_debug_trace>("Process {} frames", n_frames);
    }

    AudioPort<audio_sample_t>::PROC_process(n_frames);

    const char* port_name = name();
    m_plot_frames_processed.plot(static_cast<double>(n_frames), port_name);

    auto buf = PROC_get_buffer(n_frames);
    uint32_t to_store = std::min(n_frames, m_n_requested_samples.load());

    // Compute output checksum after AudioPort processing
    double output_checksum = checksum::compute_audio_checksum(buf, n_frames);
    ma_output_checksum = output_checksum;
    m_plot_output_checksum.plot(output_checksum, port_name);

    if (to_store > 0) {
        log<log_level_debug>("Buffering {} samples ({} total)", to_store, m_retained_samples.size() + to_store);
        if (should_log<log_level_debug_trace>()) {
            std::array<audio_sample_t, 16> arr {};
            std::copy(buf, buf+std::min(to_store, (uint32_t)16), arr.begin());
            log<log_level_debug_trace>("--> first 16 buffered samples: {}", arr);
        }
        m_retained_samples.insert(m_retained_samples.end(), buf, buf+to_store);
        m_n_requested_samples -= to_store;
        m_plot_output_retained.plot(static_cast<double>(m_retained_samples.size()), port_name);
    }
}

void DummyAudioPort::PROC_prepare(uint32_t n_frames) {
    PROC_handle_command_queue();
    auto buf = PROC_get_buffer(n_frames);
    uint32_t filled = 0;
    while (!m_queued_data.empty() && filled < n_frames) {
        auto &front = m_queued_data.front();
        uint32_t to_copy = std::min((size_t)(n_frames - filled), front.size());
        uint32_t total_copyable = m_queued_data.front().size();
        log<log_level_debug>("Dequeueing {} of {} input samples", to_copy, total_copyable);
        if (should_log<log_level_debug_trace>()) {
            log<log_level_debug_trace>("--> Dequeued input samples: {}", front);
        }
        memcpy((void *)(buf + filled), (void *)front.data(),
               sizeof(audio_sample_t) * to_copy);
        filled += to_copy;
        front.erase(front.begin(), front.begin() + to_copy);
        if (front.size() == 0) {
            m_queued_data.pop();
            bool another = !m_queued_data.empty();
            log<log_level_debug>("Pop queue item. Another: {}", another);
        }
    }
    const char* port_name = name();
    m_plot_input_queued.plot(static_cast<double>(m_queued_data.read_available()), port_name);
    memset((void *)(buf+filled), 0, sizeof(audio_sample_t) * (n_frames - filled));

    // Compute input checksum after preparing buffer
    double input_checksum = checksum::compute_audio_checksum(buf, n_frames);
    ma_input_checksum = input_checksum;
    m_plot_input_checksum.plot(input_checksum, port_name);
}

void DummyAudioPort::request_data(uint32_t n_frames) {
    exec_process_thread_command([=]() {
        this->m_n_requested_samples += n_frames;
    });
}

std::vector<audio_sample_t> DummyAudioPort::dequeue_data(uint32_t n) {
    std::vector<audio_sample_t> rval;
    exec_process_thread_command([=,&rval]() {
        auto s = m_retained_samples.size();
        if (n > s) {
            throw_error<std::runtime_error>("Not enough retained samples");
        }
        log<log_level_debug>("Yielding {} of {} output samples", n, s);
        rval = std::vector<audio_sample_t>(m_retained_samples.begin(), m_retained_samples.begin()+n);
        m_retained_samples.erase(m_retained_samples.begin(), m_retained_samples.begin()+n);
        m_plot_output_retained.plot(static_cast<double>(m_retained_samples.size()), name());
        if (should_log<log_level_debug_trace>()) {
            log<log_level_debug_trace>("--> Yielded samples: {}", rval);
        }
    });
    return rval;
}

unsigned DummyAudioPort::input_connectability() const {
    return (m_direction == ShoopPortDirection_Input) ? ShoopPortConnectability_External : ShoopPortConnectability_Internal;
}

unsigned DummyAudioPort::output_connectability() const {
    return (m_direction == ShoopPortDirection_Input) ? ShoopPortConnectability_Internal : ShoopPortConnectability_External;
}