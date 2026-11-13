#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
//! The CPU host bridge drives the same core as the pin-level simulator, through registers.
use rt_audio_math::Q824;
use rt_backend_testbench::{
    production, rtl,
    spec::{Action, Samples, Scenario, Timing, Transition},
};
use rt_fpga_adapter::{
    bridge::{
        Bridge, Bus, map,
        trace::{self, RecordingBus},
    },
    image::{Image, View},
    looping::MediaCommand,
    port::{self, Capacities, PortIo},
};
use rt_fpga_sim::{
    Simulator,
    bridge_sim::{MarlinBus, Probe},
    get, put, runtime, value_words,
};
use shoop_wasm_test_support::shoop_test;
use std::path::{Path, PathBuf};

fn cache() -> PathBuf {
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-tests");
    std::fs::create_dir_all(&cache).unwrap();
    cache
}

fn scenario(source: &str) -> (Scenario, Image) {
    let case: Scenario = serde_json::from_str(source).unwrap();
    let image = rtl::compile(&case).unwrap();
    (case, image)
}

fn gain_fixtures<D: PortIo + Probe>(device: &mut D) {
    for source in [
        include_str!("../fixtures/instant-gains.json"),
        include_str!("../../../../fpga/verification/scenarios/silence.json"),
    ] {
        let (case, image) = scenario(source);
        let expected = production::run::<Q824>(&case).unwrap();
        port::load(device, &image).unwrap();
        for (i, step) in case.steps.iter().enumerate() {
            for action in &step.actions {
                let Action::Gain { node, target, .. } = action else {
                    panic!("gain fixture")
                };
                let ramp = rtl::Value::Ramp(rtl::Ramp {
                    start: *target,
                    target: *target,
                    duration: 0,
                    elapsed: 0,
                });
                for &address in &image.gains.iter().find(|b| b.id == *node).unwrap().ramps {
                    device
                        .configure(false, address, Some(value_words(ramp)))
                        .unwrap();
                }
            }
            let outputs = port::process(device, &image, &step.inputs).unwrap();
            assert_eq!(Samples::Q824(outputs), expected.quanta[i].outputs);
            assert_eq!(device.frame(), expected.quanta[i].next_frame);
        }
    }
}

/// Replays the original record/stop/play case through any port and returns the captured media.
fn record_stop_play<D: PortIo>(device: &mut D, check_audio: bool) -> Vec<i32> {
    let (case, image) = scenario(include_str!(
        "../../../../fpga/verification/scenarios/mono-record.json"
    ));
    let expected = production::run::<Q824>(&case).unwrap();
    port::load(device, &image).unwrap();
    let mut sequence = 0;
    for (i, step) in case.steps.iter().enumerate() {
        for action in &step.actions {
            let Action::Transition {
                target,
                at,
                timing: Timing::At,
                operation,
                ..
            } = action
            else {
                panic!("record fixture")
            };
            let mut words = [0; 20];
            let code = match operation {
                Transition::Stop => 0,
                Transition::Play { .. } => 1,
                Transition::Record => 2,
                Transition::Seek { .. } => 3,
            };
            put(&mut words, 34, 2, code);
            put(
                &mut words,
                64,
                64,
                u64::from(target.slot) | (u64::from(target.generation) << 32),
            );
            sequence += 1;
            put(&mut words, 128, 64, sequence);
            put(&mut words, 192, 64, *at);
            if let Transition::Play {
                position,
                retrigger,
            }
            | Transition::Seek {
                position,
                retrigger,
            } = operation
            {
                put(&mut words, 384, 64, *position);
                put(&mut words, 36, 1, u64::from(*retrigger));
            }
            assert_eq!(device.transition(4, 0, words).unwrap().error, 0);
        }
        let outputs = port::process(device, &image, &step.inputs).unwrap();
        if check_audio {
            assert_eq!(
                Samples::Q824(outputs),
                expected.quanta[i].outputs,
                "quantum {i}"
            );
        }
        let row = device.transition(0, 0, [0; 20]).unwrap();
        assert_eq!(row.error, 0);
        assert_eq!(get(&row.words, 0, 64), expected.quanta[i].next_frame);
        for state in &expected.quanta[i].states {
            let row = device.transition(12, state.id.slot, [0; 20]).unwrap();
            assert_eq!(row.error, 0);
            let raw = row.words;
            assert_eq!(get(&raw, 1, 32), u64::from(state.id.generation));
            assert_eq!(
                ["stopped", "playing", "recording"][get(&raw, 33, 2) as usize],
                state.mode
            );
            for (offset, value) in [
                (64, state.content_length),
                (128, state.position),
                (192, state.capture_frames),
                (256, state.cycles),
            ] {
                assert_eq!(
                    get(&raw, offset, 64),
                    value,
                    "state field {offset} quantum {i}"
                );
            }
        }
    }
    let frames = expected.media[0].frames;
    let capture_view = image
        .program
        .instructions
        .iter()
        .find_map(|i| match i {
            rtl::Instruction::ProcessLogicalCapture { view, .. } => Some(*view),
            _ => None,
        })
        .unwrap();
    let words = device.configure(false, capture_view, None).unwrap();
    let read = MediaCommand {
        opcode: 6,
        view: words[0],
        start: 0,
        length: frames,
        count: frames as u32,
    };
    let reply = device.store_command(read, &[]).unwrap();
    assert_eq!(reply.code, 0);
    if check_audio {
        assert_eq!(
            Samples::Q824(vec![reply.result.samples.clone()]),
            expected.media[0].samples
        );
    }
    reply.result.samples
}

