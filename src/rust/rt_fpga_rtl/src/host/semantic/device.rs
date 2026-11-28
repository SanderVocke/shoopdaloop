use super::{GroupIdentity, GroupPacket, GroupResult};
mod result_reader;
mod service;
mod session;
mod snapshot_reader;
use crate::host::semantic_registers::*;
use crate::host::{Contract, Error, Geometry, Mailbox, Recovery, RecoveryState, RegisterBus};
use result_reader::Reader;
pub use result_reader::{MAX_RESULT_BUS_OPS, ResultStep};
pub use service::MAX_PREPARATION_BUS_OPS;
pub use session::{
    AllocationOwnership, PreparationProgress, ProgramRecovery, ProgramSession, SelectionStep,
};
pub use snapshot_reader::{
    MAX_SNAPSHOT_BUS_OPS, SNAPSHOT_COPY_WORDS, SnapshotBuffers, SnapshotDevice, SnapshotStep,
};

pub const MAX_PUBLICATION_BUS_OPS: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationStep {
    Idle,
    Waiting,
    Progress,
    Published {
        bank: usize,
        identity: GroupIdentity,
    },
}
#[derive(Clone, Copy)]
enum Phase {
    Health,
    Space,
    Select,
    Begin,
    BeginReceipt,
    Data,
    DataReceipt,
    Publish,
    PublishReceipt,
}
struct Publication {
    packet: GroupPacket,
    phase: Phase,
    bank: usize,
    cursor: usize,
}

