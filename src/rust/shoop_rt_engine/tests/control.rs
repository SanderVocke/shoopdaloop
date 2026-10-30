//! The control endpoint's protocol behaviour: telemetry, bulk reads, state
//! resynchronization, credits, limit changes and the connection lifecycle,
//! plus render shutdown and stopped cleanup.

mod support;

use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use shoop_rt_engine::Phase;
use shoop_rt_protocol::carrier::Carrier;
use shoop_rt_protocol::*;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn transition(id: LoopSlotId, mode: LoopMode) -> Request {
    Request::LoopTransition {
        loop_id: id,
        mode,
        delay: None,
    }
}
fn connect(strip: StripId, kind: PortKind, host: HostPortId) -> Request {
    Request::Connect {
        port: port(strip, kind, 0),
        host,
        connected: true,
    }
}
fn records(bytes: &[u8]) -> Vec<(u64, Vec<u8>)> {
    let mut packets = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let frame = u64::from_le_bytes(rest[..8].try_into().unwrap());
        let len = u32::from_le_bytes(rest[8..12].try_into().unwrap()) as usize;
        packets.push((frame, rest[12..12 + len].to_vec()));
        rest = &rest[12 + len..];
    }
    packets
}

/// A monitored stereo strip with MIDI, routed to Master, and one loop.
fn configured() -> (Harness, StripId, LoopSlotId) {
    let mut harness = Harness::new(&description());
    harness.input = Box::new(|port, _| if port == 0 { 0.5 } else { -0.25 });
    let track = strip(1, 1);
    let looped = loop_id(3, 1);
    harness.done(Request::CreateStrip {
        strip: track,
        shape: shape(2, 2, true),
    });
    for (index, host) in [audio_in(0), audio_in(1)].into_iter().enumerate() {
        harness.done(Request::Connect {
            port: port(track, PortKind::AudioIn, index as u8),
            host,
            connected: true,
        });
    }
    harness.done(connect(track, PortKind::AudioOut, audio_out(0)));
    harness.done(connect(track, PortKind::MidiIn, midi_in()));
    harness.done(connect(track, PortKind::MidiOut, midi_out()));
    for control in [
        StripControl::InputGain(2.),
        StripControl::Monitoring(true),
        StripControl::OutputGain(0.5),
    ] {
        harness.done(Request::StripControl {
            strip: track,
            control,
        });
    }
    harness.done(Request::Route {
        strip: track,
        bus: BusId::Master,
        connected: true,
    });
    harness.done(Request::BusControl {
        bus: BusId::Master,
        control: BusControl::Gain(0.5),
    });
    harness.done(Request::CreateLoop {
        loop_id: looped,
        strip: track,
    });
    harness.done(Request::LoopGain {
        loop_id: looped,
        gain: 0.5,
    });
    (harness, track, looped)
}

#[shoop_test]
fn telemetry_reports_levels_activity_and_driver_inputs() {
    let (mut harness, track, looped) = configured();
    harness.host.host.set_load(0.25);
    harness.host.host.add_xruns(3);
    harness.host.host.set_driver(DriverStatus {
        kind: DriverKind::Dummy,
        state: DriverRunState::Running,
        sample_rate: 1000,
        buffer_frames: 4,
    });
    let from = harness.frame();
    harness.midi_input.push((from + 1, vec![0x90, 60, 100]));
    harness.done(transition(looped, LoopMode::Recording));
    harness.run(6, 3);
    let recorded = harness.frame() - from;
    harness.done(transition(looped, LoopMode::Playing));
    harness.run(6, 3);
    let frame = harness.frame();
    let latest = |harness: &Harness, pick: &dyn Fn(&Telemetry) -> bool| {
        harness
            .telemetry
            .iter()
            .rev()
            .find(|(_, record)| pick(record))
            .map(|(_, record)| *record)
            .unwrap()
    };
    let Telemetry::Engine {
        frame: reported,
        xruns,
        load,
        driver,
    } = latest(&harness, &|r| matches!(r, Telemetry::Engine { .. }))
    else {
        unreachable!()
    };
    assert_eq!((reported, xruns, load), (frame, 3, 0.25));
    assert_eq!(driver.buffer_frames, 4);
    assert_eq!(harness.host.client.driver(), Some(driver));
    let Telemetry::Strip {
        strip: reported,
        input_peaks,
        output_peaks,
        ..
    } = latest(&harness, &|r| matches!(r, Telemetry::Strip { .. }))
    else {
        unreachable!()
    };
    assert_eq!(reported, track);
    assert_eq!(input_peaks.as_slice(), &[1., 0.5]);
    // Monitored input plus the loop at half gain, then half output gain.
    assert_eq!(output_peaks.as_slice(), &[0.75, 0.375]);
    let Telemetry::Bus { peaks, .. } = latest(&harness, &|r| matches!(r, Telemetry::Bus { .. }))
    else {
        unreachable!()
    };
    assert_eq!(peaks.as_slice(), &[0.375, 0.1875]);
    let Telemetry::Loop {
        mode,
        length,
        peaks,
        ..
    } = latest(&harness, &|r| matches!(r, Telemetry::Loop { .. }))
    else {
        unreachable!()
    };
    assert_eq!((mode, length), (LoopMode::Playing, recorded));
    assert_eq!(peaks.as_slice(), &[0.5, 0.25]);
    assert!(harness
        .telemetry
        .iter()
        .any(|(_, r)| matches!(r, Telemetry::Strip { midi_in: true, .. })));
    assert!(harness
        .telemetry
        .iter()
        .any(|(_, r)| matches!(r, Telemetry::Strip { midi_out: true, .. })));
}

