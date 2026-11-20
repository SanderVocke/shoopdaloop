#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_reference::{
    midi::{InputEvent, Message, OutputEvent},
    records::{LoopRecord, Mode},
    transitions::LoopId,
};
use rt_fpga_testbench::suites::midi::runtime::{Driver, compile};
use shoop_wasm_test_support::shoop_test;

fn message(bytes: &[u8]) -> Message {
    Message::managed(bytes).unwrap()
}
fn event(offset: u32, bytes: &[u8]) -> InputEvent {
    InputEvent {
        input: 0,
        offset,
        message: message(bytes),
    }
}
fn row(slot: u32, mode: Mode, position: u64, length: u64, begin_capture: bool) -> LoopRecord {
    LoopRecord {
        id: LoopId {
            slot,
            generation: 1,
        },
        mode,
        position,
        content_length: length,
        begin_capture,
    }
}
fn output(offset: u32, bytes: &[u8]) -> OutputEvent {
    OutputEvent {
        destination: 0,
        offset,
        message: message(bytes),
    }
}
fn imported(d: &mut Driver<'_>, packets: &[(u64, &[u8])], frames: u64) -> u64 {
    assert_eq!(d.issue(8, [0; 5]), 0);
    let view = d.sim.model.get("response_handle");
    for &(frame, bytes) in packets {
        let packed = bytes
            .iter()
            .enumerate()
            .fold((bytes.len() as u64) << 24, |a, (i, b)| {
                a | u64::from(*b) << (i * 8)
            });
        assert_eq!(d.issue(9, [view, frame, packed, 0, 0]), 0);
    }
    assert_eq!(d.issue(10, [view, frames, 0, 0, 0]), 0);
    assert_eq!(d.sim.model.get("response_count"), 1);
    view
}

