//! Generation-qualified resident configuration actions and immutable order publication.
use crate::{
    audio_image::Transfer,
    protocol::{get, put},
};
use rt_runtime::Error;
mod service;
pub use service::{Progress, Service};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Identity {
    pub slot: u16,
    pub generation: u32,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    pub identity: Identity,
    pub effective_frame: u64,
    pub sequence: u64,
    pub opcode: u8,
    pub state: bool,
    pub address: u16,
    pub published: bool,
    pub terminal: bool,
    pub error: u8,
    pub terminal_frame: u64,
    pub payload: [u32; 7],
}
impl Record {
    pub fn decode(slot: u16, words: &[u32; 26]) -> Result<Self, Error> {
        let generation = get(words, 1, 32) as u32;
        if get(words, 0, 1) == 0
            || generation == 0
            || get(words, 187, 4) > 11
            || words[15..].iter().any(|&w| w != 0)
        {
            return Err(Error::Disconnected);
        }
        let mut payload = [0; 7];
        for (i, word) in payload.iter_mut().enumerate() {
            *word = get(words, 255 + i * 32, 32) as u32;
        }
        if get(words, 186, 1) != 0
            && (get(words, 384, 63) != 0
                || get(words, 447, 32) != 0
                || (get(words, 187, 4) != 0 && payload.iter().any(|&w| w != 0)))
        {
            return Err(Error::Disconnected);
        }
        Ok(Self {
            identity: Identity { slot, generation },
            effective_frame: get(words, 33, 64),
            sequence: get(words, 97, 64),
            opcode: get(words, 161, 8) as u8,
            state: get(words, 169, 1) != 0,
            address: get(words, 170, 16) as u16,
            published: get(words, 479, 1) != 0,
            terminal: get(words, 186, 1) != 0,
            error: get(words, 187, 4) as u8,
            terminal_frame: get(words, 191, 64),
            payload,
        })
    }
}
fn root(opcode: u8, address: u16) -> Transfer {
    Transfer {
        opcode,
        state: false,
        address,
        words: [0; 20],
    }
}
pub fn install(
    id: Identity,
    sequence: u64,
    requested_frame: u64,
    action: &Transfer,
) -> Result<Transfer, Error> {
    if id.generation == 0 || !matches!(action.opcode, 0 | 5 | 6 | 8) {
        return Err(Error::InvalidParameter);
    }
    if action.words[7..].iter().any(|&v| v != 0) {
        return Err(Error::InvalidShape);
    }
    let mut t = root(128 | action.opcode, id.slot);
    t.words[..7].copy_from_slice(&action.words[..7]);
    put(&mut t.words, 384, 64, requested_frame);
    put(&mut t.words, 448, 64, sequence);
    put(&mut t.words, 512, 32, u64::from(id.generation));
    put(&mut t.words, 544, 16, u64::from(action.address));
    put(&mut t.words, 560, 1, u64::from(action.state));
    Ok(t)
}
pub fn begin(version: u64, count: u32) -> Transfer {
    let mut t = root(16, 0);
    put(&mut t.words, 0, 64, version);
    put(&mut t.words, 64, 32, u64::from(count));
    t
}
pub fn node(index: u16, id: Identity, nonce: u64) -> Transfer {
    let mut t = root(17, index);
    put(&mut t.words, 0, 16, u64::from(id.slot));
    put(&mut t.words, 16, 32, u64::from(id.generation));
    put(&mut t.words, 64, 64, nonce);
    t
}
pub fn commit(nonce: u64) -> Transfer {
    let mut t = root(18, 0);
    put(&mut t.words, 0, 64, nonce);
    t
}
pub fn read(id: Identity) -> Transfer {
    let mut t = root(19, id.slot);
    t.words[0] = id.generation;
    t
}
pub fn collect(id: Identity) -> Transfer {
    let mut t = read(id);
    t.opcode = 20;
    t
}
pub fn abort(id: Identity) -> Transfer {
    let mut t = read(id);
    t.opcode = 22;
    t
}
pub fn cancel(id: Identity) -> Transfer {
    let mut t = read(id);
    t.opcode = 23;
    t
}
pub fn status() -> Transfer {
    root(21, 0)
}
