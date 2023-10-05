#pragma once
#include "PortInterface.h"
#include "JackApi.h"
#include <vector>
#include <mutex>

struct JackAllPortsEntry {
    std::string name;
    PortDirection direction;
    PortType type;
    std::vector<std::string> connections;
};

template<typename API>
class GenericJackAllPorts {
    std::vector<JackAllPortsEntry> m_cache;
    std::mutex m_cache_mutex;

public:
    GenericJackAllPorts();

    void update(jack_client_t *client);
    std::vector<JackAllPortsEntry> get();
};

using JackAllPorts = GenericJackAllPorts<JackApi>;