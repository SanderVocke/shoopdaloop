//! Reusable carrier fixtures and protocol lifecycle assertions for test hosts.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use crate::carrier::*;
use crate::client::*;
use shoop_rt_protocol::*;

const LIMITS: Limits = Limits {
    strips: 4,
    loops: 16,
    strip_audio_channels: 2,
    master_channels: 2,
    host_audio_inputs: 2,
    host_audio_outputs: 2,
    host_midi_inputs: 1,
    host_midi_outputs: 1,
    max_frames: 256,
    smoothing_limit_frames: 4800,
    reliable_credits: 2,
    bulk_credits: 1,
};
pub const DRIVER: DriverStatus = DriverStatus {
    kind: DriverKind::Dummy,
    state: DriverRunState::Running,
    sample_rate: 48_000,
    buffer_frames: 128,
};
const MEDIA: u64 = 600;

fn strip(index: u32) -> StripId {
    StripId(SlotId::new(index, 1))
}

fn loop_id(index: u32) -> LoopSlotId {
    LoopSlotId(SlotId::new(index, 3))
}

#[derive(Default)]
pub struct TestEndpoint {
    defer: bool,
    pending: Vec<u64>,
    release: bool,
    telemetry: bool,
    grow: bool,
}

impl Endpoint for TestEndpoint {
    fn deliver(&mut self, message: ClientMessage, out: &mut Outbox) {
        let answer = match message {
            ClientMessage::Hello { version } if version == PROTOCOL_VERSION => {
                EngineMessage::Welcome {
                    version,
                    capabilities: Capabilities::MIDI_INJECTION.union(Capabilities::GROWABLE_LIMITS),
                    limits: LIMITS,
                    driver: DRIVER,
                }
            }
            ClientMessage::Hello { version } => EngineMessage::Rejected { version },
            ClientMessage::Request { correlation, .. } if self.defer => {
                self.pending.push(correlation);
                return;
            }
            ClientMessage::Request {
                correlation,
                request,
            } => EngineMessage::Result {
                correlation,
                outcome: match request {
                    Request::RemoveStrip { .. } => Outcome::Refused(Refusal::InvalidId),
                    _ => Outcome::Done,
                },
            },
            ClientMessage::Resync => {
                for record in [
                    StateRecord::Begin { epoch: 7 },
                    StateRecord::Strip {
                        strip: strip(0),
                        shape: StripShape {
                            audio_inputs: 1,
                            audio_outputs: 2,
                            midi_input: true,
                            midi_output: false,
                        },
                    },
                    StateRecord::End { epoch: 7 },
                ] {
                    out.push(EngineMessage::State(record)).unwrap();
                }
                return;
            }
            ClientMessage::Bulk(read) => {
                let start = read.offset.min(MEDIA);
                let end = (start + BULK_CHUNK_SAMPLES as u64).min(MEDIA);
                let samples: Vec<f32> = (start..end).map(|frame| frame as f32).collect();
                EngineMessage::Bulk(BulkChunk {
                    id: read.id,
                    offset: start,
                    total: MEDIA,
                    payload: BulkPayload::audio(&samples).unwrap(),
                })
            }
            ClientMessage::Goodbye => EngineMessage::Disconnected(DisconnectReason::Shutdown),
        };
        out.push(answer).unwrap();
    }

    fn poll(&mut self, out: &mut Outbox) {
        if self.release {
            for correlation in self.pending.drain(..) {
                out.push(EngineMessage::Result {
                    correlation,
                    outcome: Outcome::Done,
                })
                .unwrap();
            }
            self.release = false;
        }
        if self.telemetry {
            out.push(EngineMessage::Telemetry(Telemetry::Engine {
                frame: 1,
                callbacks: 1,
                xruns: 0,
                load: 0.1,
                input_peak: 0.5,
                output_peak: 0.25,
                driver: DRIVER,
            }))
            .unwrap();
            self.telemetry = false;
        }
        if self.grow {
            out.push(EngineMessage::LimitsChanged(Limits {
                strips: 8,
                ..LIMITS
            }))
            .unwrap();
            self.grow = false;
        }
    }
}

pub fn outbox() -> Outbox {
    Outbox::new(8, 4, 2)
}

#[derive(Clone, Default)]
pub struct LoopPort {
    inbox: Rc<RefCell<VecDeque<Vec<u8>>>>,
    outbox: Rc<RefCell<VecDeque<Vec<u8>>>>,
}

impl LoopPort {
    fn pair() -> (Self, Self) {
        let a = Rc::new(RefCell::new(VecDeque::new()));
        let b = Rc::new(RefCell::new(VecDeque::new()));
        (
            Self {
                inbox: a.clone(),
                outbox: b.clone(),
            },
            Self {
                inbox: b,
                outbox: a,
            },
        )
    }
}