#[shoop_test]
fn midi_event_only_records_repeats_and_stops_without_correction_packets() {
    let compiled = compile(1).unwrap();
    let mut d = Driver::new(&compiled, 1).unwrap();
    let packets = [
        event(0, &[0x90, 60, 80]),
        event(3, &[0xb0, 64, 127]),
        event(15, &[0x80, 60, 0]),
    ];
    assert_eq!(
        d.process(0, 16, &[row(0, Mode::Recording, 0, 0, true)], &packets),
        0
    );
    assert!(d.messages().is_empty());
    assert_eq!(
        d.process(16, 16, &[row(0, Mode::Stopped, 0, 16, false)], &[]),
        0
    );
    for start in [32, 48] {
        assert_eq!(
            d.process(start, 16, &[row(0, Mode::Playing, 0, 16, false)], &[]),
            0
        );
        assert_eq!(
            d.messages(),
            packets
                .iter()
                .map(|e| OutputEvent {
                    destination: 0,
                    offset: e.offset,
                    message: e.message
                })
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(
        d.process(64, 16, &[row(0, Mode::Stopped, 0, 16, false)], &[]),
        0
    );
    assert!(d.messages().is_empty());
    assert_eq!(d.issue(17, [0; 5]), 2);
    assert_eq!(d.issue(19, [0; 5]), 2);
}

#[shoop_test]
fn midi_event_only_shared_exports_survive_replacement_and_retirement() {
    let compiled = compile(2).unwrap();
    let mut d = Driver::new(&compiled, 2).unwrap();
    let view = imported(&mut d, &[(0, &[0x90, 60, 80]), (15, &[0x80, 60, 0])], 16);
    assert_eq!(d.issue(5, [0, view, 0, 0, 0]), 0);
    assert_eq!(d.issue(6, [0, 1, 0, 0, 0]), 0);
    assert_eq!(d.issue(11, [1, 0, 0, 0, 0]), 0);
    assert_eq!(d.sim.model.get("response_handle"), view);
    assert_eq!(d.sim.model.get("response_count"), 2);
    assert_eq!(d.issue(7, [0; 5]), 0);
    assert_eq!(d.issue(7, [1, 0, 0, 0, 0]), 0);
    assert_eq!(d.issue(13, [view, 0, 0, 0, 0]), 0);
    assert_eq!(d.issue(20, [64, 0, 0, 0, 0]), 0);
    assert_eq!(d.issue(12, [view, 1, 0, 0, 0]), 0);
    assert_eq!(d.sim.model.get("response_frame"), 15);
    assert_eq!(d.sim.model.get("response_packet"), 0x003c80);
    assert_eq!(d.issue(13, [view, 0, 0, 0, 0]), 0);
    assert_eq!(d.issue(20, [1, 0, 0, 0, 0]), 0);
    assert_eq!(d.sim.model.get("response_count"), 1);
    assert_eq!(d.issue(20, [64, 0, 0, 0, 0]), 0);
    assert_eq!(d.issue(12, [view, 0, 0, 0, 0]), 4);
}

#[shoop_test]
fn midi_event_only_history_grab_shares_event_ranges_without_state_snapshots() {
    let compiled = compile(2).unwrap();
    let mut d = Driver::new(&compiled, 2).unwrap();
    let stopped = [
        row(0, Mode::Stopped, 0, 0, false),
        row(1, Mode::Stopped, 0, 0, false),
    ];
    let packets = [
        event(0, &[0x90, 60, 80]),
        event(5, &[0xb1, 1, 40]),
        event(15, &[0x80, 60, 0]),
    ];
    assert_eq!(d.process(0, 16, &stopped, &packets), 0);
    d.write(d.scratch + 512, &[0, 1]);
    assert_eq!(d.issue(4, [0, 0, 16, d.scratch + 512, 2]), 0);
    let playing = [
        row(0, Mode::Playing, 0, 16, false),
        row(1, Mode::Playing, 0, 16, false),
    ];
    assert_eq!(d.process(16, 16, &playing, &[]), 0);
    assert_eq!(
        d.messages(),
        vec![
            output(0, &[0x90, 60, 80]),
            output(5, &[0xb1, 1, 40]),
            output(15, &[0x80, 60, 0])
        ]
    );
    assert_eq!(d.issue(11, [0; 5]), 0);
    let first = d.sim.model.get("response_handle");
    assert_eq!(d.issue(11, [1, 0, 0, 0, 0]), 0);
    assert_eq!(d.sim.model.get("response_handle"), first);
    assert_eq!(d.issue(15, [0, 1, 16, 0, 0]), 0);
    let range = d.sim.model.get("response_handle");
    assert_eq!(d.issue(12, [range, 0, 0, 0, 0]), 0);
    assert_eq!(d.sim.model.get("response_frame"), 4);
    assert_eq!(d.sim.model.get("response_packet"), 0x2801b1);
}

#[shoop_test]
fn midi_event_only_refusal_blocks_overdue_events_until_retrigger_without_recovery() {
    let compiled = compile(1).unwrap();
    let mut d = Driver::new(&compiled, 1).unwrap();
    let view = imported(&mut d, &[(0, &[0x90, 60, 80]), (1, &[0x90, 61, 90])], 32);
    assert_eq!(d.issue(5, [0, view, 0, 0, 0]), 0);
    d.set_capacity(1);
    assert_eq!(
        d.process(0, 16, &[row(0, Mode::Playing, 0, 32, false)], &[]),
        3
    );
    assert_eq!(d.messages(), vec![output(0, &[0x90, 60, 80])]);
    d.set_capacity(usize::MAX);
    assert_eq!(
        d.process(16, 16, &[row(0, Mode::Playing, 16, 32, false)], &[]),
        5
    );
    assert!(d.messages().is_empty());
    assert_eq!(
        d.process(32, 16, &[row(0, Mode::Playing, 0, 32, false)], &[]),
        0
    );
    assert_eq!(
        d.messages(),
        vec![output(0, &[0x90, 60, 80]), output(1, &[0x90, 61, 90])]
    );
}

#[shoop_test]
fn midi_event_only_recording_limit_and_transfer_validation_are_preserved() {
    let compiled = compile(1).unwrap();
    let mut d = Driver::new(&compiled, 1).unwrap();
    assert_eq!(d.issue(21, [0, 1, 0, 0, 0]), 0);
    assert_eq!(
        d.process(
            0,
            16,
            &[row(0, Mode::Recording, 0, 0, true)],
            &[event(0, &[0x90, 60, 80]), event(1, &[0x90, 61, 90])]
        ),
        3
    );
    assert_eq!(
        d.process(16, 16, &[row(0, Mode::Stopped, 0, 16, false)], &[]),
        8
    );
    assert_eq!(
        d.process(32, 16, &[row(0, Mode::Playing, 0, 16, false)], &[]),
        8
    );
    assert!(d.messages().is_empty());
    let view = imported(&mut d, &[], 16);
    d.write(d.scratch + 512, &[0, 0]);
    assert_eq!(d.issue(23, [view, d.scratch + 512, 2, 0, 0]), 4);
}

#[shoop_test]
fn midi_compiled_sp_and_shared_jp_match_audio_events_and_job_traces() {
    rt_fpga_testbench::suites::midi::compiled_mixed_conformance().unwrap();
}
#[shoop_test]
fn midi_event_only_axi32_delays_bounds_and_faults() {
    rt_fpga_testbench::suites::midi::axi_conformance(32).unwrap();
}
#[shoop_test]
fn midi_event_only_axi64_delays_bounds_and_faults() {
    rt_fpga_testbench::suites::midi::axi_conformance(64).unwrap();
}

#[shoop_test]
fn midi_event_only_history_trimming_preserves_previously_exported_blocks() {
    let compiled = compile(1).unwrap();
    let mut d = Driver::new(&compiled, 1).unwrap();
    let stopped = [row(0, Mode::Stopped, 0, 0, false)];
    assert_eq!(d.process(0, 16, &stopped, &[event(0, &[0xb0, 1, 1])]), 0);
    assert_eq!(d.issue(15, [0, 0, 16, 0, 0]), 0);
    let pinned = d.sim.model.get("response_handle");
    for tick in 1..20u64 {
        assert_eq!(
            d.process(
                tick * 16,
                16,
                &stopped,
                &[event(0, &[0xb0, 1, tick as u8 + 1])]
            ),
            0
        );
    }
    assert_eq!(d.issue(15, [0, 0, 16, 0, 0]), 1);
    assert_eq!(d.issue(12, [pinned, 0, 0, 0, 0]), 0);
    assert_eq!(d.sim.model.get("response_frame"), 0);
    assert_eq!(d.sim.model.get("response_packet"), 0x0101b0);
    assert_eq!(d.issue(15, [0, 192, 320, 0, 0]), 0);
    let current = d.sim.model.get("response_handle");
    assert_eq!(d.issue(12, [current, 0, 0, 0, 0]), 0);
    assert_eq!(d.sim.model.get("response_packet"), 0x0d01b0);
    assert_eq!(d.issue(12, [current, 7, 0, 0, 0]), 0);
    assert_eq!(d.sim.model.get("response_frame"), 112);
    assert_eq!(d.sim.model.get("response_packet"), 0x1401b0);
}

#[shoop_test]
fn midi_event_only_inputs_remain_independent_and_packet_validation_remains_active() {
    use rt_fpga_testbench::suites::midi::runtime::compile_limits;
    let mut limits = rt_fpga_reference::midi::SessionLimits::session();
    limits.loops = 2;
    limits.inputs = 2;
    limits.destinations = 2;
    limits.store.block_events = 8;
    limits.store.blocks = 128;
    limits.store.max_events = 128;
    limits.store.views = 64;
    limits.store.retirements = 32;
    limits.input_events_per_quantum = 16;
    let compiled = compile_limits(limits).unwrap();
    let mut d = Driver::with_inputs(&compiled, 2, 2).unwrap();
    let recording = [
        row(0, Mode::Recording, 0, 0, true),
        row(1, Mode::Recording, 0, 0, true),
    ];
    let events = [
        event(2, &[0xb0, 1, 30]),
        InputEvent {
            input: 1,
            offset: 7,
            message: message(&[0xb1, 1, 40]),
        },
    ];
    assert_eq!(d.process(0, 16, &recording, &events), 0);
    let playing = [
        row(0, Mode::Playing, 0, 16, false),
        row(1, Mode::Playing, 0, 16, false),
    ];
    assert_eq!(d.process(16, 16, &playing, &[]), 0);
    assert_eq!(
        d.messages(),
        vec![output(2, &[0xb0, 1, 30]), output(7, &[0xb1, 1, 40])]
    );
    assert_eq!(d.issue(15, [1, 0, 16, 0, 0]), 0);
    let view = d.sim.model.get("response_handle");
    assert_eq!(d.issue(12, [view, 0, 0, 0, 0]), 0);
    assert_eq!(d.sim.model.get("response_frame"), 7);
    assert_eq!(d.sim.model.get("response_packet"), 0x2801b1);
    assert_eq!(d.issue(14, [0, 0x030006b0, 0, 0, 0]), 2);
    assert_eq!(d.issue(14, [0, 0x030000c0, 0, 0, 0]), 1);
}
