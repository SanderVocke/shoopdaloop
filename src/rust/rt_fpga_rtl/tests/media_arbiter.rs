#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{model::Design, sim::Sim};
use shoop_wasm_test_support::shoop_test;

fn run(test: impl FnOnce(&mut Sim<'_>)) {
    let root = rt_fpga_rtl::root();
    let model = Design::new(
        "fifo_media_arbiter",
        ["fifo_media_loader", "fifo_media_arbiter"]
            .map(|name| root.join(format!("fpga/host_bridge/rtl/{name}.sv")))
            .to_vec(),
    )
    .build(&root.join("work/media-arbiter"))
    .unwrap();
    let mut s = Sim::new(model.session(None).unwrap(), &[("clk", 10_000, 0)]);
    reset(&mut s);
    test(&mut s);
}
fn reset(s: &mut Sim<'_>) {
    s.model.set_all(&[
        ("reset", 1),
        ("processing_due", 0),
        ("frontend_busy", 0),
        ("memory_fault", 0),
        ("fence_request", 0),
        ("loader_cmd_valid", 0),
        ("loader_write_valid", 0),
        ("loader_read_ready", 0),
        ("loader_response_ready", 0),
        ("job_cmd_valid", 0),
        ("job_write_valid", 0),
        ("job_read_ready", 0),
        ("operation_ready", 0),
        ("operation_write_ready", 0),
        ("operation_read_valid", 0),
        ("operation_done", 0),
        ("operation_error", 0),
        ("operation_result_view", 99),
        ("operation_result_empty", 0),
    ]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(1);
}
fn grant(s: &mut Sim<'_>, opcode: u64) {
    s.model.eval();
    assert_eq!(s.model.get("operation_valid"), 1);
    assert_eq!(s.model.get("operation_opcode"), opcode);
    s.model.set("operation_ready", 1);
    s.idle(1);
    s.model.set("operation_ready", 0);
}
fn write(s: &mut Sim<'_>, value: u64) {
    s.model.eval();
    assert_eq!(s.model.get("loader_write_ready"), 1);
    s.model
        .set_all(&[("loader_write_valid", 1), ("loader_write_data", value)]);
    s.idle(1);
    s.model.set("loader_write_valid", 0);
}
fn job_reads(s: &mut Sim<'_>, base: u64) {
    s.model.set("job_read_ready", 1);
    for index in 0..2 {
        s.model.set_all(&[
            ("operation_read_valid", 1),
            ("operation_read_data", base + index),
        ]);
        s.model.eval();
        assert_eq!(s.model.get("job_read_valid"), 1);
        assert_eq!(s.model.get("job_read_data"), base + index);
        s.idle(1);
    }
    s.model
        .set_all(&[("operation_read_valid", 0), ("operation_done", 1)]);
    s.model.eval();
    assert_eq!(s.model.get("job_done"), 1);
    assert_eq!(s.model.get("fence_response_valid"), 0);
    s.idle(1);
    s.model
        .set_all(&[("operation_done", 0), ("job_read_ready", 0)]);
}

#[shoop_test]
fn operation_ownership_survives_new_processing_and_unread_loader_returns() {
    run(|s| {
        for opcode in [5, 6] {
            reset(s);
            s.model.set_all(&[
                ("loader_cmd_valid", 1),
                ("loader_cmd_opcode", opcode),
                ("loader_cmd_count", 4),
                ("loader_cmd_view", 5),
                ("processing_due", 1),
                ("frontend_busy", 1),
            ]);
            s.idle(1);
            s.model.set("loader_cmd_valid", 0);
            if opcode == 5 {
                write(s, 0x100);
                write(s, 0x101);
            }
            s.model.set_all(&[
                ("job_cmd_valid", 1),
                ("job_opcode", 6),
                ("job_view", 41),
                ("job_count", 2),
            ]);
            grant(s, 6);
            s.model.set("job_cmd_valid", 0);
            job_reads(s, 0x300);
            s.model.eval();
            assert_eq!(s.model.get("loader_response_valid"), 0);
            assert_eq!(
                s.model.get("operation_valid"),
                0,
                "processing admission has priority over a complete loader header"
            );
            if opcode == 5 {
                write(s, 0x102);
                write(s, 0x103);
            }
            s.model
                .set_all(&[("processing_due", 0), ("frontend_busy", 0)]);
            grant(s, opcode);
            s.model.set_all(&[
                ("processing_due", 1),
                ("frontend_busy", 1),
                ("job_cmd_valid", 1),
                ("job_write_valid", 1),
                ("job_write_data", 0xdead),
                ("job_read_ready", 0),
            ]);
            for index in 0..4 {
                s.model.eval();
                assert_eq!(s.model.get("job_cmd_ready"), 0);
                assert_eq!(s.model.get("job_write_ready"), 0);
                assert_eq!(s.model.get("job_read_valid"), 0);
                if opcode == 5 {
                    assert_eq!(s.model.get("operation_write_valid"), 1);
                    assert_eq!(s.model.get("operation_write_data"), 0x100 + index);
                    s.model.set("operation_write_ready", 1);
                } else {
                    assert_eq!(s.model.get("operation_read_ready"), 1);
                    s.model.set_all(&[
                        ("operation_read_valid", 1),
                        ("operation_read_data", 0x200 + index),
                    ]);
                }
                s.idle(1);
            }
            s.model.set_all(&[
                ("operation_write_ready", 0),
                ("operation_read_valid", 0),
                ("operation_done", 1),
            ]);
            s.model.eval();
            assert_eq!(s.model.get("job_done"), 0);
            assert_eq!(s.model.get("fence_response_valid"), 0);
            s.idle(1);
            s.model
                .set_all(&[("operation_done", 0), ("job_write_valid", 0)]);
            assert_eq!(s.model.get("loader_response_valid"), 1);
            assert_eq!(s.model.get("loader_response_error"), 0);
            assert_eq!(s.model.get("loader_result_view"), 99);
            grant(s, 6);
            s.model.set("job_cmd_valid", 0);
            job_reads(s, 0x400);
            s.idle(20);
            assert_eq!(s.model.get("loader_response_valid"), 1);
            assert_eq!(s.model.get("loader_result_view"), 99);
            assert_eq!(s.model.get("faulted"), 0);
            if opcode == 6 {
                assert_eq!(s.model.get("loader_read_valid"), 1);
                assert_eq!(s.model.get("loader_read_data"), 0x200);
                assert_eq!(s.model.get("loader_result_count"), 4);
            }
        }
    });
}

#[shoop_test]
fn fence_completion_keeps_its_accepted_identity_and_cannot_reissue_while_request_is_held() {
    run(|s| {
        s.model.set_all(&[
            ("fence_request", 1),
            ("pending_epoch", 7),
            ("pending_sequence", 17),
        ]);
        grant(s, 7);
        s.model.set_all(&[
            ("pending_epoch", 8),
            ("pending_sequence", 99),
            ("processing_due", 1),
            ("frontend_busy", 1),
            ("job_cmd_valid", 1),
            ("job_opcode", 6),
        ]);
        for _ in 0..40 {
            s.model.eval();
            assert_eq!(s.model.get("operation_valid"), 0);
            assert_eq!(s.model.get("job_cmd_ready"), 0);
            assert_eq!(s.model.get("fence_response_valid"), 0);
            assert_eq!(s.model.get("fence_response_epoch"), 7);
            assert_eq!(s.model.get("fence_response_sequence"), 17);
            s.idle(1);
        }
        s.model.set("operation_done", 1);
        s.model.eval();
        assert_eq!(s.model.get("fence_response_valid"), 1);
        assert_eq!(s.model.get("fence_response_error"), 0);
        assert_eq!(s.model.get("job_done"), 0);
        s.idle(1);
        s.model
            .set_all(&[("operation_done", 0), ("job_cmd_valid", 0)]);
        s.idle(40);
        assert_eq!(s.model.get("operation_valid"), 0);
        assert_eq!(s.model.get("fence_response_valid"), 0);
        s.model.set("fence_request", 0);
        s.idle(1);
        s.model.set_all(&[
            ("fence_request", 1),
            ("pending_epoch", 9),
            ("pending_sequence", 88),
        ]);
        grant(s, 7);
        s.model
            .set_all(&[("operation_done", 1), ("operation_error", 5)]);
        s.model.eval();
        assert_eq!(s.model.get("faulted"), 1);
        assert_eq!(s.model.get("fence_response_valid"), 1);
        assert_eq!(s.model.get("fence_response_error"), 1);
        assert_eq!(s.model.get("fence_response_epoch"), 9);
        assert_eq!(s.model.get("fence_response_sequence"), 88);
        s.idle(1);
        s.model
            .set_all(&[("operation_done", 0), ("operation_error", 0)]);
        s.idle(10);
        assert_eq!(s.model.get("faulted"), 1);
        assert_eq!(s.model.get("operation_valid"), 0);
    });
}
