use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use shoop_rt_protocol::carrier::*;
use shoop_rt_protocol::client::*;
use shoop_rt_protocol::codec::*;
use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

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
    reliable_credits: 2,
    bulk_credits: 1,
};
const DRIVER: DriverStatus = DriverStatus {
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
struct TestEndpoint {
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
                xruns: 0,
                load: 0.1,
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

fn outbox() -> Outbox {
    Outbox::new(8, 4, 2)
}

#[derive(Clone, Default)]
struct LoopPort {
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
struct Looped {
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

trait TestCarrier: Carrier {
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

fn carriers() -> (InProcess<TestEndpoint>, Looped) {
    let (client, server) = LoopPort::pair();
    (
        InProcess::new(TestEndpoint::default(), outbox()),
        Looped {
            client: RecordClient::new(client),
            server: RecordServer::new(TestEndpoint::default(), server, outbox()),
        },
    )
}

fn drain<C: Carrier>(client: &mut ProtocolClient<C>) -> Vec<EngineMessage> {
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

fn lifecycle<C: TestCarrier>(carrier: C) {
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

#[shoop_test]
fn lifecycle_credits_resync_bulk_and_limits_behave_alike_on_both_carriers() {
    let (in_process, looped) = carriers();
    lifecycle(in_process);
    lifecycle(looped);
}

#[shoop_test]
fn a_version_mismatch_is_rejected_on_both_carriers() {
    let (mut in_process, mut looped) = carriers();
    for carrier in [&mut in_process as &mut dyn Carrier, &mut looped] {
        carrier
            .send(ClientMessage::Hello {
                version: PROTOCOL_VERSION + 1,
            })
            .unwrap();
        assert_eq!(
            carrier.receive(),
            Some(EngineMessage::Rejected {
                version: PROTOCOL_VERSION + 1
            })
        );
    }
}

#[shoop_test]
fn telemetry_keeps_the_latest_records_while_reliable_and_bulk_lanes_refuse() {
    let mut out = Outbox::new(1, 2, 1);
    for frame in 0..5 {
        out.push(EngineMessage::Telemetry(Telemetry::Engine {
            frame,
            xruns: 0,
            load: 0.,
            driver: DRIVER,
        }))
        .unwrap();
    }
    assert_eq!(out.dropped_telemetry(), 3);
    let result = EngineMessage::Result {
        correlation: 1,
        outcome: Outcome::Done,
    };
    out.push(result).unwrap();
    assert_eq!(out.push(result), Err(CarrierError::Full));
    assert_eq!(out.available(Lane::Reliable), 0);
    assert_eq!(out.pop(), Some(result));
    let frames: Vec<u64> = std::iter::from_fn(|| out.pop())
        .map(|message| match message {
            EngineMessage::Telemetry(Telemetry::Engine { frame, .. }) => frame,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(frames, [3, 4]);
}

fn client_samples() -> Vec<ClientMessage> {
    let requests = [
        Request::CreateStrip {
            strip: strip(1),
            shape: StripShape {
                audio_inputs: 2,
                audio_outputs: 2,
                midi_input: true,
                midi_output: true,
            },
        },
        Request::RemoveStrip { strip: strip(1) },
        Request::StripControl {
            strip: strip(1),
            control: StripControl::OutputBalance(-0.25),
        },
        Request::StripControl {
            strip: strip(1),
            control: StripControl::Monitoring(true),
        },
        Request::CreateLoop {
            loop_id: loop_id(4),
            strip: strip(1),
        },
        Request::RemoveLoop {
            loop_id: loop_id(4),
        },
        Request::LoopSync {
            loop_id: loop_id(4),
            source: Some(loop_id(0)),
        },
        Request::LoopSync {
            loop_id: loop_id(4),
            source: None,
        },
        Request::LoopTransition {
            loop_id: loop_id(4),
            mode: LoopMode::Recording,
            delay: Some(2),
        },
        Request::LoopClear {
            loop_id: loop_id(4),
        },
        Request::LoopGain {
            loop_id: loop_id(4),
            gain: 0.5,
        },
        Request::LoopBalance {
            loop_id: loop_id(4),
            balance: 1.,
        },
        Request::Connect {
            port: EnginePort::Strip(StripPort {
                strip: strip(1),
                kind: PortKind::MidiIn,
                index: 0,
            }),
            host: HostPortId(3),
            connected: true,
        },
        Request::Connect {
            port: EnginePort::Bus(BusPort {
                bus: BusId::Master,
                index: 1,
            }),
            host: HostPortId(2),
            connected: false,
        },
        Request::Route {
            port: StripPort {
                strip: strip(1),
                kind: PortKind::AudioOut,
                index: 1,
            },
            bus: BusPort {
                bus: BusId::Master,
                index: 0,
            },
            connected: false,
        },
        Request::BusControl {
            bus: BusId::Master,
            control: BusControl::Mute(true),
        },
        Request::InjectMidi {
            strip: strip(1),
            message: MidiMessage::new(&[0x90, 60, 100]).unwrap(),
        },
        Request::SetSmoothing { frames: 96 },
    ];
    let mut messages = vec![
        ClientMessage::Hello { version: 1 },
        ClientMessage::Resync,
        ClientMessage::Goodbye,
        ClientMessage::Bulk(BulkRead {
            id: 5,
            loop_id: loop_id(2),
            selector: BulkSelector::Midi,
            offset: 1024,
        }),
    ];
    messages.extend(requests.into_iter().enumerate().map(|(index, request)| {
        ClientMessage::Request {
            correlation: index as u64 + 1,
            request,
        }
    }));
    messages
}

fn engine_samples() -> Vec<EngineMessage> {
    let name = "x".repeat(MAX_NAME_BYTES);
    let peaks = Peaks::new(&[0.5; MAX_STRIP_CHANNELS]).unwrap();
    let full: Vec<f32> = (0..BULK_CHUNK_SAMPLES).map(|i| i as f32 * 0.25).collect();
    vec![
        EngineMessage::Welcome {
            version: 1,
            capabilities: Capabilities::BULK_MIDI,
            limits: LIMITS,
            driver: DRIVER,
        },
        EngineMessage::Rejected { version: 3 },
        EngineMessage::Result {
            correlation: u64::MAX,
            outcome: Outcome::Refused(Refusal::Capacity),
        },
        EngineMessage::Telemetry(Telemetry::Strip {
            strip: strip(1),
            input_peaks: peaks,
            output_peaks: Peaks::default(),
            midi_in: true,
            midi_out: false,
        }),
        EngineMessage::Telemetry(Telemetry::Loop {
            loop_id: loop_id(3),
            mode: LoopMode::Playing,
            next_mode: Some(LoopMode::Stopped),
            next_delay: Some(1),
            length: 48,
            position: 7,
            peaks,
        }),
        EngineMessage::Telemetry(Telemetry::Bus {
            bus: BusId::Master,
            peaks,
        }),
        EngineMessage::State(StateRecord::HostPort(HostPort {
            id: HostPortId(1),
            kind: HostPortKind::Audio,
            direction: Direction::Output,
            name: Name::new(&name).unwrap(),
            preferred: true,
        })),
        EngineMessage::State(StateRecord::Loop {
            loop_id: loop_id(3),
            strip: strip(1),
            sync: Some(loop_id(0)),
            gain: 0.75,
            balance: -1.,
        }),
        EngineMessage::State(StateRecord::Connection {
            port: EnginePort::Strip(StripPort {
                strip: strip(1),
                kind: PortKind::AudioOut,
                index: 1,
            }),
            host: HostPortId(2),
        }),
        EngineMessage::State(StateRecord::Connection {
            port: EnginePort::Bus(BusPort {
                bus: BusId::Master,
                index: 0,
            }),
            host: HostPortId(3),
        }),
        EngineMessage::State(StateRecord::Route {
            port: StripPort {
                strip: strip(1),
                kind: PortKind::AudioOut,
                index: 0,
            },
            bus: BusPort {
                bus: BusId::Master,
                index: 1,
            },
        }),
        EngineMessage::State(StateRecord::Bus {
            bus: BusId::Master,
            control: BusControl::Gain(0.5),
        }),
        EngineMessage::Bulk(BulkChunk {
            id: 1,
            offset: 256,
            total: 1000,
            payload: BulkPayload::audio(&full).unwrap(),
        }),
        EngineMessage::Bulk(BulkChunk {
            id: 2,
            offset: 0,
            total: 3,
            payload: BulkPayload::midi(&[0x90, 1, 2]).unwrap(),
        }),
        EngineMessage::Bulk(BulkChunk {
            id: 3,
            offset: 0,
            total: 0,
            payload: BulkPayload::Refused(Refusal::Unsupported),
        }),
        EngineMessage::LimitsChanged(LIMITS),
        EngineMessage::Disconnected(DisconnectReason::Fault),
    ]
}

#[shoop_test]
fn every_message_round_trips_through_bounded_records() {
    let mut buf = [0; MAX_RECORD_BYTES];
    for message in client_samples() {
        let len = encode_client(&message, &mut buf).unwrap();
        assert_eq!(decode_client(&buf[..len]), Ok(message));
    }
    for message in engine_samples() {
        let len = encode_engine(&message, &mut buf).unwrap();
        assert!(len <= MAX_RECORD_BYTES);
        assert_eq!(decode_engine(&buf[..len]), Ok(message));
    }
}

#[shoop_test]
fn malformed_records_are_refused_without_panicking() {
    let mut buf = [0; MAX_RECORD_BYTES];
    for message in engine_samples() {
        let len = encode_engine(&message, &mut buf).unwrap();
        for cut in 0..len {
            assert!(
                decode_engine(&buf[..cut]).is_err(),
                "{message:?} cut at {cut}"
            );
        }
        let mut longer = buf[..len].to_vec();
        longer.push(0);
        assert_eq!(decode_engine(&longer), Err(CodecError::Trailing));
        let mut padded = longer.clone();
        padded[..2].copy_from_slice(&(len as u16 + 1).to_le_bytes());
        assert!(decode_engine(&padded).is_err());
    }
    let len = encode_client(&ClientMessage::Resync, &mut buf).unwrap();
    buf[2] = 99;
    assert_eq!(decode_client(&buf[..len]), Err(CodecError::Invalid));
    let midi = ClientMessage::Request {
        correlation: 1,
        request: Request::InjectMidi {
            strip: strip(0),
            message: MidiMessage::new(&[0x90, 1, 2]).unwrap(),
        },
    };
    let len = encode_client(&midi, &mut buf).unwrap();
    let length_byte = len - 4;
    for bad in [0, MAX_MIDI_BYTES as u8 + 1] {
        let mut record = buf[..len].to_vec();
        record[length_byte] = bad;
        assert!(decode_client(&record).is_err());
    }
    assert!(Name::new(&"y".repeat(MAX_NAME_BYTES + 1)).is_none());
    assert!(Peaks::new(&[0.; MAX_STRIP_CHANNELS + 1]).is_none());
    assert!(MidiMessage::new(&[]).is_none());
    assert!(BulkPayload::audio(&[0.; BULK_CHUNK_SAMPLES + 1]).is_none());
    assert_eq!(
        encode_engine(&engine_samples()[12], &mut [0; 64]),
        Err(CodecError::Overflow)
    );
    let connect = ClientMessage::Request {
        correlation: 1,
        request: Request::Connect {
            port: EnginePort::Bus(BusPort {
                bus: BusId::Master,
                index: 7,
            }),
            host: HostPortId(2),
            connected: true,
        },
    };
    let len = encode_client(&connect, &mut buf).unwrap();
    let tag = buf[..len]
        .windows(3)
        .position(|bytes| bytes == [1, 0, 7])
        .unwrap();
    let mut record = buf[..len].to_vec();
    record[tag] = 2;
    assert_eq!(decode_client(&record), Err(CodecError::Invalid));
}
