#include "AudioMidiDriver.h"
#include "LoggingBackend.h"
#include "PortInterface.h"
#include "WithCommandQueue.h"
#include "fmt/format.h"
#include <fmt/ranges.h>
#include "types.h"
#include <chrono>
#include <cstdint>
#include <string>
#include <thread>
#include "DummyAudioMidiDriver.h"
#include <map>
#include <algorithm>
#include <regex>

#ifdef _WIN32
#undef min
#undef max
#endif

const std::map<DummyAudioMidiDriverMode, const char*> mode_names = {
    {DummyAudioMidiDriverMode::Automatic, "Automatic"},
    {DummyAudioMidiDriverMode::Controlled, "Controlled"}
};

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::enter_mode(DummyAudioMidiDriverMode mode) {
    if (m_mode.load() != mode) {
        Log::log<log_level_debug>("DummyAudioMidiDriver: mode -> {}", mode_names.at(mode));
        m_mode = mode;
        m_controlled_mode_samples_to_process = 0;

        // Ensure we finish any processing we were doing
        wait_process();
    }
}

template <typename Time, typename Size>
DummyAudioMidiDriverMode DummyAudioMidiDriver<Time, Size>::get_mode() const {
    return m_mode;
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::controlled_mode_request_samples(uint32_t samples) {
    exec_process_thread_command([=]() {
        this->m_controlled_mode_samples_to_process += samples;
        uint32_t requested = this->m_controlled_mode_samples_to_process.load();
        Log::log<log_level_debug>("DummyAudioMidiDriver: request {} samples ({} total)", samples, requested);
    });
}

template <typename Time, typename Size>
uint32_t DummyAudioMidiDriver<Time, Size>::get_controlled_mode_samples_to_process() const {
    return m_controlled_mode_samples_to_process;
}


template <typename Time, typename Size>
DummyAudioMidiDriver<Time, Size>::DummyAudioMidiDriver(void (*maybe_process_callback)())
    : AudioMidiDriver(maybe_process_callback),
      m_finish(false),
      m_paused(false),
      m_mode(DummyAudioMidiDriverMode::Automatic),
      m_controlled_mode_samples_to_process(0),
      m_audio_port_closed_cb(nullptr), m_audio_port_opened_cb(nullptr),
      m_midi_port_closed_cb(nullptr), m_midi_port_opened_cb(nullptr),
      m_external_connections(shoop_make_shared<DummyExternalConnections>())
{
    m_audio_ports.clear();
    m_midi_ports.clear();

    Log::log<log_level_debug>("DummyAudioMidiDriver: constructed");
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::start(
    AudioMidiDriverSettingsInterface &settings
) {
    auto p_settings = (DummyAudioMidiDriverSettings*)&settings;
    auto &_settings = *p_settings;

    AudioMidiDriver::set_sample_rate(_settings.sample_rate);
    AudioMidiDriver::set_buffer_size(_settings.buffer_size);
    m_client_name_str = _settings.client_name;
    AudioMidiDriver::set_client_name(m_client_name_str.c_str());
    AudioMidiDriver::set_dsp_load(0.0f);
    AudioMidiDriver::set_maybe_client_handle(nullptr);

    Log::log<log_level_debug>("Starting (sample rate {}, buf size {})", _settings.sample_rate, _settings.buffer_size);

    // Processing the command queue once will ensure that it knows processing is active.
    // That way commands added from now on will be executed on the process thread.
    ma_queue.PROC_exec_all();

    m_proc_thread = std::thread([this] {
        Log::log<log_level_debug>("Starting process thread - {}", mode_names.at(m_mode));
        auto bufs_per_second = AudioMidiDriver::get_sample_rate() / AudioMidiDriver::get_buffer_size();
        auto interval = 1.0f / ((float)bufs_per_second);
        auto micros = uint32_t(interval * 1000000.0f);
        float time_taken = 0.0f;
        while (!this->m_finish) {
            std::this_thread::sleep_for(std::chrono::microseconds((uint32_t)std::ceil(std::max(0.0f, micros - time_taken))));
            PROC_handle_command_queue();
            if (!m_paused) {
                auto start = std::chrono::high_resolution_clock::now();
                auto mode = m_mode.load();
                auto samples_to_process = m_controlled_mode_samples_to_process.load();
                uint32_t to_process = mode == DummyAudioMidiDriverMode::Controlled ?
                    std::min(samples_to_process, AudioMidiDriver::get_buffer_size()) :
                    AudioMidiDriver::get_buffer_size();
                if (to_process > 0) {
                    Log::log<log_level_debug_trace>("Process {}", to_process);
                }
                AudioMidiDriver::PROC_process(to_process);
                if (mode == DummyAudioMidiDriverMode::Controlled) {
                    m_controlled_mode_samples_to_process -= to_process;
                }
                auto end = std::chrono::high_resolution_clock::now();
                time_taken = duration_cast<std::chrono::microseconds>(end - start).count();
            }
        }
        Log::log<log_level_debug>("Ending process thread");
    });
    AudioMidiDriver::set_active(true);
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::pause() {
    Log::log<log_level_debug>("DummyAudioMidiDriver: pause");
    m_paused = true;
    wait_process();
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::resume() {
    Log::log<log_level_debug>("DummyAudioMidiDriver: resume");
    m_paused = false;
}

template <typename Time, typename Size>
DummyAudioMidiDriver<Time, Size>::~DummyAudioMidiDriver() {
    close();
}

template <typename Time, typename Size>
shoop_shared_ptr<AudioPort<audio_sample_t>>
DummyAudioMidiDriver<Time, Size>::open_audio_port(std::string name,
                                              shoop_port_direction_t direction,
                                              shoop_shared_ptr<typename AudioPort<audio_sample_t>::UsedBufferPool> buffer_pool) {
    Log::log<log_level_debug>("DummyAudioMidiDriver : add audio port");
    auto rval = shoop_make_shared<DummyAudioPort>(name, direction, buffer_pool, m_external_connections);
    m_audio_ports.insert(rval);
    return shoop_static_pointer_cast<AudioPort<audio_sample_t>>(rval);
}

template <typename Time, typename Size>
shoop_shared_ptr<MidiPort>
DummyAudioMidiDriver<Time, Size>::open_midi_port(std::string name,
                                             shoop_port_direction_t direction) {
    Log::log<log_level_debug>("DummyAudioMidiDriver: add midi port");
    auto rval = shoop_make_shared<DummyMidiPort>(name, direction, m_external_connections);
    m_midi_ports.insert(rval);
    return shoop_static_pointer_cast<MidiPort>(rval);
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::close() {
    m_finish = true;
    if (m_proc_thread.joinable()) {
        if (std::this_thread::get_id() != m_proc_thread.get_id()) {
            m_proc_thread.join();
        } else {
            m_proc_thread.detach();
        }
    }
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::controlled_mode_run_request(uint32_t timeout) {
    Log::log<log_level_debug>("DummyAudioMidiDriver: run request");

    wait_process();

    auto s = std::chrono::high_resolution_clock::now();
    auto timed_out = [this, &timeout, &s]() {
        return std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::high_resolution_clock::now() - s
        ).count() >= timeout;
    };
    while(
        m_mode == DummyAudioMidiDriverMode::Controlled &&
        m_controlled_mode_samples_to_process > 0 &&
        !timed_out()
    ) {
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }

    if (timed_out()) {
        Log::log<log_level_warning>("DummyAudioMidiDriver: run request timed out");
    }

    wait_process();

    if (m_controlled_mode_samples_to_process > 0) {
        Log::log<log_level_error>("DummyAudioMidiDriver: run request failed: requested samples remain");
    }
}

template<typename Time, typename Size>
std::vector<ExternalPortDescriptor> DummyAudioMidiDriver<Time, Size>::find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    )
{
    return m_external_connections->find_external_ports(maybe_name_regex, maybe_direction_filter, maybe_data_type_filter);
}

void DummyExternalConnections::add_external_mock_port(std::string name, shoop_port_direction_t direction, shoop_port_data_type_t data_type) {
    if (std::find_if(m_external_mock_ports.begin(), m_external_mock_ports.end(), [name](auto &a) { return a.name == name; }) == m_external_mock_ports.end()) {
        m_external_mock_ports.push_back(ExternalPortDescriptor {
            .name = name,
            .direction = direction,
            .data_type = data_type
        });
    }
}

void DummyExternalConnections::remove_external_mock_port(std::string name) {
    auto new_end = std::remove_if(m_external_mock_ports.begin(), m_external_mock_ports.end(), [name](auto &a) { return a.name == name; });

    if(!(new_end == m_external_mock_ports.end())) {
        m_external_mock_ports.erase(new_end, m_external_mock_ports.end());
        // Remove connections also
        auto new_conns_end = std::remove_if(m_external_connections.begin(), m_external_connections.end(), [name](auto &a) { return a.second == name; });
        m_external_connections.erase(
            new_conns_end,
            m_external_connections.end()
        );
    }
}

void DummyExternalConnections::remove_all_external_mock_ports() {
    m_external_mock_ports.clear();
    m_external_connections.clear();
}

ExternalPortDescriptor &DummyExternalConnections::get_port(std::string name) {
    auto it = std::find_if(m_external_mock_ports.begin(), m_external_mock_ports.end(), [name](auto &a) { return a.name == name; });
    if (it == m_external_mock_ports.end()) {
        throw std::runtime_error("Port not found");
    }
    return *it;
}

void DummyExternalConnections::connect(DummyPort* port, std::string external_port_name) {
    auto pname = port->name();
    log<log_level_debug>("connect {} to {}", pname, external_port_name);
    auto &desc = get_port(external_port_name);
    auto conn = std::make_pair(port, desc.name);
    if (std::find(m_external_connections.begin(), m_external_connections.end(), conn) == m_external_connections.end()) {
        m_external_connections.push_back(conn);
    }
}

void DummyExternalConnections::disconnect(DummyPort* port, std::string external_port_name) {
    auto pname = port->name();
    log<log_level_debug>("disconnect {} from {}", pname, external_port_name);
    auto &desc = get_port(external_port_name);
    auto conn = std::make_pair(port, desc.name);
    auto new_end = std::remove(m_external_connections.begin(), m_external_connections.end(), conn);
    m_external_connections.erase(new_end, m_external_connections.end());
}

PortExternalConnectionStatus DummyExternalConnections::connection_status_of(const DummyPort* port) {
    log<log_level_debug>("getting connection status of {}", port->name());
    PortExternalConnectionStatus rval;
    for (auto &conn : m_external_connections) {
        rval[conn.second] = conn.first == port;
    }
    return rval;
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::add_external_mock_port(std::string name, shoop_port_direction_t direction, shoop_port_data_type_t data_type) {
    Log::log<log_level_debug>("add external mock port {}", name);
    m_external_connections->add_external_mock_port(name, direction, data_type);
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::remove_external_mock_port(std::string name) {
    Log::log<log_level_debug>("remove external mock port {}", name);
    m_external_connections->remove_external_mock_port(name);
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::remove_all_external_mock_ports() {
    Log::log<log_level_debug>("remove all external mock ports");
    m_external_connections->remove_all_external_mock_ports();
}

template class DummyAudioMidiDriver<uint32_t, uint16_t>;
template class DummyAudioMidiDriver<uint32_t, uint32_t>;
template class DummyAudioMidiDriver<uint16_t, uint16_t>;
template class DummyAudioMidiDriver<uint16_t, uint32_t>;
template class DummyAudioMidiDriver<uint32_t, uint64_t>;
template class DummyAudioMidiDriver<uint64_t, uint64_t>;