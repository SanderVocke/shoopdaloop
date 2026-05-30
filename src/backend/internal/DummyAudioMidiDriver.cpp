#include "AudioMidiDriver.h"
#include "LoggingBackend.h"
#include "PortInterface.h"
#include "fmt/format.h"
#include <fmt/ranges.h>
#include "types.h"
#include <chrono>
#include <cstdint>
#include <string>
#include <thread>
#include <stdexcept>
#include "DummyAudioMidiDriver.h"
#include "RustCommandQueue.h"
#include "DummyAudioMidiDriverCxxTrampolines.h"
#include <map>

namespace backend_rust {
void dummy_audiomididriver_exec_commands(uintptr_t owner_ptr) {
    auto *owner = reinterpret_cast<AudioMidiDriver *>(owner_ptr);
    owner->trampoline_exec_commands();
}
void dummy_audiomididriver_process(uintptr_t owner_ptr, uint32_t nframes) {
    auto *owner = reinterpret_cast<AudioMidiDriver *>(owner_ptr);
    owner->trampoline_process(nframes);
}
}

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
    if ((DummyAudioMidiDriverMode)m_rust->get_mode() != mode) {
        Log::log<log_level_debug>("DummyAudioMidiDriver: mode -> {}", mode_names.at(mode));
        m_rust->enter_mode((uint32_t)mode);

        // Ensure we finish any processing we were doing
        wait_process();
    }
}

template <typename Time, typename Size>
DummyAudioMidiDriverMode DummyAudioMidiDriver<Time, Size>::get_mode() const {
    return (DummyAudioMidiDriverMode)m_rust->get_mode();
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::controlled_mode_request_samples(uint32_t samples) {
    rust_command_queue::queue_and_wait(this->m_command_queue, [this, samples]() {
        m_rust->controlled_mode_request_samples(samples);
        uint32_t requested = m_rust->get_controlled_mode_samples_to_process();
        Log::log<log_level_debug>("DummyAudioMidiDriver: request {} samples ({} total)", samples, requested);
    });
}

template <typename Time, typename Size>
uint32_t DummyAudioMidiDriver<Time, Size>::get_controlled_mode_samples_to_process() const {
    return m_rust->get_controlled_mode_samples_to_process();
}


template <typename Time, typename Size>
DummyAudioMidiDriver<Time, Size>::DummyAudioMidiDriver(void (*maybe_process_callback)())
    : AudioMidiDriver(maybe_process_callback),
      m_rust(backend_rust::new_dummy_audio_midi_driver()),
      m_audio_port_opened_cb(nullptr), m_midi_port_opened_cb(nullptr),
      m_audio_port_closed_cb(nullptr), m_midi_port_closed_cb(nullptr),
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
    auto *p_settings = dynamic_cast<DummyAudioMidiDriverSettings*>(&settings);
    if (!p_settings) {
        throw std::runtime_error("Wrong settings type passed to DummyAudioMidiDriver");
    }
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
    rust_command_queue::exec_all(this->m_command_queue);

    m_rust->start_process_thread(
        reinterpret_cast<uintptr_t>(this),
        AudioMidiDriver::get_sample_rate(),
        AudioMidiDriver::get_buffer_size());
    AudioMidiDriver::set_active(true);
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::pause() {
    Log::log<log_level_debug>("DummyAudioMidiDriver: pause");
    m_rust->pause();
    wait_process();
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::resume() {
    Log::log<log_level_debug>("DummyAudioMidiDriver: resume");
    m_rust->resume();
}

template <typename Time, typename Size>
DummyAudioMidiDriver<Time, Size>::~DummyAudioMidiDriver() {
    close();
}

template <typename Time, typename Size>
shoop_shared_ptr<RustAudioPortF32>
DummyAudioMidiDriver<Time, Size>::open_audio_port(std::string name,
                                              shoop_port_direction_t direction,
                                              shoop_shared_ptr<RustAudioPortF32::UsedBufferPool> buffer_pool) {
    Log::log<log_level_debug>("DummyAudioMidiDriver : add audio port");
    auto rval = shoop_make_shared<DummyAudioPort>(name, direction, buffer_pool, m_external_connections);
    m_audio_ports.insert(rval);
    return shoop_static_pointer_cast<RustAudioPortF32>(rval);
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
    m_rust->stop_process_thread();
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
        (DummyAudioMidiDriverMode)m_rust->get_mode() == DummyAudioMidiDriverMode::Controlled &&
        m_rust->get_controlled_mode_samples_to_process() > 0 &&
        !timed_out()
    ) {
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }

    if (timed_out()) {
        Log::log<log_level_warning>("DummyAudioMidiDriver: run request timed out");
    }

    wait_process();

    if (m_rust->get_controlled_mode_samples_to_process() > 0) {
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