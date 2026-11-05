//! Whole-callback measurements with control and collection timed separately.
use rt_runtime::profiling::{Clock, Collector};
use shoop_rt_engine::{testing::OfflineHost, EngineDescription, HostPorts, Segment};
use shoop_rt_protocol::*;
use std::time::Instant;

type Profile = Collector;
static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
fn now() -> u64 {
    EPOCH.get().unwrap().elapsed().as_nanos() as u64
}

fn profile_setup(host: &mut OfflineHost, mode: &str) -> Profile {
    EPOCH.get_or_init(Instant::now);
    host.render.set_profiling_clock(Clock {
        now,
        resolution_ns: 1,
    });
    host.request(Request::Profiling(mode != "off")).unwrap();
    Collector::default()
}
fn profile_message(profile: &mut Profile, message: &EngineMessage) -> usize {
    match *message {
        EngineMessage::ProfilingStatus(status) => {
            profile.status(status);
            54
        }
        EngineMessage::Profiling(packet) => {
            profile.receive(packet);
            96 + usize::from(packet.len) * 16
        }
        EngineMessage::ProfilingNode {
            generation,
            key,
            name,
        } => {
            profile.name(generation, key, name.as_str());
            20 + name.as_str().len()
        }
        _ => 0,
    }
}
fn profile_result(profile: &mut Profile) -> (u64, bool, u64, u64, u64) {
    let view = profile.view();
    (
        view.dropped,
        view.capture.as_ref().is_some_and(|report| report.complete),
        view.live.rows.iter().map(|row| row.samples).sum(),
        view.high_water,
        view.storage_bytes,
    )
}

