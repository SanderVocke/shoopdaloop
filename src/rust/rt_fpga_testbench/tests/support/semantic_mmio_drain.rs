use super::media::{authorize, command, payload};
use super::paced::{enable_hold, initialize, install, paced_design};
use super::{
    Geometry, Memory, Mode, Probe, Sim, finish_request, first_fault, read, read_hold, release_read,
    release_write, start_request, write, write_hold,
};
use rt_fpga_rtl::host::semantic_registers::*;
use rt_fpga_rtl::{clock::Edges, model::Model, sim::Agent};
use shoop_wasm_test_support::shoop_test;
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Ordering {
    grants: u32,
}
impl Agent for Ordering {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0)
            && m.get("control.config_operation") == 7
            && m.get("control.store_cmd_valid") != 0
        {
            assert_eq!(
                m.get("audio_enable"),
                0,
                "registered stop before any loader admission"
            );
            assert_eq!(
                m.get("busy"),
                0,
                "accepted processing completes before the drain fence"
            );
            if m.get("store_cmd_ready") != 0 {
                self.grants += 1;
            }
        }
    }
}
fn dirty(s: &mut Sim<'_>, tag: u32) {
    command(s, payload(0, 0, 0, 0, 0), tag);
    start_request(
        s,
        SEMANTIC_SERVICE_MEDIA_COMMAND,
        &payload(5, 0, 0, 0, 32),
        tag + 1,
    );
    authorize(s, tag + 1);
    for word in 0..32 {
        write(s, REG_MEDIA_WRITE, 0xd100_0000 + word);
    }
    finish_request(s, tag + 1);
}
fn stage(s: &mut Sim<'_>) {
    write(s, REG_REQUEST_EPOCH, s.model.get("runtime_epoch") as u32);
    write(s, REG_REQUEST_OP, SEMANTIC_SERVICE_DRAIN_STOP);
    for word in 0..12 {
        write(s, REG_REQUEST_DATA + 4 * word, 0);
    }
}
fn observed_design() -> rt_fpga_testbench::model::Design {
    let mut d = paced_design(false);
    d.probes
        .extend(["control.config_operation", "control.store_cmd_valid"].map(Probe::observe));
    d
}

#[shoop_test(
    no_wasm = "Native drain ordering, actual audio/SP/JP, dirty cache/DMA/AXI fence and copied replies"
)]
fn drain_prefixes_keep_audio_running_and_success_waits_for_processing_and_real_writeback() {
    let root = rt_fpga_testbench::root();
    let compiled = observed_design()
        .build(&root.join("work/marlin-suite/semantic-drain"))
        .unwrap();
    let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    let geometry = Geometry::new([8, 64, 32], 2);
    let memory = Memory::attach(
        &mut s,
        Mode::Fixed,
        geometry.words() + 128,
        0x0100_0000,
        &root.join("work/marlin-suite/semantic-drain/memory"),
        163,
    )
    .unwrap();
    memory.borrow_mut().control.stalls = true;
    let ordering = Rc::new(RefCell::new(Ordering::default()));
    s.agent(ordering.clone());
    initialize(&mut s);
    install(&mut s);
    enable_hold(&mut s, 101);
    release_write(&mut s);
    finish_request(&mut s, 101);
    s.until(
        100_000,
        "first real audio and independent handoff completion",
        |m| m.get("result_ready_mask") == 1 && m.get("sync_frame") >= 32,
    );
    dirty(&mut s, 102);
    let mut packet = vec![1, SEMANTIC_SERVICE_DRAIN_STOP];
    packet.extend([0; 12]);
    for (index, word) in packet.into_iter().enumerate() {
        write(&mut s, REG_REQUEST_PUSH, 104);
        assert_eq!(read(&mut s, REG_SUBMISSION_RESULT), RECEIPT_INCOMPLETE);
        let frame = s.model.get("sync_frame");
        s.idle(70_000);
        assert!(
            s.model.get("sync_frame") > frame,
            "unpublished drain prefix {index}"
        );
        assert_eq!(s.model.get("audio_enable"), 1);
        assert_eq!(s.model.get("faulted"), 0);
        assert_eq!(ordering.borrow().grants, 0);
        write(&mut s, REG_REQUEST_EPOCH + 4 * index as u32, word);
    }
    s.until(80_000, "accepted JP after SP program reads finish", |m| {
        m.get("state_busy") == 0 && m.get("jobs_busy") != 0
    });
    memory.borrow_mut().control.pause_responses = true;
    write_hold(&mut s, REG_REQUEST_PUSH, 104);
    s.until(
        10_000,
        "actual dirty-fence B, after accepted processing",
        |m| m.get("m_axi_bready") == 1,
    );
    assert_eq!(s.model.get("audio_enable"), 0);
    assert_eq!(s.model.get("busy"), 0);
    assert_eq!(ordering.borrow().grants, 1);
    let frame = s.model.get("sync_frame");
    s.idle(1000);
    assert_eq!(s.model.get("sync_frame"), frame);
    assert_eq!(
        read(&mut s, REG_STATUS) & 2,
        0,
        "no early successful drain reply"
    );
    assert_eq!(
        s.model.get("result_ready_mask"),
        1,
        "no result ACK or consumer-fence manufacture"
    );
    memory.borrow_mut().control.pause_responses = false;
    super::configuration::response_ready(&mut s);
    assert_eq!(read(&mut s, REG_RESPONSE_CODE), 0);
    {
        let backing = memory.borrow();
        let rt_fpga_testbench::memory::Backend::Axi(bus) = &backing.backend else {
            unreachable!()
        };
        assert_eq!(
            &bus.words[..32],
            &(0..32).map(|i| 0xd100_0000 + i).collect::<Vec<u32>>()
        );
        assert!(
            backing.active().is_none(),
            "external retirement, not cache-only readback"
        );
    }
    assert_eq!(read_hold(&mut s, REG_RESPONSE_TAG), 104);
    assert_eq!(s.model.get("s_axi_bvalid"), 1);
    release_write(&mut s);
    write(&mut s, REG_RESPONSE_POP, 104);
    let mut enabled = [0; 12];
    enabled[0] = 1;
    super::configuration::request_writes(&mut s, SEMANTIC_SERVICE_ENABLE_AUDIO, &enabled, 105);
    s.idle(100_000);
    assert_eq!(s.model.get("s_axi_rdata"), 104);
    assert_eq!(s.model.get("s_axi_rvalid"), 1);
    assert_eq!(s.model.get("sync_frame"), frame);
    assert_eq!(
        ordering.borrow().grants,
        1,
        "no fence replay after source reuse"
    );
    release_read(&mut s, 104);
    assert_eq!(
        read(&mut s, REG_RESPONSE_CODE),
        2,
        "stopped is not a fresh lifetime"
    );
    write(&mut s, REG_RESPONSE_POP, 105);
    assert_eq!(s.model.get("runtime_epoch"), 1);
    assert_eq!(s.model.get("faulted"), 0);
}

