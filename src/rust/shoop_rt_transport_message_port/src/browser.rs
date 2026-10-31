//! Browser bindings for the main-thread half; excluded from the worklet artifact.
use crate::RecordBatches;
use js_sys::{Array, Object, Reflect, Uint8Array};
use shoop_rt_protocol::DisconnectReason;
use shoop_rt_transport::carrier::{CarrierError, RecordPort};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;
use web_sys::MessagePort;

type Sideband = (&'static str, Box<dyn FnMut() -> Option<Vec<u8>>>);
struct Outgoing {
    records: RecordBatches,
    target: MessagePort,
    scheduled: bool,
    sideband: Option<Sideband>,
}

#[derive(Clone)]
pub struct Port(Rc<RefCell<Outgoing>>);
impl Port {
    pub fn new(target: MessagePort) -> Self {
        Self(Rc::new(RefCell::new(Outgoing {
            records: RecordBatches::new(),
            target,
            scheduled: false,
            sideband: None,
        })))
    }
    /// Host-owned device lanes may join the same main-thread batch.
    pub fn with_sideband(
        target: MessagePort,
        kind: &'static str,
        take: Box<dyn FnMut() -> Option<Vec<u8>>>,
    ) -> Self {
        let port = Self::new(target);
        port.0.borrow_mut().sideband = Some((kind, take));
        port
    }
    pub fn receive(&self, bytes: &[u8]) {
        self.0.borrow_mut().records.receive(bytes);
    }
    pub fn close(&self, reason: DisconnectReason) {
        self.0.borrow_mut().records.close(reason);
    }
    pub fn request_flush(&self) {
        {
            let mut outgoing = self.0.borrow_mut();
            if outgoing.scheduled {
                return;
            }
            outgoing.scheduled = true;
        }
        let port = self.clone();
        spawn_local(async move { port.flush() });
    }
    fn flush(&self) {
        let (target, records, sideband) = {
            let mut outgoing = self.0.borrow_mut();
            outgoing.scheduled = false;
            let sideband = outgoing
                .sideband
                .as_mut()
                .and_then(|(kind, take)| take().map(|bytes| (*kind, bytes)));
            (
                outgoing.target.clone(),
                outgoing.records.take_outgoing(),
                sideband,
            )
        };
        for batch in [records.map(|bytes| ("records", bytes)), sideband]
            .into_iter()
            .flatten()
        {
            if post_bytes(&target, batch.0, &batch.1).is_err() {
                self.close(DisconnectReason::Fault);
                break;
            }
        }
    }
}
impl RecordPort for Port {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        self.0.borrow_mut().records.post(record)?;
        self.request_flush();
        Ok(())
    }
    fn take(&mut self, buf: &mut [u8]) -> Option<usize> {
        self.0.borrow_mut().records.take(buf)
    }
}

pub fn post_bytes(target: &MessagePort, kind: &str, bytes: &[u8]) -> Result<(), JsValue> {
    let buffer = Uint8Array::from(bytes).buffer();
    let message = Object::new();
    Reflect::set(&message, &"kind".into(), &kind.into())?;
    Reflect::set(&message, &"bytes".into(), &buffer)?;
    target.post_message_with_transferable(&message, &Array::of1(&buffer))
}
