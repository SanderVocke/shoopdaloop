#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{model::Compiled, model::Design, sim::Sim};
use shoop_wasm_test_support::shoop_test;

fn build(width: u64) -> Compiled {
    let root = rt_fpga_rtl::root();
    Design::new(
        "axi_master",
        vec![root.join("fpga/axi_memory/rtl/axi_master.sv")],
    )
    .parameters(&[("DATA_WIDTH", width)])
    .build(&root.join("work/axi-drain"))
    .unwrap()
}
fn start(compiled: &Compiled, width: u64) -> Sim<'_> {
    let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    s.model.set_all(&[
        ("reset", 1),
        ("command_address", 0x1000),
        ("command_size", if width == 64 { 3 } else { 2 }),
        ("write_strobe", if width == 64 { 255 } else { 15 }),
    ]);
    s.idle(2);
    s.model.set("reset", 0);
    s.model.eval();
    assert_eq!(s.model.get("command_ready"), 1);
    s
}
fn command(s: &mut Sim<'_>, write: bool, length: u64) {
    s.model.set_all(&[
        ("command_valid", 1),
        ("command_write", u64::from(write)),
        ("command_length", length),
    ]);
    s.idle(1);
    s.model.set("command_valid", 0);
    s.idle(1);
}
fn stop(s: &mut Sim<'_>) {
    s.model.set("quiesce", 1);
    s.model.eval();
    assert_eq!(s.model.get("command_ready"), 0);
    assert_eq!(s.model.get("read_valid"), 0);
    assert_eq!(s.model.get("completion_valid"), 0);
}
fn drained(s: &mut Sim<'_>) {
    s.model.eval();
    assert_eq!(s.model.get("drain_fault"), 0);
    assert_eq!(s.model.get("quiescent"), 1);
    s.model.set_all(&[("quiesce", 0), ("command_valid", 1)]);
    s.idle(10);
    assert_eq!(s.model.get("quiescent"), 1);
    assert_eq!(s.model.get("command_ready"), 0);
    assert_eq!(s.model.get("m_axi_awvalid"), 0);
    assert_eq!(s.model.get("m_axi_arvalid"), 0);
    assert_eq!(s.model.get("m_axi_wvalid"), 0);
    assert_eq!(s.model.get("read_valid"), 0);
    assert_eq!(s.model.get("completion_valid"), 0);
}

