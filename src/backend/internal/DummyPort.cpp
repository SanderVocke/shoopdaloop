
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


DummyPortCore::DummyPortCore(
    std::string name,
    shoop_port_direction_t direction,
    void* driver_handle,
    shoop_weak_ptr<DummyExternalConnections> external_connections
) : m_name(name), m_direction(direction), m_driver_handle(driver_handle), m_external_connections(external_connections) {
}

const char* DummyPortCore::name() const { return m_name.c_str(); }

void DummyPortCore::close() {}

PortExternalConnectionStatus DummyPortCore::get_external_connection_status() const {
    if (auto e = m_external_connections.lock()) {
        return e->connection_status_of(this);
    }
    return PortExternalConnectionStatus();
}

void DummyPortCore::connect_external(std::string name) {
    if (auto e = m_external_connections.lock()) {
        e->connect(this, name);
    }
}

void DummyPortCore::disconnect_external(std::string name) {
    if (auto e = m_external_connections.lock()) {
        e->disconnect(this, name);
    }
}

void *DummyPortCore::maybe_driver_handle() const {
    return m_driver_handle;
}