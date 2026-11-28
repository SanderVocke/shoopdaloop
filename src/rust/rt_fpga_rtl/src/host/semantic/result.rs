use super::*;
use shoop_rt_protocol::immediate::Code;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwitchOutcome {
    None,
    Refused,
    Fenced,
    Uncertain,
}

/// Copied wire observations. Decoding is not an ownership-handoff capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResultHeader {
    pub identity: GroupIdentity,
    pub faulted: bool,
    pub frame: u64,
    pub initial_revision: u64,
    pub current_revision: u64,
    pub code: Code,
    pub first_frame: u64,
    pub switch: SwitchOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupResult {
    header: ResultHeader,
    codes: [Code; MAX_COMMANDS],
}
impl GroupResult {
    pub fn decode(words: &[u32], expected: GroupIdentity) -> Result<Self, CodecError> {
        if words.len() != HEADER_WORDS + expected.count() {
            return Err(CodecError::Length);
        }
        if words[0] != 0x5346_5231
            || words[1] != SEMANTIC_VERSION | ((expected.count() as u32) << 16)
            || words[3] & !1 != 0
        {
            return Err(CodecError::Format);
        }
        if words[2] != expected.epoch() || join(&words[4..6]) != expected.sequence() {
            return Err(CodecError::Identity);
        }
        let faulted = words[3] != 0;
        let code = match Code::from_raw(words[12]) {
            Some(
                code @ (Code::Applied
                | Code::DeadlineMissed
                | Code::StaleConfiguration
                | Code::CannotApply),
            ) => code,
            _ => return Err(CodecError::Format),
        };
        let switch = match words[15] {
            0 => SwitchOutcome::None,
            1 => SwitchOutcome::Refused,
            2 if !faulted
                && code == Code::Applied
                && join(&words[13..15]) == join(&words[6..8]) =>
            {
                SwitchOutcome::Fenced
            }
            3 if faulted => SwitchOutcome::Uncertain,
            _ => return Err(CodecError::Format),
        };
        if matches!(switch, SwitchOutcome::None | SwitchOutcome::Refused)
            && join(&words[13..15]) != 0
        {
            return Err(CodecError::Format);
        }
        let mut codes = [Code::Unprocessed; MAX_COMMANDS];
        for (&word, output) in words[HEADER_WORDS..].iter().zip(&mut codes) {
            *output = Code::from_raw(word).ok_or(CodecError::Format)?;
            if (!faulted && *output == Code::Unprocessed)
                || (!faulted && code != Code::Applied && *output != code)
            {
                return Err(CodecError::Format);
            }
        }
        let initial_revision = join(&words[8..10]);
        let current_revision = join(&words[10..12]);
        if switch == SwitchOutcome::Fenced {
            if current_revision <= initial_revision || codes[expected.count() - 1] != Code::Applied
            {
                return Err(CodecError::Format);
            }
        } else if !faulted && current_revision != initial_revision {
            return Err(CodecError::Format);
        }
        Ok(Self {
            header: ResultHeader {
                identity: expected,
                faulted,
                frame: join(&words[6..8]),
                initial_revision,
                current_revision,
                code,
                first_frame: join(&words[13..15]),
                switch,
            },
            codes,
        })
    }
    pub fn header(&self) -> ResultHeader {
        self.header
    }
    pub fn codes(&self) -> &[Code] {
        &self.codes[..self.header.identity.count()]
    }
}
