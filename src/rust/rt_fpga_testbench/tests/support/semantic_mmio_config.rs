use super::{
    Geometry, Memory, Mode, Sim, ack, channel, design, finish_request, initialize_observations,
    quantum, read, read_hold, release_read, release_write, reset, start_request, write, write_hold,
};
use rt_fpga_rtl::host::semantic_registers::*;
use shoop_wasm_test_support::shoop_test;

fn response_ready(s: &mut Sim<'_>) {
    let mut ready = false;
    for _ in 0..20_000 {
        if read(s, REG_STATUS) & 2 != 0 {
            ready = true;
            break;
        }
    }
    assert!(
        ready,
        "copied service response independent of reader readiness"
    );
}
fn request_writes(s: &mut Sim<'_>, operation: u32, payload: &[u32; 12], tag: u32) {
    let epoch = s.model.get("runtime_epoch") as u32;
    write(s, REG_REQUEST_EPOCH, epoch);
    write(s, REG_REQUEST_OP, operation);
    for (index, word) in payload.iter().enumerate() {
        write(s, REG_REQUEST_DATA + index as u32 * 4, *word);
    }
    write(s, REG_REQUEST_PUSH, tag);
}

#[shoop_test(no_wasm = "Native public configuration, real SP/JP/media and DDR ownership")]
fn public_config_prefixes_and_all_three_endpoints_never_hold_audio_or_copied_arm_data() {
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
        95,
    )
    .unwrap();
    memory.borrow_mut().control.stalls = true;
    reset(&mut s);
    ack(&mut s, true, 0, 1, 1);
    initialize_observations(&mut s, 0, 1.0f32.to_bits());
    write(&mut s, REG_REQUEST_OP, 1);
    assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_INCOMPLETE);
    let mut payload = [0; 12];
    payload[0] = 0x103;
    payload[2] = 0.5f32.to_bits();
    let mut packet = vec![1, SEMANTIC_SERVICE_CORE_CONFIG];
    packet.extend(payload);
    for (index, word) in packet.into_iter().enumerate() {
        write(&mut s, REG_REQUEST_PUSH, 7);
        assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_INCOMPLETE);
        assert_eq!(read(&mut s, REG_STATUS) & 3, 1);
        assert_eq!(
            super::super::observations(&mut s, 0).get(0, 32),
            u64::from(1.0f32.to_bits()),
            "unpublished prefix {index}"
        );
        quantum(&mut s, index as u64 * 32, 0, &mut vec![]);
        write(&mut s, REG_REQUEST_EPOCH + index as u32 * 4, word);
    }
    write_hold(&mut s, REG_REQUEST_PUSH, 7);
    quantum(&mut s, 448, 0, &mut vec![]);
    response_ready(&mut s);
    assert_eq!(s.model.get("s_axi_bvalid"), 1);
    assert_eq!(read(&mut s, REG_STATUS) & 3, 2);
    assert_eq!(
        super::super::observations(&mut s, 0).get(0, 32),
        u64::from(0.5f32.to_bits())
    );
    release_write(&mut s);
    let copied = read_hold(&mut s, REG_RESPONSE_DATA);
    write(&mut s, REG_RESPONSE_POP, 7);
    let mut audio = [0; 12];
    audio[0] = 0x102;
    audio[2] = 0x1357_2468;
    request_writes(&mut s, SEMANTIC_SERVICE_AUDIO_CONFIG, &audio, 8);
    for frame in [480, 512, 544] {
        quantum(&mut s, frame, 0, &mut vec![]);
        assert_eq!(s.model.get("s_axi_rvalid"), 1);
        assert_eq!(s.model.get("s_axi_rdata"), u64::from(copied));
        assert_eq!(s.model.get("s_axi_rresp"), 0);
    }
    release_read(&mut s, copied);
    finish_request(&mut s, 8);
    audio[0] = 2;
    audio[2] = 0;
    let frame = s.model.get("sync_frame");
    start_request(&mut s, SEMANTIC_SERVICE_AUDIO_CONFIG, &audio, 9);
    let result = finish_request(&mut s, 9);
    assert_eq!(result[0], 0x1357_2468);
    assert_eq!(&result[1..], &[0; 11]);
    let descriptor = [
        0x0102_0304,
        0x1122_3344,
        0x0001_0000,
        0x8000_0000,
        0xdead_beef,
    ];
    let mut store = [0; 12];
    store[0] = 0x101;
    store[2..7].copy_from_slice(&descriptor);
    start_request(&mut s, SEMANTIC_SERVICE_STORE_CONFIG, &store, 10);
    finish_request(&mut s, 10);
    store[0] = 1;
    store[2..7].fill(0);
    start_request(&mut s, SEMANTIC_SERVICE_STORE_CONFIG, &store, 11);
    let result = finish_request(&mut s, 11);
    assert_eq!(&result[..5], &descriptor);
    assert_eq!(&result[5..], &[0; 7]);
    start_request(&mut s, 16, &[0; 12], 12);
    response_ready(&mut s);
    assert_eq!(read(&mut s, REG_RESPONSE_TAG), 12);
    assert_eq!(read(&mut s, REG_RESPONSE_CODE), 1);
    for index in 0..12 {
        assert_eq!(read(&mut s, REG_RESPONSE_DATA + index * 4), 0);
    }
    write(&mut s, REG_RESPONSE_POP, 12);
    assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_ACCEPTED);
    assert_eq!(
        s.model.get("sync_frame"),
        frame,
        "RPC/polling do not advance time"
    );
    assert_eq!(read(&mut s, REG_NEXT_SEQUENCE_LOW), 1);
    assert_eq!(read(&mut s, REG_LAST_ADMITTED_LOW), 0);
    audio[0] = 3;
    start_request(&mut s, SEMANTIC_SERVICE_AUDIO_CONFIG, &audio, 13);
    response_ready(&mut s);
    assert_eq!(read(&mut s, REG_RESPONSE_TAG), 13);
    assert_eq!(
        read(&mut s, REG_RESPONSE_CODE),
        3,
        "history invalidation space is write-only"
    );
    for index in 0..12 {
        assert_eq!(read(&mut s, REG_RESPONSE_DATA + index * 4), 0);
    }
    write(&mut s, REG_RESPONSE_POP, 13);
    assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_ACCEPTED);
    quantum(&mut s, frame, 0, &mut vec![]);
    assert_eq!(s.model.get("faulted"), 0);
}

