//! Test ports and a mock external-connection registry.
//!
//! The dummy driver lets tests drive the engine deterministically: input ports
//! are fed from a queue of sample blocks, and output ports retain what they
//! produced so it can be dequeued and asserted on.
//!
//! Port identity in the connection registry is an explicit [`PortId`] rather than
//! registry independent of port lifetimes.

use std::collections::{BTreeMap, VecDeque};

use regex::Regex;
use thiserror::Error;

use crate::port::{AudioPort, PortConnectability, PortDataType, PortDirection};

/// Identifies a port to the connection registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PortId(pub u64);

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DummyPortError {
    #[error("no external port named {0}")]
    NoSuchExternalPort(String),
    #[error("asked for {requested} retained samples but only {available} are held")]
    NotEnoughRetained { requested: usize, available: usize },
    #[error("invalid port name pattern: {0}")]
    BadPattern(String),
}

/// A mock port outside the application, for connection tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalPortDescriptor {
    pub name: String,
    pub direction: PortDirection,
    pub data_type: PortDataType,
}

/// Registry of mock external ports and who is connected to them.
#[derive(Clone, Debug, Default)]
pub struct DummyExternalConnections {
    mock_ports: Vec<ExternalPortDescriptor>,
    /// (port, external port name) pairs, in the order they were made.
    connections: Vec<(PortId, String)>,
}

impl DummyExternalConnections {
    /// Adds a mock port. Duplicate names are ignored.
    pub fn add_mock_port(
        &mut self,
        name: impl Into<String>,
        direction: PortDirection,
        data_type: PortDataType,
    ) {
        let name = name.into();
        if self.mock_ports.iter().any(|p| p.name == name) {
            return;
        }
        self.mock_ports.push(ExternalPortDescriptor {
            name,
            direction,
            data_type,
        });
    }

    /// Removes a mock port, along with any connections to it.
    pub fn remove_mock_port(&mut self, name: &str) {
        let before = self.mock_ports.len();
        self.mock_ports.retain(|p| p.name != name);
        if self.mock_ports.len() != before {
            self.connections.retain(|(_, n)| n != name);
        }
    }

    pub fn remove_all_mock_ports(&mut self) {
        self.mock_ports.clear();
        self.connections.clear();
    }

    pub fn mock_ports(&self) -> &[ExternalPortDescriptor] {
        &self.mock_ports
    }

    /// Number of (port, external port) pairs held.
    ///
    /// Distinct from `connection_status_of`, which keys by external name and so
    /// cannot reveal duplicates.
    pub fn n_connections(&self) -> usize {
        self.connections.len()
    }

    pub fn is_connected(&self, port: PortId, external: &str) -> bool {
        self.connections
            .iter()
            .any(|candidate| candidate.0 == port && candidate.1 == external)
    }

    /// All external port names currently connected to `port`.
    pub fn connections_for(&self, port: PortId) -> Vec<String> {
        self.connections
            .iter()
            .filter_map(|(owner, name)| (*owner == port).then(|| name.clone()))
            .collect()
    }

    /// Snapshot of all `(port, external port name)` pairs.
    pub fn connections(&self) -> Vec<(PortId, String)> {
        self.connections.clone()
    }