#[shoop_test]
fn bridge_reports_identity_capacities_and_resets_with_idle_memory() {
    let rt = runtime(&cache()).unwrap();
    let mut bridge = Bridge::new(MarlinBus::new(&rt, false).unwrap()).unwrap();
    let sim = Simulator::new(&rt, None, false).unwrap();
    assert_eq!(
        bridge.capacities(),
        Capacities {
            chunk_limit: 32768,
            ..PortIo::capacities(&sim)
        }
    );
    let quiescent = 1 << map::STATUS_QUIESCENT.0;
    assert_ne!(bridge.bus_mut().read(map::STATUS) & quiescent, 0);
    bridge.reset_core().unwrap();
    assert_eq!(bridge.counters(), Default::default());
    assert!(!bridge.core_status().unwrap().faulted);
}

#[shoop_test]
fn gain_fixtures_match_production_through_registers() {
    let rt = runtime(&cache()).unwrap();
    let mut bridge = Bridge::new(MarlinBus::new(&rt, false).unwrap()).unwrap();
    gain_fixtures(&mut bridge);
    let counters = bridge.counters();
    assert!(counters.quanta > 0 && counters.last_cycles > 0);
}

#[shoop_test]
fn record_stop_play_matches_production_and_the_pin_level_simulator() {
    let rt = runtime(&cache()).unwrap();
    let mut sim = Simulator::new(&rt, None, false).unwrap();
    let mut bridge = Bridge::new(MarlinBus::new(&rt, false).unwrap()).unwrap();
    let pins = record_stop_play(&mut sim, true);
    let registers = record_stop_play(&mut bridge, true);
    assert_eq!(registers, pins);
}

#[shoop_test]
fn queued_replays_match_native_through_registers() {
    let rt = runtime(&cache()).unwrap();
    for source in [
        include_str!("../fixtures/chain.json"),
        include_str!("../fixtures/parallel.json"),
    ] {
        let scenario = serde_json::from_str(source).unwrap();
        let bridge = Bridge::new(MarlinBus::new(&rt, false).unwrap()).unwrap();
        let report = rt_fpga_sim::differential::run(&scenario, bridge).unwrap();
        assert_eq!(report.samples, 320);
        assert_eq!(report.state, 2);
    }
}

