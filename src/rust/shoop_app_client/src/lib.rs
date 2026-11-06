//! Client-side confirmed projection. Reattachment never replays commands or scripts.
#![forbid(unsafe_code)]

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[cfg(feature = "runtime")]
pub mod runtime;

mod diagnostics;

use shoop_app_protocol::*;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Detached,
    Attaching,
    Synchronizing,
    Ready,
    Lost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    NotReady,
    Full,
    Disconnected,
    Identity,
    Protocol,
    Exhausted,
}

struct Transfer {
    id: TransferId,
    revision: Revision,
    expected: usize,
    next: u32,
    bytes: Vec<u8>,
}

pub struct Client {
    diagnostics: diagnostics::Store,
    id: ClientId,

    state: State,
    authority: Option<Authority>,
    last_authority: Option<Authority>,
    refusal: Option<Refusal>,
    snapshot: Option<Arc<SessionSnapshot>>,
    next_operation: u64,
    pending: BTreeMap<OperationId, PendingOperation>,
    submitted: BTreeSet<OperationId>,
    results: BTreeMap<OperationId, Outcome>,
    transfer: Option<Transfer>,
    ready_ack: Option<Revision>,
    media_transfer: Option<Transfer>,
    expected_media: Option<(TransferId, shoop_app_api::LoopId)>,
    media: Option<Result<Arc<LoopMedia>, Refusal>>,
    next_transfer: u64,
    persistence_replies: VecDeque<persistence::Reply>,
    captured_image: Option<(persistence::Key, Arc<persistence_image::Image>)>,
}

impl Client {
    pub fn new() -> Self {
        Self {
            diagnostics: Default::default(),
            id: ClientId(0),
            state: State::Detached,
            authority: None,
            last_authority: None,
            refusal: None,
            snapshot: None,
            next_operation: 1,
            pending: BTreeMap::new(),
            submitted: BTreeSet::new(),
            results: BTreeMap::new(),
            transfer: None,
            ready_ack: None,
            media_transfer: None,
            expected_media: None,
            media: None,
            next_transfer: 1,
            persistence_replies: VecDeque::new(),
            captured_image: None,
        }
    }

    pub fn state(&self) -> State {
        self.state
    }
    pub fn authority(&self) -> Option<Authority> {
        self.authority
    }
    pub fn persistence(&self, request: persistence::Request) -> Result<ClientMessage, Error> {
        if self.state != State::Ready {
            return Err(Error::NotReady);
        }
        if !request.valid() {
            return Err(Error::Protocol);
        }
        Ok(ClientMessage::Persistence {
            authority: self.authority.ok_or(Error::NotReady)?,
            request,
        })
    }
    pub fn take_persistence_reply(&mut self) -> Option<persistence::Reply> {
        self.persistence_replies.pop_front()
    }
    pub fn take_captured_image(
        &mut self,
    ) -> Option<(persistence::Key, Arc<persistence_image::Image>)> {
        self.captured_image.take()
    }
    pub fn capture_image(
        &self,
        key: persistence::Key,
        target: persistence::Target,
    ) -> Result<ClientMessage, Error> {
        if self.state != State::Ready {
            return Err(Error::NotReady);
        }
        if !key.valid() || !target.valid() {
            return Err(Error::Protocol);
        }
        Ok(ClientMessage::CaptureImage {
            authority: self.authority.ok_or(Error::NotReady)?,
            key,
            target,
        })
    }
    pub fn stage_image(
        &self,
        key: persistence::Key,
        target: persistence::Target,
        image: Arc<persistence_image::Image>,
    ) -> Result<ClientMessage, Error> {
        if self.state != State::Ready {
            return Err(Error::NotReady);
        }
        if !key.valid() || !target.valid() {
            return Err(Error::Protocol);
        }
        Ok(ClientMessage::StageImage {
            authority: self.authority.ok_or(Error::NotReady)?,
            key,
            target,
            image,
        })
    }
    pub fn refusal(&self) -> Option<Refusal> {
        self.refusal
    }
    pub fn profiling(&self) -> &shoop_app_api::ProfilingView {
        &self.diagnostics.audio
    }
    pub fn diagnostics(&self) -> &shoop_app_api::DiagnosticsView {
        &self.diagnostics.view
    }
    pub fn advance_diagnostics(&mut self, elapsed: core::time::Duration) {
        self.diagnostics.advance(elapsed);
    }
    pub fn snapshot(&self) -> Option<&Arc<SessionSnapshot>> {
        self.snapshot.as_ref()
    }
    pub fn pending(&self) -> &BTreeMap<OperationId, PendingOperation> {
        &self.pending
    }
    pub fn has_unfinished_operations(&self) -> bool {
        !self.pending.is_empty() || !self.submitted.is_empty()
    }
    pub fn operation_in_flight(&self, operation: OperationId) -> bool {
        self.pending.contains_key(&operation) || self.submitted.contains(&operation)
    }
    pub fn results(&self) -> &BTreeMap<OperationId, Outcome> {
        &self.results
    }