    fn find_port(&self, name: &str) -> Result<&ExternalPortDescriptor, DummyPortError> {
        self.mock_ports
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| DummyPortError::NoSuchExternalPort(name.to_string()))
    }

    /// Connects a port to a mock external port. Repeat connections are ignored.
    pub fn connect(&mut self, port: PortId, external: &str) -> Result<(), DummyPortError> {
        let name = self.find_port(external)?.name.clone();
        if !self.connections.iter().any(|c| c == &(port, name.clone())) {
            self.connections.push((port, name));
        }
        Ok(())
    }

    pub fn disconnect(&mut self, port: PortId, external: &str) -> Result<(), DummyPortError> {
        let name = self.find_port(external)?.name.clone();
        self.connections.retain(|c| c != &(port, name.clone()));
        Ok(())
    }

    /// Connection status as seen by `port`.
    ///
    /// Every externally connected name appears, including ones connected by other
    /// name and a later connection to the same name overwrites an earlier one --
    /// so if two ports share an external name, only the last one is reported as
    /// connected.
    pub fn connection_status_of(&self, port: PortId) -> BTreeMap<String, bool> {
        let mut out = BTreeMap::new();
        for (owner, name) in &self.connections {
            out.insert(name.clone(), *owner == port);
        }
        out
    }

    /// Mock ports matching an optional full-match name pattern and filters.
    ///
    /// used `std::regex_match`, which requires the whole name to match.
    pub fn find_external_ports(
        &self,
        name_pattern: Option<&str>,
        direction: PortDirection,
        data_type: PortDataType,
    ) -> Result<Vec<ExternalPortDescriptor>, DummyPortError> {
        let re = match name_pattern {
            Some(p) => Some(
                Regex::new(&format!("^(?:{p})$"))
                    .map_err(|_| DummyPortError::BadPattern(p.to_string()))?,
            ),
            None => None,
        };
        Ok(self
            .mock_ports
            .iter()
            .filter(|p| {
                re.as_ref().is_none_or(|re| re.is_match(&p.name))
                    && (direction == PortDirection::Any || direction == p.direction)
                    && (data_type == PortDataType::Any || data_type == p.data_type)
            })
            .cloned()
            .collect())
    }
}

/// Samples reserved for the input queue and retained output, so a cycle neither
/// allocates nor frees.
const QUEUE_RESERVE: usize = 4096;

/// Test audio port: fed from a queue when it is an input, retaining output when
/// it is an output.
#[derive(Debug)]
pub struct DummyAudioPort {
    id: PortId,
    name: String,
    direction: PortDirection,
    /// Samples to hand to the engine, oldest first.
    ///
    /// A flat sample queue rather than a queue of blocks: consuming a block would
    /// free it on the process thread, which is as much a realtime violation as
    /// allocating. Draining a `VecDeque` only moves its head.
    queued: VecDeque<f32>,
    /// How many further samples the test has asked to retain.
    n_requested: usize,
    retained: Vec<f32>,
    buffer: Vec<f32>,
    audio: AudioPort,
}

