use crate::{midi_state::MidiState, Error, Frame};

pub use shoop_midi_protocol::{Rejection as PolicyRejection, MAX_MESSAGE_BYTES};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum MessagePolicy {
    #[default]
    RejectParameters = 0,
    LiveOnlyParameters = 1,
}

impl From<PolicyRejection> for Error {
    fn from(rejection: PolicyRejection) -> Self {
        match rejection {
            PolicyRejection::TooLong => Self::Capacity,
            PolicyRejection::Parameter => Self::InvalidParameter,
            _ => Self::InvalidEvent,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShortMessage {
    bytes: [u8; MAX_MESSAGE_BYTES],
    len: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageDisposition {
    Managed(ShortMessage),
    LiveOnly(ShortMessage),
}
impl MessageDisposition {
    pub fn message(self) -> ShortMessage {
        match self {
            Self::Managed(message) | Self::LiveOnly(message) => message,
        }
    }
}

impl MessagePolicy {
    pub fn classify(self, data: &[u8]) -> Result<MessageDisposition, PolicyRejection> {
        let disposition = shoop_midi_protocol::classify(data, self == Self::LiveOnlyParameters)?;
        let mut bytes = [0; MAX_MESSAGE_BYTES];
        bytes[..data.len()].copy_from_slice(data);
        let message = ShortMessage {
            bytes,
            len: data.len() as u8,
        };
        Ok(match disposition {
            shoop_midi_protocol::Disposition::Managed => MessageDisposition::Managed(message),
            shoop_midi_protocol::Disposition::LiveOnly => MessageDisposition::LiveOnly(message),
        })
    }
}

impl ShortMessage {
    pub fn managed(data: &[u8]) -> Result<Self, PolicyRejection> {
        match MessagePolicy::RejectParameters.classify(data)? {
            MessageDisposition::Managed(message) => Ok(message),
            MessageDisposition::LiveOnly(_) => Err(PolicyRejection::Parameter),
        }
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
    pub fn apply(self, state: &mut MidiState) -> Result<(), Error> {
        let mut update = crate::midi_state::BasicUpdate::begin(self)?;
        while !update.complete() {
            update.step(state);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventRecord {
    pub frame: Frame,
    pub order: u16,
    message: ShortMessage,
}
impl EventRecord {
    pub const ENCODED_BYTES: usize = 16;

    pub fn new(frame: Frame, order: u64, data: &[u8]) -> Result<Self, Error> {
        Ok(Self {
            frame,
            order: order.try_into().map_err(|_| Error::Exhausted)?,
            message: ShortMessage::managed(data).map_err(Error::from)?,
        })
    }
    pub fn message(self) -> ShortMessage {
        self.message
    }
    pub fn encode(self) -> [u8; Self::ENCODED_BYTES] {
        let mut bytes = [0; Self::ENCODED_BYTES];
        bytes[..8].copy_from_slice(&self.frame.0.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.message.bytes);
        bytes[12] = self.message.len;
        bytes[14..].copy_from_slice(&self.order.to_le_bytes());
        bytes
    }
    pub fn decode(bytes: [u8; Self::ENCODED_BYTES]) -> Result<Self, Error> {
        let len = usize::from(bytes[12]);
        if len == 0
            || len > MAX_MESSAGE_BYTES
            || bytes[13] != 0
            || bytes[8 + len..12].iter().any(|&byte| byte != 0)
        {
            return Err(Error::InvalidEvent);
        }
        Self::new(
            Frame(u64::from_le_bytes(bytes[..8].try_into().unwrap())),
            u64::from(u16::from_le_bytes(bytes[14..].try_into().unwrap())),
            &bytes[8..8 + len],
        )
    }
}
