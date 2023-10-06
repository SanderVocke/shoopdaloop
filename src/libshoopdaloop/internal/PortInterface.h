#pragma once
#include "ExternalUIInterface.h"
#include <string>
#include <map>

enum class PortDirection {
    Input,
    Output
};

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
    virtual PortDirection direction() const = 0;
    virtual const char* name() const = 0;
    virtual PortType type() const = 0;

    virtual PortExternalConnectionStatus get_external_connection_status() const = 0;
    virtual void connect_external(std::string name) = 0;
    virtual void disconnect_external(std::string name) = 0;

    PortInterface() {}
    virtual ~PortInterface() {};
};