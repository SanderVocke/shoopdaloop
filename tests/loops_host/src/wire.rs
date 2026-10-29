//! Fixed-width little-endian fixture protocol; no Rust owners or pointers cross it.
use shoop_loops::{LoopId, Transition};

pub const NORMAL_SLOTS: usize = 4;
pub const SLOTS: usize = NORMAL_SLOTS + 1;
pub const REQUEST_BYTES: usize = 64;
pub const RESPONSE_BYTES: usize = 96;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Size,
    Format,
    Version,
    Reserved,
    Operation,
    Identity,
    Slot,
    Sequence,
    Session,
    Busy,
    State,
    Exhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Request {
    pub session: u64,
    pub sequence: u64,
    pub slot: usize,
    pub at: u64,
    pub target: LoopId,
    pub transition: Transition,
    pub recovery: bool,
    pub cancelled: bool,
}

fn get32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn get64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn header(bytes: &[u8], length: usize, magic: &[u8; 4]) -> Result<(), Error> {
    if bytes.len() != length {
        return Err(Error::Size);
    }
    if &bytes[..4] != magic {
        return Err(Error::Format);
    }
    if bytes[4..6] != 1u16.to_le_bytes() {
        return Err(Error::Version);
    }
    Ok(())
}

impl Request {
    pub fn encode(self) -> Result<[u8; REQUEST_BYTES], Error> {
        if self.session == 0 || self.sequence == 0 {
            return Err(Error::Identity);
        }
        if self.slot >= SLOTS || (self.slot == NORMAL_SLOTS) != self.recovery {
            return Err(Error::Slot);
        }
        let (kind, option, argument) = match self.transition {
            Transition::Record { monitor_previous } => (1, monitor_previous, 0),
            Transition::Play {
                position,
                retrigger,
            } => (2, retrigger, position),
            Transition::Stop => (3, false, 0),
            Transition::Seek {
                position,
                retrigger,
            } => (4, retrigger, position),
            _ => return Err(Error::Operation),
        };
        if self.recovery && self.transition != Transition::Stop {
            return Err(Error::Operation);
        }
        let mut bytes = [0; REQUEST_BYTES];
        bytes[..4].copy_from_slice(b"LLC1");
        bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
        bytes[6] = kind;
        bytes[7] =
            u8::from(option) | (u8::from(self.cancelled) << 1) | (u8::from(self.recovery) << 2);
        put64(&mut bytes, 8, self.session);
        put64(&mut bytes, 16, self.sequence);
        put64(&mut bytes, 24, self.at);
        put32(&mut bytes, 32, self.slot as u32);
        put32(&mut bytes, 36, self.target.slot());
        put32(&mut bytes, 40, self.target.generation());
        put64(&mut bytes, 48, argument);
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        header(bytes, REQUEST_BYTES, b"LLC1")?;
        if bytes[7] & !7 != 0 || bytes[44..48].iter().chain(&bytes[56..]).any(|&b| b != 0) {
            return Err(Error::Reserved);
        }
        let option = bytes[7] & 1 != 0;
        let argument = get64(bytes, 48);
        let transition = match bytes[6] {
            1 if argument == 0 => Transition::Record {
                monitor_previous: option,
            },
            2 => Transition::Play {
                position: argument,
                retrigger: option,
            },
            3 if argument == 0 && !option => Transition::Stop,
            4 => Transition::Seek {
                position: argument,
                retrigger: option,
            },
            _ => return Err(Error::Operation),
        };
        let request = Self {
            session: get64(bytes, 8),
            sequence: get64(bytes, 16),
            at: get64(bytes, 24),
            slot: get32(bytes, 32) as usize,
            target: LoopId::from_parts(get32(bytes, 36), get32(bytes, 40))
                .map_err(|_| Error::Identity)?,
            transition,
            recovery: bytes[7] & 4 != 0,
            cancelled: bytes[7] & 2 != 0,
        };
        request.encode()?;
        Ok(request)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Phase {
    Admission = 1,
    Execution = 2,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Status {
    Admitted = 1,
    Cancelled = 2,
    Rejected = 3,
    Applied = 4,
    Superseded = 5,
    Failed = 6,
}

/// `observed` is admission-completion frame or terminal execution frame, by phase.
/// Error codes 1..=20 are runtime delivery/admission errors, 257..=268 are
/// timeline failures, and 513..=532 are runtime-resource failures returned by
/// the domain after successful runtime delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Response {
    pub session: u64,
    pub sequence: u64,
    pub slot: usize,
    pub phase: Phase,
    pub status: Status,
    pub runtime_sequence: u64,
    pub requested: u64,
    pub effective: u64,
    pub delivered: Option<u64>,
    pub observed: u64,
    pub error: u32,
}
impl Response {
    fn validate(self) -> Result<(), Error> {
        if self.session == 0 || self.sequence == 0 {
            return Err(Error::Identity);
        }
        if self.slot >= SLOTS {
            return Err(Error::Slot);
        }
        let valid = match self.phase {
            Phase::Admission => {
                self.delivered.is_none()
                    && match self.status {
                        Status::Admitted => {
                            self.error == 0
                                && self.runtime_sequence != 0
                                && self.effective >= self.requested
                        }
                        Status::Cancelled => self.error == 0 && self.runtime_sequence != 0,
                        Status::Rejected => (1..=20).contains(&self.error),
                        _ => false,
                    }
            }
            Phase::Execution => {
                self.runtime_sequence != 0
                    && self.effective >= self.requested
                    && self
                        .delivered
                        .is_none_or(|at| at >= self.effective && at <= self.observed)
                    && match self.status {
                        Status::Applied | Status::Superseded => {
                            self.error == 0 && self.delivered.is_some()
                        }
                        Status::Cancelled => self.error == 0,
                        Status::Rejected => (1..=20).contains(&self.error),
                        Status::Failed => {
                            ((257..=268).contains(&self.error) || (513..=532).contains(&self.error))
                                && self.delivered.is_some()
                        }
                        _ => false,
                    }
            }
        };
        if valid {
            Ok(())
        } else {
            Err(Error::State)
        }
    }
    pub fn encode(self) -> Result<[u8; RESPONSE_BYTES], Error> {
        self.validate()?;
        let mut bytes = [0; RESPONSE_BYTES];
        bytes[..4].copy_from_slice(b"LLR1");
        bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
        bytes[6] = self.phase as u8;
        bytes[7] = u8::from(self.delivered.is_some());
        put64(&mut bytes, 8, self.session);
        put64(&mut bytes, 16, self.sequence);
        put32(&mut bytes, 24, self.slot as u32);
        put32(&mut bytes, 28, self.status as u32);
        put64(&mut bytes, 32, self.runtime_sequence);
        put64(&mut bytes, 40, self.requested);
        put64(&mut bytes, 48, self.effective);
        put64(&mut bytes, 56, self.delivered.unwrap_or(0));
        put64(&mut bytes, 64, self.observed);
        put32(&mut bytes, 72, self.error);
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        header(bytes, RESPONSE_BYTES, b"LLR1")?;
        if bytes[7] > 1
            || bytes[76..].iter().any(|&b| b != 0)
            || (bytes[7] == 0 && get64(bytes, 56) != 0)
        {
            return Err(Error::Reserved);
        }
        let response = Self {
            session: get64(bytes, 8),
            sequence: get64(bytes, 16),
            slot: get32(bytes, 24) as usize,
            phase: match bytes[6] {
                1 => Phase::Admission,
                2 => Phase::Execution,
                _ => return Err(Error::Operation),
            },
            status: match get32(bytes, 28) {
                1 => Status::Admitted,
                2 => Status::Cancelled,
                3 => Status::Rejected,
                4 => Status::Applied,
                5 => Status::Superseded,
                6 => Status::Failed,
                _ => return Err(Error::Operation),
            },
            runtime_sequence: get64(bytes, 32),
            requested: get64(bytes, 40),
            effective: get64(bytes, 48),
            delivered: (bytes[7] == 1).then(|| get64(bytes, 56)),
            observed: get64(bytes, 64),
            error: get32(bytes, 72),
        };
        response.validate()?;
        Ok(response)
    }
}

pub(crate) fn runtime_error(error: rt_runtime::Error) -> u32 {
    use rt_runtime::Error::*;
    match error {
        InvalidConfig => 1,
        Capacity => 2,
        Exhausted => 3,
        InvalidShape => 4,
        InvalidFrame => 5,
        InvalidId => 6,
        InvalidEvent => 7,
        Full => 8,
        Disconnected => 9,
        InvalidState => 10,
        InvalidGraph => 11,
        Cycle => 12,
        PortType => 13,
        StaleEpoch => 14,
        Busy => 15,
        InvalidParameter => 16,
        InvalidOffset => 17,
        Incomplete => 18,
        Budget => 19,
        StaleRevision => 20,
    }
}
pub(crate) fn loop_error(error: shoop_loops::Error) -> u32 {
    use shoop_loops::Error::*;
    match error {
        InvalidConfig => 257,
        Capacity => 258,
        InvalidId => 259,
        DuplicateId => 260,
        InvalidState => 261,
        InvalidFrame => 262,
        InvalidProgram => 263,
        MissingSync => 264,
        InstantaneousDependencyCycle => 265,
        RecursiveProgram => 266,
        Exhausted => 267,
        EmptyContent => 268,
    }
}