    pub fn attach(&mut self) -> Result<ClientMessage, Error> {
        if !matches!(self.state, State::Detached | State::Lost) {
            return Err(Error::Protocol);
        }
        self.diagnostics = Default::default();
        self.state = State::Attaching;
        self.refusal = None;
        self.authority = None;
        self.last_authority = None;
        self.snapshot = None;
        self.transfer = None;
        self.ready_ack = None;
        self.media_transfer = None;
        self.expected_media = None;
        self.media = None;
        self.persistence_replies.clear();
        self.captured_image = None;
        self.pending.clear();
        self.submitted.clear();
        self.results.clear();
        Ok(ClientMessage::Attach { version: VERSION })
    }

    /// Construct one operation, without submission or replay. The caller must deliver it once.
    pub fn command(
        &mut self,
        command: Command,
        expected_revision: Option<Revision>,
    ) -> Result<ClientMessage, Error> {
        if self.state != State::Ready {
            return Err(Error::NotReady);
        }
        if !command.bounded() {
            return Err(Error::Protocol);
        }
        if self.pending.len() + self.submitted.len() >= MAX_PENDING_OPERATIONS {
            return Err(Error::Full);
        }
        let sequence = self.next_operation;
        self.next_operation = sequence.checked_add(1).ok_or(Error::Exhausted)?;
        let authority = self.authority.ok_or(Error::NotReady)?;
        let operation = OperationId {
            client: self.id,
            sequence,
        };
        self.submitted.insert(operation);
        Ok(ClientMessage::Submit {
            authority,
            session_epoch: self
                .snapshot
                .as_ref()
                .ok_or(Error::NotReady)?
                .persistence_epoch,
            operation,
            expected_revision,
            command,
        })
    }

    pub fn shutdown(&mut self) -> Result<ClientMessage, Error> {
        if self.state != State::Ready {
            return Err(Error::NotReady);
        }
        self.management_shutdown()
    }

    /// Reserve an operation for the authenticated management path, also after data loss.
    pub fn management_shutdown(&mut self) -> Result<ClientMessage, Error> {
        let authority = self.last_authority.ok_or(Error::NotReady)?;
        let sequence = self.next_operation;
        self.next_operation = sequence.checked_add(1).ok_or(Error::Exhausted)?;
        let operation = OperationId {
            client: self.id,
            sequence,
        };
        self.submitted.insert(operation);
        Ok(ClientMessage::Shutdown {
            authority,
            operation,
        })
    }

    /// Release a command that a carrier refused before enqueueing it.
    pub fn unsent(&mut self, operation: OperationId) {
        self.submitted.remove(&operation);
    }

    pub fn detach(&mut self) -> Result<ClientMessage, Error> {
        let authority = self.authority.ok_or(Error::NotReady)?;
        self.disconnect();
        self.state = State::Detached;
        Ok(ClientMessage::Detach { authority })
    }

    pub fn disconnect(&mut self) {
        self.diagnostics = Default::default();
        self.captured_image = None;
        self.persistence_replies.clear();
        self.state = State::Lost;
        self.authority = None;
        self.transfer = None;
        self.ready_ack = None;
        self.media_transfer = None;
        self.expected_media = None;
        self.media = None;
    }

