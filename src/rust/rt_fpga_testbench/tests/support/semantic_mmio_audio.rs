use super::{first_fault, quantum, recovery_design, wait_initial};
use rt_fpga_rtl::host::semantic_registers::*;
use rt_fpga_testbench::{
    bits::Bits,
    codec,
    drivers::store::Geometry,
    memory::{Memory, Mode},
    model::{Design, Probe},
    sim::Sim,
    suites,
};
use serde_json::{Value, json};
use shoop_wasm_test_support::shoop_test;

#[path = "semantic_mmio_clear.rs"]
mod clear;
#[path = "semantic_mmio_config.rs"]
mod configuration;
#[path = "semantic_mmio_safety.rs"]
mod safety;
#[path = "semantic_mmio_trace.rs"]
mod trace;

const BANKS: &str = "pipeline.execution.core.pipeline.consumer.banks";

fn design() -> Design {
    let root = rt_fpga_testbench::root();
    let mut design = recovery_design().parameters(&[("LOOP_DEPTH", 128)]);
    design.top = "fifo_mmio_pipeline_axi".into();
    design
        .sources
        .insert(0, root.join("fpga/host_bridge/rtl/semantic_registers.sv"));
    for module in ["fifo_config_service", "fifo_mmio", "fifo_mmio_pipeline_axi"] {
        design
            .sources
            .push(root.join(format!("fpga/host_bridge/rtl/{module}.sv")));
    }
    design.probes = ["last_admitted", "head_sequence", "release_sequence"]
        .map(|name| Probe::inject(&format!("{BANKS}.{name}")))
        .into();
    design
}
fn channel(s: &mut Sim<'_>, valid: &str, ready: &str) {
    s.model.set(valid, 1);
    assert_eq!(s.tick(&[ready]).0.get(ready), 1, "{ready}");
    s.model.set(valid, 0);
}
fn write_hold(s: &mut Sim<'_>, address: u32, value: u32) {
    s.model.set_all(&[
        ("s_axi_awaddr", u64::from(address)),
        ("s_axi_wdata", u64::from(value)),
        ("s_axi_wstrb", 15),
    ]);
    let mut channels = [
        ("s_axi_awvalid", "s_axi_awready"),
        ("s_axi_wvalid", "s_axi_wready"),
    ];
    if address & 4 != 0 {
        channels.reverse();
    }
    for (valid, ready) in channels {
        channel(s, valid, ready);
    }
    s.until(32, "copied MMIO write response", |m| {
        m.get("s_axi_bvalid") != 0
    });
    assert_eq!(s.model.get("s_axi_bresp"), 0);
}
fn release_write(s: &mut Sim<'_>) {
    assert_eq!(s.model.get("s_axi_bvalid"), 1);
    assert_eq!(s.model.get("s_axi_bresp"), 0);
    s.model.set("s_axi_bready", 1);
    s.idle(1);
    s.model.set("s_axi_bready", 0);
}
fn write(s: &mut Sim<'_>, address: u32, value: u32) {
    write_hold(s, address, value);
    release_write(s);
}
fn read_hold(s: &mut Sim<'_>, address: u32) -> u32 {
    s.model.set("s_axi_araddr", u64::from(address));
    channel(s, "s_axi_arvalid", "s_axi_arready");
    s.until(32, "copied MMIO read response", |m| {
        m.get("s_axi_rvalid") != 0
    });
    assert_eq!(s.model.get("s_axi_rresp"), 0);
    s.model.get("s_axi_rdata") as u32
}
fn release_read(s: &mut Sim<'_>, expected: u32) {
    assert_eq!(s.model.get("s_axi_rvalid"), 1);
    assert_eq!(s.model.get("s_axi_rresp"), 0);
    assert_eq!(s.model.get("s_axi_rdata"), u64::from(expected));
    s.model.set("s_axi_rready", 1);
    s.idle(1);
    s.model.set("s_axi_rready", 0);
}
fn read(s: &mut Sim<'_>, address: u32) -> u32 {
    let value = read_hold(s, address);
    release_read(s, value);
    value
}
fn publish_hold(s: &mut Sim<'_>, bank: u32, sequence: u64, revision: u32, commands: &[[u32; 12]]) {
    let epoch = s.model.get("runtime_epoch") as u32;
    let mut words = vec![0; 16];
    words[0] = SEMANTIC_ID;
    words[1] = SEMANTIC_VERSION | ((commands.len() as u32) << 16);
    words[2] = epoch;
    words[4] = sequence as u32;
    words[5] = (sequence >> 32) as u32;
    words[6] = revision;
    words[8] = revision;
    for command in commands {
        words.extend(command);
    }
    write(s, REG_COMMAND_BANK, bank);
    write(s, REG_COMMAND_BEGIN, epoch);
    for word in words {
        write(s, REG_COMMAND_DATA, word);
    }
    write_hold(s, REG_COMMAND_PUBLISH, epoch);
    assert_eq!(s.model.get("last_admitted"), sequence);
}
fn ack(s: &mut Sim<'_>, snapshot: bool, bank: u32, sequence: u64, epoch: u32) {
    let (select, low, high, authority, doorbell) = if snapshot {
        (
            REG_SNAPSHOT_BANK,
            REG_SNAPSHOT_ACK_LOW,
            REG_SNAPSHOT_ACK_HIGH,
            REG_SNAPSHOT_ACK_EPOCH,
            REG_SNAPSHOT_ACK,
        )
    } else {
        (
            REG_RESULT_BANK,
            REG_RESULT_ACK_LOW,
            REG_RESULT_ACK_HIGH,
            REG_RESULT_ACK_EPOCH,
            REG_RESULT_ACK,
        )
    };
    write(s, select, bank);
    write(s, low, sequence as u32);
    write(s, high, (sequence >> 32) as u32);
    write(s, authority, epoch);
    write(s, doorbell, 1);
}
fn start_request(s: &mut Sim<'_>, operation: u32, payload: &[u32; 12], tag: u32) {
    assert_eq!(read(s, REG_STATUS) & 3, 1, "private service reservation");
    write(s, REG_REQUEST_EPOCH, s.model.get("runtime_epoch") as u32);
    write(s, REG_REQUEST_OP, operation);
    for (index, word) in payload.iter().enumerate() {
        write(s, REG_REQUEST_DATA + 4 * index as u32, *word);
    }
    write(s, REG_REQUEST_PUSH, tag);
    assert_eq!(read(s, REG_SUBMISSION_RESULT), RECEIPT_ACCEPTED);
}
fn finish_request(s: &mut Sim<'_>, tag: u32) -> [u32; 12] {
    let mut complete = false;
    for _ in 0..20_000 {
        if read(s, REG_STATUS) & 2 != 0 {
            complete = true;
            break;
        }
    }
    assert!(complete, "copied service completion");
    assert_eq!(read(s, REG_RESPONSE_TAG), tag);
    assert_eq!(read(s, REG_RESPONSE_CODE), 0);
    let result = core::array::from_fn(|index| read(s, REG_RESPONSE_DATA + 4 * index as u32));
    write(s, REG_RESPONSE_POP, tag);
    assert_eq!(read(s, REG_SUBMISSION_RESULT), RECEIPT_ACCEPTED);
    result
}
fn start_cfg(s: &mut Sim<'_>, state: bool, loops: bool, address: u64, value: Option<&Bits>) {
    let mut payload = [0; 12];
    payload[0] = u32::from(state) | (u32::from(loops) << 1) | (u32::from(value.is_some()) << 8);
    payload[1] = address.try_into().unwrap();
    if let Some(value) = value {
        payload[2..7].copy_from_slice(&value.resized(129).array::<5>());
    }
    start_request(s, SEMANTIC_SERVICE_CORE_CONFIG, &payload, 1);
}
fn cfg(s: &mut Sim<'_>, state: bool, loops: bool, address: u64, value: Option<&Bits>) -> Bits {
    start_cfg(s, state, loops, address, value);
    let result = finish_request(s, 1);
    assert_eq!(&result[5..], &[0; 7]);
    let words: [u32; 5] = result[..5].try_into().unwrap();
    Bits::from(words).resized(129)
}
fn initialize_observations(s: &mut Sim<'_>, slot: u64, gain: u32) {
    for word in 0..3 {
        cfg(
            s,
            true,
            true,
            slot * 3 + word,
            Some(&Bits::from_u64(if word == 0 { u64::from(gain) } else { 0 })),
        );
    }
}
fn upload(s: &mut Sim<'_>, address: u64, instruction: &Value) {
    start_cfg(
        s,
        false,
        false,
        address,
        Some(&codec::instruction(instruction).unwrap().resized(129)),
    );
    finish_request(s, 1);
}
fn reset(s: &mut Sim<'_>) {
    s.model.set_all(&[
        ("reset", 1),
        ("epoch", 1),
        ("arena_base", 0x0100_0000),
        ("run_quantum", 32),
        ("run_chunk_frames", 32),
        ("run_inputs", 1),
        ("run_outputs", 1),
        ("job_enable", 1),
        ("sample_input_data", 1 << 22),
    ]);
    s.idle(2);
    s.model.set("reset", 0);
    wait_initial(s);
}

