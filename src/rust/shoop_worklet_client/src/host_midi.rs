use std::collections::{BTreeMap, VecDeque};

use anyhow::{anyhow, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostMidiDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostMidiEndpoint {
    pub id: String,
    pub name: String,
    pub direction: HostMidiDirection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostMidiInput {
    pub endpoint_id: String,
    pub data: Vec<u8>,
}

pub trait HostMidiBridge {
    fn revision(&self) -> u64;
    fn endpoints(&self) -> Vec<HostMidiEndpoint>;
    fn drain_track_messages(&mut self, max_messages: usize) -> Vec<HostMidiInput>;
    fn send(&mut self, endpoint_id: &str, message: &[u8]) -> Result<()>;
}

#[derive(Default)]
pub struct NullHostMidiBridge;

impl HostMidiBridge for NullHostMidiBridge {
    fn revision(&self) -> u64 {
        0
    }

    fn endpoints(&self) -> Vec<HostMidiEndpoint> {
        Vec::new()
    }

    fn drain_track_messages(&mut self, _max_messages: usize) -> Vec<HostMidiInput> {
        Vec::new()
    }

    fn send(&mut self, endpoint_id: &str, _message: &[u8]) -> Result<()> {
        Err(anyhow!("host MIDI endpoint is unavailable: {endpoint_id}"))
    }
}

#[derive(Default)]
pub struct InMemoryHostMidiBridge {
    revision: u64,
    endpoints: BTreeMap<String, HostMidiEndpoint>,
    incoming: VecDeque<HostMidiInput>,
    sent: Vec<(String, Vec<u8>)>,
}

impl InMemoryHostMidiBridge {
    pub fn replace_endpoints(&mut self, endpoints: Vec<HostMidiEndpoint>) {
        let replacement = endpoints
            .into_iter()
            .map(|endpoint| (endpoint.id.clone(), endpoint))
            .collect::<BTreeMap<_, _>>();
        if replacement != self.endpoints {
            self.endpoints = replacement;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub fn push_input(&mut self, input: HostMidiInput) -> Result<()> {
        let endpoint = self
            .endpoints
            .get(&input.endpoint_id)
            .ok_or_else(|| anyhow!("host MIDI input endpoint is unavailable"))?;
        if endpoint.direction != HostMidiDirection::Output {
            return Err(anyhow!("host MIDI endpoint is not an input source"));
        }
        self.incoming.push_back(input);
        Ok(())
    }

    pub fn sent(&self) -> &[(String, Vec<u8>)] {
        &self.sent
    }
}

impl HostMidiBridge for InMemoryHostMidiBridge {
    fn revision(&self) -> u64 {
        self.revision
    }

    fn endpoints(&self) -> Vec<HostMidiEndpoint> {
        self.endpoints.values().cloned().collect()
    }

    fn drain_track_messages(&mut self, max_messages: usize) -> Vec<HostMidiInput> {
        let count = max_messages.min(self.incoming.len());
        self.incoming.drain(..count).collect()
    }

    fn send(&mut self, endpoint_id: &str, message: &[u8]) -> Result<()> {
        let endpoint = self
            .endpoints
            .get(endpoint_id)
            .ok_or_else(|| anyhow!("host MIDI output endpoint is unavailable: {endpoint_id}"))?;
        if endpoint.direction != HostMidiDirection::Input {
            return Err(anyhow!("host MIDI endpoint is not an output sink"));
        }
        self.sent.push((endpoint_id.to_owned(), message.to_vec()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tracy_nextest_capture::tracy_capture_test]
    fn in_memory_bridge_preserves_endpoint_identity_direction_and_bounded_drains() {
        let mut null = NullHostMidiBridge;
        assert_eq!(null.revision(), 0);
        assert!(null.endpoints().is_empty());
        assert!(null.drain_track_messages(1).is_empty());
        assert!(null.send("missing", &[0x90]).is_err());

        let mut bridge = InMemoryHostMidiBridge::default();
        let endpoints = vec![
            HostMidiEndpoint {
                id: "source:a".to_owned(),
                name: "A".to_owned(),
                direction: HostMidiDirection::Output,
            },
            HostMidiEndpoint {
                id: "sink:b".to_owned(),
                name: "B".to_owned(),
                direction: HostMidiDirection::Input,
            },
        ];
        bridge.replace_endpoints(endpoints.clone());
        assert_eq!(bridge.revision(), 1);
        let published = bridge.endpoints();
        assert_eq!(published.len(), endpoints.len());
        assert!(endpoints
            .iter()
            .all(|endpoint| published.contains(endpoint)));
        bridge.replace_endpoints(published);
        assert_eq!(bridge.revision(), 1);
        assert!(bridge
            .push_input(HostMidiInput {
                endpoint_id: "missing".to_owned(),
                data: vec![0x90],
            })
            .is_err());
        assert!(bridge
            .push_input(HostMidiInput {
                endpoint_id: "sink:b".to_owned(),
                data: vec![0x90],
            })
            .is_err());
        bridge
            .push_input(HostMidiInput {
                endpoint_id: "source:a".to_owned(),
                data: vec![0x90, 60, 100],
            })
            .unwrap();
        assert!(bridge.drain_track_messages(0).is_empty());
        assert_eq!(bridge.drain_track_messages(1).len(), 1);
        assert!(bridge.send("missing", &[0x80]).is_err());
        assert!(bridge.send("source:a", &[0x80]).is_err());
        bridge.send("sink:b", &[0x80, 60, 0]).unwrap();
        assert_eq!(bridge.sent(), &[("sink:b".to_owned(), vec![0x80, 60, 0])]);
    }
}