impl RecordPort for LoopPort {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        self.outbox.borrow_mut().push_back(record.to_vec());
        Ok(())
    }
    fn take(&mut self, buf: &mut [u8]) -> Option<usize> {
        let record = self.inbox.borrow_mut().pop_front()?;
        buf[..record.len()].copy_from_slice(&record);
        Some(record.len())
    }
}

/// A record client and server joined by a loopback port, pumped on every call.
pub struct Looped {
    client: RecordClient<LoopPort>,
    server: RecordServer<TestEndpoint, LoopPort>,
}

impl Carrier for Looped {
    fn send(&mut self, message: ClientMessage) -> Result<(), CarrierError> {
        self.client.send(message)?;
        self.server.pump(16);
        Ok(())
    }
    fn receive(&mut self) -> Option<EngineMessage> {
        if let Some(message) = self.client.receive() {
            return Some(message);
        }
        self.server.pump(16);
        self.client.receive()
    }
}

pub trait TestCarrier: Carrier {
    fn endpoint(&mut self) -> &mut TestEndpoint;
}

impl TestCarrier for InProcess<TestEndpoint> {
    fn endpoint(&mut self) -> &mut TestEndpoint {
        self.endpoint_mut()
    }
}

impl TestCarrier for Looped {
    fn endpoint(&mut self) -> &mut TestEndpoint {
        self.server.endpoint_mut()
    }
}

pub fn carriers() -> (InProcess<TestEndpoint>, Looped) {
    let (client, server) = LoopPort::pair();
    (
        InProcess::new(TestEndpoint::default(), outbox()),
        Looped {
            client: RecordClient::new(client),
            server: RecordServer::new(TestEndpoint::default(), server, outbox()),
        },
    )
}

pub fn drain<C: Carrier>(client: &mut ProtocolClient<C>) -> Vec<EngineMessage> {
    std::iter::from_fn(|| client.poll()).collect()
}

fn connected<C: TestCarrier>(carrier: C) -> ProtocolClient<C> {
    let mut client = ProtocolClient::new(carrier);
    assert_eq!(
        client.request(Request::SetSmoothing { frames: 1 }),
        Err(ClientError::NotConnected)
    );
    client.connect().unwrap();
    drain(&mut client);
    assert_eq!(client.state(), ClientState::Connected);
    assert_eq!(client.limits(), Some(LIMITS));
    assert!(client
        .capabilities()
        .contains(Capabilities::GROWABLE_LIMITS));
    client
}

pub fn lifecycle<C: TestCarrier>(carrier: C) {
    let mut client = connected(carrier);

    let done = client
        .request(Request::SetSmoothing { frames: 64 })
        .unwrap();
    let refused = client
        .request(Request::RemoveStrip { strip: strip(2) })
        .unwrap();
    assert_eq!(
        drain(&mut client),
        [
            EngineMessage::Result {
                correlation: done,
                outcome: Outcome::Done
            },
            EngineMessage::Result {
                correlation: refused,
                outcome: Outcome::Refused(Refusal::InvalidId)
            },
        ]
    );

    client.carrier_mut().endpoint().defer = true;
    client
        .request(Request::LoopClear {
            loop_id: loop_id(1),
        })
        .unwrap();
    client
        .request(Request::LoopClear {
            loop_id: loop_id(2),
        })
        .unwrap();
    assert_eq!(
        client.request(Request::LoopClear {
            loop_id: loop_id(3)
        }),
        Err(ClientError::Busy)
    );
    client.carrier_mut().endpoint().release = true;
    assert_eq!(drain(&mut client).len(), 2);
    assert_eq!(client.reliable_in_flight(), 0);
    client.carrier_mut().endpoint().defer = false;

    client.resync().unwrap();
    let records = drain(&mut client);
    assert_eq!(
        records.first(),
        Some(&EngineMessage::State(StateRecord::Begin { epoch: 7 }))
    );
    assert_eq!(
        records.last(),
        Some(&EngineMessage::State(StateRecord::End { epoch: 7 }))
    );

    let mut media = Vec::new();
    let mut offset = 0;
    while offset < MEDIA {
        client
            .bulk(BulkRead {
                id: 9,
                loop_id: loop_id(1),
                selector: BulkSelector::Audio { channel: 0 },
                offset,
            })
            .unwrap();
        assert_eq!(
            client.bulk(BulkRead {
                id: 10,
                loop_id: loop_id(1),
                selector: BulkSelector::Audio { channel: 0 },
                offset,
            }),
            Err(ClientError::Busy)
        );
        let Some(EngineMessage::Bulk(BulkChunk {
            payload: BulkPayload::Audio { len, samples },
            total,
            ..
        })) = client.poll()
        else {
            panic!("expected a bulk chunk");
        };
        assert_eq!(total, MEDIA);
        media.extend_from_slice(&samples[..len as usize]);
        offset += u64::from(len);
    }
    assert_eq!(
        media,
        (0..MEDIA).map(|frame| frame as f32).collect::<Vec<_>>()
    );

    client.carrier_mut().endpoint().telemetry = true;
    client.carrier_mut().endpoint().grow = true;
    drain(&mut client);
    assert_eq!(client.limits().map(|limits| limits.strips), Some(8));
    assert_eq!(client.driver(), Some(DRIVER));

    client.disconnect();
    assert_eq!(client.state(), ClientState::Disconnected);
    assert_eq!(client.resync(), Err(ClientError::NotConnected));
}

