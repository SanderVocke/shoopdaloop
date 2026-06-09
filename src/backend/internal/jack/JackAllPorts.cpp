#include "JackAllPorts.h"
#include "jack_wrappers.h"

JackAllPorts::JackAllPorts(std::shared_ptr<IJackApi> api)
    : m_api(std::move(api)), m_cache(std::vector<JackAllPortsEntry>()) {}

void JackAllPorts::update(jack_client_t *client) {
    auto new_cache = std::vector<JackAllPortsEntry>();
    const char** port_names = m_api->get_ports(client, nullptr, nullptr, 0);

    for(auto port_name = port_names; port_name != nullptr && *port_name != nullptr; port_name++) {
        auto _port_name = std::string(*port_name);
        auto port = m_api->port_by_name(client, _port_name.c_str());
        if (!port) { continue; }
        JackAllPortsEntry entry;
        entry.name = _port_name;
        entry.direction = m_api->port_flags(port) & JackPortIsInput ? shoop_port_direction_t::ShoopPortDirection_Input : shoop_port_direction_t::ShoopPortDirection_Output;
        auto port_type_str = m_api->port_type(port);
        if (!port_type_str) { continue; }
        entry.type = std::string(port_type_str) == JACK_DEFAULT_AUDIO_TYPE ? PortDataType::Audio : PortDataType::Midi;
        entry.connections = std::vector<std::string>();
        const char** connected_ports = m_api->port_get_all_connections(client, port);
        for(auto connected_port = connected_ports; connected_port != nullptr && *connected_port != nullptr; connected_port++) {
            if (connected_port) {
                entry.connections.push_back(std::string(*connected_port));
            }
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