#[shoop_test(
    no_wasm = "Native drain uncertainty and explicit recovery behind a real accepted DDR fence"
)]
fn aborted_drain_does_not_cancel_accepted_writeback_or_replay_after_safe_runtime_reset() {
    let root = rt_fpga_testbench::root();
    let compiled = observed_design()
        .build(&root.join("work/marlin-suite/semantic-drain"))
        .unwrap();
    let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    let geometry = Geometry::new([8, 64, 32], 2);
    let memory = Memory::attach(
        &mut s,
        Mode::Fixed,
        geometry.words() + 128,
        0x0100_0000,
        &root.join("work/marlin-suite/semantic-drain/memory"),
        167,
    )
    .unwrap();
    memory.borrow_mut().control.stalls = true;
    initialize(&mut s);
    install(&mut s);
    enable_hold(&mut s, 201);
    release_write(&mut s);
    finish_request(&mut s, 201);
    s.until(100_000, "initial real audio handoff", |m| {
        m.get("result_ready_mask") == 1
    });
    dirty(&mut s, 202);
    stage(&mut s);
    s.until(80_000, "accepted JP before drain", |m| {
        m.get("state_busy") == 0 && m.get("jobs_busy") != 0
    });
    memory.borrow_mut().control.pause_responses = true;
    write_hold(&mut s, REG_REQUEST_PUSH, 204);
    s.until(10_000, "real accepted writeback B", |m| {
        m.get("m_axi_bready") == 1
    });
    release_write(&mut s);
    write(&mut s, REG_ABORT, 1);
    super::configuration::response_ready(&mut s);
    assert_eq!(
        read(&mut s, REG_RESPONSE_CODE),
        3,
        "uncertain effects, not rollback/cancel"
    );
    for i in 0..12 {
        assert_eq!(read(&mut s, REG_RESPONSE_DATA + 4 * i), 0);
    }
    assert_eq!(s.model.get("audio_enable"), 0);
    assert_eq!(s.model.get("m_axi_bready"), 1);
    assert_eq!(read_hold(&mut s, REG_FAULT_CAUSE), 16);
    write_hold(&mut s, REG_RECOVERY_REQUEST, 1);
    s.idle(128);
    assert_eq!(s.model.get("runtime_epoch"), 1);
    assert_eq!(
        s.model.get("runtime_reset"),
        0,
        "halt is not external retirement"
    );
    memory.borrow_mut().control.pause_responses = false;
    s.until(10_000, "accepted B retirement and serialized reset", |m| {
        m.get("runtime_epoch") == 2 && m.get("runtime_reset") == 0 && m.get("recovery_state") == 3
    });
    s.until(10_000, "new initial snapshot without ARM readback", |m| {
        m.get("busy") == 0
    });
    assert_eq!(s.model.get("audio_enable"), 0);
    assert_eq!(s.model.get("sync_frame"), 0);
    assert_eq!(s.model.get("s_axi_rvalid"), 1);
    assert_eq!(s.model.get("s_axi_rdata"), 16);
    assert_eq!(s.model.get("s_axi_bvalid"), 1);
    release_read(&mut s, 16);
    release_write(&mut s);
    s.idle(20_000);
    assert_eq!(
        s.model.get("sync_frame"),
        0,
        "no automatic replay/re-enable"
    );
    start_request(&mut s, SEMANTIC_SERVICE_DRAIN_STOP, &[0; 12], 205);
    assert_eq!(finish_request(&mut s, 205), [0; 12]);
    assert_eq!(
        s.model.get("sync_frame"),
        0,
        "idle drain does not create a quantum"
    );
    assert_eq!(
        first_fault(&mut s)[1],
        16,
        "fresh startup is not a new fault"
    );
}
