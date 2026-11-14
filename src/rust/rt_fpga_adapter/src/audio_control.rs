//! Versioned control batches, published completely before device execution.
use crate::protocol::{get, put};
use rt_runtime::Error;

pub const ABI: u8 = 2;
pub const HEADER_OPCODE: u8 = 0xa0 | ABI;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Completion {
    pub batch: u64,
    pub operation: u32,
    pub configuration: bool,
    pub opcode: u8,
    pub error: u8,
    pub quantum: u64,
    pub epoch: u32,
    pub payload: [u32; 26],
}
impl Completion {
    pub fn decode(words: &[u32; 34]) -> Result<Self, Error> {
        let kind = get(words, 836, 2);
        let error = get(words, 832, 4);
        let batch = get(words, 878, 64);
        let opcode = get(words, 838, 8) as u8;
        let cfg_bits = match opcode {
            0..=8 | 16..=18 => 129,
            19 | 20 | 22 | 23 | 128 | 133 | 134 | 136 => 480,
            21 => 257,
            _ => 0,
        };
        let invalid_cfg =
            kind == 1 && (cfg_bits == 0 || (cfg_bits..832).any(|bit| get(words, bit, 1) != 0));
        if kind > 1
            || batch == 0
            || get(words, 1038, 50) != 0
            || error > 10
            || (error != 0 && words[..26].iter().any(|&word| word != 0))
            || invalid_cfg
        {
            return Err(Error::Disconnected);
        }
        let mut payload = [0; 26];
        payload.copy_from_slice(&words[..26]);
        Ok(Self {
            batch,
            operation: get(words, 846, 32) as u32,
            configuration: kind == 1,
            opcode: get(words, 838, 8) as u8,
            error: error as u8,
            quantum: get(words, 942, 64),
            epoch: get(words, 1006, 32) as u32,
            payload,
        })
    }
}

pub fn header(sequence: u64, id: u64, commands: u32, capacity: u32) -> Result<[u32; 24], Error> {
    if id == 0 {
        return Err(Error::InvalidId);
    }
    if commands == 0 || commands >= capacity {
        return Err(Error::Capacity);
    }
    let mut words = [0; 24];
    put(&mut words, 0, 64, id);
    put(&mut words, 640, 32, u64::from(commands));
    put(&mut words, 672, 8, u64::from(HEADER_OPCODE));
    put(&mut words, 680, 2, 2);
    put(&mut words, 682, 64, sequence);
    Ok(words)
}
