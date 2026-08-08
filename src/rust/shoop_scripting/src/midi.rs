use std::collections::{BTreeMap, VecDeque};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{self, Receiver, TryRecvError, TrySendError};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

use anyhow::anyhow;
#[cfg(not(target_arch = "wasm32"))]
use anyhow::Context;
#[cfg(not(target_arch = "wasm32"))]
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

pub const MIDI_QUEUE_CAPACITY: usize = 1024;
pub const MAX_MIDI_MESSAGE_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MidiConnectionId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MidiEndpointDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiEndpoint {
    pub id: String,
    pub name: String,
    pub direction: MidiEndpointDirection,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MidiEndpointSnapshot {
    pub revision: u64,
    pub endpoints: Vec<MidiEndpoint>,
}

pub trait MidiControlService {
    fn endpoints(&mut self) -> anyhow::Result<MidiEndpointSnapshot>;
    fn connect_input(&mut self, endpoint_id: &str) -> anyhow::Result<MidiConnectionId>;
    fn connect_output(&mut self, endpoint_id: &str) -> anyhow::Result<MidiConnectionId>;
    fn disconnect(&mut self, connection: MidiConnectionId);
    fn drain_input(
        &mut self,
        connection: MidiConnectionId,
        max_messages: usize,
    ) -> anyhow::Result<Vec<Vec<u8>>>;
    fn take_dropped_input(&mut self, connection: MidiConnectionId) -> u32;
    fn send(&mut self, connection: MidiConnectionId, message: &[u8]) -> anyhow::Result<()>;
}

#[derive(Default)]
pub struct NullMidiService;

impl MidiControlService for NullMidiService {
    fn endpoints(&mut self) -> anyhow::Result<MidiEndpointSnapshot> {
        Ok(MidiEndpointSnapshot::default())
    }

    fn connect_input(&mut self, _endpoint_id: &str) -> anyhow::Result<MidiConnectionId> {
        Err(anyhow!("MIDI input service is unavailable"))
    }

    fn connect_output(&mut self, _endpoint_id: &str) -> anyhow::Result<MidiConnectionId> {
        Err(anyhow!("MIDI output service is unavailable"))
    }

    fn disconnect(&mut self, _connection: MidiConnectionId) {}

    fn drain_input(
        &mut self,
        _connection: MidiConnectionId,
        _max_messages: usize,
    ) -> anyhow::Result<Vec<Vec<u8>>> {
        Ok(Vec::new())
    }

    fn take_dropped_input(&mut self, _connection: MidiConnectionId) -> u32 {
        0
    }

    fn send(&mut self, _connection: MidiConnectionId, _message: &[u8]) -> anyhow::Result<()> {
        Err(anyhow!("MIDI output service is unavailable"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
enum NativeConnection {
    Input {
        _connection: MidiInputConnection<()>,
        receiver: Receiver<Vec<u8>>,
        dropped: Arc<AtomicU32>,
    },
    Output(MidiOutputConnection),
}

#[cfg(not(target_arch = "wasm32"))]
pub struct NativeMidiService {
    next_connection: u64,
    connections: BTreeMap<MidiConnectionId, NativeConnection>,
    endpoint_revision: u64,
    last_endpoints: Vec<MidiEndpoint>,
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeMidiService {
    pub fn new() -> Self {
        Self {
            next_connection: 1,
            connections: BTreeMap::new(),
            endpoint_revision: 0,
            last_endpoints: Vec::new(),
        }
    }

    fn next_id(&mut self) -> MidiConnectionId {
        let id = MidiConnectionId(self.next_connection);
        self.next_connection = self.next_connection.saturating_add(1);
        id
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for NativeMidiService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl MidiControlService for NativeMidiService {
    fn endpoints(&mut self) -> anyhow::Result<MidiEndpointSnapshot> {
        let input = MidiInput::new("ShoopDaLoop control discovery")
            .context("could not discover MIDI inputs")?;
        let output = MidiOutput::new("ShoopDaLoop control discovery")
            .context("could not discover MIDI outputs")?;
        let mut endpoints = Vec::new();
        for (index, port) in input.ports().iter().enumerate() {
            let name = input
                .port_name(port)
                .with_context(|| format!("could not name MIDI input {index}"))?;
            endpoints.push(MidiEndpoint {
                id: format!("source:{index}:{name}"),
                name,
                direction: MidiEndpointDirection::Output,
            });
        }
        for (index, port) in output.ports().iter().enumerate() {
            let name = output
                .port_name(port)
                .with_context(|| format!("could not name MIDI output {index}"))?;
            endpoints.push(MidiEndpoint {
                id: format!("sink:{index}:{name}"),
                name,
                direction: MidiEndpointDirection::Input,
            });
        }
        endpoints.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then(left.direction_as_u8().cmp(&right.direction_as_u8()))
                .then(left.id.cmp(&right.id))
        });
        if endpoints != self.last_endpoints {
            self.endpoint_revision = self.endpoint_revision.wrapping_add(1);
            self.last_endpoints = endpoints.clone();
        }
        Ok(MidiEndpointSnapshot {
            revision: self.endpoint_revision,
            endpoints,
        })
    }

    fn connect_input(&mut self, endpoint_id: &str) -> anyhow::Result<MidiConnectionId> {
        let mut input = MidiInput::new("ShoopDaLoop control input")
            .context("could not create MIDI control input")?;
        input.ignore(midir::Ignore::None);
        let port = input
            .ports()
            .into_iter()
            .enumerate()
            .find(|(index, port)| {
                input
                    .port_name(port)
                    .map(|name| format!("source:{index}:{name}") == endpoint_id)
                    .unwrap_or(false)
            })
            .map(|(_, port)| port)
            .ok_or_else(|| anyhow!("MIDI input endpoint disappeared: {endpoint_id}"))?;
        let (sender, receiver) = mpsc::sync_channel(MIDI_QUEUE_CAPACITY);
        let dropped = Arc::new(AtomicU32::new(0));
        let callback_dropped = Arc::clone(&dropped);
        let connection = input
            .connect(
                &port,
                "ShoopDaLoop control input",
                move |_timestamp, message, _| {
                    if message.is_empty() || message.len() > MAX_MIDI_MESSAGE_BYTES {
                        callback_dropped.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                    match sender.try_send(message.to_vec()) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                            callback_dropped.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                },
                (),
            )
            .map_err(|error| anyhow!("could not connect MIDI input: {error}"))?;
        let id = self.next_id();
        self.connections.insert(
            id,
            NativeConnection::Input {
                _connection: connection,
                receiver,
                dropped,
            },
        );
        Ok(id)
    }

    fn connect_output(&mut self, endpoint_id: &str) -> anyhow::Result<MidiConnectionId> {
        let output = MidiOutput::new("ShoopDaLoop control output")
            .context("could not create MIDI control output")?;
        let port = output
            .ports()
            .into_iter()
            .enumerate()
            .find(|(index, port)| {
                output
                    .port_name(port)
                    .map(|name| format!("sink:{index}:{name}") == endpoint_id)
                    .unwrap_or(false)
            })
            .map(|(_, port)| port)
            .ok_or_else(|| anyhow!("MIDI output endpoint disappeared: {endpoint_id}"))?;
        let connection = output
            .connect(&port, "ShoopDaLoop control output")
            .map_err(|error| anyhow!("could not connect MIDI output: {error}"))?;
        let id = self.next_id();
        self.connections
            .insert(id, NativeConnection::Output(connection));
        Ok(id)
    }

    fn disconnect(&mut self, connection: MidiConnectionId) {
        self.connections.remove(&connection);
    }

    fn drain_input(
        &mut self,
        connection: MidiConnectionId,
        max_messages: usize,
    ) -> anyhow::Result<Vec<Vec<u8>>> {
        let NativeConnection::Input { receiver, .. } = self
            .connections
            .get_mut(&connection)
            .ok_or_else(|| anyhow!("unknown MIDI input connection"))?
        else {
            return Err(anyhow!("cannot receive from a MIDI output connection"));
        };
        let mut messages = Vec::new();
        while messages.len() < max_messages {
            match receiver.try_recv() {
                Ok(message) => messages.push(message),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    return Err(anyhow!("MIDI input connection closed"));
                }
            }
        }
        Ok(messages)
    }

    fn take_dropped_input(&mut self, connection: MidiConnectionId) -> u32 {
        match self.connections.get(&connection) {
            Some(NativeConnection::Input { dropped, .. }) => dropped.swap(0, Ordering::Relaxed),
            Some(NativeConnection::Output(_)) | None => 0,
        }
    }

    fn send(&mut self, connection: MidiConnectionId, message: &[u8]) -> anyhow::Result<()> {
        if message.is_empty() || message.len() > MAX_MIDI_MESSAGE_BYTES {
            return Err(anyhow!("invalid MIDI message length {}", message.len()));
        }
        let NativeConnection::Output(connection) = self
            .connections
            .get_mut(&connection)
            .ok_or_else(|| anyhow!("unknown MIDI output connection"))?
        else {
            return Err(anyhow!("cannot send through a MIDI input connection"));
        };
        connection
            .send(message)
            .map_err(|error| anyhow!("could not send MIDI message: {error}"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl MidiEndpoint {
    fn direction_as_u8(&self) -> u8 {
        match self.direction {
            MidiEndpointDirection::Input => 0,
            MidiEndpointDirection::Output => 1,
        }
    }
}

#[derive(Default)]
struct FakeMidiState {
    endpoint_revision: u64,
    endpoints: Vec<MidiEndpoint>,
    input_messages: BTreeMap<String, VecDeque<Vec<u8>>>,
    sent: Vec<(String, Vec<u8>)>,
    fail_connections: bool,
    fail_sends: bool,
    active_connections: u32,
}

#[derive(Clone)]
pub struct FakeMidiControl {
    state: std::rc::Rc<std::cell::RefCell<FakeMidiState>>,
}

impl FakeMidiControl {
    pub fn set_endpoints(&self, endpoints: Vec<MidiEndpoint>) {
        let mut state = self.state.borrow_mut();
        if state.endpoints != endpoints {
            state.endpoint_revision = state.endpoint_revision.wrapping_add(1);
            state.endpoints = endpoints;
        }
    }

    pub fn push_input(&self, endpoint_id: &str, message: Vec<u8>) {
        self.state
            .borrow_mut()
            .input_messages
            .entry(endpoint_id.to_owned())
            .or_default()
            .push_back(message);
    }

    pub fn take_sent(&self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut self.state.borrow_mut().sent)
    }

    pub fn set_fail_connections(&self, fail: bool) {
        self.state.borrow_mut().fail_connections = fail;
    }

    pub fn set_fail_sends(&self, fail: bool) {
        self.state.borrow_mut().fail_sends = fail;
    }

    pub fn active_connections(&self) -> u32 {
        self.state.borrow().active_connections
    }
}

pub struct FakeMidiService {
    state: std::rc::Rc<std::cell::RefCell<FakeMidiState>>,
    next_connection: u64,
    connections: BTreeMap<MidiConnectionId, String>,
}

impl FakeMidiService {
    pub fn new() -> (Self, FakeMidiControl) {
        let state = std::rc::Rc::new(std::cell::RefCell::new(FakeMidiState::default()));
        (
            Self {
                state: std::rc::Rc::clone(&state),
                next_connection: 1,
                connections: BTreeMap::new(),
            },
            FakeMidiControl { state },
        )
    }

    fn connect(&mut self, endpoint_id: &str) -> anyhow::Result<MidiConnectionId> {
        if self.state.borrow().fail_connections {
            return Err(anyhow!("injected MIDI connection failure"));
        }
        if !self
            .state
            .borrow()
            .endpoints
            .iter()
            .any(|endpoint| endpoint.id == endpoint_id)
        {
            return Err(anyhow!("unknown fake MIDI endpoint {endpoint_id}"));
        }
        let id = MidiConnectionId(self.next_connection);
        self.next_connection = self.next_connection.saturating_add(1);
        self.connections.insert(id, endpoint_id.to_owned());
        let mut state = self.state.borrow_mut();
        state.active_connections = state.active_connections.saturating_add(1);
        Ok(id)
    }
}

impl MidiControlService for FakeMidiService {
    fn endpoints(&mut self) -> anyhow::Result<MidiEndpointSnapshot> {
        let state = self.state.borrow();
        Ok(MidiEndpointSnapshot {
            revision: state.endpoint_revision,
            endpoints: state.endpoints.clone(),
        })
    }

    fn connect_input(&mut self, endpoint_id: &str) -> anyhow::Result<MidiConnectionId> {
        self.connect(endpoint_id)
    }

    fn connect_output(&mut self, endpoint_id: &str) -> anyhow::Result<MidiConnectionId> {
        self.connect(endpoint_id)
    }

    fn disconnect(&mut self, connection: MidiConnectionId) {
        if self.connections.remove(&connection).is_some() {
            let mut state = self.state.borrow_mut();
            state.active_connections = state.active_connections.saturating_sub(1);
        }
    }

    fn drain_input(
        &mut self,
        connection: MidiConnectionId,
        max_messages: usize,
    ) -> anyhow::Result<Vec<Vec<u8>>> {
        let endpoint = self
            .connections
            .get(&connection)
            .ok_or_else(|| anyhow!("unknown fake MIDI connection"))?
            .clone();
        let mut state = self.state.borrow_mut();
        let queue = state.input_messages.entry(endpoint).or_default();
        let count = max_messages.min(queue.len());
        Ok(queue.drain(..count).collect())
    }

    fn take_dropped_input(&mut self, _connection: MidiConnectionId) -> u32 {
        0
    }

    fn send(&mut self, connection: MidiConnectionId, message: &[u8]) -> anyhow::Result<()> {
        if self.state.borrow().fail_sends {
            return Err(anyhow!("injected MIDI send failure"));
        }
        let endpoint = self
            .connections
            .get(&connection)
            .ok_or_else(|| anyhow!("unknown fake MIDI connection"))?
            .clone();
        self.state
            .borrow_mut()
            .sent
            .push((endpoint, message.to_vec()));
        Ok(())
    }
}
