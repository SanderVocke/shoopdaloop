use shoop_dsp::{DelayCopy, MidiEvent};
use shoop_remote_dsp::{stream::Stream, wire, Config, Error};
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;
fn config() -> Config {
    Config {
        packet_frames: 4,
        multiplier: 4,
        allowance: 8,
        channels: 2,
        ..Config::default()
    }
}
fn packet(c: Config, epoch: u64, sequence: u64, raw: &mut [u8]) -> usize {
    let audio: Vec<f32> = (0..c.channels as usize)
        .flat_map(|ch| {
            (0..c.packet_frames as usize).map(move |i| {
                (ch * 10000 + sequence as usize * c.packet_frames as usize + i + 1) as f32
            })
        })
        .collect();
    let event = MidiEvent::new(c.packet_frames as u32 - 1, 0, &[0x90, 60, 100]).unwrap();
    wire::encode(
        wire::Header::input(epoch, sequence, c),
        c,
        &audio,
        &[event],
        raw,
    )
    .unwrap()
}
#[shoop_wasm_test_support::shoop_test]
fn negotiation_and_wire_boundaries_are_explicit() {
    for k in [1, 2, 4, 8] {
        for n in [1, 2, 4, 8, 16] {
            let c = Config {
                multiplier: k,
                allowance: n,
                ..Config::default()
            };
            assert_eq!(c.validate().is_ok(), n >= k);
            if n >= k {
                assert_eq!(
                    c.deadline_ns(0).unwrap(),
                    c.ready_ns(0).unwrap() + n as u64 * c.packet_ns()
                );
            }
        }
    }
    assert!(Config {
        channels: 16,
        packet_frames: 16,
        ..Config::default()
    }
    .validate()
    .is_ok());
    assert_eq!(
        Config {
            channels: 16,
            packet_frames: 32,
            ..Config::default()
        }
        .validate(),
        Err(Error::Mtu)
    );
    let c = config();
    let mut raw = [0; 1452];
    let n = packet(c, 7, 0, &mut raw);
    assert_eq!(&raw[..8], b"SDSP\x01\0\x01\0");
    for end in 0..n {
        assert!(wire::decode(&raw[..end], c).is_err());
    }
    for offset in [0, 4, 6, 7, 24, 32, 34, 36, 38, 47, 48, 56] {
        let mut bad = raw;
        bad[offset] ^= 0x80;
        assert!(wire::decode(&bad[..n], c).is_err(), "offset {offset}");
    }
    let decoded = wire::decode(&raw[..n], c).unwrap();
    assert_eq!(decoded.midi(0).frame, 3);
}
#[shoop_wasm_test_support::shoop_test]
fn aggregation_reorders_once_and_preserves_samples_and_midi_without_allocating() {
    let c = config();
    let mut stream = Stream::prepare(c, 7).unwrap();
    let mut dsp = DelayCopy::prepare(c.dsp(), 1).unwrap();
    let mut packets = [[0; 1452]; 8];
    let mut lengths = [0; 8];
    for i in 0..8 {
        lengths[i] = packet(c, 7, i as u64, &mut packets[i]);
    }
    let mut received = [[0; 1452]; 8];
    let mut received_lengths = [0; 8];
    let mut emitted = 0;
    for block in 0..2 {
        for j in [3, 1, 0, 2] {
            let i = block * 4 + j;
            assert_no_alloc::assert_no_alloc(|| {
                stream.accept(&packets[i][..lengths[i]], 0).unwrap()
            });
        }
        assert_eq!(
            stream.accept(&packets[block * 4][..lengths[block * 4]], 0),
            Err(Error::Duplicate)
        );
        assert_no_alloc::assert_no_alloc(|| {
            stream
                .process_one(
                    0,
                    &mut dsp,
                    || 0,
                    |raw| {
                        received[emitted][..raw.len()].copy_from_slice(raw);
                        received_lengths[emitted] = raw.len();
                        emitted += 1;
                        Ok(())
                    },
                )
                .unwrap()
        });
    }
    for sequence in 0..8 {
        let decoded = wire::decode(&received[sequence][..received_lengths[sequence]], c).unwrap();
        assert_eq!(decoded.header.sequence, sequence as u64);
        assert_eq!(decoded.header.frame, (sequence * 4) as u64);
        assert_eq!(decoded.midi(0).frame, 3);
        assert_eq!(decoded.midi(0).bytes().unwrap(), &[0x90, 60, 100]);
        for ch in 0..2 {
            for i in 0..4 {
                let n = sequence * 4 + i;
                assert_eq!(
                    decoded.sample(ch, i),
                    if n == 0 { 0. } else { (ch * 10000 + n) as f32 }
                );
            }
        }
    }
    assert_eq!(stream.stats().blocks, 2);
    assert_eq!(
        stream.accept(&packets[0][..lengths[0]], 0),
        Err(Error::Stale)
    );
}
#[shoop_wasm_test_support::shoop_test]
fn missing_input_expires_at_the_boundary_and_advances_state_with_zero_and_reset() {
    let c = config();
    let mut stream = Stream::prepare(c, 1).unwrap();
    let mut dsp = DelayCopy::prepare(c.dsp(), 0).unwrap();
    let mut raw = [0; 1452];
    for seq in [0, 2, 3] {
        let n = packet(c, 1, seq, &mut raw);
        stream.accept(&raw[..n], 100).unwrap();
    }
    let due = 100 + 3 * c.packet_ns() + c.gap_timeout_us as u64 * 1000;
    assert!(!stream
        .process_one(due - 1, &mut dsp, || 0, |_| panic!("too early"))
        .unwrap());
    let mut outputs = 0;
    assert_no_alloc::assert_no_alloc(|| {
        assert!(stream
            .process_one(
                due,
                &mut dsp,
                || 0,
                |raw| {
                    let packet = wire::decode(raw, c).unwrap();
                    assert_eq!(packet.header.missing_mask, 2);
                    assert_eq!(
                        packet.header.flags & (wire::CONCEALED | wire::MIDI_RESET),
                        wire::CONCEALED | wire::MIDI_RESET
                    );
                    if packet.header.sequence == 1 {
                        assert_eq!(packet.sample(0, 0), 0.);
                    }
                    assert_eq!(packet.midi_len(), 0);
                    outputs += 1;
                    Ok(())
                }
            )
            .unwrap())
    });
    assert_eq!(outputs, 4);
    assert_eq!(stream.next_sequence(), 4);
    assert_eq!(stream.concealment(), &[(0, 2)]);
    assert!(!stream
        .process_one(due + 1, &mut dsp, || 0, |_| panic!("reprocessed"))
        .unwrap());
}
#[shoop_wasm_test_support::shoop_test]
fn malformed_stale_and_sustained_overload_cannot_change_the_timeline() {
    let c = config();
    let mut stream = Stream::prepare(c, 1).unwrap();
    let mut raw = [0; 1452];
    let n = packet(c, 2, 0, &mut raw);
    assert_eq!(stream.accept(&raw[..n], 0), Err(Error::Stale));
    assert_eq!(stream.next_sequence(), 0);
    for _ in 0..150 {
        let n = packet(c, 1, 128, &mut raw);
        let result = stream.accept(&raw[..n], 0);
        assert!(matches!(result, Err(Error::Future) | Err(Error::Retired)));
    }
    assert!(stream.stats().retired);
    assert_eq!(stream.next_sequence(), 0);
}