/// Wire transport only; attachment/publication does not establish resource ownership.
pub struct Device<B> {
    mailbox: Mailbox<B>,
    pending: Option<Publication>,
    result: Option<Reader>,
    completed: Option<GroupResult>,
    published: [Option<GroupIdentity>; SEMANTIC_GROUP_BANKS as usize],
    next: u64,
    halted: bool,
    reset_epoch: Option<u32>,
}
impl<B: RegisterBus> Device<B> {
    pub fn open(mut bus: B, expected: Contract) -> Result<Self, Error<B::Error>> {
        if expected
            != (Contract {
                clock_hz: 100_000_000,
                arena_base: 0x3000_0000,
                arena_bytes: 0x0800_0000,
            })
        {
            return Err(Error::Incompatible);
        }
        let epoch = bus.read(REG_EPOCH).map_err(Error::Bus)?;
        let mailbox = Mailbox::open_version(bus, expected, SEMANTIC_VERSION)?;
        if epoch == 0 || epoch != mailbox.epoch {
            return Err(Error::EpochChanged);
        }
        Self::attach(mailbox)
    }
    fn attach(mut mailbox: Mailbox<B>) -> Result<Self, Error<B::Error>> {
        mailbox.check_epoch()?;
        let expected = Geometry {
            instructions: 1024,
            states: 512,
            jobs: 512,
            buffers: 512,
            views: 512,
            blocks: 31 * 1024,
            chunk_frames: 32,
            block_chunks: 32,
            quantum: 32,
        };
        if mailbox.geometry()? != expected {
            return Err(Error::Incompatible);
        }
        for (register, value) in [
            (REG_SEMANTIC_ID, SEMANTIC_ID),
            (REG_SEMANTIC_CAPABILITIES, SEMANTIC_CAPABILITIES),
            (REG_GROUP_BANKS, SEMANTIC_GROUP_BANKS),
            (REG_COMMANDS_PER_GROUP, SEMANTIC_COMMANDS_PER_GROUP),
            (REG_SNAPSHOT_LOOP_LIMIT, SEMANTIC_LOOP_LIMIT),
            (REG_OBSERVATION_ROW_WORDS, SEMANTIC_ROW_WORDS),
            (REG_COMMAND_CYCLE_LIMIT, SEMANTIC_COMMAND_CYCLES),
            (REG_SNAPSHOT_CYCLE_LIMIT, SEMANTIC_SNAPSHOT_CYCLES),
            (REG_QUANTUM_CYCLE_LIMIT, SEMANTIC_QUANTUM_CYCLES),
            (REG_RECOVERY_ID, crate::host::recovery::RECOVERY_ID),
        ] {
            if mailbox.read(register)? != value {
                return Err(Error::Incompatible);
            }
        }
        if !matches!(mailbox.read(REG_RECOVERY_STATUS)?, 0 | 3) {
            return Err(Error::Full);
        }
        if mailbox.status()? != 1
            || mailbox.read(REG_COMPLETED_QUANTA)? != 0
            || mailbox.read(REG_FREE_BANKS)? != 15
            || counter(&mut mailbox, REG_NEXT_SEQUENCE_LOW, REG_NEXT_SEQUENCE_HIGH)? != 1
            || counter(&mut mailbox, REG_LAST_ADMITTED_LOW, REG_LAST_ADMITTED_HIGH)? != 0
        {
            return Err(Error::Full);
        }
        mailbox.check_epoch()?;
        Ok(Self {
            mailbox,
            pending: None,
            result: None,
            completed: None,
            published: [None; 4],
            next: 1,
            halted: false,
            reset_epoch: None,
        })
    }
    pub fn epoch(&self) -> u32 {
        self.mailbox.epoch
    }
    /// Zero denotes exhausted publication identity, never a wrapped sequence.
    pub fn next_sequence(&self) -> u64 {
        self.next
    }
    pub fn published(&self, bank: usize) -> Option<GroupIdentity> {
        self.published.get(bank).copied().flatten()
    }
    /// Queue locally. Only Published confirms FPGA admission, not semantic success.
    pub fn queue(&mut self, packet: GroupPacket) -> Result<(), Error<B::Error>> {
        if self.halted {
            return Err(Error::Fault);
        }
        if packet.identity().epoch() != self.epoch() {
            return Err(Error::EpochChanged);
        }
        if self.next == 0 || packet.identity().sequence() != self.next {
            return Err(Error::InvalidRequest);
        }
        if self.pending.is_some() || self.published.iter().all(Option::is_some) {
            return Err(Error::Full);
        }
        self.reset_epoch = None;
        self.pending = Some(Publication {
            packet,
            phase: Phase::Health,
            bank: 0,
            cursor: 0,
        });
        Ok(())
    }
    /// At most three ordered MMIO operations, without polling for space or execution.
    /// Any uncertain outcome quarantines this transport until explicit recovery.
    pub fn publication_step(&mut self) -> Result<PublicationStep, Error<B::Error>> {
        if self.halted {
            return Err(Error::Fault);
        }
        let Some(mut pending) = self.pending.take() else {
            return Ok(PublicationStep::Idle);
        };
        let result = self
            .mailbox
            .check_epoch()
            .and_then(|()| self.advance(&mut pending))
            .and_then(|step| {
                self.mailbox.check_epoch()?;
                Ok(step)
            });
        match result {
            Ok(PublicationStep::Published { bank, identity }) => {
                self.published[bank] = Some(identity);
                self.next = self.next.checked_add(1).unwrap_or(0);
            }
            Err(_) => {
                self.halted = true;
                self.pending = Some(pending);
            }
            _ => self.pending = Some(pending),
        }
        result
    }
    fn advance(&mut self, pending: &mut Publication) -> Result<PublicationStep, Error<B::Error>> {
        match pending.phase {
            Phase::Health => {
                if self.mailbox.read(REG_STATUS)? & 4 != 0 {
                    return Err(Error::Fault);
                }
                pending.phase = Phase::Space;
            }
            Phase::Space => {
                let mut mask = self.mailbox.read(REG_FREE_BANKS)?;
                if mask & !15 != 0 {
                    return Err(Error::UnexpectedResponse);
                }
                for bank in 0..self.published.len() {
                    if mask & (1 << bank) != 0 && self.published[bank].is_some() {
                        if self
                            .result
                            .as_ref()
                            .is_some_and(|reader| reader.ack_in_flight(bank))
                        {
                            mask &= !(1 << bank);
                        } else {
                            return Err(Error::UnexpectedResponse);
                        }
                    }
                }
                if mask == 0 {
                    pending.phase = Phase::Health;
                    return Ok(PublicationStep::Waiting);
                }
                pending.bank = mask.trailing_zeros() as usize;
                if self.published[pending.bank].is_some() {
                    return Err(Error::UnexpectedResponse);
                }
                pending.phase = Phase::Select;
            }
            Phase::Select => {
                self.mailbox.write(REG_COMMAND_BANK, pending.bank as u32)?;
                pending.phase = Phase::Begin;
            }
            Phase::Begin => {
                self.mailbox.write(REG_COMMAND_BEGIN, self.epoch())?;
                pending.phase = Phase::BeginReceipt;
            }
            Phase::BeginReceipt => {
                self.receipt()?;
                pending.phase = Phase::Data;
            }
            Phase::Data => {
                self.mailbox
                    .write(REG_COMMAND_DATA, pending.packet.words()[pending.cursor])?;
                pending.phase = Phase::DataReceipt;
            }
            Phase::DataReceipt => {
                self.receipt()?;
                pending.cursor += 1;
                pending.phase = if pending.cursor == pending.packet.words().len() {
                    Phase::Publish
                } else {
                    Phase::Data
                };
            }
            Phase::Publish => {
                self.mailbox.write(REG_COMMAND_PUBLISH, self.epoch())?;
                pending.phase = Phase::PublishReceipt;
            }
            Phase::PublishReceipt => {
                self.receipt()?;
                return Ok(PublicationStep::Published {
                    bank: pending.bank,
                    identity: pending.packet.identity(),
                });
            }
        }
        Ok(PublicationStep::Progress)
    }
    fn receipt(&mut self) -> Result<(), Error<B::Error>> {
        match self.mailbox.read(REG_COMMAND_RECEIPT)? {
            RECEIPT_ACCEPTED => Ok(()),
            RECEIPT_EPOCH => Err(Error::EpochChanged),
            RECEIPT_FAULT => Err(Error::Fault),
            code @ (RECEIPT_FULL | RECEIPT_INVALID | RECEIPT_UNAVAILABLE | RECEIPT_OLD_SEQUENCE
            | RECEIPT_INCOMPLETE | RECEIPT_EXHAUSTED) => Err(Error::Rejected(code)),
            _ => Err(Error::UnexpectedResponse),
        }
    }
}
impl<B: RegisterBus + Clone> Device<B> {
    /// Consume old transport/callback authority before explicitly requesting reset.
    pub fn recovery(self) -> DeviceRecovery<B> {
        DeviceRecovery {
            recovery: self.mailbox.recovery(),
            reopened: false,
            completed: None,
        }
    }
}

