mod support;

use rt_runtime::buffer::MidiBuffer;
use rt_runtime::midi::{Message, MessageKind, ResetState};
use rt_runtime::Error;
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn all_midi_statuses_accept_only_their_complete_message_lengths() {
    assert_no_heap(|| {
        for status in 0u8..=255 {
            let expected = match status {
                0x80..=0xbf | 0xe0..=0xef | 0xf2 => Some(3),
                0xc0..=0xdf | 0xf1 | 0xf3 => Some(2),
                0xf6 | 0xf8 | 0xfa..=0xfc | 0xfe | 0xff => Some(1),
                _ => None,
            };
            let packet = [status, 1, 2, 3];
            for len in 0..=4 {
                assert_eq!(
                    Message::parse(&packet[..len], 4).is_ok(),
                    expected == Some(len),
                    "status {status:02x}, len {len}"
                );
            }
        }
    });
}

#[shoop_test]
fn note_classification_handles_zero_velocity_and_all_channels() {
    assert_no_heap(|| {
        for channel in 0..16 {
            assert_eq!(
                Message::parse(&[0x90 | channel, 60, 100], 3)
                    .unwrap()
                    .kind(),
                MessageKind::NoteOn {
                    channel,
                    key: 60,
                    velocity: 100
                }
            );
            assert_eq!(
                Message::parse(&[0x90 | channel, 60, 0], 3).unwrap().kind(),
                MessageKind::NoteOff {
                    channel,
                    key: 60,
                    velocity: 0
                }
            );
            assert_eq!(
                Message::parse(&[0x80 | channel, 60, 64], 3).unwrap().kind(),
                MessageKind::NoteOff {
                    channel,
                    key: 60,
                    velocity: 64
                }
            );
            assert_eq!(
                Message::parse(&[0xb0 | channel, 64, 127], 3)
                    .unwrap()
                    .kind(),
                MessageKind::ControlChange {
                    channel,
                    controller: 64,
                    value: 127
                }
            );
        }
        for invalid in [[0x90, 0x80, 1], [0x90, 1, 0x80], [0xf2, 1, 0xff]] {
            assert_eq!(Message::parse(&invalid, 3), Err(Error::InvalidEvent));
        }
    });
}

#[shoop_test]
fn sysex_is_bounded_complete_and_rejects_embedded_status_bytes() {
    let packet = [0xf0, 0x7d, 1, 2, 3, 0xf7];
    assert_no_heap(|| {
        assert_eq!(Message::parse(&packet, 5), Err(Error::Capacity));
        let parsed = Message::parse(&packet, 6).unwrap();
        assert_eq!(parsed.kind(), MessageKind::SysEx);
        assert_eq!(parsed.data(), packet);
        assert_eq!(
            Message::parse(&[0xf0, 0xf7], 2).unwrap().kind(),
            MessageKind::SysEx
        );
        assert_eq!(Message::parse(&[0xf0], 1), Err(Error::InvalidEvent));
        assert_eq!(Message::parse(&[0xf0, 0, 1], 3), Err(Error::InvalidEvent));
        assert_eq!(
            Message::parse(&[0xf0, 0xf8, 0xf7], 3),
            Err(Error::InvalidEvent)
        );
        assert_eq!(
            Message::parse(&[0xf0, 0xf7, 0xf7], 3),
            Err(Error::InvalidEvent)
        );
        assert_eq!(Message::parse(&[0xf7], 1), Err(Error::InvalidEvent));
    });
}

#[shoop_test]
fn midi_writer_rejects_malformed_messages_without_mutating_prior_events() {
    let mut buffer = MidiBuffer::new(2, 8).unwrap();
    assert_no_heap(|| {
        buffer.push(0, 8, &[0x90, 60, 100]).unwrap();
        assert_eq!(buffer.push(1, 8, &[0x80, 60]), Err(Error::InvalidEvent));
        assert_eq!(
            buffer.insert(1, 8, &[0x90, 60, 0x80]),
            Err(Error::InvalidEvent)
        );
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.byte_len(), 3);
        assert_eq!(buffer.get(0), Some((0, &[0x90, 60, 100][..])));
        buffer.push(2, 8, &[0x80, 60, 0]).unwrap();
        assert_eq!(buffer.push(3, 8, &[0xf8]), Err(Error::Full));
        assert_eq!(buffer.len(), 2);
    });
}

#[shoop_test]
fn ordered_insert_append_and_middle_paths_match_stable_scalar_sort_with_owning_reuse() {
    let mut buffer = MidiBuffer::new(32, 96).unwrap();
    let mut seed = 0x95c3_187a_d014_b762u64;
    for round in 0..64 {
        buffer.clear();
        let mut expected = Vec::new();
        for sequence in 0..40u8 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let offset = match round % 4 {
                0 => u32::from(sequence / 5),
                1 => 7 - u32::from(sequence / 5),
                2 => 3,
                _ => (seed >> 32) as u32 % 8,
            };
            let packet = [0x90, sequence, round as u8 + 1];
            let result = assert_no_heap(|| buffer.insert(offset, 8, &packet));
            if expected.len() == 32 {
                assert_eq!(result, Err(Error::Full));
            } else {
                result.unwrap();
                expected.push((offset, packet));
                expected.sort_by_key(|entry| entry.0);
            }
            assert_eq!(buffer.len(), expected.len());
            assert_eq!(buffer.byte_len(), expected.len() * 3);
            for (index, (offset, packet)) in expected.iter().enumerate() {
                assert_eq!(buffer.get(index), Some((*offset, packet.as_slice())));
            }
        }
        assert_eq!(
            assert_no_heap(|| buffer.insert(8, 8, &[0x90, 1, 1])),
            Err(Error::InvalidEvent)
        );
        assert_eq!(buffer.len(), 32);
    }
}
#[shoop_test]
fn reset_recovery_is_reserved_bounded_and_backpressure_does_not_skip_notes() {
    let mut reset = ResetState::default();
    let mut sink = MidiBuffer::new(1, 3).unwrap();
    let mut emitted = [[0u8; 3]; ResetState::EVENTS];
    assert_no_heap(|| {
        assert_eq!(reset.acknowledge(), Err(Error::InvalidState));
        reset.request();
        for index in 0..ResetState::EVENTS {
            let message = reset.peek().unwrap();
            assert_eq!(reset.remaining(), ResetState::EVENTS - index);
            sink.push(0, 8, &message).unwrap();
            emitted[index] = message;
            reset.acknowledge().unwrap();
            if let Some(next) = reset.peek() {
                assert_eq!(sink.push(0, 8, &next), Err(Error::Full));
                assert_eq!(reset.peek(), Some(next));
                reset.request(); // Repeated overflow must not restart recovery.
                assert_eq!(reset.peek(), Some(next));
            }
            sink.clear();
        }
        assert!(!reset.pending());
        assert_eq!(reset.peek(), None);
        assert_eq!(reset.remaining(), 0);
        assert_eq!(reset.acknowledge(), Err(Error::InvalidState));
        reset.request();
        assert_eq!(reset.remaining(), ResetState::EVENTS);
    });
    for channel in 0..16 {
        assert_eq!(emitted[channel * 2], [0xb0 | channel as u8, 120, 0]);
        assert_eq!(emitted[channel * 2 + 1], [0xb0 | channel as u8, 123, 0]);
    }
}