#[shoop_wasm_test_support::shoop_test]
fn presentation_has_fixed_slots_and_late_results_never_shift_following_audio() {
    use shoop_dsp::MidiBuffer;
    use shoop_remote_dsp::returns::{Outcome, Returns};
    let c = config();
    let mut returns = Returns::prepare(c, 9).unwrap();
    let mut raw = [0; 1452];
    let mut header = wire::Header::input(9, 1, c);
    header.kind = wire::OUTPUT;
    let n = wire::encode(header, c, &[0.5; 8], &[], &mut raw).unwrap();
    assert_no_alloc::assert_no_alloc(|| {
        returns
            .accept(&raw[..n], c.deadline_ns(1).unwrap())
            .unwrap()
    });
    let mut audio = [99.; 8];
    let mut midi = MidiBuffer::new(4);
    assert_eq!(
        returns.consume(0, c.deadline_ns(0).unwrap() - 1, &mut audio, &mut midi),
        Err(Error::Deadline)
    );
    let first = assert_no_alloc::assert_no_alloc(|| {
        returns
            .consume(0, c.deadline_ns(0).unwrap(), &mut audio, &mut midi)
            .unwrap()
    });
    assert_eq!(first.frame, 0);
    assert_eq!(first.outcome, Outcome::Silence);
    assert!(first.midi_reset);
    assert_eq!(audio, [0.; 8]);
    let second = assert_no_alloc::assert_no_alloc(|| {
        returns
            .consume(1, c.deadline_ns(1).unwrap(), &mut audio, &mut midi)
            .unwrap()
    });
    assert_eq!(second.frame, 4);
    assert_eq!(second.outcome, Outcome::Processed);
    assert_eq!(audio, [0.5; 8]);
    header.sequence = 0;
    header.frame = 0;
    let n = wire::encode(header, c, &[1.; 8], &[], &mut raw).unwrap();
    assert_eq!(returns.accept(&raw[..n], 0), Err(Error::Stale));
    header.sequence = 2;
    header.frame = 8;
    let n = wire::encode(header, c, &[1.; 8], &[], &mut raw).unwrap();
    assert_eq!(
        returns.accept(&raw[..n], c.deadline_ns(2).unwrap() + 1),
        Err(Error::Deadline)
    );
    assert_eq!(
        returns
            .consume(2, c.deadline_ns(2).unwrap(), &mut audio, &mut midi)
            .unwrap()
            .outcome,
        Outcome::Silence
    );
}