    pub fn read_media(&mut self, loop_id: shoop_app_api::LoopId) -> Result<ClientMessage, Error> {
        if self.state != State::Ready {
            return Err(Error::NotReady);
        }
        if !loop_id.is_valid() {
            return Err(Error::Identity);
        }
        let transfer = TransferId(self.next_transfer);
        self.next_transfer = self.next_transfer.checked_add(1).ok_or(Error::Exhausted)?;
        self.expected_media = Some((transfer, loop_id));
        self.media_transfer = None;
        self.media = None;
        Ok(ClientMessage::ReadMedia {
            authority: self.authority.ok_or(Error::NotReady)?,
            transfer,
            loop_id,
        })
    }
    pub fn media(&self) -> Option<&Result<Arc<LoopMedia>, Refusal>> {
        self.media.as_ref()
    }
    pub fn cancel_media(&mut self) -> Result<ClientMessage, Error> {
        self.media_transfer = None;
        self.expected_media = None;
        self.media = None;
        Ok(ClientMessage::CancelMedia {
            authority: self.authority.ok_or(Error::NotReady)?,
        })
    }

    pub fn ready_message(&self) -> Option<ClientMessage> {
        Some(ClientMessage::Ready {
            authority: self.authority?,
            revision: self.ready_ack?,
        })
    }

    /// Call only after the Ready message was queued successfully on the ordered channel.
    pub fn ready_sent(&mut self) -> Result<(), Error> {
        if self.state != State::Synchronizing || self.ready_ack.take().is_none() {
            return Err(Error::Protocol);
        }
        self.state = State::Ready;
        Ok(())
    }

    pub fn receive(&mut self, message: ServiceMessage) -> Result<(), Error> {
        let result = if message.valid() {
            self.apply(message)
        } else {
            Err(Error::Protocol)
        };
        if result.is_err() {
            self.disconnect();
        }
        result
    }

    fn apply(&mut self, message: ServiceMessage) -> Result<(), Error> {
        match message {
            ServiceMessage::Diagnostic { authority, record } => {
                if self.authority == Some(authority) {
                    self.diagnostics.receive(record);
                }
            }
            ServiceMessage::CapturedImage {
                authority,
                key,
                image,
            } => {
                self.check(authority)?;
                self.captured_image = Some((key, image));
            }
            ServiceMessage::Persistence { authority, reply } => {
                self.check(authority)?;
                if self.persistence_replies.len() == MAX_MESSAGES_PER_TURN {
                    return Err(Error::Full);
                }
                self.persistence_replies.push_back(reply);
            }
            ServiceMessage::Media {
                authority,
                transfer,
                data,
            } => {
                self.check(authority)?;
                if let Some((expected, loop_id)) = self.expected_media {
                    if transfer == expected {
                        if data.loop_id != loop_id {
                            return Err(Error::Identity);
                        }
                        self.media = Some(Ok(data));
                    }
                }
            }
            ServiceMessage::MediaRefused {
                authority,
                transfer,
                reason,
            } => {
                self.check(authority)?;
                if self
                    .expected_media
                    .is_some_and(|(expected, _)| expected == transfer)
                {
                    self.media = Some(Err(reason));
                }
            }
            ServiceMessage::Attached { authority } => {
                if self.state != State::Attaching || !authority.is_valid() {
                    return Err(Error::Identity);
                }
                self.id = authority.client;
                self.next_operation = 1;
                self.authority = Some(authority);
                self.last_authority = Some(authority);
                self.state = State::Synchronizing;
            }
            ServiceMessage::Snapshot { authority, state } => {
                self.check(authority)?;
                if !matches!(self.state, State::Synchronizing | State::Ready)
                    || self
                        .snapshot
                        .as_ref()
                        .is_some_and(|old| old.revision > state.revision)
                    || state.pending.len() > MAX_PENDING_OPERATIONS
                    || state.results.len() > MAX_RETAINED_RESULTS
                {
                    return Err(Error::Protocol);
                }
                self.pending.clear();
                for operation in &state.pending {
                    if operation.id.client == self.id {
                        self.submitted.remove(&operation.id);
                        self.next_operation = self
                            .next_operation
                            .max(operation.id.sequence.saturating_add(1));
                        self.pending.insert(operation.id, operation.clone());
                    }
                }
                for result in &state.results {
                    if result.id.client == self.id {
                        self.finish(result.clone());
                    }
                }
                if self.pending.len() + self.submitted.len() > MAX_PENDING_OPERATIONS {
                    return Err(Error::Full);
                }
                if self.state == State::Synchronizing {
                    self.ready_ack = Some(state.revision);
                }
                self.snapshot = Some(state);
            }
            ServiceMessage::Accepted {
                authority,
                operation,
            } => {
                self.check(authority)?;
                if operation.id.client == self.id && !self.results.contains_key(&operation.id) {
                    self.submitted.remove(&operation.id);
                    if !self.pending.contains_key(&operation.id)
                        && self.pending.len() + self.submitted.len() == MAX_PENDING_OPERATIONS
                    {
                        return Err(Error::Full);
                    }
                    self.pending.insert(operation.id, operation);
                }
            }
            ServiceMessage::Finished { authority, result } => {
                self.check(authority)?;
                if result.id.client == self.id {
                    self.finish(result);
                }
            }
            ServiceMessage::Rejected(reason) => {
                if self.state != State::Attaching {
                    return Err(Error::Protocol);
                }
                self.disconnect();
                self.refusal = Some(reason);
            }
            ServiceMessage::Detached { authority } => {
                self.check(authority)?;
                self.disconnect();
                self.state = State::Detached;
            }
            ServiceMessage::ShutdownComplete { authority, result } => {
                self.check(authority)?;
                self.finish(result);
            }
        }
        Ok(())
    }

