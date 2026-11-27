use super::configuration::{request_writes, response_ready};
use super::{
    Geometry, Memory, Mode, Probe, Sim, ack, design, finish_request, quantum, read, read_hold,
    release_read, release_write, reset, start_request, wait_initial, write, write_hold,
};
use rt_fpga_rtl::host::semantic_registers::*;
use shoop_wasm_test_support::shoop_test;

const OWNER: &str = "pipeline.execution.core.owner.owner";
const LOADER: &str = "pipeline.execution.core.owner.loader.phase";

pub(super) fn payload(opcode: u32, view: u64, start: u64, length: u64, count: u32) -> [u32; 12] {
    [
        opcode,
        view as u32,
        (view >> 32) as u32,
        start as u32,
        (start >> 32) as u32,
        length as u32,
        (length >> 32) as u32,
        count,
        0,
        0,
        0,
        0,
    ]
}
pub(super) fn command(s: &mut Sim<'_>, args: [u32; 12], tag: u32) -> [u32; 12] {
    start_request(s, SEMANTIC_SERVICE_MEDIA_COMMAND, &args, tag);
    finish_request(s, tag)
}
pub(super) fn authorize(s: &mut Sim<'_>, tag: u32) {
    for _ in 0..64 {
        write(s, REG_MEDIA_TAG, tag);
        let receipt = read(s, REG_MEDIA_RECEIPT);
        if receipt == RECEIPT_ACCEPTED {
            return;
        }
        assert_eq!(
            receipt, RECEIPT_UNAVAILABLE,
            "descriptor validation may still be pending"
        );
    }
    panic!("private media tag authority");
}
fn malformed(s: &mut Sim<'_>, writing: bool, address: u32) {
    if writing {
        s.model.set_all(&[
            ("s_axi_awaddr", u64::from(address)),
            ("s_axi_wdata", 0xdead_beef),
            ("s_axi_wstrb", 7),
        ]);
        super::channel(s, "s_axi_awvalid", "s_axi_awready");
        super::channel(s, "s_axi_wvalid", "s_axi_wready");
        s.until(32, "malformed bulk write", |m| m.get("s_axi_bvalid") == 1);
        assert_eq!(s.model.get("s_axi_bresp"), 2);
        s.model.set("s_axi_bready", 1);
        s.idle(1);
        s.model.set("s_axi_bready", 0);
    } else {
        s.model.set("s_axi_araddr", u64::from(address));
        super::channel(s, "s_axi_arvalid", "s_axi_arready");
        s.until(32, "malformed bulk read", |m| m.get("s_axi_rvalid") == 1);
        assert_eq!(s.model.get("s_axi_rresp"), 2);
        s.model.set("s_axi_rready", 1);
        s.idle(1);
        s.model.set("s_axi_rready", 0);
    }
}
fn render(s: &mut Sim<'_>, frame: &mut u64) {
    quantum(s, *frame, 0, &mut vec![]);
    *frame += 32;
}
fn failed(s: &mut Sim<'_>, args: [u32; 12], tag: u32, code: u32) {
    start_request(s, SEMANTIC_SERVICE_MEDIA_COMMAND, &args, tag);
    response_ready(s);
    assert_eq!(read(s, REG_RESPONSE_CODE), code);
    for index in 0..12 {
        assert_eq!(read(s, REG_RESPONSE_DATA + 4 * index), 0);
    }
    assert_eq!(read(s, REG_MEDIA_READ), 0);
    write(s, REG_RESPONSE_POP, tag);
    assert_eq!(read(s, REG_SUBMISSION_RESULT), RECEIPT_ACCEPTED);
}

