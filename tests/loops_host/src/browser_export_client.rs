use crate::{
    browser::protocol_error,
    export_wire::{self, Client, Request},
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct BrowserExportClient {
    client: Client,
    session: u64,
    pub(crate) request: [u8; export_wire::REQUEST_BYTES],
    pub(crate) response: [u8; export_wire::RESPONSE_BYTES],
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl BrowserExportClient {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(session: u64) -> Result<Self, u32> {
        Ok(Self {
            client: Client::new(session).map_err(protocol_error)?,
            session,
            request: [0; export_wire::REQUEST_BYTES],
            response: [0; export_wire::RESPONSE_BYTES],
        })
    }
    pub fn request_ptr(&mut self) -> *mut u8 {
        self.request.as_mut_ptr()
    }
    pub fn response_ptr(&mut self) -> *mut u8 {
        self.response.as_mut_ptr()
    }
    pub fn outstanding(&self) -> u32 {
        self.client.outstanding() as u32
    }
    pub fn retained(&self) -> u32 {
        self.client.retained() as u32
    }
    pub fn generation(&self, bank: u32) -> u32 {
        self.client.generation(bank as usize).unwrap_or(0)
    }
    pub fn prepare(
        &mut self,
        kind: u8,
        bank: u32,
        generation: u32,
        at: u64,
        position: u64,
        channel: u8,
        selection: u8,
        cancelled: bool,
    ) -> u32 {
        crate::guarded(|| {
            let request = Request {
                session: self.session,
                sequence: 0,
                kind,
                at,
                bank,
                generation,
                target: (kind == export_wire::EXPORT)
                    .then(|| shoop_loops::LoopId::from_parts(0, 1).unwrap()),
                position,
                channel,
                selection,
                cancelled,
            };
            match self.client.prepare(request) {
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
}
