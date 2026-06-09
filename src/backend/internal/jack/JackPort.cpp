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
        backend_rust::jack_api_port_unregister(*m_api, m_client, m_port);
        m_port = 0;
    }
}

void *JackPort::maybe_driver_handle() const { return reinterpret_cast<void*>(m_port); }

uintptr_t JackPort::get_jack_port() const { return m_port; }

void *JackPort::get_buffer() const { return m_buffer.load(); }

JackPort::~JackPort() { close(); }

JackPort::JackPort(std::string name,
                   shoop_port_direction_t direction,
                   PortDataType type,
                   uintptr_t client,
                   std::shared_ptr<JackAllPorts> all_ports_tracker,
                   rust::Box<backend_rust::JackApiBridgeStrong> api)
    : m_api(std::move(api)), m_client(client), m_type(type), m_direction(direction), m_all_ports_tracker(std::move(all_ports_tracker)) {

    log<log_level_debug>("Opening JACK port: {}", name);

    uintptr_t p = 0;

    for (size_t tries = 0; tries < 10; tries++) {
        p = backend_rust::jack_api_port_register(
            *m_api,
            m_client,
            name,
            m_type == PortDataType::Audio ? 0u : 1u,
            direction == shoop_port_direction_t::ShoopPortDirection_Input ? 0u : 1u);
        if (p != 0) { break; }

        std::this_thread::sleep_for(std::chrono::milliseconds(20));
    }

    if (p == 0) {
        throw std::runtime_error("Unable to open port: " + name);
    }

    m_port = p;
    m_name = std::string(backend_rust::jack_api_port_name(*m_api, m_port));
}

PortExternalConnectionStatus JackPort::get_external_connection_status() const {
    if (!m_port || backend_rust::jack_api_get_client_name(*m_api, m_client).empty()) {
        return PortExternalConnectionStatus {};
    }

    auto entries = m_all_ports_tracker->get();
    decltype(entries) eligible_ports;
    std::copy_if (entries.begin(), entries.end(), std::back_inserter(eligible_ports),
        [this](JackAllPortsEntry const& entry) {
            return entry.direction != m_direction &&
                   entry.type == m_type;
        }
    );

    PortExternalConnectionStatus rval;
    for (auto const& n : eligible_ports) {
        rval[n.name] = false;
    }

    auto connected_ports = backend_rust::jack_api_port_get_all_connections(*m_api, m_client, m_port);
    for (auto const &n : connected_ports) {
        rval[std::string(n)] = true;
    }

    return rval;
}

void JackPort::connect_external(std::string name) {
    if (!m_port || backend_rust::jack_api_get_client_name(*m_api, m_client).empty()) {
        return;
    }

    auto port_name = std::string(backend_rust::jack_api_port_name(*m_api, m_port));
    if (m_direction == shoop_port_direction_t::ShoopPortDirection_Input) {
        backend_rust::jack_api_connect(*m_api, m_client, name, port_name);
    } else {
        backend_rust::jack_api_connect(*m_api, m_client, port_name, name);
    }
}

void JackPort::disconnect_external(std::string name) {
    if (!m_port || backend_rust::jack_api_get_client_name(*m_api, m_client).empty()) {
        return;
    }

    auto port_name = std::string(backend_rust::jack_api_port_name(*m_api, m_port));
    if (m_direction == shoop_port_direction_t::ShoopPortDirection_Input) {
        backend_rust::jack_api_disconnect(*m_api, m_client, name, port_name);
    } else {
        backend_rust::jack_api_disconnect(*m_api, m_client, port_name, name);
    }
}

void JackPort::PROC_prepare(uint32_t nframes) {
    m_buffer = reinterpret_cast<void*>(backend_rust::jack_api_port_get_buffer(*m_api, m_port, nframes));
}
