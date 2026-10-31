use shoop_rt_protocol::{ClientMessage, EngineMessage};
use shoop_rt_transport::buffer::{Batch, Stream};
use shoop_rt_transport::{carrier::*, contracts::*};
use shoop_rt_transport_message_port::{
    server::{RecordLanes, Records},
    RecordBatches,
};
use shoop_wasm_test_support::shoop_test;
use std::{cell::RefCell, rc::Rc};
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

struct Bridge {
    client: RecordClient<RecordBatches>,
    server: RecordServer<TestEndpoint, RecordLanes>,
    buffers: RecordLanes,
}
impl Bridge {
    fn pump(&mut self) {
        if let Some(bytes) = self.client.port_mut().take_outgoing() {
            let mut buffers = self.buffers.0.borrow_mut();
            buffers.inbox.free_mut()[..bytes.len()].copy_from_slice(&bytes);
            assert!(buffers.inbox.commit(bytes.len()));
        }
        self.server.pump(32);
        let mut buffers = self.buffers.0.borrow_mut();
        self.client.port_mut().receive(buffers.outbox.as_slice());
        buffers.outbox.clear();
        buffers.inbox.compact();
    }
}
impl Carrier for Bridge {
    fn send(&mut self, message: ClientMessage) -> Result<(), CarrierError> {
        self.client.send(message)?;
        self.pump();
        Ok(())
    }
    fn receive(&mut self) -> Option<EngineMessage> {
        self.pump();
        self.client.receive()
    }
}
impl TestCarrier for Bridge {
    fn endpoint(&mut self) -> &mut TestEndpoint {
        self.server.endpoint_mut()
    }
}
#[shoop_test]
fn message_port_uses_the_shared_lifecycle_credit_and_bulk_contract() {
    let buffers = RecordLanes(Rc::new(RefCell::new(Records {
        inbox: Stream::new(16384),
        outbox: Batch::new(16384),
        framing_errors: 0,
    })));
    lifecycle(Bridge {
        client: RecordClient::new(RecordBatches::new()),
        server: RecordServer::new(TestEndpoint::default(), buffers.clone(), outbox()),
        buffers,
    });
}
