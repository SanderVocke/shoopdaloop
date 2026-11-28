use super::{Contract, Error, Mailbox, RegisterBus, registers::*};

pub const RECOVERY_ID: u32 = 0x52535431;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryState {
    Draining,
    Resetting,
    /// Hardware reset completed, not application Ready.
    Ready(u32),
    Failed,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RequestState {
    Idle,
    Pending(u32),
    Failed,
}

pub struct Recovery<B> {
    bus: B,
    contract: Contract,
    epoch: u32,
    version: u32,
    request: RequestState,
}

impl<B: RegisterBus + Clone> Mailbox<B> {
    pub fn recovery(&self) -> Recovery<B> {
        Recovery {
            bus: self.bus.clone(),
            contract: self.contract,
            epoch: self.epoch,
            version: self.version,
            request: RequestState::Idle,
        }
    }
}

impl<B: RegisterBus + Clone> Recovery<B> {
    pub fn supported(&mut self) -> Result<bool, Error<B::Error>> {
        self.bus
            .read(REG_RECOVERY_ID)
            .map(|id| id == RECOVERY_ID)
            .map_err(Error::Bus)
    }

    pub fn begin(&mut self) -> Result<(), Error<B::Error>> {
        if self.request != RequestState::Idle || !self.supported()? {
            return Err(Error::InvalidRequest);
        }
        if !matches!(
            self.bus.read(REG_RECOVERY_STATUS).map_err(Error::Bus)?,
            0 | 3
        ) {
            return Err(Error::Full);
        }
        let epoch = self.bus.read(REG_EPOCH).map_err(Error::Bus)?;
        if epoch != self.epoch || epoch == 0 || epoch == u32::MAX {
            return Err(Error::EpochChanged);
        }
        self.request = RequestState::Pending(epoch);
        self.bus
            .write(REG_RECOVERY_REQUEST, epoch)
            .map_err(Error::Bus)?;
        match self.bus.read(REG_RECOVERY_STATUS).map_err(Error::Bus)? {
            1..=3 => {}
            4 => self.request = RequestState::Failed,
            _ => {
                self.request = RequestState::Failed;
                return Err(Error::EpochChanged);
            }
        }
        Ok(())
    }

    pub fn poll(&mut self) -> Result<RecoveryState, Error<B::Error>> {
        let previous = match self.request {
            RequestState::Idle => return Err(Error::InvalidRequest),
            RequestState::Pending(epoch) => epoch,
            RequestState::Failed => return Ok(RecoveryState::Failed),
        };
        let state = self.bus.read(REG_RECOVERY_STATUS).map_err(Error::Bus)?;
        Ok(match state {
            1 => RecoveryState::Draining,
            2 => RecoveryState::Resetting,
            3 => {
                let epoch = self.bus.read(REG_EPOCH).map_err(Error::Bus)?;
                if epoch != previous + 1 {
                    self.request = RequestState::Failed;
                    return Err(Error::EpochChanged);
                }
                self.epoch = epoch;
                self.request = RequestState::Idle;
                RecoveryState::Ready(epoch)
            }
            4 => {
                self.request = RequestState::Failed;
                RecoveryState::Failed
            }
            _ => {
                self.request = RequestState::Failed;
                return Err(Error::UnexpectedResponse);
            }
        })
    }

    pub fn reopen(&self) -> Result<Mailbox<B>, Error<B::Error>> {
        if self.request != RequestState::Idle {
            return Err(Error::Full);
        }
        let mailbox = Mailbox::open_version(self.bus.clone(), self.contract, self.version)?;
        if mailbox.epoch != self.epoch {
            return Err(Error::EpochChanged);
        }
        Ok(mailbox)
    }
}
