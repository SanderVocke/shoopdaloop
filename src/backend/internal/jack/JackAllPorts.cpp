#include "JackAllPorts.h"

JackAllPorts::JackAllPorts(rust::Box<backend_rust::JackApiBridgeStrong> api)
    : m_api(std::move(api)), m_cache(std::vector<JackAllPortsEntry>()) {}

void JackAllPorts::update(uintptr_t client) {
    auto new_cache = std::vector<JackAllPortsEntry>();
    auto port_names = backend_rust::jack_api_get_ports(*m_api, client);

    for (auto const &port_name : port_names) {
        auto port = backend_rust::jack_api_port_by_name(*m_api, client, std::string(port_name));
        if (!port) { continue; }
        JackAllPortsEntry entry;
        entry.name = std::string(port_name);
        entry.direction = backend_rust::jack_api_port_is_input(*m_api, port)
            ? shoop_port_direction_t::ShoopPortDirection_Input
            : shoop_port_direction_t::ShoopPortDirection_Output;
        entry.type = backend_rust::jack_api_port_is_audio(*m_api, port)
            ? PortDataType::Audio
            : PortDataType::Midi;
        auto connected_ports = backend_rust::jack_api_port_get_all_connections(*m_api, client, port);
        for (auto const &connected_port : connected_ports) {
            entry.connections.push_back(std::string(connected_port));
        }
        new_cache.push_back(entry);
    }

    {
        std::lock_guard<std::mutex> lock(m_cache_mutex);
        m_cache = new_cache;
    }
}

std::vector<JackAllPortsEntry> JackAllPorts::get() {
    std::lock_guard<std::mutex> lock(m_cache_mutex);
    return m_cache;
}