fn drain(host: &mut OfflineHost, profile: &mut Profile) -> usize {
    let mut bytes = 0;
    for _ in 0..100_000 {
        let Some(message) = host.receive() else {
            return bytes;
        };
        if let EngineMessage::Result {
            outcome: Outcome::Refused(reason),
            ..
        } = message
        {
            panic!("benchmark request refused: {reason:?}");
        }
        bytes += profile_message(profile, &message);
    }
    panic!("benchmark drain failed to become idle");
}
fn settle(host: &mut OfflineHost, profile: &mut Profile) {
    let frames = host.max_frames();
    for _ in 0..10_000 {
        host.process(frames).unwrap();
        drain(host, profile);
        if host.endpoint().is_idle() {
            return;
        }
    }
    panic!("benchmark setup did not settle");
}
fn strip(index: u32) -> StripId {
    StripId(SlotId::new(index, 1))
}
fn loop_id(index: u32) -> LoopSlotId {
    LoopSlotId(SlotId::new(index, 1))
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        6,
        "arguments: strips quantum callbacks off|on|stalled steady|midi|recording|edits|partitions"
    );
    let strips: u32 = args[1].parse().unwrap();
    let quantum: usize = args[2].parse().unwrap();
    let callbacks: usize = args[3].parse().unwrap();
    let mode = args[4].as_str();
    let scenario = args[5].as_str();
    assert!(matches!(mode, "off" | "on" | "stalled"));
    assert!(matches!(
        scenario,
        "steady" | "midi" | "recording" | "edits" | "partitions"
    ));
    assert!(mode != "stalled" || scenario == "steady");
    let description = EngineDescription {
        quantum,
        sample_rate: 48000,
        max_frames: 512,
        host: HostPorts::numbered(2, 2, 1),
        segments: vec![Segment {
            strips,
            loops: strips,
        }],
        buses: 1,
        bus_channels: 2,
        master_connections: vec![(0, 0), (1, 1)],
        reliable_credits: 512,
        pending_transitions: 4 * strips as usize + 64,
        resource_pages: 64 * strips as usize + 512,
        page_bytes: 4096,
        telemetry_interval_frames: 4800,
        ..EngineDescription::default()
    };
    let mut host = OfflineHost::new(&description).unwrap();
    let mut profile = Profile::default();
    host.connect().unwrap();
    drain(&mut host, &mut profile);
    settle(&mut host, &mut profile);
    host.request(Request::ReplacementBegin { id: 1 }).unwrap();
    drain(&mut host, &mut profile);
    let bus = BusId(SlotId::new(0, 2));
    let mut edit = |value| {
        host.request(Request::ReplacementEdit { id: 1, edit: value })
            .unwrap();
        drain(&mut host, &mut profile);
    };
    edit(ReplacementEdit::Bus { bus, channels: 2 });
    for channel in 0..2 {
        edit(ReplacementEdit::Connect {
            port: EnginePort::Bus(BusPort {
                bus,
                index: channel,
            }),
            host: HostPortId(2 + u32::from(channel)),
        });
    }
    for slot in 0..strips {
        edit(ReplacementEdit::Strip {
            strip: strip(slot),
            shape: StripShape {
                audio_inputs: 2,
                audio_outputs: 2,
                midi_input: true,
                midi_output: true,
            },
            controls: StripPreset {
                automatic_monitoring: false,
                input_gain: 1.,
                input_balance: 0.,
                monitoring: true,
                output_gain: 1.,
                output_balance: 0.,
                output_mute: false,
            },
        });
        edit(ReplacementEdit::Loop {
            loop_id: loop_id(slot),
            strip: strip(slot),
            sync: None,
            frames: 0,
            audio: None,
            midi: None,
            gain: 1.,
            balance: 0.,
        });
        for channel in 0..2 {
            edit(ReplacementEdit::Connect {
                port: EnginePort::Strip(StripPort {
                    strip: strip(slot),
                    kind: PortKind::AudioIn,
                    index: channel,
                }),
                host: HostPortId(u32::from(channel)),
            });
            edit(ReplacementEdit::Route {
                port: StripPort {
                    strip: strip(slot),
                    kind: PortKind::AudioOut,
                    index: channel,
                },
                bus: BusPort {
                    bus,
                    index: channel,
                },
            });
        }
        edit(ReplacementEdit::Connect {
            port: EnginePort::Strip(StripPort {
                strip: strip(slot),
                kind: PortKind::MidiIn,
                index: 0,
            }),
            host: HostPortId(4),
        });
    }
    host.request(Request::ReplacementPrepare { id: 1 }).unwrap();
    settle(&mut host, &mut profile);
    host.request(Request::ReplacementCommit { id: 1 }).unwrap();
    settle(&mut host, &mut profile);
    profile = profile_setup(&mut host, mode);
    for _ in 0..32 {
        host.process(256).unwrap();
        drain(&mut host, &mut profile);
    }
    let mut times = Vec::with_capacity(callbacks);
    let mut budgets = Vec::with_capacity(callbacks);
    let mut collection_ns = 0u128;
    let mut bytes = 0usize;
    let mut rendered_frames = 0u64;
    let mut node_count = 0usize;
    let mut node_calls = 0usize;
    let mut misses = 0usize;
    for index in 0..callbacks {
        let frames = if scenario == "partitions" {
            [1, 7, 127, 256, 33, 511][index % 6]
        } else {
            256
        };
        let first = host.next_frame();
        for channel in 0..2 {
            for (offset, sample) in host.input_mut(channel)[..frames].iter_mut().enumerate() {
                *sample = ((first + offset as u64) % 32) as f32 / 64.;
            }
        }
        if scenario == "midi" {
            for event in 0..16 {
                host.midi_input_mut(0)
                    .push(
                        (event * frames / 16) as u32,
                        frames,
                        &[0x90, 48 + event as u8, 100],
                    )
                    .unwrap();
            }
        }
        let control_started = Instant::now();
        if scenario == "recording" && index % 16 == 0 {
            let mode = if index % 32 == 0 {
                LoopMode::Recording
            } else {
                LoopMode::Stopped
            };
            for slot in 0..strips {
                host.request(Request::LoopTransition {
                    loop_id: loop_id(slot),
                    mode,
                    delay: None,
                })
                .unwrap();
            }
        }
        if scenario == "edits" && index % 16 == 0 {
            host.request(Request::Connect {
                port: EnginePort::Strip(StripPort {
                    strip: strip(0),
                    kind: PortKind::AudioIn,
                    index: 0,
                }),
                host: HostPortId(0),
                connected: index % 32 != 0,
            })
            .unwrap();
        }
        collection_ns += control_started.elapsed().as_nanos();
        if index == callbacks / 2 {
            host.host.add_xruns(1);
        }
        let started = Instant::now();
        host.process(frames).unwrap();
        let elapsed = started.elapsed().as_nanos() as u64;
        let budget = frames as u64 * 1_000_000_000 / 48000;
        times.push(elapsed);
        budgets.push(budget);
        misses += usize::from(elapsed > budget);
        rendered_frames += frames as u64;
        let diagnostics = host.render.diagnostics();
        node_calls += diagnostics.nodes_processed;
        if diagnostics.subranges != 0 {
            node_count = node_count.max(diagnostics.nodes_processed / diagnostics.subranges);
        }
        if mode != "stalled" {
            let started = Instant::now();
            bytes += drain(&mut host, &mut profile);
            if index % 32 == 0 {
                let _ = profile_result(&mut profile);
            }
            collection_ns += started.elapsed().as_nanos();
        }
    }
    let post_started = Instant::now();
    bytes += drain(&mut host, &mut profile);
    collection_ns += post_started.elapsed().as_nanos();
    let (dropped, captured, accepted_samples, high_water, storage_bytes) =
        profile_result(&mut profile);
    println!("{{\"strips\":{strips},\"nodes\":{node_count},\"node_calls\":{node_calls},\"quantum\":{quantum},\"mode\":\"{mode}\",\"scenario\":\"{scenario}\",\"frames\":{rendered_frames},\"misses\":{misses},\"control_collection_ns\":{collection_ns},\"wire_bytes\":{bytes},\"dropped_batches\":{dropped},\"high_water_pages\":{high_water},\"sample_storage_bytes\":{storage_bytes},\"capture_complete\":{captured},\"accepted_samples_final_interval\":{accepted_samples},\"callback_ns\":{times:?},\"budget_ns\":{budgets:?}}}");
}
