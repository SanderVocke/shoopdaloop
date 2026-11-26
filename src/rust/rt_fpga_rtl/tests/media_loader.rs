#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{model::Design, sim::Sim};
use shoop_wasm_test_support::shoop_test;

fn run(test: impl FnOnce(&mut Sim<'_>)) {
    let root = rt_fpga_rtl::root();
    let model = Design::new(
        "fifo_media_loader",
        vec![root.join("fpga/host_bridge/rtl/fifo_media_loader.sv")],
    )
    .build(&root.join("work/media-loader"))
    .unwrap();
    let mut s = Sim::new(model.session(None).unwrap(), &[("clk", 10_000, 0)]);
    reset(&mut s);
    test(&mut s);
}
fn reset(s: &mut Sim<'_>) {
    s.model.set_all(&[
        ("reset", 1),
        ("cmd_valid", 0),
        ("write_valid", 0),
        ("read_ready", 0),
        ("response_ready", 0),
        ("backend_ready", 0),
        ("backend_write_ready", 0),
        ("backend_read_valid", 0),
        ("backend_done", 0),
        ("backend_error", 0),
        ("backend_result_empty", 0),
        ("backend_result_view", 9),
    ]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(1);
}
fn begin(s: &mut Sim<'_>, opcode: u64, count: u64) {
    assert_eq!(s.model.get("cmd_ready"), 1);
    s.model.set_all(&[
        ("cmd_valid", 1),
        ("cmd_opcode", opcode),
        ("cmd_view", 0x1234),
        ("cmd_start", 17),
        ("cmd_length", 257),
        ("cmd_count", count),
    ]);
    s.idle(1);
    s.model.set("cmd_valid", 0);
}
fn accept(s: &mut Sim<'_>) {
    assert_eq!(s.model.get("backend_valid"), 1);
    s.model.set("backend_ready", 1);
    s.idle(1);
    s.model.set("backend_ready", 0);
    assert_eq!(s.model.get("backend_valid"), 0);
}
fn ack(s: &mut Sim<'_>) {
    assert_eq!(s.model.get("response_valid"), 1);
    s.model.set("response_ready", 1);
    s.idle(1);
    s.model.set("response_ready", 0);
}

#[shoop_test]
fn every_partial_upload_prefix_is_private_and_committed_data_never_waits_for_arm() {
    run(|s| {
        for count in [1, 16, 32] {
            begin(s, 5, count);
            for index in 0..count {
                s.idle(3);
                assert_eq!(s.model.get("backend_valid"), 0);
                assert_eq!(s.model.get("backend_write_valid"), 0);
                assert_eq!(s.model.get("write_ready"), 1);
                s.model
                    .set_all(&[("write_valid", 1), ("write_data", 0xa000 + index)]);
                s.idle(1);
                s.model.set("write_valid", 0);
            }
            for _ in 0..20 {
                assert_eq!(s.model.get("backend_valid"), 1);
                assert_eq!(s.model.get("backend_count"), count);
                assert_eq!(s.model.get("backend_view"), 0x1234);
                assert_eq!(s.model.get("backend_start"), 17);
                assert_eq!(s.model.get("backend_length"), 257);
                assert_eq!(s.model.get("write_ready"), 0);
                s.idle(1);
            }
            accept(s);
            for index in 0..count {
                assert_eq!(s.model.get("backend_write_valid"), 1);
                assert_eq!(s.model.get("backend_write_data"), 0xa000 + index);
                s.idle(2);
                assert_eq!(s.model.get("backend_write_data"), 0xa000 + index);
                s.model.set_all(&[
                    ("backend_write_ready", 1),
                    ("backend_done", u64::from(index + 1 == count)),
                ]);
                s.idle(1);
                s.model
                    .set_all(&[("backend_write_ready", 0), ("backend_done", 0)]);
            }
            assert_eq!(s.model.get("response_valid"), 1);
            assert_eq!(s.model.get("response_error"), 0);
            assert_eq!(s.model.get("faulted"), 0);
            assert_eq!(s.model.get("result_view"), 9);
            s.idle(100);
            assert_eq!(s.model.get("backend_valid"), 0);
            assert_eq!(s.model.get("backend_write_valid"), 0);
            assert_eq!(s.model.get("cmd_ready"), 0);
            ack(s);
            assert_eq!(s.model.get("cmd_ready"), 1);
        }
    });
}

#[shoop_test]
fn complete_read_results_release_the_store_before_arm_reads_or_acknowledges() {
    run(|s| {
        begin(s, 6, 32);
        accept(s);
        for index in 0..32 {
            assert_eq!(s.model.get("backend_read_ready"), 1);
            assert_eq!(s.model.get("read_valid"), 0);
            s.model.set_all(&[
                ("backend_read_valid", 1),
                ("backend_read_data", 0xb000 + index),
                ("backend_done", u64::from(index == 31)),
            ]);
            s.idle(1);
        }
        s.model
            .set_all(&[("backend_read_valid", 0), ("backend_done", 0)]);
        for _ in 0..100 {
            assert_eq!(s.model.get("backend_valid"), 0);
            assert_eq!(s.model.get("backend_read_ready"), 0);
            assert_eq!(s.model.get("read_valid"), 1);
            assert_eq!(s.model.get("read_data"), 0xb000);
            assert_eq!(s.model.get("response_valid"), 1);
            assert_eq!(s.model.get("result_count"), 32);
            assert_eq!(s.model.get("cmd_ready"), 0);
            s.idle(1);
        }
        ack(s);
        assert_eq!(
            s.model.get("cmd_ready"),
            0,
            "unread data retains only the loader bank"
        );
        s.model.set("read_ready", 1);
        for index in 0..32 {
            s.model.eval();
            assert_eq!(s.model.get("read_valid"), 1);
            assert_eq!(s.model.get("read_data"), 0xb000 + index);
            s.idle(1);
        }
        s.model.set("read_ready", 0);
        assert_eq!(s.model.get("cmd_ready"), 1);
        assert_eq!(s.model.get("read_valid"), 0);
        assert_eq!(s.model.get("faulted"), 0);
    });
}

#[shoop_test]
fn invalid_sizes_short_success_empty_reads_and_memory_faults_do_not_replay() {
    run(|s| {
        begin(s, 5, 33);
        assert_eq!(s.model.get("response_error"), 4);
        assert_eq!(s.model.get("backend_valid"), 0);
        ack(s);
        begin(s, 6, 4);
        accept(s);
        s.model
            .set_all(&[("backend_read_valid", 1), ("backend_read_data", 123)]);
        s.idle(2);
        s.model
            .set_all(&[("backend_read_valid", 0), ("backend_done", 1)]);
        s.idle(1);
        assert_eq!(s.model.get("response_error"), 5);
        assert_eq!(s.model.get("faulted"), 1);
        assert_eq!(s.model.get("backend_valid"), 0);
        s.idle(20);
        assert_eq!(s.model.get("cmd_ready"), 0);
        reset(s);
        assert_eq!(s.model.get("read_valid"), 0);
        assert_eq!(s.model.get("result_count"), 0);
        begin(s, 6, 4);
        accept(s);
        s.model
            .set_all(&[("backend_result_empty", 1), ("backend_done", 1)]);
        s.idle(1);
        s.model.set("backend_done", 0);
        assert_eq!(s.model.get("response_error"), 0);
        assert_eq!(s.model.get("result_count"), 0);
        assert_eq!(s.model.get("read_valid"), 0);
        ack(s);
        begin(s, 5, 0);
        accept(s);
        s.model
            .set_all(&[("backend_done", 1), ("backend_error", 5)]);
        s.idle(1);
        assert_eq!(s.model.get("response_error"), 5);
        assert_eq!(s.model.get("faulted"), 1);
        ack(s);
        assert_eq!(s.model.get("cmd_ready"), 0);
        assert_eq!(s.model.get("backend_valid"), 0);
    });
}