#[shoop_test(no_wasm = "Native service halves across actual safe runtime reset and held AXI RDATA")]
fn public_config_authority_poison_survives_safe_reset_with_a_copied_reply_held() {
    let root = rt_fpga_testbench::root();
    let compiled = design()
        .build(&root.join("work/marlin-suite/semantic-mmio-audio"))
        .unwrap();
    for aw_first in [false, true] {
        let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
        let geometry = Geometry::new([8, 64, 32], 2);
        let memory = Memory::attach(
            &mut s,
            Mode::Fixed,
            geometry.words() + 128,
            0x0100_0000,
            &root.join("work/marlin-suite/semantic-mmio-audio/memory"),
            96 + u64::from(aw_first),
        )
        .unwrap();
        memory.borrow_mut().control.stalls = true;
        reset(&mut s);
        let mut audio = [0; 12];
        audio[0] = 0x102;
        audio[2] = 0x5152_5354;
        start_request(&mut s, SEMANTIC_SERVICE_AUDIO_CONFIG, &audio, 1);
        finish_request(&mut s, 1);
        audio[0] = 2;
        audio[2] = 0;
        start_request(&mut s, SEMANTIC_SERVICE_AUDIO_CONFIG, &audio, 2);
        response_ready(&mut s);
        assert_eq!(read_hold(&mut s, REG_RESPONSE_DATA), 0x5152_5354);
        write(&mut s, REG_RESPONSE_POP, 2);
        memory.borrow_mut().control.hold_aw = true;
        let mut upload = [0; 12];
        upload[0] = 0x100;
        request_writes(&mut s, SEMANTIC_SERVICE_CORE_CONFIG, &upload, 3);
        s.until(10_000, "actual DDR owner before recovery", |m| {
            m.get("m_axi_awvalid") != 0
        });
        write(&mut s, REG_RECOVERY_REQUEST, 1);
        assert_eq!(s.model.get("recovery_state"), 1);
        s.model.set_all(&[
            ("s_axi_awaddr", u64::from(REG_REQUEST_EPOCH)),
            ("s_axi_wdata", 2),
            ("s_axi_wstrb", 15),
        ]);
        let (first, second) = if aw_first {
            (
                ("s_axi_awvalid", "s_axi_awready"),
                ("s_axi_wvalid", "s_axi_wready"),
            )
        } else {
            (
                ("s_axi_wvalid", "s_axi_wready"),
                ("s_axi_awvalid", "s_axi_awready"),
            )
        };
        channel(&mut s, first.0, first.1);
        memory.borrow_mut().control.hold_aw = false;
        s.until(20_000, "real DDR retirement then fresh reset", |m| {
            m.get("recovery_state") == 3
        });
        s.until(8192, "fresh initial capture without reply reader", |m| {
            m.get("run_ready") != 0
        });
        assert_eq!(s.model.get("runtime_epoch"), 2);
        assert_eq!(s.model.get("s_axi_rvalid"), 1);
        assert_eq!(s.model.get("s_axi_rdata"), 0x5152_5354);
        channel(&mut s, second.0, second.1);
        s.until(32, "old service half refused", |m| {
            m.get("s_axi_bvalid") != 0
        });
        release_write(&mut s);
        release_read(&mut s, 0x5152_5354);
        assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_EPOCH);
        assert_eq!(read(&mut s, REG_STATUS) & 3, 1);
        write(&mut s, REG_REQUEST_OP, SEMANTIC_SERVICE_CORE_CONFIG);
        assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_INCOMPLETE);
        assert_eq!(s.model.get("runtime_epoch"), 2);
        assert_eq!(s.model.get("faulted"), 0);
        assert!(memory.borrow().writes() > 0);
        quantum(&mut s, 0, 0, &mut vec![]);
    }
}