#[shoop_test(no_wasm = "Native public bulk service, actual private loader/store and DDR fences")]
fn public_media_complete_uploads_and_copied_reads_never_retain_the_backend_or_pace_audio() {
    let root = rt_fpga_testbench::root();
    let mut d = design();
    d.probes
        .extend([OWNER, LOADER, "store_cmd_valid"].map(Probe::observe));
    let compiled = d
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
        131,
    )
    .unwrap();
    memory.borrow_mut().control.stalls = true;
    reset(&mut s);
    ack(&mut s, true, 0, 1, 1);
    assert_eq!(
        &command(&mut s, payload(0, 0, 0, 0, 0), 10)[..3],
        &[0, 0, 0]
    );
    assert_eq!(command(&mut s, payload(5, 0, 0, 0, 0), 26), [0; 12]);
    let args = payload(5, 0, 0, 0, 32);
    let mut words = vec![1, SEMANTIC_SERVICE_MEDIA_COMMAND];
    words.extend(args);
    let mut frame = 0;
    for (index, word) in words.iter().enumerate() {
        write(&mut s, REG_REQUEST_PUSH, 11);
        assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_INCOMPLETE);
        assert_eq!(s.model.get("store_cmd_valid"), 0);
        render(&mut s, &mut frame);
        write(&mut s, REG_REQUEST_EPOCH + 4 * index as u32, *word);
    }
    write_hold(&mut s, REG_REQUEST_PUSH, 11);
    render(&mut s, &mut frame);
    assert_eq!(s.model.get(LOADER), 1);
    assert_eq!(s.model.get(OWNER), 0);
    release_write(&mut s);
    write(&mut s, REG_MEDIA_WRITE, 0xdead_beef);
    assert_eq!(read(&mut s, REG_MEDIA_RECEIPT), RECEIPT_OLD_SEQUENCE);
    write(&mut s, REG_MEDIA_TAG, 99);
    assert_eq!(read(&mut s, REG_MEDIA_RECEIPT), RECEIPT_OLD_SEQUENCE);
    authorize(&mut s, 11);
    assert_eq!(read(&mut s, REG_MEDIA_STATUS), 5);
    malformed(&mut s, true, REG_MEDIA_WRITE);
    for index in 0..32 {
        render(&mut s, &mut frame);
        assert_eq!(s.model.get(LOADER), 1, "partial body {index}");
        assert_eq!(
            s.model.get(OWNER),
            0,
            "partial body cannot acquire the store"
        );
        if index == 31 {
            write_hold(&mut s, REG_MEDIA_WRITE, 0xa000_0000 + index);
        } else {
            write(&mut s, REG_MEDIA_WRITE, 0xa000_0000 + index);
            assert_eq!(read(&mut s, REG_MEDIA_RECEIPT), RECEIPT_ACCEPTED);
        }
    }
    render(&mut s, &mut frame);
    response_ready(&mut s);
    assert_eq!(read(&mut s, REG_RESPONSE_CODE), 0);
    assert_eq!(s.model.get("store_cmd_ready"), 1);
    assert_eq!(s.model.get("store_done"), 0);
    assert_eq!(s.model.get(OWNER), 0);
    render(&mut s, &mut frame);
    assert_eq!(s.model.get("s_axi_bvalid"), 1);
    release_write(&mut s);
    finish_request(&mut s, 11);
    assert_eq!(command(&mut s, payload(6, 0, 0, 32, 0), 27), [0; 12]);
    memory.borrow_mut().control.pause_responses = true;
    start_request(
        &mut s,
        SEMANTIC_SERVICE_MEDIA_COMMAND,
        &payload(7, 0, 0, 0, 0),
        12,
    );
    s.until(100_000, "real dirty sample/metadata fence B", |m| {
        m.get("m_axi_bready") == 1
    });
    render(&mut s, &mut frame);
    assert_eq!(read(&mut s, REG_STATUS) & 2, 0);
    assert_eq!(
        read(&mut s, REG_RESULT_READY),
        0,
        "loader fence is not a semantic handoff"
    );
    memory.borrow_mut().control.pause_responses = false;
    finish_request(&mut s, 12);
    {
        let backing = memory.borrow();
        let rt_fpga_testbench::memory::Backend::Axi(bus) = &backing.backend else {
            unreachable!("actual AXI backing required");
        };
        assert_eq!(
            &bus.words[..32],
            &(0..32)
                .map(|index| 0xa000_0000 + index)
                .collect::<Vec<u32>>()
        );
        assert!(
            backing.active().is_none(),
            "fence includes external retirement, not only cache readback"
        );
    }
    start_request(
        &mut s,
        SEMANTIC_SERVICE_MEDIA_COMMAND,
        &payload(6, 0, 0, 32, 32),
        13,
    );
    response_ready(&mut s);
    assert_eq!(read(&mut s, REG_RESPONSE_CODE), 0);
    assert_eq!(read(&mut s, REG_RESPONSE_DATA + 8), 32);
    assert_eq!(
        s.model.get("store_cmd_ready"),
        0,
        "only private read data remains"
    );
    assert_eq!(s.model.get(OWNER), 0);
    render(&mut s, &mut frame);
    assert_eq!(read(&mut s, REG_MEDIA_READ), 0);
    assert_eq!(read(&mut s, REG_MEDIA_RECEIPT), RECEIPT_OLD_SEQUENCE);
    authorize(&mut s, 13);
    assert_eq!(read(&mut s, REG_MEDIA_STATUS), 6);
    malformed(&mut s, false, REG_MEDIA_READ + 1);
    write(&mut s, REG_RESPONSE_POP, 13);
    assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_INCOMPLETE);
    for index in 0..31 {
        assert_eq!(read(&mut s, REG_MEDIA_READ), 0xa000_0000 + index);
        render(&mut s, &mut frame);
    }
    let copied = read_hold(&mut s, REG_MEDIA_READ);
    assert_eq!(copied, 0xa000_001f);
    assert_eq!(s.model.get("store_cmd_ready"), 1);
    write(&mut s, REG_RESPONSE_POP, 13);
    request_writes(
        &mut s,
        SEMANTIC_SERVICE_MEDIA_COMMAND,
        &payload(6, 0, 1, 32, 1),
        14,
    );
    render(&mut s, &mut frame);
    s.until(100_000, "replacement private read", |m| {
        m.get("store_read_valid") == 1
    });
    assert_eq!(s.model.get("store_read_data"), 0xa000_0001);
    assert_eq!(s.model.get("s_axi_rdata"), u64::from(copied));
    release_read(&mut s, copied);
    authorize(&mut s, 14);
    assert_eq!(read(&mut s, REG_MEDIA_READ), 0xa000_0001);
    assert_eq!(finish_request(&mut s, 14)[2], 1);
    assert_eq!(command(&mut s, payload(1, 0, 4, 8, 0), 15)[0], 1);
    start_request(
        &mut s,
        SEMANTIC_SERVICE_MEDIA_COMMAND,
        &payload(6, 1, 0, 8, 8),
        16,
    );
    response_ready(&mut s);
    authorize(&mut s, 16);
    for index in 4..12 {
        assert_eq!(read(&mut s, REG_MEDIA_READ), 0xa000_0000 + index);
    }
    assert_eq!(finish_request(&mut s, 16)[2], 8);
    assert_eq!(command(&mut s, payload(4, 1, 0, 0, 0), 17)[1], 1);
    command(&mut s, payload(2, 1, 0, 0, 0), 18);
    command(&mut s, payload(3, 1, 0, 0, 0), 19);
    start_request(&mut s, SEMANTIC_SERVICE_CLEAR_VIEW, &[0; 12], 20);
    assert_eq!(
        finish_request(&mut s, 20),
        [0; 12],
        "nonempty clear uses the same real owner"
    );
    command(&mut s, payload(7, 0, 0, 0, 0), 21);
    failed(&mut s, payload(6, 0, 0, 32, 1), 22, 3);
    failed(&mut s, payload(2, 1 << 32, 0, 0, 0), 23, 3);
    failed(&mut s, payload(5, 0, 0, 0, 33), 24, 2);
    let mut reserved = payload(0, 0, 0, 0, 0);
    reserved[11] = 1;
    failed(&mut s, reserved, 25, 2);
    assert_eq!(
        s.model.get("sync_frame"),
        frame,
        "service polls/data/ACKs do not advance F"
    );
    assert_eq!(s.model.get("faulted"), 0);
    render(&mut s, &mut frame);
}

