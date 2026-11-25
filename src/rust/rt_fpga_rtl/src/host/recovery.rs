use super::{Contract, Error, Mailbox, RegisterBus, registers::*};

pub const RECOVERY_ID: u32 = 0x52535431;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryState {
    Draining,
    Resetting,
    Ready(u32),
    Failed,
}

pub struct Recovery<B> {
    bus: B,
    contract: Contract,
    previous: Option<u32>,
}

impl<B: RegisterBus + Clone> Mailbox<B> {
    pub fn recovery(&self) -> Recovery<B> {
        Recovery {
            bus: self.bus.clone(),
            contract: self.contract,
            previous: None,
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
        if self.previous.is_some() || !self.supported()? {
            return Err(Error::InvalidRequest);
        }
        if !matches!(
            self.bus.read(REG_RECOVERY_STATUS).map_err(Error::Bus)?,
            0 | 3
        ) {
            return Err(Error::Full);
        }
        let epoch = self.bus.read(REG_EPOCH).map_err(Error::Bus)?;
        if epoch == 0 || epoch == u32::MAX {
            return Err(Error::EpochChanged);
        }
        self.bus
            .write(REG_RECOVERY_REQUEST, epoch)
            .map_err(Error::Bus)?;
        if !matches!(
            self.bus.read(REG_RECOVERY_STATUS).map_err(Error::Bus)?,
            1..=4
        ) {
            return Err(Error::EpochChanged);
        }
        self.previous = Some(epoch);
        Ok(())
    }

    pub fn poll(&mut self) -> Result<RecoveryState, Error<B::Error>> {
        let previous = self.previous.ok_or(Error::InvalidRequest)?;
        let state = self.bus.read(REG_RECOVERY_STATUS).map_err(Error::Bus)?;
        Ok(match state {
            1 => RecoveryState::Draining,
            2 => RecoveryState::Resetting,
            3 => {
                let epoch = self.bus.read(REG_EPOCH).map_err(Error::Bus)?;
                if epoch != previous + 1 {
                    return Err(Error::EpochChanged);
                }
                self.previous = None;
                RecoveryState::Ready(epoch)
            }
            4 => RecoveryState::Failed,
            _ => return Err(Error::UnexpectedResponse),
        })
    }

    pub fn reopen(&self) -> Result<Mailbox<B>, Error<B::Error>> {
        if self.previous.is_some() {
            return Err(Error::Full);
        }
        Mailbox::open(self.bus.clone(), self.contract)
    }
}