impl DummyAudioPort {
    pub fn new(
        id: PortId,
        name: impl Into<String>,
        direction: PortDirection,
        ringbuffer_buffer_size: usize,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            direction,
            queued: VecDeque::with_capacity(QUEUE_RESERVE),
            n_requested: 0,
            retained: Vec::with_capacity(QUEUE_RESERVE),
            buffer: Vec::new(),
            audio: AudioPort::new(ringbuffer_buffer_size),
        }
    }

    pub fn id(&self) -> PortId {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn direction(&self) -> PortDirection {
        self.direction
    }
    pub fn data_type(&self) -> PortDataType {
        PortDataType::Audio
    }
    pub fn audio(&self) -> &AudioPort {
        &self.audio
    }
    pub fn audio_mut(&mut self) -> &mut AudioPort {
        &mut self.audio
    }

    /// An input port supplies data to the engine, so the engine reads it; an
    /// output port is written to.
    pub fn has_internal_read_access(&self) -> bool {
        self.direction == PortDirection::Input
    }
    pub fn has_internal_write_access(&self) -> bool {
        self.direction == PortDirection::Output
    }
    pub fn has_implicit_input_source(&self) -> bool {
        self.direction == PortDirection::Input
    }
    pub fn has_implicit_output_sink(&self) -> bool {
        self.direction == PortDirection::Output
    }

    pub fn input_connectability(&self) -> PortConnectability {
        if self.direction == PortDirection::Input {
            PortConnectability::EXTERNAL
        } else {
            PortConnectability::INTERNAL
        }
    }
    pub fn output_connectability(&self) -> PortConnectability {
        if self.direction == PortDirection::Input {
            PortConnectability::INTERNAL
        } else {
            PortConnectability::EXTERNAL
        }
    }

    // --- test-facing controls ---

    /// Queues samples for the engine to read.
    ///
    /// A control-thread operation: it may grow the queue past its reservation.
    pub fn queue_data(&mut self, data: &[f32]) {
        self.queued.extend(data.iter().copied());
    }
    pub fn queue_empty(&self) -> bool {
        self.queued.is_empty()
    }

    /// Asks that the next `n` frames of output be retained for inspection.
    pub fn request_data(&mut self, n: usize) {
        self.n_requested += n;
    }
    pub fn n_requested(&self) -> usize {
        self.n_requested
    }
    pub fn n_retained(&self) -> usize {
        self.retained.len()
    }

    /// Takes `n` retained samples, oldest first.
    pub fn dequeue_data(&mut self, n: usize) -> Result<Vec<f32>, DummyPortError> {
        if n > self.retained.len() {
            return Err(DummyPortError::NotEnoughRetained {
                requested: n,
                available: self.retained.len(),
            });
        }
        Ok(self.retained.drain(..n).collect())
    }

    // --- processing ---

    fn ensure_buffer(&mut self, n_frames: usize) {
        let needed = self.buffer.len().max(n_frames).max(1);
        if needed > self.buffer.len() {
            crate::realtime_allow_alloc_once!("DummyAudioPort::ensure_buffer resize", || {
                self.buffer.resize(needed, 0.0)
            });
        }
    }

    pub fn buffer(&mut self, n_frames: usize) -> &mut [f32] {
        self.ensure_buffer(n_frames);
        &mut self.buffer[..n_frames]
    }

    /// Fills the buffer from the queue, zero-padding whatever is left over.
    ///
    /// More queued samples than the cycle needs stay for the next cycle.
    pub fn prepare(&mut self, n_frames: usize) {
        self.ensure_buffer(n_frames);
        let take = n_frames.min(self.queued.len());
        for (i, s) in self.queued.drain(..take).enumerate() {
            self.buffer[i] = s;
        }
        for s in self.buffer[take..n_frames].iter_mut() {
            *s = 0.0;
        }
    }

    /// Applies the audio path, then retains output the test asked for.
    pub fn process(&mut self, n_frames: usize) {
        self.ensure_buffer(n_frames);
        {
            let (buf, audio) = (&mut self.buffer[..n_frames], &mut self.audio);
            audio.process(buf);
        }
        let to_store = n_frames.min(self.n_requested);
        if to_store > 0 {
            self.retained.extend_from_slice(&self.buffer[..to_store]);
            self.n_requested -= to_store;
        }
    }

    pub fn close(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    use PortDataType as T;
    use PortDirection as D;

    fn input_port() -> DummyAudioPort {
        DummyAudioPort::new(PortId(1), "in", D::Input, 4)
    }
    fn output_port() -> DummyAudioPort {
        DummyAudioPort::new(PortId(2), "out", D::Output, 4)
    }

    // --- external connection registry ---

    #[shoop_wasm_test_support::shoop_test]
    fn mock_ports_are_added_once() {
        let mut c = DummyExternalConnections::default();
        c.add_mock_port("a", D::Input, T::Audio);
        c.add_mock_port("a", D::Output, T::Midi);
        check!(c.mock_ports().len() == 1);
        check!(c.mock_ports()[0].direction == D::Input);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn connecting_requires_an_existing_mock_port() {
        let mut c = DummyExternalConnections::default();
        let r = c.connect(PortId(1), "nope");
        check!(r == Err(DummyPortError::NoSuchExternalPort("nope".into())));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn connection_status_reports_own_and_others_connections() {
        let mut c = DummyExternalConnections::default();
        c.add_mock_port("a", D::Input, T::Audio);
        c.add_mock_port("b", D::Input, T::Audio);
        assert2::assert!(let Ok(()) = c.connect(PortId(1), "a"));
        assert2::assert!(let Ok(()) = c.connect(PortId(2), "b"));

        let s = c.connection_status_of(PortId(1));
        check!(s.get("a") == Some(&true));
        // b is connected, but by someone else.
        check!(s.get("b") == Some(&false));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn repeat_connections_are_ignored() {
        let mut c = DummyExternalConnections::default();
        c.add_mock_port("a", D::Input, T::Audio);
        assert2::assert!(let Ok(()) = c.connect(PortId(1), "a"));
        assert2::assert!(let Ok(()) = c.connect(PortId(1), "a"));
        // The pair is stored once. Checking the status map alone would not show
        // this, since it keys by external name.
        check!(c.n_connections() == 1);
        check!(c.connection_status_of(PortId(1)).len() == 1);

        // A second port connecting to the same external port is a new pair.
        assert2::assert!(let Ok(()) = c.connect(PortId(2), "a"));
        check!(c.n_connections() == 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn disconnecting_removes_the_connection() {
        let mut c = DummyExternalConnections::default();
        c.add_mock_port("a", D::Input, T::Audio);
        assert2::assert!(let Ok(()) = c.connect(PortId(1), "a"));
        assert2::assert!(let Ok(()) = c.disconnect(PortId(1), "a"));
        check!(c.connection_status_of(PortId(1)).is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn removing_a_mock_port_drops_its_connections() {
        let mut c = DummyExternalConnections::default();
        c.add_mock_port("a", D::Input, T::Audio);
        assert2::assert!(let Ok(()) = c.connect(PortId(1), "a"));
        c.remove_mock_port("a");
        check!(c.mock_ports().is_empty());
        check!(c.connection_status_of(PortId(1)).is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn removing_all_mock_ports_clears_everything() {
        let mut c = DummyExternalConnections::default();
        c.add_mock_port("a", D::Input, T::Audio);
        assert2::assert!(let Ok(()) = c.connect(PortId(1), "a"));
        c.remove_all_mock_ports();
        check!(c.mock_ports().is_empty());
        check!(c.connection_status_of(PortId(1)).is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn find_filters_by_direction_and_data_type() {
        let mut c = DummyExternalConnections::default();
        c.add_mock_port("ai", D::Input, T::Audio);
        c.add_mock_port("ao", D::Output, T::Audio);
        c.add_mock_port("mi", D::Input, T::Midi);

        assert2::assert!(let Ok(all) = c.find_external_ports(None, D::Any, T::Any));
        check!(all.len() == 3);
        assert2::assert!(let Ok(ins) = c.find_external_ports(None, D::Input, T::Any));
        check!(ins.len() == 2);
        assert2::assert!(let Ok(audio_in) = c.find_external_ports(None, D::Input, T::Audio));
        check!(audio_in.len() == 1);
        check!(audio_in[0].name == "ai");
    }

    #[shoop_wasm_test_support::shoop_test]
    fn find_requires_a_full_name_match() {
        let mut c = DummyExternalConnections::default();
        c.add_mock_port("capture_1", D::Input, T::Audio);
        c.add_mock_port("system:capture_1", D::Input, T::Audio);

        // A partial pattern must not match, mirroring std::regex_match.
        assert2::assert!(let Ok(r) = c.find_external_ports(Some("capture"), D::Any, T::Any));
        check!(r.is_empty());
        // The full name does.
        assert2::assert!(let Ok(r) = c.find_external_ports(Some("capture_1"), D::Any, T::Any));
        check!(r.len() == 1);
        // And a wildcard reaches both.
        assert2::assert!(let Ok(r) = c.find_external_ports(Some(".*capture_1"), D::Any, T::Any));
        check!(r.len() == 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_bad_pattern_is_reported() {
        let c = DummyExternalConnections::default();
        let r = c.find_external_ports(Some("("), D::Any, T::Any);
        check!(r == Err(DummyPortError::BadPattern("(".into())));
    }

    // --- dummy audio port ---

    #[shoop_wasm_test_support::shoop_test]
    fn reports_its_identity_and_role() {
        let p = input_port();
        check!(p.id() == PortId(1));
        check!(p.name() == "in");
        check!(p.direction() == D::Input);
        check!(p.data_type() == T::Audio);
        check!(p.has_internal_read_access());
        check!(!p.has_internal_write_access());
        check!(p.has_implicit_input_source());
        check!(!p.has_implicit_output_sink());
        check!(p.input_connectability() == PortConnectability::EXTERNAL);
        check!(p.output_connectability() == PortConnectability::INTERNAL);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn an_output_ports_roles_are_mirrored() {
        let p = output_port();
        check!(!p.has_internal_read_access());
        check!(p.has_internal_write_access());
        check!(!p.has_implicit_input_source());
        check!(p.has_implicit_output_sink());
        check!(p.input_connectability() == PortConnectability::INTERNAL);
        check!(p.output_connectability() == PortConnectability::EXTERNAL);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn queued_data_appears_in_the_buffer() {
        let mut p = input_port();
        check!(p.queue_empty());
        p.queue_data(&[1.0, 2.0, 3.0, 4.0]);
        check!(!p.queue_empty());
        p.prepare(4);
        check!(p.buffer(4) == [1.0, 2.0, 3.0, 4.0]);
        check!(p.queue_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_short_queue_is_zero_padded() {
        let mut p = input_port();
        p.queue_data(&[1.0, 2.0]);
        p.prepare(4);
        check!(p.buffer(4) == [1.0, 2.0, 0.0, 0.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn an_empty_queue_yields_silence() {
        let mut p = input_port();
        p.prepare(4);
        check!(p.buffer(4) == [0.0, 0.0, 0.0, 0.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_long_queued_block_spans_cycles() {
        let mut p = input_port();
        p.queue_data(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        p.prepare(4);
        check!(p.buffer(4) == [1.0, 2.0, 3.0, 4.0]);
        // The remainder is still queued for the next cycle.
        check!(!p.queue_empty());
        p.prepare(4);
        check!(p.buffer(4) == [5.0, 6.0, 0.0, 0.0]);
        check!(p.queue_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn several_queued_blocks_fill_one_cycle() {
        let mut p = input_port();
        p.queue_data(&[1.0, 2.0]);
        p.queue_data(&[3.0, 4.0]);
        p.prepare(4);
        check!(p.buffer(4) == [1.0, 2.0, 3.0, 4.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn nothing_is_retained_unless_requested() {
        let mut p = output_port();
        p.prepare(4);
        p.buffer(4).copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        p.process(4);
        check!(p.n_retained() == 0);
        check!(
            p.dequeue_data(1)
                == Err(DummyPortError::NotEnoughRetained {
                    requested: 1,
                    available: 0
                })
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn requested_output_is_retained_and_dequeued() {
        let mut p = output_port();
        p.request_data(4);
        p.prepare(4);
        p.buffer(4).copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        p.process(4);
        check!(p.n_retained() == 4);
        assert2::assert!(let Ok(d) = p.dequeue_data(2));
        check!(d == vec![1.0, 2.0]);
        // Dequeueing consumes, oldest first.
        check!(p.n_retained() == 2);
        assert2::assert!(let Ok(d) = p.dequeue_data(2));
        check!(d == vec![3.0, 4.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_request_larger_than_one_cycle_spans_cycles() {
        let mut p = output_port();
        p.request_data(6);
        p.prepare(4);
        p.buffer(4).copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        p.process(4);
        check!(p.n_requested() == 2);
        p.prepare(4);
        p.buffer(4).copy_from_slice(&[5.0, 6.0, 7.0, 8.0]);
        p.process(4);
        // Only the two still-requested samples were kept.
        check!(p.n_retained() == 6);
        assert2::assert!(let Ok(d) = p.dequeue_data(6));
        check!(d == vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn retained_output_reflects_gain() {
        let mut p = output_port();
        p.audio_mut().set_gain(2.0);
        p.request_data(2);
        p.prepare(2);
        p.buffer(2).copy_from_slice(&[1.0, 2.0]);
        p.process(2);
        assert2::assert!(let Ok(d) = p.dequeue_data(2));
        check!(d == vec![2.0, 4.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn an_input_port_round_trips_queued_data_through_processing() {
        let mut p = input_port();
        p.request_data(4);
        p.queue_data(&[0.5, -0.5, 0.25, 0.0]);
        p.prepare(4);
        p.process(4);
        assert2::assert!(let Ok(d) = p.dequeue_data(4));
        check!(d == vec![0.5, -0.5, 0.25, 0.0]);
        check!(p.audio().input_peak() == 0.5);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn close_is_harmless() {
        let mut p = input_port();
        p.close();
        check!(p.name() == "in");
    }
}