#[shoop_test(no_wasm = "Native AXI drain RTL simulator")]
fn every_write_prefix_and_aw_w_order_drains_without_padding_or_fabricating_completion() {
    let mut cases = 0;
    for width in [32, 64] {
        let compiled = build(width);
        for length in 0..16 {
            for prefix in 0..=length + 1 {
                for address_first in [false, true] {
                    let mut s = start(&compiled, width);
                    command(&mut s, true, length);
                    assert_eq!(s.model.get("m_axi_awvalid"), 1);
                    if address_first {
                        s.model.set("m_axi_awready", 1);
                        s.idle(1);
                        s.model.set("m_axi_awready", 0);
                    }
                    for beat in 0..=length {
                        if beat == prefix {
                            stop(&mut s);
                            s.idle(3);
                            assert_eq!(s.model.get("quiescent"), 0);
                            assert_eq!(s.model.get("m_axi_wvalid"), 0);
                        }
                        let data = (if width == 64 {
                            0x1234_5678_0000_0000
                        } else {
                            0
                        }) | beat;
                        s.model.set_all(&[
                            ("write_valid", 1),
                            ("write_data", data),
                            ("m_axi_wready", 1),
                        ]);
                        s.model.eval();
                        assert_eq!(s.model.get("m_axi_wvalid"), 1);
                        assert_eq!(s.model.get("m_axi_wlast"), u64::from(beat == length));
                        assert_eq!(s.model.get("m_axi_wdata"), data);
                        assert_eq!(
                            s.model.get("m_axi_wstrb"),
                            if width == 64 { 255 } else { 15 }
                        );
                        s.idle(1);
                        s.model.set_all(&[("write_valid", 0), ("m_axi_wready", 0)]);
                    }
                    if prefix == length + 1 {
                        stop(&mut s);
                    }
                    s.idle(3);
                    assert_eq!(s.model.get("quiescent"), 0);
                    if !address_first {
                        assert_eq!(s.model.get("m_axi_awvalid"), 1);
                        assert_eq!(s.model.get("m_axi_awaddr"), 0x1000);
                        assert_eq!(s.model.get("m_axi_awlen"), length);
                        s.model.set("m_axi_awready", 1);
                        s.idle(1);
                        s.model.set("m_axi_awready", 0);
                    }
                    s.idle(3);
                    assert_eq!(s.model.get("quiescent"), 0, "B must actually return");
                    assert_eq!(s.model.get("m_axi_bready"), 1, "consumer ready is zero");
                    s.model.set_all(&[
                        ("m_axi_bvalid", 1),
                        ("m_axi_bresp", if prefix % 2 == 0 { 2 } else { 0 }),
                    ]);
                    s.idle(1);
                    s.model.set("m_axi_bvalid", 0);
                    drained(&mut s);
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 608);
}

#[shoop_test(no_wasm = "Native AXI drain RTL simulator")]
fn every_read_prefix_drains_real_r_last_even_when_the_consumer_never_reads() {
    for width in [32, 64] {
        let compiled = build(width);
        for length in 0..16 {
            for prefix in 0..=length {
                let mut s = start(&compiled, width);
                command(&mut s, false, length);
                if prefix == 0 {
                    stop(&mut s);
                    s.idle(3);
                    assert_eq!(s.model.get("m_axi_arvalid"), 1);
                    assert_eq!(s.model.get("m_axi_araddr"), 0x1000);
                }
                s.model.set_all(&[("m_axi_arready", 1), ("read_ready", 1)]);
                s.idle(1);
                s.model.set("m_axi_arready", 0);
                for beat in 0..=length {
                    if beat == prefix {
                        s.model.set("read_ready", 0);
                        stop(&mut s);
                    }
                    s.idle(2);
                    assert_eq!(s.model.get("quiescent"), 0);
                    s.model.set_all(&[
                        ("m_axi_rvalid", 1),
                        ("m_axi_rdata", 0x1234 + beat),
                        ("m_axi_rlast", u64::from(beat == length)),
                        ("m_axi_rresp", if beat % 2 == 0 { 2 } else { 0 }),
                    ]);
                    s.model.eval();
                    assert_eq!(s.model.get("m_axi_rready"), 1);
                    if beat >= prefix {
                        assert_eq!(s.model.get("read_valid"), 0);
                    }
                    s.idle(1);
                    s.model.set("m_axi_rvalid", 0);
                }
                drained(&mut s);
            }
        }
    }
}

#[shoop_test(no_wasm = "Native AXI drain RTL simulator")]
fn command_cutoff_cancels_unpresented_headers_and_a_missing_write_body_never_drains() {
    for width in [32, 64] {
        let compiled = build(width);
        for length in [0, 15, 16, 255] {
            let mut s = start(&compiled, width);
            s.model.set_all(&[
                ("command_valid", 1),
                ("command_write", 1),
                ("command_length", length),
            ]);
            s.idle(1);
            stop(&mut s);
            s.idle(1);
            drained(&mut s);
        }
        let mut s = start(&compiled, width);
        command(&mut s, true, 15);
        s.model.set("m_axi_awready", 1);
        s.idle(1);
        s.model.set("m_axi_awready", 0);
        stop(&mut s);
        s.idle(200);
        assert_eq!(s.model.get("quiescent"), 0);
        assert_eq!(s.model.get("m_axi_wvalid"), 0, "no synthetic payload");
        assert_eq!(s.model.get("m_axi_bready"), 0);
        assert_eq!(s.model.get("completion_valid"), 0);
        assert_eq!(
            s.model.get("drain_fault"),
            0,
            "stalls are not protocol errors"
        );
    }
}

#[shoop_test(no_wasm = "Native AXI drain RTL simulator")]
fn unsolicited_early_wrong_id_and_wrong_last_responses_permanently_refuse_safe_reuse() {
    for width in [32, 64] {
        let compiled = build(width);
        for kind in 0..8 {
            let mut s = start(&compiled, width);
            match kind {
                0 => s.model.set("m_axi_bvalid", 1),
                1 => s.model.set("m_axi_rvalid", 1),
                2 | 3 | 4 => {
                    command(&mut s, false, if kind == 3 { 1 } else { 0 });
                    s.model.set("m_axi_arready", 1);
                    s.idle(1);
                    s.model.set_all(&[
                        ("m_axi_arready", 0),
                        ("m_axi_rvalid", 1),
                        ("m_axi_rid", u64::from(kind == 2)),
                        ("m_axi_rlast", u64::from(kind != 4)),
                    ]);
                }
                _ => {
                    command(&mut s, true, 0);
                    if kind != 5 {
                        s.model.set_all(&[
                            ("m_axi_awready", 1),
                            ("write_valid", 1),
                            ("m_axi_wready", 1),
                        ]);
                        s.idle(1);
                        s.model.set_all(&[
                            ("m_axi_awready", 0),
                            ("write_valid", 0),
                            ("m_axi_wready", 0),
                        ]);
                    }
                    s.model
                        .set_all(&[("m_axi_bvalid", 1), ("m_axi_bid", u64::from(kind == 6))]);
                    if kind == 7 {
                        stop(&mut s);
                        s.idle(1);
                        s.model.set("m_axi_bvalid", 0);
                        s.idle(1);
                        s.model.set("m_axi_bvalid", 1);
                    }
                }
            }
            stop(&mut s);
            s.idle(1);
            s.model
                .set_all(&[("m_axi_bvalid", 0), ("m_axi_rvalid", 0), ("quiesce", 0)]);
            s.idle(20);
            assert_eq!(s.model.get("drain_fault"), 1, "width {width}, kind {kind}");
            assert_eq!(s.model.get("quiescent"), 0);
            assert_eq!(s.model.get("command_ready"), 0);
        }
    }
}
