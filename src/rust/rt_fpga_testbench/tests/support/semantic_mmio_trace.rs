use super::{
    Bits, Geometry, Memory, Mode, Sim, ack, cfg, codec, design, initialize_observations,
    publish_hold, quantum, read, read_hold, release_read, release_write, reset, suites, upload,
    wait_initial, write_hold,
};
use rt_fpga_rtl::host::semantic_registers::*;
use serde_json::json;
use shoop_wasm_test_support::shoop_test;

fn packet(s: &Sim<'_>) -> [u32; 11] {
    assert_eq!(s.model.get("trace_valid"), 1);
    let revision = s.model.get("trace_revision");
    let mut words = [0; 11];
    words[..3].copy_from_slice(&[
        s.model.get("trace_epoch") as u32,
        revision as u32,
        (revision >> 32) as u32,
    ]);
    words[3..].copy_from_slice(&s.model.bits("trace_data").array::<8>());
    words
}

#[shoop_test(no_wasm = "Native optional trace, actual SP observations and safe AXI recovery")]
fn public_trace_partial_and_stalled_readers_cannot_pace_or_replace_mandatory_observations() {
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
        127,
    )
    .unwrap();
    memory.borrow_mut().control.stalls = true;
    reset(&mut s);
    ack(&mut s, true, 0, 1, 1);
    assert_eq!(read(&mut s, REG_TRACE_STATUS), 0);
    assert_eq!(read(&mut s, REG_TRACE_LOST_LOW), 0);
    initialize_observations(&mut s, 0, 1.0f32.to_bits());
    for word in 0..16 {
        let value = match word {
            0 => 1 | (7 << 1) | (1 << 33) | (1 << 35) | (1 << 36),
            1 => 7,
            2 | 3 => 32,
            _ => 0,
        };
        cfg(&mut s, false, true, word, Some(&Bits::from_u64(value)));
    }
    let mut sync = Bits::zero(96);
    sync.set(0, 8, 12).set(24, 32, 7).set(72, 16, 1);
    cfg(&mut s, false, false, 0, Some(&sync));
    for (address, instruction) in [
        json!({"kind":"process_input","port":0,"destination":0}),
        json!({"kind":"process_gain","ramp":0,"source":0,"destination":1}),
        json!({"kind":"process_output","source":1,"port":0}),
    ]
    .iter()
    .enumerate()
    {
        upload(&mut s, address as u64 + 1, instruction);
    }
    cfg(
        &mut s,
        true,
        false,
        0,
        Some(&codec::state(&suites::audio::ramp(1 << 24, 1 << 24, 0, 0))),
    );
    let mut select = [0; 12];
    select[0] = 5;
    select[6] = 1;
    select[8] = 4 << 16;
    select[9] = 1 | (1 << 16);
    select[10] = 2;
    publish_hold(&mut s, 0, 1, 0, &[select]);
    let mut gains = vec![];
    quantum(&mut s, 0, 1 << 22, &mut gains);
    s.until(100_000, "actual program/cache/DMA fence", |m| {
        m.get("result_ready_mask") == 1
    });
    release_write(&mut s);
    ack(&mut s, false, 0, 1, 1);
    let held = packet(&s);
    assert_eq!(&held[..3], &[1, 1, 0]);
    assert_eq!(s.model.bits("trace_data").get(192, 64), 0);
    let initial_losses = s.model.get("trace_lost");
    let mut frame = 32;
    for (index, expected) in held.iter().take(10).enumerate() {
        assert_eq!(read(&mut s, REG_TRACE_DATA + 4 * index as u32), *expected);
        quantum(&mut s, frame, 1 << 22, &mut gains);
        assert_eq!(cfg(&mut s, true, true, 1, None).get(0, 64), frame);
        assert_eq!(cfg(&mut s, true, true, 2, None).get(0, 1), 1);
        assert_eq!(packet(&s), held);
        assert_eq!(s.model.get("faulted"), 0);
        frame += 32;
    }
    assert!(s.model.get("trace_lost") > initial_losses);
    let copied = read_hold(&mut s, REG_TRACE_DATA + 40);
    assert_eq!(copied, held[10]);
    assert_eq!(s.model.get("trace_valid"), 0);
    quantum(&mut s, frame, 1 << 22, &mut gains);
    assert_eq!(s.model.bits("trace_data").get(192, 64), frame);
    frame += 32;
    let replacement = packet(&s);
    release_read(&mut s, copied);
    assert_eq!(read(&mut s, REG_TRACE_DATA + 40), 0);
    assert_eq!(read(&mut s, REG_TRACE_RECEIPT), RECEIPT_INCOMPLETE);
    assert_eq!(packet(&s), replacement);
    let losses = read_hold(&mut s, REG_TRACE_LOST_LOW);
    quantum(&mut s, frame, 1 << 22, &mut gains);
    assert!(s.model.get("trace_lost") > u64::from(losses));
    assert_eq!(s.model.get("s_axi_rdata"), u64::from(losses));
    frame += 32;
    release_read(&mut s, losses);
    for (index, expected) in replacement.iter().enumerate() {
        assert_eq!(read(&mut s, REG_TRACE_DATA + 4 * index as u32), *expected);
    }
    assert_eq!(read(&mut s, REG_TRACE_STATUS), 0);
    assert_eq!(
        s.model.get("sync_frame"),
        frame,
        "trace reads do not advance F"
    );
    quantum(&mut s, frame, 1 << 22, &mut gains);
    let old = packet(&s);
    for (index, expected) in old.iter().take(3).enumerate() {
        assert_eq!(read(&mut s, REG_TRACE_DATA + 4 * index as u32), *expected);
    }
    let copied = read_hold(&mut s, REG_TRACE_DATA + 12);
    assert_eq!(copied, old[3]);
    write_hold(&mut s, REG_RECOVERY_REQUEST, 1);
    s.until(100_000, "real trace runtime reset", |m| {
        m.get("runtime_epoch") == 2 && m.get("recovery_state") == 3 && m.get("runtime_reset") == 0
    });
    wait_initial(&mut s);
    assert_eq!(s.model.get("trace_valid"), 0);
    assert_eq!(s.model.get("trace_lost"), 0);
    assert_eq!(s.model.get("sync_frame"), 0);
    release_read(&mut s, copied);
    release_write(&mut s);
    assert_eq!(read(&mut s, REG_TRACE_RECEIPT), RECEIPT_EPOCH);
    assert_eq!(read(&mut s, REG_TRACE_DATA + 16), 0);
    assert_eq!(read(&mut s, REG_TRACE_RECEIPT), RECEIPT_UNAVAILABLE);
    assert_eq!(read(&mut s, REG_TRACE_STATUS), 0);
}
