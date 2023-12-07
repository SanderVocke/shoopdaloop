#include "JackPort.h"
#include "JackAllPorts.h"
#include "LoggingBackend.h"
#include "PortInterface.h"
#include <algorithm>
#include <stdexcept>

template<typename API>
const char* GenericJackPort<API>::name() const { return m_name.c_str(); }


template<typename API>
void GenericJackPort<API>::close() {
    if (m_port) {
        API::port_unregister(m_client, m_port);
        m_port = nullptr;
    }
}

template<typename API>
void *GenericJackPort<API>::maybe_driver_handle() const { return (void*)m_port; }

template<typename API>
GenericJackPort<API>::~GenericJackPort() { close(); }

template<typename API>
PortType GenericJackPort<API>::type() const { return m_type; }

template<typename API>
GenericJackPort<API>::GenericJackPort(std::string name,
                   shoop_port_direction_t direction,
                   PortDataType type,
                   jack_client_t *client,
                   std::shared_ptr<GenericJackAllPorts<API>> all_ports_tracker)
    : m_client(client), m_type(type), m_direction(direction), m_all_ports_tracker(all_ports_tracker) {

    log<log_level_debug>("Opening JACK port: {}", name);

    auto p = API::port_register(
        m_client,
        name.c_str(),
        m_type == PortType::Audio ? JACK_DEFAULT_AUDIO_TYPE : JACK_DEFAULT_MIDI_TYPE,
        direction == shoop_port_direction_t::Input ? JackPortIsInput : JackPortIsOutput,
        0);

    if (p == nullptr) {
        throw std::runtime_error("Unable to open port.");
    }

    m_port = p;
    m_name = std::string(API::port_name(m_port));
}

template<typename API>
PortExternalConnectionStatus GenericJackPort<API>::get_external_connection_status() const {
    if (!m_port || API::get_client_name(m_client) == nullptr) {
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
    const char ** connected_ports = API::port_get_all_connections(m_client, m_port);
    for(auto n = connected_ports; n != nullptr && *n != nullptr; n++) {
        std::string _n(*n);
        rval[_n] = true;
    }

    return rval;
}

template<typename API>
void GenericJackPort<API>::connect_external(std::string name) {
    if (!m_port || API::get_client_name(m_client) == nullptr) {
        return;
    }

    if (m_direction == shoop_port_direction_t::Input) {
        API::connect(m_client, name.c_str(), API::port_name(m_port));
    } else {
        API::connect(m_client, API::port_name(m_port), name.c_str());
    }
}

template<typename API>
void GenericJackPort<API>::disconnect_external(std::string name) {
    if (!m_port || API::get_client_name(m_client) == nullptr) {
        return;
    }

    if (m_direction == shoop_port_direction_t::Input) {
        API::disconnect(m_client, name.c_str(), API::port_name(m_port));
    } else {
        API::disconnect(m_client, API::port_name(m_port), name.c_str());
    }
}

template<typename API>
PortConnectivityType GenericJackPort<API>::input_connectivity() const {
    return m_direction == Input ? PortConnectivityType::External : PortConnectivityType::Internal;
}

template<typename API>
PortConnectivityType GenericJackPort<API>::output_connectivity() const {
    return m_direction == Output ? PortConnectivityType::External : PortConnectivityType::Internal;
}

template<typename API>
void GenericJackPort<API>::PROC_prepare(uint32_t nframes) {
    m_buffer = API::port_get_buffer(m_port, nframes);
}

template class GenericJackPort<JackApi>;
template class GenericJackPort<JackTestApi>;