#[shoop_test]
fn bulk_reads_return_recorded_audio_channels_and_midi() {
    let (mut harness, track, looped) = configured();
    harness.input = Box::new(|port, frame| (frame as f32 + 1.) * if port == 0 { 1. } else { -1. });
    let start = harness.frame();
    harness.midi_input.push((start + 2, vec![0x90, 60, 100]));
    harness.midi_input.push((start + 300, vec![0x80, 60, 0]));
    harness.done(transition(looped, LoopMode::Recording));
    harness.run(5, 1);
    let injected = harness.frame();
    harness.done(Request::InjectMidi {
        strip: track,
        message: MidiMessage::new(&[0xb0, 64, 127]).unwrap(),
    });
    harness.run((start + 400 - harness.frame()) as usize, 16);
    harness.done(transition(looped, LoopMode::Stopped));
    let stop = harness.frame() - 1;
    let frames = stop - start;
    let (total, left, _) = harness.read(looped, BulkSelector::Audio { channel: 0 });
    assert_eq!(total, frames);
    assert!(left.len() > BULK_CHUNK_SAMPLES);
    let expected: Vec<f32> = (start..stop).map(|f| (f as f32 + 1.) * 2.).collect();
    assert_eq!(left, expected);
    let (_, right, _) = harness.read(looped, BulkSelector::Audio { channel: 1 });
    assert_eq!(right, expected.iter().map(|s| -s).collect::<Vec<_>>());
    let (total, _, bytes) = harness.read(looped, BulkSelector::Midi);
    assert_eq!(total as usize, bytes.len());
    assert_eq!(
        records(&bytes),
        [
            (2, vec![0x90, 60, 100]),
            (injected - start, vec![0xb0, 64, 127]),
            (300, vec![0x80, 60, 0]),
        ]
    );
    // Monitoring passes the same events to the strip's MIDI output.
    let monitored: Vec<_> = harness
        .midi
        .iter()
        .filter(|(frame, _)| *frame < stop)
        .cloned()
        .collect();
    assert_eq!(
        monitored,
        [
            (start + 2, vec![0x90, 60, 100]),
            (injected, vec![0xb0, 64, 127]),
            (start + 300, vec![0x80, 60, 0]),
        ]
    );
    // The loop's MIDI plays back through the strip's MIDI output.
    harness.midi_input.clear();
    harness.done(Request::StripControl {
        strip: track,
        control: StripControl::Monitoring(false),
    });
    let played = harness.frame();
    harness.done(transition(looped, LoopMode::Playing));
    harness.run(310, 16);
    for (offset, packet) in [
        (2, vec![0x90, 60, 100]),
        (injected - start, vec![0xb0, 64, 127]),
        (300, vec![0x80, 60, 0]),
    ] {
        assert!(harness.midi.contains(&(played + offset, packet)));
    }
    assert_eq!(
        harness.apply(Request::InjectMidi {
            strip: strip(0, 1),
            message: MidiMessage::new(&[0x90, 1, 1]).unwrap(),
        }),
        Outcome::Refused(Refusal::InvalidId)
    );
}

