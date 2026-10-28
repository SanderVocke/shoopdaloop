use crate::{Config, Operation, PresetId, Receipt, Refusal, Request};
use rt_runtime::{
    transport::{
        self, ControlEndpoint, Lane, Outcome, RenderEndpoint, SubmitError, TransportConfig,
    },
    Epoch, Error, Frame, Sequence,
};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    Arc,
};

pub type Completion = transport::Completion<Request, ()>;

#[derive(Clone, Debug)]
pub struct Ticket {
    inner: transport::Ticket,
    identity: Arc<()>,
}
impl Ticket {
    pub fn sequence(&self) -> Sequence {
        self.inner.sequence()
    }
    /// Best effort until dequeue; use explicit recovery for already-delivered work.
    pub fn cancel(&self) {
        self.inner.cancel();
    }
}

pub struct Client {
    transport: ControlEndpoint<Request, (), ()>,
    identity: Arc<()>,
}

pub(crate) struct Mailbox {
    pub(crate) armed: AtomicBool,
    pub(crate) frame: AtomicU64,
    pub(crate) slot: AtomicU32,
    pub(crate) revision: AtomicU32,
    pub(crate) acknowledged: AtomicBool,
    pub(crate) actual_frame: AtomicU64,
    pub(crate) previous_revision: AtomicU32,
    pub(crate) actual_revision: AtomicU32,
}

pub struct RenderControl {
    transport: RenderEndpoint<Request, (), ()>,
    mailbox: Arc<Mailbox>,
    catalog: Box<[PresetId]>,
    config: Config,
    revision: u32,
    pending: Option<Sequence>,
    cancelled: Option<Sequence>,
    callback: Option<Frame>,
    callbacks: u64,
}

pub(crate) fn prepare(
    config: Config,
    catalog: Box<[PresetId]>,
    mailbox: Arc<Mailbox>,
) -> Result<(Client, RenderControl), Error> {
    let (mut client, render) = transport::channel(TransportConfig {
        normal_slots: 1,
        recovery_slots: 1,
        telemetry_slots: 1,
        operations_per_cycle: 2,
        recovery_per_cycle: 1,
        publications_per_cycle: 2,
    })?;
    client.prepare_submissions()?;
    Ok((
        Client {
            transport: client,
            identity: Arc::new(()),
        },
        RenderControl {
            transport: render,
            mailbox,
            catalog,
            config,
            revision: 1,
            pending: None,
            cancelled: None,
            callback: None,
            callbacks: 0,
        },
    ))
}

impl Client {
    fn submit(&mut self, lane: Lane, operation: Operation) -> Result<Ticket, SubmitError<Request>> {
        self.transport
            .submit_prepared(
                lane,
                Request {
                    operation,
                    result: None,
                },
            )
            .map(|inner| Ticket {
                inner,
                identity: Arc::clone(&self.identity),
            })
    }
    pub fn select(
        &mut self,
        at: Frame,
        preset: PresetId,
        expected_revision: u32,
    ) -> Result<Ticket, SubmitError<Request>> {
        self.submit(
            Lane::Normal,
            Operation::SelectPreparedPreset {
                at,
                preset,
                expected_revision,
            },
        )
    }
    pub fn cancel_pending(&mut self, ticket: &Ticket) -> Result<Ticket, SubmitError<Request>> {
        let operation = Operation::CancelPending {
            target: ticket.sequence(),
        };
        if !Arc::ptr_eq(&ticket.identity, &self.identity) {
            return Err(SubmitError {
                error: Error::InvalidId,
                value: Request {
                    operation,
                    result: None,
                },
            });
        }
        self.submit(Lane::Recovery, operation)
    }
    pub fn try_completion(&mut self) -> Option<Completion> {
        let mut completion = self.transport.try_completion()?;
        if completion.outcome == Outcome::Cancelled && completion.request.result.is_none() {
            completion.request.result = Some(Err(Refusal::Cancelled {
                frame: completion.frame,
            }));
        }
        Some(completion)
    }
    pub fn outstanding(&self) -> usize {
        self.transport.outstanding()
    }
    pub fn connected(&self) -> bool {
        self.transport.connected()
    }
}

impl RenderControl {
    pub fn callbacks(&self) -> u64 {
        self.callbacks
    }
    pub fn work_counters(&self) -> transport::WorkCounters {
        self.transport.counters()
    }
    pub fn revision(&self) -> u32 {
        self.revision
    }
    pub fn connected(&self) -> bool {
        self.transport.connected()
    }