#[shoop_test(no_wasm = "Native indexed MMIO, actual audio/media and DDR simulator")]
fn indexed_mmio_copies_real_128_row_snapshots_and_executes_audio_with_arm_responses_held() {
    let root = rt_fpga_testbench::root();
    let compiled = design()
        .build(&root.join("work/marlin-suite/semantic-mmio-audio"))
        .unwrap();
    let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    let geometry = Geometry::new([8, 64, 32], 2);
    let memory = Memory::attach(
        &mut s,
        Mode::Fixed,
        geometry.words() + 128,
        0x0100_0000,
        &root.join("work/marlin-suite/semantic-mmio-audio/memory"),
        91,
    )
    .unwrap();
    memory.borrow_mut().control.stalls = true;
    reset(&mut s);
    assert_eq!(read(&mut s, REG_SEMANTIC_ID), SEMANTIC_ID);
    assert_eq!(read(&mut s, REG_SNAPSHOT_LOOP_LIMIT), 128);
    assert_eq!(read(&mut s, REG_SNAPSHOT_READY), 1);
    assert_eq!(read(&mut s, REG_NEXT_SEQUENCE_LOW), 1);
    let header: Vec<_> = (0..16).map(|_| read(&mut s, REG_SNAPSHOT_DATA)).collect();
    assert_eq!(header[0], 0x53465331);
    assert_eq!(header[1], SEMANTIC_VERSION);
    assert_eq!(&header[2..12], &[1, 32, 1, 0, 0, 0, 0, 0, 128, 19]);
    write(&mut s, REG_SNAPSHOT_REWIND, 1);
    for _ in 0..4 {
        read(&mut s, REG_SNAPSHOT_DATA);
    }
    assert_eq!(read_hold(&mut s, REG_SNAPSHOT_DATA), 1);
    ack(&mut s, true, 0, 1, 1);
    let mut gains = vec![];
    let mut costs = vec![];
    for frame in [0, 32, 64] {
        costs.push(quantum(&mut s, frame, 0, &mut gains));
        assert_eq!(s.model.get("s_axi_rvalid"), 1);
        assert_eq!(s.model.get("s_axi_rdata"), 1);
    }
    assert_eq!(s.model.get("snapshot_sequence0"), 2);
    assert_eq!(s.model.get("snapshot_sequence1"), 3);
    assert_eq!(s.model.get("snapshot_skipped"), 1);
    release_read(&mut s, 1);
    assert_eq!(read(&mut s, REG_SNAPSHOT_SKIPPED_LOW), 1);
    ack(&mut s, true, 0, 2, 1);
    ack(&mut s, true, 1, 3, 1);
    assert_eq!(read(&mut s, REG_SNAPSHOT_RECEIPT), RECEIPT_ACCEPTED);
    initialize_observations(&mut s, 0, 1.0f32.to_bits());
    for word in 0..16 {
        cfg(
            &mut s,
            false,
            true,
            word,
            Some(&Bits::from_u64(match word {
                0 => 1 | (7 << 1),
                3 => 256,
                _ => 0,
            })),
        );
    }
    for (address, instruction) in [
        json!({"kind":"process_input","port":0,"destination":0}),
        json!({"kind":"process_gain","ramp":0,"source":0,"destination":1}),
        json!({"kind":"process_output","source":1,"port":0}),
    ]
    .iter()
    .enumerate()
    {
        upload(&mut s, address as u64, instruction);
    }
    cfg(
        &mut s,
        true,
        false,
        0,
        Some(&codec::state(&suites::audio::ramp(1 << 24, 1 << 24, 0, 0))),
    );
    assert_eq!(s.model.get("sync_frame"), 96, "preparation is not pacing");
    let mut select = [0; 12];
    select[0] = 5;
    select[6] = 1;
    select[8] = 3 << 16;
    select[9] = 1 << 16;
    select[10] = 2;
    publish_hold(&mut s, 0, 1, 0, &[select]);
    costs.push(quantum(&mut s, 96, 1 << 22, &mut gains));
    s.until(
        100_000,
        "real JP/cache/DMA switch fence with MMIO B held",
        |m| m.get("result_ready_mask") == 1,
    );
    assert_eq!(s.model.get("s_axi_bvalid"), 1);
    assert_eq!(s.model.get("s_axi_bresp"), 0);
    assert_eq!(s.model.get("active_revision"), 1);
    assert!(memory.borrow().reads() > 0);
    assert!(memory.borrow().writes() > 0);
    release_write(&mut s);
    assert_eq!(read(&mut s, REG_COMMAND_RECEIPT), RECEIPT_ACCEPTED);
    let result: Vec<_> = (0..17).map(|_| read(&mut s, REG_RESULT_DATA)).collect();
    assert_eq!(result[0], 0x53465231);
    assert_eq!(result[4], 1);
    assert_eq!(result[6], 96);
    assert_eq!(result[10], 1);
    assert_eq!(result[15], 2);
    assert_eq!(result[16], 1);
    write(&mut s, REG_RESULT_REWIND, 1);
    for _ in 0..4 {
        read(&mut s, REG_RESULT_DATA);
    }
    assert_eq!(read_hold(&mut s, REG_RESULT_DATA), 1);
    ack(&mut s, false, 0, 1, 1);
    let mut gain = [0; 12];
    gain[0] = 3;
    gain[2] = 7;
    gain[3] = 1;
    gain[5] = 0.5f32.to_bits();
    gain[6] = 1 << 23;
    publish_hold(&mut s, 0, 2, 1, &[gain; 32]);
    release_write(&mut s);
    costs.push(quantum(&mut s, 128, 1 << 21, &mut gains));
    assert_eq!(gains, vec![(0, u64::from(0.5f32.to_bits())); 32]);
    assert_eq!(s.model.get("result_ready_mask"), 1);
    release_read(&mut s, 1);
    assert_eq!(read(&mut s, REG_RESULT_RECEIPT), RECEIPT_ACCEPTED);
    write(&mut s, REG_RESULT_REWIND, 1);
    let result: Vec<_> = (0..48).map(|_| read(&mut s, REG_RESULT_DATA)).collect();
    assert_eq!(result[4], 2);
    assert_eq!(result[6], 128);
    assert_eq!(result[12], 1);
    assert_eq!(&result[16..], &[1; 32]);
    write(&mut s, REG_SNAPSHOT_BANK, 1);
    let snapshot: Vec<_> = (0..4880).map(|_| read(&mut s, REG_SNAPSHOT_DATA)).collect();
    assert_eq!(snapshot[0], 0x53465331);
    assert_eq!(snapshot[1], 0x10010);
    assert_eq!(
        &snapshot[2..16],
        &[1, 32, 6, 0, 128, 0, 1, 0, 128, 19, 1, 0, 0, 0]
    );
    assert_eq!(snapshot[16], 1 | (7 << 1));
    assert_eq!(snapshot[48], 0.5f32.to_bits());
    assert_eq!(&snapshot[49..54], &[0; 5]);
    assert_eq!(read(&mut s, REG_SNAPSHOT_DATA), 0);
    assert_eq!(read(&mut s, REG_SNAPSHOT_RECEIPT), RECEIPT_UNAVAILABLE);
    assert_eq!(s.model.get("sync_frame"), 160, "readback is not pacing");
    assert_eq!(s.model.get("faulted"), 0);
    println!(
        "SEMANTIC_MMIO_AUDIO_QUANTUM_CYCLES {costs:?}; command {}; snapshot {}; DDR reads {}; writes {}",
        s.model.get("peak_command_cycles"),
        s.model.get("peak_snapshot_cycles"),
        memory.borrow().reads(),
        memory.borrow().writes()
    );
}

