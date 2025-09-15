#pragma once
#include "AudioMidiDriver.h"
#include "PortInterface.h"
#include "LoggingEnabled.h"
#include "types.h"
#include <memory>
#include <set>
#include <thread>
#include <vector>
#include <memory>
#include <stdint.h>

class DummyPort;

struct DummyExternalConnections : private ModuleLoggingEnabled<"Backend.DummyExternalConnections"> {
    std::vector<std::pair<DummyPort*, std::string>> m_external_connections;
    std::vector<ExternalPortDescriptor> m_external_mock_ports;

    void add_external_mock_port(std::string name, shoop_port_direction_t direction, shoop_port_data_type_t data_type);
    void remove_external_mock_port(std::string name);
    void remove_all_external_mock_ports();

    void connect(DummyPort* port, std::string external_port_name);
    void disconnect(DummyPort* port, std::string external_port_name);

    ExternalPortDescriptor &get_port(std::string name);

    std::vector<ExternalPortDescriptor> find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    );

    PortExternalConnectionStatus connection_status_of(const DummyPort* p);
};

class DummyPort : public virtual PortInterface {
protected:
    std::string m_name = "";
    shoop_port_direction_t m_direction = ShoopPortDirection_Input;
    shoop_weak_ptr<DummyExternalConnections> m_external_connections;

public:
    DummyPort(
        std::string name,
        shoop_port_direction_t direction,
        PortDataType type,
        shoop_weak_ptr<DummyExternalConnections> external_connections = shoop_weak_ptr<DummyExternalConnections>()
    );

    const char* name() const override;
    void close() override;
    void *maybe_driver_handle() const override;

    PortExternalConnectionStatus get_external_connection_status() const override;
    void connect_external(std::string name) override;
    void disconnect_external(std::string name) override;
};