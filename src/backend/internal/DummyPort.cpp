
#include "DummyPort.h"
#include <algorithm>
#include <regex>

std::vector<ExternalPortDescriptor> DummyExternalConnections::find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    )
{
    std::vector<ExternalPortDescriptor> rval;
    for(auto &p : m_external_mock_ports) {
        bool name_matched = (!maybe_name_regex || std::regex_match(p.name, std::regex(std::string(maybe_name_regex))));
        bool direction_matched = (maybe_direction_filter == ShoopPortDirection_Any) || maybe_direction_filter == p.direction;
        bool data_type_matched = (maybe_data_type_filter == ShoopPortDataType_Any) || maybe_data_type_filter == p.data_type;
        if (name_matched && direction_matched && data_type_matched) {
            rval.push_back(p);
        }
    }

    return rval;
}


DummyPort::DummyPort(
    std::string name,
    shoop_port_direction_t direction,
    PortDataType type,
    shoop_weak_ptr<DummyExternalConnections> external_connections
) : m_name(name), m_direction(direction), m_external_connections(external_connections) {}

const char* DummyPort::name() const { return m_name.c_str(); }

void DummyPort::close() {}

PortExternalConnectionStatus DummyPort::get_external_connection_status() const {
    if (auto e = m_external_connections.lock()) {
        return e->connection_status_of(this);
    }
    return PortExternalConnectionStatus();
}

void DummyPort::connect_external(std::string name) {
    if (auto e = m_external_connections.lock()) {
        e->connect(this, name);
    }
}

void DummyPort::disconnect_external(std::string name) {
    if (auto e = m_external_connections.lock()) {
        e->disconnect(this, name);
    }
}

void *DummyPort::maybe_driver_handle() const {
    return (void*)this;
}