#[shoop_test(no_wasm = "Native indexed MMIO, actual AXI drain/reset and retained diagnostics")]
fn indexed_mmio_uses_safe_runtime_reset_and_the_retained_full_width_fault_sequence() {
    let root = rt_fpga_testbench::root();
    let compiled = design()
        .build(&root.join("work/marlin-suite/semantic-mmio-audio"))
        .unwrap();
    let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    let geometry = Geometry::new([8, 64, 32], 2);
    let memory = Memory::attach(
        &mut s,
        Mode::Fixed,
        geometry.words() + 128,
        0x0100_0000,
        &root.join("work/marlin-suite/semantic-mmio-audio/memory"),
        91,
    )
    .unwrap();
    memory.borrow_mut().control.stalls = true;
    reset(&mut s);
    let sequence = 0x1234_5678_0000_0042;
    for (name, value) in [
        ("last_admitted", sequence - 1),
        ("head_sequence", sequence),
        ("release_sequence", sequence),
    ] {
        s.model
            .inject(&format!("{BANKS}.{name}"), Some(&Bits::from_u64(value)));
    }
    s.idle(1);
    for name in ["last_admitted", "head_sequence", "release_sequence"] {
        s.model.inject(&format!("{BANKS}.{name}"), None);
    }
    let mut refused = [0; 12];
    refused[0] = 6;
    refused[4] = 5;
    memory.borrow_mut().control.hold_aw = true;
    start_cfg(
        &mut s,
        false,
        false,
        0,
        Some(
            &codec::instruction(&json!({"kind":"process_input","port":0,"destination":0}))
                .unwrap()
                .resized(129),
        ),
    );
    publish_hold(&mut s, 0, sequence, 0, &[refused]);
    s.until(10_000, "actual held DDR upload AW", |m| {
        m.get("m_axi_awvalid") != 0
    });
    s.model.set("run_valid", 1);
    s.idle(1);
    s.model.set("run_valid", 0);
    s.until(
        4096,
        "accepted full-width group before local refusal",
        |m| m.get("diagnostic_opcode") == 6,
    );
    s.model.set("pipeline_fault", 1);
    s.idle(1);
    s.model.set("pipeline_fault", 0);
    s.idle(4);
    let saved = first_fault(&s);
    assert_eq!(saved[0], 1);
    assert_eq!(saved[1], 64);
    assert_eq!(saved[7], sequence);
    assert_eq!(read_hold(&mut s, REG_FAULT_SEQUENCE_HIGH), 0x12345678);
    release_write(&mut s);
    write_hold(&mut s, REG_RECOVERY_REQUEST, 1);
    s.idle(64);
    assert_eq!(s.model.get("recovery_state"), 1);
    assert_eq!(s.model.get("runtime_epoch"), 1);
    assert_eq!(s.model.get("runtime_reset"), 0);
    assert_eq!(s.model.get("m_axi_awvalid"), 1);
    assert_eq!(s.model.get("s_axi_bvalid"), 1);
    assert_eq!(s.model.get("s_axi_rdata"), 0x12345678);
    memory.borrow_mut().control.hold_aw = false;
    s.until(
        20_000,
        "actual external write retirement then runtime reset",
        |m| m.get("recovery_state") == 3,
    );
    s.until(8192, "fresh local capture without ARM R/B readiness", |m| {
        m.get("run_ready") != 0
    });
    assert_eq!(s.model.get("runtime_epoch"), 2);
    assert!(memory.borrow().writes() > 0);
    assert_eq!(first_fault(&s), saved);
    assert_eq!(s.model.get("diagnostic_sequence"), 0);
    release_read(&mut s, 0x12345678);
    release_write(&mut s);
    for (register, value) in [
        (REG_SUBMISSION_RESULT, 0),
        (REG_FAULT_EPOCH, saved[0]),
        (REG_FAULT_CAUSE, saved[1]),
        (REG_FAULT_OP, saved[3]),
        (REG_FAULT_CYCLES, saved[4]),
        (REG_FAULT_CONTROL_CYCLES, saved[5]),
        (REG_FAULT_DETAIL, saved[6]),
        (REG_FAULT_REVISION_LOW, saved[8] & 0xffffffff),
        (REG_FAULT_REVISION_HIGH, saved[8] >> 32),
        (REG_FAULT_TAG, saved[7] & 0xffffffff),
        (REG_FAULT_SEQUENCE_HIGH, saved[7] >> 32),
        (REG_FAULT_FRAME_LOW, saved[9] & 0xffffffff),
        (REG_FAULT_FRAME_HIGH, saved[9] >> 32),
    ] {
        assert_eq!(
            u64::from(read(&mut s, register)),
            value,
            "register {register:#x}"
        );
    }
    assert_eq!(read(&mut s, REG_FAULT_SEQUENCE_HIGH), 0x12345678);
    assert_eq!(read(&mut s, REG_NEXT_SEQUENCE_LOW), 1);
    assert_eq!(read(&mut s, REG_LAST_ADMITTED_LOW), 0);
    assert_eq!(read(&mut s, REG_RESULT_READY), 0);
    assert_eq!(read(&mut s, REG_SNAPSHOT_READY), 1);
    write(&mut s, REG_COMMAND_BEGIN, 1);
    assert_eq!(read(&mut s, REG_COMMAND_RECEIPT), RECEIPT_EPOCH);
    ack(&mut s, true, 0, 1, 1);
    assert_eq!(read(&mut s, REG_SNAPSHOT_RECEIPT), RECEIPT_EPOCH);
    assert_eq!(read(&mut s, REG_SNAPSHOT_READY), 1);
    quantum(&mut s, 0, 0, &mut vec![]);
    assert_eq!(first_fault(&s), saved);
    s.model.set("pipeline_fault", 1);
    s.idle(1);
    s.model.set("pipeline_fault", 0);
    s.idle(4);
    assert_eq!(s.model.get("first_fault_epoch"), 2);
    assert_eq!(s.model.get("first_fault_sequence"), 0);
    assert_eq!(read(&mut s, REG_FAULT_SEQUENCE_HIGH), 0);
}

