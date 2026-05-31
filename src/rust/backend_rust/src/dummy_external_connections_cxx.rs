//! CXX bridge for DummyExternalConnections to expose to C++.

use crate::dummy_external_connections::DummyExternalConnections;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    // Shared structs
    struct ExternalPortDescriptor {
        name: String,
        direction: u32,
        data_type: u32,
    }

    struct ConnectionStatusPair {
        name: String,
        connected: bool,
    }

    extern "Rust" {
        type DummyExternalConnections;

        fn new_dummy_external_connections() -> Box<DummyExternalConnections>;

        fn add_external_mock_port(
            self: &mut DummyExternalConnections,
            name: &str,
            direction: u32,
            data_type: u32,
        );

        fn remove_external_mock_port(self: &mut DummyExternalConnections, name: &str);
        fn remove_all_external_mock_ports(self: &mut DummyExternalConnections);

        fn connect(self: &mut DummyExternalConnections, port_ptr: usize, external_port_name: &str);
        fn disconnect(
            self: &mut DummyExternalConnections,
            port_ptr: usize,
            external_port_name: &str,
        );

        // These are declared as free functions taking &DummyExternalConnections
        // to avoid type name conflicts with the Rust impl module's types.
        fn dummy_external_connections_status_of(
            conn: &DummyExternalConnections,
            port_ptr: usize,
        ) -> Vec<ConnectionStatusPair>;

        fn dummy_external_connections_find_external_ports(
            conn: &DummyExternalConnections,
            maybe_name_regex: &str,
            direction_filter: u32,
            data_type_filter: u32,
        ) -> Vec<ExternalPortDescriptor>;
    }
}

fn new_dummy_external_connections() -> Box<DummyExternalConnections> {
    Box::new(DummyExternalConnections::new())
}

fn dummy_external_connections_status_of(
    conn: &DummyExternalConnections,
    port_ptr: usize,
) -> Vec<ffi::ConnectionStatusPair> {
    conn.connection_status_of(port_ptr)
        .into_iter()
        .map(|p| ffi::ConnectionStatusPair {
            name: p.name,
            connected: p.connected,
        })
        .collect()
}

fn dummy_external_connections_find_external_ports(
    conn: &DummyExternalConnections,
    maybe_name_regex: &str,
    direction_filter: u32,
    data_type_filter: u32,
) -> Vec<ffi::ExternalPortDescriptor> {
    conn.find_external_ports(maybe_name_regex, direction_filter, data_type_filter)
        .into_iter()
        .map(|desc| ffi::ExternalPortDescriptor {
            name: desc.name,
            direction: desc.direction,
            data_type: desc.data_type,
        })
        .collect()
}
