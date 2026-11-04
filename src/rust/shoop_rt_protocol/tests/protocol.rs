use shoop_rt_protocol::codec::*;
use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

const LIMITS: Limits = Limits {
    content_chunk_bytes: 1024,
    strips: 4,
    loops: 16,
    strip_audio_channels: 2,
    master_channels: 2,
    buses: 8,
    bus_channels: 8,
    host_audio_inputs: 2,
    host_audio_outputs: 2,
    host_midi_inputs: 1,
    host_midi_outputs: 1,
    max_frames: 256,
    smoothing_limit_frames: 4800,
    reliable_credits: 2,
    bulk_credits: 1,
};
const DRIVER: DriverStatus = DriverStatus {
    kind: DriverKind::Dummy,
    state: DriverRunState::Running,
    sample_rate: 48_000,
    buffer_frames: 128,
};

fn strip(index: u32) -> StripId {
    StripId(SlotId::new(index, 1))
}

fn loop_id(index: u32) -> LoopSlotId {
    LoopSlotId(SlotId::new(index, 3))
}

fn client_samples() -> Vec<ClientMessage> {
    let requests = [
        Request::CreateBus {
            bus: BusId(SlotId::new(7, 42)),
            channels: 8,
        },
        Request::RemoveBus {
            bus: BusId(SlotId::new(7, 42)),
        },
        Request::ReplacementEdit {
            id: 2,
            edit: ReplacementEdit::Bus {
                bus: BusId(SlotId::new(7, 42)),
                channels: 8,
            },
        },
        Request::CancelBulk,
        Request::ReplacementBegin { id: 1 },
        Request::ReplacementEdit {
            id: 1,
            edit: ReplacementEdit::Strip {
                strip: strip(0),
                shape: StripShape {
                    audio_inputs: 2,
                    audio_outputs: 2,
                    midi_input: true,
                    midi_output: true,
                },
                controls: StripPreset {
                    input_gain: 0.5,
                    input_balance: -0.25,
                    monitoring: false,
                    output_gain: 1.,
                    output_balance: 0.25,
                    output_mute: true,
                },
            },
        },
        Request::ReplacementEdit {
            id: 1,
            edit: ReplacementEdit::Loop {
                loop_id: loop_id(0),
                strip: strip(0),
                sync: Some(loop_id(1)),
                frames: u64::MAX,
                audio: Some(2),
                midi: None,
                gain: 0.5,
                balance: 0.25,
            },
        },
        Request::ReplacementEdit {
            id: 1,
            edit: ReplacementEdit::Connect {
                port: EnginePort::Strip(StripPort {
                    strip: strip(0),
                    kind: PortKind::AudioOut,
                    index: 0,
                }),
                host: HostPortId(2),
            },
        },
        Request::ReplacementEdit {
            id: 1,
            edit: ReplacementEdit::Route {
                port: StripPort {
                    strip: strip(0),
                    kind: PortKind::AudioOut,
                    index: 1,
                },
                bus: BusPort {
                    bus: BusId::MASTER,
                    index: 1,
                },
            },
        },
        Request::ReplacementEdit {
            id: 1,
            edit: ReplacementEdit::BusControl {
                bus: BusId::MASTER,
                control: BusControl::Mute(true),
            },
        },
        Request::ReplacementPrepare { id: 1 },
        Request::ReplacementCommit { id: 1 },
        Request::ReplacementCancel { id: 1 },
        Request::ContentBegin(ContentDescription {
            id: 1,
            midi: false,
            frames: 64,
            bytes: 512,
        }),
        Request::ContentWrite {
            id: 1,
            offset: 0,
            data: ContentChunk::new(&[42; 1024]).unwrap(),
        },
        Request::ContentFinish { id: 1 },
        Request::ContentApply {
            loop_id: loop_id(0),
            frames: 4,
            audio: Some(1),
            midi: Some(2),
        },
        Request::ContentInitial {
            id: 1,
            message: MidiMessage::new(&[0xb0, 7, 64]).unwrap(),
        },
        Request::ContentRelease { id: 1 },
        Request::PreviewStart {
            request_id: 42,
            bus: BusId::MASTER,
            content: 81,
            frames: 12_000,
        },
        Request::PreviewStop,
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
                bus: BusId::MASTER,
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
                bus: BusId::MASTER,
                index: 0,
            },
            connected: false,
        },
        Request::BusControl {
            bus: BusId::MASTER,
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
        ClientMessage::Bulk(BulkRead {
            id: 6,
            loop_id: loop_id(2),
            selector: BulkSelector::AudioFrames,
            offset: 128,
        }),
        ClientMessage::Bulk(BulkRead {
            id: 7,
            loop_id: loop_id(2),
            selector: BulkSelector::MidiInitial { channel: 15 },
            offset: 0,
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
        EngineMessage::Telemetry(Telemetry::Preview { request_id: 42 }),
        EngineMessage::Telemetry(Telemetry::Bus {
            bus: BusId::MASTER,
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
                bus: BusId::MASTER,
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
                bus: BusId::MASTER,
                index: 1,
            },
        }),
        EngineMessage::State(StateRecord::Bus {
            bus: BusId::MASTER,
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
        EngineMessage::State(StateRecord::BusShape {
            bus: BusId(SlotId::new(7, 42)),
            channels: 8,
        }),
        EngineMessage::State(StateRecord::BusGenerations { generation: 42 }),
        EngineMessage::State(StateRecord::Generations {
            strips: u32::MAX,
            loops: 7,
        }),
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
    let bulk = engine_samples()
        .into_iter()
        .find(|message| matches!(message, EngineMessage::Bulk(_)))
        .unwrap();
    assert_eq!(
        encode_engine(&bulk, &mut [0; 64]),
        Err(CodecError::Overflow)
    );
    let connect = ClientMessage::Request {
        correlation: 1,
        request: Request::Connect {
            port: EnginePort::Bus(BusPort {
                bus: BusId::MASTER,
                index: 7,
            }),
            host: HostPortId(2),
            connected: true,
        },
    };
    let len = encode_client(&connect, &mut buf).unwrap();
    let tag = buf[..len]
        .windows(10)
        .position(|bytes| bytes == [1, 0, 0, 0, 0, 1, 0, 0, 0, 7])
        .unwrap();
    let mut record = buf[..len].to_vec();
    record[tag] = 2;
    assert_eq!(decode_client(&record), Err(CodecError::Invalid));
}
