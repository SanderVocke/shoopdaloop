#include "JackPort.h"
#include "JackAllPorts.h"
#include "PortInterface.h"
#include <algorithm>
#include <stdexcept>

const char *JackPort::name() const { return m_name.c_str(); }

PortDirection JackPort::direction() const { return m_direction; }

void JackPort::close() { jack_port_unregister(m_client, m_port); }

jack_port_t *JackPort::get_jack_port() const { return m_port; }

JackPort::~JackPort() { close(); }

PortType JackPort::type() const { return m_type; }

JackPort::JackPort(std::string name,
                   PortDirection direction,
                   PortType type,
                   jack_client_t *client,
                   std::shared_ptr<JackAllPorts> all_ports_tracker)
    : m_client(client), m_type(type), m_direction(direction), m_all_ports_tracker(all_ports_tracker) {

    auto p = jack_port_register(
        m_client,
        name.c_str(),
        m_type == PortType::Audio ? JACK_DEFAULT_AUDIO_TYPE : JACK_DEFAULT_MIDI_TYPE,
        direction == PortDirection::Input ? JackPortIsInput : JackPortIsOutput,
        0);

    if (p == nullptr) {
        throw std::runtime_error("Unable to open port.");
    }

    m_name = std::string(jack_port_name(m_port));
}

PortExternalConnectionStatus JackPort::get_external_connection_status() const {
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
    const char ** connected_ports = jack_port_get_all_connections(m_client, m_port);
    for(auto n = connected_ports; n != nullptr; n++) {
        std::string _n(*n);
        rval[_n] = true;
    }

    return rval;
}

void JackPort::connect_external(std::string name) {
    jack_connect(m_client, jack_port_name(m_port), name.c_str());
}

void JackPort::disconnect_external(std::string name) {
    jack_disconnect(m_client, jack_port_name(m_port), name.c_str());
}