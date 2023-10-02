#pragma once
#include "PortInterface.h"
#include <jack/types.h>
#include <jack_wrappers.h>
#include <vector>
#include <memory>

struct JackAllPortsEntry {
    std::string name;
    PortDirection direction;
    PortType type;
    std::vector<std::string> connections;
};

class JackAllPorts {
    std::shared_ptr<std::vector<JackAllPortsEntry>> m_cache;

public:
    JackAllPorts();

    void update(jack_client_t *client);
    std::vector<JackAllPortsEntry> get() const;
};