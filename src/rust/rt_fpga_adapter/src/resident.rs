//! Payloads prepared on ARM; target validity and musical resolution remain at the device cut.
use crate::{
    audio_actions as action,
    audio_image::Transfer,
    protocol::{self, get, put},
    transition_service::wire,
};
use rt_runtime::{Error, Frame};
use shoop_loops::{LoopId, Request};
pub const NATIVE: u8 = 0;
pub const RUNTIME_ERROR: u8 = 1;
pub const TIMELINE_ERROR: u8 = 2;
pub const QUEUED_CANCELLED: u8 = 3;
pub const DONE: u8 = 4;
pub const SNAPSHOT: u8 = 5;
pub const GRAPH_ACTIVATED: u8 = 6;
pub const INITIAL_ACTIVATED: u8 = 7;
pub const CURSOR: u8 = 8;
pub const CANCEL_RESULT: u8 = 9;
pub const TAG: usize = 504;
pub const DEFERRED_ERROR: usize = 496;
pub fn runtime_code(error: Error) -> u8 {
    match error {
        Error::InvalidId => 1,
        Error::InvalidParameter => 2,
        Error::Capacity => 3,
        Error::Busy => 4,
        Error::Exhausted => 5,
        Error::InvalidState => 6,
        Error::InvalidShape => 7,
        Error::InvalidFrame => 8,
        Error::Cancelled => 9,
        Error::StaleEpoch => 10,
        Error::InvalidConfig => 11,
        Error::InvalidEvent => 12,
        Error::Full => 13,
        Error::Disconnected => 14,
        Error::InvalidGraph => 15,
        Error::Cycle => 16,
        Error::PortType => 17,
        Error::InvalidOffset => 18,
        Error::Incomplete => 19,
        Error::Budget => 20,
        Error::StaleRevision => 21,
    }
}
pub fn runtime_error(code: u8) -> Result<Error, Error> {
    Ok(match code {
        1 => Error::InvalidId,
        2 => Error::InvalidParameter,
        3 => Error::Capacity,
        4 => Error::Busy,
        5 => Error::Exhausted,
        6 => Error::InvalidState,
        7 => Error::InvalidShape,
        8 => Error::InvalidFrame,
        9 => Error::Cancelled,
        10 => Error::StaleEpoch,
        11 => Error::InvalidConfig,
        12 => Error::InvalidEvent,
        13 => Error::Full,
        14 => Error::Disconnected,
        15 => Error::InvalidGraph,
        16 => Error::Cycle,
        17 => Error::PortType,
        18 => Error::InvalidOffset,
        19 => Error::Incomplete,
        20 => Error::Budget,
        21 => Error::StaleRevision,
        _ => return Err(Error::Disconnected),
    })
}
fn transfer(opcode: u8) -> Transfer {
    Transfer {
        opcode,
        state: false,
        address: 0,
        words: [0; 20],
    }
}
fn identity(id: LoopId) -> u64 {
    u64::from(id.slot()) | (u64::from(id.generation()) << 32)
}
pub fn transition(request: Request) -> Transfer {
    let mut t = transfer(action::TRANSITION);
    match protocol::transition(request) {
        Ok(words) => t.words = wire::install(&words, 0),
        Err(error) => {
            put(&mut t.words, wire::TARGET, 64, identity(request.target));
            put(&mut t.words, wire::SEQUENCE, 64, request.sequence.0);
            put(&mut t.words, wire::AT, 64, request.at.0);
            put(
                &mut t.words,
                DEFERRED_ERROR,
                8,
                u64::from(runtime_code(error)),
            );
        }
    }
    t
}
pub fn clear(target: LoopId, requested: Frame) -> Transfer {
    let mut t = transfer(action::CLEAR);
    put(&mut t.words, 0, 64, identity(target));
    put(&mut t.words, 64, 64, requested.0);
    t
}
pub fn repeat(target: LoopId, enabled: bool) -> Transfer {
    let mut t = transfer(action::REPEAT);
    put(&mut t.words, 0, 64, identity(target));
    put(&mut t.words, 64, 1, u64::from(enabled));
    t
}
pub fn cancel(sequence: rt_runtime::Sequence) -> Transfer {
    let mut t = transfer(action::CANCEL);
    put(&mut t.words, 0, 64, sequence.0);
    t
}
pub fn tag(payload: &[u32; 16]) -> u8 {
    get(payload, TAG, 8) as u8
}

pub fn state(target: LoopId) -> Transfer {
    let mut t = transfer(action::QUERY);
    put(&mut t.words, 0, 64, identity(target));
    t
}
pub fn export(target: LoopId, selection: shoop_loops::runtime::Selection) -> Transfer {
    let mut t = transfer(action::EXPORT);
    put(&mut t.words, 0, 64, identity(target));
    put(
        &mut t.words,
        64,
        2,
        match selection {
            shoop_loops::runtime::Selection::Playback => 0,
            shoop_loops::runtime::Selection::Latest => 1,
            shoop_loops::runtime::Selection::Recording => 2,
        },
    );
    t
}
