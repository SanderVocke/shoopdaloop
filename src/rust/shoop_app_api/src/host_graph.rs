//! Physical host graph, separate from logical application routing.
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;

use crate::{PortDataType, PortDirection};

pub const MAX_HOST_GRAPH_PORTS: usize = 4096;
pub const MAX_HOST_GRAPH_LINKS: usize = 8192;

pub fn host_graph_name_valid(name: &str) -> bool {
    !name.is_empty() && name.len() <= 256 && !name.contains('\0')
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HostGraphPort {
    pub name: String,
    pub data_type: PortDataType,
    pub direction: PortDirection,
    pub owned: bool,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HostGraphLink {
    pub source: String,
    pub destination: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HostGraphState {
    /// Changes whenever the driver is restarted.
    pub epoch: u64,
    pub revision: u64,
    pub available: bool,
    pub ports: Vec<HostGraphPort>,
    /// Observed links touching this host's bank, never optimistic intent.
    pub links: Vec<HostGraphLink>,
    pub error: Option<String>,
}
impl HostGraphState {
    pub fn valid(&self) -> bool {
        if self.ports.len() > MAX_HOST_GRAPH_PORTS
            || self.links.len() > MAX_HOST_GRAPH_LINKS
            || self.error.as_ref().is_some_and(|error| error.len() > 512)
            || (self.available && (self.epoch == 0 || self.revision == 0))
            || (!self.available && (!self.ports.is_empty() || !self.links.is_empty()))
        {
            return false;
        }
        let mut ports = BTreeMap::new();
        for port in &self.ports {
            if !host_graph_name_valid(&port.name) || ports.insert(&port.name, port).is_some() {
                return false;
            }
        }
        let mut links = BTreeSet::new();
        self.links.iter().all(|link| {
            let (Some(source), Some(destination)) =
                (ports.get(&link.source), ports.get(&link.destination))
            else {
                return false;
            };
            source.direction == PortDirection::Output
                && destination.direction == PortDirection::Input
                && source.data_type == destination.data_type
                && (source.owned || destination.owned)
                && links.insert(link)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HostGraphRequest {
    pub epoch: u64,
    pub revision: u64,
    pub local: String,
    pub peer: String,
    pub connected: bool,
}
impl HostGraphRequest {
    pub fn valid(&self) -> bool {
        self.epoch != 0
            && self.revision != 0
            && host_graph_name_valid(&self.local)
            && host_graph_name_valid(&self.peer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use shoop_wasm_test_support::shoop_test;
    #[shoop_test]
    fn graph_validation_rejects_dangling_duplicate_and_incompatible_links() {
        let mut graph = HostGraphState {
            epoch: 1,
            revision: 1,
            available: true,
            ports: vec![
                HostGraphPort {
                    name: "host:out".into(),
                    data_type: PortDataType::Audio,
                    direction: PortDirection::Output,
                    owned: true,
                },
                HostGraphPort {
                    name: "peer:in".into(),
                    data_type: PortDataType::Audio,
                    direction: PortDirection::Input,
                    owned: false,
                },
            ],
            links: vec![HostGraphLink {
                source: "host:out".into(),
                destination: "peer:in".into(),
            }],
            error: None,
        };
        assert!(graph.valid());
        graph.links.push(graph.links[0].clone());
        assert!(!graph.valid());
        graph.links.pop();
        graph.ports[1].data_type = PortDataType::Midi;
        assert!(!graph.valid());
        graph.ports[1].data_type = PortDataType::Audio;
        graph.ports[0].owned = false;
        assert!(!graph.valid());
        graph.ports[0].owned = true;
        graph.ports.pop();
        assert!(!graph.valid());
        graph.ports.clear();
        graph.links.clear();
        graph.available = false;
        assert!(graph.valid());
        let mut request = HostGraphRequest {
            epoch: 1,
            revision: 1,
            local: "host:out".into(),
            peer: "peer:in".into(),
            connected: true,
        };
        assert!(request.valid());
        request.peer.push('\0');
        assert!(!request.valid());
        request.peer = "a".repeat(257);
        assert!(!request.valid());
        request.peer = "peer:in".into();
        request.epoch = 0;
        assert!(!request.valid());
    }
}