#[shoop_wasm_test_support::shoop_test]
fn expired_input_is_not_replayed_and_its_zeroes_remain_in_delayed_algorithm_state() {
    let c = config();
    let delay = c.dsp().frames + 3;
    let mut stream = Stream::prepare(c, 1).unwrap();
    let mut dsp = DelayCopy::prepare(c.dsp(), delay as u32).unwrap();
    let mut raw = [0; 1452];
    let due = 3 * c.packet_ns() + c.gap_timeout_us as u64 * 1000;
    for batch in 0..3 {
        for j in 0..4 {
            let sequence = batch * 4 + j;
            if sequence == 1 {
                continue;
            }
            let n = packet(c, 1, sequence, &mut raw);
            stream.accept(&raw[..n], 0).unwrap();
        }
        assert_no_alloc::assert_no_alloc(|| {
            assert!(stream
                .process_one(
                    due,
                    &mut dsp,
                    || 0,
                    |raw| {
                        let output = wire::decode(raw, c).unwrap();
                        for ch in 0..2 {
                            for i in 0..4 {
                                let n = output.header.frame as usize + i;
                                let expected = if n < delay || (4..8).contains(&(n - delay)) {
                                    0.
                                } else {
                                    (ch * 10000 + n - delay + 1) as f32
                                };
                                assert_eq!(output.sample(ch, i), expected);
                            }
                        }
                        Ok(())
                    }
                )
                .unwrap())
        });
        let n = packet(c, 1, 1, &mut raw);
        assert_eq!(stream.accept(&raw[..n], due), Err(Error::Stale));
    }
    assert_eq!(stream.stats().blocks, 3);
    assert_eq!(stream.stats().concealed_packets, 1);
}
#[shoop_wasm_test_support::shoop_test]
fn algorithm_failure_retires_without_replaying_partly_modified_state() {
    use shoop_dsp::{Block, Info, Processor};
    struct Failing {
        calls: usize,
        inner: DelayCopy,
    }
    impl Processor for Failing {
        fn info(&self) -> Info {
            self.inner.info()
        }
        fn reset(&mut self) -> Result<(), shoop_dsp::Error> {
            self.inner.reset()
        }
        fn process(&mut self, _block: Block<'_>) -> Result<(), shoop_dsp::Error> {
            self.calls += 1;
            Err(shoop_dsp::Error::Algorithm)
        }
    }
    let c = config();
    let mut stream = Stream::prepare(c, 1).unwrap();
    let mut raw = [0; 1452];
    for seq in 0..4 {
        let n = packet(c, 1, seq, &mut raw);
        stream.accept(&raw[..n], 0).unwrap();
    }
    let mut dsp = Failing {
        calls: 0,
        inner: DelayCopy::prepare(c.dsp(), 0).unwrap(),
    };
    assert_eq!(
        stream.process_one(0, &mut dsp, || 0, |_| panic!("failed DSP emitted output")),
        Err(Error::Dsp)
    );
    assert_eq!(
        stream.process_one(0, &mut dsp, || 0, |_| panic!("retired DSP emitted output")),
        Err(Error::Retired)
    );
    assert_eq!(dsp.calls, 1);
    assert_eq!(stream.stats().fault, Some(Error::Dsp));
}
#[shoop_wasm_test_support::shoop_test]
fn midi_overflow_requests_reset_without_discarding_audio() {
    use shoop_dsp::{Block, Info, Processor};
    struct ManyMidi(DelayCopy);
    impl Processor for ManyMidi {
        fn info(&self) -> Info {
            self.0.info()
        }
        fn reset(&mut self) -> Result<(), shoop_dsp::Error> {
            self.0.reset()
        }
        fn process(&mut self, block: Block<'_>) -> Result<(), shoop_dsp::Error> {
            block.output.copy_from_slice(block.input);
            block.midi_output.clear();
            for note in 60..63 {
                block
                    .midi_output
                    .push(MidiEvent::new(0, 0, &[0x90, note, 100])?)?;
            }
            Ok(())
        }
    }
    let c = Config {
        midi_per_packet: 1,
        ..config()
    };
    let mut stream = Stream::prepare(c, 1).unwrap();
    let mut raw = [0; 1452];
    for seq in 0..4 {
        let n = packet(c, 1, seq, &mut raw);
        stream.accept(&raw[..n], 0).unwrap();
    }
    let mut dsp = ManyMidi(DelayCopy::prepare(c.dsp(), 0).unwrap());
    let mut count = 0;
    assert!(stream
        .process_one(
            0,
            &mut dsp,
            || 0,
            |raw| {
                let p = wire::decode(raw, c).unwrap();
                assert!(p.sample(0, 0) > 0.);
                if p.header.sequence == 0 {
                    assert_ne!(p.header.flags & wire::MIDI_RESET, 0);
                    assert_eq!(p.midi_len(), 0);
                }
                count += 1;
                Ok(())
            }
        )
        .unwrap());
    assert_eq!(count, 4);
}
#[shoop_wasm_test_support::shoop_test]
fn arbitrary_truncated_datagrams_cannot_panic_or_advance_state() {
    let c = config();
    let mut stream = Stream::prepare(c, 1).unwrap();
    let mut raw = [0; 1452];
    let mut seed = 1234567u64;
    for index in 0..5000 {
        for b in &mut raw {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *b = seed as u8;
        }
        let len = index % raw.len();
        assert!(stream.accept(&raw[..len], 0).is_err());
    }
    assert_eq!(stream.next_sequence(), 0);
    assert_eq!(stream.stats().invalid, 5000);
}
#[shoop_wasm_test_support::shoop_test]
fn long_source_timeline_does_not_overflow_the_intermediate_nanosecond_conversion() {
    let c = config();
    let sequence = 48_000u64 * 60 * 60 * 24 * 10 / c.packet_frames as u64;
    assert!(c.ready_ns(sequence).unwrap() > 8 * 24 * 60 * 60 * 1_000_000_000u64);
}

