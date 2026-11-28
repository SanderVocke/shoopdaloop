use super::{Device, Error, RegisterBus};
use crate::host::semantic_registers::*;

pub const MAX_PREPARATION_BUS_OPS: usize = 3;

#[derive(Clone, Copy)]
enum Phase {
    Space,
    Fill,
    FillReceipt,
    Push,
    PushReceipt,
    Wait,
    Tag,
    Code,
    Data,
    Pop,
    PopReceipt,
    Done,
}

pub(super) struct Service {
    operation: u32,
    words: [u32; 12],
    tag: u32,
    phase: Phase,
    cursor: usize,
}
impl Service {
    pub(super) fn new(operation: u32, words: [u32; 12], tag: u32) -> Self {
        Self {
            operation,
            words,
            tag,
            phase: Phase::Space,
            cursor: 0,
        }
    }
    pub(super) fn step<B: RegisterBus>(
        &mut self,
        device: &mut Device<B>,
    ) -> Result<bool, Error<B::Error>> {
        if device.halted {
            return Err(Error::Fault);
        }
        let result = device
            .mailbox
            .check_epoch()
            .and_then(|()| self.advance(device))
            .and_then(|done| {
                device.mailbox.check_epoch()?;
                Ok(done)
            });
        if result.is_err() {
            device.halted = true;
        }
        result
    }
    fn receipt<B: RegisterBus>(device: &mut Device<B>) -> Result<(), Error<B::Error>> {
        match device.mailbox.read(REG_SUBMISSION_RESULT)? {
            0 => Ok(()),
            2 => Err(Error::EpochChanged),
            3 => Err(Error::Fault),
            code @ 1..=7 => Err(Error::Rejected(code)),
            _ => Err(Error::UnexpectedResponse),
        }
    }
    fn advance<B: RegisterBus>(&mut self, device: &mut Device<B>) -> Result<bool, Error<B::Error>> {
        match self.phase {
            Phase::Space => {
                let status = device.mailbox.read(REG_STATUS)?;
                if status & !7 != 0 || status & 2 != 0 {
                    return Err(Error::UnexpectedResponse);
                }
                if status & 4 != 0 {
                    return Err(Error::Fault);
                }
                if status & 1 != 0 {
                    self.phase = Phase::Fill;
                }
            }
            Phase::Fill => {
                let word = match self.cursor {
                    0 => device.epoch(),
                    1 => self.operation,
                    i => self.words[i - 2],
                };
                device
                    .mailbox
                    .write(REG_REQUEST_EPOCH + 4 * self.cursor as u32, word)?;
                self.phase = Phase::FillReceipt;
            }
            Phase::FillReceipt => {
                Self::receipt(device)?;
                self.cursor += 1;
                self.phase = if self.cursor == 14 {
                    Phase::Push
                } else {
                    Phase::Fill
                };
            }
            Phase::Push => {
                device.mailbox.write(REG_REQUEST_PUSH, self.tag)?;
                self.phase = Phase::PushReceipt;
            }
            Phase::PushReceipt => {
                Self::receipt(device)?;
                self.phase = Phase::Wait;
            }
            Phase::Wait => {
                let status = device.mailbox.read(REG_STATUS)?;
                if status & !7 != 0 {
                    return Err(Error::UnexpectedResponse);
                }
                if status & 4 != 0 {
                    return Err(Error::Fault);
                }
                if status & 2 != 0 {
                    self.phase = Phase::Tag;
                }
            }
            Phase::Tag => {
                if device.mailbox.read(REG_RESPONSE_TAG)? != self.tag {
                    return Err(Error::UnexpectedResponse);
                }
                self.phase = Phase::Code;
            }
            Phase::Code => {
                match device.mailbox.read(REG_RESPONSE_CODE)? {
                    0 => {}
                    3 => return Err(Error::Fault),
                    code => return Err(Error::Rejected(code)),
                }
                self.cursor = 0;
                self.phase = Phase::Data;
            }
            Phase::Data => {
                let value = device
                    .mailbox
                    .read(REG_RESPONSE_DATA + 4 * self.cursor as u32)?;
                let reserved = match self.operation {
                    SEMANTIC_SERVICE_CORE_CONFIG => {
                        self.cursor >= 5 || (self.cursor == 4 && value > 1)
                    }
                    SEMANTIC_SERVICE_AUDIO_CONFIG => self.cursor != 0,
                    _ => true,
                };
                if reserved && value != 0 {
                    return Err(Error::UnexpectedResponse);
                }
                self.cursor += 1;
                if self.cursor == 12 {
                    self.phase = Phase::Pop;
                }
            }
            Phase::Pop => {
                device.mailbox.write(REG_RESPONSE_POP, self.tag)?;
                self.phase = Phase::PopReceipt;
            }
            Phase::PopReceipt => {
                Self::receipt(device)?;
                self.phase = Phase::Done;
            }
            Phase::Done => return Ok(true),
        }
        Ok(matches!(self.phase, Phase::Done))
    }
}
