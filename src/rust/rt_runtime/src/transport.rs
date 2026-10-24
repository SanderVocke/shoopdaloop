//! Credit-bounded single-producer/single-consumer ownership transport.
//!
//! Each admitted operation reserves one completion/retirement slot until the
//! control side receives its terminal result. An operation may return one owned
//! retirement bundle; a bundle can contain multiple preallocated objects.
//! The render endpoint must only be destroyed after callbacks have stopped.

use crate::{Epoch, Error, Frame, Sequence};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportConfig {
    pub normal_slots: usize,
    pub recovery_slots: usize,
    pub telemetry_slots: usize,
    pub operations_per_cycle: usize,
    pub recovery_per_cycle: usize,
    /// Separate caps for terminal completions and optional telemetry, so filling
    /// telemetry cannot steal a completion's reserved publication budget.
    pub publications_per_cycle: usize,
}

impl TransportConfig {
    fn validate(self) -> Result<(), Error> {
        let values = [
            self.normal_slots,
            self.recovery_slots,
            self.telemetry_slots,
            self.operations_per_cycle,
            self.recovery_per_cycle,
            self.publications_per_cycle,
        ];
        if values.contains(&0) {
            return Err(Error::InvalidConfig);
        }
        if values.iter().any(|&n| n > i32::MAX as usize)
            || self.normal_slots.checked_add(self.recovery_slots).is_none()
        {
            return Err(Error::Capacity);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lane {
    Normal,
    Recovery,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Applied,
    Scheduled {
        effective_frame: Frame,
        late_by: u64,
    },
    Rejected(Error),
    Cancelled,
}

/// Cancellation is best effort until render dequeue observes the flag. A request
/// racing that observation may be too late; only the terminal completion decides.
#[derive(Clone, Debug)]
pub struct Ticket {
    sequence: Sequence,
    cancelled: Arc<AtomicBool>,
}

impl Ticket {
    pub fn sequence(&self) -> Sequence {
        self.sequence
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[derive(Debug)]
pub struct SubmitError<T> {
    pub error: Error,
    pub value: T,
}

/// The owning operation and displaced objects are returned together, never freed
/// by the callback. Receiving this result releases the transport admission credit.
pub struct Completion<T, R> {
    pub sequence: Sequence,
    pub lane: Lane,
    pub outcome: Outcome,
    pub frame: Frame,
    pub epoch: Epoch,
    pub request: T,
    pub retired: Option<R>,
    cancelled: Arc<AtomicBool>,
}

pub struct Operation<'a, T> {
    pub sequence: Sequence,
    pub lane: Lane,
    pub request: &'a mut T,
}

#[derive(Debug)]
pub struct CompleteError<R> {
    pub error: Error,
    pub retired: Option<R>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkCounters {
    pub operations: usize,
    pub completions: usize,
    pub telemetry: usize,
    pub telemetry_refused: usize,
}

pub struct ControlEndpoint<T, R, S> {
    normal: Producer<Completion<T, R>>,
    recovery: Producer<Completion<T, R>>,
    returns: Consumer<Completion<T, R>>,
    telemetry: Consumer<S>,
    config: TransportConfig,
    normal_outstanding: usize,
    recovery_outstanding: usize,
    next_sequence: Sequence,
}

pub struct RenderEndpoint<T, R, S> {
    normal: Consumer<Completion<T, R>>,
    recovery: Consumer<Completion<T, R>>,
    returns: Producer<Completion<T, R>>,
    telemetry: Producer<S>,
    config: TransportConfig,
    current: Option<Completion<T, R>>,
    pending_return: Option<Completion<T, R>>,
    normal_remaining: usize,
    recovery_remaining: usize,
    frame: Frame,
    epoch: Epoch,
    counters: WorkCounters,
}

/// Allocates queue storage outside rendering. Recovery has independent admission
/// credits and is serviced first; applications use it only for bounded release or
/// recovery operations, never for operations requiring normal-lane FIFO ordering.
pub fn channel<T, R, S: Copy>(
    config: TransportConfig,
) -> Result<(ControlEndpoint<T, R, S>, RenderEndpoint<T, R, S>), Error> {
    config.validate()?;
    let total = config.normal_slots + config.recovery_slots;
    // RingBuffer allocates capacity * size_of::<T>(); reject invalid byte layouts
    // consistently on native and wasm32 before entering the allocator.
    for (count, size) in [
        (total, size_of::<Completion<T, R>>()),
        (config.telemetry_slots, size_of::<S>()),
    ] {
        if count
            .checked_mul(size)
            .is_none_or(|n| n > i32::MAX as usize)
        {
            return Err(Error::Capacity);
        }
    }
    let (normal_tx, normal_rx) = RingBuffer::new(config.normal_slots);
    let (recovery_tx, recovery_rx) = RingBuffer::new(config.recovery_slots);
    let (return_tx, return_rx) = RingBuffer::new(total);
    let (telemetry_tx, telemetry_rx) = RingBuffer::new(config.telemetry_slots);
    Ok((
        ControlEndpoint {
            normal: normal_tx,
            recovery: recovery_tx,
            returns: return_rx,
            telemetry: telemetry_rx,
            config,
            normal_outstanding: 0,
            recovery_outstanding: 0,
            next_sequence: Sequence(1),
        },
        RenderEndpoint {
            normal: normal_rx,
            recovery: recovery_rx,
            returns: return_tx,
            telemetry: telemetry_tx,
            config,
            current: None,
            pending_return: None,
            normal_remaining: 0,
            recovery_remaining: 0,
            frame: Frame(0),
            epoch: Epoch(0),
            counters: WorkCounters::default(),
        },
    ))
}

impl<T, R, S: Copy> ControlEndpoint<T, R, S> {
    /// Control-side only: allocates a cancellation flag after checking admission.
    pub fn submit(&mut self, lane: Lane, request: T) -> Result<Ticket, SubmitError<T>> {
        if self.normal.is_abandoned() {
            return Err(SubmitError {
                error: Error::Disconnected,
                value: request,
            });
        }
        let (outstanding, capacity) = match lane {
            Lane::Normal => (self.normal_outstanding, self.config.normal_slots),
            Lane::Recovery => (self.recovery_outstanding, self.config.recovery_slots),
        };
        if outstanding == capacity {
            return Err(SubmitError {
                error: Error::Full,
                value: request,
            });
        }
        let Ok(next) = self.next_sequence.checked_next() else {
            return Err(SubmitError {
                error: Error::Exhausted,
                value: request,
            });
        };
        let ticket = Ticket {
            sequence: self.next_sequence,
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        let packet = Completion {
            sequence: ticket.sequence,
            lane,
            outcome: Outcome::Rejected(Error::InvalidState),
            frame: Frame(0),
            epoch: Epoch(0),
            request,
            retired: None,
            cancelled: Arc::clone(&ticket.cancelled),
        };
        let queue = match lane {
            Lane::Normal => &mut self.normal,
            Lane::Recovery => &mut self.recovery,
        };
        if let Err(PushError::Full(packet)) = queue.push(packet) {
            return Err(SubmitError {
                error: Error::Full,
                value: packet.request,
            });
        }
        match lane {
            Lane::Normal => self.normal_outstanding += 1,
            Lane::Recovery => self.recovery_outstanding += 1,
        }
        self.next_sequence = next;
        Ok(ticket)
    }

    pub fn try_completion(&mut self) -> Option<Completion<T, R>> {
        let packet = self.returns.pop().ok()?;
        match packet.lane {
            Lane::Normal => self.normal_outstanding -= 1,
            Lane::Recovery => self.recovery_outstanding -= 1,
        }
        Some(packet)
    }

    pub fn try_telemetry(&mut self) -> Option<S> {
        self.telemetry.pop().ok()
    }

    pub fn outstanding(&self) -> usize {
        self.normal_outstanding + self.recovery_outstanding
    }

    pub fn connected(&self) -> bool {
        !self.normal.is_abandoned()
    }
}

impl<T, R, S: Copy> RenderEndpoint<T, R, S> {
    /// Snapshots available work; concurrently arriving work waits until the next
    /// cycle. Call exactly once per host callback, including callbacks that only
    /// stage input. A current long-lived operation keeps its owning return credit.
    pub fn begin_cycle(&mut self, frame: Frame, epoch: Epoch) {
        self.frame = frame;
        self.epoch = epoch;
        self.counters = WorkCounters::default();
        self.normal_remaining = self.normal.slots().min(self.config.operations_per_cycle);
        self.recovery_remaining = self.recovery.slots().min(self.config.recovery_per_cycle);
        self.flush_return();
    }

    fn flush_return(&mut self) {
        if let Some(packet) = self.pending_return.take() {
            if let Err(PushError::Full(packet)) = self.returns.push(packet) {
                // Defensive retention preserves ownership even if credit accounting
                // is changed incorrectly. No retry loop or callback destruction.
                self.pending_return = Some(packet);
            }
        }
    }

    /// At most one operation can be borrowed at a time. Cancelled dequeues consume
    /// the same work/completion budget as normal operations and are returned here.
    pub fn next_operation(&mut self) -> Option<Operation<'_, T>> {
        if self.pending_return.is_some() {
            return None;
        }
        if self.current.is_none() {
            while self.counters.operations < self.config.operations_per_cycle
                && self.counters.completions < self.config.publications_per_cycle
            {
                let packet = if self.recovery_remaining > 0 {
                    self.recovery_remaining -= 1;
                    self.recovery.pop().ok()
                } else if self.normal_remaining > 0 {
                    self.normal_remaining -= 1;
                    self.normal.pop().ok()
                } else {
                    return None;
                };
                let Some(mut packet) = packet else {
                    return None;
                };
                self.counters.operations += 1;
                if packet.cancelled.load(Ordering::Acquire) {
                    packet.outcome = Outcome::Cancelled;
                    packet.frame = self.frame;
                    packet.epoch = self.epoch;
                    self.counters.completions += 1;
                    self.pending_return = Some(packet);
                    self.flush_return();
                    if self.pending_return.is_some() {
                        return None;
                    }
                } else {
                    self.current = Some(packet);
                    break;
                }
            }
        }
        self.current.as_mut().map(|packet| Operation {
            sequence: packet.sequence,
            lane: packet.lane,
            request: &mut packet.request,
        })
    }

    /// Returns ownership of `retired` on API misuse or budget exhaustion. The
    /// caller must retain that error payload rather than dropping it on RT.
    pub fn complete(
        &mut self,
        outcome: Outcome,
        retired: Option<R>,
    ) -> Result<(), CompleteError<R>> {
        if self.pending_return.is_some()
            || self.counters.completions == self.config.publications_per_cycle
        {
            return Err(CompleteError {
                error: Error::Full,
                retired,
            });
        }
        let Some(mut packet) = self.current.take() else {
            return Err(CompleteError {
                error: Error::InvalidState,
                retired,
            });
        };
        packet.outcome = outcome;
        packet.frame = self.frame;
        packet.epoch = self.epoch;
        packet.retired = retired;
        self.counters.completions += 1;
        self.pending_return = Some(packet);
        self.flush_return();
        Ok(())
    }

    /// Copy-only diagnostics cannot own render resources. Publication refuses
    /// rather than growing or waiting; the caller can retry its latest state.
    pub fn publish(&mut self, state: S) -> Result<(), Error> {
        if self.counters.telemetry >= self.config.publications_per_cycle {
            self.counters.telemetry_refused = self.counters.telemetry_refused.saturating_add(1);
            return Err(Error::Full);
        }
        match self.telemetry.push(state) {
            Ok(()) => {
                self.counters.telemetry += 1;
                Ok(())
            }
            Err(_) => {
                self.counters.telemetry_refused = self.counters.telemetry_refused.saturating_add(1);
                Err(Error::Full)
            }
        }
    }

    /// Update acknowledgement metadata after a transaction activates; this does
    /// not replenish callback budgets or change the ingress snapshot.
    pub fn set_epoch(&mut self, epoch: Epoch) {
        self.epoch = epoch;
    }

    pub fn counters(&self) -> WorkCounters {
        self.counters
    }
    pub fn connected(&self) -> bool {
        !self.normal.is_abandoned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn sequence_exhaustion_never_reuses_an_admitted_sequence() {
        let config = TransportConfig {
            normal_slots: 1,
            recovery_slots: 1,
            telemetry_slots: 1,
            operations_per_cycle: 1,
            recovery_per_cycle: 1,
            publications_per_cycle: 1,
        };
        let (mut control, mut render) = channel::<u8, (), u8>(config).unwrap();
        control.next_sequence = Sequence(u64::MAX - 1);
        let ticket = control.submit(Lane::Normal, 1).unwrap();
        assert_eq!(ticket.sequence(), Sequence(u64::MAX - 1));
        render.begin_cycle(Frame(0), Epoch(0));
        render.next_operation().unwrap();
        render.complete(Outcome::Applied, None).unwrap();
        ticket.cancel();
        assert_eq!(control.try_completion().unwrap().outcome, Outcome::Applied);
        let refused = control.submit(Lane::Normal, 2).unwrap_err();
        assert_eq!(refused.error, Error::Exhausted);
        assert_eq!(refused.value, 2);
        assert_eq!(control.outstanding(), 0);
    }
}
