use alloc::{boxed::Box, vec::Vec};
use shoop_rt_protocol::{
    EngineMessage, Lane, Outcome, ProcessorLogCommand, ProcessorLogPage, ProcessorSlot,
    ProcessorTarget, Refusal, BULK_CHUNK_BYTES, PROCESSOR_LOG_GENERATIONS,
    PROCESSOR_LOG_STREAM_BYTES,
};
use shoop_rt_transport::carrier::Outbox;

use crate::ControlEndpoint;

pub struct HostedProcessorLog {
    pub generation: u64,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_dropped: u64,
    pub stderr_dropped: u64,
}

pub trait HostedLogTicket: Send {
    fn poll(&mut self) -> Option<Result<Vec<HostedProcessorLog>, Refusal>>;
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct Owner {
    target: ProcessorTarget,
    slot: ProcessorSlot,
    service_owner: u64,
    worker_generation: u64,
}

struct Pending {
    owner: Owner,
    correlation: u64,
    ticket: Box<dyn HostedLogTicket>,
}

struct Snapshot {
    owner: Owner,
    id: u64,
    logs: Vec<HostedProcessorLog>,
}

struct PendingPage {
    owner: Owner,
    snapshot: u64,
    correlation: u64,
    page: ProcessorLogPage,
}

#[derive(Default)]
pub(crate) struct Store {
    pending: Option<Pending>,
    snapshot: Option<Snapshot>,
    page: Option<Box<PendingPage>>,
    latest_id: u64,
}

impl Store {
    pub fn idle(&self) -> bool {
        self.pending.is_none() && self.page.is_none()
    }

    pub fn disconnect(&mut self) {
        self.snapshot = None;
        self.pending = None;
        self.page = None;
    }

    fn publish(&mut self, owner: Owner, logs: Vec<HostedProcessorLog>) -> Result<Outcome, Refusal> {
        if logs.len() > PROCESSOR_LOG_GENERATIONS
            || logs.iter().any(|log| {
                log.generation == 0
                    || log.generation > owner.worker_generation
                    || log.stdout.len() > PROCESSOR_LOG_STREAM_BYTES
                    || log.stderr.len() > PROCESSOR_LOG_STREAM_BYTES
            })
            || logs
                .windows(2)
                .any(|logs| logs[0].generation >= logs[1].generation)
        {
            return Err(Refusal::Capacity);
        }
        let id = self.latest_id.checked_add(1).ok_or(Refusal::Capacity)?;
        self.latest_id = id;
        let generations = logs.len() as u8;
        self.snapshot = Some(Snapshot { owner, id, logs });
        Ok(Outcome::ProcessorLogs {
            snapshot: id,
            generations,
        })
    }

    fn get(&self, owner: Owner, id: u64) -> Result<&Snapshot, Refusal> {
        self.snapshot
            .as_ref()
            .filter(|snapshot| snapshot.owner == owner && snapshot.id == id)
            .ok_or(Refusal::InvalidId)
    }
}

impl ControlEndpoint {
    fn current_log_owner(&self, owner: Owner) -> bool {
        self.hosted_processor_owner(owner.target)
            .is_ok_and(|target| target.slot == Some(owner.slot))
            && self
                .hosted_processors
                .as_ref()
                .and_then(|controls| controls.status(owner.slot.index))
                .is_some_and(|current| {
                    current.owner == owner.service_owner
                        && current.status.generation == owner.worker_generation
                })
    }

    pub(crate) fn processor_logs(
        &mut self,
        correlation: u64,
        target: ProcessorTarget,
        slot: ProcessorSlot,
        worker_generation: u64,
        command: ProcessorLogCommand,
    ) -> Result<Option<Outcome>, Refusal> {
        if worker_generation == 0 {
            return Err(Refusal::Unsupported);
        }
        if self
            .hosted_logs
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| !self.current_log_owner(snapshot.owner))
        {
            self.hosted_logs.snapshot = None;
        }
        let controls = self.confirmed_hosted_processor(target, slot)?;
        let observed = controls.status(slot.index).ok_or(Refusal::Unsupported)?;
        if observed.status.generation != worker_generation {
            return Err(Refusal::InvalidId);
        }
        let owner = Owner {
            target,
            slot,
            service_owner: observed.owner,
            worker_generation,
        };
        match command {
            ProcessorLogCommand::Open => {
                if self.hosted_logs.pending.is_some() || self.hosted_logs.snapshot.is_some() {
                    return Ok(Some(Outcome::Refused(Refusal::Busy)));
                }
                let ticket =
                    controls.read_logs(slot.index, owner.service_owner, worker_generation)?;
                self.hosted_logs.pending = Some(Pending {
                    owner,
                    correlation,
                    ticket,
                });
                Ok(None)
            }
            ProcessorLogCommand::Read {
                snapshot,
                generation_index,
                stderr,
                offset,
            } => {
                if self.hosted_logs.page.is_some() {
                    return Err(Refusal::Busy);
                }
                let log = self
                    .hosted_logs
                    .get(owner, snapshot)?
                    .logs
                    .get(usize::from(generation_index))
                    .ok_or(Refusal::InvalidId)?;
                let (stream, dropped) = if stderr {
                    (&log.stderr, log.stderr_dropped)
                } else {
                    (&log.stdout, log.stdout_dropped)
                };
                let tail = stream.get(offset as usize..).ok_or(Refusal::InvalidValue)?;
                let len = tail.len().min(BULK_CHUNK_BYTES);
                let mut bytes = [0; BULK_CHUNK_BYTES];
                bytes[..len].copy_from_slice(&tail[..len]);
                self.hosted_logs.page = Some(Box::new(PendingPage {
                    owner,
                    snapshot,
                    correlation,
                    page: ProcessorLogPage {
                        generation: log.generation,
                        dropped,
                        total: stream.len() as u32,
                        offset,
                        len: len as u16,
                        bytes,
                    },
                }));
                Ok(None)
            }
            ProcessorLogCommand::Close { snapshot } => {
                self.hosted_logs.get(owner, snapshot)?;
                self.hosted_logs.snapshot = None;
                Ok(Some(Outcome::Done))
            }
        }
    }

    pub(crate) fn advance_hosted_logs(&mut self) {
        let Some(result) = self
            .hosted_logs
            .pending
            .as_mut()
            .and_then(|pending| pending.ticket.poll())
        else {
            return;
        };
        let pending = self
            .hosted_logs
            .pending
            .take()
            .expect("pending log request");
        let result = if self.current_log_owner(pending.owner) {
            result.and_then(|logs| self.hosted_logs.publish(pending.owner, logs))
        } else {
            Err(Refusal::InvalidId)
        };
        self.answer(pending.correlation, result.unwrap_or_else(Outcome::Refused));
    }

    pub(crate) fn flush_hosted_log_page(&mut self, out: &mut Outbox) {
        if out.available(Lane::Reliable) == 0 {
            return;
        }
        let Some(pending) = self.hosted_logs.page.take() else {
            return;
        };
        let correlation = pending.correlation;
        let message = if self.current_log_owner(pending.owner)
            && self
                .hosted_logs
                .get(pending.owner, pending.snapshot)
                .is_ok()
        {
            EngineMessage::ProcessorLogPage {
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
            self.hosted_logs.page = Some(pending);
        }
    }
}
