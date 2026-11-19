//! Immutable state snapshots served on the control endpoint.

use alloc::{boxed::Box, string::String};
use shoop_rt_protocol::{
    EngineMessage, Lane, Outcome, ProcessorCheckpointCommand, ProcessorCheckpointPage,
    ProcessorSlot, ProcessorTarget, Refusal, BULK_CHUNK_BYTES, MAX_PROCESSOR_CHECKPOINT_BYTES,
};
use shoop_rt_transport::carrier::Outbox;

use crate::{endpoint::HostedOwner, ControlEndpoint};

pub trait HostedCheckpointTicket: Send {
    fn poll(&mut self) -> Option<Result<String, Refusal>>;
}

struct Capture {
    correlation: u64,
    owner: HostedOwner,
    ticket: Box<dyn HostedCheckpointTicket>,
}

struct Snapshot {
    owner: HostedOwner,
    id: u64,
    state: String,
}

struct PendingPage {
    correlation: u64,
    owner: HostedOwner,
    page: ProcessorCheckpointPage,
}

#[derive(Default)]
pub(crate) struct Store {
    capture: Option<Capture>,
    snapshot: Option<Snapshot>,
    page: Option<Box<PendingPage>>,
    latest_id: u64,
}

impl Store {
    pub fn idle(&self) -> bool {
        self.capture.is_none() && self.page.is_none()
    }

    pub fn disconnect(&mut self) {
        self.capture = None;
        self.snapshot = None;
        self.page = None;
    }

    fn get(&self, owner: HostedOwner, id: u64) -> Result<&Snapshot, Refusal> {
        self.snapshot
            .as_ref()
            .filter(|snapshot| snapshot.id == id && snapshot.owner == owner)
            .ok_or(Refusal::InvalidId)
    }

    fn publish(&mut self, owner: HostedOwner, state: String) -> Result<Outcome, Refusal> {
        if state.is_empty() || state.len() > MAX_PROCESSOR_CHECKPOINT_BYTES {
            return Err(Refusal::Capacity);
        }
        let id = self.latest_id.checked_add(1).ok_or(Refusal::Capacity)?;
        self.latest_id = id;
        let bytes = state.len() as u32;
        self.snapshot = Some(Snapshot { owner, id, state });
        Ok(Outcome::ProcessorCheckpoint {
            snapshot: id,
            bytes,
        })
    }
}

impl ControlEndpoint {
    pub(crate) fn processor_checkpoint(
        &mut self,
        correlation: u64,
        target: ProcessorTarget,
        slot: ProcessorSlot,
        worker_generation: u64,
        command: ProcessorCheckpointCommand,
    ) -> Result<Option<Outcome>, Refusal> {
        if self
            .hosted_checkpoints
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| !self.current_hosted_owner(snapshot.owner))
        {
            self.hosted_checkpoints.snapshot = None;
        }
        let (owner, controls) = self.confirm_hosted_owner(target, slot, worker_generation)?;
        match command {
            ProcessorCheckpointCommand::Open => {
                if self.hosted_checkpoints.capture.is_some()
                    || self.hosted_checkpoints.snapshot.is_some()
                {
                    return Ok(Some(Outcome::Refused(Refusal::Busy)));
                }
                let ticket = controls.capture_checkpoint(
                    slot.index,
                    owner.service_owner,
                    worker_generation,
                )?;
                self.hosted_checkpoints.capture = Some(Capture {
                    correlation,
                    owner,
                    ticket,
                });
                Ok(None)
            }
            ProcessorCheckpointCommand::Read { snapshot, offset } => {
                if self.hosted_checkpoints.page.is_some() {
                    return Err(Refusal::Busy);
                }
                let stored = self.hosted_checkpoints.get(owner, snapshot)?;
                let tail = stored
                    .state
                    .as_bytes()
                    .get(offset as usize..)
                    .filter(|tail| !tail.is_empty())
                    .ok_or(Refusal::InvalidValue)?;
                let len = tail.len().min(BULK_CHUNK_BYTES);
                let mut bytes = [0; BULK_CHUNK_BYTES];
                bytes[..len].copy_from_slice(&tail[..len]);
                self.hosted_checkpoints.page = Some(Box::new(PendingPage {
                    correlation,
                    owner,
                    page: ProcessorCheckpointPage {
                        snapshot,
                        offset,
                        total: stored.state.len() as u32,
                        len: len as u16,
                        bytes,
                    },
                }));
                Ok(None)
            }
            ProcessorCheckpointCommand::Close { snapshot } => {
                self.hosted_checkpoints.get(owner, snapshot)?;
                self.hosted_checkpoints.snapshot = None;
                Ok(Some(Outcome::Done))
            }
        }
    }

    pub(crate) fn advance_hosted_checkpoints(&mut self) {
        let Some(result) = self
            .hosted_checkpoints
            .capture
            .as_mut()
            .and_then(|capture| capture.ticket.poll())
        else {
            return;
        };
        let capture = self
            .hosted_checkpoints
            .capture
            .take()
            .expect("pending checkpoint capture");
        let result = if self.current_hosted_owner(capture.owner) {
            result.and_then(|state| self.hosted_checkpoints.publish(capture.owner, state))
        } else {
            Err(Refusal::InvalidId)
        };
        self.answer(capture.correlation, result.unwrap_or_else(Outcome::Refused));
    }

    pub(crate) fn flush_hosted_checkpoint_page(&mut self, out: &mut Outbox) {
        if out.available(Lane::Reliable) == 0 {
            return;
        }
        let Some(pending) = self.hosted_checkpoints.page.take() else {
            return;
        };
        let correlation = pending.correlation;
        let message = if self.current_hosted_owner(pending.owner)
            && self
                .hosted_checkpoints
                .get(pending.owner, pending.page.snapshot)
                .is_ok()
        {
            EngineMessage::ProcessorCheckpointPage {
                correlation,
                page: pending.page,
            }
        } else {
            EngineMessage::Result {
                correlation,
                outcome: Outcome::Refused(Refusal::InvalidId),
            }
        };
        if out.push(message).is_ok() {
            self.unanswered = self.unanswered.saturating_sub(1);
        } else {
            self.hosted_checkpoints.page = Some(pending);
        }
    }
}