#[shoop_test(
    no_wasm = "Native bulk data copies and reset-crossing halves during real AXI retirement"
)]
fn public_media_data_authority_resets_only_after_real_retirement_and_keeps_copied_words() {
    let root = rt_fpga_testbench::root();
    let compiled = design()
        .build(&root.join("work/marlin-suite/semantic-mmio-audio"))
        .unwrap();
    for aw_first in [true, false] {
        let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
        let geometry = Geometry::new([8, 64, 32], 2);
        let memory = Memory::attach(
            &mut s,
            Mode::Fixed,
            geometry.words() + 128,
            0x0100_0000,
            &root.join("work/marlin-suite/semantic-mmio-audio/memory"),
            137,
        )
        .unwrap();
        memory.borrow_mut().control.stalls = true;
        reset(&mut s);
        ack(&mut s, true, 0, 1, 1);
        command(&mut s, payload(0, 0, 0, 0, 0), 30);
        start_request(
            &mut s,
            SEMANTIC_SERVICE_MEDIA_COMMAND,
            &payload(5, 0, 0, 0, 32),
            31,
        );
        authorize(&mut s, 31);
        for index in 0..32 {
            write(&mut s, REG_MEDIA_WRITE, 0xb000_0000 + index);
        }
        finish_request(&mut s, 31);
        start_request(
            &mut s,
            SEMANTIC_SERVICE_MEDIA_COMMAND,
            &payload(6, 0, 0, 32, 1),
            32,
        );
        response_ready(&mut s);
        authorize(&mut s, 32);
        let copied = read_hold(&mut s, REG_MEDIA_READ);
        assert_eq!(copied, 0xb000_0000);
        write(&mut s, REG_RESPONSE_POP, 32);
        memory.borrow_mut().control.pause_responses = true;
        request_writes(
            &mut s,
            SEMANTIC_SERVICE_MEDIA_COMMAND,
            &payload(7, 0, 0, 0, 0),
            33,
        );
        s.until(100_000, "owned public media fence B", |m| {
            m.get("m_axi_bready") == 1
        });
        quantum(&mut s, 0, 0, &mut vec![]);
        write(&mut s, REG_RECOVERY_REQUEST, 1);
        s.until(100, "real media AXI drain", |m| {
            m.get("recovery_state") == 1
        });
        s.model.set_all(&[
            ("s_axi_awaddr", u64::from(REG_MEDIA_WRITE)),
            ("s_axi_wdata", 0xdead_beef),
            ("s_axi_wstrb", 15),
        ]);
        let (first, first_ready, second, second_ready) = if aw_first {
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
        super::channel(&mut s, first, first_ready);
        s.idle(32);
        assert_eq!(s.model.get("runtime_epoch"), 1);
        assert_eq!(s.model.get("runtime_reset"), 0);
        assert_eq!(s.model.get("s_axi_rdata"), u64::from(copied));
        memory.borrow_mut().control.pause_responses = false;
        s.until(100_000, "actual media retirement/reset", |m| {
            m.get("runtime_epoch") == 2
                && m.get("recovery_state") == 3
                && m.get("runtime_reset") == 0
        });
        wait_initial(&mut s);
        super::channel(&mut s, second, second_ready);
        s.until(32, "poisoned media write receipt", |m| {
            m.get("s_axi_bvalid") == 1
        });
        release_write(&mut s);
        release_read(&mut s, copied);
        assert_eq!(read(&mut s, REG_MEDIA_RECEIPT), RECEIPT_EPOCH);
        assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_EPOCH);
        assert_eq!(read(&mut s, REG_MEDIA_STATUS), 0);
        assert_eq!(read(&mut s, REG_MEDIA_READ), 0);
        assert_eq!(read(&mut s, REG_MEDIA_RECEIPT), RECEIPT_UNAVAILABLE);
        assert_eq!(s.model.get("sync_frame"), 0);
        quantum(&mut s, 0, 0, &mut vec![]);
    }
}
