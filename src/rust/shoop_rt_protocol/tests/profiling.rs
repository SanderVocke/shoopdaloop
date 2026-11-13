use rt_profiling::wire::Record as AudioRecord;
use rt_profiling::*;
use shoop_rt_protocol::{codec::*, *};
use shoop_wasm_test_support::shoop_test;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn profiling_records_round_trip_and_reject_malformed_lengths() {
    let mut packet = Packet {
        generation: 7,
        capture: 3,
        sequence: 99,
        callback: 11,
        graphs: 3,
        callback_frame: 128,
        frame: 160,
        frames: 32,
        epoch: 9,
        start_ns: 100,
        end_ns: 200,
        total: CHUNK_SAMPLES as u32,
        len: CHUNK_SAMPLES as u8,
        flags: CAPTURE_END,
        trigger: HOST_XRUN,
        ..Packet::default()
    };
    for (i, sample) in packet.samples.iter_mut().enumerate() {
        *sample = Sample {
            key: i as u64,
            at: 101 + i as u64,
        };
    }
    let mut encoder = rt_profiling::wire::Encoder::default();
    let messages = [
        EngineMessage::Profiling(encoder.encode(AudioRecord::Packet(packet))),
        EngineMessage::Profiling(encoder.encode(AudioRecord::Status(Status {
            generation: 7,
            requested: true,
            active: true,
            available: true,
            resolution_ns: 20,
            dropped: 19,
            skipped_captures: 2,
            high_water: 63,
            storage_bytes: 4_000_000,
        }))),
        EngineMessage::Profiling(encoder.encode(AudioRecord::Node {
            generation: 7,
            key: 2,
            name: Name::new("Loop 3 MIDI playback").unwrap(),
        })),
        EngineMessage::Profiling(encoder.encode(AudioRecord::Graph {
            generation: 7,
            epoch: 9,
            record: GraphRecord::Begin {
                nodes: 2,
                edges: 1,
                sample_rate: 48000,
                quantum: 32,
            },
        })),
        EngineMessage::Profiling(encoder.encode(AudioRecord::Graph {
            generation: 7,
            epoch: 9,
            record: GraphRecord::Node {
                index: 0,
                node: GraphNode {
                    key: 1,
                    group: 2,
                    slot: 3,
                    owner_generation: 1,
                },
            },
        })),
        EngineMessage::Profiling(encoder.encode(AudioRecord::Graph {
            generation: 7,
            epoch: 9,
            record: GraphRecord::Edge {
                index: 0,
                edge: GraphEdge {
                    source: Port::default(),
                    destination: Port {
                        node: Some(1),
                        kind: 1,
                        channel: 2,
                    },
                },
            },
        })),
        EngineMessage::Profiling(encoder.encode(AudioRecord::Graph {
            generation: 7,
            epoch: 9,
            record: GraphRecord::End,
        })),
    ];
    let mut bytes = [0; MAX_RECORD_BYTES];
    for message in messages {
        let size = encode_engine(&message, &mut bytes).unwrap();
        assert_eq!(decode_engine(&bytes[..size]).unwrap(), message);
        assert_eq!(message.lane(), Lane::Profiling);
        for prefix in 0..size {
            assert!(decode_engine(&bytes[..prefix]).is_err());
        }
        bytes[size] = 0;
        assert!(decode_engine(&bytes[..size + 1]).is_err());
    }
    for enabled in [false, true] {
        let message = ClientMessage::Request {
            correlation: 1,
            request: Request::Diagnostic(rt_profiling::wire::subscribe(enabled)),
        };
        let size = encode_client(&message, &mut bytes).unwrap();
        assert_eq!(decode_client(&bytes[..size]).unwrap(), message);
    }
    packet.len = (CHUNK_SAMPLES + 1) as u8;
    assert!(AudioRecord::decode(AudioRecord::Packet(packet).encode(100)).is_none());
    packet.len = 1;
    packet.index = u32::MAX;
    assert!(AudioRecord::decode(AudioRecord::Packet(packet).encode(100)).is_none());
    packet.index = 0;
    packet.total = MAX_SAMPLES as u32 + 1;
    assert!(AudioRecord::decode(AudioRecord::Packet(packet).encode(100)).is_none());
}

#[shoop_test]
fn unknown_diagnostic_schema_is_opaque_to_engine_codec() {
    let message = EngineMessage::Profiling(profiling_protocol::Record::Data(
        profiling_protocol::Chunk {
            stream: profiling_protocol::StreamId {
                source: u32::MAX,
                epoch: u64::MAX,
                report_type: u32::MAX,
                schema: u16::MAX,
            },
            sequence: u64::MAX,
            offset: 0,
            total: 1024,
            payload: profiling_protocol::Bytes::new(&[255; 1024]).unwrap(),
        },
    ));
    let mut bytes = [0; MAX_RECORD_BYTES];
    let len = encode_engine(&message, &mut bytes).unwrap();
    assert_eq!(decode_engine(&bytes[..len]).unwrap(), message);
}
