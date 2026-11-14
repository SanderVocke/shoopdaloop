//! Private canonical parameter/image maps produced by the ARM compiler.
use crate::{
    audio_image::Transfer,
    image::{Ramp, Value},
    image_wire,
    protocol::put,
};
#[derive(Default)]
pub struct Command {
    pub opcode: u8,
    pub key: u16,
    pub bank: bool,
    pub nonce: u64,
    pub count: u32,
    pub address: u16,
    pub value: Option<Value>,
    pub target: i32,
    pub duration: u32,
}
impl Command {
    pub fn transfer(self) -> Transfer {
        let mut t = Transfer {
            opcode: 49,
            state: false,
            address: 0,
            words: [0; 20],
        };
        put(&mut t.words, 0, 8, u64::from(self.opcode));
        put(&mut t.words, 8, 16, u64::from(self.key));
        put(&mut t.words, 24, 1, u64::from(self.bank));
        put(&mut t.words, 32, 64, self.nonce);
        put(&mut t.words, 96, 32, u64::from(self.count));
        put(&mut t.words, 128, 16, u64::from(self.address));
        if let Some(value) = self.value {
            let words = image_wire::value_words(value);
            for (i, &word) in words[..4].iter().enumerate() {
                put(&mut t.words, 144 + i * 32, 32, u64::from(word));
            }
            put(&mut t.words, 272, 1, u64::from(words[4]));
        }
        put(&mut t.words, 273, 32, u64::from(self.target as u32));
        put(&mut t.words, 305, 32, u64::from(self.duration));
        t
    }
    pub fn seed(key: u16, ramp: Ramp) -> Self {
        Self {
            key,
            value: Some(Value::Ramp(ramp)),
            ..Self::default()
        }
    }
    pub fn begin(nonce: u64, count: u32) -> Self {
        Self {
            opcode: 1,
            bank: nonce & 1 != 0,
            nonce,
            count,
            ..Self::default()
        }
    }
    pub fn binding(nonce: u64, index: u32, key: u16, address: u16) -> Self {
        Self {
            opcode: 2,
            bank: nonce & 1 != 0,
            nonce,
            count: index,
            key,
            address,
            ..Self::default()
        }
    }
    pub fn smoothing_binding(nonce: u64, index: u32, address: u16, base: u32) -> Self {
        Self {
            opcode: 10,
            bank: nonce & 1 != 0,
            nonce,
            count: index,
            address,
            value: Some(Value::Word(u64::from(base))),
            ..Self::default()
        }
    }
}
