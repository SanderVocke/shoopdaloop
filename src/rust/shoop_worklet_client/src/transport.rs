use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use shoop_audio_protocol::{
    Command, CommandEnvelope, Event, EventEnvelope, COMMAND_CAPACITY, PROTOCOL_VERSION,
    SESSION_TRANSFER_CHUNK_BYTES,
};
use shoop_backend::BackendDriverState;

const DURABLE_COMMAND_CAPACITY: usize = COMMAND_CAPACITY - 1;

pub trait MessageEndpoint {
    fn post_message(&self, message: &str) -> Result<()>;
    fn close(&self) {}
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConnectionState {
    #[default]
    Detached,
    Attached,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProtocolState {
    #[default]
    Detached,
    Initializing,
    Negotiated,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReplayState {
    #[default]
    NotStarted,
    Replaying,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RemoteEngineState {
    #[default]
    Unknown,
    Observed,
    Failed,
    Stopped,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RemoteReadiness {
    pub driver_state: BackendDriverState,
    pub connection: ConnectionState,
    pub protocol: ProtocolState,
    pub replay: ReplayState,
    pub engine: RemoteEngineState,
}

impl RemoteReadiness {
    pub fn is_ready(self) -> bool {
        matches!(
            self.driver_state,
            BackendDriverState::Running | BackendDriverState::Dummy
        ) && self.connection == ConnectionState::Attached
            && self.protocol == ProtocolState::Negotiated
            && self.replay == ReplayState::Complete
            && self.engine == RemoteEngineState::Observed
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransportDiagnostics {
    pub generation: u64,
    pub pending_commands: usize,
    pub queued_events: usize,
    pub overflows: u32,
    pub stale_messages: u32,
    pub duplicate_or_unknown_responses: u32,
    pub out_of_order_responses: u32,
}

#[derive(Clone, Debug)]
pub(crate) enum JournalMutation {
    Appended,
    Replaced(Command),
}

struct PendingCommand {
    command: Command,
    replay: bool,
    journal_mutation: Option<JournalMutation>,
}

fn is_global_journal_command(command: &Command) -> bool {
    matches!(
        command,
        Command::ConfigureMidiEndpoints { .. } | Command::SetLoopSmoothingMs { .. }
    )
}

fn is_session_replay_command(command: &Command) -> bool {
    matches!(
        command,
        Command::BeginSessionReplace { .. }
            | Command::WriteSessionReplace { .. }
            | Command::CommitSessionReplace { .. }
    )
}

#[derive(Clone)]
struct DurableSessionReplay {
    generation: u64,
    bytes: Arc<[u8]>,
}

struct ActiveSessionReplay {
    generation: u64,
    next_offset: usize,
    begin_sent: bool,
    commit_sent: bool,
}

pub(crate) struct ReceivedEvent {
    pub envelope: EventEnvelope,
    pub command: Command,
    pub replay: bool,
    pub journal_mutation: Option<JournalMutation>,
    pub generation: u64,
}

pub(crate) struct TransportCore {
    generation: u64,
    readiness: RemoteReadiness,
    error: Option<String>,
    endpoint: Option<Box<dyn MessageEndpoint>>,
    journal: Vec<Command>,
    session_replay: Option<DurableSessionReplay>,
    reserved_session_replay: Option<DurableSessionReplay>,
    active_session_replay: Option<ActiveSessionReplay>,
    deferred_journal_replay: VecDeque<Command>,
    inbound: VecDeque<ReceivedEvent>,
    next_sequence: u64,
    next_response_sequence: u64,
    pending: BTreeMap<u64, PendingCommand>,
    replay_sequences: BTreeSet<u64>,
    overflows: u32,
    stale_messages: u32,
    duplicate_or_unknown_responses: u32,
    out_of_order_responses: u32,
}

impl Default for TransportCore {
    fn default() -> Self {
        Self {
            generation: 0,
            readiness: RemoteReadiness::default(),
            error: None,
            endpoint: None,
            journal: Vec::new(),
            session_replay: None,
            reserved_session_replay: None,
            active_session_replay: None,
            deferred_journal_replay: VecDeque::new(),
            inbound: VecDeque::new(),
            next_sequence: 1,
            next_response_sequence: 1,
            pending: BTreeMap::new(),
            replay_sequences: BTreeSet::new(),
            overflows: 0,
            stale_messages: 0,
            duplicate_or_unknown_responses: 0,
            out_of_order_responses: 0,
        }
    }
}

impl TransportCore {
    pub(crate) fn journal(&mut self, command: Command) -> Result<()> {
        if self.reserved_session_replay.is_some() && !is_global_journal_command(&command) {
            return Err(anyhow!(
                "session mutation is unavailable during session replacement"
            ));
        }
        let existing_index = self
            .journal
            .iter()
            .rposition(|existing| command.supersedes_in_journal(existing));
        let previous = if let Some(index) = existing_index {
            Some(std::mem::replace(&mut self.journal[index], command.clone()))
        } else {
            let retained = self
                .journal
                .iter()
                .filter(|command| is_global_journal_command(command))
                .count();
            if self.journal.len() >= DURABLE_COMMAND_CAPACITY
                || retained.saturating_add(1) > DURABLE_COMMAND_CAPACITY
            {
                self.overflows = self.overflows.saturating_add(1);
                return Err(anyhow!("remote worklet command journal is full"));
            }
            self.journal.push(command.clone());
            None
        };
        let journal_mutation = Some(match previous.as_ref() {
            Some(previous) => JournalMutation::Replaced(previous.clone()),
            None => JournalMutation::Appended,
        });
        if self.endpoint.is_some() {
            if let Err(error) = self.send(command.clone(), false, journal_mutation) {
                if let (Some(index), Some(previous)) = (existing_index, previous) {
                    self.journal[index] = previous;
                } else {
                    debug_assert_eq!(self.journal.last(), Some(&command));
                    self.journal.pop();
                }
                return Err(error);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn reject_journaled(&mut self, command: &Command) {
        self.journal.retain(|candidate| candidate != command);
    }

    pub(crate) fn restore_rejected_journal(
        &mut self,
        command: &Command,
        mutation: Option<JournalMutation>,
    ) {
        let fallback = mutation.clone();
        for pending in self.pending.values_mut() {
            if pending
                .journal_mutation
                .as_ref()
                .is_some_and(|candidate| {
                    matches!(candidate, JournalMutation::Replaced(previous) if previous == command)
                })
            {
                pending.journal_mutation = fallback.clone();
            }
        }
        let Some(index) = self
            .journal
            .iter()
            .rposition(|candidate| candidate == command)
        else {
            return;
        };
        match mutation {
            Some(JournalMutation::Replaced(previous)) => self.journal[index] = previous,
            Some(JournalMutation::Appended) | None => {
                self.journal.remove(index);
            }
        }
    }

    pub(crate) fn reserve_session_replay(
        &mut self,
        generation: u64,
        bytes: Arc<[u8]>,
    ) -> Result<()> {
        if self.reserved_session_replay.is_some() {
            return Err(anyhow!("session replay is already reserved"));
        }
        self.reserved_session_replay = Some(DurableSessionReplay { generation, bytes });
        Ok(())
    }

    pub(crate) fn commit_reserved_session_replay(&mut self) {
        let Some(replay) = self.reserved_session_replay.take() else {
            return;
        };
        self.journal.retain(is_global_journal_command);
        self.session_replay = Some(replay);
    }

    pub(crate) fn cancel_reserved_session_replay(&mut self) {
        self.reserved_session_replay = None;
    }

    pub(crate) fn ephemeral(&mut self, command: Command) -> Result<()> {
        if self.endpoint.is_none() {
            return Err(anyhow!("remote worklet is not connected"));
        }
        self.send(command, false, None).map(|_| ())
    }

    fn send(
        &mut self,
        command: Command,
        replay: bool,
        journal_mutation: Option<JournalMutation>,
    ) -> Result<u64> {
        if self.pending.len() >= COMMAND_CAPACITY {
            self.overflows = self.overflows.saturating_add(1);
            return Err(anyhow!("remote worklet command queue is full"));
        }
        let sequence = self.next_sequence;
        let envelope = CommandEnvelope::new(sequence, command.clone());
        let json = serde_json::to_string(&envelope)?;
        self.endpoint
            .as_ref()
            .ok_or_else(|| anyhow!("remote worklet is unavailable"))?
            .post_message(&json)
            .map_err(|error| anyhow!("could not post remote worklet command: {error}"))?;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.pending.insert(
            sequence,
            PendingCommand {
                command,
                replay,
                journal_mutation,
            },
        );
        if replay {
            self.replay_sequences.insert(sequence);
        }
        Ok(sequence)
    }

    fn pump_session_replay(&mut self) -> Result<()> {
        while self.pending.len() < COMMAND_CAPACITY {
            let Some(mut active) = self.active_session_replay.take() else {
                break;
            };
            if active.commit_sent {
                self.active_session_replay = Some(active);
                break;
            }
            let replay = self
                .session_replay
                .as_ref()
                .expect("active session replay has durable bytes");
            let command = if !active.begin_sent {
                Command::BeginSessionReplace {
                    generation: active.generation,
                    total_bytes: replay.bytes.len(),
                }
            } else if active.next_offset < replay.bytes.len() {
                let end = active
                    .next_offset
                    .saturating_add(SESSION_TRANSFER_CHUNK_BYTES)
                    .min(replay.bytes.len());
                Command::WriteSessionReplace {
                    generation: active.generation,
                    offset: active.next_offset,
                    bytes: replay.bytes[active.next_offset..end].to_vec(),
                }
            } else {
                Command::CommitSessionReplace {
                    generation: active.generation,
                }
            };
            if let Err(error) = self.send(command.clone(), true, None) {
                self.active_session_replay = Some(active);
                return Err(error);
            }
            match command {
                Command::BeginSessionReplace { .. } => active.begin_sent = true,
                Command::WriteSessionReplace { bytes, .. } => {
                    active.next_offset = active.next_offset.saturating_add(bytes.len());
                }
                Command::CommitSessionReplace { .. } => active.commit_sent = true,
                _ => unreachable!("session replay emitted an unrelated command"),
            }
            self.active_session_replay = Some(active);
        }
        Ok(())
    }

    fn pump_deferred_journal_replay(&mut self) -> Result<()> {
        if self.active_session_replay.is_some() {
            return Ok(());
        }
        while self.pending.len() < COMMAND_CAPACITY {
            let Some(command) = self.deferred_journal_replay.pop_front() else {
                break;
            };
            if let Err(error) = self.send(command.clone(), true, None) {
                self.deferred_journal_replay.push_front(command);
                return Err(error);
            }
        }
        Ok(())
    }

    fn attach(
        &mut self,
        endpoint: Box<dyn MessageEndpoint>,
        generation: u64,
        input_channels: u32,
        output_channels: u32,
    ) -> Result<()> {
        if let Some(previous) = self.endpoint.take() {
            previous.close();
        }
        self.generation = generation;
        self.endpoint = Some(endpoint);
        self.inbound.clear();
        self.pending.clear();
        self.replay_sequences.clear();
        self.deferred_journal_replay.clear();
        self.active_session_replay =
            self.session_replay
                .as_ref()
                .map(|replay| ActiveSessionReplay {
                    generation: replay.generation,
                    next_offset: 0,
                    begin_sent: false,
                    commit_sent: false,
                });
        self.next_sequence = 1;
        self.next_response_sequence = 1;
        self.error = None;
        self.readiness.connection = ConnectionState::Attached;
        self.readiness.protocol = ProtocolState::Initializing;
        self.readiness.replay = ReplayState::Replaying;
        self.readiness.engine = RemoteEngineState::Unknown;
        self.send(
            Command::ConfigureDeviceChannels {
                input_channels,
                output_channels,
            },
            true,
            None,
        )?;
        let journal = self.journal.clone();
        for command in journal
            .iter()
            .filter(|command| matches!(command, Command::ConfigureMidiEndpoints { .. }))
            .cloned()
        {
            self.send(command, true, None)?;
        }
        for command in journal
            .iter()
            .filter(|command| {
                is_global_journal_command(command)
                    && !matches!(command, Command::ConfigureMidiEndpoints { .. })
            })
            .cloned()
        {
            self.send(command, true, None)?;
        }
        self.deferred_journal_replay.extend(
            journal
                .into_iter()
                .filter(|command| !is_global_journal_command(command)),
        );
        self.pump_session_replay()?;
        self.pump_deferred_journal_replay()?;
        Ok(())
    }

    fn receive(&mut self, generation: u64, json: &str) -> Result<()> {
        if generation != self.generation {
            self.stale_messages = self.stale_messages.saturating_add(1);
            return Err(anyhow!(
                "stale remote worklet generation {generation}; active generation is {}",
                self.generation
            ));
        }
        let event = match serde_json::from_str::<EventEnvelope>(json) {
            Ok(event) => event,
            Err(error) => {
                let message = format!("malformed remote worklet event: {error}");
                self.fail(message.clone());
                return Err(anyhow!(message));
            }
        };
        if event.version != PROTOCOL_VERSION {
            let message = format!(
                "remote worklet protocol version {} does not match {PROTOCOL_VERSION}",
                event.version
            );
            self.fail(message.clone());
            return Err(anyhow!(message));
        }
        if event.sequence == 0 {
            let Event::Error { message } = event.event else {
                let message =
                    "uncorrelated remote worklet event is not a terminal error".to_owned();
                self.fail(message.clone());
                return Err(anyhow!(message));
            };
            let message = format!("remote engine failure: {message}");
            self.fail(message.clone());
            return Err(anyhow!(message));
        }
        if event.sequence != self.next_response_sequence {
            if !self.pending.contains_key(&event.sequence) {
                self.duplicate_or_unknown_responses =
                    self.duplicate_or_unknown_responses.saturating_add(1);
            } else {
                self.out_of_order_responses = self.out_of_order_responses.saturating_add(1);
            }
            let message = format!(
                "unexpected remote worklet response sequence {}; expected {}",
                event.sequence, self.next_response_sequence
            );
            self.fail(message.clone());
            return Err(anyhow!(message));
        }
        let Some(pending) = self.pending.remove(&event.sequence) else {
            self.duplicate_or_unknown_responses =
                self.duplicate_or_unknown_responses.saturating_add(1);
            let message = format!(
                "unknown remote worklet response sequence {}",
                event.sequence
            );
            self.fail(message.clone());
            return Err(anyhow!(message));
        };
        self.next_response_sequence = self.next_response_sequence.saturating_add(1);
        if pending.replay {
            self.replay_sequences.remove(&event.sequence);
            if matches!(&pending.command, Command::CommitSessionReplace { .. }) {
                self.active_session_replay = None;
            }
        }
        if self.readiness.protocol == ProtocolState::Initializing && event.sequence == 1 {
            self.readiness.protocol = ProtocolState::Negotiated;
        }
        if !matches!(&event.event, Event::Error { .. }) {
            self.pump_session_replay()?;
            self.pump_deferred_journal_replay()?;
        }
        if self.replay_sequences.is_empty()
            && self.active_session_replay.is_none()
            && self.deferred_journal_replay.is_empty()
        {
            self.readiness.replay = ReplayState::Complete;
        }
        match &event.event {
            Event::Snapshot(_) => self.readiness.engine = RemoteEngineState::Observed,
            Event::Stopped => self.readiness.engine = RemoteEngineState::Stopped,
            _ => {}
        }
        if pending.replay
            && is_session_replay_command(&pending.command)
            && matches!(
                &event.event,
                Event::Ack | Event::SessionReplaceComplete { .. }
            )
        {
            return Ok(());
        }
        if self.inbound.len() >= COMMAND_CAPACITY {
            self.overflows = self.overflows.saturating_add(1);
            let message = "remote worklet event queue is full".to_owned();
            self.fail(message.clone());
            return Err(anyhow!(message));
        }
        self.inbound.push_back(ReceivedEvent {
            envelope: event,
            command: pending.command,
            replay: pending.replay,
            journal_mutation: pending.journal_mutation,
            generation,
        });
        Ok(())
    }

    fn detach(&mut self, send_shutdown: bool) {
        if send_shutdown && self.endpoint.is_some() && self.pending.len() < COMMAND_CAPACITY {
            let _ = self.send(Command::Shutdown, false, None);
        }
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close();
        }
        self.pending.clear();
        self.replay_sequences.clear();
        self.active_session_replay = None;
        self.deferred_journal_replay.clear();
        self.inbound.clear();
        self.readiness.connection = ConnectionState::Detached;
        self.readiness.protocol = ProtocolState::Detached;
        self.readiness.replay = ReplayState::NotStarted;
        self.readiness.engine = RemoteEngineState::Unknown;
    }

    pub(crate) fn fail(&mut self, message: String) {
        if self.error.is_none() {
            self.error = Some(message);
        }
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close();
        }
        self.pending.clear();
        self.replay_sequences.clear();
        self.active_session_replay = None;
        self.deferred_journal_replay.clear();
        self.readiness.driver_state = BackendDriverState::Failed;
        self.readiness.connection = ConnectionState::Failed;
        self.readiness.protocol = ProtocolState::Failed;
        self.readiness.replay = ReplayState::Failed;
        self.readiness.engine = RemoteEngineState::Failed;
    }

    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }

    #[cfg(test)]
    pub(crate) fn journal_commands(&self) -> Vec<Command> {
        self.journal.clone()
    }

    #[cfg(test)]
    pub(crate) fn has_reserved_session_replay(&self) -> bool {
        self.reserved_session_replay.is_some()
    }

    #[cfg(test)]
    pub(crate) fn session_replay(&self) -> Option<(u64, Arc<[u8]>)> {
        self.session_replay
            .as_ref()
            .map(|replay| (replay.generation, Arc::clone(&replay.bytes)))
    }

    pub(crate) fn readiness(&self) -> RemoteReadiness {
        self.readiness
    }

    pub(crate) fn driver_state(&self) -> BackendDriverState {
        if matches!(
            self.readiness.driver_state,
            BackendDriverState::Running | BackendDriverState::Dummy
        ) && !self.readiness.is_ready()
        {
            BackendDriverState::Starting
        } else {
            self.readiness.driver_state
        }
    }

    pub(crate) fn is_quiescent(&self) -> bool {
        self.pending.is_empty()
            && self.inbound.is_empty()
            && self.replay_sequences.is_empty()
            && self.deferred_journal_replay.is_empty()
            && self.readiness.replay != ReplayState::Replaying
    }

    pub(crate) fn overflows(&self) -> u32 {
        self.overflows
    }

    pub(crate) fn add_overflows(&mut self, count: u32) {
        self.overflows = self.overflows.saturating_add(count);
    }

    pub(crate) fn drain_events(&mut self) -> Vec<ReceivedEvent> {
        self.inbound.drain(..).collect()
    }

    pub(crate) fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    pub(crate) fn diagnostics(&self) -> TransportDiagnostics {
        TransportDiagnostics {
            generation: self.generation,
            pending_commands: self.pending.len(),
            queued_events: self.inbound.len(),
            overflows: self.overflows,
            stale_messages: self.stale_messages,
            duplicate_or_unknown_responses: self.duplicate_or_unknown_responses,
            out_of_order_responses: self.out_of_order_responses,
        }
    }
}

#[derive(Clone)]
pub struct RemoteBackendControl {
    pub(crate) inner: Rc<RefCell<TransportCore>>,
}

impl RemoteBackendControl {
    pub fn attach(
        &self,
        endpoint: Box<dyn MessageEndpoint>,
        generation: u64,
        input_channels: u32,
        output_channels: u32,
    ) -> Result<()> {
        self.inner
            .borrow_mut()
            .attach(endpoint, generation, input_channels, output_channels)
    }

    pub fn detach(&self, send_shutdown: bool) {
        self.inner.borrow_mut().detach(send_shutdown);
    }

    pub fn receive(&self, generation: u64, message: &str) -> Result<()> {
        self.inner.borrow_mut().receive(generation, message)
    }

    pub fn set_driver_state(&self, state: BackendDriverState) {
        self.inner.borrow_mut().readiness.driver_state = state;
    }

    pub fn driver_state(&self) -> BackendDriverState {
        self.inner.borrow().readiness.driver_state
    }

    pub fn fail(&self, message: impl Into<String>) {
        self.inner.borrow_mut().fail(message.into());
    }

    pub fn readiness(&self) -> RemoteReadiness {
        self.inner.borrow().readiness
    }

    pub fn diagnostics(&self) -> TransportDiagnostics {
        self.inner.borrow().diagnostics()
    }

    pub fn is_quiescent(&self) -> bool {
        self.inner.borrow().is_quiescent()
    }

    pub fn wait_for_quiescence(&self, timeout: Duration, mut progress: impl FnMut()) -> Result<()> {
        let started = Instant::now();
        loop {
            if self.is_quiescent() {
                return Ok(());
            }
            if started.elapsed() >= timeout {
                return Err(anyhow!(
                    "remote transport did not become quiescent within {timeout:?}"
                ));
            }
            progress();
        }
    }
}

pub(crate) fn transport_pair() -> (Rc<RefCell<TransportCore>>, RemoteBackendControl) {
    let inner = Rc::new(RefCell::new(TransportCore::default()));
    (
        inner.clone(),
        RemoteBackendControl {
            inner: inner.clone(),
        },
    )
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;

    #[derive(Clone, Default)]
    struct MemoryEndpoint {
        sent: Rc<RefCell<Vec<String>>>,
    }

    impl MessageEndpoint for MemoryEndpoint {
        fn post_message(&self, message: &str) -> Result<()> {
            self.sent.borrow_mut().push(message.to_owned());
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct SwitchableEndpoint {
        sent: Rc<RefCell<Vec<String>>>,
        fail: Rc<Cell<bool>>,
    }

    impl MessageEndpoint for SwitchableEndpoint {
        fn post_message(&self, message: &str) -> Result<()> {
            if self.fail.get() {
                return Err(anyhow!("injected endpoint failure"));
            }
            self.sent.borrow_mut().push(message.to_owned());
            Ok(())
        }
    }

    struct FailingEndpoint;

    impl MessageEndpoint for FailingEndpoint {
        fn post_message(&self, _message: &str) -> Result<()> {
            Err(anyhow!("injected endpoint failure"))
        }
    }

    fn response(sequence: u64, event: Event) -> String {
        serde_json::to_string(&EventEnvelope {
            version: PROTOCOL_VERSION,
            sequence,
            event,
        })
        .unwrap()
    }

    #[shoop_wasm_test_support::shoop_test]
    fn replay_is_ordered_and_excludes_ephemeral_commands() {
        let (transport, control) = transport_pair();
        transport
            .borrow_mut()
            .journal(Command::SetLoopGain {
                loop_id: 2,
                gain: 0.5,
            })
            .unwrap();
        transport
            .borrow_mut()
            .journal(Command::SetBusControl {
                bus_id: 1,
                control: shoop_audio_protocol::WireBusControl::GainDb(-3.0),
            })
            .unwrap();
        transport
            .borrow_mut()
            .journal(Command::SetBusControl {
                bus_id: 1,
                control: shoop_audio_protocol::WireBusControl::GainDb(-6.0),
            })
            .unwrap();
        transport
            .borrow_mut()
            .journal(Command::SetBusControl {
                bus_id: 1,
                control: shoop_audio_protocol::WireBusControl::Mute(true),
            })
            .unwrap();
        assert!(transport.borrow_mut().ephemeral(Command::Poll).is_err());
        let endpoint = MemoryEndpoint::default();
        let sent = endpoint.sent.clone();
        control.attach(Box::new(endpoint), 4, 1, 2).unwrap();
        let commands = sent
            .borrow()
            .iter()
            .map(|json| {
                serde_json::from_str::<CommandEnvelope>(json)
                    .unwrap()
                    .command
            })
            .collect::<Vec<_>>();
        let expected = vec![
            Command::ConfigureDeviceChannels {
                input_channels: 1,
                output_channels: 2,
            },
            Command::SetLoopGain {
                loop_id: 2,
                gain: 0.5,
            },
            Command::SetBusControl {
                bus_id: 1,
                control: shoop_audio_protocol::WireBusControl::GainDb(-6.0),
            },
            Command::SetBusControl {
                bus_id: 1,
                control: shoop_audio_protocol::WireBusControl::Mute(true),
            },
        ];
        assert_eq!(commands, expected);

        transport.borrow_mut().ephemeral(Command::Poll).unwrap();
        sent.borrow_mut().clear();
        control.detach(false);
        control
            .attach(Box::new(MemoryEndpoint { sent: sent.clone() }), 5, 1, 2)
            .unwrap();
        let replayed = sent
            .borrow()
            .iter()
            .map(|json| {
                serde_json::from_str::<CommandEnvelope>(json)
                    .unwrap()
                    .command
            })
            .collect::<Vec<_>>();
        assert_eq!(replayed, expected);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn session_replacement_atomically_replaces_complete_session_replay_state() {
        let (transport, control) = transport_pair();
        for command in [
            Command::SetPortConnected {
                application_port_id: 90,
                host_port_id: "old:playback".to_owned(),
                connected: true,
            },
            Command::SetMixerRoute {
                source_port_id: 11,
                destination_channel_id: 1,
                connected: true,
            },
            Command::SetBusControl {
                bus_id: 1,
                control: shoop_audio_protocol::WireBusControl::Mute(true),
            },
            Command::SetLoopGain {
                loop_id: 2,
                gain: 0.5,
            },
        ] {
            transport.borrow_mut().journal(command).unwrap();
        }
        let retained = Command::SetLoopSmoothingMs { milliseconds: 19 };
        transport.borrow_mut().journal(retained.clone()).unwrap();
        let replacement = vec![
            Command::BeginSessionReplace {
                generation: 7,
                total_bytes: 2,
            },
            Command::WriteSessionReplace {
                generation: 7,
                offset: 0,
                bytes: vec![1, 2],
            },
            Command::CommitSessionReplace { generation: 7 },
        ];
        transport
            .borrow_mut()
            .reserve_session_replay(7, Arc::from([1_u8, 2]))
            .unwrap();
        assert!(transport
            .borrow_mut()
            .journal(Command::SetMixerRoute {
                source_port_id: 33,
                destination_channel_id: 3,
                connected: true,
            })
            .is_err());
        transport.borrow_mut().commit_reserved_session_replay();
        let post_replacement = Command::SetBusControl {
            bus_id: 1,
            control: shoop_audio_protocol::WireBusControl::GainDb(-6.0),
        };
        transport
            .borrow_mut()
            .journal(post_replacement.clone())
            .unwrap();

        let endpoint = MemoryEndpoint::default();
        let sent = endpoint.sent.clone();
        control.attach(Box::new(endpoint), 1, 0, 2).unwrap();
        let commands = sent
            .borrow()
            .iter()
            .map(|json| {
                serde_json::from_str::<CommandEnvelope>(json)
                    .unwrap()
                    .command
            })
            .collect::<Vec<_>>();
        let initial = [
            vec![Command::ConfigureDeviceChannels {
                input_channels: 0,
                output_channels: 2,
            }],
            vec![retained],
            replacement,
        ]
        .concat();
        assert_eq!(commands, initial);
        for sequence in 1..=5 {
            control.receive(1, &response(sequence, Event::Ack)).unwrap();
        }
        let commands = sent
            .borrow()
            .iter()
            .map(|json| {
                serde_json::from_str::<CommandEnvelope>(json)
                    .unwrap()
                    .command
            })
            .collect::<Vec<_>>();
        assert_eq!(commands, [initial, vec![post_replacement]].concat());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn session_replacement_reservation_blocks_only_session_mutations_until_cancel() {
        let (transport, _) = transport_pair();
        transport
            .borrow_mut()
            .reserve_session_replay(7, Arc::from(vec![0_u8; 9 * 1024 * 1024]))
            .unwrap();
        let session_command = Command::SetLoopGain {
            loop_id: 1,
            gain: 0.25,
        };
        assert!(transport
            .borrow_mut()
            .journal(session_command.clone())
            .is_err());
        transport
            .borrow_mut()
            .journal(Command::SetLoopSmoothingMs { milliseconds: 17 })
            .unwrap();
        transport.borrow_mut().cancel_reserved_session_replay();
        transport.borrow_mut().journal(session_command).unwrap();
    }

    #[shoop_wasm_test_support::shoop_test]
    fn large_session_replay_streams_with_bounded_pending_commands() {
        let (transport, control) = transport_pair();
        let bytes: Arc<[u8]> = Arc::from(vec![0_u8; 9 * 1024 * 1024]);
        let chunk_count = bytes.len().div_ceil(SESSION_TRANSFER_CHUNK_BYTES);
        transport
            .borrow_mut()
            .reserve_session_replay(7, Arc::clone(&bytes))
            .unwrap();
        transport.borrow_mut().commit_reserved_session_replay();
        let endpoint = MemoryEndpoint::default();
        let sent = endpoint.sent.clone();
        control.attach(Box::new(endpoint), 1, 0, 2).unwrap();
        let total_commands = 1 + 1 + chunk_count + 1;
        for sequence in 1..=total_commands as u64 {
            assert!(transport.borrow().pending_len() <= COMMAND_CAPACITY);
            control.receive(1, &response(sequence, Event::Ack)).unwrap();
        }
        assert_eq!(sent.borrow().len(), total_commands);
        assert!(transport.borrow().inbound.len() <= 1);
        assert_eq!(transport.borrow().readiness().replay, ReplayState::Complete);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn full_durable_journal_leaves_device_configuration_replay_headroom() {
        let (transport, control) = transport_pair();
        for loop_id in 0..DURABLE_COMMAND_CAPACITY as u64 {
            transport
                .borrow_mut()
                .journal(Command::SetLoopGain { loop_id, gain: 0.5 })
                .unwrap();
        }
        let endpoint = MemoryEndpoint::default();
        let sent = endpoint.sent.clone();
        control.attach(Box::new(endpoint), 1, 0, 2).unwrap();
        assert_eq!(sent.borrow().len(), COMMAND_CAPACITY);
        assert_eq!(transport.borrow().pending_len(), COMMAND_CAPACITY);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn loop_smoothing_journal_replays_only_the_latest_value() {
        let (transport, control) = transport_pair();
        transport
            .borrow_mut()
            .journal(Command::SetLoopSmoothingMs { milliseconds: 0 })
            .unwrap();
        transport
            .borrow_mut()
            .journal(Command::SetLoopSmoothingMs { milliseconds: 17 })
            .unwrap();
        let endpoint = MemoryEndpoint::default();
        let sent = endpoint.sent.clone();
        control.attach(Box::new(endpoint), 1, 0, 2).unwrap();
        let commands = sent
            .borrow()
            .iter()
            .map(|json| {
                serde_json::from_str::<CommandEnvelope>(json)
                    .unwrap()
                    .command
            })
            .collect::<Vec<_>>();
        assert_eq!(
            commands,
            vec![
                Command::ConfigureDeviceChannels {
                    input_channels: 0,
                    output_channels: 2,
                },
                Command::SetLoopSmoothingMs { milliseconds: 17 },
            ]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn response_validation_rejects_stale_unknown_duplicate_and_out_of_order_events() {
        let (_, stale) = transport_pair();
        stale
            .attach(Box::new(MemoryEndpoint::default()), 7, 0, 2)
            .unwrap();
        assert!(stale.receive(6, &response(1, Event::Ack)).is_err());
        assert_eq!(stale.diagnostics().stale_messages, 1);

        let (_, unknown) = transport_pair();
        unknown
            .attach(Box::new(MemoryEndpoint::default()), 7, 0, 2)
            .unwrap();
        assert!(unknown.receive(7, &response(99, Event::Ack)).is_err());
        assert_eq!(unknown.diagnostics().duplicate_or_unknown_responses, 1);

        let (_, duplicate) = transport_pair();
        duplicate
            .attach(Box::new(MemoryEndpoint::default()), 7, 0, 2)
            .unwrap();
        duplicate.receive(7, &response(1, Event::Ack)).unwrap();
        assert!(duplicate.receive(7, &response(1, Event::Ack)).is_err());
        assert_eq!(duplicate.diagnostics().duplicate_or_unknown_responses, 1);

        let (transport, out_of_order) = transport_pair();
        transport
            .borrow_mut()
            .journal(Command::SetLoopGain {
                loop_id: 1,
                gain: 0.25,
            })
            .unwrap();
        out_of_order
            .attach(Box::new(MemoryEndpoint::default()), 7, 0, 2)
            .unwrap();
        assert!(out_of_order.receive(7, &response(2, Event::Ack)).is_err());
        assert_eq!(out_of_order.diagnostics().out_of_order_responses, 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn response_validation_rejects_malformed_and_wrong_version_events() {
        let (_, malformed) = transport_pair();
        malformed
            .attach(Box::new(MemoryEndpoint::default()), 1, 0, 2)
            .unwrap();
        assert!(malformed.receive(1, "not json").is_err());
        assert_eq!(malformed.readiness().connection, ConnectionState::Failed);

        let (_, wrong_version) = transport_pair();
        wrong_version
            .attach(Box::new(MemoryEndpoint::default()), 1, 0, 2)
            .unwrap();
        let event = serde_json::to_string(&EventEnvelope {
            version: PROTOCOL_VERSION.saturating_add(1),
            sequence: 1,
            event: Event::Ack,
        })
        .unwrap();
        assert!(wrong_version.receive(1, &event).is_err());
        assert_eq!(wrong_version.readiness().protocol, ProtocolState::Failed);

        let (_, terminal) = transport_pair();
        terminal
            .attach(Box::new(MemoryEndpoint::default()), 1, 0, 2)
            .unwrap();
        let error = terminal
            .receive(
                1,
                &response(
                    0,
                    Event::Error {
                        message: "worker trapped".to_owned(),
                    },
                ),
            )
            .unwrap_err();
        assert_eq!(error.to_string(), "remote engine failure: worker trapped");
        assert_eq!(terminal.readiness().engine, RemoteEngineState::Failed);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn bounded_failure_shutdown_and_journal_edges_are_observable() {
        let (journal, _) = transport_pair();
        for loop_id in 0..DURABLE_COMMAND_CAPACITY as u64 {
            journal
                .borrow_mut()
                .journal(Command::SetLoopGain { loop_id, gain: 0.5 })
                .unwrap();
        }
        assert!(journal
            .borrow_mut()
            .journal(Command::SetLoopGain {
                loop_id: u64::MAX,
                gain: 0.25,
            })
            .is_err());
        let rejected = Command::SetLoopGain {
            loop_id: 0,
            gain: 0.5,
        };
        journal.borrow_mut().reject_journaled(&rejected);
        assert_eq!(journal.borrow().journal.len(), DURABLE_COMMAND_CAPACITY - 1);

        let (_, failed_post) = transport_pair();
        assert!(failed_post
            .attach(Box::new(FailingEndpoint), 1, 0, 0)
            .is_err());

        let (_, uncorrelated) = transport_pair();
        uncorrelated
            .attach(Box::new(MemoryEndpoint::default()), 1, 0, 0)
            .unwrap();
        assert!(uncorrelated.receive(1, &response(0, Event::Ack)).is_err());

        let (_, stopped) = transport_pair();
        stopped
            .attach(Box::new(MemoryEndpoint::default()), 1, 0, 0)
            .unwrap();
        stopped.receive(1, &response(1, Event::Stopped)).unwrap();
        assert_eq!(stopped.readiness().engine, RemoteEngineState::Stopped);

        let (_, shutdown) = transport_pair();
        let endpoint = MemoryEndpoint::default();
        let sent = endpoint.sent.clone();
        shutdown.attach(Box::new(endpoint), 1, 0, 0).unwrap();
        shutdown.detach(true);
        let command = serde_json::from_str::<CommandEnvelope>(sent.borrow().last().unwrap())
            .unwrap()
            .command;
        assert!(matches!(command, Command::Shutdown));

        let (queue, overflow) = transport_pair();
        overflow
            .attach(Box::new(MemoryEndpoint::default()), 1, 0, 0)
            .unwrap();
        for _ in 1..COMMAND_CAPACITY {
            queue.borrow_mut().ephemeral(Command::Poll).unwrap();
        }
        for sequence in 1..=COMMAND_CAPACITY as u64 {
            overflow
                .receive(1, &response(sequence, Event::Ack))
                .unwrap();
        }
        queue.borrow_mut().ephemeral(Command::Poll).unwrap();
        assert!(overflow
            .receive(1, &response(COMMAND_CAPACITY as u64 + 1, Event::Ack),)
            .is_err());
        assert_eq!(overflow.readiness().connection, ConnectionState::Failed);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn bounded_quiescence_wait_completes_only_after_pending_work_settles() {
        let (_, control) = transport_pair();
        control
            .attach(Box::new(MemoryEndpoint::default()), 1, 0, 2)
            .unwrap();
        assert!(control.wait_for_quiescence(Duration::ZERO, || {}).is_err());
        let mut delivered = false;
        control
            .wait_for_quiescence(Duration::from_secs(1), || {
                if !delivered {
                    control.receive(1, &response(1, Event::Ack)).unwrap();
                    control.inner.borrow_mut().drain_events();
                    delivered = true;
                }
            })
            .unwrap();
        assert!(control.is_quiescent());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn failed_submission_restores_new_and_superseded_journal_entries() {
        let (transport, control) = transport_pair();
        let old_route = Command::SetMixerRoute {
            source_port_id: 2,
            destination_channel_id: 1,
            connected: false,
        };
        transport.borrow_mut().journal(old_route.clone()).unwrap();
        let endpoint = SwitchableEndpoint::default();
        let switch = endpoint.fail.clone();
        control.attach(Box::new(endpoint), 1, 0, 2).unwrap();
        control.receive(1, &response(1, Event::Ack)).unwrap();
        control.receive(1, &response(2, Event::Ack)).unwrap();
        switch.set(true);

        assert!(transport
            .borrow_mut()
            .journal(Command::SetMixerRoute {
                source_port_id: 2,
                destination_channel_id: 1,
                connected: true,
            })
            .is_err());
        assert_eq!(transport.borrow().journal, [old_route.clone()]);

        transport.borrow_mut().reject_journaled(&old_route);
        assert!(transport
            .borrow_mut()
            .journal(Command::SetPortConnected {
                application_port_id: 9,
                host_port_id: "system:playback_1".to_owned(),
                connected: true,
            })
            .is_err());
        assert!(transport.borrow().journal.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn bounded_command_saturation_is_observable_without_failing_the_driver() {
        let (transport, control) = transport_pair();
        control.set_driver_state(BackendDriverState::Running);
        control
            .attach(Box::new(MemoryEndpoint::default()), 1, 0, 2)
            .unwrap();
        for _ in 0..=COMMAND_CAPACITY {
            let _ = transport.borrow_mut().ephemeral(Command::Poll);
        }
        let diagnostics = control.diagnostics();
        assert_eq!(diagnostics.pending_commands, COMMAND_CAPACITY);
        assert!(diagnostics.overflows > 0);
        assert_eq!(control.driver_state(), BackendDriverState::Running);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn readiness_requires_driver_protocol_replay_and_engine_observation() {
        let (_, control) = transport_pair();
        control.set_driver_state(BackendDriverState::Running);
        control
            .attach(Box::new(MemoryEndpoint::default()), 3, 0, 2)
            .unwrap();
        assert!(!control.readiness().is_ready());
        control.receive(3, &response(1, Event::Ack)).unwrap();
        assert!(!control.readiness().is_ready());

        let mut inner = control.inner.borrow_mut();
        inner.ephemeral(Command::Poll).unwrap();
        drop(inner);
        control
            .receive(3, &response(2, Event::Snapshot(Default::default())))
            .unwrap();
        assert!(control.readiness().is_ready());

        control
            .attach(Box::new(MemoryEndpoint::default()), 4, 0, 2)
            .unwrap();
        let restarted = control.readiness();
        assert_eq!(restarted.driver_state, BackendDriverState::Running);
        assert_eq!(restarted.connection, ConnectionState::Attached);
        assert_eq!(restarted.protocol, ProtocolState::Initializing);
        assert_eq!(restarted.replay, ReplayState::Replaying);
        assert_eq!(restarted.engine, RemoteEngineState::Unknown);
        assert!(!restarted.is_ready());
        assert!(control.receive(3, &response(3, Event::Ack)).is_err());
        assert_eq!(control.diagnostics().stale_messages, 1);
    }
}
