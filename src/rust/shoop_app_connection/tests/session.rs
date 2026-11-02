use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_app_client::{Client, State};
use shoop_app_connection::Session;
use shoop_app_protocol::*;
use shoop_rt_backend::{EngineClock, RtBackend};
use shoop_rt_engine::{testing::OfflineRender, EngineDescription};
use shoop_transport::{CarrierError, RecordPort};
use shoop_wasm_test_support::shoop_test;
use std::{cell::RefCell, collections::VecDeque, rc::Rc, time::Duration};

struct Clock(Rc<RefCell<OfflineRender>>);
impl EngineClock for Clock {
    fn advance(&mut self, elapsed: Duration) {
        self.0.borrow_mut().run_for(elapsed, |_, _| 0.25).unwrap();
    }
    fn wait(&mut self) -> bool {
        self.0.borrow_mut().run(16, |_, _| 0.25).unwrap();
        true
    }
}
fn service() -> Service {
    let (render, client) = OfflineRender::new(&EngineDescription::default()).unwrap();
    let backend = RtBackend::connect(client, Clock(Rc::new(RefCell::new(render)))).unwrap();
    let mut service = Service::new(
        Box::new(backend),
        ServiceOptions {
            host: HostId(1),
            session: SessionId(2),
            master_auto_connect: true,
            loop_smoothing_ms: 3,
        },
    )
    .unwrap();
    for _ in 0..128 {
        service.tick(Duration::from_millis(10));
        if service.is_ready() {
            return service;
        }
    }
    panic!("service did not initialize");
}
#[derive(Default)]
struct Memory {
    input: VecDeque<Vec<u8>>,
    output: VecDeque<Vec<u8>>,
}
impl Memory {
    fn send(&mut self, message: WireMessage) {
        let mut bytes = [0; 1280];
        let n = codec::encode(&message, &mut bytes).unwrap();
        self.input.push_back(bytes[..n].to_vec());
    }
}
impl RecordPort for Memory {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        if self.output.len() == 8 {
            return Err(CarrierError::Full);
        }
        self.output.push_back(record.to_vec());
        Ok(())
    }
    fn take(&mut self, out: &mut [u8]) -> Option<usize> {
        let record = self.input.pop_front()?;
        out[..record.len()].copy_from_slice(&record);
        Some(record.len())
    }
    fn pending_records(&self) -> usize {
        self.output.len()
    }
}
fn step(session: &mut Session<Memory>, service: &mut Service, client: &mut Client) {
    session.advance(Duration::from_millis(10));
    session.receive(service);
    service.tick(Duration::from_millis(10));
    session.write(service);
    while let Some(bytes) = session.port.output.pop_front() {
        client.receive_wire(codec::decode(&bytes).unwrap()).unwrap();
    }
    if let Some(message) = client.ready_message() {
        session.port.send(WireMessage::Client(message));
        client.ready_sent().unwrap();
    }
}
#[shoop_test]
fn portable_host_driven_connection_attaches_confirms_and_detaches() {
    let mut service = service();
    let mut session = Session::new(ConnectionId(1), Memory::default());
    let mut client = Client::new(ClientId(3), HostId(1), SessionId(2)).unwrap();
    session
        .port
        .send(WireMessage::Client(client.attach().unwrap()));
    for _ in 0..128 {
        step(&mut session, &mut service, &mut client);
        if client.state() == State::Ready && service.has_connection(ConnectionId(1)) {
            break;
        }
    }
    assert_eq!(client.state(), State::Ready);
    let message = client
        .command(Command::SetLoopSmoothingMs(7), None)
        .unwrap();
    let ClientMessage::Submit { operation, .. } = message else {
        panic!()
    };
    session.port.send(WireMessage::Client(message));
    for _ in 0..128 {
        step(&mut session, &mut service, &mut client);
        if client.results().contains_key(&operation) {
            break;
        }
    }
    assert!(matches!(
        client.results()[&operation],
        Outcome::Confirmed { .. }
    ));
    session
        .port
        .send(WireMessage::Client(ClientMessage::Detach {
            authority: client.authority().unwrap(),
        }));
    session.receive(&mut service);
    assert!(!service.has_connection(ConnectionId(1)));
}
