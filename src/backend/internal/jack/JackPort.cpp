#include "JackPort.h"
#include "JackAllPorts.h"
#include "LoggingBackend.h"
#include "PortInterface.h"
#include <algorithm>
#include <stdexcept>
#include <thread>
#include <chrono>

const char* JackPort::name() const { return m_name.c_str(); }

void JackPort::close() {
    if (m_port) {
        log<log_level_debug>("Closing JACK port: {}", m_name);
        m_api->port_unregister(m_client, m_port);
        m_port = nullptr;
    }
}

void *JackPort::maybe_driver_handle() const { return (void*)m_port; }

jack_port_t *JackPort::get_jack_port() const { return m_port; }

void *JackPort::get_buffer() const { return m_buffer.load(); }

JackPort::~JackPort() { close(); }

JackPort::JackPort(std::string name,
                   shoop_port_direction_t direction,
                   PortDataType type,
                   jack_client_t *client,
                   std::shared_ptr<JackAllPorts> all_ports_tracker,
                   std::shared_ptr<IJackApi> api)
    : m_api(std::move(api)), m_client(client), m_type(type), m_direction(direction), m_all_ports_tracker(std::move(all_ports_tracker)) {

    log<log_level_debug>("Opening JACK port: {}", name);

    jack_port_t* p = nullptr;

    // Try a few times, we may have race conditions with previously opened and now closing ports
    for (size_t tries = 0; tries < 10; tries++) {
        p = m_api->port_register(
            m_client,
            name.c_str(),
            m_type == PortDataType::Audio ? JACK_DEFAULT_AUDIO_TYPE : JACK_DEFAULT_MIDI_TYPE,
            direction == shoop_port_direction_t::ShoopPortDirection_Input ? JackPortIsInput : JackPortIsOutput,
            0);
        if (p != nullptr) { break; }

        std::this_thread::sleep_for(std::chrono::milliseconds(20));
    }

    if (p == nullptr) {
        throw std::runtime_error("Unable to open port: " + name);
    }

    m_port = p;
    m_name = std::string(m_api->port_name(m_port));
}

PortExternalConnectionStatus JackPort::get_external_connection_status() const {
    if (!m_port || m_api->get_client_name(m_client) == nullptr) {
        return PortExternalConnectionStatus {};
    }

    // Get list of ports we can connect to
    auto entries = m_all_ports_tracker->get();
    decltype(entries) eligible_ports;
    std::copy_if (entries.begin(), entries.end(), std::back_inserter(eligible_ports),
        [this](JackAllPortsEntry const& entry) {
            return entry.direction != m_direction &&
                   entry.type == m_type;
        }
    );

    // Create entries
    PortExternalConnectionStatus rval;
    for (auto const& n : eligible_ports) {
        rval[n.name] = false;
    }

    // Get list of port names we are connected to and update/create entries
    const char ** connected_ports = m_api->port_get_all_connections(m_client, m_port);
    for(auto n = connected_ports; n != nullptr && *n != nullptr; n++) {
        std::string _n(*n);
        rval[_n] = true;
    }

    return rval;
}

void JackPort::connect_external(std::string name) {
    if (!m_port || m_api->get_client_name(m_client) == nullptr) {
        return;
    }

    if (m_direction == shoop_port_direction_t::ShoopPortDirection_Input) {
        m_api->connect(m_client, name.c_str(), m_api->port_name(m_port));
    } else {
        m_api->connect(m_client, m_api->port_name(m_port), name.c_str());
    }
}

void JackPort::disconnect_external(std::string name) {
    if (!m_port || m_api->get_client_name(m_client) == nullptr) {
        return;
    }

    if (m_direction == shoop_port_direction_t::ShoopPortDirection_Input) {
        m_api->disconnect(m_client, name.c_str(), m_api->port_name(m_port));
    } else {
        m_api->disconnect(m_client, m_api->port_name(m_port), name.c_str());
    }
}

void JackPort::PROC_prepare(uint32_t nframes) {
    m_buffer = m_api->port_get_buffer(m_port, nframes);
}
