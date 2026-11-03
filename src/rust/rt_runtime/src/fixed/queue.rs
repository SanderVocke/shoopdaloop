//! Copy-valued commands with terminal credit reserved through result consumption.
use super::Quantum;
use crate::{Error, Frame};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use rtrb::{Consumer, Producer, RingBuffer};

static NEXT_SCOPE: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ticket {
    scope: u32,
    sequence: u64,
    recovery: bool,
}
impl Ticket {
    pub fn sequence(self) -> u64 {
        self.sequence
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Receipt<C, R> {
    pub ticket: Ticket,
    pub command: Option<C>,
    pub requested: Frame,
    pub effective: Option<Frame>,
    pub actual: Frame,
    pub result: Result<R, Error>,
}
#[derive(Clone, Copy)]
struct Command<C> {
    ticket: Ticket,
    requested: Frame,
    value: C,
}
#[derive(Clone, Copy)]
struct Pending<C> {
    command: Command<C>,
    effective: Frame,
}
#[derive(Clone, Copy)]
struct Cancel {
    ticket: Ticket,
    target: Ticket,
}

pub struct CommandControl<C, R> {
    commands: Producer<Command<C>>,
    cancels: Producer<Cancel>,
    results: Consumer<Receipt<C, R>>,
    scope: u32,
    sequence: u64,
    capacity: usize,
    outstanding: usize,
    recovery: bool,
}
pub struct CommandQueue<C, R> {
    commands: Consumer<Command<C>>,
    cancels: Consumer<Cancel>,
    results: Producer<Receipt<C, R>>,
    pending: Vec<Pending<C>>,
    quantum: Quantum,
    next: Frame,
}

pub fn command_channel<C: Copy + Default + Send, R: Copy + Default + Send>(
    quantum: Quantum,
    capacity: usize,
) -> Result<(CommandControl<C, R>, CommandQueue<C, R>), Error> {
    if capacity == 0 || capacity >= i32::MAX as usize {
        return Err(Error::InvalidConfig);
    }
    let scope = loop {
        let current = NEXT_SCOPE.load(Ordering::Relaxed);
        let next = current.checked_add(1).ok_or(Error::Exhausted)?;
        if NEXT_SCOPE
            .compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            break current;
        }
    };
    for (count, bytes) in [
        (capacity, size_of::<Pending<C>>()),
        (capacity + 1, size_of::<Receipt<C, R>>()),
    ] {
        if count
            .checked_mul(bytes)
            .is_none_or(|n| n > i32::MAX as usize)
        {
            return Err(Error::Capacity);
        }
    }
    let (mut commands_tx, mut commands_rx) = RingBuffer::new(capacity);
    let (mut cancels_tx, mut cancels_rx) = RingBuffer::new(1);
    let (mut results_tx, mut results_rx) = RingBuffer::new(capacity + 1);
    let ticket = Ticket {
        scope,
        sequence: 0,
        recovery: false,
    };
    let dummy = Command {
        ticket,
        requested: Frame(0),
        value: C::default(),
    };
    warm(&mut commands_tx, &mut commands_rx, dummy, capacity)?;
    warm(
        &mut cancels_tx,
        &mut cancels_rx,
        Cancel {
            ticket,
            target: ticket,
        },
        1,
    )?;
    warm(
        &mut results_tx,
        &mut results_rx,
        Receipt {
            ticket,
            command: Some(dummy.value),
            requested: Frame(0),
            effective: None,
            actual: Frame(0),
            result: Err(Error::InvalidState),
        },
        capacity + 1,
    )?;
    let mut pending = alloc::vec![Pending {command:dummy,effective:Frame(0)};capacity];
    for entry in &mut pending {
        *core::hint::black_box(entry) = Pending {
            command: dummy,
            effective: Frame(0),
        };
    }
    pending.clear();
    Ok((
        CommandControl {
            commands: commands_tx,
            cancels: cancels_tx,
            results: results_rx,
            scope,
            sequence: 0,
            capacity,
            outstanding: 0,
            recovery: false,
        },
        CommandQueue {
            commands: commands_rx,
            cancels: cancels_rx,
            results: results_tx,
            pending,
            quantum,
            next: Frame(0),
        },
    ))
}
impl<C: Copy, R: Copy> CommandControl<C, R> {
    fn ticket(&mut self, recovery: bool) -> Result<Ticket, Error> {
        let next = self.sequence.checked_add(1).ok_or(Error::Exhausted)?;
        let ticket = Ticket {
            scope: self.scope,
            sequence: self.sequence,
            recovery,
        };
        self.sequence = next;
        Ok(ticket)
    }
    pub fn submit(&mut self, requested: Frame, value: C) -> Result<Ticket, Error> {
        if self.commands.is_abandoned() {
            return Err(Error::Disconnected);
        }
        if self.outstanding == self.capacity {
            return Err(Error::Full);
        }
        let ticket = self.ticket(false)?;
        self.commands
            .push(Command {
                ticket,
                requested,
                value,
            })
            .map_err(|_| Error::Full)?;
        self.outstanding += 1;
        Ok(ticket)
    }
    /// One independent cancellation credit remains available with normal work full.
    pub fn cancel(&mut self, target: Ticket) -> Result<Ticket, Error> {
        if self.cancels.is_abandoned() {
            return Err(Error::Disconnected);
        }
        if target.scope != self.scope || target.recovery {
            return Err(Error::InvalidId);
        }
        if self.recovery {
            return Err(Error::Full);
        }
        let ticket = self.ticket(true)?;
        self.cancels
            .push(Cancel { ticket, target })
            .map_err(|_| Error::Full)?;
        self.recovery = true;
        Ok(ticket)
    }
    pub fn try_receipt(&mut self) -> Option<Receipt<C, R>> {
        let result = self.results.pop().ok()?;
        if result.ticket.recovery {
            self.recovery = false
        } else {
            self.outstanding -= 1
        }
        Some(result)
    }
    pub fn outstanding(&self) -> usize {
        self.outstanding + usize::from(self.recovery)
    }
}
impl<C: Copy, R: Copy + Default> CommandQueue<C, R> {
    pub fn pending(&self) -> usize {
        self.pending.len()
    }
    fn emit(&mut self, value: Receipt<C, R>) -> Result<(), Error> {
        self.results.push(value).map_err(|_| Error::Full)
    }
    /// Ingress is snapshotted once per host callback, including callbacks without a tick.
    pub fn begin_callback(&mut self, next: Frame) -> Result<(), Error> {
        if next != self.next || !self.quantum.aligned(next) {
            return Err(Error::InvalidFrame);
        }
        // Visible cancellations must include their preceding normal submissions.
        let cancels = self.cancels.slots();
        let commands = self.commands.slots();
        for _ in 0..commands {
            let command = self.commands.pop().map_err(|_| Error::InvalidState)?;
            match self.quantum.effective(command.requested, next) {
                Ok(effective) => {
                    if self.pending.len() == self.pending.capacity() {
                        return Err(Error::Capacity);
                    }
                    let at = self.pending.partition_point(|old| {
                        (old.effective, old.command.ticket.sequence)
                            <= (effective, command.ticket.sequence)
                    });
                    self.pending.insert(at, Pending { command, effective });
                }
                Err(e) => self.emit(Receipt {
                    ticket: command.ticket,
                    command: Some(command.value),
                    requested: command.requested,
                    effective: None,
                    actual: next,
                    result: Err(e),
                })?,
            }
        }
        for _ in 0..cancels {
            let cancel = self.cancels.pop().map_err(|_| Error::InvalidState)?;
            let result = if let Some(index) = self
                .pending
                .iter()
                .position(|p| p.command.ticket == cancel.target)
            {
                let p = self.pending.remove(index);
                self.emit(Receipt {
                    ticket: p.command.ticket,
                    command: Some(p.command.value),
                    requested: p.command.requested,
                    effective: Some(p.effective),
                    actual: next,
                    result: Err(Error::Cancelled),
                })?;
                Ok(R::default())
            } else {
                Err(Error::InvalidState)
            };
            self.emit(Receipt {
                ticket: cancel.ticket,
                command: None,
                requested: next,
                effective: Some(next),
                actual: next,
                result,
            })?;
        }
        Ok(())
    }
    /// Equal-tick commands execute in submission order; handlers must not allocate.
    pub fn dispatch(
        &mut self,
        frame: Frame,
        mut apply: impl FnMut(C) -> Result<R, Error>,
    ) -> Result<(), Error> {
        if frame != self.next {
            return Err(Error::InvalidFrame);
        }
        let next = frame.checked_add(self.quantum.frames())?;
        while self.pending.first().is_some_and(|p| p.effective <= frame) {
            let p = self.pending.remove(0);
            let result = apply(p.command.value);
            self.emit(Receipt {
                ticket: p.command.ticket,
                command: Some(p.command.value),
                requested: p.command.requested,
                effective: Some(p.effective),
                actual: frame,
                result,
            })?;
        }
        self.next = next;
        Ok(())
    }
}

fn warm<T: Copy>(
    tx: &mut Producer<T>,
    rx: &mut Consumer<T>,
    value: T,
    count: usize,
) -> Result<(), Error> {
    for _ in 0..count {
        tx.push(value).map_err(|_| Error::InvalidState)?;
    }
    for _ in 0..count {
        rx.pop().map_err(|_| Error::InvalidState)?;
    }
    Ok(())
}
