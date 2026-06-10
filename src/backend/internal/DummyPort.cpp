#include "DummyPort.h"
#include <algorithm>

void DummyExternalConnections::connect(DummyPortCore* port, std::string external_port_name) {
    m_rust->connect(reinterpret_cast<size_t>(port), external_port_name);
}

void DummyExternalConnections::disconnect(DummyPortCore* port, std::string external_port_name) {
    m_rust->disconnect(reinterpret_cast<size_t>(port), external_port_name);
}

std::vector<ExternalPortDescriptor> DummyExternalConnections::find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    )
{
    std::string regex_str = maybe_name_regex ? std::string(maybe_name_regex) : "";
    auto rust_vec = backend_rust::dummy_external_connections_find_external_ports(
        *m_rust,
        regex_str,
        (uint32_t)maybe_direction_filter,
        (uint32_t)maybe_data_type_filter
    );
    std::vector<ExternalPortDescriptor> rval;
    rval.reserve(rust_vec.size());
    for (auto &desc : rust_vec) {
        rval.push_back(ExternalPortDescriptor {
            .name = std::string(desc.name),
            .direction = (shoop_port_direction_t)desc.direction,
            .data_type = (shoop_port_data_type_t)desc.data_type,
        });
    }
    return rval;
}

PortExternalConnectionStatus DummyExternalConnections::connection_status_of(const DummyPortCore* p) {
    auto pairs = backend_rust::dummy_external_connections_status_of(*m_rust, reinterpret_cast<size_t>(p));
    PortExternalConnectionStatus rval;
    for (auto &pair : pairs) {
        rval[std::string(pair.name)] = pair.connected;
    }
    return rval;
}

DummyPortCore::DummyPortCore(
    std::string name,
    shoop_port_direction_t direction,
    void* driver_handle,
    std::weak_ptr<DummyExternalConnections> external_connections
) : m_rust(backend_rust::new_port_core(name, direction == ShoopPortDirection_Output, reinterpret_cast<size_t>(driver_handle))),
    m_name(name),
    m_external_connections_cpp(external_connections) {
}

const char* DummyPortCore::name() const { return m_name.c_str(); }

shoop_port_direction_t DummyPortCore::direction() const {
    return m_rust->is_output() ? ShoopPortDirection_Output : ShoopPortDirection_Input;
}

void DummyPortCore::close() {}

PortExternalConnectionStatus DummyPortCore::get_external_connection_status() const {
    if (auto e = m_external_connections_cpp.lock()) {
        return e->connection_status_of(this);
    }
    return PortExternalConnectionStatus();
}

void DummyPortCore::connect_external(std::string name) {
    if (auto e = m_external_connections_cpp.lock()) {
        e->connect(this, name);
    }
}

void DummyPortCore::disconnect_external(std::string name) {
    if (auto e = m_external_connections_cpp.lock()) {
        e->disconnect(this, name);
    }
}

void *DummyPortCore::maybe_driver_handle() const {
    return reinterpret_cast<void*>(m_rust->maybe_driver_handle());
}
