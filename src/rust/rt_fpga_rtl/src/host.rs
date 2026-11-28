//! ARM-side access to the versioned FPGA host mailbox.
#[cfg(feature = "rt-interface")]
pub mod control;
mod recovery;
pub mod registers;
#[cfg(feature = "rt-interface")]
pub mod semantic;
pub mod semantic_registers;
pub use recovery::{Recovery, RecoveryState};
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
pub struct Geometry {
    pub instructions: u32,
    pub states: u32,
    pub jobs: u32,
    pub buffers: u32,
    pub views: u32,
    pub blocks: u32,
    pub chunk_frames: u32,
    pub block_chunks: u32,
    pub quantum: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Metrics {
    pub clock_hz: u32,
    pub completed_quanta: u32,
    pub peak_quantum_cycles: u32,
    pub audio_xruns: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Response {
    pub tag: u32,
    pub code: u32,
    pub data: [u32; HOST_PAYLOAD_WORDS],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncEvent {
    pub revision: u64,
    pub data: [u32; 8],
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
    Timeout,
    Rejected(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FaultRecord {
    pub epoch: u32,
    pub cause: u32,
    pub tag: u32,
    pub operation: u32,
    pub cycles: u32,
    pub control_cycles: u32,
    pub detail: u32,
    pub revision: u64,
    pub frame: u64,
}

pub struct Mailbox<B> {
    bus: B,
    epoch: u32,
    clock_hz: u32,
    contract: Contract,
    version: u32,
}

impl<B: RegisterBus> Mailbox<B> {
    pub fn open(bus: B, expected: Contract) -> Result<Self, Error<B::Error>> {
        Self::open_version(bus, expected, HOST_VERSION)
    }

    fn open_version(bus: B, expected: Contract, version: u32) -> Result<Self, Error<B::Error>> {
        let mut device = Self {
            bus,
            epoch: 0,
            clock_hz: expected.clock_hz,
            contract: expected,
            version,
        };
        if device.read(REG_ID)? != HOST_ID
            || device.read(REG_VERSION)? != version
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
        self.write(REG_REQUEST_PUSH, tag)?;
        let result = self.read(REG_SUBMISSION_RESULT)?;
        self.check_epoch()?;
        match result {
            SUBMIT_ACCEPTED => Ok(()),
            SUBMIT_FULL => Err(Error::Full),
            SUBMIT_EPOCH | SUBMIT_UNAVAILABLE => Err(Error::EpochChanged),
            SUBMIT_FAULT => Err(Error::Fault),
            _ => Err(Error::InvalidRequest),
        }
    }

    /// The first fault in the latest failed epoch, retained across datapath recovery.
    pub fn fault_record(&mut self) -> Result<Option<FaultRecord>, Error<B::Error>> {
        let epoch = self.read(REG_FAULT_EPOCH)?;
        if epoch == 0 {
            return Ok(None);
        }
        let record = FaultRecord {
            epoch,
            cause: self.read(REG_FAULT_CAUSE)?,
            tag: self.read(REG_FAULT_TAG)?,
            operation: self.read(REG_FAULT_OP)?,
            cycles: self.read(REG_FAULT_CYCLES)?,
            control_cycles: self.read(REG_FAULT_CONTROL_CYCLES)?,
            detail: self.read(REG_FAULT_DETAIL)?,
            revision: u64::from(self.read(REG_FAULT_REVISION_LOW)?)
                | (u64::from(self.read(REG_FAULT_REVISION_HIGH)?) << 32),
            frame: u64::from(self.read(REG_FAULT_FRAME_LOW)?)
                | (u64::from(self.read(REG_FAULT_FRAME_HIGH)?) << 32),
        };
        if self.read(REG_FAULT_EPOCH)? != epoch {
            return Err(Error::EpochChanged);
        }
        Ok(Some(record))
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
        self.check_epoch()?;
        Ok(Some(Response { tag, code, data }))
    }

    /// Drain events even when the dispatcher has faulted, retaining their arrival order.
    pub fn sync_event(&mut self) -> Result<Option<SyncEvent>, Error<B::Error>> {
        self.check_epoch()?;
        if self.read(REG_EVENT_COUNT)? & u32::from(u16::MAX) == 0 {
            return Ok(None);
        }
        let tag = self.read(REG_EVENT_TAG)?;
        let revision = u64::from(self.read(REG_EVENT_REVISION_LOW)?)
            | (u64::from(self.read(REG_EVENT_REVISION_HIGH)?) << 32);
        let mut data = [0; 8];
        for (index, value) in data.iter_mut().enumerate() {
            *value = self.read(REG_EVENT_DATA + 4 * index as u32)?;
        }
        self.check_epoch()?;
        self.write(REG_EVENT_POP, tag)?;
        self.check_epoch()?;
        Ok(Some(SyncEvent { revision, data }))
    }

    pub fn into_bus(self) -> B {
        self.bus
    }

    pub fn geometry(&mut self) -> Result<Geometry, Error<B::Error>> {
        self.check_epoch()?;
        let geometry = Geometry {
            instructions: self.read(REG_PROGRAM_DEPTH)?,
            states: self.read(REG_STATE_DEPTH)?,
            jobs: self.read(REG_JOB_DEPTH)?,
            buffers: self.read(REG_BUFFERS)?,
            views: self.read(REG_VIEWS)?,
            blocks: self.read(REG_MEDIA_BLOCKS)?,
            chunk_frames: self.read(REG_CHUNK_FRAMES)?,
            block_chunks: self.read(REG_BLOCK_CHUNKS)?,
            quantum: self.read(REG_QUANTUM)?,
        };
        self.check_epoch()?;
        Ok(geometry)
    }

    pub fn metrics(&mut self) -> Result<Metrics, Error<B::Error>> {
        self.check_epoch()?;
        let metrics = Metrics {
            clock_hz: self.clock_hz,
            completed_quanta: self.read(REG_COMPLETED_QUANTA)?,
            peak_quantum_cycles: self.read(REG_PEAK_QUANTUM_CYCLES)?,
            audio_xruns: self.read(REG_AUDIO_XRUNS)?,
        };
        self.check_epoch()?;
        Ok(metrics)
    }

    pub fn abort(&mut self) -> Result<(), Error<B::Error>> {
        self.check_epoch()?;
        self.write(REG_ABORT, self.epoch)?;
        self.check_epoch()
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
