#include "JackAudioSystem.h"
#include "JackMidiPort.h"
#include "LoggingBackend.h"
#include "MidiPortInterface.h"
#include "PortInterface.h"
#include "JackAudioPort.h"
#include <stdexcept>
#include <memory>
#include <atomic>

template<typename API>
std::string GenericJackAudioSystem<API>::log_module_name() const {
    return "Backend.JackAudioSystem";
}

template<typename API>
int GenericJackAudioSystem<API>::PROC_process_cb_static(jack_nframes_t nframes, void *arg) {
    auto &inst = *((GenericJackAudioSystem<API> *)arg);
    return inst.PROC_process_cb_inst(nframes);
}

template<typename API>
int GenericJackAudioSystem<API>::PROC_xrun_cb_static(void *arg) {
    auto &inst = *((GenericJackAudioSystem<API> *)arg);
    return inst.PROC_xrun_cb_inst();
}

template<typename API>
void GenericJackAudioSystem<API>::PROC_port_connect_cb_static(jack_port_id_t a, jack_port_id_t b, int connect, void *arg) {
    auto &inst = *((GenericJackAudioSystem<API> *)arg);
    inst.PROC_update_ports_cb_inst();
}

template<typename API>
void GenericJackAudioSystem<API>::PROC_port_registration_cb_static(jack_port_id_t port, int, void *arg) {
    auto &inst = *((GenericJackAudioSystem<API> *)arg);
    inst.PROC_update_ports_cb_inst();
}

template<typename API>
void GenericJackAudioSystem<API>::PROC_port_rename_cb_static(jack_port_id_t port, const char *old_name, const char *new_name, void *arg) {
    auto &inst = *((GenericJackAudioSystem<API> *)arg);
    inst.PROC_update_ports_cb_inst();
}

template<typename API>
void GenericJackAudioSystem<API>::error_cb_static(const char *msg) {
    std::string _msg = "JACK error: " + std::string(msg);
    logging::get_logger("Backend.JackAudioSystem").log(logging::LogLevel::err, _msg.c_str());
}

template<typename API>
void GenericJackAudioSystem<API>::info_cb_static(const char *msg) {
    std::string _msg = "JACK error: " + std::string(msg);
    logging::get_logger("Backend.JackAudioSystem").log(logging::LogLevel::info, _msg.c_str());
}

template<typename API>
int GenericJackAudioSystem<API>::PROC_process_cb_inst(jack_nframes_t nframes) {
    if (m_process_cb) {
        m_process_cb((size_t)nframes);
    }
    return 0;
}

template<typename API>
int GenericJackAudioSystem<API>::PROC_xrun_cb_inst() {
    m_xruns++;
    return 0;
}

template<typename API>
void GenericJackAudioSystem<API>::PROC_update_ports_cb_inst() {
    m_all_ports_tracker->update(m_client);
}

template<typename API>
GenericJackAudioSystem<API>::GenericJackAudioSystem(std::string client_name,
                                 std::function<void(size_t)> process_cb)
    : AudioSystemInterface(client_name, process_cb), m_client_name(client_name),
      m_process_cb(process_cb), m_all_ports_tracker(std::make_shared<GenericJackAllPorts<API>>()) {
    log_init();

    API::init();

    jack_status_t status;

    log<logging::LogLevel::info>("Opening JACK client with name {}.", client_name);
    m_client = API::client_open(client_name.c_str(), JackNullOption, &status);

    if (m_client == nullptr) {
        throw_error<std::runtime_error>("Unable to open JACK client.");
    }

    if (status && JackNameNotUnique) {
        m_client_name = std::string(API::get_client_name(m_client));
    }

    API::set_process_callback(m_client, GenericJackAudioSystem<API>::PROC_process_cb_static,
                              (void *)this);
    API::set_xrun_callback(m_client, GenericJackAudioSystem<API>::PROC_xrun_cb_static,
                           (void *)this);
    API::set_port_connect_callback(m_client,
                                   GenericJackAudioSystem<API>::PROC_port_connect_cb_static,
                                   (void *)this);
    API::set_port_registration_callback(m_client,
                                   GenericJackAudioSystem<API>::PROC_port_registration_cb_static,
                                   (void *)this);
    API::set_port_rename_callback(m_client,
                                   GenericJackAudioSystem<API>::PROC_port_rename_cb_static,
                                   (void *)this);
    API::set_error_function(GenericJackAudioSystem<API>::error_cb_static);
    API::set_info_function(GenericJackAudioSystem<API>::info_cb_static);
    
    m_all_ports_tracker->update(m_client);
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
size_t GenericJackAudioSystem<API>::get_sample_rate() const {
    return API::get_sample_rate(m_client);
}

template<typename API>
size_t GenericJackAudioSystem<API>::get_buffer_size() const {
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
        API::client_close(m_client);
        m_client = nullptr;
    }
}

template<typename API>
size_t GenericJackAudioSystem<API>::get_xruns() const { return m_xruns; }

template<typename API>
void GenericJackAudioSystem<API>::reset_xruns() { m_xruns = 0; }

template class GenericJackAudioSystem<JackApi>;
template class GenericJackAudioSystem<JackTestApi>;