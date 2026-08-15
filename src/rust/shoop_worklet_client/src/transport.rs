use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use shoop_audio_protocol::{
    Command, CommandEnvelope, Event, EventEnvelope, COMMAND_CAPACITY, PROTOCOL_VERSION,
};
use shoop_backend::BackendDriverState;

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

struct PendingCommand {
    command: Command,
    replay: bool,
}

pub(crate) struct ReceivedEvent {
    pub envelope: EventEnvelope,
    pub command: Command,
    pub generation: u64,
}

pub(crate) struct TransportCore {
    generation: u64,
    readiness: RemoteReadiness,
    error: Option<String>,
    endpoint: Option<Box<dyn MessageEndpoint>>,
    journal: Vec<Command>,
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
        if let Some(existing) = self
            .journal
            .iter_mut()
            .rev()
            .find(|existing| command.supersedes_in_journal(existing))
        {
            *existing = command.clone();
        } else {
            if self.journal.len() >= COMMAND_CAPACITY {
                self.overflows = self.overflows.saturating_add(1);
                return Err(anyhow!("remote worklet command journal is full"));
            }
            self.journal.push(command.clone());
        }
        if self.endpoint.is_some() {
            self.send(command, false)?;
        }
        Ok(())
    }

    pub(crate) fn reject_journaled(&mut self, command: &Command) {
        self.journal.retain(|candidate| candidate != command);
    }

    pub(crate) fn ephemeral(&mut self, command: Command) -> Result<()> {
        if self.endpoint.is_none() {
            return Err(anyhow!("remote worklet is not connected"));
        }
        self.send(command, false).map(|_| ())
    }

    fn send(&mut self, command: Command, replay: bool) -> Result<u64> {
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
        self.pending
            .insert(sequence, PendingCommand { command, replay });
        if replay {
            self.replay_sequences.insert(sequence);
        }
        Ok(sequence)
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
        )?;
        let journal = self.journal.clone();
        for command in journal
            .iter()
            .filter(|command| matches!(command, Command::ConfigureMidiEndpoints { .. }))
            .cloned()
        {
            self.send(command, true)?;
        }
        for command in journal
            .into_iter()
            .filter(|command| !matches!(command, Command::ConfigureMidiEndpoints { .. }))
        {
            self.send(command, true)?;
        }
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
        }
        if self.readiness.protocol == ProtocolState::Initializing && event.sequence == 1 {
            self.readiness.protocol = ProtocolState::Negotiated;
        }
        if self.replay_sequences.is_empty() {
            self.readiness.replay = ReplayState::Complete;
        }
        match &event.event {
            Event::Snapshot(_) => self.readiness.engine = RemoteEngineState::Observed,
            Event::Stopped => self.readiness.engine = RemoteEngineState::Stopped,
            _ => {}
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
            generation,
        });
        Ok(())
    }

    fn detach(&mut self, send_shutdown: bool) {
        if send_shutdown && self.endpoint.is_some() && self.pending.len() < COMMAND_CAPACITY {
            let _ = self.send(Command::Shutdown, false);
        }
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close();
        }
        self.pending.clear();
        self.replay_sequences.clear();
        self.inbound.clear();
        self.readiness.connection = ConnectionState::Detached;
        self.readiness.protocol = ProtocolState::Detached;
        self.readiness.replay = ReplayState::NotStarted;
        self.readiness.engine = RemoteEngineState::Unknown;
    }

    fn fail(&mut self, message: String) {
        if self.error.is_none() {
            self.error = Some(message);
        }
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close();
        }
        self.pending.clear();
        self.replay_sequences.clear();
        self.readiness.driver_state = BackendDriverState::Failed;
        self.readiness.connection = ConnectionState::Failed;
        self.readiness.protocol = ProtocolState::Failed;
        self.readiness.replay = ReplayState::Failed;
        self.readiness.engine = RemoteEngineState::Failed;
    }

    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
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
    use std::cell::RefCell;
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

    fn response(sequence: u64, event: Event) -> String {
        serde_json::to_string(&EventEnvelope {
            version: PROTOCOL_VERSION,
            sequence,
            event,
        })
        .unwrap()
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn replay_is_ordered_and_excludes_ephemeral_commands() {
        let (transport, control) = transport_pair();
        transport
            .borrow_mut()
            .journal(Command::SetLoopGain {
                loop_id: 2,
                gain: 0.5,
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

    #[tracy_nextest_capture::tracy_capture_test]
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

    #[tracy_nextest_capture::tracy_capture_test]
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

    #[tracy_nextest_capture::tracy_capture_test]
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

    #[tracy_nextest_capture::tracy_capture_test]
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

    #[tracy_nextest_capture::tracy_capture_test]
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
