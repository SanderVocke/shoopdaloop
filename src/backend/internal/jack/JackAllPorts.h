#pragma once
#include "PortInterface.h"
#include "JackApi.h"
#include <jack/types.h>
#include <memory>
#include <mutex>
#include <vector>

struct JackAllPortsEntry {
    std::string name = "";
    shoop_port_direction_t direction = ShoopPortDirection_Input;
    PortDataType type;
    std::vector<std::string> connections;
};

class JackAllPorts {
    std::shared_ptr<IJackApi> m_api;
    std::vector<JackAllPortsEntry> m_cache;
    std::mutex m_cache_mutex;

public:
    explicit JackAllPorts(std::shared_ptr<IJackApi> api);

    void update(jack_client_t *client);
    std::vector<JackAllPortsEntry> get();
};