/// Typed access to the shared recovery owner; no second recovery controller.
pub struct DeviceRecovery<B> {
    recovery: Recovery<B>,
    reopened: bool,
    completed: Option<u32>,
}
impl<B: RegisterBus + Clone> DeviceRecovery<B> {
    pub fn begin(&mut self) -> Result<(), Error<B::Error>> {
        if self.reopened {
            return Err(Error::InvalidRequest);
        }
        self.completed = None;
        self.recovery.begin()
    }
    pub fn poll(&mut self) -> Result<RecoveryState, Error<B::Error>> {
        if self.reopened {
            return Err(Error::InvalidRequest);
        }
        let state = self.recovery.poll()?;
        if let RecoveryState::Ready(epoch) = state {
            self.completed = Some(epoch);
        }
        Ok(state)
    }
    pub fn reopen(&mut self) -> Result<Device<B>, Error<B::Error>> {
        if self.reopened {
            return Err(Error::Full);
        }
        let mut device = Device::attach(self.recovery.reopen()?)?;
        device.reset_epoch = self.completed.filter(|&epoch| epoch == device.epoch());
        self.reopened = true;
        Ok(device)
    }
}
fn counter<B: RegisterBus>(
    mailbox: &mut Mailbox<B>,
    low: u32,
    high: u32,
) -> Result<u64, Error<B::Error>> {
    let first = mailbox.read(high)?;
    let value = mailbox.read(low)?;
    if mailbox.read(high)? != first {
        return Err(Error::UnexpectedResponse);
    }
    Ok(u64::from(value) | (u64::from(first) << 32))
}
