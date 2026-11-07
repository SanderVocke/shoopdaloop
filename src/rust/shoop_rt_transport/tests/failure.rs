use shoop_rt_protocol::{ClientMessage, DisconnectReason, EngineMessage};
use shoop_rt_transport::carrier::*;
use shoop_wasm_test_support::shoop_test;
use std::{cell::Cell, rc::Rc};
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

struct Port(Rc<Cell<bool>>);
impl RecordPort for Port {
    fn post(&mut self, _: &[u8]) -> Result<(), CarrierError> {
        Ok(())
    }
    fn take(&mut self, _: &mut [u8]) -> Option<usize> {
        None
    }
    fn failure(&mut self) -> Option<CarrierError> {
        self.0.get().then_some(CarrierError::Disconnected)
    }
}
#[shoop_test]
fn idle_is_distinct_from_eof_and_eof_is_delivered_once_without_a_send() {
    let closed = Rc::new(Cell::new(false));
    let mut client = RecordClient::new(Port(closed.clone()));
    assert_eq!(client.receive(), None);
    closed.set(true);
    assert_eq!(
        client.receive(),
        Some(EngineMessage::Disconnected(DisconnectReason::Shutdown))
    );
    assert_eq!(client.receive(), None);
    assert_eq!(
        client.send(ClientMessage::Resync),
        Err(CarrierError::Disconnected)
    );
}
#[shoop_test]
fn failed_record_server_can_return_its_endpoint_without_dropping_it() {
    let closed = Rc::new(Cell::new(true));
    let mut server = RecordServer::new(
        shoop_rt_transport::contracts::TestEndpoint::default(),
        Port(closed),
        Outbox::new(8, 4, 2),
    );
    server.pump(1);
    assert_eq!(server.failure(), Some(CarrierError::Disconnected));
    let (_endpoint, _port, _outbox) = server.into_parts();
}
