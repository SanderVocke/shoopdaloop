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
#include "IProcessor.h"
#include "RustCommandQueue.h"
#include "DummyAudioMidiDriverCxxTrampolines.h"
#include <map>

namespace backend_rust {
void dummy_audiomididriver_exec_commands(uintptr_t owner_ptr) {
    auto *owner = reinterpret_cast<AudioMidiDriver *>(owner_ptr);
    owner->exec_all_commands_for_process_thread();
}
void dummy_audiomididriver_process(uintptr_t owner_ptr, uint32_t nframes) {
    auto *owner = reinterpret_cast<AudioMidiDriver *>(owner_ptr);
    owner->process(nframes);
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
    rust_command_queue::queue_and_wait(this->get_command_queue(), [this, samples]() {
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
      m_external_connections(std::make_shared<DummyExternalConnections>())
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

    set_sample_rate(_settings.sample_rate);
    set_buffer_size(_settings.buffer_size);
    m_client_name_str = _settings.client_name;
    set_client_name(m_client_name_str.c_str());
    set_dsp_load(0.0f);
    set_maybe_client_handle(nullptr);

    Log::log<log_level_debug>("Starting (sample rate {}, buf size {})", _settings.sample_rate, _settings.buffer_size);

    // Processing the command queue once will ensure that it knows processing is active.
    // That way commands added from now on will be executed on the process thread.
    rust_command_queue::exec_all(this->get_command_queue());

    m_rust->start_process_thread(
        reinterpret_cast<uintptr_t>(static_cast<AudioMidiDriver *>(this)),
        get_sample_rate(),
        get_buffer_size());
    set_active(true);
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
std::shared_ptr<RustAudioPortF32>
DummyAudioMidiDriver<Time, Size>::open_audio_port(std::string name,
                                              shoop_port_direction_t direction,
                                              std::shared_ptr<RustAudioPortF32::UsedBufferPool> buffer_pool) {
    Log::log<log_level_debug>("DummyAudioMidiDriver : add audio port");
    auto rval = std::make_shared<DummyAudioPort>(name, direction, buffer_pool, m_external_connections);
    m_audio_ports.insert(rval);
    return std::static_pointer_cast<RustAudioPortF32>(rval);
}

template <typename Time, typename Size>
std::shared_ptr<MidiPort>
DummyAudioMidiDriver<Time, Size>::open_midi_port(std::string name,
                                             shoop_port_direction_t direction) {
    Log::log<log_level_debug>("DummyAudioMidiDriver: add midi port");
    auto rval = std::make_shared<DummyMidiPort>(name, direction, m_external_connections);
    m_midi_ports.insert(rval);
    return std::static_pointer_cast<MidiPort>(rval);
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

template <typename Time, typename Size>
rust::Box<backend_rust::DecoupledMidiPortBridgeStrong> DummyAudioMidiDriver<Time, Size>::open_decoupled_midi_port(std::string name, shoop_port_direction_t direction) {
    auto port = open_midi_port(name, direction);
    return make_decoupled_midi_port(port, this->weak_driver_from_this(), direction);
}

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::add_processor(std::shared_ptr<IProcessor> p) { AudioMidiDriver::add_processor(p); }

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::remove_processor(std::shared_ptr<IProcessor> p) { AudioMidiDriver::remove_processor(p); }

template <typename Time, typename Size>
std::vector<std::unique_ptr<ProcessorBridgeWeak>> DummyAudioMidiDriver<Time, Size>::processors() const { return AudioMidiDriver::processors(); }

template <typename Time, typename Size>
uint32_t DummyAudioMidiDriver<Time, Size>::get_xruns() const { return AudioMidiDriver::get_xruns(); }

template <typename Time, typename Size>
float DummyAudioMidiDriver<Time, Size>::get_dsp_load() { return AudioMidiDriver::get_dsp_load(); }

template <typename Time, typename Size>
uint32_t DummyAudioMidiDriver<Time, Size>::get_sample_rate() { return AudioMidiDriver::get_sample_rate(); }

template <typename Time, typename Size>
uint32_t DummyAudioMidiDriver<Time, Size>::get_buffer_size() { return AudioMidiDriver::get_buffer_size(); }

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::reset_xruns() { AudioMidiDriver::reset_xruns(); }

template <typename Time, typename Size>
const char* DummyAudioMidiDriver<Time, Size>::get_client_name() const { return AudioMidiDriver::get_client_name(); }

template <typename Time, typename Size>
void* DummyAudioMidiDriver<Time, Size>::get_maybe_client_handle() const { return AudioMidiDriver::get_maybe_client_handle(); }

template <typename Time, typename Size>
bool DummyAudioMidiDriver<Time, Size>::get_active() const { return AudioMidiDriver::get_active(); }

template <typename Time, typename Size>
uint32_t DummyAudioMidiDriver<Time, Size>::get_last_processed() const { return AudioMidiDriver::get_last_processed(); }

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::wait_process() { AudioMidiDriver::wait_process(); }

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::queue_process_thread_command(std::function<void()> fn) { AudioMidiDriver::queue_process_thread_command(std::move(fn)); }

template <typename Time, typename Size>
void DummyAudioMidiDriver<Time, Size>::exec_process_thread_command(std::function<void()> fn) { AudioMidiDriver::exec_process_thread_command(std::move(fn)); }

template <typename Time, typename Size>
backend_rust::CommandQueue &DummyAudioMidiDriver<Time, Size>::get_command_queue() { return AudioMidiDriver::get_command_queue(); }

template class DummyAudioMidiDriver<uint32_t, uint16_t>;
template class DummyAudioMidiDriver<uint32_t, uint32_t>;
template class DummyAudioMidiDriver<uint16_t, uint16_t>;
template class DummyAudioMidiDriver<uint16_t, uint32_t>;
template class DummyAudioMidiDriver<uint32_t, uint64_t>;
template class DummyAudioMidiDriver<uint64_t, uint64_t>;