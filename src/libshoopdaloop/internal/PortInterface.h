#pragma once
#include <string>
#include <map>
#include "types.h"
#include <stdint.h>

enum class PortType {
    Audio,
    Midi
};

// Used for abstractly connecting a port to external entities.
// The status includes the possible entities to connect to and
// whether the port is currently connected.
using PortExternalConnectionStatus = std::map<std::string, bool>;

class PortInterface {
public:

    virtual void close() = 0;
    virtual shoop_port_direction_t direction() const = 0;
    virtual const char* name() const = 0;
    virtual PortType type() const = 0;
    virtual void* maybe_driver_handle() const = 0;

    virtual PortExternalConnectionStatus get_external_connection_status() const = 0;
    virtual void connect_external(std::string name) = 0;
    virtual void disconnect_external(std::string name) = 0;

    virtual void PROC_change_buffer_size(uint32_t buffer_size) {}

    PortInterface() {}
    virtual ~PortInterface() {};
};