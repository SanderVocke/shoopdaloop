//! Checkpoints for a private replacement graph. The spare processor must be claimed before restore.
use alloc::{boxed::Box, string::String, vec::Vec};
use core::task::Poll;
use shoop_rt_protocol::{
    content::MAX_CONTENT_BYTES, ProcessorSlot, ProcessorTarget, Refusal,
    ReplacementCheckpointCommand as Command, MAX_PROCESSOR_CHECKPOINT_BYTES,
};

use crate::{
    model::{Model, ProcessorOwner},
    ControlEndpoint, HostedControlTicket, ProcessorPhase,
};

struct Upload {
    target: ProcessorTarget,
    slot: ProcessorSlot,
    expected: usize,
    bytes: Vec<u8>,
}

struct Pending {
    slot: ProcessorSlot,
    owner: u64,
    generation: u64,
    ticket: Box<dyn HostedControlTicket>,
}

#[derive(Default)]
pub(crate) struct Store {
    uploads: Vec<Upload>,
    bytes: u64,
    pending: Option<Pending>,
}

fn candidate_owner(candidate: &Model, target: ProcessorTarget) -> Result<ProcessorOwner, Refusal> {
    match target {
        ProcessorTarget::Strip(strip) => {
            candidate.strip(strip).ok_or(Refusal::InvalidId)?.processor
        }
        ProcessorTarget::Bus(bus) => candidate.bus(bus).ok_or(Refusal::InvalidId)?.processor,
    }
    .filter(|owner| owner.kind.is_carla())
    .ok_or(Refusal::Unsupported)
}

impl Store {
    pub(crate) fn receive(
        &mut self,
        candidate: &Model,
        target: ProcessorTarget,
        command: Command,
    ) -> Result<(), Refusal> {
        let slot = candidate_owner(candidate, target)?
            .slot
            .ok_or(Refusal::InvalidId)?;
        let current = self.uploads.iter_mut().find(|upload| upload.slot == slot);
        match command {
            Command::Begin { bytes } => {
                let expected = bytes as usize;
                if current.is_some() || expected == 0 || expected > MAX_PROCESSOR_CHECKPOINT_BYTES {
                    return Err(Refusal::InvalidValue);
                }
                let total = self
                    .bytes
                    .checked_add(u64::from(bytes))
                    .filter(|total| *total <= MAX_CONTENT_BYTES)
                    .ok_or(Refusal::Capacity)?;
                let mut storage = Vec::new();
                storage
                    .try_reserve_exact(expected)
                    .map_err(|_| Refusal::Capacity)?;
                self.uploads.try_reserve(1).map_err(|_| Refusal::Capacity)?;
                self.uploads.push(Upload {
                    target,
                    slot,
                    expected,
                    bytes: storage,
                });
                self.bytes = total;
            }
            Command::Write { offset, data } => {
                let upload = current
                    .filter(|upload| upload.target == target)
                    .ok_or(Refusal::InvalidId)?;
                let data = data.as_bytes();
                if offset as usize != upload.bytes.len()
                    || data.len() > upload.expected - upload.bytes.len()
                {
                    return Err(Refusal::InvalidValue);
                }
                upload.bytes.extend_from_slice(data);
            }
        }
        Ok(())
    }

    pub(crate) fn validate(&self, candidate: &Model) -> Result<(), Refusal> {
        let owners = candidate
            .strips
            .iter()
            .flatten()
            .filter_map(|strip| strip.processor)
            .chain(
                candidate
                    .buses
                    .iter()
                    .flatten()
                    .filter_map(|bus| bus.processor),
            )
            .filter(|owner| owner.kind.is_carla())
            .count();
        if owners != self.uploads.len()
            || self.uploads.iter().any(|upload| {
                upload.bytes.len() != upload.expected
                    || candidate_owner(candidate, upload.target)
                        .ok()
                        .and_then(|owner| owner.slot)
                        != Some(upload.slot)
            })
        {
            return Err(Refusal::InvalidValue);
        }
        Ok(())
    }
}

impl ControlEndpoint {
    pub(crate) fn poll_replacement_checkpoint(&mut self) -> Poll<Result<(), Refusal>> {
        let store = &mut self.replacement.checkpoints;
        let Some(pending) = &mut store.pending else {
            return Poll::Ready(Ok(()));
        };
        let Some(result) = pending.ticket.poll() else {
            return Poll::Pending;
        };
        let pending = store.pending.take().expect("pending checkpoint restore");
        Poll::Ready(result.and_then(|()| {
            let current = self
                .hosted_processors
                .as_ref()
                .and_then(|controls| controls.status(pending.slot.index));
            if current.is_some_and(|status| {
                status.owner == pending.owner && status.status.generation == pending.generation
            }) {
                Ok(())
            } else {
                Err(Refusal::InvalidId)
            }
        }))
    }

    pub(crate) fn start_replacement_checkpoint(&mut self) -> Result<bool, Refusal> {
        let Some(upload) = self.replacement.checkpoints.uploads.pop() else {
            return Ok(false);
        };
        let processor = self
            .processor_state(usize::from(upload.slot.index))
            .ok_or(Refusal::InvalidId)?;
        if !processor.kind.is_carla()
            || processor.generation != upload.slot.generation
            || processor.phase != ProcessorPhase::Owned
        {
            return Err(Refusal::InvalidId);
        }
        let controls = self
            .hosted_processors
            .as_ref()
            .ok_or(Refusal::Unsupported)?;
        let current = controls
            .status(upload.slot.index)
            .ok_or(Refusal::InvalidId)?;
        let state = String::from_utf8(upload.bytes).map_err(|_| Refusal::InvalidValue)?;
        let generation = current.status.generation;
        let ticket =
            controls.restore_candidate(upload.slot.index, current.owner, generation, state)?;
        self.replacement.checkpoints.pending = Some(Pending {
            slot: upload.slot,
            owner: current.owner,
            generation,
            ticket,
        });
        Ok(true)
    }
}