#[shoop_test]
fn appends_stream_through_the_fifo_and_match_the_pin_level_simulator() {
    let rt = runtime(&cache()).unwrap();
    let (_, mut image) = scenario(include_str!("../fixtures/instant-gains.json"));
    image.media.views = vec![
        View {
            chunks: vec![],
            frames: 0,
        };
        1
    ];
    let samples: Vec<i32> = (0..150).map(|i| i * 1009 - 70_000).collect();
    fn append_and_read<D: PortIo>(
        device: &mut D,
        image: &Image,
        samples: &[i32],
    ) -> (u8, usize, Vec<i32>) {
        port::load(device, image).unwrap();
        let command = |opcode, length: usize, count: usize| MediaCommand {
            opcode,
            view: 0,
            start: 0,
            length: length as u64,
            count: count as u32,
        };
        assert_eq!(device.store_command(command(8, 0, 0), &[]).unwrap().code, 0);
        let append = device
            .store_command(command(5, 0, samples.len()), samples)
            .unwrap();
        let read = device
            .store_command(command(6, samples.len(), samples.len()), &[])
            .unwrap();
        assert_eq!(read.code, 0);
        (append.code, append.written, read.result.samples)
    }
    let mut sim = Simulator::new(&rt, None, false).unwrap();
    let mut bridge = Bridge::new(MarlinBus::new(&rt, false).unwrap()).unwrap();
    let pins = append_and_read(&mut sim, &image, &samples);
    let registers = append_and_read(&mut bridge, &image, &samples);
    assert_eq!(registers, (0, samples.len(), samples.clone()));
    assert_eq!(registers, pins);
}

#[shoop_test]
fn external_audio_runs_quanta_without_sample_traffic_and_digests_deterministically() {
    let rt = runtime(&cache()).unwrap();
    let mut bridge = Bridge::new(MarlinBus::new(&rt, true).unwrap()).unwrap();
    assert!(bridge.capacities().external_audio);
    let mut runs = vec![];
    for _ in 0..2 {
        let reads = bridge.bus().reads;
        record_stop_play(&mut bridge, false);
        let counters = bridge.counters();
        assert!(counters.quanta > 0);
        assert!(counters.output_words > 0);
        assert_ne!(counters.digest, 0);
        assert!(bridge.bus().reads > reads);
        runs.push(counters);
    }
    assert_eq!(runs[0], runs[1]);
}

#[shoop_test]
fn board_profile_trace_replays_every_read_including_the_output_digest() {
    let rt = runtime(&cache()).unwrap();
    let bus = RecordingBus::new(MarlinBus::board(&rt).unwrap(), 1_000_000);
    let mut bridge = Bridge::new(bus).unwrap();
    assert_eq!(bridge.physical_base(), 0x1780_0000);
    assert!(bridge.capacities().external_audio);
    record_stop_play(&mut bridge, false);
    let counters = bridge.counters();
    assert!(counters.quanta > 0 && counters.output_words > 0 && counters.digest != 0);
    assert_eq!(bridge.bus().dropped(), 0);
    let mut log = String::new();
    bridge.bus().dump(&mut log).unwrap();
    let path = cache().join("board-register-trace.log");
    std::fs::write(&path, &log).unwrap();
    let mut records = trace::parse(&log).unwrap();
    let mut replay = MarlinBus::board(&rt).unwrap();
    trace::replay(&mut replay, &records).unwrap();
    let mut replay = Bridge::new(replay).unwrap();
    assert_eq!(replay.counters(), counters);

    let last = records.len() - 1;
    let trace::Access::Read { value, .. } = &mut records[last] else {
        panic!("digest read");
    };
    *value ^= 1;
    let mut replay = MarlinBus::board(&rt).unwrap();
    let error = trace::replay(&mut replay, &records).unwrap_err();
    assert_eq!(error.index, last);
    assert_eq!(error.offset, map::DIGEST + 4);
}
