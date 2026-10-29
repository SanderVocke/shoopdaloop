mod support;
#[allow(dead_code)]
#[path = "../../../../tests/rt_runtime_host/src/wire.rs"]
mod wire;

use rt_runtime::graph::{Connection, Endpoint, NodeStep, PortType};
use rt_runtime::{Error, NodeId};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
use wire::*;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

struct Random(u32);
impl Random {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
}
fn reference(packet: &Packet, slot: usize, stream: u64) -> Result<(), Error> {
    if !(48..=512).contains(&packet.len) {
        return Err(Error::InvalidShape);
    }
    let p = &packet.bytes;
    let number = |start: usize, count: usize| {
        (0..count).fold(0u64, |value, n| {
            value | (u64::from(p[start + n]) << (8 * n))
        })
    };
    if p[0] != 82
        || p[1] != 84
        || p[2] != 82
        || p[3] != 49
        || number(4, 2) != 1
        || p[7] > 1
        || number(16, 8) != stream
        || number(40, 4) != (packet.len - 48) as u64
        || number(44, 2) != slot as u64
        || p[46] != 0
        || p[47] != 0
    {
        Err(Error::InvalidEvent)
    } else if number(8, 8) == 0 {
        Err(Error::InvalidState)
    } else {
        Ok(())
    }
}

#[shoop_test]
fn seeded_wire_headers_match_an_independent_scalar_reference_without_heap_work() {
    let iterations = if cfg!(miri) { 32 } else { 2048 };
    for seed in 1..=8 {
        let mut random = Random(0x9e37_79b9 ^ seed);
        assert_no_heap(|| {
            for index in 0..iterations {
                let mut bytes = [0; BYTES];
                for byte in &mut bytes {
                    *byte = random.next() as u8;
                }
                let slot = random.next() as usize % SLOTS;
                let mut packet = Packet {
                    bytes,
                    len: random.next() as usize % (BYTES + 32),
                };
                if index % 2 == 0 {
                    packet = Packet::new(
                        random.next() as u8,
                        slot,
                        u64::from(random.next()),
                        u64::from(random.next()),
                        u64::from(random.next()),
                    );
                    packet.body(&bytes[..random.next() as usize % (BYTES - HEADER + 1)]);
                    if index % 4 == 0 {
                        let position = random.next() as usize % HEADER;
                        packet.bytes[position] ^= 1 << (random.next() % 8);
                    }
                }
                assert_eq!(
                    packet.header(slot, STREAM).map(|_| ()),
                    reference(&packet, slot, STREAM)
                );
                assert_eq!(
                    packet.header(slot, STREAM + 1).map(|_| ()),
                    reference(&packet, slot, STREAM + 1)
                );
                assert_eq!(
                    packet.header(SLOTS, STREAM).map(|_| ()),
                    reference(&packet, SLOTS, STREAM)
                );
                if let Ok(header) = packet.header(slot, STREAM) {
                    assert_eq!(header.op, packet.bytes[6]);
                    assert_eq!(header.cancelled, packet.bytes[7] == 1);
                    assert_eq!(
                        header.sequence,
                        u64::from_le_bytes(packet.bytes[8..16].try_into().unwrap())
                    );
                    assert_eq!(
                        header.epoch,
                        u64::from_le_bytes(packet.bytes[24..32].try_into().unwrap())
                    );
                    assert_eq!(
                        header.frame,
                        u64::from_le_bytes(packet.bytes[32..40].try_into().unwrap())
                    );
                }
            }
        });
    }
}

#[shoop_test]
fn seeded_relocatable_records_round_trip_and_truncation_never_reads_outside_input() {
    let iterations = if cfg!(miri) { 16 } else { 4096 };
    let mut random = Random(0xa138_901f);
    assert_no_heap(|| {
        for _ in 0..iterations {
            let node = NodeId::from_parts(random.next() % 1024, random.next().max(1)).unwrap();
            let endpoint = Endpoint {
                node: if random.next() & 1 == 0 {
                    None
                } else {
                    Some(node)
                },
                kind: if random.next() & 1 == 0 {
                    PortType::Audio
                } else {
                    PortType::Midi
                },
                port: random.next() % 65536,
            };
            let destination = Endpoint {
                node: Some(node),
                kind: endpoint.kind,
                port: random.next() % 65536,
            };
            let edge = Connection {
                source: endpoint,
                destination,
            };
            let step = NodeStep {
                node,
                input_end: random.next() as usize,
            };
            let mut record = [0u8; 24];
            encode_edge(edge, &mut record);
            assert_eq!(decode_edge(&record), Ok(edge));
            assert_eq!(decode_endpoint(&record[..12]), Ok(endpoint));
            for length in 0..24 {
                assert_eq!(decode_edge(&record[..length]), Err(Error::InvalidShape));
            }
            let mut encoded_step = [0; 12];
            encode_step(step, &mut encoded_step);
            let decoded = decode_step(&encoded_step).unwrap();
            assert_eq!(decoded.node, step.node);
            assert_eq!(decoded.input_end, step.input_end);
            for length in 0..12 {
                assert_eq!(
                    decode_step(&encoded_step[..length]).err(),
                    Some(Error::InvalidShape)
                );
            }
            let mut malformed = record;
            malformed[8] = 2;
            assert_eq!(decode_edge(&malformed), Err(Error::PortType));
            malformed = record;
            malformed[9] = 1;
            assert_eq!(decode_edge(&malformed), Err(Error::InvalidEvent));
            malformed = record;
            put32(&mut malformed, 12, u32::MAX);
            put32(&mut malformed, 16, 1);
            assert_eq!(decode_edge(&malformed), Err(Error::InvalidId));
            assert_eq!(get32(&record, usize::MAX), Err(Error::Capacity));
            assert_eq!(get64(&record, usize::MAX - 3), Err(Error::Capacity));
            for offset in 0..32 {
                assert_eq!(get32(&record, offset).is_ok(), offset + 4 <= record.len());
                assert_eq!(get64(&record, offset).is_ok(), offset + 8 <= record.len());
            }
        }
    });
}

#[shoop_test]
fn every_header_field_boundary_is_an_explicit_refusal_not_a_truncated_identity() {
    assert_no_heap(|| {
        for length in [0, 1, HEADER - 1, BYTES + 1, usize::MAX] {
            let mut packet = Packet::new(STATUS, 0, 1, 0, 0);
            packet.len = length;
            assert_eq!(packet.header(0, STREAM).err(), Some(Error::InvalidShape));
        }
        let mut packet = Packet::new(STATUS, 0, 0, 0, 0);
        assert_eq!(packet.header(0, STREAM).err(), Some(Error::InvalidState));
        put64(&mut packet.bytes, 8, u64::MAX);
        assert_eq!(packet.header(0, STREAM).unwrap().sequence, u64::MAX);
        for (offset, replacement) in [
            (0, 0),
            (4, 2),
            (5, 1),
            (7, 2),
            (16, 0),
            (40, 1),
            (44, 1),
            (46, 1),
            (47, 1),
        ] {
            let mut malformed = packet;
            malformed.bytes[offset] = replacement;
            assert_eq!(malformed.header(0, STREAM).err(), Some(Error::InvalidEvent));
        }
        for length in 0..HEADER {
            assert!(get64(&packet.bytes[..length], HEADER - 8).is_err());
        }
    });
}
