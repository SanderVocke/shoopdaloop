//! Version 1 deployment records on the authenticated management carrier.
//! This envelope survives changes to the UI/session protocol. Variant order,
//! fixed little-endian integer encoding and the discriminator are wire contracts.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
use alloc::string::String;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub const VERSION: u8 = 1;
pub const MAX_BYTES: usize = 512;
const FAMILY: &[u8; 7] = b"SHOOPDP";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Operation {
    pub client: u128,
    pub sequence: u64,
}
impl Operation {
    pub fn valid(self) -> bool {
        self.client != 0 && self.sequence != 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Info {
    pub profile: String,
    pub build: String,
    pub hash: [u8; 32],
    pub slot: Option<u8>,
    pub confirmed: bool,
    pub trial: bool,
    pub boot_nonce: u64,
    pub last_operation: Option<Operation>,
    pub session_version: u16,
    pub busy: bool,
    pub ready: bool,
}
impl Info {
    pub fn valid(&self) -> bool {
        !self.profile.is_empty()
            && self.profile.len() <= 32
            && !self.build.is_empty()
            && self.build.len() <= 40
            && self.profile.bytes().all(|b| b.is_ascii_graphic())
            && self
                .build
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            && self.boot_nonce != 0
            && self.slot.is_none_or(|slot| slot < 2)
            && self.last_operation.is_none_or(Operation::valid)
            && !(self.confirmed && self.trial)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Request {
    Status,
    Enter {
        boot_nonce: u64,
        operation: Operation,
    },
    Result {
        boot_nonce: u64,
        operation: Operation,
    },
}
impl Request {
    pub fn valid(self) -> bool {
        match self {
            Self::Status => true,
            Self::Enter {
                boot_nonce,
                operation,
            }
            | Self::Result {
                boot_nonce,
                operation,
            } => boot_nonce != 0 && operation.valid(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Refusal {
    Version,
    Unsupported,
    Busy,
    Invalid,
    StaleBoot,
    NotReady,
    CleanupFailed,
    Conflict,
    Overloaded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    Info(Info),
    Accepted(Operation),
    Finished {
        operation: Operation,
        result: Result<(), Refusal>,
    },
    Refused(Refusal),
}
impl Reply {
    pub fn valid(&self) -> bool {
        match self {
            Self::Info(info) => info.valid(),
            Self::Accepted(op) | Self::Finished { operation: op, .. } => op.valid(),
            Self::Refused(_) => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Invalid,
    Version,
    Overflow,
}

pub fn is_record(bytes: &[u8]) -> bool {
    bytes.get(2..9) == Some(FAMILY.as_slice())
}
fn encode<T: Serialize>(value: &T, output: &mut [u8]) -> Result<usize, Error> {
    let limit = output.len().min(MAX_BYTES);
    if limit < 10 {
        return Err(Error::Overflow);
    }
    let n = bincode::serde::encode_into_slice(
        value,
        &mut output[10..limit],
        bincode::config::standard()
            .with_fixed_int_encoding()
            .with_little_endian()
            .with_limit::<MAX_BYTES>(),
    )
    .map_err(|_| Error::Overflow)?;
    output[..2].copy_from_slice(&((n + 10) as u16).to_le_bytes());
    output[2..9].copy_from_slice(FAMILY);
    output[9] = VERSION;
    Ok(n + 10)
}
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    if bytes.len() < 10 || bytes.len() > MAX_BYTES || !is_record(bytes) {
        return Err(Error::Invalid);
    }
    if bytes[9] != VERSION {
        return Err(Error::Version);
    }
    let (value, n) = bincode::serde::decode_from_slice(
        &bytes[10..],
        bincode::config::standard()
            .with_fixed_int_encoding()
            .with_little_endian()
            .with_limit::<MAX_BYTES>(),
    )
    .map_err(|_| Error::Invalid)?;
    if n + 10 != bytes.len() || u16::from_le_bytes([bytes[0], bytes[1]]) as usize != bytes.len() {
        return Err(Error::Invalid);
    }
    Ok(value)
}
pub fn encode_request(value: Request, output: &mut [u8]) -> Result<usize, Error> {
    if !value.valid() {
        return Err(Error::Invalid);
    }
    encode(&value, output)
}
pub fn decode_request(bytes: &[u8]) -> Result<Request, Error> {
    let value: Request = decode(bytes)?;
    if !value.valid() {
        return Err(Error::Invalid);
    }
    Ok(value)
}
pub fn encode_reply(value: &Reply, output: &mut [u8]) -> Result<usize, Error> {
    if !value.valid() {
        return Err(Error::Invalid);
    }
    encode(value, output)
}
pub fn decode_reply(bytes: &[u8]) -> Result<Reply, Error> {
    let value: Reply = decode(bytes)?;
    if !value.valid() {
        return Err(Error::Invalid);
    }
    Ok(value)
}
