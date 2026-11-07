#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use rt_runtime::{buffer::MidiBuffer, midi_state::MidiState, Error};
use shoop_loops::midi_restore::*;
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Voice {
    velocity: u8,
    held: bool,
    latched: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Keys {
    voices: [Voice; 128],
    pedals: [bool; 3],
}
impl Default for Keys {
    fn default() -> Self {
        Self {
            voices: [Voice::default(); 128],
            pedals: [false; 3],
        }
    }
}
impl Keys {
    fn packet(&mut self, packet: &[u8]) {
        if packet[0] & 15 != 0 {
            return;
        }
        match packet[0] {
            0x90 if packet[2] != 0 => {
                self.voices[packet[1] as usize].held = true;
                self.voices[packet[1] as usize].velocity = packet[2];
            }
            0x80 | 0x90 => self.voices[packet[1] as usize].held = false,
            0xb0 => {
                if let Some(index) = [64, 66, 69].iter().position(|cc| *cc == packet[1]) {
                    let down = packet[2] >= 64;
                    if index == 1 && self.pedals[index] != down {
                        for voice in &mut self.voices {
                            voice.latched = down && voice.held;
                        }
                    }
                    self.pedals[index] = down;
                }
            }
            _ => {}
        }
        for voice in &mut self.voices {
            if !voice.held && !voice.latched && !self.pedals[0] && !self.pedals[2] {
                voice.velocity = 0;
            }
        }
    }
}
fn buffer() -> MidiBuffer {
    MidiBuffer::new(RESTORE_MAX_EVENTS, RESTORE_MAX_EVENTS * 3).unwrap()
}
#[shoop_test]
fn cleanup_and_sostenuto_restore_match_an_independent_key_pedal_machine() {
    for hold in [64, 69] {
        let mut source = MidiState::default();
        let mut device = Keys::default();
        let mut output = buffer();
        assert_no_heap(|| {
            for packet in [
                [0x90, 60, 100],
                [0xb0, 66, 127],
                [0x90, 61, 90],
                [0xb0, hold, 127],
                [0x80, 60, 0],
                [0x80, 61, 0],
                [0x90, 62, 80],
            ] {
                source.apply_packet(&packet, 3).unwrap();
                device.packet(&packet);
            }
            let expected = device;
            let unchanged = source;
            let report = cleanup(&source, |packet| {
                device.packet(packet);
                output.push(0, 1, packet)
            });
            assert_eq!(report.first_error, None);
            assert_eq!(report.accepted, 5);
            assert!(device
                .voices
                .iter()
                .all(|v| v.velocity == 0 && !v.held && !v.latched));
            output.clear();
            let report = restore(&source, |packet| {
                device.packet(packet);
                output.push(0, 1, packet)
            });
            assert_eq!(report.first_error, None);
            assert_eq!(device, expected);
            assert_eq!(source, unchanged);
            let locate = |data: &[u8]| {
                (0..output.len())
                    .find(|i| output.get(*i).unwrap().1 == data)
                    .unwrap()
            };
            assert!(locate(&[0x90, 60, 100]) < locate(&[0xb0, 66, 127]));
            assert!(locate(&[0xb0, 66, 127]) < locate(&[0x90, 61, 90]));
            assert!(locate(&[0x90, 61, 90]) < locate(&[0x80, 61, 0]));
        });
    }
}
#[shoop_test]
fn program_bank_pending_selectors_modes_and_parameter_order_are_not_conflated() {
    let mut source = MidiState::default();
    let mut output = buffer();
    assert_no_heap(|| {
        source.apply_packet(&[0xff], 3).unwrap();
        source.apply_packet(&[0xb0, 121, 0], 3).unwrap();
        for packet in [[0xb0, 0, 1], [0xb0, 32, 2]] {
            source.apply_packet(&packet, 3).unwrap();
        }
        source.apply_packet(&[0xc0, 10], 3).unwrap();
        for packet in [
            [0xb0, 0, 3],
            [0xb0, 32, 4],
            [0xb0, 101, 5],
            [0xb0, 100, 6],
            [0xb0, 6, 12],
            [0xb0, 38, 1],
            [0xb0, 96, 0],
            [0xb0, 97, 0],
            [0xb0, 99, 7],
            [0xb0, 98, 8],
            [0xb0, 125, 0],
            [0xb0, 124, 0],
            [0xb0, 127, 0],
            [0xb0, 126, 2],
        ] {
            source.apply_packet(&packet, 3).unwrap();
        }
        let mut bank = [99, 99];
        let mut loaded = None;
        let mut selectors = [[9, 9], [8, 8]];
        let mut family = 0;
        let mut parameter_writes = 0;
        let report = restore(&source, |packet| {
            output.push(0, 1, packet)?;
            match packet[0] {
                0xff => {
                    bank = [0, 0];
                    selectors = [[127; 2]; 2];
                }
                0xc0 => loaded = Some((packet[1], bank)),
                0xb0 => match packet[1] {
                    0 => bank[0] = packet[2],
                    32 => bank[1] = packet[2],
                    121 => {
                        selectors = [[127; 2]; 2];
                        family = 0;
                    }
                    101 => {
                        family = 0;
                        selectors[0][0] = packet[2];
                    }
                    100 => {
                        family = 0;
                        selectors[0][1] = packet[2];
                    }
                    99 => {
                        family = 1;
                        selectors[1][0] = packet[2];
                    }
                    98 => {
                        family = 1;
                        selectors[1][1] = packet[2];
                    }
                    6 | 38 | 96 | 97 => {
                        if selectors[family] != [127, 127] {
                            parameter_writes += 1;
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
            Ok(())
        });
        assert_eq!(report.first_error, None);
        assert!(report.parameter_history_untracked);
        assert_eq!(output.get(0).unwrap().1, &[0xff]);
        assert_eq!(loaded, Some((10, [1, 2])));
        assert_eq!(bank, [3, 4]);
        assert_eq!(selectors, [[5, 6], [7, 8]]);
        assert_eq!(family, 1);
        assert_eq!(parameter_writes, 0);
        for i in 0..output.len() {
            let (_, packet) = output.get(i).unwrap();
            if packet[0] == 0xb0 {
                assert!(!matches!(packet[1], 96 | 97 | 125 | 127));
            }
        }
        assert!((0..output.len()).any(|i| output.get(i).unwrap().1 == [0xb0, 124, 0]));
        assert!((0..output.len()).any(|i| output.get(i).unwrap().1 == [0xb0, 126, 2]));
    });
}
#[shoop_test]
fn fixed_emission_bounds_cover_all_channels_keys_controls_and_pressure() {
    let mut source = MidiState::default();
    let mut restored = MidiState::default();
    let mut output = buffer();
    assert_no_heap(|| {
        source.apply_packet(&[0xff], 3).unwrap();
        for channel in 0..16 {
            source.apply_packet(&[0xb0 | channel, 121, 0], 3).unwrap();
            for cc in 0..128 {
                if !matches!(cc, 120 | 121 | 123) {
                    source.apply_packet(&[0xb0 | channel, cc, 1], 3).unwrap();
                }
            }
            source.apply_packet(&[0xc0 | channel, 12], 3).unwrap();
            source.apply_packet(&[0xd0 | channel, 64], 3).unwrap();
            source.apply_packet(&[0xb0 | channel, 64, 127], 3).unwrap();
            source.apply_packet(&[0xb0 | channel, 69, 127], 3).unwrap();
            source.apply_packet(&[0xb0 | channel, 66, 0], 3).unwrap();
            for key in 0..128 {
                source.apply_packet(&[0x90 | channel, key, 100], 3).unwrap();
            }
            source.apply_packet(&[0xb0 | channel, 66, 127], 3).unwrap();
            for key in 0..128 {
                source.apply_packet(&[0x80 | channel, key, 0], 3).unwrap();
                source.apply_packet(&[0xa0 | channel, key, 77], 3).unwrap();
            }
        }
        let report = restore(&source, |packet| {
            restored.apply_packet(packet, 3)?;
            output.push(0, 1, packet)
        });
        assert_eq!(report.first_error, None);
        assert_eq!(report.attempted, report.accepted);
        assert!(report.accepted <= RESTORE_MAX_EVENTS);
        assert!(report.accepted > 16 * 128 * 3);
        for channel in 0..16 {
            let expected = source.channel(channel).unwrap();
            let actual = restored.channel(channel).unwrap();
            for key in 0..128 {
                assert_eq!(actual.note(key), expected.note(key));
            }
            for cc in 0..128 {
                if let Some(value) = expected.controller(cc) {
                    assert_eq!(actual.controller(cc), Some(value));
                }
            }
            assert_eq!(actual.program(), expected.program());
            assert_eq!(actual.program_bank(), expected.program_bank());
            assert_eq!(actual.pitch_bend(), expected.pitch_bend());
            assert_eq!(actual.pressure(), expected.pressure());
            assert_eq!(actual.parameter_family(), expected.parameter_family());
        }
        output.clear();
        let report = cleanup(&source, |packet| output.push(0, 1, packet));
        assert_eq!(report.accepted, CLEANUP_MAX_EVENTS);
        assert_eq!(report.first_error, None);
    });
}
#[shoop_test]
fn sink_failure_is_explicit_and_never_retries_or_mutates_the_source() {
    let mut source = MidiState::default();
    source.apply_packet(&[0x90, 60, 99], 3).unwrap();
    let expected = source;
    let complete = assert_no_heap(|| restore(&source, |_| Ok(())));
    for capacity in [0, 1, 5] {
        let mut output = MidiBuffer::new(capacity, capacity * 3).unwrap();
        let mut calls = 0;
        assert_no_heap(|| {
            let report = restore(&source, |packet| {
                calls += 1;
                output.push(0, 1, packet)
            });
            assert_eq!(report.first_error, Some(Error::Full));
            assert_eq!(report.accepted, capacity);
            assert_eq!(report.attempted, complete.attempted);
            assert_eq!(calls, capacity + 1);
            assert_eq!(source, expected);
            assert!(!output.is_recovery());
        });
    }
    assert_no_heap(|| {
        let mut calls = 0;
        let report = restore(&source, |_| {
            calls += 1;
            Err(Error::Disconnected)
        });
        assert_eq!(calls, 1);
        assert_eq!(report.first_error, Some(Error::Disconnected));
        assert_eq!(report.attempted, complete.attempted);
    });
}
