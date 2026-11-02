use shoop_app::{ConnectionId, Service};
use shoop_app_client::{Client, State};
use shoop_app_connection::Session;
use shoop_app_protocol::*;
use shoop_transport::{CarrierError, RecordPort};
use shoop_wasm_test_support::shoop_test;
use std::{collections::VecDeque, time::Duration};

mod support;
use support::service;
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

#[shoop_test]
fn timed_out_connection_releases_only_its_authority_and_cannot_evict_replacement() {
    let mut service = service();
    let mut session = Session::new(ConnectionId(1), Memory::default());
    let mut client = Client::new(ClientId(3), HostId(1), SessionId(2)).unwrap();
    session
        .port
        .send(WireMessage::Client(client.attach().unwrap()));
    for _ in 0..128 {
        step(&mut session, &mut service, &mut client);
        if client.state() == State::Ready {
            break;
        }
    }
    let old = client.authority().unwrap();
    session = session.with_heartbeat(Default::default(), 1).unwrap();
    session.advance(Duration::from_secs(1));
    session.write(&mut service);
    session.port.output.clear();
    session.advance(Duration::from_secs(5));
    session.receive(&mut service);
    assert!(!session.alive(&service));
    assert!(!service.has_connection(ConnectionId(1)));
    assert!(service.is_ready());
    let mut replacement = Session::new(ConnectionId(2), Memory::default());
    let mut next = Client::new(ClientId(4), HostId(1), SessionId(2)).unwrap();
    replacement
        .port
        .send(WireMessage::Client(next.attach().unwrap()));
    for _ in 0..128 {
        step(&mut replacement, &mut service, &mut next);
        if next.state() == State::Ready {
            break;
        }
    }
    assert!(next.authority().unwrap().generation > old.generation);
    assert!(matches!(
        service.management(ManagementRequest::Shutdown {
            authority: old,
            operation: OperationId {
                client: old.client,
                sequence: 1
            }
        }),
        ManagementReply::Refused(Refusal::StaleAuthority)
    ));
    session.port.send(WireMessage::Pong(1));
    session.receive(&mut service);
    assert!(service.has_connection(ConnectionId(2)));
}
