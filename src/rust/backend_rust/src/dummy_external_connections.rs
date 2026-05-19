//! DummyExternalConnections - Mock external port registry and connection tracking.
//!
//! Ported from C++ DummyExternalConnections in DummyPort.h/cpp
//!
//! This struct maintains a list of mock external ports and active connections.
//! It is owned by DummyAudioMidiDriver and passed weakly to every DummyPortCore.

use regex::Regex;

/// Descriptor for an external port
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalPortDescriptor {
    pub name: String,
    pub direction: u32, // shoop_port_direction_t as raw value
    pub data_type: u32, // shoop_port_data_type_t as raw value
}

/// Connection status pair for returning connection info across FFI
#[derive(Debug, Clone)]
pub struct ConnectionStatusPair {
    pub name: String,
    pub connected: bool,
}

/// Mock external port registry and connection tracking
#[derive(Debug)]
pub struct DummyExternalConnections {
    mock_ports: Vec<ExternalPortDescriptor>,
    /// Connections: (port_ptr_as_usize, external_port_name)
    /// Storing as usize because CXX cannot hold raw pointers to opaque types.
    connections: Vec<(usize, String)>,
}

impl DummyExternalConnections {
    pub fn new() -> Self {
        DummyExternalConnections {
            mock_ports: Vec::new(),
            connections: Vec::new(),
        }
    }

    /// Add a mock external port
    pub fn add_external_mock_port(&mut self, name: &str, direction: u32, data_type: u32) {
        if !self.mock_ports.iter().any(|p| p.name == name) {
            self.mock_ports.push(ExternalPortDescriptor {
                name: name.to_string(),
                direction,
                data_type,
            });
        }
    }

    /// Remove a mock external port by name
    pub fn remove_external_mock_port(&mut self, name: &str) {
        let had_port = self.mock_ports.iter().any(|p| p.name == name);
        if had_port {
            self.mock_ports.retain(|p| p.name != name);
            // Remove connections also
            self.connections.retain(|(_, port_name)| port_name != name);
        }
    }

    /// Remove all mock external ports
    pub fn remove_all_external_mock_ports(&mut self) {
        self.mock_ports.clear();
        self.connections.clear();
    }

    /// Connect a port to an external port
    pub fn connect(&mut self, port_ptr: usize, external_port_name: &str) {
        let desc = self.get_port(external_port_name);
        let conn = (port_ptr, desc.name.clone());
        if !self.connections.contains(&conn) {
            self.connections.push(conn);
        }
    }

    /// Disconnect a port from an external port
    pub fn disconnect(&mut self, port_ptr: usize, external_port_name: &str) {
        let _desc = self.get_port(external_port_name);
        let conn = (port_ptr, external_port_name.to_string());
        self.connections.retain(|c| *c != conn);
    }

    /// Get a port descriptor by name (panics if not found, matching C++)
    pub fn get_port(&self, name: &str) -> &ExternalPortDescriptor {
        self.mock_ports
            .iter()
            .find(|p| p.name == name)
            .expect("Port not found")
    }

    /// Find external ports matching filters
    pub fn find_external_ports(
        &self,
        maybe_name_regex: &str,
        direction_filter: u32,
        data_type_filter: u32,
    ) -> Vec<ExternalPortDescriptor> {
        let regex = if maybe_name_regex.is_empty() {
            None
        } else {
            Regex::new(maybe_name_regex).ok()
        };

        self.mock_ports
            .iter()
            .filter(|p| {
                let name_matched = regex.as_ref().map(|r| r.is_match(&p.name)).unwrap_or(true);
                let direction_matched = direction_filter == 2 || direction_filter == p.direction; // ShoopPortDirection_Any = 2
                let data_type_matched = data_type_filter == 2 || data_type_filter == p.data_type; // ShoopPortDataType_Any = 2
                name_matched && direction_matched && data_type_matched
            })
            .cloned()
            .collect()
    }

    /// Get connection status of a port
    pub fn connection_status_of(&self, port_ptr: usize) -> Vec<ConnectionStatusPair> {
        self.connections
            .iter()
            .map(|(ptr, name)| ConnectionStatusPair {
                name: name.clone(),
                connected: *ptr == port_ptr,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_get_port() {
        let mut conn = DummyExternalConnections::new();
        conn.add_external_mock_port("test", 0, 0);
        let port = conn.get_port("test");
        assert_eq!(port.name, "test");
    }

    #[test]
    fn test_remove_port() {
        let mut conn = DummyExternalConnections::new();
        conn.add_external_mock_port("test", 0, 0);
        conn.remove_external_mock_port("test");
        assert!(conn.find_external_ports("", 2, 2).is_empty());
    }

    #[test]
    fn test_connect_disconnect() {
        let mut conn = DummyExternalConnections::new();
        conn.add_external_mock_port("ext", 0, 0);
        conn.connect(0x1234, "ext");
        assert_eq!(conn.connection_status_of(0x1234).len(), 1);
        assert!(conn.connection_status_of(0x1234)[0].connected);
        conn.disconnect(0x1234, "ext");
        assert!(conn.connection_status_of(0x1234).is_empty());
    }

    #[test]
    fn test_find_external_ports_regex() {
        let mut conn = DummyExternalConnections::new();
        conn.add_external_mock_port("port_a", 0, 0);
        conn.add_external_mock_port("port_b", 1, 0);
        let found = conn.find_external_ports("port_a", 2, 2);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "port_a");
    }

    #[test]
    fn test_connection_status_of_other_port() {
        let mut conn = DummyExternalConnections::new();
        conn.add_external_mock_port("ext", 0, 0);
        conn.connect(0x1234, "ext");
        let status = conn.connection_status_of(0x5678);
        assert_eq!(status.len(), 1);
        assert!(!status[0].connected);
    }
}