/// Deterministic fixture control for the same assertions over independent peers.
pub struct ControlledEndpoint(pub TestEndpoint);
impl Endpoint for ControlledEndpoint {
    fn deliver(&mut self, message: ClientMessage, out: &mut Outbox) {
        if let ClientMessage::Request {
            correlation: 0,
            request: Request::SetSmoothing { frames },
        } = message
        {
            self.0.defer = frames & 1 != 0;
            self.0.release |= frames & 2 != 0;
            self.0.telemetry |= frames & 4 != 0;
            self.0.grow |= frames & 8 != 0;
            self.0.poll(out);
            out.push(EngineMessage::Rejected { version: 0 }).unwrap();
        } else if let ClientMessage::Bulk(BulkRead { id: 0, .. }) = message {
            out.push(EngineMessage::Bulk(BulkChunk {
                id: 0,
                offset: 0,
                total: 0,
                payload: BulkPayload::audio(&[]).unwrap(),
            }))
            .unwrap();
        } else {
            self.0.deliver(message, out);
        }
    }
    fn poll(&mut self, out: &mut Outbox) {
        self.0.poll(out);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct RemoteCarrier<P> {
    client: RecordClient<P>,
    shadow: TestEndpoint,
    pending: VecDeque<EngineMessage>,
}
#[cfg(not(target_arch = "wasm32"))]
impl<P: RecordPort> RemoteCarrier<P> {
    pub fn new(port: P) -> Self {
        Self {
            client: RecordClient::new(port),
            shadow: TestEndpoint::default(),
            pending: VecDeque::new(),
        }
    }
    fn synchronize(&mut self) -> Result<(), CarrierError> {
        let flags = u32::from(self.shadow.defer)
            | (u32::from(self.shadow.release) << 1)
            | (u32::from(self.shadow.telemetry) << 2)
            | (u32::from(self.shadow.grow) << 3);
        self.shadow.release = false;
        self.shadow.telemetry = false;
        self.shadow.grow = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let control = ClientMessage::Request {
            correlation: 0,
            request: Request::SetSmoothing { frames: flags },
        };
        self.exchange(control, false, deadline)?;
        // A reliable acknowledgment cannot fence queued bulk delivery.
        self.exchange(
            ClientMessage::Bulk(BulkRead {
                id: 0,
                loop_id: loop_id(0),
                selector: BulkSelector::Audio { channel: 0 },
                offset: 0,
            }),
            true,
            deadline,
        )
    }

    fn exchange(
        &mut self,
        message: ClientMessage,
        bulk: bool,
        deadline: std::time::Instant,
    ) -> Result<(), CarrierError> {
        loop {
            match self.client.send(message) {
                Ok(()) => break,
                Err(CarrierError::Full) => {}
                Err(error) => return Err(error),
            }
            if std::time::Instant::now() >= deadline {
                return Err(CarrierError::Disconnected);
            }
            std::thread::yield_now();
        }
        loop {
            match self.client.receive() {
                Some(EngineMessage::Rejected { version: 0 }) if !bulk => return Ok(()),
                Some(EngineMessage::Bulk(BulkChunk {
                    id: 0,
                    offset: 0,
                    total: 0,
                    payload: BulkPayload::Audio { len: 0, .. },
                })) if bulk => return Ok(()),
                Some(message @ EngineMessage::Disconnected(_)) => {
                    self.pending.push_back(message);
                    return Err(CarrierError::Disconnected);
                }
                Some(message) => self.pending.push_back(message),
                None => {}
            }
            if std::time::Instant::now() >= deadline {
                return Err(CarrierError::Disconnected);
            }
            std::thread::yield_now();
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl<P: RecordPort> Carrier for RemoteCarrier<P> {
    fn send(&mut self, message: ClientMessage) -> Result<(), CarrierError> {
        self.synchronize()?;
        self.client.send(message)?;
        if matches!(message, ClientMessage::Goodbye) {
            return Ok(());
        }
        self.synchronize()
    }
    fn receive(&mut self) -> Option<EngineMessage> {
        if let Some(message) = self.pending.pop_front() {
            return Some(message);
        }
        let _ = self.synchronize();
        self.pending.pop_front()
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl<P: RecordPort> TestCarrier for RemoteCarrier<P> {
    fn endpoint(&mut self) -> &mut TestEndpoint {
        &mut self.shadow
    }
}
