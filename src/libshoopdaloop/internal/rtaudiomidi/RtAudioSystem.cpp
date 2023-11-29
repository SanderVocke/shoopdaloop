#include "RtAudioSystem.h"
#include "LoggingBackend.h"
#include "MidiPortInterface.h"
#include "PortInterface.h"

template<typename API>
GenericRtAudioSystem<API>::GenericRtAudioSystem(
        std::string device_name,
        size_t input_channels,
        size_t output_channels,
        size_t sample_rate,
        size_t buffer_size,
        size_t n_buffers,
        std::function<void(uint32_t)> process_cb
    )
{
    int bufsize = (int) buffer_size;
    m_api = new API(
        0, // device ID, update
        (int) output_channels,
        0, // device ID, update
        (int) input_channels,
        RTAUDIO_FLOAT32, // Is this correct?
        (int) sample_rate,
        &bufsize,
        (int)n_buffers
    );
}

template<typename API>
void GenericJackAudioSystem<API>::start() {
    if (API::activate(m_client)) {
        throw_error<std::runtime_error>("Could not activate JACK client.");
    }
}

template<typename API>
GenericJackAudioSystem<API>::~GenericJackAudioSystem() {
    close();
    m_client = nullptr;
}

template<typename API>
std::shared_ptr<AudioPortInterface<float>>GenericJackAudioSystem<API>::open_audio_port(std::string name, PortDirection direction) {
    std::shared_ptr<PortInterface> port =
        std::static_pointer_cast<PortInterface>(
            std::make_shared<GenericJackAudioPort<API>>(name, direction, m_client, m_all_ports_tracker)
        );
    m_ports[port->name()] = port;
    return std::dynamic_pointer_cast<AudioPortInterface<float>>(port);
}

template<typename API>
std::shared_ptr<MidiPortInterface> GenericJackAudioSystem<API>::open_midi_port(std::string name, PortDirection direction) {
    std::shared_ptr<PortInterface> port =
        std::static_pointer_cast<PortInterface>(
            std::make_shared<GenericJackMidiPort<API>>(name, direction, m_client, m_all_ports_tracker)
        );
    m_ports[port->name()] = port;
    return std::dynamic_pointer_cast<MidiPortInterface>(port);
}

template<typename API>
uint32_t GenericJackAudioSystem<API>::get_sample_rate() const {
    return API::get_sample_rate(m_client);
}

template<typename API>
uint32_t GenericJackAudioSystem<API>::get_buffer_size() const {
    return API::get_buffer_size(m_client);
}

template<typename API>
void *GenericJackAudioSystem<API>::maybe_client_handle() const { return (void *)m_client; }

template<typename API>
const char *GenericJackAudioSystem<API>::client_name() const {
    return m_client_name.c_str();
}

template<typename API>
void GenericJackAudioSystem<API>::close() {
    if (m_client) {
        log<debug>("Closing JACK client.");
        try {
            run_in_thread_with_timeout_unsafe([this]() { API::client_close(m_client); }, 10000);
        } catch (std::exception &e) {
            log<warning>("Attempt to close JACK client failed: {}. Abandoning.", e.what());
        }
        m_client = nullptr;
    }
}

template<typename API>
uint32_t GenericJackAudioSystem<API>::get_xruns() const { return m_xruns; }

template<typename API>
void GenericJackAudioSystem<API>::reset_xruns() { m_xruns = 0; }

template class GenericRtAudioSystem<RtAudioApi>;