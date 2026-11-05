//! Prepared application pools and complete callbacks under simultaneous input and replacement.
use shoop_rt_engine::{testing::OfflineHost, EngineDescription, HostPorts, Segment};
use shoop_rt_protocol::*;
use shoop_wasm_test_support::heap::{assert_no_heap, measure_memory, TrackingAllocator};
use std::time::Instant;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn drain(host: &mut OfflineHost) {
    while host.receive().is_some() {}
}
fn request(host: &mut OfflineHost, request: Request) -> Outcome {
    let correlation = host.request(request).unwrap();
    for _ in 0..10000 {
        while let Some(message) = host.receive() {
            if let EngineMessage::Result {
                correlation: received,
                outcome,
            } = message
            {
                if correlation == received {
                    return outcome;
                }
            }
        }
        assert_no_heap(|| host.process(32)).unwrap();
    }
    panic!("request timeout: {request:?}");
}
fn done(host: &mut OfflineHost, value: Request) {
    assert_eq!(request(host, value), Outcome::Done, "{value:?}");
}
fn input(host: &mut OfflineHost, id: u64, intent: impl Fn(u32) -> InputIntent) {
    assert!(matches!(
        request(host, Request::InputBegin { id, count: 2 }),
        Outcome::Input(_)
    ));
    for slot in 0..2 {
        assert!(matches!(
            request(
                host,
                Request::InputAppend {
                    id,
                    edit: InputEdit {
                        loop_id: LoopSlotId(SlotId::new(slot, 2)),
                        intent: intent(slot)
                    }
                }
            ),
            Outcome::Input(_)
        ));
    }
    assert!(matches!(
        request(host, Request::InputPrepare { id }),
        Outcome::Input(InputProgress {
            phase: InputPhase::Ready,
            ..
        })
    ));
    assert!(matches!(
        request(host, Request::InputCommit { id, delay: None }),
        Outcome::Input(InputProgress {
            phase: InputPhase::Applied,
            ..
        })
    ));
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let channels: u8 = args.get(1).map_or(2, |s| s.parse().unwrap());
    let callbacks: usize = args.get(2).map_or(128, |s| s.parse().unwrap());
    let density: usize = args.get(3).map_or(256, |s| s.parse().unwrap());
    assert!(density <= 256);
    let frames: usize = args.get(4).map_or(32, |s| s.parse().unwrap());
    assert!(frames > 0 && frames <= 512);
    let d = EngineDescription {
        segments: vec![Segment {
            strips: 16,
            loops: 128,
        }],
        resource_pages: 65536,
        pending_transitions: 256,
        strip_channels: channels,
        max_frames: 512,
        external_dry_wet: true,
        host: HostPorts::numbered(channels as usize, channels as usize, 1),
        ..EngineDescription::default()
    };
    let capacity = d.history_capacity().unwrap();
    let started = Instant::now();
    let (mut host, memory) = measure_memory(|| OfflineHost::new(&d).unwrap());
    let preparation_ms = started.elapsed().as_millis();
    host.connect().unwrap();
    assert_no_heap(|| host.process(32)).unwrap();
    drain(&mut host);
    done(&mut host, Request::ReplacementBegin { id: 1 });
    for slot in 0..16 {
        let strip = StripId(SlotId::new(slot, 2));
        done(
            &mut host,
            Request::ReplacementEdit {
                id: 1,
                edit: ReplacementEdit::Strip {
                    strip,
                    shape: StripShape {
                        audio_inputs: channels,
                        audio_outputs: channels,
                        midi_input: true,
                        midi_output: true,
                    },
                    controls: StripPreset {
                        automatic_monitoring: false,
                        input_gain: 1.,
                        input_balance: 0.,
                        monitoring: false,
                        output_gain: 1.,
                        output_balance: 0.,
                        output_mute: false,
                    },
                },
            },
        );
        done(
            &mut host,
            Request::ReplacementEdit {
                id: 1,
                edit: ReplacementEdit::Loop {
                    loop_id: LoopSlotId(SlotId::new(slot, 2)),
                    strip,
                    sync: None,
                    frames: 0,
                    audio: None,
                    dry_audio: None,
                    recorded_state: None,
                    midi: None,
                    gain: 1.,
                    balance: 0.,
                },
            },
        );
        for channel in 0..channels {
            done(
                &mut host,
                Request::ReplacementEdit {
                    id: 1,
                    edit: ReplacementEdit::Connect {
                        port: EnginePort::Strip(StripPort {
                            strip,
                            kind: PortKind::AudioIn,
                            index: channel,
                        }),
                        host: HostPortId(channel as u32),
                    },
                },
            );
        }
        for (kind, port) in [
            (PortKind::MidiIn, 2 * channels as u32),
            (PortKind::MidiOut, 2 * channels as u32 + 1),
        ] {
            done(
                &mut host,
                Request::ReplacementEdit {
                    id: 1,
                    edit: ReplacementEdit::Connect {
                        port: EnginePort::Strip(StripPort {
                            strip,
                            kind,
                            index: 0,
                        }),
                        host: HostPortId(port),
                    },
                },
            );
        }
    }
    done(&mut host, Request::ReplacementPrepare { id: 1 });
    done(&mut host, Request::ReplacementCommit { id: 1 });
    let mut times = Vec::with_capacity(callbacks + 32);
    let mut work = Vec::with_capacity(callbacks + 32);
    for phase in 0..2 {
        if phase == 1 {
            input(&mut host, 1, |_| InputIntent::Grab {
                sync: None,
                reverse_start_cycle: None,
                cycles_length: None,
                go_to_cycle: None,
                mode: LoopMode::Stopped,
                maximum_frames: 0,
            });
            input(&mut host, 2, |_| InputIntent::Mode {
                mode: LoopMode::Replacing,
                position: Some(0),
            });
        }
        for index in 0..if phase == 0 { 32 } else { callbacks } {
            for channel in 0..channels as usize {
                host.input_mut(channel)[..frames].fill(0.25);
            }
            for event in 0..density {
                let packet = if index % 2 == 0 {
                    [0xb0 | (event / 128) as u8, 120, 0]
                } else {
                    [0x90 | (event / 128) as u8, (event % 128) as u8, 100]
                };
                host.midi_input_mut(0)
                    .push((event * frames / density) as u32, frames, &packet)
                    .unwrap();
            }
            let started = Instant::now();
            assert_no_heap(|| host.process(frames)).unwrap();
            times.push((phase, started.elapsed().as_nanos()));
            work.push(host.render.resource_work().1.validation_steps);
            assert!(!host.midi_output(0).is_recovery());
            drain(&mut host);
        }
    }
    input(&mut host, 3, |_| InputIntent::Mode {
        mode: LoopMode::Stopped,
        position: None,
    });
    let mut stopped = host.into_render().into_stopped();
    stopped.cleanup().unwrap();
    let usage = stopped.resource_usage();
    assert_eq!(usage.free_pages, capacity.total_resource_pages);
    assert_eq!(usage.reserved_pages, 0);
    let misses = times
        .iter()
        .filter(|(_, nanos)| *nanos > frames as u128 * 1_000_000_000u128 / 48_000)
        .count();
    let times = times
        .iter()
        .map(|(phase, nanos)| format!("[{phase},{nanos}]"))
        .collect::<Vec<_>>()
        .join(",");
    println!("{{\"channels\":{channels},\"sources\":16,\"loops\":128,\"quantum\":32,\"host_max\":512,\"callback_frames\":{frames},\"events_per_source_callback\":{density},\"prepared_peak\":{},\"prepared_retained\":{},\"preparation_ms\":{preparation_ms},\"callback_misses\":{misses},\"callbacks\":[{times}],\"render_steps\":{work:?},\"output_recovery_callbacks\":0,\"guarded_callbacks\":true,\"all_pages_recovered\":true}}", memory.peak, memory.live);
}