    fn check(&self, authority: Authority) -> Result<(), Error> {
        if self.authority == Some(authority) {
            Ok(())
        } else {
            Err(Error::Identity)
        }
    }

    fn finish(&mut self, result: OperationResult) {
        self.pending.remove(&result.id);
        self.submitted.remove(&result.id);
        self.next_operation = self
            .next_operation
            .max(result.id.sequence.saturating_add(1));
        self.results.insert(result.id, result.outcome);
        while self.results.len() > MAX_RETAINED_RESULTS {
            self.results.pop_first();
        }
    }

    pub fn pump_local(&mut self, port: &local::ClientPort) -> Result<usize, Error> {
        let mut handled = 0;
        for _ in 0..MAX_MESSAGES_PER_TURN {
            match port.updates.try_recv() {
                Ok(message) => {
                    self.receive(message)?;
                    handled += 1;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.disconnect();
                    return Err(Error::Disconnected);
                }
            }
        }
        if let Some(message) = self.ready_message() {
            match port.commands.try_send(message) {
                Ok(()) => self.ready_sent()?,
                Err(std::sync::mpsc::TrySendError::Full(_)) => {}
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    self.disconnect();
                    return Err(Error::Disconnected);
                }
            }
        }
        Ok(handled)
    }

    pub fn submit_local(
        &mut self,
        port: &local::ClientPort,
        command: Command,
        expected_revision: Option<Revision>,
    ) -> Result<OperationId, Error> {
        let message = self.command(command, expected_revision)?;
        let ClientMessage::Submit { operation, .. } = &message else {
            unreachable!()
        };
        let operation = *operation;
        match port.commands.try_send(message) {
            Ok(()) => Ok(operation),
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                self.unsent(operation);
                Err(Error::Full)
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                self.unsent(operation);
                self.disconnect();
                Err(Error::Disconnected)
            }
        }
    }

    pub fn receive_wire(&mut self, message: WireMessage) -> Result<(), Error> {
        let result = self.apply_wire(message);
        if result.is_err() {
            self.disconnect();
        }
        result
    }

    fn apply_wire(&mut self, message: WireMessage) -> Result<(), Error> {
        if !message.valid() {
            return Err(Error::Protocol);
        }
        match message {
            WireMessage::Diagnostic { authority, record } => {
                self.receive(ServiceMessage::Diagnostic { authority, record })
            }
            WireMessage::Persistence { authority, reply } => {
                self.receive(ServiceMessage::Persistence { authority, reply })
            }
            WireMessage::CarrierReady
            | WireMessage::ManagementRequest(_)
            | WireMessage::ManagementReply(_) => Err(Error::Protocol),
            WireMessage::MediaBegin {
                authority,
                transfer,
                bytes,
            } => {
                self.check(authority)?;
                if self
                    .expected_media
                    .is_some_and(|(expected, _)| expected == transfer)
                {
                    if self.media_transfer.is_some() {
                        return Err(Error::Protocol);
                    }
                    self.media_transfer = Some(Transfer {
                        id: transfer,
                        revision: Revision(0),
                        expected: bytes as usize,
                        next: 0,
                        bytes: Vec::with_capacity(bytes as usize),
                    });
                }
                Ok(())
            }
            WireMessage::MediaChunk {
                transfer,
                index,
                bytes,
            } => {
                if !self
                    .expected_media
                    .is_some_and(|(expected, _)| expected == transfer)
                {
                    return Ok(());
                }
                let active = self.media_transfer.as_mut().ok_or(Error::Protocol)?;
                if active.id != transfer
                    || active.next != index
                    || bytes.len() > active.expected - active.bytes.len()
                {
                    return Err(Error::Protocol);
                }
                active.bytes.extend_from_slice(&bytes);
                active.next += 1;
                Ok(())
            }
            WireMessage::MediaEnd { transfer } => {
                if !self
                    .expected_media
                    .is_some_and(|(expected, _)| expected == transfer)
                {
                    return Ok(());
                }
                let active = self.media_transfer.take().ok_or(Error::Protocol)?;
                if active.id != transfer || active.bytes.len() != active.expected {
                    return Err(Error::Protocol);
                }
                let data = codec::decode_media(&active.bytes).map_err(|_| Error::Protocol)?;
                self.receive(ServiceMessage::Media {
                    authority: self.authority.ok_or(Error::Identity)?,
                    transfer,
                    data: Arc::new(data),
                })
            }
            WireMessage::MediaRefused {
                authority,
                transfer,
                reason,
            } => self.receive(ServiceMessage::MediaRefused {
                authority,
                transfer,
                reason,
            }),
            WireMessage::Client(_) | WireMessage::Ping(_) | WireMessage::Pong(_) => {
                return Err(Error::Protocol)
            }
            WireMessage::Attached { authority } => {
                self.receive(ServiceMessage::Attached { authority })
            }
            WireMessage::Rejected(reason) => self.receive(ServiceMessage::Rejected(reason)),
            WireMessage::Accepted {
                authority,
                operation,
            } => self.receive(ServiceMessage::Accepted {
                authority,
                operation,
            }),
            WireMessage::Finished { authority, result } => {
                self.receive(ServiceMessage::Finished { authority, result })
            }
            WireMessage::Detached { authority } => {
                self.receive(ServiceMessage::Detached { authority })
            }
            WireMessage::ShutdownComplete { authority, result } => {
                self.receive(ServiceMessage::ShutdownComplete { authority, result })
            }
            WireMessage::SnapshotBegin {
                authority,
                transfer,
                revision,
                bytes,
            } => {
                self.check(authority)?;
                if self.transfer.is_some() {
                    return Err(Error::Protocol);
                }
                self.transfer = Some(Transfer {
                    id: transfer,
                    revision,
                    expected: bytes as usize,
                    next: 0,
                    bytes: Vec::with_capacity(bytes as usize),
                });
                Ok(())
            }
            WireMessage::SnapshotChunk {
                transfer,
                index,
                bytes,
            } => {
                let active = self.transfer.as_mut().ok_or(Error::Protocol)?;
                if active.id != transfer
                    || active.next != index
                    || bytes.len() > active.expected - active.bytes.len()
                {
                    return Err(Error::Protocol);
                }
                active.bytes.extend_from_slice(&bytes);
                active.next += 1;
                Ok(())
            }
            WireMessage::SnapshotEnd { transfer } => {
                let active = self.transfer.take().ok_or(Error::Protocol)?;
                if active.id != transfer || active.bytes.len() != active.expected {
                    return Err(Error::Protocol);
                }
                let state = codec::decode_snapshot(&active.bytes).map_err(|_| Error::Protocol)?;
                if state.revision != active.revision {
                    return Err(Error::Protocol);
                }
                self.receive(ServiceMessage::Snapshot {
                    authority: self.authority.ok_or(Error::Identity)?,
                    state: Arc::new(state),
                })
            }
        }
    }
}
