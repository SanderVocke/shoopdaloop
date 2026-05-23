#pragma once
#include "AudioMidiDriver.h"
#include "PortInterface.h"
#include "LoggingEnabled.h"
#include "types.h"
#include "shoop_shared_ptr.h"
#include "backend_rust/src/dummy_external_connections_cxx.rs.h"
#include "backend_rust/src/port_core_cxx.rs.h"
#include <memory>
#include <vector>
#include <stdint.h>

struct DummyPortCore;

struct DummyExternalConnections : private ModuleLoggingEnabled<"Backend.DummyExternalConnections"> {
    rust::Box<backend_rust::DummyExternalConnections> m_rust;

    DummyExternalConnections() : m_rust(backend_rust::new_dummy_external_connections()) {}

    void add_external_mock_port(std::string name, shoop_port_direction_t direction, shoop_port_data_type_t data_type) {
        m_rust->add_external_mock_port(name, (uint32_t)direction, (uint32_t)data_type);
    }
    void remove_external_mock_port(std::string name) {
        m_rust->remove_external_mock_port(name);
    }
    void remove_all_external_mock_ports() {
        m_rust->remove_all_external_mock_ports();
    }

    void connect(DummyPortCore* port, std::string external_port_name);
    void disconnect(DummyPortCore* port, std::string external_port_name);

    std::vector<ExternalPortDescriptor> find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    );

    PortExternalConnectionStatus connection_status_of(const DummyPortCore* p);
};

/**
 * DummyPortCore - Composable struct providing dummy port metadata and
 * external connection management.
 *
 * Replaces the former DummyPort base class. Used as a member in
 * DummyAudioPort and DummyMidiPort via composition instead of inheritance.
 *
 * The m_driver_handle is set by the owning port object so that
 * maybe_driver_handle() returns a stable identity for the outer object.
 */
struct DummyPortCore {
    rust::Box<backend_rust::PortCore> m_rust;
    shoop_weak_ptr<DummyExternalConnections> m_external_connections_cpp;
    std::string m_name;  // Cached name to avoid dangling pointer from Rust String

    DummyPortCore(
        std::string name,
        shoop_port_direction_t direction,
        void* driver_handle,
        shoop_weak_ptr<DummyExternalConnections> external_connections = shoop_weak_ptr<DummyExternalConnections>()
    );

    const char* name() const;
    shoop_port_direction_t direction() const;
    void close();
    void *maybe_driver_handle() const;

    PortExternalConnectionStatus get_external_connection_status() const;
    void connect_external(std::string name);
    void disconnect_external(std::string name);
};
