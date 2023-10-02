#include "JackAudioSystem.h"
#include "JackMidiPort.h"
#include "MidiPortInterface.h"
#include "PortInterface.h"
#include "JackAudioPort.h"
#include <jack_wrappers.h>
#include <stdexcept>
#include <memory>
#include <atomic>

int JackAudioSystem::PROC_process_cb_static(jack_nframes_t nframes, void *arg) {
    auto &inst = *((JackAudioSystem *)arg);
    return inst.PROC_process_cb_inst(nframes);
}

int JackAudioSystem::PROC_xrun_cb_static(void *arg) {
    auto &inst = *((JackAudioSystem *)arg);
    return inst.PROC_xrun_cb_inst();
}

void JackAudioSystem::PROC_port_connect_cb_static(jack_port_id_t a, jack_port_id_t b, int connect, void *arg) {
    auto &inst = *((JackAudioSystem *)arg);
    inst.PROC_update_ports_cb_inst();
}

void JackAudioSystem::PROC_port_registration_cb_static(jack_port_id_t port, int, void *arg) {
    auto &inst = *((JackAudioSystem *)arg);
    inst.PROC_update_ports_cb_inst();
}

void JackAudioSystem::PROC_port_rename_cb_static(jack_port_id_t port, const char *old_name, const char *new_name, void *arg) {
    auto &inst = *((JackAudioSystem *)arg);
    inst.PROC_update_ports_cb_inst();
}

int JackAudioSystem::PROC_process_cb_inst(jack_nframes_t nframes) {
    if (m_process_cb) {
        m_process_cb((size_t)nframes);
    }
    return 0;
}

int JackAudioSystem::PROC_xrun_cb_inst() {
    m_xruns++;
    return 0;
}

void JackAudioSystem::PROC_update_ports_cb_inst() {
    m_all_ports_tracker->update(m_client);
}

JackAudioSystem::JackAudioSystem(std::string client_name,
                                 std::function<void(size_t)> process_cb)
    : AudioSystemInterface(client_name, process_cb), m_client_name(client_name),
      m_process_cb(process_cb), m_all_ports_tracker(std::make_shared<JackAllPorts>()) {
    // We use a wrapper which dlopens Jack to not have a hard linkage
    // dependency. It needs to be initialized first.
    if (initialize_jack_wrappers(0)) {
        throw std::runtime_error("Unable to find Jack client library.");
    }

    jack_status_t status;

    m_client = jack_client_open(client_name.c_str(), JackNullOption, &status);

    if (m_client == nullptr) {
        throw std::runtime_error("Unable to open JACK client.");
    }

    if (status && JackNameNotUnique) {
        m_client_name = std::string(jack_get_client_name(m_client));
    }

    jack_set_process_callback(m_client, JackAudioSystem::PROC_process_cb_static,
                              (void *)this);
    jack_set_xrun_callback(m_client, JackAudioSystem::PROC_xrun_cb_static,
                           (void *)this);
    jack_set_port_connect_callback(m_client,
                                   JackAudioSystem::PROC_port_connect_cb_static,
                                   (void *)this);
    jack_set_port_registration_callback(m_client,
                                   JackAudioSystem::PROC_port_registration_cb_static,
                                   (void *)this);
    jack_set_port_rename_callback(m_client,
                                   JackAudioSystem::PROC_port_rename_cb_static,
                                   (void *)this);
    
    m_all_ports_tracker->update(m_client);
}

void JackAudioSystem::start() {
    if (jack_activate(m_client)) {
        throw std::runtime_error("Could not activate JACK client.");
    }
}

JackAudioSystem::~JackAudioSystem() {
    close();
    m_client = nullptr;
}

std::shared_ptr<AudioPortInterface<float>>
JackAudioSystem::open_audio_port(std::string name, PortDirection direction) {
    std::shared_ptr<PortInterface> port =
        std::static_pointer_cast<PortInterface>(
            std::make_shared<JackAudioPort>(name, direction, m_client, m_all_ports_tracker)
        );
    m_ports[port->name()] = port;
    return std::dynamic_pointer_cast<AudioPortInterface<float>>(port);
}

std::shared_ptr<MidiPortInterface>
JackAudioSystem::open_midi_port(std::string name, PortDirection direction) {
    std::shared_ptr<PortInterface> port =
        std::static_pointer_cast<PortInterface>(
            std::make_shared<JackMidiPort>(name, direction, m_client, m_all_ports_tracker)
        );
    m_ports[port->name()] = port;
    return std::dynamic_pointer_cast<MidiPortInterface>(port);
}

size_t JackAudioSystem::get_sample_rate() const {
    return jack_get_sample_rate(m_client);
}

size_t JackAudioSystem::get_buffer_size() const {
    return jack_get_buffer_size(m_client);
}

void *JackAudioSystem::maybe_client_handle() const { return (void *)m_client; }

const char *JackAudioSystem::client_name() const {
    return m_client_name.c_str();
}

void JackAudioSystem::close() {
    if (m_client) {
        jack_client_close(m_client);
    }
}

size_t JackAudioSystem::get_xruns() const { return m_xruns; }

void JackAudioSystem::reset_xruns() { m_xruns = 0; }