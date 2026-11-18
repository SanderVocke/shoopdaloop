//! ARM-side access to the versioned FPGA host mailbox.
#[cfg(feature = "rt-interface")]
pub mod control;
pub mod registers;
mod request;
mod upload;
use registers::*;
pub use request::{Client, Request};
pub use upload::{Upload, UploadStep};

/// Read and write naturally aligned, little-endian MMIO words in program order.
pub trait RegisterBus {
    type Error;

    fn read(&mut self, offset: u32) -> Result<u32, Self::Error>;
    fn write(&mut self, offset: u32, value: u32) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Contract {
    pub clock_hz: u32,
    pub arena_base: u32,
    pub arena_bytes: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Response {
    pub tag: u32,
    pub code: u32,
    pub data: [u32; HOST_PAYLOAD_WORDS],
}

#[derive(Debug, Eq, PartialEq)]
pub enum Error<E> {
    Bus(E),
    Incompatible,
    EpochChanged,
    Full,
    Fault,
    InvalidRequest,
    UnexpectedResponse,
}

pub struct Mailbox<B> {
    bus: B,
    epoch: u32,
}

impl<B: RegisterBus> Mailbox<B> {
    pub fn open(bus: B, expected: Contract) -> Result<Self, Error<B::Error>> {
        let mut device = Self { bus, epoch: 0 };
        if device.read(REG_ID)? != HOST_ID
            || device.read(REG_VERSION)? != HOST_VERSION
            || device.read(REG_CORE_CLOCK_HZ)? != expected.clock_hz
            || device.read(REG_ARENA_BASE)? != expected.arena_base
            || device.read(REG_ARENA_BYTES)? != expected.arena_bytes
        {
            return Err(Error::Incompatible);
        }
        device.epoch = device.read(REG_EPOCH)?;
        if device.epoch == 0 {
            return Err(Error::Incompatible);
        }
        Ok(device)
    }

    pub fn submit(
        &mut self,
        tag: u32,
        operation: u32,
        args: &[u32],
    ) -> Result<(), Error<B::Error>> {
        if tag == 0 || args.len() > HOST_PAYLOAD_WORDS {
            return Err(Error::InvalidRequest);
        }
        let status = self.status()?;
        if status & STATUS_FAULT != 0 {
            return Err(Error::Fault);
        }
        if status & STATUS_REQUEST_SPACE == 0 {
            return Err(Error::Full);
        }
        self.write(REG_REQUEST_EPOCH, self.epoch)?;
        self.write(REG_REQUEST_OP, operation)?;
        for index in 0..HOST_PAYLOAD_WORDS {
            self.write(
                REG_REQUEST_DATA + 4 * index as u32,
                args.get(index).copied().unwrap_or(0),
            )?;
        }
        self.write(REG_REQUEST_PUSH, tag)
    }

    pub fn response(&mut self) -> Result<Option<Response>, Error<B::Error>> {
        let status = self.status()?;
        if status & STATUS_RESPONSE_PENDING == 0 {
            return if status & STATUS_FAULT == 0 {
                Ok(None)
            } else {
                Err(Error::Fault)
            };
        }
        let tag = self.read(REG_RESPONSE_TAG)?;
        let code = self.read(REG_RESPONSE_CODE)?;
        let mut data = [0; HOST_PAYLOAD_WORDS];
        for (index, value) in data.iter_mut().enumerate() {
            *value = self.read(REG_RESPONSE_DATA + 4 * index as u32)?;
        }
        self.check_epoch()?;
        self.write(REG_RESPONSE_POP, tag)?;
        Ok(Some(Response { tag, code, data }))
    }

    pub fn into_bus(self) -> B {
        self.bus
    }

    fn check_epoch(&mut self) -> Result<(), Error<B::Error>> {
        if self.read(REG_EPOCH)? == self.epoch {
            Ok(())
        } else {
            Err(Error::EpochChanged)
        }
    }

    fn status(&mut self) -> Result<u32, Error<B::Error>> {
        self.check_epoch()?;
        self.read(REG_STATUS)
    }

    fn read(&mut self, offset: u32) -> Result<u32, Error<B::Error>> {
        self.bus.read(offset).map_err(Error::Bus)
    }

    fn write(&mut self, offset: u32, value: u32) -> Result<(), Error<B::Error>> {
        self.bus.write(offset, value).map_err(Error::Bus)
    }
}