    fn settle(&mut self, result: Result<Receipt, Refusal>, outcome: Outcome) -> Result<(), Error> {
        self.transport
            .next_operation()
            .ok_or(Error::InvalidState)?
            .request
            .result = Some(result);
        self.transport
            .complete(outcome, None)
            .map_err(|error| error.error)
    }
    fn refuse(&mut self, refusal: Refusal) -> Result<(), Error> {
        let outcome = match refusal {
            Refusal::Cancelled { .. } => Outcome::Cancelled,
            Refusal::Late { .. } | Refusal::InvalidOperation { .. } => {
                Outcome::Rejected(Error::InvalidEvent)
            }
            Refusal::InvalidPreset { .. } | Refusal::InvalidRecoveryTarget { .. } => {
                Outcome::Rejected(Error::InvalidId)
            }
            Refusal::StaleRevision { .. } => Outcome::Rejected(Error::InvalidState),
            Refusal::Disconnected { .. } => Outcome::Rejected(Error::Disconnected),
            Refusal::RevisionExhausted { .. } => Outcome::Rejected(Error::Capacity),
            Refusal::Fault { error, .. } => Outcome::Rejected(error),
        };
        self.settle(Err(refusal), outcome)
    }

    /// Call once before the host's one engine callback, never once per interval.
    /// Zero-frame calls are inert and do not snapshot ingress or replenish quotas.
    pub fn begin_callback(&mut self, frame: Frame, frames: usize) -> Result<(), Error> {
        if self.callback.is_some() {
            return Err(Error::InvalidState);
        }
        if frames > self.config.max_frames {
            return Err(Error::InvalidShape);
        }
        if frames == 0 {
            return Ok(());
        }
        let end = Frame(frame.0.checked_add(frames as u64).ok_or(Error::Capacity)?);
        self.callback = Some(frame);
        self.callbacks = self.callbacks.saturating_add(1);
        self.transport.begin_cycle(frame, Epoch(0));
        for _ in 0..2 {
            let Some(operation) = self.transport.next_operation() else {
                break;
            };
            let (sequence, lane, operation) = (
                operation.sequence,
                operation.lane,
                operation.request.operation,
            );
            if !self.transport.connected() {
                self.pending = None;
                self.refuse(Refusal::Disconnected { frame })?;
                continue;
            }
            match (lane, operation) {
                (Lane::Recovery, Operation::CancelPending { target }) => {
                    if target.0 >= sequence.0 {
                        self.refuse(Refusal::InvalidRecoveryTarget { target })?;
                    } else {
                        self.cancelled = Some(target);
                        self.settle(
                            Ok(Receipt::CancelRegistered { frame, target }),
                            Outcome::Applied,
                        )?;
                    }
                }
                (
                    Lane::Normal,
                    Operation::SelectPreparedPreset {
                        at,
                        preset,
                        expected_revision,
                    },
                ) => {
                    self.pending = Some(sequence);
                    if self.cancelled == Some(sequence) {
                        self.pending = None;
                        self.cancelled = None;
                        self.refuse(Refusal::Cancelled { frame })?;
                        continue;
                    }
                    let refusal = if at < frame {
                        Some(Refusal::Late {
                            requested: at,
                            decision_frame: frame,
                        })
                    } else if self.catalog.get(preset.slot() as usize) != Some(&preset) {
                        Some(Refusal::InvalidPreset { preset })
                    } else if expected_revision != self.revision {
                        Some(Refusal::StaleRevision {
                            expected: expected_revision,
                            actual: self.revision,
                        })
                    } else if self.revision == u32::MAX {
                        Some(Refusal::RevisionExhausted { frame })
                    } else {
                        None
                    };
                    if let Some(refusal) = refusal {
                        self.pending = None;
                        self.refuse(refusal)?;
                        continue;
                    }
                    if at < end {
                        self.mailbox.frame.store(at.0, Ordering::Relaxed);
                        self.mailbox.slot.store(preset.slot(), Ordering::Relaxed);
                        self.mailbox
                            .revision
                            .store(expected_revision, Ordering::Relaxed);
                        self.mailbox.armed.store(true, Ordering::Release);
                    }
                    break;
                }
                _ => self.refuse(Refusal::InvalidOperation { frame })?,
            }
        }
        Ok(())
    }

    /// Call even after processing fails; an actual published activation remains
    /// applied. The host must propagate the engine's processing result separately.
    pub fn finish_callback(&mut self, processing: Result<(), Error>) -> Result<(), Error> {
        let Some(start) = self.callback.take() else {
            return Ok(());
        };
        if self.mailbox.acknowledged.swap(false, Ordering::Acquire) {
            let frame = Frame(self.mailbox.actual_frame.load(Ordering::Relaxed));
            let previous_revision = self.mailbox.previous_revision.load(Ordering::Relaxed);
            self.revision = self.mailbox.actual_revision.load(Ordering::Relaxed);
            let preset = *self
                .catalog
                .get(self.mailbox.slot.load(Ordering::Relaxed) as usize)
                .ok_or(Error::InvalidId)?;
            self.pending = None;
            self.settle(
                Ok(Receipt::Applied {
                    requested_frame: frame,
                    frame,
                    previous_revision,
                    revision: self.revision,
                    preset,
                }),
                Outcome::Applied,
            )?;
        } else if let Some(error) = processing.err().or_else(|| {
            self.mailbox
                .armed
                .load(Ordering::Acquire)
                .then_some(Error::InvalidState)
        }) {
            self.mailbox.armed.store(false, Ordering::Release);
            if self.pending.take().is_some() {
                self.refuse(Refusal::Fault {
                    decision_frame: start,
                    error,
                })?;
            }
        }
        Ok(())
    }
}
