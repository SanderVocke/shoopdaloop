#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{model::Design, sim::Sim};
use shoop_wasm_test_support::shoop_test;

fn fields(s: &Sim<'_>) -> [u64; 9] {
    [
        "epoch",
        "cause",
        "op",
        "cycles",
        "control_cycles",
        "detail",
        "sequence",
        "revision",
        "frame",
    ]
    .map(|n| s.model.get(&format!("record_{n}")))
}

#[shoop_test(no_wasm = "Native retained first-fault RTL simulator")]
fn first_fault_is_atomic_full_width_monotonic_and_immutable_until_a_new_incarnation_faults() {
    let root = rt_fpga_rtl::root();
    let compiled = Design::new(
        "host_fault_record",
        vec![root.join("fpga/host_bridge/rtl/host_fault_record.sv")],
    )
    .build(&root.join("work/first-fault-record"))
    .unwrap();
    let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    s.model.set("reset", 1);
    s.idle(2);
    s.model
        .set_all(&[("reset", 0), ("capture", 1), ("epoch", 0)]);
    s.idle(1);
    assert_eq!(fields(&s), [0; 9]);
    let expected = [
        7,
        0x40,
        3,
        0xffff_ffff,
        4095,
        0xdead_beef,
        0x1122_3344_5566_7788,
        0xfedc_ba98_7654_3210,
        0x1234_5678_abcd_ef01,
    ];
    for (n, value) in [
        "epoch",
        "cause",
        "operation",
        "cycles",
        "control_cycles",
        "detail",
        "sequence_value",
        "revision",
        "frame",
    ]
    .into_iter()
    .zip(expected)
    {
        s.model.set(n, value);
    }
    s.idle(1);
    assert_eq!(fields(&s), expected);
    s.model
        .set_all(&[("cause", 0x90), ("sequence_value", u64::MAX), ("frame", 0)]);
    s.idle(100);
    assert_eq!(
        fields(&s),
        expected,
        "same-epoch abort cannot erase the original cause/context"
    );
    for epoch in [0, 6, 1] {
        s.model.set("epoch", epoch);
        s.idle(2);
        assert_eq!(
            fields(&s),
            expected,
            "old incarnation cannot replace a newer record"
        );
    }
    s.model.set_all(&[("capture", 0), ("epoch", 8)]);
    s.idle(2);
    assert_eq!(fields(&s), expected, "epoch advance alone is not a fault");
    s.model
        .set_all(&[("capture", 1), ("sequence_value", 0x8000_0000_0000_0001)]);
    s.idle(1);
    let next = fields(&s);
    assert_eq!(next[0], 8);
    assert_eq!(next[1], 0x90);
    assert_eq!(next[6], 0x8000_0000_0000_0001);
    s.model.set_all(&[("epoch", 7), ("cause", 1)]);
    s.idle(2);
    assert_eq!(fields(&s), next);
    s.model.set_all(&[("reset", 1), ("capture", 0)]);
    s.idle(1);
    assert_eq!(fields(&s), [0; 9]);
    s.model
        .set_all(&[("reset", 0), ("capture", 1), ("epoch", 1)]);
    s.idle(1);
    assert_eq!(
        fields(&s)[0],
        1,
        "a whole-device reset starts a new record lifetime"
    );
}
