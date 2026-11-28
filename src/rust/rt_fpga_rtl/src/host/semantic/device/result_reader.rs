use super::*;
use crate::host::semantic::{HEADER_WORDS, MAX_RESULT_WORDS};

pub const MAX_RESULT_BUS_OPS: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResultStep {
    Idle,
    Progress,
    Buffered,
}
#[derive(Clone, Copy)]
enum ReadPhase {
    Ready,
    Health,
    Select,
    Rewind,
    Data,
    DataReceipt,
    AckLow,
    AckHigh,
    AckEpoch,
    Ack,
    AckReceipt,
}
pub(super) struct Reader {
    phase: ReadPhase,
    bank: usize,
    identity: Option<GroupIdentity>,
    cursor: usize,
    words: [u32; MAX_RESULT_WORDS],
    decoded: Option<GroupResult>,
}
impl Reader {
    fn new() -> Self {
        Self {
            phase: ReadPhase::Ready,
            bank: 0,
            identity: None,
            cursor: 0,
            words: [0; MAX_RESULT_WORDS],
            decoded: None,
        }
    }
    pub(super) fn ack_in_flight(&self, bank: usize) -> bool {
        self.bank == bank && matches!(self.phase, ReadPhase::AckReceipt)
    }
}
impl<B: RegisterBus> Device<B> {
    /// Copy/validate/ACK one reliable packet with at most three MMIO operations per step.
    pub fn result_step(&mut self) -> Result<ResultStep, Error<B::Error>> {
        if self.halted {
            return Err(Error::Fault);
        }
        if self.completed.is_some() {
            return Ok(ResultStep::Buffered);
        }
        let mut reader = self.result.take().unwrap_or_else(Reader::new);
        let result = self
            .mailbox
            .check_epoch()
            .and_then(|()| self.advance_result(&mut reader))
            .and_then(|step| {
                self.mailbox.check_epoch()?;
                Ok(step)
            });
        match result {
            Ok(ResultStep::Buffered) => {
                self.published[reader.bank] = None;
                self.completed = reader.decoded.take();
            }
            Ok(ResultStep::Idle) => {}
            Ok(ResultStep::Progress) => self.result = Some(reader),
            Err(_) => {
                self.halted = true;
                self.result = Some(reader);
            }
        }
        result
    }
    /// Owned wire observations, not a lease-retirement or application-Ready capability.
    pub fn take_result(&mut self) -> Option<GroupResult> {
        self.completed.take()
    }
    fn advance_result(&mut self, reader: &mut Reader) -> Result<ResultStep, Error<B::Error>> {
        match reader.phase {
            ReadPhase::Ready => {
                let mask = self.mailbox.read(REG_RESULT_READY)?;
                if mask & !15 != 0 || mask.count_ones() > 1 {
                    return Err(Error::UnexpectedResponse);
                }
                if mask == 0 {
                    reader.phase = ReadPhase::Health;
                    return Ok(ResultStep::Progress);
                }
                let bank = mask.trailing_zeros() as usize;
                let Some(identity) = self.published[bank] else {
                    if self.pending.as_ref().is_some_and(|pending| {
                        pending.bank == bank && matches!(pending.phase, Phase::PublishReceipt)
                    }) {
                        return Ok(ResultStep::Idle);
                    }
                    return Err(Error::UnexpectedResponse);
                };
                if self
                    .published
                    .iter()
                    .flatten()
                    .any(|earlier| earlier.sequence() < identity.sequence())
                {
                    return Err(Error::UnexpectedResponse);
                }
                reader.bank = bank;
                reader.identity = Some(identity);
                reader.phase = ReadPhase::Select;
            }
            ReadPhase::Health => {
                if self.mailbox.read(REG_STATUS)? & 4 != 0 {
                    return Err(Error::Fault);
                }
                return Ok(ResultStep::Idle);
            }
            ReadPhase::Select => {
                self.mailbox.write(REG_RESULT_BANK, reader.bank as u32)?;
                reader.phase = ReadPhase::Rewind;
            }
            ReadPhase::Rewind => {
                self.mailbox.write(REG_RESULT_REWIND, 1)?;
                reader.phase = ReadPhase::Data;
            }
            ReadPhase::Data => {
                reader.words[reader.cursor] = self.mailbox.read(REG_RESULT_DATA)?;
                reader.phase = ReadPhase::DataReceipt;
            }
            ReadPhase::DataReceipt => {
                self.result_receipt()?;
                reader.cursor += 1;
                let identity = reader.identity.ok_or(Error::UnexpectedResponse)?;
                if reader.cursor == HEADER_WORDS + identity.count() {
                    reader.decoded = Some(
                        GroupResult::decode(&reader.words[..reader.cursor], identity)
                            .map_err(|_| Error::UnexpectedResponse)?,
                    );
                    reader.phase = ReadPhase::AckLow;
                } else {
                    reader.phase = ReadPhase::Data;
                }
            }
            ReadPhase::AckLow => {
                let identity = reader.identity.ok_or(Error::UnexpectedResponse)?;
                self.mailbox
                    .write(REG_RESULT_ACK_LOW, identity.sequence() as u32)?;
                reader.phase = ReadPhase::AckHigh;
            }
            ReadPhase::AckHigh => {
                let identity = reader.identity.ok_or(Error::UnexpectedResponse)?;
                self.mailbox
                    .write(REG_RESULT_ACK_HIGH, (identity.sequence() >> 32) as u32)?;
                reader.phase = ReadPhase::AckEpoch;
            }
            ReadPhase::AckEpoch => {
                self.mailbox.write(REG_RESULT_ACK_EPOCH, self.epoch())?;
                reader.phase = ReadPhase::Ack;
            }
            ReadPhase::Ack => {
                self.mailbox.write(REG_RESULT_ACK, 1)?;
                reader.phase = ReadPhase::AckReceipt;
            }
            ReadPhase::AckReceipt => {
                self.result_receipt()?;
                if reader.decoded.is_none() {
                    return Err(Error::UnexpectedResponse);
                }
                return Ok(ResultStep::Buffered);
            }
        }
        Ok(ResultStep::Progress)
    }
    fn result_receipt(&mut self) -> Result<(), Error<B::Error>> {
        match self.mailbox.read(REG_RESULT_RECEIPT)? {
            RECEIPT_ACCEPTED => Ok(()),
            RECEIPT_EPOCH => Err(Error::EpochChanged),
            RECEIPT_FAULT => Err(Error::Fault),
            code @ (RECEIPT_FULL | RECEIPT_INVALID | RECEIPT_UNAVAILABLE | RECEIPT_OLD_SEQUENCE
            | RECEIPT_INCOMPLETE | RECEIPT_EXHAUSTED) => Err(Error::Rejected(code)),
            _ => Err(Error::UnexpectedResponse),
        }
    }
}
