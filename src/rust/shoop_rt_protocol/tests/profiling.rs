use shoop_rt_protocol::{codec::*, profiling::*, *};
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
    let messages = [
        EngineMessage::Profiling(packet),
        EngineMessage::ProfilingStatus(Status {
            generation: 7,
            requested: true,
            active: true,
            available: true,
            resolution_ns: 20,
            dropped: 19,
            skipped_captures: 2,
            high_water: 63,
            storage_bytes: 4_000_000,
        }),
        EngineMessage::ProfilingNode {
            generation: 7,
            key: 2,
            name: Name::new("Loop 3 MIDI playback").unwrap(),
        },
        EngineMessage::ProfilingGraph {
            generation: 7,
            epoch: 9,
            record: GraphRecord::Begin {
                nodes: 2,
                edges: 1,
                sample_rate: 48000,
                quantum: 32,
            },
        },
        EngineMessage::ProfilingGraph {
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
        },
        EngineMessage::ProfilingGraph {
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
        },
        EngineMessage::ProfilingGraph {
            generation: 7,
            epoch: 9,
            record: GraphRecord::End,
        },
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
            request: Request::Profiling(enabled),
        };
        let size = encode_client(&message, &mut bytes).unwrap();
        assert_eq!(decode_client(&bytes[..size]).unwrap(), message);
    }
    packet.len = (CHUNK_SAMPLES + 1) as u8;
    assert!(encode_engine(&EngineMessage::Profiling(packet), &mut bytes).is_err());
    packet.len = 1;
    packet.index = u32::MAX;
    assert!(encode_engine(&EngineMessage::Profiling(packet), &mut bytes).is_err());
    packet.index = 0;
    packet.total = MAX_SAMPLES as u32 + 1;
    assert!(encode_engine(&EngineMessage::Profiling(packet), &mut bytes).is_err());
}
