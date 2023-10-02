#include "JackAllPorts.h"
#include "jack_wrappers.h"

JackAllPorts::JackAllPorts() : m_cache(std::make_shared<std::vector<JackAllPortsEntry>>()) {}

void JackAllPorts::update(jack_client_t *client) {
    auto new_cache = std::make_shared<std::vector<JackAllPortsEntry>>();
    const char** port_names = jack_get_ports(client, nullptr, nullptr, 0);

    for(auto port_name = port_names; port_name != nullptr; port_name++) {
        auto _port_name = std::string(*port_name);
        auto port = jack_port_by_name(client, _port_name.c_str());
        JackAllPortsEntry entry;
        entry.name = _port_name;
        entry.direction = jack_port_flags(port) & JackPortIsInput ? PortDirection::Input : PortDirection::Output;
        entry.type = std::string(jack_port_type(port)) == JACK_DEFAULT_AUDIO_TYPE ? PortType::Audio : PortType::Midi;
        entry.connections = std::vector<std::string>();
        const char** connected_ports = jack_port_get_all_connections(client, port);
        for(auto connected_port = connected_ports; connected_port != nullptr; connected_port++) {
            entry.connections.push_back(std::string(*connected_port));
        }

        new_cache->push_back(entry);
    }

    m_cache = new_cache;
}

std::vector<JackAllPortsEntry> JackAllPorts::get() const {
    return *m_cache;
}