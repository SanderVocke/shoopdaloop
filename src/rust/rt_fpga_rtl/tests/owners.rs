#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{model::Design, sim::Sim};
use shoop_wasm_test_support::shoop_test;
#[shoop_test(no_wasm = "native simulator model ownership")]
fn two_sessions_share_cached_artifacts_without_holding_a_lifetime_lock() {
    let root = rt_fpga_rtl::root();
    let design = Design::new(
        "state_processor",
        "sp_ram sp_job_fifo sp_engine state_processor"
            .split_whitespace()
            .map(|n| root.join(format!("fpga/state_processor/rtl/{n}.sv")))
            .collect(),
    )
    .parameters(&[
        ("PROGRAM_DEPTH", 1),
        ("STATE_DEPTH", 1),
        ("JOB_DEPTH", 1),
        ("LOOP_DEPTH", 1),
    ]);
    let compiled = design
        .build(&root.join("work/shared-drivers/cache-ownership"))
        .unwrap();
    let mut first = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    let mut second = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    first.model.set("reset", 1);
    first.idle(2);
    first.model.set("reset", 0);
    second.model.set("reset", 1);
    second.idle(2);
    second.model.set("reset", 0);
    first.idle(3);
    second.idle(5);
    assert_eq!(first.cycles, 5);
    assert_eq!(second.cycles, 7);
}

#[shoop_test(no_wasm = "separate native simulator owners")]
fn cached_model_can_be_loaded_while_another_owner_is_alive() {
    fn design() -> Design {
        let root = rt_fpga_rtl::root();
        Design::new(
            "state_processor",
            "sp_ram sp_job_fifo sp_engine state_processor"
                .split_whitespace()
                .map(|n| root.join(format!("fpga/state_processor/rtl/{n}.sv")))
                .collect(),
        )
        .parameters(&[
            ("PROGRAM_DEPTH", 1),
            ("STATE_DEPTH", 1),
            ("JOB_DEPTH", 1),
            ("LOOP_DEPTH", 1),
        ])
    }
    let cache = rt_fpga_rtl::root().join("work/shared-drivers/cache-ownership");
    let compiled = design().build(&cache).unwrap();
    let _first = compiled.session(None).unwrap();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let compiled = design().build(&cache).unwrap();
        let _other = compiled.session(None).unwrap();
        tx.send(()).unwrap();
    });
    rx.recv_timeout(std::time::Duration::from_secs(30))
        .expect("another live owner must not hold the cache lock");
    worker.join().unwrap();
}