#[shoop_wasm_test_support::shoop_test]
fn stream_attachment_preserves_a_nonzero_absolute_source_frame() {
    use shoop_dsp::MidiBuffer;
    use shoop_remote_dsp::returns::Returns;
    let c = Config {
        start_frame: 1_000_003,
        multiplier: 1,
        ..config()
    };
    let mut stream = Stream::prepare(c, 3).unwrap();
    let mut returns = Returns::prepare(c, 3).unwrap();
    let mut dsp = DelayCopy::prepare(c.dsp(), 1).unwrap();
    let mut raw = [0; 1452];
    let mut audio = [0.; 8];
    let mut midi = MidiBuffer::new(4);
    let n = packet(c, 3, 0, &mut raw);
    stream.accept(&raw[..n], 0).unwrap();
    assert!(stream
        .process_one(
            0,
            &mut dsp,
            || 0,
            |raw| {
                let packet = wire::decode(raw, c).unwrap();
                assert_eq!(packet.header.frame, 1_000_003);
                returns.accept(raw, 0)
            }
        )
        .unwrap());
    let result = returns
        .consume(0, c.deadline_ns(0).unwrap(), &mut audio, &mut midi)
        .unwrap();
    assert_eq!(result.frame, 1_000_003);
    assert_eq!(audio[0], 0.);
    assert_eq!(audio[1], 1.);
}
