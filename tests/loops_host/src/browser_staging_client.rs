use crate::{
    allocation_guard,
    browser::protocol_error,
    export_wire::{put32, put64},
    staging_wire::*,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct BrowserStagingClient {
    client: Client,
    records: Box<[[u8; 64]]>,
    pub(crate) request: [u8; REQUEST_BYTES],
    pub(crate) response: [u8; RESPONSE_BYTES],
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl BrowserStagingClient {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(session: u64) -> Result<Self, u32> {
        assert!(!allocation_guard::is_rendering());
        let topology = crate::scenario::staging_topology(true);
        let mut records = vec![[0; 64]; topology.encoded_records() + 1];
        topology.encode_header(&mut records[0]);
        for (i, record) in records[1..].iter_mut().enumerate() {
            topology.encode_record(i, record).map_err(|_| 11u32)?;
        }
        Ok(Self {
            client: Client::new(session).map_err(protocol_error)?,
            records: records.into_boxed_slice(),
            request: [0; REQUEST_BYTES],
            response: [0; RESPONSE_BYTES],
        })
    }
    pub fn record_count(&self) -> u32 {
        self.records.len() as u32 - 1
    }
    pub fn generation(&self, bank: u32, program: bool) -> u32 {
        self.client.generation(bank as usize, program).unwrap_or(0)
    }
    pub fn outstanding(&self) -> u32 {
        self.client.outstanding() as u32
    }
    pub fn retained(&self) -> u32 {
        self.client.retained() as u32
    }
    pub fn prepare(
        &mut self,
        kind: u8,
        bank: u32,
        generation: u32,
        at: u64,
        a: u64,
        b: u64,
        c: u64,
        index: u32,
        cancelled: bool,
    ) -> u32 {
        crate::guarded(|| {
            let data = match kind {
                BEGIN => {
                    if index > 1 {
                        return 4;
                    }
                    let mut bytes = [0; 64];
                    bytes[0] = if index == 0 { 1 } else { 2 };
                    put32(&mut bytes, 4, if index == 0 { 2 } else { 0 });
                    bytes
                }
                WRITE => match data_payload(index) {
                    Some(bytes) => bytes,
                    None => return 7,
                },
                PROGRAM_BEGIN => self.records[0],
                APPEND => match index
                    .checked_add(1)
                    .and_then(|i| self.records.get(i as usize))
                {
                    Some(data) => *data,
                    None => return 7,
                },
                _ => [0; 64],
            };
            match self.client.prepare(Request {
                session: 0,
                sequence: 0,
                kind,
                cancelled,
                at,
                bank,
                generation,
                args: [a, b, c],
                data,
            }) {
                Ok(bytes) => {
                    self.request = bytes;
                    0
                }
                Err(error) => protocol_error(error),
            }
        })
    }
    pub fn receive(&mut self) -> u32 {
        crate::guarded(|| {
            self.client
                .receive(&self.response)
                .map_or_else(protocol_error, |_| 0)
        })
    }
    pub fn request_ptr(&self) -> usize {
        self.request.as_ptr() as usize
    }
    pub fn response_ptr(&mut self) -> usize {
        self.response.as_mut_ptr() as usize
    }
}
pub(crate) fn data_payload(index: u32) -> Option<[u8; 64]> {
    let mut bytes = [0; 64];
    match index {
        0 | 2 => {
            for frame in 0..8 {
                let value = (if index == 0 { 10 } else { 100 }) + frame;
                for channel in 0..2 {
                    let value = (value * if channel == 0 { 1 } else { 10 }) as f32;
                    bytes[(frame * 2 + channel) * 4..(frame * 2 + channel + 1) * 4]
                        .copy_from_slice(&value.to_le_bytes());
                }
            }
        }
        1 => {
            put64(&mut bytes, 0, 0);
            put32(&mut bytes, 8, 3);
            bytes[12..15].copy_from_slice(&[0x90, 61, 90]);
            put64(&mut bytes, 15, 4);
            put32(&mut bytes, 23, 37);
            bytes[27] = 0xf0;
            for i in 0..35 {
                bytes[28 + i] = i as u8;
            }
            bytes[63] = 0xf7;
        }
        _ => return None,
    }
    Some(bytes)
}
