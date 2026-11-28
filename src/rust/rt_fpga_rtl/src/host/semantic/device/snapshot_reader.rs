use super::*;
use crate::host::semantic::{
    CodecError, HEADER_WORDS, ROW_WORDS, Snapshot, SnapshotFormat, SnapshotHeader,
};

pub const MAX_SNAPSHOT_BUS_OPS: usize = 3;
pub const SNAPSHOT_COPY_WORDS: usize = HEADER_WORDS + SEMANTIC_LOOP_LIMIT as usize * ROW_WORDS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotStep {
    Idle,
    Progress,
    Updated(SnapshotHeader),
}
#[derive(Clone, Copy)]
enum CopyPhase {
    Ready,
    Health,
    HeaderSelect,
    HeaderRewind,
    HeaderData,
    HeaderReceipt,
    BodySelect,
    BodyRewind,
    BodyData,
    BodyReceipt,
    AckSelect,
    AckLow,
    AckHigh,
    AckEpoch,
    Ack,
    AckReceipt,
}

/// Caller-owned copy storage, not device, membership or resource-ownership authority.
pub struct SnapshotBuffers<'a> {
    buffers: [&'a mut [u32]; 2],
    latest: usize,
    latest_header: Option<SnapshotHeader>,
    epoch: u32,
    poisoned: bool,
    phase: CopyPhase,
    mask: u32,
    bank: usize,
    selected: usize,
    cursor: usize,
    scan: [u32; HEADER_WORDS],
    headers: [Option<SnapshotHeader>; 2],
    ack_sequence: u64,
    discard: bool,
}
impl<'a> SnapshotBuffers<'a> {
    pub fn new(first: &'a mut [u32], second: &'a mut [u32]) -> Result<Self, CodecError> {
        if first.len() != SNAPSHOT_COPY_WORDS || second.len() != SNAPSHOT_COPY_WORDS {
            return Err(CodecError::Length);
        }
        Ok(Self {
            buffers: [first, second],
            latest: 0,
            latest_header: None,
            epoch: 0,
            poisoned: false,
            phase: CopyPhase::Ready,
            mask: 0,
            bank: 0,
            selected: 0,
            cursor: 0,
            scan: [0; HEADER_WORDS],
            headers: [None; 2],
            ack_sequence: 0,
            discard: false,
        })
    }
    /// Last validated local image, possibly stale; its epoch/revision do not grant membership.
    fn latest(&self) -> Option<Snapshot<'_>> {
        self.latest_header?;
        Snapshot::decode(self.buffers[self.latest], self.format().ok()?).ok()
    }
    fn format(&self) -> Result<SnapshotFormat, CodecError> {
        SnapshotFormat::new(self.epoch, 32, SEMANTIC_LOOP_LIMIT as usize)
    }
    fn start_ack(&mut self, bank: usize, discard: bool) -> Result<(), CodecError> {
        self.bank = bank;
        self.ack_sequence = self.headers[bank].ok_or(CodecError::Identity)?.sequence;
        self.discard = discard;
        self.phase = CopyPhase::AckSelect;
        Ok(())
    }
}
/// Buffer attachment to the same transport owner, with no second bus/controller.
pub struct SnapshotDevice<'a, B> {
    device: Device<B>,
    buffers: SnapshotBuffers<'a>,
}
impl<B: RegisterBus> SnapshotDevice<'_, B> {
    pub fn epoch(&self) -> u32 {
        self.device.epoch()
    }
    pub fn next_sequence(&self) -> u64 {
        self.device.next_sequence()
    }
    pub fn published(&self, bank: usize) -> Option<GroupIdentity> {
        self.device.published(bank)
    }
    pub fn queue(&mut self, packet: GroupPacket) -> Result<(), Error<B::Error>> {
        self.device.queue(packet)
    }
    pub fn publication_step(&mut self) -> Result<PublicationStep, Error<B::Error>> {
        self.device.publication_step()
    }
    pub fn result_step(&mut self) -> Result<ResultStep, Error<B::Error>> {
        self.device.result_step()
    }
    pub fn take_result(&mut self) -> Option<GroupResult> {
        self.device.take_result()
    }
    pub fn latest_snapshot(&self) -> Option<Snapshot<'_>> {
        self.buffers.latest()
    }
    /// Latest complete as of one finite header scan; no execution/space wait loop.
    pub fn snapshot_step(&mut self) -> Result<SnapshotStep, Error<B::Error>> {
        self.device.pump_snapshot(&mut self.buffers)
    }
}
impl<B: RegisterBus + Clone> SnapshotDevice<'_, B> {
    pub fn recovery(self) -> DeviceRecovery<B> {
        self.device.recovery()
    }
}
impl<B: RegisterBus> Device<B> {
    pub fn with_snapshots<'a>(self, buffers: SnapshotBuffers<'a>) -> SnapshotDevice<'a, B> {
        SnapshotDevice {
            device: self,
            buffers,
        }
    }
    fn pump_snapshot(
        &mut self,
        copy: &mut SnapshotBuffers<'_>,
    ) -> Result<SnapshotStep, Error<B::Error>> {
        if self.halted {
            return Err(Error::Fault);
        }
        if copy.epoch != 0 && copy.epoch != self.epoch() {
            copy.poisoned = true;
            copy.latest_header = None;
            return Err(Error::EpochChanged);
        }
        if copy.poisoned {
            return Err(Error::InvalidRequest);
        }
        copy.epoch = self.epoch();
        let result = self
            .mailbox
            .check_epoch()
            .and_then(|()| self.advance_snapshot(copy))
            .and_then(|step| {
                self.mailbox.check_epoch()?;
                Ok(step)
            });
        match result {
            Ok(SnapshotStep::Updated(header)) => {
                copy.latest ^= 1;
                copy.latest_header = Some(header);
                copy.phase = CopyPhase::Ready;
            }
            Err(_) => {
                self.halted = true;
                copy.poisoned = true;
                if matches!(result, Err(Error::EpochChanged)) {
                    copy.latest_header = None;
                }
            }
            _ => {}
        }
        result
    }
    fn advance_snapshot(
        &mut self,
        copy: &mut SnapshotBuffers<'_>,
    ) -> Result<SnapshotStep, Error<B::Error>> {
        match copy.phase {
            CopyPhase::Ready => {
                copy.mask = self.mailbox.read(REG_SNAPSHOT_READY)?;
                if copy.mask & !3 != 0 {
                    return Err(Error::UnexpectedResponse);
                }
                if copy.mask == 0 {
                    copy.phase = CopyPhase::Health;
                } else {
                    copy.headers = [None; 2];
                    copy.bank = copy.mask.trailing_zeros() as usize;
                    copy.cursor = 0;
                    copy.phase = CopyPhase::HeaderSelect;
                }
            }
            CopyPhase::Health => {
                if self.mailbox.read(REG_STATUS)? & 4 != 0 {
                    return Err(Error::Fault);
                }
                copy.phase = CopyPhase::Ready;
                return Ok(SnapshotStep::Idle);
            }
            CopyPhase::HeaderSelect => {
                self.mailbox.write(REG_SNAPSHOT_BANK, copy.bank as u32)?;
                copy.phase = CopyPhase::HeaderRewind;
            }
            CopyPhase::HeaderRewind => {
                self.mailbox.write(REG_SNAPSHOT_REWIND, 1)?;
                copy.phase = CopyPhase::HeaderData;
            }
            CopyPhase::HeaderData => {
                copy.scan[copy.cursor] = self.mailbox.read(REG_SNAPSHOT_DATA)?;
                copy.phase = CopyPhase::HeaderReceipt;
            }
            CopyPhase::HeaderReceipt => {
                self.snapshot_receipt()?;
                copy.cursor += 1;
                if copy.cursor == HEADER_WORDS {
                    let header = SnapshotHeader::decode(
                        &copy.scan,
                        copy.format().map_err(|_| Error::UnexpectedResponse)?,
                    )
                    .map_err(|_| Error::UnexpectedResponse)?;
                    copy.headers[copy.bank] = Some(header);
                    let other = copy.bank ^ 1;
                    if copy.mask & (1 << other) != 0 && copy.headers[other].is_none() {
                        copy.bank = other;
                        copy.cursor = 0;
                        copy.phase = CopyPhase::HeaderSelect;
                    } else {
                        copy.selected = copy.bank;
                        if let Some(other_header) = copy.headers[other] {
                            if other_header.sequence == header.sequence {
                                return Err(Error::UnexpectedResponse);
                            }
                            if other_header.sequence > header.sequence {
                                copy.selected = other;
                            }
                        }
                        let selected =
                            copy.headers[copy.selected].ok_or(Error::UnexpectedResponse)?;
                        if copy
                            .latest_header
                            .is_some_and(|previous| previous.sequence >= selected.sequence)
                        {
                            return Err(Error::UnexpectedResponse);
                        }
                        if copy.mask == 3 {
                            copy.start_ack(copy.selected ^ 1, true)
                                .map_err(|_| Error::UnexpectedResponse)?;
                        } else {
                            copy.phase = CopyPhase::BodySelect;
                        }
                    }
                } else {
                    copy.phase = CopyPhase::HeaderData;
                }
            }
            CopyPhase::BodySelect => {
                self.mailbox
                    .write(REG_SNAPSHOT_BANK, copy.selected as u32)?;
                copy.cursor = 0;
                copy.phase = CopyPhase::BodyRewind;
            }
            CopyPhase::BodyRewind => {
                self.mailbox.write(REG_SNAPSHOT_REWIND, 1)?;
                copy.phase = CopyPhase::BodyData;
            }
            CopyPhase::BodyData => {
                copy.buffers[copy.latest ^ 1][copy.cursor] =
                    self.mailbox.read(REG_SNAPSHOT_DATA)?;
                copy.phase = CopyPhase::BodyReceipt;
            }
            CopyPhase::BodyReceipt => {
                self.snapshot_receipt()?;
                copy.cursor += 1;
                if copy.cursor == SNAPSHOT_COPY_WORDS {
                    let image = Snapshot::decode(
                        copy.buffers[copy.latest ^ 1],
                        copy.format().map_err(|_| Error::UnexpectedResponse)?,
                    )
                    .map_err(|_| Error::UnexpectedResponse)?;
                    if Some(image.header()) != copy.headers[copy.selected] {
                        return Err(Error::UnexpectedResponse);
                    }
                    copy.start_ack(copy.selected, false)
                        .map_err(|_| Error::UnexpectedResponse)?;
                } else {
                    copy.phase = CopyPhase::BodyData;
                }
            }
            CopyPhase::AckSelect => {
                self.mailbox.write(REG_SNAPSHOT_BANK, copy.bank as u32)?;
                copy.phase = CopyPhase::AckLow;
            }
            CopyPhase::AckLow => {
                self.mailbox
                    .write(REG_SNAPSHOT_ACK_LOW, copy.ack_sequence as u32)?;
                copy.phase = CopyPhase::AckHigh;
            }
            CopyPhase::AckHigh => {
                self.mailbox
                    .write(REG_SNAPSHOT_ACK_HIGH, (copy.ack_sequence >> 32) as u32)?;
                copy.phase = CopyPhase::AckEpoch;
            }
            CopyPhase::AckEpoch => {
                self.mailbox.write(REG_SNAPSHOT_ACK_EPOCH, self.epoch())?;
                copy.phase = CopyPhase::Ack;
            }
            CopyPhase::Ack => {
                self.mailbox.write(REG_SNAPSHOT_ACK, 1)?;
                copy.phase = CopyPhase::AckReceipt;
            }
            CopyPhase::AckReceipt => {
                self.snapshot_receipt()?;
                if copy.discard {
                    copy.phase = CopyPhase::BodySelect;
                } else {
                    return Ok(SnapshotStep::Updated(
                        copy.headers[copy.selected].ok_or(Error::UnexpectedResponse)?,
                    ));
                }
            }
        }
        Ok(SnapshotStep::Progress)
    }
    fn snapshot_receipt(&mut self) -> Result<(), Error<B::Error>> {
        match self.mailbox.read(REG_SNAPSHOT_RECEIPT)? {
            RECEIPT_ACCEPTED => Ok(()),
            RECEIPT_EPOCH => Err(Error::EpochChanged),
            RECEIPT_FAULT => Err(Error::Fault),
            code @ (RECEIPT_FULL | RECEIPT_INVALID | RECEIPT_UNAVAILABLE | RECEIPT_OLD_SEQUENCE
            | RECEIPT_INCOMPLETE | RECEIPT_EXHAUSTED) => Err(Error::Rejected(code)),
            _ => Err(Error::UnexpectedResponse),
        }
    }
}
