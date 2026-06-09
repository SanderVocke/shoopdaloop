#pragma once
#include "../PortInterface.h"
#include "backend_rust/src/jack_api_cxx.rs.h"
#include <cstdint>
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
    rust::Box<backend_rust::JackApiBridgeStrong> m_api;
    std::vector<JackAllPortsEntry> m_cache;
    std::mutex m_cache_mutex;

public:
    explicit JackAllPorts(rust::Box<backend_rust::JackApiBridgeStrong> api);

    void update(uintptr_t client);
    std::vector<JackAllPortsEntry> get();
};