#[shoop_test]
fn resync_streams_the_confirmed_state() {
    let (mut harness, track, looped) = configured();
    let follower = loop_id(0, 4);
    harness.done(Request::CreateLoop {
        loop_id: follower,
        strip: track,
    });
    harness.done(Request::LoopSync {
        loop_id: follower,
        source: Some(looped),
    });
    harness.states.clear();
    assert_no_heap(|| harness.host.client.resync()).unwrap();
    for _ in 0..8 {
        harness.run(1, 1);
    }
    let states = &harness.states;
    let (Some(StateRecord::Begin { epoch }), Some(StateRecord::End { epoch: end })) =
        (states.first(), states.last())
    else {
        panic!("framed stream {states:?}")
    };
    assert_eq!(epoch, end);
    let host_ports: Vec<_> = states
        .iter()
        .filter_map(|r| match r {
            StateRecord::HostPort(port) => {
                Some((port.id, port.kind, port.direction, port.preferred))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        host_ports,
        [
            (HostPortId(0), HostPortKind::Audio, Direction::Input, false),
            (HostPortId(1), HostPortKind::Audio, Direction::Input, false),
            (HostPortId(2), HostPortKind::Audio, Direction::Output, true),
            (HostPortId(3), HostPortKind::Audio, Direction::Output, true),
            (HostPortId(4), HostPortKind::Midi, Direction::Input, false),
            (HostPortId(5), HostPortKind::Midi, Direction::Output, false),
        ]
    );
    let expected = [
        StateRecord::Strip {
            strip: track,
            shape: shape(2, 2, true),
        },
        StateRecord::StripControl {
            strip: track,
            control: StripControl::InputGain(2.),
        },
        StateRecord::StripControl {
            strip: track,
            control: StripControl::Monitoring(true),
        },
        StateRecord::StripControl {
            strip: track,
            control: StripControl::OutputGain(0.5),
        },
        StateRecord::StripControl {
            strip: track,
            control: StripControl::OutputMute(false),
        },
        StateRecord::Route {
            strip: track,
            bus: BusId::Master,
        },
        StateRecord::Connection {
            port: port(track, PortKind::AudioIn, 1),
            host: audio_in(1),
        },
        StateRecord::Connection {
            port: port(track, PortKind::AudioOut, 0),
            host: audio_out(0),
        },
        StateRecord::Connection {
            port: port(track, PortKind::MidiIn, 0),
            host: midi_in(),
        },
        StateRecord::Connection {
            port: port(track, PortKind::MidiOut, 0),
            host: midi_out(),
        },
        StateRecord::Loop {
            loop_id: follower,
            strip: track,
            sync: Some(looped),
            gain: 1.,
            balance: 0.,
        },
        StateRecord::Loop {
            loop_id: looped,
            strip: track,
            sync: None,
            gain: 0.5,
            balance: 0.,
        },
        StateRecord::Bus {
            bus: BusId::Master,
            control: BusControl::Gain(0.5),
        },
        StateRecord::Bus {
            bus: BusId::Master,
            control: BusControl::Mute(false),
        },
    ];
    for record in expected {
        assert!(states.contains(&record), "missing {record:?}");
    }
    let strips = states
        .iter()
        .filter(|r| matches!(r, StateRecord::Strip { .. }))
        .count();
    assert_eq!(strips, 1);
}

#[shoop_test]
fn credits_limits_and_the_connection_lifecycle_are_enforced() {
    let (mut harness, track, looped) = configured();
    harness.done(transition(looped, LoopMode::Recording));
    harness.run(4, 2);
    harness.done(transition(looped, LoopMode::Playing));
    let credits = harness.host.client.limits().unwrap().reliable_credits as u64;
    for correlation in 1000..1000 + credits + 1 {
        assert_no_heap(|| {
            harness
                .host
                .client
                .carrier_mut()
                .send(ClientMessage::Request {
                    correlation,
                    request: Request::StripControl {
                        strip: track,
                        control: StripControl::InputBalance(0.),
                    },
                })
        })
        .unwrap();
    }
    harness.pump();
    assert_eq!(
        harness.results.get(&(1000 + credits)),
        Some(&Outcome::Refused(Refusal::Overloaded))
    );
    harness.settle();
    assert!((1000..1000 + credits).all(|c| harness.results[&c] == Outcome::Done));

    for id in 0..3 {
        assert_no_heap(|| {
            harness
                .host
                .client
                .carrier_mut()
                .send(ClientMessage::Bulk(BulkRead {
                    id,
                    loop_id: looped,
                    selector: BulkSelector::Midi,
                    offset: 0,
                }))
        })
        .unwrap();
    }
    harness.pump();
    assert!(harness
        .chunks
        .iter()
        .any(|c| c.id == 2 && c.payload == BulkPayload::Refused(Refusal::Overloaded)));
    harness.settle();
    assert_eq!(harness.chunks.len(), 3);

    assert_eq!(
        assert_no_heap(|| harness.host.endpoint().set_credits(4, 1)),
        Ok(())
    );
    assert_eq!(
        harness.host.endpoint().set_credits(64, 1),
        Err(Refusal::InvalidValue)
    );
    harness.pump();
    let limits = harness.host.client.limits().unwrap();
    assert_eq!((limits.reliable_credits, limits.bulk_credits), (4, 1));
    assert!(harness
        .lifecycle
        .contains(&EngineMessage::LimitsChanged(limits)));

    // Goodbye ends the connection; the engine keeps its state and keeps playing.
    assert_no_heap(|| harness.host.client.disconnect());
    harness.pump();
    assert_eq!(
        harness.lifecycle.last(),
        Some(&EngineMessage::Disconnected(DisconnectReason::Shutdown))
    );
    let correlation = 2000;
    assert_no_heap(|| {
        harness
            .host
            .client
            .carrier_mut()
            .send(ClientMessage::Request {
                correlation,
                request: Request::RemoveStrip { strip: track },
            })
    })
    .unwrap();
    harness.pump();
    assert_eq!(
        harness.results[&correlation],
        Outcome::Refused(Refusal::NotConnected)
    );
    let from = harness.outputs[0].len();
    harness.run(8, 4);
    assert!(harness.outputs[0][from..].iter().all(|s| *s != 0.));
    assert_no_heap(|| {
        harness
            .host
            .client
            .carrier_mut()
            .send(ClientMessage::Hello { version: 99 })
    })
    .unwrap();
    harness.pump();
    assert_eq!(
        harness.lifecycle.last(),
        Some(&EngineMessage::Rejected {
            version: PROTOCOL_VERSION
        })
    );
    harness.states.clear();
    assert_no_heap(|| harness.host.connect()).unwrap();
    harness.pump();
    assert!(matches!(
        harness.lifecycle.last(),
        Some(EngineMessage::Welcome { .. })
    ));
    assert_no_heap(|| harness.host.client.resync()).unwrap();
    harness.run(4, 1);
    assert!(harness.states.contains(&StateRecord::Loop {
        loop_id: looped,
        strip: track,
        sync: None,
        gain: 0.5,
        balance: 0.,
    }));
}

#[shoop_test]
fn dropping_the_endpoint_silences_output_and_stopped_cleanup_recovers_pools() {
    let description = description();
    let (mut harness, _, looped) = configured();
    harness.done(transition(looped, LoopMode::Recording));
    harness.run(5, 5);
    harness.done(transition(looped, LoopMode::Playing));
    harness.run(5, 5);
    assert!(harness.outputs[0].iter().rev().take(5).all(|s| *s != 0.));
    let mut render = harness.host.into_render();
    let inputs = [vec![1.; 4], vec![1.; 4]];
    let mut outputs = [vec![9.; 4], vec![9.; 4]];
    let midi_inputs = [MidiBuffer::new(8, 64).unwrap()];
    let mut midi_outputs = [MidiBuffer::new(8, 64).unwrap()];
    let [left, right] = &mut outputs;
    let report = assert_no_heap(|| {
        render.process(
            AudioBlock {
                frame: render.next_frame(),
                frames: 4,
                inputs: &[&inputs[0][..], &inputs[1][..]],
                outputs: &mut [&mut left[..], &mut right[..]],
            },
            MidiBlock {
                inputs: &midi_inputs,
                outputs: &mut midi_outputs,
            },
        )
    })
    .unwrap();
    assert_eq!(report.phase, Phase::Disconnected);
    assert!(outputs.iter().flatten().all(|s| *s == 0.));
    assert_eq!(report.recovery_packets, 32);
    assert_eq!(render.midi_recovery(0).unwrap().len(), 32);
    let mut stopped = render.into_stopped();
    stopped.cleanup().unwrap();
    stopped.cleanup().unwrap();
    assert_eq!(stopped.media_usage().references, 0);
    let usage = stopped.resource_usage();
    assert_eq!(usage.free_pages, description.resource_pages);
    assert_eq!(usage.reserved_pages, 0);
}