#[shoop_test(no_wasm = "Native public discovery and recovery against actual AXI ownership")]
fn indexed_mmio_discovers_geometry_counts_audio_and_refuses_reset_crossing_safety_halves() {
    let root = rt_fpga_testbench::root();
    let profile = design();
    let geometry_registers = [
        (REG_PROGRAM_DEPTH, "PROGRAM_DEPTH"),
        (REG_STATE_DEPTH, "STATE_DEPTH"),
        (REG_JOB_DEPTH, "JOB_DEPTH"),
        (REG_BUFFERS, "BUFFERS"),
        (REG_VIEWS, "VIEWS"),
        (REG_CHUNK_FRAMES, "CHUNK_FRAMES"),
        (REG_BLOCK_CHUNKS, "BLOCK_CHUNKS"),
    ]
    .map(|(register, name)| (register, profile.parameters[name].parse::<u32>().unwrap()));
    let media_blocks = profile.parameters["SLOTS"].parse::<u32>().unwrap()
        / profile.parameters["BLOCK_CHUNKS"].parse::<u32>().unwrap();
    let compiled = profile
        .build(&root.join("work/marlin-suite/semantic-mmio-audio"))
        .unwrap();
    for (old_half, late_address) in [
        ("aw", REG_RECOVERY_REQUEST),
        ("w", REG_RECOVERY_REQUEST),
        ("aw", REG_ABORT),
        ("w", REG_ABORT),
    ] {
        let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
        let geometry = Geometry::new([8, 64, 32], 2);
        let memory = Memory::attach(
            &mut s,
            Mode::Fixed,
            geometry.words() + 128,
            0x0100_0000,
            &root.join("work/marlin-suite/semantic-mmio-audio/memory"),
            92,
        )
        .unwrap();
        memory.borrow_mut().control.stalls = true;
        reset(&mut s);
        for (register, value) in geometry_registers {
            assert_eq!(read(&mut s, register), value);
        }
        for (register, value) in [
            (REG_ID, SEMANTIC_DISCOVERY_ID),
            (REG_VERSION, 16),
            (REG_CORE_CLOCK_HZ, 100_000_000),
            (REG_ARENA_BASE, 0x0100_0000),
            (REG_ARENA_BYTES, 0x0800_0000),
            (REG_MEDIA_BLOCKS, media_blocks),
            (REG_QUANTUM, 32),
            (REG_RECOVERY_ID, 0x52535431),
            (REG_RECOVERY_STATUS, 0),
            (REG_EPOCH, 1),
            (REG_STATUS, 1),
            (REG_COMPLETED_QUANTA, 0),
            (REG_PEAK_QUANTUM_CYCLES, 0),
            (REG_FAULT_EPOCH, 0),
        ] {
            assert_eq!(read(&mut s, register), value, "register {register:#x}");
        }
        s.model.set("audio_xruns", 0x80000012);
        assert_eq!(read(&mut s, REG_AUDIO_XRUNS), 0x80000012);
        assert_eq!(s.model.get("sync_frame"), 0);
        assert_eq!(s.model.get("irq"), 1);
        ack(&mut s, true, 0, 1, 1);
        assert_eq!(s.model.get("irq"), 0);
        let cycles = quantum(&mut s, 0, 0, &mut vec![]);
        assert_eq!(read(&mut s, REG_COMPLETED_QUANTA), 1);
        assert_eq!(u64::from(read(&mut s, REG_PEAK_QUANTUM_CYCLES)), cycles);
        assert_eq!(s.model.get("irq"), 1);
        ack(&mut s, true, 0, 2, 1);
        assert_eq!(s.model.get("irq"), 0);
        s.model.set_all(&[
            ("s_axi_awaddr", u64::from(REG_RECOVERY_REQUEST)),
            ("s_axi_wdata", 1),
            ("s_axi_wstrb", 3),
        ]);
        channel(&mut s, "s_axi_wvalid", "s_axi_wready");
        channel(&mut s, "s_axi_awvalid", "s_axi_awready");
        s.until(32, "malformed recovery strobe", |m| {
            m.get("s_axi_bvalid") != 0
        });
        assert_eq!(s.model.get("s_axi_bresp"), 2);
        s.model.set("s_axi_bready", 1);
        s.idle(1);
        s.model.set("s_axi_bready", 0);
        assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_UNAVAILABLE);
        write(&mut s, REG_RECOVERY_REQUEST, 0);
        assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_EPOCH);
        assert_eq!(read(&mut s, REG_RECOVERY_STATUS), 0);
        memory.borrow_mut().control.hold_aw = true;
        start_cfg(
            &mut s,
            false,
            false,
            0,
            Some(&Bits::from_u64(0).resized(129)),
        );
        s.until(10_000, "actual DDR AW before public recovery", |m| {
            m.get("m_axi_awvalid") != 0
        });
        write(&mut s, REG_RECOVERY_REQUEST, 1);
        assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_ACCEPTED);
        assert_eq!(read(&mut s, REG_RECOVERY_STATUS), 1);
        write(&mut s, REG_RECOVERY_REQUEST, 1);
        assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_UNAVAILABLE);
        s.model.set_all(&[
            ("s_axi_awaddr", u64::from(late_address)),
            ("s_axi_wdata", 2),
            ("s_axi_wstrb", 15),
        ]);
        let (valid, ready, other_valid, other_ready) = if old_half == "aw" {
            (
                "s_axi_awvalid",
                "s_axi_awready",
                "s_axi_wvalid",
                "s_axi_wready",
            )
        } else {
            (
                "s_axi_wvalid",
                "s_axi_wready",
                "s_axi_awvalid",
                "s_axi_awready",
            )
        };
        channel(&mut s, valid, ready);
        assert_eq!(read_hold(&mut s, REG_EPOCH), 1);
        memory.borrow_mut().control.hold_aw = false;
        s.until(20_000, "real retirement and public-requested reset", |m| {
            m.get("recovery_state") == 3
        });
        s.until(
            8192,
            "fresh startup with incomplete old AXI-Lite write",
            |m| m.get("run_ready") != 0,
        );
        assert!(memory.borrow().writes() > 0);
        assert_eq!(s.model.get("runtime_epoch"), 2);
        assert_eq!(s.model.get("sync_frame"), 0);
        release_read(&mut s, 1);
        channel(&mut s, other_valid, other_ready);
        s.until(32, "old-half recovery refusal", |m| {
            m.get("s_axi_bvalid") != 0
        });
        release_write(&mut s);
        assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_EPOCH);
        assert_eq!(read(&mut s, REG_RECOVERY_STATUS), 3);
        assert_eq!(read(&mut s, REG_EPOCH), 2);
        assert_eq!(read(&mut s, REG_COMPLETED_QUANTA), 0);
        assert_eq!(read(&mut s, REG_PEAK_QUANTUM_CYCLES), 0);
        assert_eq!(
            s.model.get("faulted"),
            0,
            "old half must not abort the fresh runtime"
        );
        assert_eq!(read(&mut s, REG_FAULT_EPOCH), 1);
        assert_eq!(read(&mut s, REG_FAULT_CAUSE), 0x90);
        assert_eq!(read(&mut s, REG_FAULT_TAG), 0);
        assert_eq!(read(&mut s, REG_FAULT_SEQUENCE_HIGH), 0);
        assert_eq!(read(&mut s, REG_FAULT_FRAME_LOW), 32);
        assert_eq!(read(&mut s, REG_FAULT_FRAME_HIGH), 0);
        assert_eq!(read(&mut s, REG_FAULT_REVISION_LOW), 0);
        assert_eq!(read(&mut s, REG_FAULT_REVISION_HIGH), 0);
        assert_eq!(read(&mut s, REG_FAULT_OP), u32::MAX);
        s.idle(32);
        assert_eq!(
            s.model.get("runtime_epoch"),
            2,
            "no recovery replay from {old_half}"
        );
        assert_eq!(s.model.get("recovery_state"), 3);
    }
}
