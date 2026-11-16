use super::{MessageDisposition, MessagePolicy, ShortMessage};
use crate::{midi_state::MidiState, Error};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum InitialFormat {
    LegacyPrefixV0 = 0,
    BasicPrefixV1 = 1,
}

pub struct InitialDecoder {
    state: MidiState,
    format: InitialFormat,
    normalized: u64,
}
impl InitialDecoder {
    pub fn new(format: InitialFormat) -> Self {
        Self {
            state: MidiState::default(),
            format,
            normalized: 0,
        }
    }
    pub fn push(&mut self, packet: &[u8]) -> Result<(), Error> {
        if self.format == InitialFormat::LegacyPrefixV0 {
            match MessagePolicy::LiveOnlyParameters
                .classify(packet)
                .map_err(Error::from)?
            {
                MessageDisposition::LiveOnly(message) => {
                    let data = message.as_bytes();
                    if matches!(data[1], 98..=101) && data[2] == 127 {
                        self.normalized = self.normalized.checked_add(1).ok_or(Error::Exhausted)?;
                        return Ok(());
                    }
                    return Err(Error::InvalidParameter);
                }
                MessageDisposition::Managed(message) => return message.apply(&mut self.state),
            }
        }
        ShortMessage::managed(packet)
            .map_err(Error::from)?
            .apply(&mut self.state)
    }
    pub fn state(&self) -> &MidiState {
        &self.state
    }
    pub fn normalized_selectors(&self) -> u64 {
        self.normalized
    }
}
